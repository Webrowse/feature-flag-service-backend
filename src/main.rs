mod config;
mod evaluation;
mod routes;
mod state;

use axum::http::{header, HeaderValue, Method};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::timeout::TimeoutLayer;
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

    let parts: Vec<&str> = config
        .allowed_origin
        .split(',')
        .map(|o| o.trim())
        .filter(|o| !o.is_empty())
        .collect();

    let allow_origin = if parts.contains(&"*") {
        tracing::warn!("ALLOWED_ORIGIN contains wildcard — accepting all origins");
        AllowOrigin::any()
    } else {
        let origins: Vec<HeaderValue> = parts
            .iter()
            .map(|o| o.parse().expect("ALLOWED_ORIGIN contains invalid value"))
            .collect();
        tracing::info!("Allowed origins: {:?}", origins);
        AllowOrigin::list(origins)
    };

    let cors = CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([header::CONTENT_TYPE, header::AUTHORIZATION]);

    let addr = config.addr();

    let mailer = match (&config.smtp_host, &config.smtp_username, &config.smtp_password) {
        (Some(host), Some(username), Some(password)) => {
            use lettre::transport::smtp::authentication::Credentials;
            use lettre::{AsyncSmtpTransport, Tokio1Executor};
            let creds = Credentials::new(username.clone(), password.clone());
            match AsyncSmtpTransport::<Tokio1Executor>::relay(host) {
                Ok(builder) => {
                    tracing::info!("SMTP configured ({}:{})", host, config.smtp_port);
                    Some(builder.credentials(creds).port(config.smtp_port).build())
                }
                Err(e) => {
                    tracing::warn!("Failed to create SMTP transport: {}", e);
                    None
                }
            }
        }
        _ => {
            tracing::warn!("SMTP_HOST/SMTP_USERNAME/SMTP_PASSWORD not set — password reset emails will not be sent");
            None
        }
    };

    let state = state::AppState {
        db,
        jwt_secret: config.jwt_secret,
        mailer,
        smtp_from: config.smtp_from,
        app_url: config.app_url,
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
