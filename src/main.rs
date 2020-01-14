use futures::prelude::*;
use serde::Deserialize;
use std::env;
use telegram_bot::prelude::*;
use telegram_bot::{Api, UpdateKind};

#[derive(Debug)]
struct Error {
    message: String,
}

#[derive(Deserialize, Debug)]
struct Monitor {
    mid: String,
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

#[tokio::main]
async fn main() -> Result<(), Error> {
    let token = env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN not set");

    let base_url = format!(
        "http://{host}:{port}/{token}",
        host = env::var("SHINOBI_HOST").expect("SHINOBI_HOST is required"),
        port = env::var("SHINOBI_PORT").unwrap_or_else(|_| "8080".to_string()),
        token = env::var("SHINOBI_TOKEN").expect("SHINOBI_TOKEN is required")
    );
    let group_key = env::var("SHINOBI_GROUP_KEY").expect("SHINOBI_GROUP_KEY is required");

    let api = Api::new(token);
    let mut stream = api.stream();

    while let Some(update) = stream.next().await {
        if let UpdateKind::Message(telegram_bot::types::message::Message {
            from: _user,
            chat,
            kind: telegram_bot::types::message::MessageKind::Text { data, entities },
            ..
        }) = update?.kind
        {
            for entity in &entities {
                if entity.kind == telegram_bot::types::message::MessageEntityKind::BotCommand {
                    let command = &data.as_str()
                        [entity.offset as usize..entity.offset as usize + entity.length as usize];
                    if command == "/photo" {
                        let monitors: Vec<Monitor> = reqwest::get(&format!(
                            "{base_url}/smonitor/{group_key}",
                            base_url = base_url,
                            group_key = group_key
                        ))
                        .await?
                        .json()
                        .await?;

                        for monitor in &monitors {
                            let bytes = reqwest::get(&format!(
                                "{base_url}/jpeg/{group_key}/{monitor_id}/s.jpg",
                                base_url = base_url,
                                group_key = group_key,
                                monitor_id = monitor.mid
                            ))
                            .await?
                            .bytes()
                            .await?;

                            api.send(chat.photo(telegram_bot::types::InputFileUpload::with_data(
                                bytes, "teste",
                            )))
                            .await?;
                        }
                    }
                }
            }
        }
    }

    Ok(())
}
