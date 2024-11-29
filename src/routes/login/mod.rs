use std::fmt::Write;
use actix_web::{
    http::{
        header::{ContentType, LOCATION},
        StatusCode,
    }, web, HttpResponse, ResponseError
};
use actix_web_flash_messages::{FlashMessage, IncomingFlashMessages};
use sqlx::PgPool;

use crate::{authentication::{validate_credentials, AuthError, Credentials}, session_state::TypedSession};

use super::error_chain_fmt;

pub async fn get_login(flash_messages: IncomingFlashMessages) -> HttpResponse {
    let mut error_html = String::new();
    for m in flash_messages.iter() {
        writeln!(error_html, "<p><i>{}</i></p>", m.content()).unwrap();
    }
    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(include_str!("login.html"), error_html))
}

#[derive(serde::Deserialize)]
pub struct LoginFormData {
    username: String,
    password: String,
}

#[derive(thiserror::Error)]
pub enum LoginError {
    #[error("Invalid credentials")]
    AuthError(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for LoginError {
    fn status_code(&self) -> StatusCode {
        StatusCode::SEE_OTHER
    }

    fn error_response(&self) -> HttpResponse<actix_web::body::BoxBody> {
        FlashMessage::error(self.to_string()).send();
        HttpResponse::SeeOther()
            .insert_header((LOCATION, "/login"))
            .finish()
    }
}

#[tracing::instrument(name = "Login", skip(form, pool, session))]
pub async fn post_login(
    form: web::Form<LoginFormData>,
    pool: web::Data<PgPool>,
    session: TypedSession,
) -> Result<HttpResponse, LoginError> {
    let credentials = Credentials {
        username: form.0.username,
        password: form.0.password,
    };
    let user_id = validate_credentials(&pool, credentials)
        .await
        .map_err(|e| match e {
            AuthError::InvalidCredentials(_) => LoginError::AuthError(e.into()),
            AuthError::UnexpectedError(_) => LoginError::UnexpectedError(e.into()),
        })?;
    tracing::Span::current().record("user_id", tracing::field::display(&user_id));
    session.renew();
    session.insert_user_id(user_id).map_err(|e| LoginError::UnexpectedError(e.into()))?;
    Ok(HttpResponse::SeeOther()
        .insert_header((LOCATION, "/admin/dashboard"))
        .finish())
}
