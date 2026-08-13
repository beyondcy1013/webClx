use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post, put},
};

use crate::{AppState, auth};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/auth/presets", get(auth::list_auth_presets).post(auth::save_auth_preset))
        .route("/api/auth/presets/reorder", put(auth::reorder_auth_presets))
        .route("/api/auth/presets/test-all", post(auth::test_all_auth_presets))
        .route("/api/auth/api-presets", get(auth::list_api_presets).post(auth::save_api_preset))
        .route("/api/auth/api-presets/import", post(auth::import_api_accounts))
        .route(
            "/api/auth/api-presets/import-file",
            post(auth::import_api_accounts_file).layer(DefaultBodyLimit::max(
                auth::API_ACCOUNT_IMPORT_MAX_UPLOAD_BYTES + 1024 * 1024,
            )),
        )
        .route("/api/auth/api-presets/reorder", put(auth::reorder_api_presets))
        .route("/api/auth/api-presets/test-all", post(auth::test_all_api_presets))
        .route(
            "/api/auth/claude-presets",
            get(auth::list_claude_presets).post(auth::save_claude_preset),
        )
        .route("/api/auth/claude-presets/reorder", put(auth::reorder_claude_presets))
        .route("/api/auth/claude-presets/test-all", post(auth::test_all_claude_presets))
        .route("/api/auth/oauth/codex/start", post(auth::start_codex_oauth_session))
        .route(
            "/api/auth/oauth/codex/sessions/{session_id}",
            get(auth::get_codex_oauth_session),
        )
        .route("/api/auth/current", put(auth::apply_current_auth))
        .route("/api/auth/preset-run-leases", post(auth::acquire_preset_run_lease))
        .route(
            "/api/auth/preset-run-leases/{lease_id}/heartbeat",
            put(auth::heartbeat_preset_run_lease),
        )
        .route(
            "/api/auth/preset-run-leases/{lease_id}",
            axum::routing::delete(auth::release_preset_run_lease),
        )
        .route(
            "/api/auth/presets/{preset_id}",
            put(auth::update_auth_preset).delete(auth::delete_auth_preset),
        )
        .route(
            "/api/auth/api-presets/{preset_id}",
            put(auth::update_api_preset).delete(auth::delete_api_preset),
        )
        .route(
            "/api/auth/claude-presets/{preset_id}",
            put(auth::update_claude_preset).delete(auth::delete_claude_preset),
        )
        .route("/api/auth/presets/{preset_id}/apply", put(auth::apply_auth_preset))
        .route("/api/auth/presets/{preset_id}/test", post(auth::test_auth_preset))
        .route(
            "/api/auth/presets/{preset_id}/refresh-quota",
            put(auth::refresh_auth_preset_quota),
        )
        .route(
            "/api/auth/presets/refresh-all-quotas",
            put(auth::refresh_all_auth_preset_quotas),
        )
        .route("/api/auth/api-presets/{preset_id}/apply", put(auth::apply_api_preset))
        .route("/api/auth/api-presets/{preset_id}/verify", get(auth::verify_api_preset))
        .route("/api/auth/api-presets/{preset_id}/test", post(auth::test_api_preset))
        .route("/api/auth/claude-presets/{preset_id}/apply", put(auth::apply_claude_preset))
        .route("/api/auth/claude-presets/{preset_id}/test", post(auth::test_claude_preset))
        .route(
            "/api/auth/claude-presets/{preset_id}/apply-opencode",
            put(auth::apply_claude_preset_to_opencode),
        )
        .route("/api/auth/upstream-proxy-settings", put(auth::update_upstream_proxy_settings))
        .route(
            "/api/auth/preset-test-schedules",
            get(auth::list_preset_test_schedules).post(auth::create_preset_test_schedule),
        )
        .route(
            "/api/auth/preset-test-schedules/{schedule_id}",
            put(auth::update_preset_test_schedule).delete(auth::delete_preset_test_schedule),
        )
        .route(
            "/api/auth/preset-test-schedules/{schedule_id}/run",
            post(auth::run_preset_test_schedule_now),
        )
}
