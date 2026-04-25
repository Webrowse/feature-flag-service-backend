use axum::{
    extract::{FromRequestParts, Request, State},
    http::request::Parts,
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use uuid::Uuid;

use crate::state::AppState;

pub struct JwtUser(pub Uuid);

impl<S> FromRequestParts<S> for JwtUser
where
    S: Send + Sync,
{
    type Rejection = (StatusCode, &'static str);

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<Uuid>()
            .copied()
            .map(JwtUser)
            .ok_or((StatusCode::UNAUTHORIZED, "missing user"))
    }
}

#[derive(Deserialize)]
struct Claims {
    sub: String,
    #[allow(dead_code)]
    exp: usize,
    #[allow(dead_code)]
    iat: usize,
}

pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request,
    next: Next,
) -> Result<Response, impl IntoResponse> {
    let auth_header = req
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok());

    let token = match auth_header {
        Some(h) if h.starts_with("Bearer ") => &h[7..],
        _ => return Err((StatusCode::UNAUTHORIZED, "missing token")),
    };

    let mut validation = Validation::new(Algorithm::HS256);
    validation.validate_exp = true;

    let token_data = match decode::<Claims>(
        token,
        &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
        &validation,
    ) {
        Ok(data) => data,
        Err(e) => {
            tracing::warn!("JWT validation failed: {}", e);
            return Err((StatusCode::UNAUTHORIZED, "invalid token"));
        }
    };

    match Uuid::parse_str(&token_data.claims.sub) {
        Ok(user_id) => {
            req.extensions_mut().insert(user_id);
            Ok(next.run(req).await)
        }
        Err(_) => Err((StatusCode::UNAUTHORIZED, "invalid token")),
    }
}
