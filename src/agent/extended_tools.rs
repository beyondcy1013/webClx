use std::{collections::HashMap, path::Path, process::Stdio, time::Duration};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde_json::{Value, json};
use tokio::{
    fs,
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::Command,
    time::timeout,
};
use toml_edit::DocumentMut;

use crate::{ApiResult, AppError, AppState, runtime_paths};

const DEFAULT_WEB_MAX_CHARS: usize = 32_000;
const MAX_WEB_MAX_CHARS: usize = 200_000;
const MAX_IMAGE_BYTES: usize = 12 * 1024 * 1024;
const MCP_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn web_fetch(url: &str, max_chars: Option<u64>) -> ApiResult<Value> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|error| AppError::bad_request(format!("URL 无效: {error}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return Err(AppError::bad_request("web_fetch 仅支持 HTTP/HTTPS URL。"));
    }
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(45))
        .user_agent("webClx-native-agent/1.0")
        .build()
        .map_err(|error| AppError::internal(format!("创建网页客户端失败: {error}")))?;
    let response = client
        .get(parsed)
        .send()
        .await
        .map_err(|error| AppError::internal(format!("读取网页失败: {error}")))?;
    let status = response.status().as_u16();
    let final_url = response.url().to_string();
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .to_string();
    let body = response
        .text()
        .await
        .map_err(|error| AppError::internal(format!("读取网页正文失败: {error}")))?;
    let text = if content_type.contains("html") || body.trim_start().starts_with('<') {
        html_to_text(&body)
    } else {
        body
    };
    let limit = max_chars
        .unwrap_or(DEFAULT_WEB_MAX_CHARS as u64)
        .clamp(1, MAX_WEB_MAX_CHARS as u64) as usize;
    let (content, truncated) = truncate_chars(&text, limit);
    Ok(json!({
        "status": status,
        "url": final_url,
        "content_type": content_type,
        "content": content,
        "truncated": truncated,
    }))
}

pub async fn web_search(query: &str, max_chars: Option<u64>) -> ApiResult<Value> {
    if query.trim().is_empty() {
        return Err(AppError::bad_request("搜索词不能为空。"));
    }
    let encoded = utf8_percent_encode(query.trim(), NON_ALPHANUMERIC).to_string();
    let url = format!("https://html.duckduckgo.com/html/?q={encoded}");
    let mut result = web_fetch(&url, max_chars).await?;
    result["query"] = Value::String(query.trim().to_string());
    Ok(result)
}

pub async fn view_image(path: &Path) -> ApiResult<Value> {
    let metadata = fs::metadata(path)
        .await
        .map_err(|error| AppError::not_found(format!("图片不存在: {error}")))?;
    if !metadata.is_file() || metadata.len() as usize > MAX_IMAGE_BYTES {
        return Err(AppError::bad_request("图片必须是小于 12 MiB 的普通文件。"));
    }
    let mime_type = match path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        _ => return Err(AppError::bad_request("不支持的图片格式。")),
    };
    let bytes = fs::read(path)
        .await
        .map_err(|error| AppError::internal(format!("读取图片失败: {error}")))?;
    Ok(json!({
        "path": path.display().to_string(),
        "mime_type": mime_type,
        "bytes": bytes.len(),
        "data_url": format!("data:{mime_type};base64,{}", STANDARD.encode(bytes)),
    }))
}

