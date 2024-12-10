use sqlx::Executor;
use actix_web::{
    http::header::ContentType,
    web::{self, ReqData},
    HttpResponse,
};
use actix_web_flash_messages::{FlashMessage, IncomingFlashMessages};
use anyhow::Context;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;
use std::fmt::Write;

use crate::{
    authentication::UserId,
    idempotency::{save_response, try_processing, IdempotencyKey, NextAction},
    utils::see_other,
};

pub async fn get_publish_newsletters(flash_messages: IncomingFlashMessages) -> HttpResponse {
    let mut msg_html = String::new();
    for m in flash_messages.iter() {
        writeln!(msg_html, "<p><i>{}</i></p>", m.content()).unwrap();
    }
    let idempotency_key = uuid::Uuid::new_v4();

    HttpResponse::Ok()
        .content_type(ContentType::html())
        .body(format!(
            include_str!("newsletters.html"),
            msg_html, idempotency_key
        ))
}

#[derive(serde::Deserialize)]
pub struct BodyData {
    title: String,
    content_text: String,
    content_html: String,
    idempotency_key: String,
}

#[tracing::instrument(name = "Publish a new newsletter", skip_all, fields(user_id=%&*user_id))]
pub async fn post_publish_newsletters(
    body: web::Form<BodyData>,
    pool: web::Data<PgPool>,
    user_id: ReqData<UserId>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();
    let BodyData {
        title,
        content_html,
        content_text,
        idempotency_key,
    } = body.0;
    let idempotency_key: IdempotencyKey = idempotency_key
        .try_into()
        .map_err(actix_web::error::ErrorBadRequest)?;

    let mut transaction = match try_processing(&pool, &idempotency_key, *user_id).await.map_err(actix_web::error::ErrorInternalServerError)? {
        NextAction::StartProcessing(transaction) => transaction,
        NextAction::ReturnSavedResponse(response) => {
            success_message().send();
            return Ok(response);
        }
    };

    let issue_id = insert_newsletter_issue(
        &mut transaction,
        &title,
        &content_html,
        &content_text,
    )
        .await
        .context("Failed to store newsletter issue details")
        .map_err(actix_web::error::ErrorInternalServerError)?;

    enqueue_delivery_tasks(
        &mut transaction,
        issue_id
    )
        .await
        .context("Failed to enqueue delivery tasks")
        .map_err(actix_web::error::ErrorInternalServerError)?;

    let response = see_other("/admin/newsletters");
    let response = save_response(transaction, &idempotency_key, *user_id, response)
        .await
        .map_err(actix_web::error::ErrorInternalServerError)?;
    success_message().send();
    Ok(response)
}

#[tracing::instrument(skip_all)]
async fn insert_newsletter_issue(
    transaction: &mut Transaction<'_, Postgres>,
    title: &str,
    text_content: &str,
    html_content: &str,
) -> Result<Uuid, sqlx::Error> {
    let newsletter_issue_id = Uuid::new_v4();
    let query = sqlx::query!(
        r#"
        INSERT INTO newsletter_issues (
            newsletter_issue_id,
            title,
            text_content,
            html_content,
            published_at
        )
        VALUES ($1, $2, $3, $4, now())
        "#,
        newsletter_issue_id,
        title,
        text_content,
        html_content,
    );
    transaction.execute(query).await?;
    Ok(newsletter_issue_id)
}

#[tracing::instrument(skip_all)]
async fn enqueue_delivery_tasks(
    transaction: &mut Transaction<'_, Postgres>,
    newsletter_issue_id: Uuid,
) -> Result<(), sqlx::Error> {
    let query = sqlx::query!(
        r#"
        INSERT INTO issue_delivery_queue (
            newsletter_issue_id,
            subscriber_email
        )
        SELECT $1, email
        FROM subscriptions
        WHERE status = 'confirmed'
        "#,
        newsletter_issue_id,
    );
    transaction.execute(query).await?;
    Ok(())
}

fn success_message() -> FlashMessage {
    FlashMessage::info("The newsletter issue has been accepted - emails will go out shortly!")
}
