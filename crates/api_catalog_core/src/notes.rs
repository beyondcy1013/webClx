pub(crate) const CATALOG_NOTES: &[&str] = &[
    "This endpoint is intended for external automation and AI clients.",
    "All request and response examples are illustrative and may omit unrelated fields.",
];

pub(crate) const PATH_SCOPE_NOTES: &[&str] = &[
    "Relative paths are limited to the configured workspace root and its parent directory.",
    "Errors are returned as plain-text bodies with HTTP 400, 404, or 500.",
];

pub(crate) const FILE_READ_NOTES: &[&str] = &[
    "Relative paths are limited to the configured workspace root and its parent directory.",
    "Only UTF-8 text files can be returned inline.",
    "Files larger than 1 MiB return editable=false and an explanatory message.",
    "Errors are returned as plain-text bodies with HTTP 400, 404, or 500.",
];

pub(crate) const FILE_SAVE_NOTES: &[&str] = &[
    "Relative paths are limited to the configured workspace root and its parent directory.",
    "The target file must already exist.",
    "When saving .codex/config.toml, preserved sections are merged instead of blindly overwritten.",
    "Errors are returned as plain-text bodies with HTTP 400, 404, or 500.",
];

pub(crate) const FILE_RENAME_NOTES: &[&str] = &[
    "Relative paths are limited to the configured workspace root and its parent directory.",
    "Only ordinary files and directories can be renamed.",
    "The new name is a basename only; it cannot include path separators.",
    "The target name must not already exist.",
    "Errors are returned as plain-text bodies with HTTP 400, 404, or 500.",
];

pub(crate) const SETTINGS_SAVE_NOTES: &[&str] = &[
    "Only supplied fields are changed; omitted fields keep the current value.",
    "workspace_dir may be any existing accessible absolute directory; favorite_paths remain under the configured platform workspace limit.",
    "Errors are returned as plain-text bodies with HTTP 400, 404, or 500.",
];

pub(crate) const SAVE_AUTH_NOTES: &[&str] = &[
    "auth must be login-style auth.json content with tokens.access_token and account_id; id_token and refresh_token are optional.",
    "Errors are returned as plain-text bodies with HTTP 400, 404, or 500.",
];

pub(crate) const SAVE_API_NOTES: &[&str] = &[
    "base_url is normalized to remove a trailing slash.",
    "provider_name is auto-generated from base_url when blank.",
    "Errors are returned as plain-text bodies with HTTP 400, 404, or 500.",
];

pub(crate) const SAVE_CLAUDE_NOTES: &[&str] = &[
    "base_url is normalized to remove a trailing slash.",
    "provider_name is auto-generated from base_url when blank.",
    "default_haiku_model also accepts the legacy alias small_fast_model.",
    "third_party_model must not be mixed with the official model trio fields.",
    "Errors are returned as plain-text bodies with HTTP 400, 404, or 500.",
];

pub(crate) const APPLY_CURRENT_AUTH_NOTES: &[&str] = &[
    "Applying current auth also clears the API provider selection from config.toml.",
    "Errors are returned as plain-text bodies with HTTP 400, 404, or 500.",
];

pub(crate) const CODEX_OAUTH_NOTES: &[&str] = &[
    "This flow uses the official Codex device OAuth flow and returns the resulting auth.json-compatible token bundle.",
    "Start the flow first, then poll the session endpoint until status becomes completed, error, or expired.",
    "Errors are returned as plain-text bodies with HTTP 400, 404, or 500.",
];

pub(crate) const APPLY_PRESET_NOTES: &[&str] =
    &["Errors are returned as plain-text bodies with HTTP 400, 404, or 500."];

pub(crate) const TERMINAL_LIST_NOTES: &[&str] = &[
    "When all=true, path is ignored and the response spans all known session directories.",
    "Errors are returned as plain-text bodies with HTTP 400, 404, or 500.",
];

pub(crate) const TERMINAL_CREATE_NOTES: &[&str] = &[
    "The target directory must exist under the allowed workspace scope.",
    "Errors are returned as plain-text bodies with HTTP 400, 404, or 500.",
];

pub(crate) const TERMINAL_RENAME_NOTES: &[&str] = &[
    "name is trimmed and must not be empty.",
    "Errors are returned as plain-text bodies with HTTP 400, 404, or 500.",
];

pub(crate) const TERMINAL_WS_NOTES: &[&str] = &[
    "Successful calls return HTTP 101 and upgrade to websocket.",
    "If session_id is omitted, the latest session for the directory is opened or created.",
    "Client text frames accept JSON {\"type\":\"input\",\"data\":\"...\"} or {\"type\":\"resize\",\"cols\":120,\"rows\":32}.",
    "Client binary frames are treated as UTF-8 terminal input bytes.",
    "Server messages are websocket binary frames containing terminal output bytes.",
    "Handshake errors are returned as plain-text HTTP responses before upgrade.",
];
