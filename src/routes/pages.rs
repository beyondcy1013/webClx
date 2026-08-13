use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, index_page, login, login_page, static_asset, terminal_page};

pub(super) fn protected() -> Router<AppState> {
    Router::new()
        .route("/", get(index_page))
        .route("/terminal", get(terminal_page))
}

pub(super) fn public(state: AppState) -> Router {
    Router::new()
        .route("/login", get(login_page))
        .route("/api/auth/login", post(login::login))
        .route("/api/auth/logout", post(login::logout))
        .route("/api/auth/session", get(login::session_status))
        .route("/assets/{*asset_path}", get(static_asset))
        .with_state(state)
}
