use crate::schema::*;

pub(crate) const DIRECTORY_ENTRY_FIELDS: &[ApiField] = &[
    field(
        "name",
        "string",
        true,
        false,
        "Base filename.",
        Some("src"),
        EMPTY_TEXTS,
        EMPTY_FIELDS,
    ),
    field(
        "path",
        "string",
        true,
        false,
        "Workspace-relative path.",
        Some("src"),
        EMPTY_TEXTS,
        EMPTY_FIELDS,
    ),
    field(
        "kind",
        "string",
        true,
        false,
        "Entry type.",
        Some("dir"),
        DIRECTORY_ENTRY_KIND_VALUES,
        EMPTY_FIELDS,
    ),
    field(
        "size",
        "integer",
        true,
        true,
        "File size in bytes for regular files.",
        Some("4096"),
        EMPTY_TEXTS,
        EMPTY_FIELDS,
    ),
];
