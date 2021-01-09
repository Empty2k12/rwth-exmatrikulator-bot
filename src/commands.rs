use telegram_bot::*;

pub use crate::db::{get_chatter_by_id, is_chatter_admin, Chatter};

pub async fn handle_command_aboutme(
    api: &Api,
    chatter: &Option<Chatter>,
    message: Message,
) -> anyhow::Result<()> {
    log::debug!("Handling /aboutme command for {:?}", chatter);

    let message_text = if chatter.is_some() {
        format!(
            "Hi, {}! Das weiß ich über dich:\n```\n{:?}\n```",
            &message.from.first_name,
            chatter.as_ref().unwrap()
        )
    } else {
        format!(
            "Hi, {}!\nDu hast dich bisher noch nicht verifiziert. Wenn ein Admin /verifyAll ausführt, wirst du automtaisch verifiziert.",
            &message.from.first_name,
        )
    };
    api.send(
        message
            .text_reply(message_text)
            .parse_mode(ParseMode::Markdown),
    )
    .await?;
    Ok(())
}

pub async fn handle_command_verify_all(
    api: &Api,
    chatter: &Option<Chatter>,
    message: Message,
) -> anyhow::Result<()> {
    log::debug!("Handling /verifyAll command for {:?}", chatter);

    match is_chatter_admin(&api, &message, &chatter).await {
        Ok(is_admin) if is_admin => log::debug!("Chatter is admin, {:?}", chatter),
        _ => log::debug!("Chatter is not an admin, {:?}", chatter),
    }
    Ok(())
}
