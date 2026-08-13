use crate::fields::*;
use crate::schema::*;

pub(crate) const TERMINAL_SESSIONS_RESPONSE_BODY: ApiBodySchema = body(
    "application/json",
    "object",
    "Terminal session list response.",
    &[
        field(
            "all",
            "boolean",
            true,
            false,
            "Whether the list spans all directories.",
            Some("false"),
            BOOL_VALUES,
            EMPTY_FIELDS,
        ),
        field(
            "path",
            "string",
            true,
            false,
            "Workspace-relative directory path. Empty when all=true.",
            Some("src"),
            EMPTY_TEXTS,
            EMPTY_FIELDS,
        ),
        field(
            "display_path",
            "string",
            true,
            false,
            "Human-readable path label.",
            Some("/home/codes/webClx/src"),
            EMPTY_TEXTS,
            EMPTY_FIELDS,
        ),
        field(
            "sessions",
            "array<object>",
            true,
            false,
            "Terminal sessions for the requested scope.",
            None,
            EMPTY_TEXTS,
            TERMINAL_SESSION_INFO_FIELDS,
        ),
    ],
);

pub(crate) const CREATE_SESSION_REQUEST_BODY: ApiBodySchema = body(
    "application/json",
    "object",
    "Create a terminal session in a workspace-relative directory.",
    &[field(
        "path",
        "string",
        true,
        false,
        "Workspace-relative directory path for the new session.",
        Some("src"),
        EMPTY_TEXTS,
        EMPTY_FIELDS,
    )],
);

pub(crate) const TERMINAL_SESSION_INFO_BODY: ApiBodySchema = body(
    "application/json",
    "object",
    "Single terminal session info record.",
    TERMINAL_SESSION_INFO_FIELDS,
);

pub(crate) const RENAME_SESSION_REQUEST_BODY: ApiBodySchema = body(
    "application/json",
    "object",
    "Rename a terminal session.",
    &[field(
        "name",
        "string",
        true,
        false,
        "New non-empty session display name.",
        Some("backend-test"),
        EMPTY_TEXTS,
        EMPTY_FIELDS,
    )],
);

pub(crate) const WS_UPGRADE_RESPONSE_BODY: ApiBodySchema = body(
    "application/websocket",
    "websocket",
    "Successful upgrade to a terminal websocket stream.",
    &[
        field(
            "client_text_message",
            "object",
            true,
            false,
            "JSON message accepted from the client over websocket text frames.",
            None,
            EMPTY_TEXTS,
            &[
                field(
                    "type",
                    "string",
                    true,
                    false,
                    "input or resize.",
                    Some("input"),
                    &["input", "resize"],
                    EMPTY_FIELDS,
                ),
                field(
                    "data",
                    "string",
                    false,
                    false,
                    "Required when type=input. Text to write to the terminal.",
                    Some("ls\\n"),
                    EMPTY_TEXTS,
                    EMPTY_FIELDS,
                ),
                field(
                    "cols",
                    "integer",
                    false,
                    false,
                    "Required when type=resize. Terminal width.",
                    Some("120"),
                    EMPTY_TEXTS,
                    EMPTY_FIELDS,
                ),
                field(
                    "rows",
                    "integer",
                    false,
                    false,
                    "Required when type=resize. Terminal height.",
                    Some("32"),
                    EMPTY_TEXTS,
                    EMPTY_FIELDS,
                ),
            ],
        ),
        field(
            "client_binary_message",
            "binary",
            true,
            false,
            "UTF-8 bytes written directly to terminal stdin.",
            None,
            EMPTY_TEXTS,
            EMPTY_FIELDS,
        ),
        field(
            "server_binary_message",
            "binary",
            true,
            false,
            "Terminal output bytes, including backlog replay after connect.",
            None,
            EMPTY_TEXTS,
            EMPTY_FIELDS,
        ),
    ],
);
