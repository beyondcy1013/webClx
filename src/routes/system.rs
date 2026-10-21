use axum::{
    Router,
    routing::{delete, get, post, put},
};

use crate::{AppState, system};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/system/logs", get(system::get_system_logs))
        .route("/api/system/proxy", get(system::get_system_proxy))
        .route("/api/system/proxy", put(system::save_system_proxy))
        .route("/api/system/proxy", delete(system::clear_system_proxy))
        .route("/api/system/restart", post(system::restart_service))
        .route("/api/system/save-and-poweroff", post(system::save_and_poweroff))
        .route("/api/system/save-and-restart", post(system::save_and_restart_service))
        .route("/api/system/info", get(system::get_system_info))
        .route("/api/update/check", get(system::get_update_check))
        .route("/api/update/download", get(system::get_update_binary))
}
