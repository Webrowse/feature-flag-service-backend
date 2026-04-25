use dotenvy::dotenv;
use std::env;

pub struct Config {
    pub port: u16,
    pub host: String,
    pub database_url: String,
    pub jwt_secret: String,
    pub allowed_origin: String,
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

        Self {
            port,
            host,
            database_url,
            jwt_secret,
            allowed_origin,
        }
    }

    pub fn addr(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}
