use axum::{
    Router,
    routing::{delete, get, patch, post, put},
};

use crate::{AppState, codex_conversation_model, config_files, terminal};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/terminal/sessions",
            get(terminal::list_sessions).post(terminal::create_session),
        )
        .route("/api/terminal/sessions/search", get(terminal::search_sessions))
        .route(
            "/api/terminal/auto-continue-tasks",
            get(terminal::list_auto_continue_tasks).delete(terminal::delete_auto_continue_tasks),
        )
        .route(
            "/api/terminal/auto-continue-tasks/{marker}",
            put(terminal::update_auto_continue_task).delete(terminal::delete_auto_continue_task),
        )
        .route(
            "/api/terminal/scheduled-inputs",
            get(terminal::list_scheduled_inputs).post(terminal::create_scheduled_input),
        )
        .route(
            "/api/terminal/scheduled-inputs/{task_id}",
            patch(terminal::update_scheduled_input).delete(terminal::delete_scheduled_input),
        )
        .route("/api/terminal/sessions/message", post(terminal::send_session_message))
        .route(
            "/api/terminal/codex-full-access",
            get(config_files::codex_full_access_status)
                .put(config_files::enable_codex_full_access)
                .delete(config_files::disable_codex_full_access),
        )
        .route("/api/terminal/quick-command", post(terminal::prepare_quick_command))
        .route("/api/terminal/auto-typed-input", post(terminal::send_auto_typed_input))
        .route(
            "/api/terminal/sessions/{session_id}",
            put(terminal::rename_session)
                .patch(terminal::update_session_origin)
                .delete(terminal::delete_session),
        )
        .route("/api/terminal/sessions/{session_id}/input", post(terminal::send_session_input))
        .route(
            "/api/terminal/sessions/{session_id}/extract-preset",
            post(terminal::extract_session_preset),
        )
        .route(
            "/api/terminal/sessions/{session_id}/continue",
            post(terminal::send_session_continue),
        )
        .route(
            "/api/terminal/sessions/{session_id}/auto-continue",
            post(terminal::send_session_auto_continue),
        )
        .route(
            "/api/terminal/sessions/{session_id}/input-history",
            get(terminal::get_session_input_history),
        )
        .route(
            "/api/terminal/sessions/{session_id}/agents-doc",
            get(terminal::read_session_agents_doc).put(terminal::save_session_agents_doc),
        )
        .route(
            "/api/terminal/sessions/{session_id}/agents-docs",
            get(terminal::list_session_agents_docs),
        )
        .route(
            "/api/terminal/sessions/{session_id}/paste-assets",
            post(terminal::upload_paste_assets),
        )
        .route("/api/terminal/sessions/{session_id}/idle", put(terminal::idle_session))
        .route("/api/terminal/sessions/{session_id}/restore", put(terminal::restore_session))
        .route(
            "/api/terminal/sessions/{session_id}/current-directory",
            get(terminal::current_session_directory),
        )
        .route(
            "/api/terminal/sessions/{session_id}/agent-session",
            get(terminal::current_agent_session),
        )
        .route(
            "/api/terminal/sessions/{session_id}/agent-session/complete",
            get(terminal::current_agent_session_complete),
        )
        .route(
            "/api/terminal/sessions/{session_id}/interrupt-and-resume",
            post(terminal::force_interrupt_and_resume_session),
        )
        .route("/api/terminal/completion-bell.wav", get(terminal::completion_bell_sound))
        .route("/api/terminal/codex-conversations", get(terminal::list_codex_conversations))
        .route(
            "/api/terminal/codex-conversations/model",
            put(codex_conversation_model::update_codex_conversation_model),
        )
        .route(
            "/api/terminal/codex-conversations/{session_id}",
            delete(terminal::delete_codex_conversation),
        )
        .route(
            "/api/terminal/resume-archives",
            get(terminal::list_resume_archives).post(terminal::save_resume_archive),
        )
        .route(
            "/api/terminal/resume-archives/{archive_id}",
            delete(terminal::delete_resume_archive),
        )
        .route(
            "/api/terminal/resume-archives/{archive_id}/touch",
            put(terminal::touch_resume_archive),
        )
        .route("/api/terminal/ws", get(terminal::terminal_ws))
}
