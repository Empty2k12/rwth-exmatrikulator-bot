mod commands;
mod db;

use std::env;

use anyhow::Result;
use futures::StreamExt;
use sqlx::postgres::PgPoolOptions;
use telegram_bot::*;

pub use crate::db::{get_chatter_by_id, Chatter};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    log::info!("Starting up RWTH Exmatrikulator Bot");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&env::var("DATABASE_URL")?)
        .await?;

    log::info!("Running database migrations, stand by!");

    sqlx::migrate!("db/migrations").run(&pool).await?;

    log::info!("Establishing connection to Telegram servers.");

    let token = env::var("TELEGRAM_BOT_TOKEN").expect("TELEGRAM_BOT_TOKEN not set");
    let api = Api::new(token);

    log::info!("Starting to handle messages.");

    let mut stream = api.stream();
    while let Some(update) = stream.next().await {
        let update = update?;

        match update.kind {
            UpdateKind::Message(message) => {
                let user_id: i64 = message.from.id.into();
                let chatter = get_chatter_by_id(user_id, &pool).await?;

                match message.kind {
                    MessageKind::Text { ref data, .. } => match data.as_str() {
                        "/aboutme" => {
                            commands::handle_command_aboutme(&api, &chatter, message).await?
                        }
                        "/verifyAll" => {
                            commands::handle_command_verify_all(&api, &chatter, message).await?
                        }
                        _ => {
                            log::info!("<{}>: {}", &message.from.first_name, data);
                            api.send(message.text_reply(format!(
                                "Hi, {}! You just wrote '{}'",
                                &message.from.first_name, data
                            )))
                            .await?;
                        }
                    },
                    MessageKind::NewChatMembers { ref data, .. } => {
                        // Show the captcha for new unverified users
                        if chatter.is_some() && !chatter.unwrap().is_verified {
                            match handle_new_member(&api, &message, data).await {
                                Ok(_) => {}
                                Err(_) => {}
                            }
                        }
                    }
                    MessageKind::LeftChatMember { .. } => {
                        // Delete Leaving Messages of unverified users
                        if chatter.is_some() && !chatter.unwrap().is_verified {
                            match api.send(&message.delete()).await {
                                Ok(_) => {}
                                Err(_) => {}
                            }
                        }
                    }
                    unhandled_message_kind => {
                        log::debug!("unhandled message kind: {:?}", unhandled_message_kind)
                    }
                }
            }
            UpdateKind::CallbackQuery(callback_query) => {
                let button_data = callback_query
                    .data
                    .clone()
                    .expect("button data was not set");

                match Some(button_data) {
                    Some(button_data) if is_user_verifying(&button_data, &callback_query) => {
                        handle_user_verification(&api, &callback_query, &pool).await?
                    }
                    _ => {
                        api.send(&callback_query.answer("Dieser Button ist nicht für dich!"))
                            .await?;
                    }
                }
            }
            unhandled_event => log::debug!("unhandled event: {:?}", unhandled_event),
        }
    }
    Ok(())
}

async fn handle_new_member(
    api: &Api,
    message: &telegram_bot::Message,
    users: &Vec<telegram_bot::User>,
) -> Result<(), Error> {
    match &message.chat {
        MessageChat::Group(group) => {
            send_captcha(&api, &message, users, &group.title).await?;
        }
        MessageChat::Supergroup(group) => {
            send_captcha(&api, &message, users, &group.title).await?;
        }
        other_chat_type => log::debug!("other chat type: {:?}", other_chat_type),
    }
    Ok(())
}

async fn send_captcha(
    api: &Api,
    message: &telegram_bot::Message,
    users: &Vec<telegram_bot::User>,
    group_title: &String,
) -> Result<(), Error> {
    let message_text = format!(
        "Willkommen in {}, [{}](tg://user?id={})!\n\nUm Spam zu verhinden bitte ich dich, den Button unten innerhalb von 30 Sekunden zu drücken. Danke!",
        group_title,
        &users[0].first_name,
        &users[0].id,
    );

    let text = format!("notabot_{}", &users[0].id);
    let inline_keyboard = reply_markup!(inline_keyboard,
        ["Ich bin kein Bot!" callback text]
    );

    let mut reply_captcha = message.text_reply(message_text);
    api.send(
        reply_captcha
            .parse_mode(ParseMode::Markdown)
            .reply_markup(inline_keyboard),
    )
    .await?;

    Ok(())
}

fn is_user_verifying(button_data: &String, callback_query: &CallbackQuery) -> bool {
    if button_data.starts_with("notabot") {
        let parts = button_data.split("_").collect::<Vec<&str>>();
        if parts.len() == 2 && parts[1] == format!("{}", &callback_query.from.id) {
            return true;
        }
    }
    return false;
}

async fn delete_message_and_quoted_message(api: &Api, message: &Message) -> Result<(), Error> {
    if let Some(reply_box) = &message.reply_to_message {
        match &**reply_box {
            MessageOrChannelPost::Message(message) => {
                api.send(&message.delete()).await?;
            }
            _ => { /* */ }
        }
    }
    api.send(&message.delete()).await?;
    Ok(())
}

async fn handle_user_verification(
    api: &Api,
    callback_query: &CallbackQuery,
    pool: &sqlx::PgPool,
) -> anyhow::Result<()> {
    match api.send(&callback_query.acknowledge()).await {
        Ok(_) => {}
        Err(_) => {}
    }

    let button_message = callback_query
        .message
        .as_ref()
        .expect("button message was null");

    match button_message {
        MessageOrChannelPost::Message(message) => {
            let res = api.send(&message.chat.text(format!("[{}](tg://user?id={}), danke, dass du dich verifiziert hast. Ich werde in anderen Gruppen nicht mehr fragen.", &callback_query.from.first_name, &callback_query.from.id)).parse_mode(ParseMode::Markdown)).await?;

            let user_id: i64 = callback_query.from.id.into();
            let _: Option<(bool,)> =
                sqlx::query_as("INSERT INTO chatters (telegram_id, is_verified) VALUES ($1, true)")
                    .bind(user_id)
                    .fetch_optional(pool)
                    .await?;

            match delete_message_and_quoted_message(&api, &message).await {
                Ok(_) => {}
                Err(_) => {}
            }

            // Delay deletion of the thanking message by 5 seconds.
            tokio::time::delay_for(std::time::Duration::from_secs(10)).await;
            match res {
                MessageOrChannelPost::Message(message) => {
                    match delete_message_and_quoted_message(&api, &message).await {
                        Ok(_) => {}
                        Err(_) => {}
                    }
                }
                _ => { /* */ }
            }
        }
        _ => { /* */ }
    }
    Ok(())
}
