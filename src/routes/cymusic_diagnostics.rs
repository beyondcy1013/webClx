use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

use crate::{AppState, cymusic_diagnostics};

pub(super) fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/api/cymusic/diagnostics", post(cymusic_diagnostics::upload))
        .layer(DefaultBodyLimit::max(300 * 1024))
}

pub(super) fn protected_routes() -> Router<AppState> {
    Router::new().route("/api/cymusic/diagnostics", get(cymusic_diagnostics::list))
}
