use std::sync::LazyLock;

use reqwest::Url;
use sqlx::{postgres::PgPoolOptions, Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;
use wiremock::MockServer;
use zero2prod::{
    configuration::{get_configuration, DatabaseSettings}, startup::{get_connection_pool, Application}, telemetry::{get_subscriber, init_subscriber}
};

static TRACING: LazyLock<()> = LazyLock::new(|| {
    if std::env::var("TEST_LOG").is_ok() {
        let subscriber = get_subscriber("debug".to_string(), std::io::stdout);
        init_subscriber(subscriber);
    } else {
        let subscriber = get_subscriber("debug".to_string(), std::io::sink);
        init_subscriber(subscriber);
    };
});

pub struct TestApp {
    pub address: String,
    pub port: u16,
    pub db_pool: PgPool,
    pub email_server: MockServer,
}

pub struct ConfirmationLinks {
    pub html: reqwest::Url,
    pub text: reqwest::Url,
}

impl TestApp {
    pub async fn post_subscriptions(&self, body: String) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{}/subscriptions", &self.address))
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .expect("Failed to post to /subscriptions")
    }

    pub async fn post_newsletters(&self, body: serde_json::Value) -> reqwest::Response {
        reqwest::Client::new()
            .post(format!("{}/newsletters", &self.address))
            .json(&body)
            .send()
            .await
            .expect("Failed to execute request")
    }

    pub fn get_confirmation_links(&self, email_request: &wiremock::Request) -> ConfirmationLinks {
        let body: serde_json::Value = serde_json::from_slice(&email_request.body).unwrap();
        let get_link = |s: &str| {
            let links: Vec<_> = linkify::LinkFinder::new()
                .links(s)
                .filter(|l| *l.kind() == linkify::LinkKind::Url)
                .collect();
            assert_eq!(links.len(), 1);
            let raw_link = links[0].as_str().to_owned();
            let mut confirmation_link = Url::parse(&raw_link).unwrap();
            // Let's make sure we don't call random APIs on the web
            assert_eq!(confirmation_link.host_str().unwrap(), "127.0.0.1");
            // Let's rewrite the URL to include the port
            confirmation_link.set_port(Some(self.port)).unwrap();
            confirmation_link
        };

        let html_link = get_link(&body["html"].as_str().unwrap());
        let text_link = get_link(&body["html"].as_str().unwrap());

        ConfirmationLinks {
            html: html_link,
            text: text_link,
        }
    }
}

pub async fn spawn_app() -> TestApp {
    LazyLock::force(&TRACING);

    // Launch a mock server to stand in for mailersend's API
    let email_server = MockServer::start().await;

    // Randomise configuration to ensure test isolation
    let mut configuration = get_configuration().expect("Failed to read configuration.");
    // Use different database for each test
    configuration.database.database_name = Uuid::new_v4().to_string();
    // Use different listen port for each test
    configuration.application.http_listen_port = 0;
    // Use the mock server as email API
    configuration.email_client.base_url = email_server.uri();

    // Create and migrate the database
    configure_database(&configuration.database).await;

    // Launch the application as a background task
    let application = Application::build(configuration.clone()).await.expect("Failed to build the application");
    let port = application.port();
    let address = format!("http://127.0.0.1:{}", port);
    let _ = tokio::spawn(application.run_until_stopped());

    TestApp {
        address,
        port,
        db_pool: get_connection_pool(&configuration.database),
        email_server,
    }
}

async fn configure_database(settings: &DatabaseSettings) -> PgPool {
    // Create database
    let maintenance_settings = DatabaseSettings {
        database_name: "postgres".to_string(), // database that comes installed with every postgres
        // installation
        username: "postgres".to_string(),
        password: "password".to_string(),
        ..settings.clone()
    };
    let mut connection = PgConnection::connect_with(&maintenance_settings.connect_options())
        .await
        .expect("Failed to connect to postgres");

    connection
        .execute(
            format!(
                r#"CREATE DATABASE "{}" WITH OWNER "{}";"#,
                settings.database_name, settings.username
            )
            .as_str(),
        )
        .await
        .expect("Failed to create database");

    let connection_pool = PgPoolOptions::new().connect_lazy_with(settings.connect_options());

    sqlx::migrate!("./migrations")
        .run(&connection_pool)
        .await
        .expect("Failed to migrate database");

    connection_pool
}
