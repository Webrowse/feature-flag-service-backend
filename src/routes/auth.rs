use crate::state::AppState;
use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MAX_PASSWORD_LEN: usize = 1024;

#[derive(Deserialize)]
pub struct RegistrationRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct RegisterResponse {
    pub id: Uuid,
    pub email: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub token: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    exp: usize,
    iat: usize,
}

fn is_valid_email(email: &str) -> bool {
    let email = email.trim();
    match email.find('@') {
        None => false,
        Some(at) => at > 0 && at < email.len() - 1 && email[at + 1..].contains('.'),
    }
}

pub async fn register(
    State(state): State<AppState>,
    Json(payload): Json<RegistrationRequest>,
) -> impl IntoResponse {
    let email = payload.email.trim().to_string();

    if !is_valid_email(&email) {
        return (StatusCode::BAD_REQUEST, "Invalid email address").into_response();
    }

    if payload.password.len() < 8 {
        return (StatusCode::BAD_REQUEST, "Password must be at least 8 characters").into_response();
    }

    if payload.password.len() > MAX_PASSWORD_LEN {
        return (StatusCode::BAD_REQUEST, "Password too long").into_response();
    }

    let salt = SaltString::generate(&mut OsRng);
    let argon = Argon2::default();

    let password_hash = match argon.hash_password(payload.password.as_bytes(), &salt) {
        Ok(h) => h.to_string(),
        Err(e) => {
            tracing::error!("Failed to hash password: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    let user_id = Uuid::new_v4();

    let res = sqlx::query(
        r#"INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3)"#,
    )
    .bind(user_id)
    .bind(&email)
    .bind(&password_hash)
    .execute(&state.db)
    .await;

    match res {
        Ok(_) => (
            StatusCode::CREATED,
            Json(RegisterResponse {
                id: user_id,
                email,
            }),
        )
            .into_response(),
        Err(e) => {
            if let Some(db_err) = e.as_database_error() {
                if db_err.code() == Some(std::borrow::Cow::Borrowed("23505")) {
                    return (StatusCode::CONFLICT, "Email already registered").into_response();
                }
            }
            tracing::error!("Failed to create user: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Could not create user").into_response()
        }
    }
}

pub async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> impl IntoResponse {
    if payload.password.len() > MAX_PASSWORD_LEN {
        return (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response();
    }

    let row = sqlx::query_as::<_, (uuid::Uuid, String)>(
        r#"SELECT id, password_hash FROM users WHERE email = $1"#,
    )
    .bind(&payload.email)
    .fetch_optional(&state.db)
    .await;

    let (db_user_id, db_password_hash) = match row {
        Ok(Some(r)) => r,
        Ok(None) => return (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response(),
        Err(e) => {
            tracing::error!("DB error during login: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    let parsed_hash = match PasswordHash::new(&db_password_hash) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("Corrupt password hash for user {}: {}", db_user_id, e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    if Argon2::default()
        .verify_password(payload.password.as_bytes(), &parsed_hash)
        .is_err()
    {
        return (StatusCode::UNAUTHORIZED, "Invalid credentials").into_response();
    }

    let now = Utc::now();
    let claims = Claims {
        sub: db_user_id.to_string(),
        exp: (now + Duration::hours(24)).timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    );

    match token {
        Ok(t) => (StatusCode::OK, Json(LoginResponse { token: t })).into_response(),
        Err(e) => {
            tracing::error!("JWT encode error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response()
        }
    }
}
