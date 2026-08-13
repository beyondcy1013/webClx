use crate::fields::*;
use crate::schema::*;

pub(crate) const CATALOG_RESPONSE_BODY: ApiBodySchema = body(
    "application/json",
    "object",
    "Machine-readable API catalog for external tools and AI agents.",
    &[
        field(
            "format",
            "string",
            true,
            false,
            "Catalog schema version identifier.",
            Some(CATALOG_FORMAT),
            EMPTY_TEXTS,
            EMPTY_FIELDS,
        ),
        field(
            "total",
            "integer",
            true,
            false,
            "Number of exposed endpoints in the catalog.",
            Some("18"),
            EMPTY_TEXTS,
            EMPTY_FIELDS,
        ),
        field(
            "endpoints",
            "array<object>",
            true,
            false,
            "Public endpoint descriptions.",
            None,
            EMPTY_TEXTS,
            ENDPOINT_META_FIELDS,
        ),
    ],
);
