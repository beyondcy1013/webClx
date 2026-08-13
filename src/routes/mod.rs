mod agent;
mod artifacts;
mod auth;
mod codex_task;
mod frp;
mod gateway;
mod operations;
mod pages;
mod system;
mod terminal;
mod workspace;

use axum::{Router, http, routing::get};
use tower_http::compression::CompressionLayer;

use crate::{AppState, auth_guard, spa_fallback};

pub(super) fn app(state: AppState) -> Router {
    let protected = Router::new()
        .merge(pages::protected())
        .merge(artifacts::protected_routes())
        .merge(workspace::routes())
        .merge(operations::routes())
        .merge(gateway::routes())
        .merge(frp::routes())
        .merge(system::routes())
        .merge(auth::routes())
        .merge(codex_task::routes())
        .merge(terminal::routes())
        .merge(agent::routes())
        // SPA path routing: any non-API/asset GET that matches no explicit route
        // serves the app shell so deep links like /settings/agent survive refresh.
        .fallback(get(spa_fallback))
        .with_state(state.clone())
        .layer(axum::middleware::from_fn_with_state(state.clone(), auth_guard::require_auth));

    pages::public(state.clone())
        .merge(artifacts::public_routes().with_state(state))
        .merge(protected)
        // Compress text/JSON APIs as usual, but skip file downloads: a compressed
        // response drops its Content-Length header, so browsers cannot show download
        // progress or estimate the remaining size. APK/exe/zip are already dense and
        // gain nothing from gzip anyway.
        .layer(CompressionLayer::new().compress_when(skip_file_downloads))
}

/// Returns `false` (don't compress) for binary file downloads so their
/// `Content-Length` header is preserved. Detects downloads via a
/// `Content-Disposition: attachment` header or a known binary content type.
fn skip_file_downloads(
    _status: http::StatusCode,
    _version: http::Version,
    headers: &http::HeaderMap,
    _extensions: &http::Extensions,
) -> bool {
    if let Some(disposition) = headers.get(http::header::CONTENT_DISPOSITION) {
        if disposition
            .to_str()
            .map(|value| value.to_ascii_lowercase().contains("attachment"))
            .unwrap_or(false)
        {
            return false;
        }
    }
    let content_type = headers
        .get(http::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    !matches!(
        content_type,
        "application/vnd.android.package-archive"
            | "application/zip"
            | "application/gzip"
            | "application/octet-stream"
    )
}
