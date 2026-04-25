use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    db: &'static str,
}

pub async fn health(State(state): State<AppState>) -> impl IntoResponse {
    match sqlx::query("SELECT 1").execute(&state.db).await {
        Ok(_) => Json(HealthResponse {
            status: "ok",
            db: "ok",
        })
        .into_response(),
        Err(e) => {
            tracing::error!("Health check DB ping failed: {}", e);
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(HealthResponse {
                    status: "degraded",
                    db: "error",
                }),
            )
                .into_response()
        }
    }
}
