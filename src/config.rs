use dotenvy::dotenv;
use std::env;

pub struct Config {
    pub port: u16,
    pub host: String,
    pub database_url: String,
    pub jwt_secret: String,
    pub allowed_origin: String,
    pub smtp_host: Option<String>,
    pub smtp_port: u16,
    pub smtp_username: Option<String>,
    pub smtp_password: Option<String>,
    pub smtp_from: String,
    pub app_url: String,
}

impl Config {
    pub fn from_env() -> Self {
        if let Err(e) = dotenv() {
            tracing::warn!(".env file not loaded: {}", e);
        }

        let port = env::var("PORT")
            .expect("PORT is required")
            .parse::<u16>()
            .expect("PORT must be a valid u16");

        let host = env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string());

        let database_url = env::var("DATABASE_URL").expect("DATABASE_URL is required");

        let jwt_secret = env::var("JWT_SECRET").expect("JWT_SECRET is required");
        if jwt_secret.len() < 32 {
            panic!("JWT_SECRET must be at least 32 characters");
        }

        let allowed_origin =
            env::var("ALLOWED_ORIGIN").unwrap_or_else(|_| "http://localhost:3000".to_string());

        let smtp_host = env::var("SMTP_HOST").ok();
        let smtp_port = env::var("SMTP_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(587u16);
        let smtp_username = env::var("SMTP_USERNAME").ok();
        let smtp_password = env::var("SMTP_PASSWORD").ok();
        let smtp_from = env::var("SMTP_FROM").unwrap_or_else(|_| "noreply@example.com".to_string());
        let app_url = env::var("APP_URL").unwrap_or_else(|_| "http://localhost:3000".to_string());

        Self {
            port,
            host,
            database_url,
            jwt_secret,
            allowed_origin,
            smtp_host,
            smtp_port,
            smtp_username,
            smtp_password,
            smtp_from,
            app_url,
        }
    }

    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
