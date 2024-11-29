use anyhow::Context;
use argon2::PasswordHasher;
use argon2::{password_hash::SaltString, Algorithm, Params, PasswordVerifier, Version};
use argon2::{Argon2, PasswordHash};
use sqlx::PgPool;
use uuid::Uuid;

use crate::telemetry::spawn_blocking_with_tracing;

#[derive(thiserror::Error, Debug)]
pub enum AuthError {
    #[error("Invalid credentials")]
    InvalidCredentials(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error),
}

pub struct Credentials {
    pub username: String,
    pub password: String,
}

#[tracing::instrument(name = "Validate user credentials", skip(pool, credentials))]
pub async fn validate_credentials(
    pool: &PgPool,
    credentials: Credentials,
) -> Result<uuid::Uuid, AuthError> {
    let (user_id, expected_password_hash) = get_stored_credentials(pool, &credentials.username)
        .await
        .map_err(AuthError::UnexpectedError)?
        .ok_or_else(|| AuthError::InvalidCredentials(anyhow::anyhow!("Unknown username.")))?;

    spawn_blocking_with_tracing(move || {
        tracing::info_span!("Verify password hash").in_scope(|| {
            let expected_password_hash = PasswordHash::new(&expected_password_hash)
                .context("Failed to parse hash in PHC string format")
                .map_err(AuthError::UnexpectedError)?;

            Argon2::default()
                .verify_password(credentials.password.as_bytes(), &expected_password_hash)
                .context("Invalid password")
                .map_err(AuthError::InvalidCredentials)
        })
    })
    .await
    .context("Failed to spawn blocking task")
    .map_err(AuthError::UnexpectedError)??;

    Ok(user_id)
}

#[tracing::instrument(name = "Change user password", skip(pool, user_id, password))]
pub async fn change_password(
    pool: &PgPool,
    user_id: Uuid,
    password: String,
) -> Result<(), anyhow::Error> {
    let password_hash = spawn_blocking_with_tracing(move || compute_password_hash(password))
        .await?
        .context("Failed to hash password")?;
    sqlx::query!(
        r#"UPDATE users
        SET password_hash = $1
        WHERE user_id = $2"#,
        password_hash,
        user_id,
    )
        .execute(pool)
        .await
        .context("Failed to change user's password in database")?;
    Ok(())
}

fn compute_password_hash(password: String) -> Result<String, anyhow::Error> {
    let salt = SaltString::generate(&mut rand::thread_rng());
    let password_hash = Argon2::new(
        Algorithm::Argon2id,
        Version::V0x13,
        Params::new(15000, 2, 1, None).unwrap(),
    )
    .hash_password(password.as_bytes(), &salt)?
    .to_string();
    Ok(password_hash)
}

#[tracing::instrument(name = "Fetch user credentials", skip(pool, username))]
async fn get_stored_credentials(
    pool: &PgPool,
    username: &str,
) -> Result<Option<(uuid::Uuid, String)>, anyhow::Error> {
    let row = sqlx::query!(
        r#"
        SELECT user_id, password_hash
        FROM users
        WHERE username = $1
        "#,
        username,
    )
    .fetch_optional(pool)
    .await
    .context("Failed to perform a query to fetch user credentials.")?
    .map(|row| (row.user_id, row.password_hash));
    Ok(row)
}
