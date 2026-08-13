use serde::Serialize;

pub(crate) const CATALOG_FORMAT: &str = "webclx_codex_api_catalog_v2";
pub(crate) const EMPTY_FIELDS: &[ApiField] = &[];
pub(crate) const EMPTY_TEXTS: &[&str] = &[];
pub(crate) const GET_ONLY: &[&str] = &["GET"];
pub(crate) const POST_ONLY: &[&str] = &["POST"];
pub(crate) const PUT_ONLY: &[&str] = &["PUT"];
pub(crate) const GET_POST: &[&str] = &["GET", "POST"];
pub(crate) const PUT_DELETE: &[&str] = &["PUT", "DELETE"];
pub(crate) const GET_PUT: &[&str] = &["GET", "PUT"];
pub(crate) const WS_UPGRADE: &[&str] = &["GET", "WS"];
pub(crate) const BOOL_VALUES: &[&str] = &["true", "false"];
pub(crate) const CURRENT_AUTH_MODE_VALUES: &[&str] = &["none", "auth", "api"];
pub(crate) const FAVORITE_PATH_KIND_VALUES: &[&str] = &["dir", "file"];
pub(crate) const DIRECTORY_ENTRY_KIND_VALUES: &[&str] = &["dir", "file", "symlink", "other"];

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ApiCatalogResponse {
    pub(crate) format: &'static str,
    pub(crate) total: usize,
    pub(crate) endpoints: &'static [ApiEndpoint],
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ApiEndpoint {
    pub(crate) path: &'static str,
    pub(crate) methods: &'static [&'static str],
    pub(crate) description: &'static str,
    pub(crate) operations: &'static [ApiOperation],
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ApiOperation {
    pub(crate) method: &'static str,
    pub(crate) description: &'static str,
    pub(crate) success_status: u16,
    #[serde(skip_serializing_if = "slice_is_empty")]
    pub(crate) path_params: &'static [ApiField],
    #[serde(skip_serializing_if = "slice_is_empty")]
    pub(crate) query_params: &'static [ApiField],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_body: Option<ApiBodySchema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) response_body: Option<ApiBodySchema>,
    #[serde(skip_serializing_if = "slice_is_empty")]
    pub(crate) notes: &'static [&'static str],
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) request_example: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) response_example: Option<&'static str>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ApiBodySchema {
    pub(crate) content_type: &'static str,
    #[serde(rename = "type")]
    pub(crate) body_type: &'static str,
    pub(crate) description: &'static str,
    #[serde(skip_serializing_if = "slice_is_empty")]
    pub(crate) fields: &'static [ApiField],
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub(crate) struct ApiField {
    pub(crate) name: &'static str,
    #[serde(rename = "type")]
    pub(crate) field_type: &'static str,
    pub(crate) required: bool,
    pub(crate) nullable: bool,
    pub(crate) description: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) example: Option<&'static str>,
    #[serde(skip_serializing_if = "slice_is_empty")]
    pub(crate) enum_values: &'static [&'static str],
    #[serde(skip_serializing_if = "slice_is_empty")]
    pub(crate) fields: &'static [ApiField],
}

pub(crate) const fn slice_is_empty<T>(items: &[T]) -> bool {
    items.is_empty()
}

pub(crate) const fn field(
    name: &'static str,
    field_type: &'static str,
    required: bool,
    nullable: bool,
    description: &'static str,
    example: Option<&'static str>,
    enum_values: &'static [&'static str],
    fields: &'static [ApiField],
) -> ApiField {
    ApiField {
        name,
        field_type,
        required,
        nullable,
        description,
        example,
        enum_values,
        fields,
    }
}

pub(crate) const fn body(
    content_type: &'static str,
    body_type: &'static str,
    description: &'static str,
    fields: &'static [ApiField],
) -> ApiBodySchema {
    ApiBodySchema {
        content_type,
        body_type,
        description,
        fields,
    }
}
