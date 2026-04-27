mod config;
mod evaluation;
mod routes;
mod state;

use axum::http::{header, HeaderValue, Method};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tower_http::{cors::CorsLayer, timeout::TimeoutLayer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = config::Config::from_env();

    let db = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(600))
        .connect_lazy(&config.database_url)
        .expect("Invalid DATABASE_URL");

    let allowed_origin: HeaderValue = config
        .allowed_origin
        .parse()
        .expect("ALLOWED_ORIGIN is not a valid header value");

    let cors = CorsLayer::new()
        .allow_origin(allowed_origin)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let addr = config.addr();

    let state = state::AppState {
        db,
        jwt_secret: config.jwt_secret,
    };

    let app = routes::routes(state)
        .layer(cors)
        .layer(TimeoutLayer::new(Duration::from_secs(30)));
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("Failed to bind");

    tracing::info!("Server listening on {}", addr);

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("Server error");
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
    tracing::info!("Shutdown signal received, draining connections...");
}
