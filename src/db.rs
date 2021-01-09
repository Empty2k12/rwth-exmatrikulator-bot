use telegram_bot::*;

#[derive(sqlx::FromRow, Debug)]
pub struct Chatter {
    pub telegram_id: i64,
    pub is_verified: bool,
    pub is_global_admin: bool,
}

pub async fn get_chatter_by_id(
    user_id: i64,
    pool: &sqlx::Pool<sqlx::Postgres>,
) -> Result<Option<Chatter>, sqlx::Error> {
    return sqlx::query_as::<_, Chatter>(
        r#"
    SELECT is_global_admin, is_verified, created_at, telegram_id
    FROM chatters
    WHERE telegram_id = $1
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await;
}

pub async fn is_chatter_admin(
    api: &Api,
    message: &Message,
    chatter: &Option<Chatter>,
) -> anyhow::Result<bool> {
    if chatter.is_some() {
        let chatter = chatter.as_ref().unwrap();
        if chatter.is_global_admin {
            return Ok(true);
        }
    } else {
        let administrators: Vec<i64> = api
            .send(message.chat.get_administrators())
            .await?
            .iter()
            .map(|el| el.user.id.into())
            .collect();
        return Ok(administrators.contains(&message.from.id.into()));
    }
    return Ok(false);
}
