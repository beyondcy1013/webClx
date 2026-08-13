use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, api_catalog, artifacts};

pub(super) fn public_routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/artifacts/update/android/{project}",
            get(artifacts::android_update_manifest),
        )
        .route(
            "/api/artifacts/download/{artifact_id}/{file_name}",
            get(artifacts::download_artifact),
        )
}

pub(super) fn protected_routes() -> Router<AppState> {
    Router::new()
        .route("/downloads", get(artifacts::downloads_page))
        .route("/api/codex_apis", get(api_catalog::list_api_catalog))
        .route("/api/artifacts", get(artifacts::list_artifacts))
        .route("/api/artifacts/publish", post(artifacts::publish_artifact))
}
