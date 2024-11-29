use std::fmt::Write;
use actix_web::{
    http::
        header::ContentType
    ,
    web, HttpResponse,
};
use actix_web_flash_messages::{FlashMessage, IncomingFlashMessages};
use anyhow::Context;
use sqlx::PgPool;

use crate::{
    domain::SubscriberEmail,
    email_client::EmailClient,
};

pub async fn get_publish_newsletters(flash_messages: IncomingFlashMessages) -> HttpResponse {
    let mut msg_html = String::new();
    for m in flash_messages.iter() {
        writeln!(msg_html, "<p><i>{}</i></p>", m.content()).unwrap();
    }

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(include_str!("newsletters.html"), msg_html))
}

#[derive(serde::Deserialize)]
pub struct BodyData {
    title: String,
    content_text: String,
    content_html: String,
}

struct ConfirmedSubscriber {
    email: String,
}

#[tracing::instrument(
    name = "Publish a new newsletter",
    skip(body, pool, email_client),
)]
pub async fn post_publish_newsletters(
    body: web::Form<BodyData>,
    pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
) -> Result<HttpResponse, actix_web::Error> {
    let subscribers = get_confirmed_subscribers(&pool).await.map_err(actix_web::error::ErrorInternalServerError)?;
    for subscriber in subscribers {
        let subscriber_email = match SubscriberEmail::parse(subscriber.email) {
            Ok(email) => email,
            Err(error) => {
                tracing::warn!(
                    "A confirmed subscriber is using an invalid email address.\n{}",
                    error
                );
                continue;
            }
        };
        email_client
            .send_email(
                &subscriber_email,
                &body.title,
                &body.content_html,
                &body.content_text,
            )
            .await
            .with_context(|| format!("Failed to send newsletter issue to {}", subscriber_email))
            .map_err(actix_web::error::ErrorInternalServerError)?;
    }
    FlashMessage::info("The newsletter issue has been published!").send();
    Ok(HttpResponse::Ok().finish())
}

#[tracing::instrument(name = "Get confirmed subscribers", skip(pool))]
async fn get_confirmed_subscribers(
    pool: &PgPool,
) -> Result<Vec<ConfirmedSubscriber>, anyhow::Error> {
    let rows = sqlx::query_as!(
        ConfirmedSubscriber,
        r#"
        SELECT email
        FROM subscriptions
        WHERE status = 'confirmed'
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
