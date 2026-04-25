pub mod routes;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct FlagRule {
    pub id: Uuid,
    pub flag_id: Uuid,
    pub rule_type: String,
    pub rule_value: String,
    pub enabled: bool,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateRuleRequest {
    pub rule_type: String,
    pub rule_value: String,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateRuleRequest {
    pub rule_value: Option<String>,
    pub enabled: Option<bool>,
    pub priority: Option<i32>,
}

#[derive(Debug, Serialize)]
pub struct RuleResponse {
    pub id: Uuid,
    pub flag_id: Uuid,
    pub rule_type: String,
    pub rule_value: String,
    pub enabled: bool,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
}

pub fn validate_rule_type(rule_type: &str) -> Result<(), String> {
    match rule_type {
        "user_id" | "user_email" | "email_domain" => Ok(()),
        _ => Err(format!(
            "Invalid rule type '{}'. Must be one of: user_id, user_email, email_domain",
            rule_type
        )),
    }
}

pub fn validate_rule_value(rule_type: &str, rule_value: &str) -> Result<(), String> {
    if rule_value.trim().is_empty() {
        return Err("Rule value cannot be empty".to_string());
    }
    match rule_type {
        "email_domain" => {
            if !rule_value.starts_with('@') {
                return Err("Email domain must start with '@' (e.g., @company.com)".to_string());
            }
            if rule_value.len() < 3 {
                return Err("Email domain too short".to_string());
            }
        }
        "user_email" => {
            if !rule_value.contains('@') {
                return Err("Invalid email format".to_string());
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_rule_type() {
        assert!(validate_rule_type("user_id").is_ok());
        assert!(validate_rule_type("user_email").is_ok());
        assert!(validate_rule_type("email_domain").is_ok());
        assert!(validate_rule_type("invalid").is_err());
    }

    #[test]
    fn test_validate_rule_value() {
        assert!(validate_rule_value("email_domain", "@company.com").is_ok());
        assert!(validate_rule_value("email_domain", "company.com").is_err());
        assert!(validate_rule_value("email_domain", "@c").is_err());

        assert!(validate_rule_value("user_email", "user@example.com").is_ok());
        assert!(validate_rule_value("user_email", "invalid").is_err());

        assert!(validate_rule_value("user_id", "user_123").is_ok());
        assert!(validate_rule_value("user_id", "").is_err());
    }
}
