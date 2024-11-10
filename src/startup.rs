use std::net::TcpListener;
use actix_web::{dev::Server, web, App, HttpServer};
use sqlx::PgPool;
use crate::routes::{health, subscribe};

pub fn run(listener: TcpListener, connection_pool: PgPool) -> Result<Server, std::io::Error> {
    let connection_pool = web::Data::new(connection_pool);
    let server = HttpServer::new(move || {
            App::new()
                .route("/health_check", web::get().to(health))
                .route("/subscriptions", web::post().to(subscribe))
                .app_data(connection_pool.clone())
        })
        .listen(listener)?
        .run();
    Ok(server)
}
