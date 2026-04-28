use axum::{
    middleware,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use uuid::Uuid;

mod auth;
pub mod environments;
mod flags;
mod health;
mod middleware_auth;
mod projects;
mod rate_limit;
mod rules;
mod sdk;
mod sdk_auth;

use crate::routes::auth::{login, register};
use crate::routes::middleware_auth::JwtUser;
use crate::state::AppState;

pub fn routes(state: AppState) -> Router {
    let projects_router = Router::new()
        .route(
            "/",
            post(projects::routes::create).get(projects::routes::list),
        )
        .route(
            "/{id}",
            get(projects::routes::get)
                .put(projects::routes::update)
                .delete(projects::routes::delete),
        )
        .route(
            "/{id}/regenerate-key",
            post(projects::routes::regenerate_key),
        );

    let rules_router = Router::new()
        .route("/", post(rules::routes::create).get(rules::routes::list))
        .route(
            "/{rule_id}",
            get(rules::routes::get)
                .put(rules::routes::update)
                .delete(rules::routes::delete),
        );

    let flags_router = Router::new()
        .route("/", post(flags::routes::create).get(flags::routes::list))
        .route(
            "/{flag_id}",
            get(flags::routes::get)
                .put(flags::routes::update)
                .delete(flags::routes::delete),
        )
        .route("/{flag_id}/toggle", post(flags::routes::toggle))
        .nest("/{flag_id}/rules", rules_router);

    let environments_router = Router::new()
        .route(
            "/",
            post(environments::routes::create).get(environments::routes::list),
        )
        .route(
            "/{environment_id}",
            get(environments::routes::get)
                .put(environments::routes::update)
                .delete(environments::routes::delete),
        );

    let auth_router = Router::new()
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
        .layer(middleware::from_fn_with_state(
            rate_limit::per_minute(10),
            rate_limit::by_ip,
        ));

    Router::new()
        .route("/", get(root))
        .route("/health", get(health::health))
        .merge(auth_router)
        .nest(
            "/api",
            Router::new()
                .route("/me", get(me_handler))
                .nest("/projects", projects_router)
                .nest("/projects/{project_id}/environments", environments_router)
                .nest(
                    "/projects/{project_id}/environments/{environment_id}/flags",
                    flags_router,
                )
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    middleware_auth::require_auth,
                )),
        )
        .nest(
            "/sdk/v1",
            Router::new()
                .route("/evaluate", post(sdk::routes::evaluate))
                // sdk_auth runs after the rate limit check
                .layer(middleware::from_fn_with_state(
                    state.clone(),
                    sdk_auth::require_sdk_key,
                ))
                .layer(middleware::from_fn_with_state(
                    rate_limit::per_minute(200),
                    rate_limit::by_ip,
                )),
        )
        .with_state(state)
}

async fn root() -> &'static str {
    "Feature Flag Service"
}

#[derive(Serialize)]
struct MeResponse {
    user_id: Uuid,
}

async fn me_handler(JwtUser(user_id): JwtUser) -> impl IntoResponse {
    Json(MeResponse { user_id })
}
