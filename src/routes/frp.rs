use axum::{
    Router,
    routing::{delete, get, post},
};

use crate::{AppState, frpc};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/frpc", get(frpc::get_frpc_status).put(frpc::save_frpc_config))
        .route("/api/frpc/start", post(frpc::start_frpc))
        .route("/api/frpc/stop", post(frpc::stop_frpc))
        .route("/api/frpc/restart", post(frpc::restart_frpc))
        .route("/api/frpc/download", post(frpc::download_frpc_binary))
        .route("/api/frps", get(frpc::get_frps_status).put(frpc::save_frps_config))
        .route("/api/frps/start", post(frpc::start_frps))
        .route("/api/frps/stop", post(frpc::stop_frps))
        .route("/api/frps/restart", post(frpc::restart_frps))
        .route("/api/frps/download", post(frpc::download_frps_binary))
        .route("/api/frp/roles", get(frpc::list_frp_roles).post(frpc::save_frp_role))
        .route("/api/frp/roles/{id}", delete(frpc::delete_frp_role))
        .route("/api/frp/roles/{id}/unmanage", post(frpc::unmanage_frp_role))
        .route("/api/frp/roles/{id}/start", post(frpc::start_frp_role))
        .route("/api/frp/roles/{id}/stop", post(frpc::stop_frp_role))
        .route("/api/frp/roles/{id}/restart", post(frpc::restart_frp_role))
        .route("/api/frp/roles/{id}/download", post(frpc::download_frp_role_binary))
        .route("/api/frp/test-port", post(frpc::test_frp_port))
        .route("/api/frp/system", get(frpc::discover_system_frp))
        .route("/api/frp/system/adopt", post(frpc::adopt_system_frp))
}
