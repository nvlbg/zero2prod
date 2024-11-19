use sqlx::Executor;
use actix_web::{web, HttpResponse, Responder};
use chrono::Utc;
use rand::{distributions::Alphanumeric, thread_rng, Rng};
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
    domain::{NewSubscriber, SubscriberEmail, SubscriberName},
    email_client::EmailClient,
    startup::ApplicationBaseUrl,
};

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
) -> impl Responder {
    let mut transaction = match connection_pool.begin().await {
        Ok(transaction) => transaction,
        Err(_) => return HttpResponse::InternalServerError(),
    };
    let subscriber = match form.0.try_into() {
        Ok(subscriber) => subscriber,
        Err(_) => return HttpResponse::BadRequest(),
    };
    let subscriber_uuid = match insert_subscriber(&mut transaction, &subscriber).await {
        Ok(subscriber_uuid) => { subscriber_uuid },
        Err(_) => return HttpResponse::InternalServerError(),
    };
    let subscription_token = generate_subscription_token();
    if store_token(&mut transaction, subscriber_uuid, &subscription_token).await.is_err() {
        return HttpResponse::InternalServerError();
    }
    if transaction.commit().await.is_err() {
        return HttpResponse::InternalServerError();
    }
    match send_confirmation_email(&email_client, subscriber, &base_url.0, &subscription_token).await
    {
        Ok(_) => HttpResponse::Ok(),
        Err(e) => {
            tracing::error!("Failed to send out an email: {:?}", e);
            return HttpResponse::InternalServerError();
        }
    }
}

#[tracing::instrument(name = "Saving subscriber in the database", skip(transaction, subscriber))]
async fn insert_subscriber(transaction: &mut Transaction<'_, Postgres>, subscriber: &NewSubscriber) -> Result<Uuid, sqlx::Error> {
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
    transaction.execute(query)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(subscriber_uuid)
}

#[tracing::instrument(
    name = "Store subscription token",
    skip(transaction, subscriber_uuid, subscription_token)
)]
async fn store_token(transaction: &mut Transaction<'_, Postgres>, subscriber_uuid: Uuid, subscription_token: &str) -> Result<(), sqlx::Error> {
    let query = sqlx::query!(
        r#"
            INSERT INTO subscription_tokens (subscription_token, subscriber_id)
            VALUES ($1, $2)
        "#,
        subscription_token,
        subscriber_uuid,
    );
    transaction.execute(query)
    .await
    .map_err(|e| {
        tracing::error!("Failed to execute query: {:?}", e);
        e
    })?;
    Ok(())
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
        .send_email(subscriber.email, "Welcome!", &html_body, &text_body)
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
