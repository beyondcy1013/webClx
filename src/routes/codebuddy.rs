use axum::{
    Json,
    extract::{Path as AxumPath, State},
    routing::{get, post, put},
};
use serde::Deserialize;
use serde_json::Value;

use crate::{ApiResult, AppError, AppState};

#[derive(Debug, Deserialize)]
struct AddCodeBuddyAccountRequest {
    #[serde(default)]
    name: String,
    #[serde(default)]
    access_token: String,
    #[serde(default)]
    refresh_token: String,
    #[serde(default)]
    variant: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct OAuthStartRequest {
    #[serde(default)]
    variant: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OAuthPollRequest {
    #[serde(default)]
    login_id: String,
}

#[derive(Debug, Deserialize)]
struct UpdateCodeBuddyAutoRequest {
    #[serde(default)]
    checkin: Option<bool>,
    #[serde(default)]
    travel: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct UpdateCodeBuddyOptionsRequest {
    #[serde(rename = "bypassPermissions", default)]
    bypass_permissions: bool,
    #[serde(default)]
    sandbox: bool,
}

pub(super) fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route(
            "/api/auth/codebuddy-accounts",
            get(list_codebuddy_accounts).post(add_codebuddy_account),
        )
        .route("/api/auth/codebuddy-accounts/import", post(import_codebuddy_accounts))
        .route(
            "/api/auth/codebuddy-accounts/{account_id}",
            axum::routing::delete(delete_codebuddy_account),
        )
        .route(
            "/api/auth/codebuddy-accounts/{account_id}/switch",
            put(switch_codebuddy_account),
        )
        .route(
            "/api/auth/codebuddy-accounts/{account_id}/checkin",
            post(checkin_codebuddy_account),
        )
        .route(
            "/api/auth/codebuddy-accounts/{account_id}/travel",
            post(travel_codebuddy_account),
        )
        .route(
            "/api/auth/codebuddy-accounts/{account_id}/credits",
            get(credits_codebuddy_account),
        )
        .route(
            "/api/auth/codebuddy-accounts/detect-current",
            post(detect_current_codebuddy_account),
        )
        .route("/api/auth/codebuddy-accounts/oauth/start", post(start_codebuddy_oauth))
        .route("/api/auth/codebuddy-accounts/oauth/poll", post(poll_codebuddy_oauth))
        .route("/api/auth/codebuddy-auto", get(get_codebuddy_auto).put(update_codebuddy_auto))
        .route(
            "/api/auth/codebuddy-options",
            get(get_codebuddy_options).put(update_codebuddy_options),
        )
}

async fn detect_current_codebuddy_account(State(state): State<AppState>) -> ApiResult<Json<Value>> {
    state
        .codebuddy_manager
        .detect_current()
        .map(Json)
        .map_err(AppError::bad_request)
}

async fn list_codebuddy_accounts(State(state): State<AppState>) -> Json<Value> {
    Json(Value::Array(state.codebuddy_manager.list_accounts()))
}

async fn add_codebuddy_account(
    State(state): State<AppState>,
    Json(payload): Json<AddCodeBuddyAccountRequest>,
) -> ApiResult<Json<Value>> {
    if payload.access_token.trim().is_empty() {
        return Err(AppError::bad_request("access_token 不能为空。"));
    }
    let meta = state
        .codebuddy_manager
        .add_manual_account(
            payload.access_token,
            payload.refresh_token,
            payload.name,
            payload.variant,
        )
        .await
        .map_err(AppError::bad_request)?;
    Ok(Json(meta))
}

async fn import_codebuddy_accounts(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> ApiResult<Json<Value>> {
    let result = state
        .codebuddy_manager
        .import_accounts(&payload)
        .map_err(AppError::bad_request)?;
    Ok(Json(result))
}

async fn delete_codebuddy_account(
    State(state): State<AppState>,
    AxumPath(account_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let result = state
        .codebuddy_manager
        .delete_account(account_id.trim())
        .map_err(AppError::bad_request)?;
    Ok(Json(result))
}

async fn switch_codebuddy_account(
    State(state): State<AppState>,
    AxumPath(account_id): AxumPath<String>,
) -> ApiResult<Json<Value>> {
    let result = state
        .codebuddy_manager
        .switch_account(account_id.trim())
        .map_err(AppError::bad_request)?;
    Ok(Json(result))
}

async fn checkin_codebuddy_account(
    State(state): State<AppState>,
    AxumPath(account_id): AxumPath<String>,
) -> Json<Value> {
    Json(
        state
            .codebuddy_manager
            .checkin_account(account_id.trim())
            .await,
    )
}

async fn travel_codebuddy_account(
    State(state): State<AppState>,
    AxumPath(account_id): AxumPath<String>,
) -> Json<Value> {
    Json(
        state
            .codebuddy_manager
            .travel_account(account_id.trim())
            .await,
    )
}

async fn credits_codebuddy_account(
    State(state): State<AppState>,
    AxumPath(account_id): AxumPath<String>,
) -> Json<Value> {
    Json(
        state
            .codebuddy_manager
            .credits_account(account_id.trim())
            .await,
    )
}

async fn start_codebuddy_oauth(
    State(state): State<AppState>,
    request: axum::extract::Request,
) -> ApiResult<Json<Value>> {
    let bytes = axum::body::to_bytes(request.into_body(), 1024 * 64)
        .await
        .unwrap_or_default();
    let payload: OAuthStartRequest = serde_json::from_slice(&bytes).unwrap_or_default();
    let result = state
        .codebuddy_manager
        .oauth_start(payload.variant.as_deref())
        .await
        .map_err(AppError::bad_request)?;
    Ok(Json(result))
}

async fn poll_codebuddy_oauth(
    State(state): State<AppState>,
    Json(payload): Json<OAuthPollRequest>,
) -> Json<Value> {
    Json(
        state
            .codebuddy_manager
            .oauth_poll(payload.login_id.trim())
            .await,
    )
}

async fn get_codebuddy_auto(State(state): State<AppState>) -> Json<Value> {
    Json(state.codebuddy_manager.auto_config())
}

async fn update_codebuddy_auto(
    State(state): State<AppState>,
    Json(payload): Json<UpdateCodeBuddyAutoRequest>,
) -> ApiResult<Json<Value>> {
    let auto = state.codebuddy_manager.auto_config();
    let checkin = payload.checkin.unwrap_or_else(|| {
        auto.get("auto")
            .and_then(|value| value.get("checkin"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
    });
    let travel = payload.travel.unwrap_or_else(|| {
        auto.get("auto")
            .and_then(|value| value.get("travel"))
            .and_then(Value::as_bool)
            .unwrap_or(true)
    });
    let result = state
        .codebuddy_manager
        .set_auto_config(checkin, travel)
        .map_err(AppError::bad_request)?;
    Ok(Json(result))
}

async fn get_codebuddy_options(State(state): State<AppState>) -> Json<Value> {
    Json(state.codebuddy_manager.cli_options())
}

async fn update_codebuddy_options(
    State(state): State<AppState>,
    Json(payload): Json<UpdateCodeBuddyOptionsRequest>,
) -> ApiResult<Json<Value>> {
    let result = state
        .codebuddy_manager
        .set_cli_options(payload.bypass_permissions, payload.sandbox)
        .map_err(AppError::bad_request)?;
    Ok(Json(result))
}
