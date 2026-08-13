use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};

use crate::{AppState, config_files, filesystem, preset_sync, settings};

pub(super) fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/entries", get(filesystem::list_entries))
        .route("/api/workspace-icon", get(filesystem::read_workspace_icon))
        .route("/api/file", get(filesystem::read_file).put(filesystem::save_file))
        .route("/api/file/rename", post(filesystem::rename_path))
        .route("/api/settings", get(settings::get_settings).put(settings::save_settings))
        .route("/api/settings/merge-field", post(settings::merge_field))
        .route("/api/settings/merge-tab", post(settings::merge_tab))
        .route("/api/settings/merge-all", post(settings::merge_all))
        .route(
            "/api/settings/config-file",
            get(config_files::read_config_file).put(config_files::save_config_file),
        )
        .route(
            "/api/settings/codex-common-config",
            get(config_files::read_codex_common_config).put(config_files::save_codex_common_config),
        )
        .route("/api/settings/preset-config", get(preset_sync::export_preset_config))
        .route(
            "/api/settings/preset-config/clipboard/{section}/export",
            post(preset_sync::export_account_presets_to_clipboard),
        )
        .route(
            "/api/settings/preset-config/clipboard/{section}/import",
            post(preset_sync::import_account_presets_from_clipboard)
                .layer(DefaultBodyLimit::max(16 * 1024 * 1024)),
        )
        .route(
            "/api/settings/preset-config/remote-preview",
            post(preset_sync::preview_remote_preset_config),
        )
        .route(
            "/api/settings/preset-config/import-remote",
            post(preset_sync::import_remote_preset_config),
        )
}