pub async fn run_browser_actions(
    cwd: &Path,
    url: Option<&str>,
    actions: &[Value],
    timeout_secs: Option<u64>,
) -> ApiResult<Value> {
    let payload = json!({
        "cwd": cwd.display().to_string(),
        "url": url,
        "actions": actions,
        "chromium": "/home/third_party/browser-tools/bin/chromium",
    });
    let mut child = Command::new("python3")
        .args(["-c", BROWSER_SCRIPT])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| AppError::internal(format!("启动 Playwright 失败: {error}")))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(payload.to_string().as_bytes())
            .await
            .map_err(|error| AppError::internal(format!("写入浏览器动作失败: {error}")))?;
    }
    let limit = Duration::from_secs(timeout_secs.unwrap_or(60).clamp(1, 120));
    let output = timeout(limit, child.wait_with_output())
        .await
        .map_err(|_| AppError::bad_request("浏览器动作超时。"))?
        .map_err(|error| AppError::internal(format!("等待浏览器动作失败: {error}")))?;
    if !output.status.success() {
        return Err(AppError::internal(format!(
            "浏览器动作失败: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|error| AppError::internal(format!("浏览器结果解析失败: {error}")))
}

#[derive(Clone)]
struct McpServer {
    name: String,
    command: String,
    args: Vec<String>,
    env: HashMap<String, String>,
}

pub async fn list_mcp_tools(state: &AppState) -> ApiResult<Value> {
    let servers = configured_mcp_servers(state).await?;
    let mut listed = Vec::new();
    for server in servers {
        match invoke_mcp(&server, "tools/list", json!({})).await {
            Ok(result) => listed.push(json!({"server": server.name, "result": result})),
            Err(error) => listed.push(json!({"server": server.name, "error": error.message})),
        }
    }
    Ok(json!({"servers": listed}))
}

pub async fn call_mcp_tool(
    state: &AppState,
    server_name: &str,
    tool_name: &str,
    arguments: Value,
) -> ApiResult<Value> {
    let server = configured_mcp_servers(state)
        .await?
        .into_iter()
        .find(|server| server.name == server_name)
        .ok_or_else(|| {
            AppError::not_found(format!("MCP 服务器 `{server_name}` 未启用或不存在。"))
        })?;
    invoke_mcp(&server, "tools/call", json!({"name": tool_name, "arguments": arguments})).await
}

async fn configured_mcp_servers(state: &AppState) -> ApiResult<Vec<McpServer>> {
    let user = state.workspace_settings.terminal_user();
    let config_path = runtime_paths::resolve_user_home_preferring_env(&user)
        .map_err(|error| AppError::internal(format!("解析 Codex 配置目录失败: {error}")))?
        .join(".codex/config.toml");
    let content = fs::read_to_string(&config_path).await.unwrap_or_default();
    let document = content
        .parse::<DocumentMut>()
        .map_err(|error| AppError::internal(format!("解析 Codex MCP 配置失败: {error}")))?;
    let Some(table) = document
        .get("mcp_servers")
        .and_then(|item| item.as_table_like())
    else {
        return Ok(Vec::new());
    };
    let mut servers = Vec::new();
    for (name, item) in table.iter() {
        let Some(server) = item.as_table_like() else {
            continue;
        };
        if server.get("enabled").and_then(|item| item.as_bool()) == Some(false) {
            continue;
        }
        let Some(command) = server.get("command").and_then(|item| item.as_str()) else {
            continue;
        };
        let args = server
            .get("args")
            .and_then(|item| item.as_array())
            .map(|array| {
                array
                    .iter()
                    .filter_map(|value| value.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        let env = server
            .get("env")
            .and_then(|item| item.as_table_like())
            .map(|table| {
                table
                    .iter()
                    .filter_map(|(key, value)| {
                        value
                            .as_str()
                            .map(|value| (key.to_string(), value.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();
        servers.push(McpServer {
            name: name.to_string(),
            command: command.to_string(),
            args,
            env,
        });
    }
    Ok(servers)
}

async fn invoke_mcp(server: &McpServer, method: &str, params: Value) -> ApiResult<Value> {
    let mut child = Command::new(&server.command)
        .args(&server.args)
        .envs(&server.env)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| AppError::internal(format!("启动 MCP 服务器失败: {error}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| AppError::internal("MCP stdin 不可用。"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError::internal("MCP stdout 不可用。"))?;
    write_mcp_message(&mut stdin, json!({
        "jsonrpc": "2.0", "id": 1, "method": "initialize",
        "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "webclx", "version": env!("CARGO_PKG_VERSION")}}
    })).await?;
    let mut lines = BufReader::new(stdout).lines();
    read_mcp_response(&mut lines, 1).await?;
    write_mcp_message(
        &mut stdin,
        json!({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}}),
    )
    .await?;
    write_mcp_message(
        &mut stdin,
        json!({"jsonrpc": "2.0", "id": 2, "method": method, "params": params}),
    )
    .await?;
    let result = read_mcp_response(&mut lines, 2).await;
    let _ = child.kill().await;
    result
}

async fn write_mcp_message(stdin: &mut tokio::process::ChildStdin, value: Value) -> ApiResult<()> {
    stdin
        .write_all(format!("{}\n", value).as_bytes())
        .await
        .map_err(|error| AppError::internal(format!("写入 MCP 请求失败: {error}")))?;
    stdin
        .flush()
        .await
        .map_err(|error| AppError::internal(format!("刷新 MCP 请求失败: {error}")))
}

async fn read_mcp_response(
    lines: &mut tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    expected_id: u64,
) -> ApiResult<Value> {
    timeout(MCP_TIMEOUT, async {
        while let Some(line) = lines
            .next_line()
            .await
            .map_err(|error| AppError::internal(format!("读取 MCP 响应失败: {error}")))?
        {
            let Ok(value) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if value.get("id").and_then(Value::as_u64) != Some(expected_id) {
                continue;
            }
            if let Some(error) = value.get("error") {
                return Err(AppError::internal(format!("MCP 返回错误: {error}")));
            }
            return Ok(value.get("result").cloned().unwrap_or(Value::Null));
        }
        Err(AppError::internal("MCP 服务器在响应前退出。"))
    })
    .await
    .map_err(|_| AppError::internal("MCP 请求超时。"))?
}

fn html_to_text(html: &str) -> String {
    let mut text = String::with_capacity(html.len().min(DEFAULT_WEB_MAX_CHARS));
    let mut in_tag = false;
    let mut previous_space = false;
    for character in html.chars() {
        match character {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                if !previous_space {
                    text.push(' ');
                    previous_space = true;
                }
            }
            _ if in_tag => {}
            _ if character.is_whitespace() => {
                if !previous_space {
                    text.push(' ');
                    previous_space = true;
                }
            }
            _ => {
                text.push(character);
                previous_space = false;
            }
        }
    }
    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

fn truncate_chars(value: &str, limit: usize) -> (String, bool) {
    let mut chars = value.chars();
    let content = chars.by_ref().take(limit).collect::<String>();
    let truncated = chars.next().is_some();
    (content, truncated)
}

const BROWSER_SCRIPT: &str = r#"
import json, os, sys
from playwright.sync_api import sync_playwright
p = json.load(sys.stdin)
results = []
with sync_playwright() as pw:
    options = {"headless": True}
    if os.path.exists(p.get("chromium", "")):
        options["executable_path"] = p["chromium"]
    browser = pw.chromium.launch(**options)
    page = browser.new_page(viewport={"width": 1280, "height": 800})
    if p.get("url"):
        page.goto(p["url"], wait_until="domcontentloaded")
    for action in p.get("actions", []):
        kind = action.get("type")
        selector = action.get("selector")
        if kind == "goto": page.goto(action["url"], wait_until=action.get("wait_until", "domcontentloaded"))
        elif kind == "click": page.locator(selector).click()
        elif kind == "fill": page.locator(selector).fill(action.get("value", ""))
        elif kind == "press": page.locator(selector).press(action["key"])
        elif kind == "wait_for": page.locator(selector).wait_for(state=action.get("state", "visible"))
        elif kind == "text": results.append({"type": "text", "selector": selector, "value": page.locator(selector).inner_text()})
        elif kind == "screenshot":
            path = action.get("path") or os.path.join(p["cwd"], "agent-browser.png")
            page.screenshot(path=path, full_page=bool(action.get("full_page", False)))
            results.append({"type": "screenshot", "path": path})
        else: raise ValueError("unsupported browser action: %s" % kind)
    result = {"url": page.url, "title": page.title(), "results": results}
    browser.close()
print(json.dumps(result, ensure_ascii=False))
"#;
