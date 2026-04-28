use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use std::collections::HashMap;
use uuid::Uuid;

use super::{EvaluateRequest, EvaluateResponse, FlagState};
use crate::evaluation::{evaluate_flag, FlagData, RuleData};
use crate::routes::sdk_auth::SdkProject;
use crate::state::AppState;

#[derive(Debug, sqlx::FromRow)]
struct EnvironmentRow {
    id: Uuid,
}

#[derive(Debug, sqlx::FromRow)]
struct FlagRow {
    id: Uuid,
    key: String,
    enabled: bool,
    rollout_percentage: i32,
}

#[derive(Debug, sqlx::FromRow)]
struct RuleRow {
    flag_id: Uuid,
    rule_type: String,
    rule_value: String,
    enabled: bool,
    priority: i32,
}

pub async fn evaluate(
    State(state): State<AppState>,
    SdkProject(project_id): SdkProject,
    Json(request): Json<EvaluateRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let environment_key = request.environment.trim().to_string();

    if environment_key.is_empty() || environment_key.len() > 64 {
        return Err((
            StatusCode::BAD_REQUEST,
            "Invalid environment key".to_string(),
        ));
    }

    let context = request.context;

    let environment: Option<EnvironmentRow> =
        sqlx::query_as(r#"SELECT id FROM environments WHERE project_id = $1 AND key = $2"#)
            .bind(project_id)
            .bind(&environment_key)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch environment: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            })?;

    let environment_id = match environment {
        Some(env) => env.id,
        None => {
            return Err((
                StatusCode::NOT_FOUND,
                format!("Environment '{}' not found", environment_key),
            ));
        }
    };

    let flags: Vec<FlagRow> = sqlx::query_as(
        r#"SELECT id, key, enabled, rollout_percentage FROM feature_flags WHERE environment_id = $1"#,
    )
    .bind(environment_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch flags: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    if flags.is_empty() {
        return Ok(Json(EvaluateResponse {
            flags: HashMap::new(),
        }));
    }

    let flag_ids: Vec<Uuid> = flags.iter().map(|f| f.id).collect();

    let rules: Vec<RuleRow> = sqlx::query_as(
        r#"
        SELECT flag_id, rule_type, rule_value, enabled, priority
        FROM flag_rules
        WHERE flag_id = ANY($1)
        ORDER BY priority DESC
        "#,
    )
    .bind(&flag_ids)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch rules: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    let mut rules_by_flag: HashMap<Uuid, Vec<RuleData>> = HashMap::new();
    for rule in rules {
        rules_by_flag
            .entry(rule.flag_id)
            .or_default()
            .push(RuleData {
                rule_type: rule.rule_type,
                rule_value: rule.rule_value,
                enabled: rule.enabled,
                priority: rule.priority,
            });
    }

    let user_identifier = context
        .user_id
        .as_deref()
        .or(context.user_email.as_deref())
        .unwrap_or("anonymous");

    let mut result_flags = HashMap::new();
    let mut eval_flag_ids: Vec<Uuid> = Vec::new();
    let mut eval_user_ids: Vec<String> = Vec::new();
    let mut eval_results: Vec<bool> = Vec::new();

    for flag in &flags {
        let flag_rules = rules_by_flag
            .get(&flag.id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        let evaluation = evaluate_flag(
            &FlagData {
                key: flag.key.clone(),
                enabled: flag.enabled,
                rollout_percentage: flag.rollout_percentage,
            },
            flag_rules,
            &context,
        );

        result_flags.insert(
            flag.key.clone(),
            FlagState {
                enabled: evaluation.enabled,
                reason: evaluation.reason,
            },
        );

        eval_flag_ids.push(flag.id);
        eval_user_ids.push(user_identifier.to_string());
        eval_results.push(evaluation.enabled);
    }

    // Fire-and-forget: log evaluations without blocking the response.
    let db = state.db.clone();
    tokio::spawn(async move {
        if let Err(e) = sqlx::query(
            r#"
            INSERT INTO flag_evaluations (flag_id, user_identifier, result)
            SELECT * FROM UNNEST($1::uuid[], $2::text[], $3::bool[])
            "#,
        )
        .bind(&eval_flag_ids)
        .bind(&eval_user_ids)
        .bind(&eval_results)
        .execute(&db)
        .await
        {
            tracing::error!("Failed to write evaluation logs: {}", e);
        }
    });

    Ok(Json(EvaluateResponse {
        flags: result_flags,
    }))
}
