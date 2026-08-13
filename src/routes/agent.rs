use axum::{
    Router,
    routing::{get, post, put},
};

use crate::{AppState, agent};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/agent", get(agent::agent_page))
        .route("/api/agent/sessions", get(agent::list_sessions).post(agent::create_session))
        .route(
            "/api/agent/sessions/{session_id}",
            get(agent::get_session)
                .put(agent::rename_session)
                .delete(agent::delete_session),
        )
        .route("/api/agent/sessions/{session_id}/clear", post(agent::clear_session))
        .route("/api/agent/sessions/{session_id}/context", get(agent::get_session_context))
        .route("/api/agent/sessions/{session_id}/run", get(agent::get_run_status))
        .route("/api/agent/sessions/{session_id}/approvals", get(agent::list_approvals))
        .route(
            "/api/agent/sessions/{session_id}/approvals/{approval_id}/allow",
            post(agent::allow_approval),
        )
        .route(
            "/api/agent/sessions/{session_id}/approvals/{approval_id}/deny",
            post(agent::deny_approval),
        )
        .route(
            "/api/agent/sessions/{session_id}/approvals/allow-all",
            post(agent::allow_all_approvals),
        )
        .route("/api/agent/sessions/{session_id}/summary", put(agent::update_session_summary))
        .route("/api/agent/sessions/{session_id}/checkpoints", get(agent::list_checkpoints))
        .route(
            "/api/agent/sessions/{session_id}/checkpoints/{checkpoint_id}/restore",
            post(agent::restore_checkpoint),
        )
        .route("/api/agent/sessions/{session_id}/compact", post(agent::compact_session))
        .route(
            "/api/agent/sessions/{session_id}/commands",
            get(agent::list_background_commands).post(agent::start_background_command_session),
        )
        .route(
            "/api/agent/sessions/{session_id}/commands/{command_id}",
            get(agent::get_background_command_session),
        )
        .route(
            "/api/agent/sessions/{session_id}/commands/{command_id}/stdin",
            post(agent::write_background_command_session),
        )
        .route(
            "/api/agent/sessions/{session_id}/commands/{command_id}/terminate",
            post(agent::terminate_background_command_session),
        )
        .route("/api/agent/sessions/{session_id}/chat", post(agent::chat))
        .route("/api/agent/sessions/{session_id}/chat/stop", post(agent::stop_chat))
        .route("/api/agent/tools", get(agent::list_tools))
        .route("/api/agent/models", get(agent::available_models))
        .route("/api/agent/config", get(agent::get_config).put(agent::save_config))
        .route(
            "/api/agent/terminal-profiles",
            get(agent::list_terminal_profiles).post(agent::create_terminal_profile),
        )
        .route(
            "/api/agent/terminal-profiles/{profile_id}",
            get(agent::get_terminal_profile)
                .put(agent::update_terminal_profile)
                .delete(agent::delete_terminal_profile),
        )
        .route("/api/agent/api-presets", get(agent::list_api_presets_for_agent))
        .route("/api/agent/exec-with-preset", post(agent::exec_with_preset))
        .route("/api/agent/skills", get(agent::list_skills_api))
        .route("/api/agent/skills/toggle", post(agent::toggle_skill))
        .route(
            "/api/agent/skill-dirs",
            post(agent::add_skill_dir).delete(agent::remove_skill_dir),
        )
}
