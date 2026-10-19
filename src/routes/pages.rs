use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, index_page, login, login_page, static_asset, terminal_page};

pub(super) fn protected() -> Router<AppState> {
    Router::new()
        .route("/", get(index_page))
        .route("/terminal", get(terminal_page))
        .route("/api/auth/password", post(login::update_password))
}

pub(super) fn public(state: AppState) -> Router {
    Router::new()
        .route("/login", get(login_page))
        .route("/api/auth/login", post(login::login))
        .route("/api/auth/cross-instance-ticket", get(login::cross_instance_ticket))
        .route("/api/auth/cross-instance-login", post(login::cross_instance_login))
        .route("/api/auth/logout", post(login::logout))
        .route("/api/auth/session", get(login::session_status))
        .route("/api/auth/proxy-check", get(login::proxy_auth_check))
        .route("/assets/{*asset_path}", get(static_asset))
        .with_state(state)
}
