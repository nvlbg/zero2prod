use crate::{
    authentication::reject_anonymous_users, configuration::{DatabaseSettings, Settings}, email_client::EmailClient, routes::{admin_dashboard, change_password_get, change_password_post, confirm, get_login, get_publish_newsletters, health, home, logout, post_login, post_publish_newsletters, subscribe}
};
use actix_session::{storage::RedisSessionStore, SessionMiddleware};
use actix_web::{cookie::Key, dev::Server, middleware::from_fn, web, App, HttpServer};
use actix_web_flash_messages::{storage::CookieMessageStore, FlashMessagesFramework};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::net::TcpListener;
use tracing_actix_web::TracingLogger;

pub struct Application {
    port: u16,
    server: Server,
}

impl Application {
    pub async fn build(configuration: Settings) -> Result<Self, anyhow::Error> {
        let db_connection_pool = get_connection_pool(&configuration.database);

        let email_client = configuration
            .email_client
            .client();

        let listen_address = format!(
            "{}:{}",
            configuration.application.http_bind_address, configuration.application.http_listen_port
        );
        let listener = TcpListener::bind(listen_address)?;
        let port = listener.local_addr().unwrap().port();

        let server = run(
            listener,
            db_connection_pool,
            email_client,
            configuration.application.base_url,
            configuration.application.hmac_secret,
            configuration.redis_uri,
        ).await?;

        Ok(Self { port, server })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub async fn run_until_stopped(self) -> Result<(), std::io::Error> {
        self.server.await
    }
}

pub fn get_connection_pool(configuration: &DatabaseSettings) -> PgPool {
    PgPoolOptions::new().connect_lazy_with(configuration.connect_options())
}

pub struct ApplicationBaseUrl(pub String);

pub async fn run(
    listener: TcpListener,
    connection_pool: PgPool,
    email_client: EmailClient,
    base_url: String,
    hmac_secret: String,
    redis_uri: String,
) -> Result<Server, anyhow::Error> {
    let connection_pool = web::Data::new(connection_pool);
    let email_client = web::Data::new(email_client);
    let base_url = web::Data::new(ApplicationBaseUrl(base_url));

    let secret_key = Key::from(hmac_secret.as_bytes());
    let message_store = CookieMessageStore::builder(secret_key.clone()).build();
    let message_framework = FlashMessagesFramework::builder(message_store).build();

    let redis_store = RedisSessionStore::new(redis_uri).await?;

    let server = HttpServer::new(move || {
        App::new()
            .wrap(SessionMiddleware::new(redis_store.clone(), secret_key.clone()))
            .wrap(message_framework.clone())
            .wrap(TracingLogger::default())
            .route("/", web::get().to(home))
            .route("/login", web::get().to(get_login))
            .route("/login", web::post().to(post_login))
            .route("/health_check", web::get().to(health))
            .route("/subscriptions", web::post().to(subscribe))
            .route("/subscriptions/confirm", web::get().to(confirm))
            .service(
                web::scope("/admin")
                    .wrap(from_fn(reject_anonymous_users))
                    .route("/dashboard", web::get().to(admin_dashboard))
                    .route("/password", web::get().to(change_password_get))
                    .route("/password", web::post().to(change_password_post))
                    .route("/logout", web::post().to(logout))
                    .route("/newsletters", web::get().to(get_publish_newsletters))
                    .route("/newsletters", web::post().to(post_publish_newsletters))
            )
            .app_data(connection_pool.clone())
            .app_data(email_client.clone())
            .app_data(base_url.clone())
    })
    .listen(listener)?
    .run();
    Ok(server)
}
