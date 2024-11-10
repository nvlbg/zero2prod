use std::net::TcpListener;
use sqlx::PgPool;
use zero2prod::startup::run;
use zero2prod::configuration::get_configuration;

#[tokio::main]
async fn main() -> Result<(), std::io::Error> {
    let configuration = get_configuration()
        .expect("Failed to read configuration.");
    let listen_address = format!("127.0.0.1:{}", configuration.http_listen_port);
    let listener = TcpListener::bind(listen_address)?;
    let db_connection_pool = PgPool::connect(
            &configuration.database_settings.connection_string()
        )
        .await
        .expect("Failed to connect to postgres");
    run(listener, db_connection_pool)?.await
}
