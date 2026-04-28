use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use super::{
    generate_sdk_key, CreateProjectRequest, Project, ProjectResponse, UpdateProjectRequest,
};
use crate::routes::middleware_auth::JwtUser;
use crate::state::AppState;

const DEFAULT_ENVIRONMENTS: &[(&str, &str)] =
    &[("production", "Production"), ("staging", "Staging")];

const MAX_NAME_LEN: usize = 255;

fn validate_name(name: &str) -> Result<(), (StatusCode, String)> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "Name cannot be empty".to_string()));
    }
    if trimmed.len() > MAX_NAME_LEN {
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
    Json(payload): Json<CreateProjectRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    validate_name(&payload.name)?;

    let sdk_key = generate_sdk_key();

    let mut tx = state.db.begin().await.map_err(|e| {
        tracing::error!("Failed to start transaction: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    let project = sqlx::query_as::<_, Project>(
        r#"
        INSERT INTO projects (name, description, sdk_key, created_by)
        VALUES ($1, $2, $3, $4)
        RETURNING *
        "#,
    )
    .bind(payload.name.trim())
    .bind(&payload.description)
    .bind(&sdk_key)
    .bind(user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("Failed to create project: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    for (env_key, env_name) in DEFAULT_ENVIRONMENTS {
        sqlx::query(
            r#"
            INSERT INTO environments (project_id, name, key, description)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(project.id)
        .bind(*env_name)
        .bind(*env_key)
        .bind(format!("{env_name} environment"))
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!("Failed to create default environment '{}': {}", env_key, e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            )
        })?;
    }

    tx.commit().await.map_err(|e| {
        tracing::error!("Failed to commit transaction: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    Ok((
        StatusCode::CREATED,
        Json(ProjectResponse {
            id: project.id,
            name: project.name,
            description: project.description,
            sdk_key: project.sdk_key,
            created_at: project.created_at,
            updated_at: project.updated_at,
        }),
    ))
}

pub async fn list(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let projects = sqlx::query_as::<_, Project>(
        r#"SELECT * FROM projects WHERE created_by = $1 ORDER BY created_at DESC"#,
    )
    .bind(user_id)
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to fetch projects: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    let response: Vec<ProjectResponse> = projects
        .into_iter()
        .map(|p| ProjectResponse {
            id: p.id,
            name: p.name,
            description: p.description,
            sdk_key: p.sdk_key,
            created_at: p.created_at,
            updated_at: p.updated_at,
        })
        .collect();

    Ok(Json(response))
}

pub async fn get(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let project =
        sqlx::query_as::<_, Project>(r#"SELECT * FROM projects WHERE id = $1 AND created_by = $2"#)
            .bind(project_id)
            .bind(user_id)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                tracing::error!("Failed to fetch project: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "Database error".to_string(),
                )
            })?;

    match project {
        Some(p) => Ok(Json(ProjectResponse {
            id: p.id,
            name: p.name,
            description: p.description,
            sdk_key: p.sdk_key,
            created_at: p.created_at,
            updated_at: p.updated_at,
        })),
        None => Err((StatusCode::NOT_FOUND, "Project not found".to_string())),
    }
}

pub async fn update(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path(project_id): Path<Uuid>,
    Json(payload): Json<UpdateProjectRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    if payload.name.is_none() && payload.description.is_none() {
        return Err((StatusCode::BAD_REQUEST, "No fields to update".to_string()));
    }

    if let Some(ref name) = payload.name {
        validate_name(name)?;
    }

    let project = sqlx::query_as::<_, Project>(
        r#"
        UPDATE projects
        SET
            name        = COALESCE($2, name),
            description = COALESCE($3, description),
            updated_at  = NOW()
        WHERE id = $1 AND created_by = $4
        RETURNING *
        "#,
    )
    .bind(project_id)
    .bind(payload.name.as_deref().map(str::trim))
    .bind(payload.description.as_deref())
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to update project: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    match project {
        Some(p) => Ok(Json(ProjectResponse {
            id: p.id,
            name: p.name,
            description: p.description,
            sdk_key: p.sdk_key,
            created_at: p.created_at,
            updated_at: p.updated_at,
        })),
        None => Err((StatusCode::NOT_FOUND, "Project not found".to_string())),
    }
}

pub async fn delete(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let result = sqlx::query(r#"DELETE FROM projects WHERE id = $1 AND created_by = $2"#)
        .bind(project_id)
        .bind(user_id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!("Failed to delete project: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Database error".to_string(),
            )
        })?;

    if result.rows_affected() == 0 {
        return Err((StatusCode::NOT_FOUND, "Project not found".to_string()));
    }

    Ok(StatusCode::NO_CONTENT)
}

pub async fn regenerate_key(
    State(state): State<AppState>,
    JwtUser(user_id): JwtUser,
    Path(project_id): Path<Uuid>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let new_sdk_key = generate_sdk_key();

    let project = sqlx::query_as::<_, Project>(
        r#"
        UPDATE projects
        SET sdk_key = $1, updated_at = NOW()
        WHERE id = $2 AND created_by = $3
        RETURNING *
        "#,
    )
    .bind(&new_sdk_key)
    .bind(project_id)
    .bind(user_id)
    .fetch_optional(&state.db)
    .await
    .map_err(|e| {
        tracing::error!("Failed to regenerate SDK key: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Database error".to_string(),
        )
    })?;

    match project {
        Some(p) => Ok(Json(ProjectResponse {
            id: p.id,
            name: p.name,
            description: p.description,
            sdk_key: p.sdk_key,
            created_at: p.created_at,
            updated_at: p.updated_at,
        })),
        None => Err((StatusCode::NOT_FOUND, "Project not found".to_string())),
    }
}
