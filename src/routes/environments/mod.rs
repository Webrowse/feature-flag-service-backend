pub mod routes;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Environment {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub key: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateEnvironmentRequest {
    pub name: String,
    pub key: String,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateEnvironmentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct EnvironmentResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub name: String,
    pub key: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub fn validate_environment_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("Environment key cannot be empty".to_string());
    }
    if key.len() > 64 {
        return Err("Environment key is too long (max 64 characters)".to_string());
    }
    if !key.chars().next().map_or(false, |c| c.is_ascii_lowercase()) {
        return Err("Environment key must start with a lowercase letter".to_string());
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(
            "Environment key can only contain lowercase letters, numbers, underscores, and hyphens"
                .to_string(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_environment_key() {
        assert!(validate_environment_key("production").is_ok());
        assert!(validate_environment_key("staging").is_ok());
        assert!(validate_environment_key("dev-test").is_ok());
        assert!(validate_environment_key("env_123").is_ok());

        assert!(validate_environment_key("").is_err());
        assert!(validate_environment_key("Production").is_err()); // uppercase start
        assert!(validate_environment_key("_invalid").is_err());   // starts with underscore
        assert!(validate_environment_key("has space").is_err());
        assert!(validate_environment_key("has.dot").is_err());
    }
}
