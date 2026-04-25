use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use super::{
    validate_rule_type, validate_rule_value, CreateRuleRequest, FlagRule, RuleResponse,
    UpdateRuleRequest,
};
use crate::routes::middleware_auth::JwtUser;
use crate::state::AppState;

pub async fn create(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id, flag_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(payload): Json<CreateRuleRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_rule_type(&payload.rule_type).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    validate_rule_value(&payload.rule_type, &payload.rule_value)
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let flag_exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM feature_flags f
            JOIN environments e ON f.environment_id = e.id
            JOIN projects p ON e.project_id = p.id
            WHERE f.id = $1 AND f.environment_id = $2 AND e.project_id = $3 AND p.created_by = $4
        )
        "#,
    )
    .bind(flag_id)
    .bind(environment_id)
    .bind(project_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check flag: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    if !flag_exists {
        return Err((StatusCode::NOT_FOUND, "Flag not found".to_string()));
    }

    let rule = sqlx::query_as::<_, FlagRule>(
        r#"
        INSERT INTO flag_rules (flag_id, rule_type, rule_value, enabled, priority)
        VALUES ($1, $2, $3, $4, $5)
        RETURNING id, flag_id, rule_type, rule_value, enabled, priority, created_at
        "#,
    )
    .bind(flag_id)
    .bind(&payload.rule_type)
    .bind(&payload.rule_value)
    .bind(payload.enabled.unwrap_or(true))
    .bind(payload.priority.unwrap_or(0))
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create rule: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    Ok((StatusCode::CREATED, Json(rule_to_response(rule))))
}

pub async fn list(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id, flag_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let flag_exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM feature_flags f
            JOIN environments e ON f.environment_id = e.id
            JOIN projects p ON e.project_id = p.id
            WHERE f.id = $1 AND f.environment_id = $2 AND e.project_id = $3 AND p.created_by = $4
        )
        "#,
    )
    .bind(flag_id)
    .bind(environment_id)
    .bind(project_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check flag: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    if !flag_exists {
        return Err((StatusCode::NOT_FOUND, "Flag not found".to_string()));
    }

    let rules = sqlx::query_as::<_, FlagRule>(
        r#"
        SELECT id, flag_id, rule_type, rule_value, enabled, priority, created_at
        FROM flag_rules
        WHERE flag_id = $1
        ORDER BY priority DESC, created_at DESC
        "#,
    )
    .bind(flag_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch rules: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    Ok(Json(
        rules.into_iter().map(rule_to_response).collect::<Vec<_>>(),
    ))
}

pub async fn get(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id, flag_id, rule_id)): Path<(Uuid, Uuid, Uuid, Uuid)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let rule = sqlx::query_as::<_, FlagRule>(
        r#"
        SELECT r.id, r.flag_id, r.rule_type, r.rule_value, r.enabled, r.priority, r.created_at
        FROM flag_rules r
        JOIN feature_flags f ON r.flag_id = f.id
        JOIN environments e ON f.environment_id = e.id
        JOIN projects p ON e.project_id = p.id
        WHERE r.id = $1 AND r.flag_id = $2 AND f.environment_id = $3
          AND e.project_id = $4 AND p.created_by = $5
        "#,
    )
    .bind(rule_id)
    .bind(flag_id)
    .bind(environment_id)
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch rule: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    match rule {
        Some(r) => Ok(Json(rule_to_response(r))),
        None => Err((StatusCode::NOT_FOUND, "Rule not found".to_string())),
    }
}

pub async fn update(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id, flag_id, rule_id)): Path<(Uuid, Uuid, Uuid, Uuid)>,
    Json(payload): Json<UpdateRuleRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if payload.rule_value.is_none() && payload.enabled.is_none() && payload.priority.is_none() {
        return Err((StatusCode::BAD_REQUEST, "No fields to update".to_string()));
    }

    // Fetch the existing rule to validate the updated value against its type.
    let existing = sqlx::query_as::<_, FlagRule>(
        r#"
        SELECT r.id, r.flag_id, r.rule_type, r.rule_value, r.enabled, r.priority, r.created_at
        FROM flag_rules r
        JOIN feature_flags f ON r.flag_id = f.id
        JOIN environments e ON f.environment_id = e.id
        JOIN projects p ON e.project_id = p.id
        WHERE r.id = $1 AND r.flag_id = $2 AND f.environment_id = $3
          AND e.project_id = $4 AND p.created_by = $5
        "#,
    )
    .bind(rule_id)
    .bind(flag_id)
    .bind(environment_id)
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch rule for update: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    let existing = match existing {
        Some(r) => r,
        None => return Err((StatusCode::NOT_FOUND, "Rule not found".to_string())),
    };

    if let Some(ref value) = payload.rule_value {
        validate_rule_value(&existing.rule_type, value)
            .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    }

    let updated = sqlx::query_as::<_, FlagRule>(
        r#"
        UPDATE flag_rules
        SET
            rule_value = COALESCE($2, rule_value),
            enabled    = COALESCE($3, enabled),
            priority   = COALESCE($4, priority)
        WHERE id = $1
        RETURNING id, flag_id, rule_type, rule_value, enabled, priority, created_at
        "#,
    )
    .bind(rule_id)
    .bind(payload.rule_value.as_deref())
    .bind(payload.enabled)
    .bind(payload.priority)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update rule: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    Ok(Json(rule_to_response(updated)))
}

pub async fn delete(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id, flag_id, rule_id)): Path<(Uuid, Uuid, Uuid, Uuid)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let result = sqlx::query(
        r#"
        DELETE FROM flag_rules
        WHERE id = $1 AND flag_id = $2
          AND EXISTS(
              SELECT 1 FROM feature_flags f
              JOIN environments e ON f.environment_id = e.id
              JOIN projects p ON e.project_id = p.id
              WHERE f.id = $2 AND f.environment_id = $3
                AND e.project_id = $4 AND p.created_by = $5
          )
        "#,
    )
    .bind(rule_id)
    .bind(flag_id)
    .bind(environment_id)
    .bind(project_id)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to delete rule: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Rule not found".to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}

fn rule_to_response(r: FlagRule) -> RuleResponse {
    RuleResponse {
        id: r.id,
        flag_id: r.flag_id,
        rule_type: r.rule_type,
        rule_value: r.rule_value,
        enabled: r.enabled,
        priority: r.priority,
        created_at: r.created_at,
    }
}
