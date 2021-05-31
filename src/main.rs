#![feature(once_cell)]
#![feature(async_closure)]

use anyhow::Result;
use futures::prelude::*;
use log::{debug, info};
use std::{env, error::Error, lazy::SyncLazy};
use teloxide::{prelude::*, utils::command::BotCommand};

static SHINOBI_API: SyncLazy<ShinobiApi> = SyncLazy::new(ShinobiApi::from_env);
static TELEGRAM_BOT: SyncLazy<AutoSend<Bot>> = SyncLazy::new(|| Bot::from_env().auto_send());

#[derive(BotCommand)]
#[command(rename = "lowercase")]
enum Command {
    Photo,
}

#[derive(serde::Deserialize, Debug)]
struct Monitor {
    mid: String,
}

struct ShinobiApi {
    group_key: String,
    url: String,
    token: String,
}

impl ShinobiApi {
    fn from_env() -> Self {
        Self {
            group_key: env::var("GROUP_KEY").expect("GROUP_KEY not provided"),
            url: env::var("SHINOBI_URL").expect("SHINOBI_URL not provided"),
            token: env::var("SHINOBI_TOKEN").expect("SHINOBI_TOKEN not provided"),
        }
    }

    async fn get_monitors(&self) -> Result<Vec<Monitor>> {
        debug!("retrieving monitors from {}", self.get_request_url());

        Ok(reqwest::get(&format!(
            "{base_url}/smonitor/{group_key}",
            base_url = self.get_request_url(),
            group_key = self.group_key,
        ))
        .await?
        .json::<Vec<Monitor>>()
        .await?)
    }

    fn get_request_url(&self) -> String {
        format!("{url}/{token}", url = self.url, token = self.token)
    }
}

impl Monitor {
    async fn get_photo(&self, shinobi_api: &ShinobiApi) -> Result<bytes::Bytes> {
        let photo = reqwest::get(&format!(
            "{base_url}/jpeg/{group_key}/{monitor_id}/s.jpg",
            base_url = shinobi_api.get_request_url(),
            group_key = shinobi_api.group_key,
            monitor_id = self.mid,
        ))
        .await?
        .bytes()
        .await?;

        Ok(photo)
    }
}

async fn send_photos_to_chat(cx: &UpdateWithCx<AutoSend<Bot>, Message>) -> Result<()> {
    let username = match &cx.update.chat.kind {
        teloxide::types::ChatKind::Private(teloxide::types::ChatPrivate { username, .. }) => {
            username
        }
        teloxide::types::ChatKind::Public(teloxide::types::ChatPublic { title, .. }) => title,
    }
    .as_ref()
    .unwrap_or(&"USUARIO_NAO_RECONHECIDO".to_string())
    .to_owned();

    let monitors = SHINOBI_API.get_monitors().await?;

    info!(
        "Sending photos of #{} monitors to chat {}",
        monitors.len(),
        username
    );

    let photos: Vec<teloxide::types::InputMedia> =
        future::join_all(monitors.iter().map(async move |m| {
            teloxide::types::InputMedia::Photo(teloxide::types::InputMediaPhoto::new(
                teloxide::types::InputFile::memory(
                    m.mid.to_owned(),
                    m.get_photo(&SHINOBI_API).await.unwrap().as_ref().to_owned(),
                ),
            ))
        }))
        .await;

    cx.answer_media_group(photos).await?;

    Ok(())
}

async fn answer(
    cx: UpdateWithCx<AutoSend<Bot>, Message>,
    command: Command,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match command {
        Command::Photo => {
            send_photos_to_chat(&cx).await?;
        }
    };

    Ok(())
}

#[tokio::main]
async fn main() {
    info!("Bot starting...");

    teloxide::enable_logging!();

    teloxide::commands_repl(TELEGRAM_BOT.clone(), "camera_bot", answer).await;
}
