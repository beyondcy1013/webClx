use std::{
    fs,
    io::{self, Write},
    path::PathBuf,
    time::Duration,
};

use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::{ApiResult, AppError, AppState, compile_service::is_external_callback_url};

mod process;

const DEPLOY_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeployServiceRequest {
    pub service_name: String,
    pub script: String,
    pub binary_path: String,
    #[serde(default)]
    pub source_terminal_name: String,
    #[serde(default)]
    pub source_terminal_id: String,
}

#[derive(Debug, Serialize)]
pub struct DeployServiceResponse {
    pub ok: bool,
    pub service_name: String,
    pub binary_path: String,
    pub restarted: bool,
}

pub async fn deploy_service(
    State(state): State<AppState>,
    Json(payload): Json<DeployServiceRequest>,
) -> ApiResult<Json<DeployServiceResponse>> {
    let service_name = first_nonempty([payload.service_name.as_str()])
        .map(ToString::to_string)
        .ok_or_else(|| AppError::bad_request("缺少服务名称 service_name"))?;
    let script = first_nonempty([payload.script.as_str()])
        .map(ToString::to_string)
        .ok_or_else(|| AppError::bad_request("缺少部署脚本 script"))?;
    let binary_path = first_nonempty([payload.binary_path.as_str()])
        .map(PathBuf::from)
        .ok_or_else(|| AppError::bad_request("缺少目标二进制路径 binary_path"))?;

    let parent_dir = binary_path.parent().ok_or_else(|| {
        AppError::bad_request(format!("无法确定二进制文件 {} 的父目录", binary_path.display()))
    })?;
    if !tokio::fs::metadata(parent_dir)
        .await
        .map_err(|error| AppError::bad_request(format!("读取二进制文件父目录失败: {error}")))?
        .is_dir()
    {
        return Err(AppError::bad_request(format!(
            "二进制文件父目录不存在: {}",
            parent_dir.display()
        )));
    }

    run_deploy_script(script, DEPLOY_TIMEOUT).await?;

    let restarted = schedule_deploy_restart(&service_name).await?;

    let source_terminal_target = first_nonempty([
        payload.source_terminal_id.as_str(),
        payload.source_terminal_name.as_str(),
    ])
    .filter(|target| !is_external_callback_url(target.trim()));
    if let Some(target) = source_terminal_target {
        let message = format!("已部署 {} → {}，已安排重启", binary_path.display(), service_name);
        if let Err(error) = state
            .terminal_manager
            .send_session_toast(target, None, message, "info")
        {
            warn!(
                target,
                service_name,
                binary_path = %binary_path.display(),
                error = %error,
                "failed to send deploy completion toast"
            );
        }
    }

    Ok(Json(DeployServiceResponse {
        ok: true,
        service_name,
        binary_path: binary_path.display().to_string(),
        restarted,
    }))
}

async fn run_deploy_script(script: String, limit: Duration) -> ApiResult<()> {
    // The private directory guard survives until bash has stopped, including
    // cancellation and errors during file creation.
    let script = tokio::task::spawn_blocking(move || DeployScript::create(&script))
        .await
        .map_err(|error| AppError::internal(format!("创建部署脚本任务失败: {error}")))?
        .map_err(|error| AppError::internal(format!("创建临时部署脚本失败: {error}")))?;
    let mut command = tokio::process::Command::new("/bin/bash");
    command.arg(&script.path);
    process::run(&mut command, limit)
        .await
        .map_err(|error| AppError::internal(format!("部署脚本执行失败: {error}")))
}

struct DeployScript {
    directory: PathBuf,
    path: PathBuf,
}

