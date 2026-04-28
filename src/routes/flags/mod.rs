pub mod routes;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct FeatureFlag {
    pub id: Uuid,
    pub project_id: Uuid,
    pub environment_id: Option<Uuid>,
    pub name: String,
    pub key: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub rollout_percentage: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateFlagRequest {
    pub name: String,
    pub key: String,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub rollout_percentage: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateFlagRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub enabled: Option<bool>,
    pub rollout_percentage: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct FlagResponse {
    pub id: Uuid,
    pub project_id: Uuid,
    pub environment_id: Option<Uuid>,
    pub name: String,
    pub key: String,
    pub description: Option<String>,
    pub enabled: bool,
    pub rollout_percentage: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub fn validate_flag_key(key: &str) -> Result<(), String> {
    if key.is_empty() {
        return Err("Flag key cannot be empty".to_string());
    }
    if key.len() > 64 {
        return Err("Flag key is too long (max 64 characters)".to_string());
    }
    if !key.chars().next().map_or(false, |c| c.is_ascii_lowercase()) {
        return Err("Flag key must start with a lowercase letter".to_string());
    }
    if !key
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(
            "Flag key can only contain lowercase letters, numbers, underscores, and hyphens"
                .to_string(),
        );
    }
    Ok(())
}

pub fn validate_rollout_percentage(percentage: i32) -> Result<(), String> {
    if !(0..=100).contains(&percentage) {
        return Err("Rollout percentage must be between 0 and 100".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_flag_key() {
        assert!(validate_flag_key("new_checkout").is_ok());
        assert!(validate_flag_key("dark-mode").is_ok());
        assert!(validate_flag_key("beta_features_2024").is_ok());

        assert!(validate_flag_key("").is_err());
        assert!(validate_flag_key("New_Checkout").is_err()); // uppercase start
        assert!(validate_flag_key("_invalid").is_err()); // starts with underscore
        assert!(validate_flag_key("has space").is_err());
        assert!(validate_flag_key("has.dot").is_err());
    }

    #[test]
    fn test_validate_rollout_percentage() {
        assert!(validate_rollout_percentage(0).is_ok());
        assert!(validate_rollout_percentage(50).is_ok());
        assert!(validate_rollout_percentage(100).is_ok());

        assert!(validate_rollout_percentage(-1).is_err());
        assert!(validate_rollout_percentage(101).is_err());
    }
}
