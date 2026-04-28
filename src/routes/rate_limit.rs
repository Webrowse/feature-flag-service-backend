use axum::{
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::{IntoResponse, Response},
};
use governor::{clock::DefaultClock, state::keyed::DefaultKeyedStateStore, Quota, RateLimiter};
use std::num::NonZeroU32;
use std::sync::Arc;

pub type Limiter = Arc<RateLimiter<String, DefaultKeyedStateStore<String>, DefaultClock>>;

pub fn per_minute(n: u32) -> Limiter {
    Arc::new(RateLimiter::keyed(Quota::per_minute(
        NonZeroU32::new(n).expect("rate limit must be > 0"),
    )))
}

fn client_ip(req: &Request) -> String {
    req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .or_else(|| req.headers().get("x-real-ip").and_then(|v| v.to_str().ok()))
        .unwrap_or("unknown")
        .trim()
        .to_owned()
}

pub async fn by_ip(State(limiter): State<Limiter>, req: Request, next: Next) -> Response {
    let ip = client_ip(&req);
    match limiter.check_key(&ip) {
        Ok(_) => next.run(req).await,
        Err(_) => (StatusCode::TOO_MANY_REQUESTS, "Rate limit exceeded").into_response(),
    }
}
