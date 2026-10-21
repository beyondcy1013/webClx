use axum::Json;

pub async fn list_api_catalog() -> Json<api_catalog_core::ApiCatalogResponse> {
    Json(api_catalog_core::api_catalog())
}