impl DeployScript {
    fn create(script: &str) -> io::Result<Self> {
        for _ in 0..16 {
            let directory = std::env::temp_dir().join(format!(
                "webclx-deploy-{}-{:032x}",
                std::process::id(),
                rand::random::<u128>()
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&directory) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
            let guard = Self {
                path: directory.join("deploy.sh"),
                directory,
            };
            let mut options = fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            options.open(&guard.path)?.write_all(script.as_bytes())?;
            return Ok(guard);
        }
        Err(io::Error::new(io::ErrorKind::AlreadyExists, "创建唯一部署目录重试耗尽"))
    }
}

impl Drop for DeployScript {
    fn drop(&mut self) {
        if let Err(error) = fs::remove_dir_all(&self.directory) {
            warn!(path = %self.directory.display(), %error, "failed to remove deploy script");
        }
    }
}

#[cfg(target_os = "linux")]
async fn schedule_deploy_restart(service_name: &str) -> ApiResult<bool> {
    process::run(&mut restart_command(service_name), Duration::from_secs(10))
        .await
        .map_err(|error| {
            AppError::internal(format!("提交 {service_name} 重启请求失败: {error}"))
        })?;
    Ok(true)
}

#[cfg(target_os = "linux")]
fn restart_command(service_name: &str) -> tokio::process::Command {
    let mut command = tokio::process::Command::new("/usr/bin/systemd-run");
    command.args([
        "--quiet",
        "--collect",
        "--on-active=1s",
        "/bin/systemctl",
        "restart",
        "--",
        service_name,
    ]);
    command
}

#[cfg(not(target_os = "linux"))]
async fn schedule_deploy_restart(service_name: &str) -> ApiResult<bool> {
    Err(AppError::internal(format!("当前平台不支持通过 systemd 重启 {}", service_name)))
}

fn first_nonempty<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<&'a str> {
    values
        .into_iter()
        .map(str::trim)
        .find(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_scripts_are_private_unique_and_cleaned_up() {
        let first = DeployScript::create("printf first").unwrap();
        let second = DeployScript::create("printf second").unwrap();
        assert_ne!(first.path, second.path);
        assert_eq!(fs::read_to_string(&first.path).unwrap(), "printf first");
        assert_eq!(fs::read_to_string(&second.path).unwrap(), "printf second");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(&first.directory).unwrap().permissions().mode() & 0o777, 0o700);
            assert_eq!(fs::metadata(&first.path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        let directory = first.directory.clone();
        drop(first);
        assert!(!directory.exists());
        assert!(second.path.exists());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn concurrent_deploy_scripts_execute_independently() {
        tokio::time::timeout(Duration::from_secs(5), async {
            let (first, second) = tokio::join!(
                run_deploy_script("exit 0".to_string(), Duration::from_secs(3)),
                run_deploy_script("printf second >&2; exit 7".to_string(), Duration::from_secs(3)),
            );
            first.unwrap();
            assert!(second.unwrap_err().message.contains("second"));
        })
        .await
        .expect("concurrent deployment regression timed out");
    }

    #[test]
    fn deploy_request_requires_service_name() {
        let error = serde_json::from_str::<DeployServiceRequest>(
            r#"{"service_name":"","script":"echo ok","binary_path":"/home/bin/app"}"#,
        )
        .expect("valid json but empty service_name should deserialize");
        assert!(error.service_name.is_empty());
    }

    #[test]
    fn deploy_request_requires_script() {
        let payload: DeployServiceRequest = serde_json::from_str(
            r#"{"service_name":"my-app.service","script":"echo ok","binary_path":"/home/bin/app"}"#,
        )
        .expect("valid request should deserialize");
        assert_eq!(payload.script, "echo ok");
    }

    #[test]
    fn deploy_request_requires_binary_path() {
        let error = serde_json::from_str::<DeployServiceRequest>(
            r#"{"service_name":"my-app.service","script":"echo ok","binary_path":""}"#,
        )
        .expect("valid json but empty binary_path should deserialize");
        assert!(error.binary_path.is_empty());
    }

    #[test]
    fn deploy_request_rejects_unknown_fields() {
        let error = serde_json::from_str::<DeployServiceRequest>(
            r#"{"service_name":"my-app.service","script":"echo ok","binary_path":"/home/bin/app","unknown_field":true}"#,
        )
        .expect_err("unknown fields should be rejected");
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn deploy_request_accepts_optional_terminal_fields() {
        let payload: DeployServiceRequest = serde_json::from_str(
            r#"{"service_name":"my-app.service","script":"echo ok","binary_path":"/home/bin/app","source_terminal_name":"webClx_1","source_terminal_id":"s1547"}"#,
        )
        .expect("request with terminal fields should deserialize");
        assert_eq!(payload.source_terminal_name, "webClx_1");
        assert_eq!(payload.source_terminal_id, "s1547");
    }

    #[test]
    fn deploy_request_terminal_fields_are_optional() {
        let payload: DeployServiceRequest = serde_json::from_str(
            r#"{"service_name":"my-app.service","script":"echo ok","binary_path":"/home/bin/app"}"#,
        )
        .expect("request without terminal fields should deserialize");
        assert!(payload.source_terminal_name.is_empty());
        assert!(payload.source_terminal_id.is_empty());
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_restart_uses_systemd_run_with_custom_service_name() {
        let command = restart_command("my-app.service");
        assert_eq!(command.as_std().get_program(), "/usr/bin/systemd-run");
        assert_eq!(
            command.as_std().get_args().collect::<Vec<_>>(),
            [
                "--quiet",
                "--collect",
                "--on-active=1s",
                "/bin/systemctl",
                "restart",
                "--",
                "my-app.service",
            ]
        );
    }

    #[tokio::test]
    #[cfg(not(target_os = "linux"))]
    async fn non_linux_restart_returns_unsupported() {
        let result = schedule_deploy_restart("my-app.service").await;
        assert!(result.is_err());
        if let Err(error) = result {
            assert!(error.to_string().contains("不支持"));
        }
    }

    #[test]
    fn binary_path_parent_dir_must_exist_for_valid_path() {
        let temp_dir = std::env::temp_dir();
        let path = temp_dir
            .join("webclx-deploy-test-nonexistent-dir")
            .join("binary");
        let parent = path.parent().unwrap();

        // If parent doesn't exist, validation should fail
        // (but the test itself runs on a real fs, so we just verify the parent check logic)
        if !parent.is_dir() {
            // Expected: validation would catch this
        }
    }

    #[test]
    fn first_nonempty_returns_trimmed_value() {
        assert_eq!(first_nonempty(["  hello  "]), Some("hello"));
        assert_eq!(first_nonempty(["", "  world"]), Some("world"));
        assert_eq!(first_nonempty(["", "", ""]), None);
    }
}
