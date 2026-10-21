use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, collaboration};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/collaboration", get(crate::collaboration_page))
        .route(
            "/api/collaboration/agents",
            get(collaboration::list_agents).post(collaboration::register_agent),
        )
        .route(
            "/api/collaboration/groups",
            get(collaboration::list_groups).post(collaboration::create_group),
        )
        .route("/api/collaboration/groups/{group_id}", get(collaboration::get_group))
        .route("/api/collaboration/groups/{group_id}/join", post(collaboration::join_group))
        .route("/api/collaboration/groups/{group_id}/leave", post(collaboration::leave_group))
        .route(
            "/api/collaboration/groups/{group_id}/messages",
            get(collaboration::list_messages).post(collaboration::send_message),
        )
        .route(
            "/api/collaboration/groups/{group_id}/markdown",
            get(collaboration::markdown_history),
        )
        .route("/api/collaboration/groups/{group_id}/reconcile", post(collaboration::reconcile))
        .route("/api/collaboration/messages/{message_id}/ack", post(collaboration::ack_message))
}
