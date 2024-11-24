use std::fmt::Display;

use actix_web::{http::StatusCode, web, HttpResponse, Responder, ResponseError};
use anyhow::Context;
use chrono::Utc;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use sqlx::Executor;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
    startup::ApplicationBaseUrl,
};

use super::error_chain_fmt;

#[derive(serde::Deserialize)]
pub struct FormData {
    pub email: String,
    pub name: String,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;

    fn try_from(value: FormData) -> Result<Self, Self::Error> {
        let name = SubscriberName::parse(value.name)?;
        let email = SubscriberEmail::parse(value.email)?;

        Ok(Self { email, name })
    }
}

#[tracing::instrument(
    name = "Addig a new subscriber",
    skip(form, connection_pool, email_client, base_url),
    fields(
        subscriber_email = %form.email,
        subscriber_name = %form.name
    )
)]
pub async fn subscribe(
    form: web::Form<FormData>,
    connection_pool: web::Data<PgPool>,
    email_client: web::Data<EmailClient>,
    base_url: web::Data<ApplicationBaseUrl>,
) -> Result<impl Responder, SubscribeError> {
    let subscriber = form.0.try_into().map_err(SubscribeError::ValidationError)?;
    let mut transaction = connection_pool.begin().await
        .context("Failed to acquire a connection from the pool")?;
    let subscriber_uuid = insert_subscriber(&mut transaction, &subscriber).await
        .context("Failed to insert new subscriber in the database")?;
    let subscription_token = generate_subscription_token();
    store_token(&mut transaction, subscriber_uuid, &subscription_token).await
        .context("Failed to store the confirmation")?;
    transaction.commit().await
        .context("Failed to commit a transaction to store a new subscriber")?;
    send_confirmation_email(&email_client, subscriber, &base_url.0, &subscription_token).await
        .context("Failed to send confirmation email")?;
    Ok(HttpResponse::Ok())
}

#[tracing::instrument(
    name = "Saving subscriber in the database",
    skip(transaction, subscriber)
)]
async fn insert_subscriber(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber: &NewSubscriber,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_uuid = Uuid::new_v4();
    let query = sqlx::query!(
        r#"
            INSERT INTO subscriptions (id, email, name, subscribed_at, status)
            VALUES ($1, $2, $3, $4, 'pending_confirmation')
        "#,
        subscriber_uuid,
        subscriber.email.as_ref(),
        subscriber.name.as_ref(),
        Utc::now(),
    );
    transaction.execute(query).await?;
    Ok(subscriber_uuid)
}

#[tracing::instrument(
    name = "Store subscription token",
    skip(transaction, subscriber_uuid, subscription_token)
)]
async fn store_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_uuid: Uuid,
    subscription_token: &str,
) -> Result<(), StoreTokenError> {
    let query = sqlx::query!(
        r#"
            INSERT INTO subscription_tokens (subscription_token, subscriber_id)
            VALUES ($1, $2)
        "#,
        subscription_token,
        subscriber_uuid,
    );
    transaction
        .execute(query)
        .await
        .map_err(StoreTokenError)?;
    Ok(())
}

#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl ResponseError for SubscribeError {
    fn status_code(&self) -> StatusCode {
        match self {
            SubscribeError::ValidationError(_) => StatusCode::BAD_REQUEST,
            | SubscribeError::UnexpectedError(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

pub struct StoreTokenError(sqlx::Error);

impl Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A database error was encountered while trying to store a subscription token."
        )
    }
}

impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl std::error::Error for StoreTokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

#[tracing::instrument(
    name = "Send confirmation email",
    skip(email_client, subscriber, base_url, subscription_token)
)]
async fn send_confirmation_email(
    email_client: &EmailClient,
    subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url, subscription_token
    );
    let html_body = format!(
        "Welcome to our newsletter!<br>\
        Click <a href=\"{}\">here</a> to confirm your subscription.",
        confirmation_link
    );
    let text_body = format!(
        "Welcome to our newsletter!\nVisit {} to confirm your subscription.",
        confirmation_link
    );
    email_client
        .send_email(&subscriber.email, "Welcome!", &html_body, &text_body)
        .await
}

/// Generate a random 25-characters-long case-sensitive subscription token.
fn generate_subscription_token() -> String {
    let mut rng = thread_rng();
    std::iter::repeat_with(|| rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}
