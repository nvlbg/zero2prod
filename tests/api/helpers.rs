use std::sync::LazyLock;

use sqlx::{postgres::PgPoolOptions, Connection, Executor, PgConnection, PgPool};
use uuid::Uuid;
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
    pub db_pool: PgPool,
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
}

pub async fn spawn_app() -> TestApp {
    LazyLock::force(&TRACING);

    // Randomise configuration to ensure test isolation
    let mut configuration = get_configuration().expect("Failed to read configuration.");
    // Use different database for each test
    configuration.database.database_name = Uuid::new_v4().to_string();
    // Use different listen port for each test
    configuration.application.http_listen_port = 0;

    // Create and migrate the database
    configure_database(&configuration.database).await;

    // Launch the application as a background task
    let application = Application::build(configuration.clone()).await.expect("Failed to build the application");
    let address = format!("http://127.0.0.1:{}", application.port());
    let _ = tokio::spawn(application.run_until_stopped());

    TestApp {
        address,
        db_pool: get_connection_pool(&configuration.database),
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
