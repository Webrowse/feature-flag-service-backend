use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use super::{
    validate_environment_key, CreateEnvironmentRequest, Environment, EnvironmentResponse,
    UpdateEnvironmentRequest,
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
            format!("Name must be {MAX_NAME_LEN} characters or fewer"),
        ));
    }
    Ok(())
}

pub async fn create(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path(project_id): Path<Uuid>,
    Json(payload): Json<CreateEnvironmentRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_name(&payload.name)?;
    validate_environment_key(&payload.key).map_err(|e| (StatusCode::BAD_REQUEST, e))?;

    let project_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND created_by = $2)",
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check project: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    if !project_exists {
        return Err((StatusCode::NOT_FOUND, "Project not found".to_string()));
    }

    let environment = match sqlx::query_as::<_, Environment>(
        r#"
        INSERT INTO environments (project_id, name, key, description)
        VALUES ($1, $2, $3, $4)
        RETURNING id, project_id, name, key, description, created_at, updated_at
        "#,
    )
    .bind(project_id)
    .bind(payload.name.trim())
    .bind(&payload.key)
    .bind(&payload.description)
    .fetch_one(&state.db)
    .await
    {
        Ok(env) => env,
        Err(e) => {
            if let Some(db_error) = e.as_database_error() {
                if db_error.code() == Some(std::borrow::Cow::Borrowed("23505")) {
                    return Err((
                        StatusCode::CONFLICT,
                        "Environment key already exists".to_string(),
                    ));
                }
            }
            tracing::error!("Failed to create environment: {}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            ));
        }
    };

    Ok((
        StatusCode::CREATED,
        Json(EnvironmentResponse {
            id: environment.id,
            project_id: environment.project_id,
            name: environment.name,
            key: environment.key,
            description: environment.description,
            created_at: environment.created_at,
            updated_at: environment.updated_at,
        }),
    ))
}

pub async fn list(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let project_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM projects WHERE id = $1 AND created_by = $2)",
    )
    .bind(project_id)
    .bind(user_id)
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to check project: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    if !project_exists {
        return Err((StatusCode::NOT_FOUND, "Project not found".to_string()));
    }

    let environments = sqlx::query_as::<_, Environment>(
        r#"
        SELECT id, project_id, name, key, description, created_at, updated_at
        FROM environments
        WHERE project_id = $1
        ORDER BY created_at ASC
        "#,
    )
    .bind(project_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch environments: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    let response: Vec<EnvironmentResponse> = environments
        .into_iter()
        .map(|e| EnvironmentResponse {
            id: e.id,
            project_id: e.project_id,
            name: e.name,
            key: e.key,
            description: e.description,
            created_at: e.created_at,
            updated_at: e.updated_at,
        })
        .collect();

    Ok(Json(response))
}

pub async fn get(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let environment = sqlx::query_as::<_, Environment>(
        r#"
        SELECT e.id, e.project_id, e.name, e.key, e.description, e.created_at, e.updated_at
        FROM environments e
        JOIN projects p ON e.project_id = p.id
        WHERE e.id = $1 AND e.project_id = $2 AND p.created_by = $3
        "#,
    )
    .bind(environment_id)
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch environment: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    match environment {
        Some(e) => Ok(Json(EnvironmentResponse {
            id: e.id,
            project_id: e.project_id,
            name: e.name,
            key: e.key,
            description: e.description,
            created_at: e.created_at,
            updated_at: e.updated_at,
        })),
        None => Err((StatusCode::NOT_FOUND, "Environment not found".to_string())),
    }
}

pub async fn update(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id)): Path<(Uuid, Uuid)>,
    Json(payload): Json<UpdateEnvironmentRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if payload.name.is_none() && payload.description.is_none() {
        return Err((StatusCode::BAD_REQUEST, "No fields to update".to_string()));
    }

    if let Some(ref name) = payload.name {
        validate_name(name)?;
    }

    let exists = sqlx::query_scalar::<_, bool>(
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
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    if !exists {
        return Err((StatusCode::NOT_FOUND, "Environment not found".to_string()));
    }

    let environment = sqlx::query_as::<_, Environment>(
        r#"
        UPDATE environments
        SET
            name        = COALESCE($2, name),
            description = COALESCE($3, description),
            updated_at  = NOW()
        WHERE id = $1
        RETURNING id, project_id, name, key, description, created_at, updated_at
        "#,
    )
    .bind(environment_id)
    .bind(payload.name.as_deref().map(str::trim))
    .bind(payload.description.as_deref())
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update environment: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    Ok(Json(EnvironmentResponse {
        id: environment.id,
        project_id: environment.project_id,
        name: environment.name,
        key: environment.key,
        description: environment.description,
        created_at: environment.created_at,
        updated_at: environment.updated_at,
    }))
}

pub async fn delete(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path((project_id, environment_id)): Path<(Uuid, Uuid)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let result = sqlx::query(
        r#"
        DELETE FROM environments
        WHERE id = $1 AND project_id = $2
        AND EXISTS(SELECT 1 FROM projects WHERE id = $2 AND created_by = $3)
        "#,
    )
    .bind(environment_id)
    .bind(project_id)
    .bind(user_id)
    .execute(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to delete environment: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Environment not found".to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}
