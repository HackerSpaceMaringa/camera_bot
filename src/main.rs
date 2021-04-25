#![feature(once_cell)]

use actix_web::middleware::Logger;
use actix_web::web::resource;
use actix_web::{App, HttpResponse, HttpServer};
use anyhow::Result;
use futures::prelude::*;
use futures::try_join;
use log::{debug, error, info, warn};
use std::env;
use std::lazy::SyncLazy;
use std::sync::atomic::{AtomicBool, Ordering};
use telegram_bot::prelude::*;
use telegram_bot::types::*;

static BASE_URL: SyncLazy<String> = SyncLazy::new(|| {
    format!(
        "http://{host}:{port}/{token}",
        host = env::var("SHINOBI_HOST").expect("SHINOBI_HOST is required"),
        port = env::var("SHINOBI_PORT").unwrap_or_else(|_| "8080".to_string()),
        token = env::var("SHINOBI_TOKEN").expect("SHINOBI_TOKEN is required")
    )
});

static GROUP_KEY: SyncLazy<String> =
    SyncLazy::new(|| env::var("SHINOBI_GROUP_KEY").expect("SHINOBI_GROUP_KEY is required"));

static HS_OPEN: AtomicBool = AtomicBool::new(false);

static TELEGRAM_API: SyncLazy<telegram_bot::Api> = SyncLazy::new(|| {
    telegram_bot::Api::new(env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN not set"))
});

static TELEGRAM_CHAT: SyncLazy<MessageChat> = SyncLazy::new(|| {
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

static WEB_SERVER_BIND: SyncLazy<String> = SyncLazy::new(|| {
    env::var("WEB_SERVER_BIND").expect("WEB_SERVER_BIND is required, format 127.0.0.1:8080")
});

#[derive(serde::Deserialize, Debug)]
struct Monitor {
    mid: String,
}

async fn index() -> HttpResponse {
    info!("Receving request from Shinobi (Trigger)");

    if !HS_OPEN.load(Ordering::Relaxed) {
        match send_photos_to_chat(&TELEGRAM_CHAT).await {
            Ok(_) => {
                info!("Sent triggered photos to chat");
                HttpResponse::Ok().finish()
            }
            Err(_) => {
                error!("Couldn't send event photos to chat");
                HttpResponse::InternalServerError().finish()
            }
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

    info!(
        "Sending photos of #{} monitors to chat {}",
        monitors.len(),
        chat.id()
    );

    for monitor in monitors {
        debug!("Sending photo of {} to {}", monitor.mid, chat.id());

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
    info!("Bot starting...");

    TELEGRAM_API
        .send(TELEGRAM_CHAT.text(format!(
            "{bot_name} v{bot_version} online!",
            bot_name = env!("CARGO_PKG_NAME"),
            bot_version = env!("CARGO_PKG_VERSION"),
        )))
        .await?;

    debug!("Sent online message");

    TELEGRAM_API
        .stream()
        .for_each_concurrent(100, |update| async {
            info!("Receive new event");

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
                debug!("Message received \"{}\" from {}", data, chat.id());

                for entity in entities {
                    if entity.kind == MessageEntityKind::BotCommand {
                        let command = &data.as_str()[entity.offset as usize
                            ..entity.offset as usize + entity.length as usize];
                        match command {
                            "/photo" => {
                                info!("Got a photo request from {}", chat.id());

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
            } else {
                warn!("Event received with an error {:?}", update);
            };
        })
        .await;

    Ok(())
}

#[actix_web::main]
async fn main() -> Result<()> {
    env_logger::init();

    try_join!(
        HttpServer::new(|| {
            App::new()
                .wrap(Logger::default())
                .service(resource("/").to(index))
        })
        .bind(WEB_SERVER_BIND.to_string())?
        .run()
        .err_into(),
        bot(),
    )?;

    Ok(())
}
