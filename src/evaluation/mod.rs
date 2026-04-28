use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Clone)]
pub struct UserContext {
    pub user_id: Option<String>,
    pub user_email: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    pub custom_attributes: std::collections::HashMap<String, String>,
}

#[derive(Debug, Serialize)]
pub struct FlagEvaluation {
    pub enabled: bool,
    pub reason: String,
}

#[derive(Debug, Clone)]
pub struct FlagData {
    pub key: String,
    pub enabled: bool,
    pub rollout_percentage: i32,
}

#[derive(Debug, Clone)]
pub struct RuleData {
    pub rule_type: String,
    pub rule_value: String,
    pub enabled: bool,
    pub priority: i32,
}

pub fn evaluate_flag(flag: &FlagData, rules: &[RuleData], context: &UserContext) -> FlagEvaluation {
    if !flag.enabled {
        return FlagEvaluation {
            enabled: false,
            reason: "Flag is globally disabled".to_string(),
        };
    }

    // Sort rules by priority DESC so unit tests work without pre-sorted input.
    // (The SDK query also sorts, so this is cheap on already-sorted slices.)
    let mut sorted_rules = rules.to_vec();
    sorted_rules.sort_by(|a, b| b.priority.cmp(&a.priority));

    for rule in &sorted_rules {
        if !rule.enabled {
            continue;
        }

        let matched = match rule.rule_type.as_str() {
            "user_id" => context
                .user_id
                .as_deref()
                .map_or(false, |id| id == rule.rule_value),
            "user_email" => context
                .user_email
                .as_deref()
                .map_or(false, |email| email == rule.rule_value),
            "email_domain" => context.user_email.as_deref().map_or(false, |email| {
                // rule_value starts with '@' (enforced by validation + DB CHECK).
                email.ends_with(rule.rule_value.as_str())
            }),
            _ => false,
        };

        if matched {
            return FlagEvaluation {
                enabled: true,
                // Never echo back rule_value — it would disclose targeting lists to SDK callers.
                reason: format!("Matched {} targeting rule", rule.rule_type),
            };
        }
    }

    if flag.rollout_percentage > 0 {
        let user_identifier = context
            .user_id
            .as_deref()
            .or(context.user_email.as_deref())
            .unwrap_or("anonymous");

        return if should_enable_for_percentage(&flag.key, user_identifier, flag.rollout_percentage)
        {
            FlagEvaluation {
                enabled: true,
                reason: format!("User in {}% rollout", flag.rollout_percentage),
            }
        } else {
            FlagEvaluation {
                enabled: false,
                reason: format!("User not in {}% rollout", flag.rollout_percentage),
            }
        };
    }

    FlagEvaluation {
        enabled: true,
        reason: "Flag enabled globally".to_string(),
    }
}

/// FNV-1a hash — deterministic and stable across Rust versions and process restarts,
/// unlike `DefaultHasher` which must never be used for business-logic bucketing.
fn fnv1a(s: &str) -> u64 {
    const OFFSET: u64 = 14695981039346656037;
    const PRIME: u64 = 1099511628211;
    s.bytes().fold(OFFSET, |hash, byte| {
        hash.wrapping_mul(PRIME) ^ (byte as u64)
    })
}

