#[macro_use]
extern crate lazy_static;

use actix_web::{middleware, web, App, HttpResponse, HttpServer};
use futures::prelude::*;
use serde::Deserialize;
use std::env;
use telegram_bot::prelude::*;
use telegram_bot::{Api, UpdateKind};

static HS_OPEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

lazy_static! {
    static ref BASE_URL: String = format!(
        "http://{host}:{port}/{token}",
        host = env::var("SHINOBI_HOST").expect("SHINOBI_HOST is required"),
        port = env::var("SHINOBI_PORT").unwrap_or_else(|_| "8080".to_string()),
        token = env::var("SHINOBI_TOKEN").expect("SHINOBI_TOKEN is required")
    );
    static ref API: telegram_bot::Api =
        Api::new(env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN not set"));
    static ref GROUP_KEY: String =
        env::var("SHINOBI_GROUP_KEY").expect("SHINOBI_GROUP_KEY is required");
    static ref WEB_SERVER_BIND: String =
        env::var("WEB_SERVER_BIND").expect("WEB_SERVER_BIND is required, format 127.0.0.1:8080");
    static ref CHAT: telegram_bot::types::chat::MessageChat =
        telegram_bot::types::chat::MessageChat::Group(telegram_bot::types::chat::Group {
            id: telegram_bot::types::refs::GroupId::new(
                env::var("ID_GROUP")
                    .expect("ID_GROUP is required")
                    .parse::<telegram_bot::types::primitive::Integer>()
                    .expect("ID_GROUP not an integer")
            ),
            title: "Group".to_string(),
            all_members_are_administrators: false,
            invite_link: None,
        });
}

#[derive(Debug)]
struct Error {
    message: String,
}

#[derive(Deserialize, Debug)]
struct Monitor {
    mid: String,
}

impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Error {
            message: "IO deu ruim".to_string(),
        }
    }
}

impl From<telegram_bot::Error> for Error {
    fn from(_: telegram_bot::Error) -> Self {
        Error {
            message: "Bot deu ruim".to_string(),
        }
    }
}

impl From<reqwest::Error> for Error {
    fn from(_: reqwest::Error) -> Self {
        Error {
            message: "Shinobi deu ruim".to_string(),
        }
    }
}

async fn index() -> HttpResponse {
    if !HS_OPEN.load(std::sync::atomic::Ordering::Relaxed) {
        if send_photos_to_chat(&CHAT).await.is_ok() {
            HttpResponse::Ok().finish()
        } else {
            HttpResponse::InternalServerError().finish()
        }
    } else {
        HttpResponse::Ok().finish()
    }
}

async fn send_photos_to_chat(chat: &telegram_bot::types::MessageChat) -> Result<(), Error> {
    let monitors: Vec<Monitor> = reqwest::get(&format!(
        "{base_url}/smonitor/{group_key}",
        base_url = *BASE_URL,
        group_key = *GROUP_KEY
    ))
    .await?
    .json()
    .await?;

    for monitor in &monitors {
        let bytes = reqwest::get(&format!(
            "{base_url}/jpeg/{group_key}/{monitor_id}/s.jpg",
            base_url = *BASE_URL,
            group_key = *GROUP_KEY,
            monitor_id = monitor.mid
        ))
        .await?
        .bytes()
        .await?;

        API.send(chat.photo(telegram_bot::types::InputFileUpload::with_data(
            bytes, "teste",
        )))
        .await?;
    }

    Ok(())
}

async fn bot() -> Result<(), Error> {
    API.send(CHAT.text("To Vivo!!")).await?;

    API.stream()
        .for_each_concurrent(100, |update| async {
            if let UpdateKind::Message(telegram_bot::types::message::Message {
                from: _user,
                chat,
                kind: telegram_bot::types::message::MessageKind::Text { data, entities },
                ..
            }) = update.unwrap().kind
            {
                for entity in &entities {
                    if entity.kind == telegram_bot::types::message::MessageEntityKind::BotCommand {
                        let command = &data.as_str()[entity.offset as usize
                            ..entity.offset as usize + entity.length as usize];
                        match command {
                            "/photo" => send_photos_to_chat(&chat).await.expect("Falha ao enviar"),
                            "/open" => HS_OPEN.store(true, std::sync::atomic::Ordering::Relaxed),
                            "/close" => HS_OPEN.store(false, std::sync::atomic::Ordering::Relaxed),
                            _ => {}
                        }
                    }
                }
            };
        })
        .await;

    Ok(())
}

#[actix_rt::main]
async fn main() -> Result<(), Error> {
    futures::try_join!(
        HttpServer::new(|| {
            App::new()
                .wrap(middleware::Logger::default())
                .service(web::resource("/").to(index))
        })
        .bind(WEB_SERVER_BIND.to_string())?
        .run()
        .err_into(),
        bot()
    )?;

    Ok(())
}
