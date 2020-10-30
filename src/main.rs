use actix_web::middleware::Logger;
use actix_web::web::resource;
use actix_web::{App, HttpResponse, HttpServer};
use anyhow::Result;
use futures::prelude::*;
use once_cell::sync::Lazy;
use std::env;
use std::sync::atomic::{AtomicBool, Ordering};
use telegram_bot::prelude::*;
use telegram_bot::types::*;

static BASE_URL: Lazy<String> = Lazy::new(|| {
    format!(
        "http://{host}:{port}/{token}",
        host = env::var("SHINOBI_HOST").expect("SHINOBI_HOST is required"),
        port = env::var("SHINOBI_PORT").unwrap_or_else(|_| "8080".to_string()),
        token = env::var("SHINOBI_TOKEN").expect("SHINOBI_TOKEN is required")
    )
});

static GROUP_KEY: Lazy<String> =
    Lazy::new(|| env::var("SHINOBI_GROUP_KEY").expect("SHINOBI_GROUP_KEY is required"));

static HS_OPEN: AtomicBool = AtomicBool::new(false);

static TELEGRAM_API: Lazy<telegram_bot::Api> = Lazy::new(|| {
    telegram_bot::Api::new(env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN not set"))
});

static TELEGRAM_CHAT: Lazy<MessageChat> = Lazy::new(|| {
    MessageChat::Group(Group {
        id: GroupId::new(
            env::var("ID_GROUP")
                .expect("ID_GROUP is required")
                .parse::<Integer>()
                .expect("ID_GROUP not an integer"),
        ),
        title: "Group".to_string(),
        all_members_are_administrators: false,
        invite_link: None,
    })
});

static WEB_SERVER_BIND: Lazy<String> = Lazy::new(|| {
    env::var("WEB_SERVER_BIND").expect("WEB_SERVER_BIND is required, format 127.0.0.1:8080")
});

#[derive(serde::Deserialize, Debug)]
struct Monitor {
    mid: String,
}

async fn index() -> HttpResponse {
    if !HS_OPEN.load(Ordering::Relaxed) {
        match send_photos_to_chat(&TELEGRAM_CHAT).await {
            Ok(_) => HttpResponse::Ok().finish(),
            Err(_) => HttpResponse::InternalServerError().finish(),
        }
    } else {
        HttpResponse::Ok().finish()
    }
}

async fn send_photos_to_chat(chat: &MessageChat) -> Result<()> {
    let monitors = reqwest::get(&format!(
        "{base_url}/smonitor/{group_key}",
        base_url = *BASE_URL,
        group_key = *GROUP_KEY,
    ))
    .await?
    .json::<Vec<Monitor>>()
    .await?;

    for monitor in monitors {
        TELEGRAM_API
            .send(
                chat.photo(InputFileUpload::with_data(
                    reqwest::get(&format!(
                        "{base_url}/jpeg/{group_key}/{monitor_id}/s.jpg",
                        base_url = *BASE_URL,
                        group_key = *GROUP_KEY,
                        monitor_id = monitor.mid,
                    ))
                    .await?
                    .bytes()
                    .await?,
                    monitor.mid,
                )),
            )
            .await?;
    }

    Ok(())
}

async fn bot() -> Result<()> {
    TELEGRAM_API
        .send(TELEGRAM_CHAT.text(format!(
            "{bot_name} v{bot_version} online!",
            bot_name = env!("CARGO_PKG_NAME"),
            bot_version = env!("CARGO_PKG_VERSION"),
        )))
        .await?;

    TELEGRAM_API
        .stream()
        .for_each_concurrent(100, |update| async {
            if let Ok(Update {
                kind:
                    UpdateKind::Message(Message {
                        chat,
                        kind: MessageKind::Text { data, entities },
                        ..
                    }),
                ..
            }) = update
            {
                for entity in entities {
                    if entity.kind == MessageEntityKind::BotCommand {
                        let command = &data.as_str()[entity.offset as usize
                            ..entity.offset as usize + entity.length as usize];
                        match command {
                            "/photo" => {
                                send_photos_to_chat(&chat)
                                    .await
                                    .expect("Failed to send photos to chat");
                            }
                            "/open" => HS_OPEN.store(true, Ordering::Relaxed),
                            "/close" => HS_OPEN.store(false, Ordering::Relaxed),
                            "/status" => {
                                TELEGRAM_API
                                    .send(chat.text(format!("{}", HS_OPEN.load(Ordering::Relaxed))))
                                    .await
                                    .expect("Failed to send status to chat");
                            }
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
async fn main() -> Result<()> {
    futures::try_join!(
        HttpServer::new(|| {
            App::new()
                .wrap(Logger::default())
                .service(resource("/").to(index))
        })
        .bind(WEB_SERVER_BIND.to_string())?
        .run()
        .err_into(),
        bot()
    )?;

    Ok(())
}
