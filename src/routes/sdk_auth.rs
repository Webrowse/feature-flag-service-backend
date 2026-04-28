use axum::{
    extract::{FromRequestParts, Request, State},
    http::request::Parts,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use uuid::Uuid;

use crate::state::AppState;

/// Extractor for SDK authentication; yields the authenticated project_id.
pub struct SdkProject(pub Uuid);

impl<S> FromRequestParts<S> for SdkProject
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Uuid>()
            .copied()
            .map(SdkProject)
            .ok_or((StatusCode::UNAUTHORIZED, "missing project"))
    }
}

pub async fn require_sdk_key(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, impl IntoResponse> {
    let sdk_key = req
        .headers()
        .get("x-sdk-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_owned());

    let sdk_key = match sdk_key {
        Some(k) => k,
        None => return Err((StatusCode::UNAUTHORIZED, "Missing X-SDK-Key header")),
    };

    // Reject obviously malformed keys before hitting the DB.
    if sdk_key.len() < 4 || sdk_key.len() > 128 || !sdk_key.starts_with("sdk_") {
        return Err((StatusCode::UNAUTHORIZED, "Invalid SDK key"));
    }

    let project =
        sqlx::query_as::<_, (uuid::Uuid,)>(r#"SELECT id FROM projects WHERE sdk_key = $1"#)
            .bind(&sdk_key)
            .fetch_optional(&state.db)
            .await
            .map_err(|e| {
                tracing::error!("DB error validating SDK key: {}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error")
            })?;

    match project {
        Some((project_id,)) => {
            req.extensions_mut().insert(project_id);
            Ok(next.run(req).await)
        }
        None => Err((StatusCode::UNAUTHORIZED, "Invalid SDK key")),
    }
}
