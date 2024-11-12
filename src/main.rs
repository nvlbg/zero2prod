use sqlx::PgPool;
use std::net::TcpListener;
use zero2prod::configuration::get_configuration;
use zero2prod::startup::run;
use zero2prod::telemetry;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let subscriber = telemetry::get_subscriber("info".to_string(), std::io::stdout);
    telemetry::init_subscriber(subscriber);

    let configuration = get_configuration().expect("Failed to read configuration.");
    let listen_address = format!(
        "{}:{}",
        configuration.application.http_bind_address,
        configuration.application.http_listen_port
    );
    let listener = TcpListener::bind(listen_address)?;
    let db_connection_pool =
        PgPool::connect_lazy(&configuration.database.connection_string())
            .expect("Failed to connect to postgres");
    run(listener, db_connection_pool)?.await
}
