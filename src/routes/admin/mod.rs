use actix_web::{
    http::header::ContentType,
    web, HttpResponse,
};
use actix_web_flash_messages::FlashMessage;
use actix_web_flash_messages::IncomingFlashMessages;
use anyhow::Context;
use sqlx::PgPool;
use std::fmt::Write;
use uuid::Uuid;

use crate::{
    authentication::{change_password, validate_credentials, AuthError, Credentials, UserId},
    session_state::TypedSession, utils::see_other,
};

mod newsletters;

pub use newsletters::*;

pub async fn admin_dashboard(
    user_id: web::ReqData<UserId>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    let username = get_username(&user_id.into_inner(), &pool)
            .await
            .map_err(actix_web::error::ErrorInternalServerError)?;
    Ok(HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(include_str!("dashboard.html"), username)))
}

pub async fn change_password_get(
    flash_messages: IncomingFlashMessages,
) -> Result<HttpResponse, actix_web::Error> {
    let mut msgs_html = String::new();
    for m in flash_messages.iter() {
        writeln!(msgs_html, "<p><i>{}</i></p>", m.content()).unwrap();
    }
    Ok(HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(include_str!("change_password.html"), msgs_html)))
}

#[derive(serde::Deserialize)]
pub struct ChangePasswordFormData {
    pub current_password: String,
    pub new_password: String,
    pub new_password_check: String,
}

pub async fn change_password_post(
    form: web::Form<ChangePasswordFormData>,
    user_id: web::ReqData<UserId>,
    pool: web::Data<PgPool>,
) -> Result<HttpResponse, actix_web::Error> {
    if form.new_password != form.new_password_check {
        FlashMessage::error(
            "You entered two different new passwords - the field values must match.".to_string(),
        )
        .send();
        return Ok(see_other("/admin/password"));
    }
    let user_id = *user_id.into_inner();
    let username = get_username(&user_id, &pool)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let credentials = Credentials {
        username,
        password: form.0.current_password,
    };

    if let Err(e) = validate_credentials(&pool, credentials).await {
        match e {
            AuthError::InvalidCredentials(_) => {
                FlashMessage::error("The current password is incorrect".to_string()).send();
                return Ok(see_other("/admin/password"));
            }
            AuthError::UnexpectedError(e) => {
                return Err(actix_web::error::ErrorInternalServerError(e));
            }
        }
    }
    change_password(&pool, user_id, form.0.new_password).await.map_err(actix_web::error::ErrorInternalServerError)?;
    FlashMessage::info("You have successfully changed your password".to_string()).send();
    Ok(see_other("/admin/password"))
}

pub async fn logout(session: TypedSession) -> Result<HttpResponse, actix_web::Error> {
    FlashMessage::info("You have successfully logged out".to_string()).send();
    session.logout();
    Ok(see_other("/login"))
}

#[tracing::instrument(name = "Get username", skip(pool))]
async fn get_username(user_id: &Uuid, pool: &PgPool) -> Result<String, anyhow::Error> {
    let row = sqlx::query!(
        r#"SELECT username
        FROM users
        WHERE user_id = $1"#,
        user_id
    )
    .fetch_one(pool)
    .await
    .context("Failed to perform a query to retrieve a username")?;
    Ok(row.username)
}