fn should_enable_for_percentage(flag_key: &str, user_identifier: &str, percentage: i32) -> bool {
    if percentage <= 0 {
        return false;
    }
    if percentage >= 100 {
        return true;
    }
    let key = format!("{}:{}", flag_key, user_identifier);
    let bucket = (fnv1a(&key) % 100) as i32;
    bucket < percentage
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(user_id: Option<&str>, email: Option<&str>) -> UserContext {
        UserContext {
            user_id: user_id.map(str::to_string),
            user_email: email.map(str::to_string),
            custom_attributes: Default::default(),
        }
    }

    fn enabled_flag(key: &str, rollout: i32) -> FlagData {
        FlagData {
            key: key.to_string(),
            enabled: true,
            rollout_percentage: rollout,
        }
    }

    #[test]
    fn test_globally_disabled_flag() {
        let flag = FlagData {
            key: "test".to_string(),
            enabled: false,
            rollout_percentage: 100,
        };
        let result = evaluate_flag(&flag, &[], &ctx(Some("u1"), None));
        assert!(!result.enabled);
        assert!(result.reason.contains("globally disabled"));
    }

    #[test]
    fn test_user_id_rule_match() {
        let rules = vec![RuleData {
            rule_type: "user_id".to_string(),
            rule_value: "user123".to_string(),
            enabled: true,
            priority: 10,
        }];
        let result = evaluate_flag(&enabled_flag("f", 0), &rules, &ctx(Some("user123"), None));
        assert!(result.enabled);
        assert!(result.reason.contains("user_id"));
        // Must not leak the value itself.
        assert!(!result.reason.contains("user123"));
    }

    #[test]
    fn test_user_email_rule_match() {
        let rules = vec![RuleData {
            rule_type: "user_email".to_string(),
            rule_value: "alice@example.com".to_string(),
            enabled: true,
            priority: 5,
        }];
        let result = evaluate_flag(
            &enabled_flag("f", 0),
            &rules,
            &ctx(None, Some("alice@example.com")),
        );
        assert!(result.enabled);
        assert!(result.reason.contains("user_email"));
        assert!(!result.reason.contains("alice@example.com"));
    }

    #[test]
    fn test_email_domain_match() {
        let rules = vec![RuleData {
            rule_type: "email_domain".to_string(),
            rule_value: "@company.com".to_string(),
            enabled: true,
            priority: 5,
        }];
        let result = evaluate_flag(
            &enabled_flag("f", 0),
            &rules,
            &ctx(None, Some("john@company.com")),
        );
        assert!(result.enabled);
        assert!(!result.reason.contains("@company.com"));
    }

    #[test]
    fn test_email_domain_no_false_positive() {
        let rules = vec![RuleData {
            rule_type: "email_domain".to_string(),
            rule_value: "@company.com".to_string(),
            enabled: true,
            priority: 5,
        }];
        // "attacker@notcompany.com" must NOT match the "@company.com" domain rule.
        // The flag is globally enabled, so it's still true — but the reason must not
        // say "email_domain" (that would mean the rule incorrectly matched).
        let result = evaluate_flag(
            &enabled_flag("f", 0),
            &rules,
            &ctx(None, Some("attacker@notcompany.com")),
        );
        assert!(result.enabled); // flag is globally enabled regardless
        assert!(!result.reason.contains("email_domain")); // rule must NOT have fired
    }

    #[test]
    fn test_disabled_rule_skipped() {
        let rules = vec![RuleData {
            rule_type: "user_id".to_string(),
            rule_value: "user123".to_string(),
            enabled: false, // disabled — must be skipped
            priority: 10,
        }];
        let result = evaluate_flag(&enabled_flag("f", 0), &rules, &ctx(Some("user123"), None));
        // Falls through to globally-enabled default; must not claim the rule matched.
        assert!(result.enabled);
        assert!(!result.reason.contains("user_id"));
        assert!(result.reason.contains("globally"));
    }

    #[test]
    fn test_consistent_hashing_stable() {
        // Same inputs must always yield the same result.
        let r1 = should_enable_for_percentage("flag", "user123", 50);
        let r2 = should_enable_for_percentage("flag", "user123", 50);
        assert_eq!(r1, r2);

        assert!(!should_enable_for_percentage("flag", "user123", 0));
        assert!(should_enable_for_percentage("flag", "user123", 100));
    }

    #[test]
    fn test_anonymous_user_percentage() {
        // Anonymous users (no id/email) get a consistent bucket.
        let r1 = should_enable_for_percentage("flag", "anonymous", 50);
        let r2 = should_enable_for_percentage("flag", "anonymous", 50);
        assert_eq!(r1, r2);
    }

    #[test]
    fn test_rule_priority_order() {
        let rules = vec![
            RuleData {
                rule_type: "email_domain".to_string(),
                rule_value: "@company.com".to_string(),
                enabled: true,
                priority: 5,
            },
            RuleData {
                rule_type: "user_id".to_string(),
                rule_value: "user123".to_string(),
                enabled: true,
                priority: 10,
            },
        ];
        // user_id rule has higher priority and matches; that reason should be returned.
        let result = evaluate_flag(
            &enabled_flag("f", 0),
            &rules,
            &ctx(Some("user123"), Some("john@company.com")),
        );
        assert!(result.enabled);
        assert!(result.reason.contains("user_id"));
    }
}
