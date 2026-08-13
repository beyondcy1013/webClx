use axum::{Router, routing::post};

use crate::{AppState, compile_service, deploy_service};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/build/compile", post(compile_service::request_compile))
        .route("/api/build/deploy", post(compile_service::request_deploy))
        .route("/api/build/compile/complete", post(compile_service::complete_compile_request))
        .route("/api/build/compile/status", axum::routing::get(compile_service::compile_status))
        .route("/api/build/compile/notify", post(compile_service::notify_compile_terminal))
        .route("/api/service/deploy", post(deploy_service::deploy_service))
}
