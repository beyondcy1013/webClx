use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, agent};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/agent", get(agent::agent_page))
        .route("/agent/runtime", get(agent::deepseek_harness_redirect))
        .route("/agent/profiles", get(agent::agent_profiles_page))
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
}
