use sqlx::PgPool;

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub jwt_secret: String,
    pub mailer: Option<lettre::AsyncSmtpTransport<lettre::Tokio1Executor>>,
    pub smtp_from: String,
    pub app_url: String,
}
