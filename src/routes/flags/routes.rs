use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use super::{
    validate_flag_key, validate_rollout_percentage, CreateFlagRequest, FeatureFlag, FlagResponse,
    UpdateFlagRequest,
};
use crate::routes::middleware_auth::JwtUser;
use crate::state::AppState;

const MAX_NAME_LEN: usize = 255;

fn validate_name(name: &str) -> Result<(), (StatusCode, String)> {
    let t = name.trim();
    if t.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Name cannot be empty".to_string()));
    }
    if t.len() > MAX_NAME_LEN {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Name must be {} characters or fewer", MAX_NAME_LEN),
        ));
    }
    Ok(())
}

pub async fn create(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<CreateFlagRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_name(&payload.name)?;
    validate_flag_key(&payload.key).map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    if let Some(percentage) = payload.rollout_percentage {
        validate_rollout_percentage(percentage).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    }

    let environment_exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM environments e
            JOIN projects p ON e.project_id = p.id
            WHERE e.id = $1 AND e.project_id = $2 AND p.created_by = $3
        )
        "#,
    )
    .bind(environment_id)
    .bind(project_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check environment: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    if !environment_exists {
        return Err((StatusCode::NOT_FOUND, "Environment not found".to_string()));
    }

    let flag = match sqlx::query_as::<_, FeatureFlag>(
        r#"
        INSERT INTO feature_flags
            (project_id, environment_id, name, key, description, enabled, rollout_percentage)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id, project_id, environment_id, name, key, description, enabled,
                  rollout_percentage, created_at, updated_at
        "#,
    )
    .bind(project_id)
    .bind(environment_id)
    .bind(payload.name.trim())
    .bind(&payload.key)
    .bind(&payload.description)
    .bind(payload.enabled.unwrap_or(false))
    .bind(payload.rollout_percentage.unwrap_or(0))
    .fetch_one(&state.db)
    .await
    {
        Ok(f) => f,
        Err(e) => {
            if let Some(db_error) = e.as_database_error() {
                if db_error.code() == Some(std::borrow::Cow::Borrowed("23505")) {
                    return Err((
                        StatusCode::CONFLICT,
                        "Flag key already exists in this environment".to_string(),
                    ));
                }
            }
            tracing::error!("Failed to create flag: {}", e);
            return Err((StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string()));
        }
    };

    Ok((StatusCode::CREATED, Json(flag_to_response(flag))))
}

pub async fn list(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let environment_exists = sqlx::query_scalar::<_, bool>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM environments e
            JOIN projects p ON e.project_id = p.id
            WHERE e.id = $1 AND e.project_id = $2 AND p.created_by = $3
        )
        "#,
    )
    .bind(environment_id)
    .bind(project_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check environment: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    if !environment_exists {
        return Err((StatusCode::NOT_FOUND, "Environment not found".to_string()));
    }

    let flags = sqlx::query_as::<_, FeatureFlag>(
        r#"
        SELECT id, project_id, environment_id, name, key, description, enabled,
               rollout_percentage, created_at, updated_at
        FROM feature_flags
        WHERE environment_id = $1
        ORDER BY created_at DESC
        "#,
    )
    .bind(environment_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch flags: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    Ok(Json(
        flags.into_iter().map(flag_to_response).collect::<Vec<_>>(),
    ))
}

pub async fn get(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id, flag_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let flag = sqlx::query_as::<_, FeatureFlag>(
        r#"
        SELECT f.id, f.project_id, f.environment_id, f.name, f.key, f.description,
               f.enabled, f.rollout_percentage, f.created_at, f.updated_at
        FROM feature_flags f
        JOIN environments e ON f.environment_id = e.id
        JOIN projects p ON e.project_id = p.id
        WHERE f.id = $1 AND f.environment_id = $2 AND e.project_id = $3 AND p.created_by = $4
        "#,
    )
    .bind(flag_id)
    .bind(environment_id)
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch flag: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    match flag {
        Some(f) => Ok(Json(flag_to_response(f))),
        None => Err((StatusCode::NOT_FOUND, "Flag not found".to_string())),
    }
}

pub async fn update(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id, flag_id)): Path<(Uuid, Uuid, Uuid)>,
    Json(payload): Json<UpdateFlagRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if payload.name.is_none()
        && payload.description.is_none()
        && payload.enabled.is_none()
        && payload.rollout_percentage.is_none()
    {
        return Err((StatusCode::BAD_REQUEST, "No fields to update".to_string()));
    }

    if let Some(ref name) = payload.name {
        validate_name(name)?;
    }
    if let Some(percentage) = payload.rollout_percentage {
        validate_rollout_percentage(percentage).map_err(|e| (StatusCode::BAD_REQUEST, e))?;
    }

    let exists = sqlx::query_scalar::<_, bool>(
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

    if !exists {
        return Err((StatusCode::NOT_FOUND, "Flag not found".to_string()));
    }

    let flag = sqlx::query_as::<_, FeatureFlag>(
        r#"
        UPDATE feature_flags
        SET
            name               = COALESCE($2, name),
            description        = COALESCE($3, description),
            enabled            = COALESCE($4, enabled),
            rollout_percentage = COALESCE($5, rollout_percentage),
            updated_at         = NOW()
        WHERE id = $1
        RETURNING id, project_id, environment_id, name, key, description, enabled,
                  rollout_percentage, created_at, updated_at
        "#,
    )
    .bind(flag_id)
    .bind(payload.name.as_deref().map(str::trim))
    .bind(payload.description.as_deref())
    .bind(payload.enabled)
    .bind(payload.rollout_percentage)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update flag: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    Ok(Json(flag_to_response(flag)))
}

pub async fn delete(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id, flag_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let result = sqlx::query(
        r#"
        DELETE FROM feature_flags f
        USING environments e, projects p
        WHERE f.id = $1 AND f.environment_id = $2
          AND e.id = f.environment_id AND e.project_id = $3
          AND p.id = e.project_id AND p.created_by = $4
        "#,
    )
    .bind(flag_id)
    .bind(environment_id)
    .bind(project_id)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to delete flag: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Flag not found".to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn toggle(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id, flag_id)): Path<(Uuid, Uuid, Uuid)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let flag = sqlx::query_as::<_, FeatureFlag>(
        r#"
        UPDATE feature_flags f
        SET enabled = NOT f.enabled, updated_at = NOW()
        FROM environments e, projects p
        WHERE f.id = $1 AND f.environment_id = $2
          AND e.id = f.environment_id AND e.project_id = $3
          AND p.id = e.project_id AND p.created_by = $4
        RETURNING f.id, f.project_id, f.environment_id, f.name, f.key, f.description,
                  f.enabled, f.rollout_percentage, f.created_at, f.updated_at
        "#,
    )
    .bind(flag_id)
    .bind(environment_id)
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to toggle flag: {}", e);
        (StatusCode::INTERNAL_SERVER_ERROR, "Database error".to_string())
    })?;

    match flag {
        Some(f) => Ok(Json(flag_to_response(f))),
        None => Err((StatusCode::NOT_FOUND, "Flag not found".to_string())),
    }
}

fn flag_to_response(f: FeatureFlag) -> FlagResponse {
    FlagResponse {
        id: f.id,
        project_id: f.project_id,
        environment_id: f.environment_id,
        name: f.name,
        key: f.key,
        description: f.description,
        enabled: f.enabled,
        rollout_percentage: f.rollout_percentage,
        created_at: f.created_at,
        updated_at: f.updated_at,
    }
}
