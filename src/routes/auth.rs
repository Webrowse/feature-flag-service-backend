use crate::state::AppState;
use argon2::password_hash::{PasswordHash, SaltString};
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::{
    extract::{Json, State},
    http::StatusCode,
    response::IntoResponse,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{Duration, Utc};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use lettre::{message::header::ContentType, AsyncTransport, Message};
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
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

#[derive(Deserialize)]
pub struct ForgotPasswordRequest {
    pub email: String,
}

#[derive(Serialize)]
pub struct ForgotPasswordResponse {
    pub message: String,
}

#[derive(Deserialize)]
pub struct ResetPasswordRequest {
    pub token: String,
    #[serde(rename = "newPassword")]
    pub new_password: String,
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

fn generate_reset_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn hash_reset_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    format!("{digest:x}")
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
        return (
            StatusCode::BAD_REQUEST,
            "Password must be at least 8 characters",
        )
            .into_response();
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

    let res = sqlx::query(r#"INSERT INTO users (id, email, password_hash) VALUES ($1, $2, $3)"#)
        .bind(user_id)
        .bind(&email)
        .bind(&password_hash)
        .execute(&state.db)
        .await;

    match res {
        Ok(_) => (
            StatusCode::CREATED,
            Json(RegisterResponse { id: user_id, email }),
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

pub async fn forgot_password(
    State(state): State<AppState>,
    Json(payload): Json<ForgotPasswordRequest>,
) -> impl IntoResponse {
    let email = payload.email.trim().to_string();
    let generic_response = Json(ForgotPasswordResponse {
        message: "If the account exists, a reset link has been sent.".to_string(),
    });

    if !is_valid_email(&email) {
        return (StatusCode::OK, generic_response).into_response();
    }

    let user_id = match sqlx::query_scalar::<_, Uuid>(r#"SELECT id FROM users WHERE email = $1"#)
        .bind(&email)
        .fetch_optional(&state.db)
        .await
    {
        Ok(id) => id,
        Err(e) => {
            tracing::error!("DB error during forgot password lookup: {}", e);
            return (StatusCode::OK, generic_response).into_response();
        }
    };

    if let Some(user_id) = user_id {
        let token = generate_reset_token();
        let token_hash = hash_reset_token(&token);
        let expires_at = Utc::now() + Duration::minutes(30);

        if let Err(e) = sqlx::query(
            r#"INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)"#,
        )
        .bind(Uuid::new_v4())
        .bind(user_id)
        .bind(&token_hash)
        .bind(expires_at)
        .execute(&state.db)
        .await
        {
            tracing::error!("Failed to store password reset token: {}", e);
            return (StatusCode::OK, generic_response).into_response();
        }

        tracing::info!("Password reset requested for user_id={}", user_id);

        if let Some(ref mailer) = state.mailer {
            let reset_url = format!("{}/reset-password?token={}", state.app_url, token);
            let body = format!(
                "You requested a password reset.\n\nReset your password here:\n\n{}\n\nThis link expires in 30 minutes.\n\nIf you did not request this, ignore this email.",
                reset_url
            );
            match (
                state.smtp_from.parse::<lettre::message::Mailbox>(),
                email.parse::<lettre::message::Mailbox>(),
            ) {
                (Ok(from), Ok(to)) => {
                    match Message::builder()
                        .from(from)
                        .to(to)
                        .subject("Password Reset Request")
                        .header(ContentType::TEXT_PLAIN)
                        .body(body)
                    {
                        Ok(msg) => {
                            if let Err(e) = mailer.send(msg).await {
                                tracing::error!("Failed to send password reset email to {}: {}", email, e);
                            }
                        }
                        Err(e) => tracing::error!("Failed to build reset email: {}", e),
                    }
                }
                _ => tracing::error!("Invalid mailbox address for reset email"),
            }
        } else {
            tracing::warn!("SMTP not configured; reset token for {} was not emailed", email);
        }
    }

    (StatusCode::OK, generic_response).into_response()
}

pub async fn reset_password(
    State(state): State<AppState>,
    Json(payload): Json<ResetPasswordRequest>,
) -> impl IntoResponse {
    if payload.new_password.len() < 8 {
        return (
            StatusCode::BAD_REQUEST,
            "Password must be at least 8 characters",
        )
            .into_response();
    }
    if payload.new_password.len() > MAX_PASSWORD_LEN {
        return (StatusCode::BAD_REQUEST, "Password too long").into_response();
    }

    let token_hash = hash_reset_token(payload.token.trim());
    let now = Utc::now();
    let row = sqlx::query_as::<_, (Uuid, chrono::DateTime<Utc>, bool)>(
        r#"SELECT user_id, expires_at, used FROM password_reset_tokens WHERE token_hash = $1"#,
    )
    .bind(&token_hash)
    .fetch_optional(&state.db)
    .await;

    let (user_id, expires_at, used) = match row {
        Ok(Some(v)) => v,
        Ok(None) => return (StatusCode::BAD_REQUEST, "Invalid or expired token").into_response(),
        Err(e) => {
            tracing::error!("DB error during reset password token lookup: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    if used || expires_at < now {
        return (StatusCode::BAD_REQUEST, "Invalid or expired token").into_response();
    }

    let salt = SaltString::generate(&mut OsRng);
    let argon = Argon2::default();
    let password_hash = match argon.hash_password(payload.new_password.as_bytes(), &salt) {
        Ok(h) => h.to_string(),
        Err(e) => {
            tracing::error!("Failed to hash reset password: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    let mut tx = match state.db.begin().await {
        Ok(tx) => tx,
        Err(e) => {
            tracing::error!("Failed to start transaction for password reset: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
        }
    };

    if let Err(e) = sqlx::query(r#"UPDATE users SET password_hash = $1 WHERE id = $2"#)
        .bind(&password_hash)
        .bind(user_id)
        .execute(&mut *tx)
        .await
    {
        tracing::error!("Failed to update user password: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
    }

    if let Err(e) =
        sqlx::query(r#"UPDATE password_reset_tokens SET used = true WHERE user_id = $1"#)
            .bind(user_id)
            .execute(&mut *tx)
            .await
    {
        tracing::error!("Failed to invalidate password reset tokens: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
    }

    if let Err(e) = tx.commit().await {
        tracing::error!("Failed to commit password reset transaction: {}", e);
        return (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error").into_response();
    }

    (StatusCode::OK, "Password reset successful").into_response()
}
