use crate::schema::*;

pub(crate) const PATH_QUERY_OPTIONAL: &[ApiField] = &[field(
    "path",
    "string",
    false,
    false,
    "Workspace-relative path. Empty string or / resolves to the current workspace root.",
    Some("src"),
    EMPTY_TEXTS,
    EMPTY_FIELDS,
)];

pub(crate) const FILE_PATH_QUERY_REQUIRED: &[ApiField] = &[field(
    "path",
    "string",
    true,
    false,
    "Workspace-relative file path.",
    Some("src/main.rs"),
    EMPTY_TEXTS,
    EMPTY_FIELDS,
)];

pub(crate) const LIST_SESSIONS_QUERY_FIELDS: &[ApiField] = &[
    field(
        "path",
        "string",
        false,
        false,
        "Workspace-relative directory path. Ignored when all=true.",
        Some("src"),
        EMPTY_TEXTS,
        EMPTY_FIELDS,
    ),
    field(
        "all",
        "boolean",
        false,
        false,
        "When true, list sessions across all directories.",
        Some("true"),
        BOOL_VALUES,
        EMPTY_FIELDS,
    ),
];

pub(crate) const COMPILE_STATUS_QUERY_FIELDS: &[ApiField] = &[field(
    "include_history",
    "boolean",
    false,
    false,
    "When false, return pending and currently running work without historical run details.",
    Some("false"),
    BOOL_VALUES,
    EMPTY_FIELDS,
)];

pub(crate) const TERMINAL_WS_QUERY_FIELDS: &[ApiField] = &[
    field(
        "path",
        "string",
        false,
        false,
        "Workspace-relative directory path whose session should be attached.",
        Some("src"),
        EMPTY_TEXTS,
        EMPTY_FIELDS,
    ),
    field(
        "session_id",
        "string",
        false,
        true,
        "Existing session id in that directory. Empty creates or opens the latest session.",
        Some("1"),
        EMPTY_TEXTS,
        EMPTY_FIELDS,
    ),
];

pub(crate) const PRESET_ID_PATH_FIELDS: &[ApiField] = &[field(
    "preset_id",
    "string",
    true,
    false,
    "Preset identifier returned by the relevant preset list endpoint.",
    Some("api-1712640000"),
    EMPTY_TEXTS,
    EMPTY_FIELDS,
)];

pub(crate) const SESSION_ID_PATH_FIELDS: &[ApiField] = &[field(
    "session_id",
    "string",
    true,
    false,
    "Terminal session id returned by /api/terminal/sessions.",
    Some("1"),
    EMPTY_TEXTS,
    EMPTY_FIELDS,
)];
