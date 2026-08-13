use axum::{Router, routing::get};

use crate::{AppState, codex_task};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/codex/tasks", get(codex_task::list_tasks).post(codex_task::create_task))
        .route(
            "/api/codex/tasks/{task_id}",
            get(codex_task::get_task).delete(codex_task::cancel_task),
        )
}
