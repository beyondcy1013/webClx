mod bodies;
mod endpoints;
mod fields;
mod notes;
mod params;
mod schema;

use endpoints::API_ENDPOINTS;
use schema::CATALOG_FORMAT;

pub use schema::ApiCatalogResponse;

pub fn api_catalog() -> ApiCatalogResponse {
    ApiCatalogResponse {
        format: CATALOG_FORMAT,
        total: API_ENDPOINTS.len(),
        endpoints: API_ENDPOINTS,
    }
}

#[cfg(test)]
mod tests {
    use crate::endpoints::API_ENDPOINTS;

    #[test]
    fn api_catalog_includes_self_and_unique_paths() {
        let mut seen_paths = std::collections::HashSet::new();

        for endpoint in API_ENDPOINTS {
            assert!(
                seen_paths.insert(endpoint.path),
                "duplicate API path found: {}",
                endpoint.path
            );
        }

        assert!(
            API_ENDPOINTS
                .iter()
                .any(|endpoint| endpoint.path == "/api/codex_apis"),
            "API catalog must expose itself",
        );
    }

    #[test]
    fn every_endpoint_has_operations_with_supported_methods() {
        for endpoint in API_ENDPOINTS {
            assert!(
                !endpoint.operations.is_empty(),
                "endpoint {} must expose at least one operation",
                endpoint.path
            );

            for operation in endpoint.operations {
                assert!(
                    endpoint.methods.contains(&operation.method),
                    "operation method {} is missing from endpoint method list for {}",
                    operation.method,
                    endpoint.path
                );
                assert!(
                    operation.response_body.is_some(),
                    "operation {} {} should describe a success response",
                    operation.method,
                    endpoint.path
                );
            }
        }
    }
}
