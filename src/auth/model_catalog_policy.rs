use std::path::{Path, PathBuf};

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};

use crate::{ApiResult, AppError, AppState};

#[derive(Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelCatalogPolicy {
    pub suppress_comp_hash_changes: bool,
}

fn policy_path(state: &AppState) -> PathBuf {
    state
        .auth_manager
        .api_preset_file()
        .with_file_name("webclx-model-catalog-policy.json")
}

async fn read_policy(path: &Path) -> ApiResult<ModelCatalogPolicy> {
    match tokio::fs::read(path).await {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .map_err(|error| AppError::internal(format!("读取模型目录策略失败: {error}"))),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(ModelCatalogPolicy::default())
        }
        Err(error) => Err(AppError::internal(format!("读取模型目录策略失败: {error}"))),
    }
}

pub(super) async fn load(state: &AppState) -> ApiResult<ModelCatalogPolicy> {
    read_policy(&policy_path(state)).await
}

pub async fn get_model_catalog_policy(
    State(state): State<AppState>,
) -> ApiResult<Json<ModelCatalogPolicy>> {
    Ok(Json(load(&state).await?))
}

pub async fn save_model_catalog_policy(
    State(state): State<AppState>,
    Json(policy): Json<ModelCatalogPolicy>,
) -> ApiResult<Json<ModelCatalogPolicy>> {
    // Serialize with apply/update, never hold a lock for an Agent lifetime.
    let _guard = super::lock_active_config_for_request(&state).await;
    let bytes = serde_json::to_vec_pretty(&policy)
        .map_err(|error| AppError::internal(format!("编码模型目录策略失败: {error}")))?;
    crate::private_file::write_atomic(&policy_path(&state), &bytes)
        .map_err(|error| AppError::internal(format!("保存模型目录策略失败: {error}")))?;
    // Saving policy does not apply a preset or mutate a running session. Every
    // subsequent preset apply/update writes the policy into its real catalog.
    Ok(Json(policy))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn model_catalog_policy_round_trip_and_invalid_file() {
        let dir = std::env::temp_dir().join(format!(
            "webclx-catalog-policy-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        struct Cleanup(PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(dir.clone());
        let path = dir.join("policy.json");
        assert_eq!(read_policy(&path).await.unwrap(), ModelCatalogPolicy::default());
        for suppress in [true, false] {
            let policy = ModelCatalogPolicy {
                suppress_comp_hash_changes: suppress,
            };
            crate::private_file::write_atomic(&path, &serde_json::to_vec(&policy).unwrap())
                .unwrap();
            assert_eq!(read_policy(&path).await.unwrap(), policy);
        }
        tokio::fs::write(&path, b"{broken").await.unwrap();
        assert!(read_policy(&path).await.is_err());
        assert!(
            serde_json::from_str::<ModelCatalogPolicy>(r#"{"suppress_comp_hash_changes":"false"}"#)
                .is_err()
        );
    }
}
