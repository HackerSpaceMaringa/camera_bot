#![feature(async_await)]

use futures::{
    future::{err, join_all, ok},
    prelude::*,
    stream::Stream,
};
use serde::Deserialize;
use std::{env, io::Cursor, time::Duration};
use telebot::{functions::*, Bot};

#[derive(Deserialize, Debug)]
struct Monitor {
    mid: String,
}

fn main() {
    // Create the bot
    let mut bot = Bot::new(&env::var("TELEGRAM_BOT_KEY").unwrap()).update_interval(200);
    let base_url = format!(
        "http://{host}:{port}/{token}",
        host = env::var("SHINOBI_HOST").expect("SHINOBI_HOST is required"),
        port = env::var("SHINOBI_PORT").unwrap_or_else(|_| "8080".to_owned()),
        token = env::var("SHINOBI_TOKEN").expect("SHINOBI_TOKEN is required")
    );
    let group_key = env::var("SHINOBI_GROUP_KEY").expect("SHINOBI_GROUP_KEY is required");

    let http_client = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .expect("IMPOSSIBLE TO RUN BOT WITHOUT A HTTP CLIENT!");

    // Register a reply command which answers a message
    let handle = bot
        .new_cmd("/photo")
        .and_then(move |(bot, msg)| {
            let response: Vec<Monitor> = http_client
                .get(&format!(
                    "{base_url}/smonitor/{group_key}",
                    base_url = base_url,
                    group_key = group_key
                ))
                .send()
                .unwrap()
                .json()
                .unwrap();

            join_all(
                response
                    .iter()
                    .map(|monitor| {
                        let mut resp = http_client
                            .get(&format!(
                                "{base_url}/jpeg/{group_key}/{monitor_id}/s.jpg",
                                base_url = base_url,
                                group_key = group_key,
                                monitor_id = monitor.mid
                            ))
                            .send()
                            .unwrap();

                        if resp.status().is_success() {
                            dbg!(&resp);

                            let mut buf = if let Some(length) = resp.headers().get("content-length")
                            {
                                if let Ok(length) = length.to_str() {
                                    if let Ok(length) = length.parse() {
                                        Vec::with_capacity(length)
                                    } else {
                                        Vec::new()
                                    }
                                } else {
                                    Vec::new()
                                }
                            } else {
                                Vec::new()
                            };

                            resp.copy_to(&mut buf).unwrap();

                            let cursor = Cursor::new(buf);

                            return bot
                                .photo(msg.chat.id)
                                .file(telebot::file::File::Memory {
                                    name: "photo_phoda.jpg".to_owned(),
                                    source: Box::new(cursor),
                                })
                                .send();
                        }

                        panic!("FUCK THIS SHIT");

                        //return bot
                        //.message(msg.chat.id, "Não foi possível obter uma foto!".to_owned())
                        //.send();
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .for_each(|_| Ok(()));

    bot.run_with(handle);
}
