use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{any, delete, get, post, put},
};

use crate::{AppState, codex_proxy, proxy, quota, upstream_proxy};

const MAX_PROXY_REQUEST_BODY_BYTES: usize = 32 * 1024 * 1024;

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/proxy/presets",
            get(proxy::list_proxy_presets).post(proxy::create_proxy_preset),
        )
        .route("/api/proxy/presets/reorder", put(proxy::reorder_proxy_presets))
        .route(
            "/api/proxy/presets/{preset_id}",
            put(proxy::update_proxy_preset).delete(proxy::delete_proxy_preset),
        )
        .route("/api/proxy/test", post(proxy::test_proxy))
        .route("/api/proxy/active", get(proxy::get_active_proxy))
        .route("/api/proxy/active", put(proxy::apply_proxy))
        .route("/api/proxy/active", delete(proxy::clear_proxy))
        .route("/api/quota/config", get(quota::get_quota_config).put(quota::save_quota_config))
        .route("/api/quota/query", get(quota::query_quota))
}

// Only forwarding endpoints belong here. Preset/quota administration stays
// behind the management auth middleware, even when the gateway is enabled.
pub(super) fn proxy_routes() -> Router<AppState> {
    Router::new()
        // Codex compaction can exceed Axum's default 2 MiB JSON extractor limit. Match
        // `MAX_PROXY_BODY_BYTES` in the generic upstream proxy (32 MiB).
        .route("/api/codex-proxy/minimax/v1/responses", post(codex_proxy::minimax_responses))
        .route("/api/codex-proxy/zhipu/v1/responses", post(codex_proxy::zhipu_responses))
        .route("/api/codex-proxy/deepseek/v1/responses", post(codex_proxy::deepseek_responses))
        .route(
            "/api/codex-proxy/anthropic/v1/responses",
            post(codex_proxy::anthropic_responses),
        )
        .route("/api/upstream/openai/v1", any(upstream_proxy::openai_upstream_proxy))
        .route(
            "/api/upstream/openai/v1/{*proxy_path}",
            any(upstream_proxy::openai_upstream_proxy),
        )
        .route("/api/upstream/anthropic", any(upstream_proxy::anthropic_upstream_proxy))
        .route(
            "/api/upstream/anthropic/{*proxy_path}",
            any(upstream_proxy::anthropic_upstream_proxy),
        )
        .layer(DefaultBodyLimit::max(MAX_PROXY_REQUEST_BODY_BYTES))
        .layer(axum::middleware::from_fn(crate::auth_guard::strip_local_api_token))
}
