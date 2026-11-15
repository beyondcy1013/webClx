use std::{
    env,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_rustls::{
    TlsConnector,
    rustls::{ClientConfig, RootCertStore, pki_types::ServerName},
};
use tracing::{info, warn};

use crate::proxy::{ProxyManager, ProxyPreset, ProxyType, build_proxy_env};

const BRIDGE_ADDR_ENV: &str = "WEBCLX_HTTPS_PROXY_BRIDGE_ADDR";
const DEFAULT_PORT_OFFSET: u16 = 1000;
const BRIDGE_USERNAME: &str = "webclx-https-proxy";
const CONNECT_TIMEOUT_SECS: u64 = 15;
const INITIAL_REQUEST_TIMEOUT_SECS: u64 = 15;
const MAX_INITIAL_REQUEST_BYTES: usize = 64 * 1024;

pub(crate) async fn spawn_https_proxy_bridge(
    proxy_manager: ProxyManager,
    webclx_addr: SocketAddr,
) -> Result<SocketAddr> {
    let bind_addr = configured_bridge_addr(webclx_addr)?;
    spawn_https_proxy_bridge_on(proxy_manager, bind_addr).await
}

async fn spawn_https_proxy_bridge_on(
    proxy_manager: ProxyManager,
    bind_addr: SocketAddr,
) -> Result<SocketAddr> {
    let listener = TcpListener::bind(bind_addr)
        .await
        .with_context(|| format!("绑定 HTTPS 代理桥接地址 {bind_addr} 失败"))?;
    let local_addr = listener
        .local_addr()
        .context("读取 HTTPS 代理桥接地址失败")?;
    let tls_config = Arc::new(build_tls_config());

    tokio::spawn(async move {
        run_bridge(listener, proxy_manager, tls_config).await;
    });
    info!("HTTPS proxy bridge listening on http://{local_addr}");
    Ok(local_addr)
}

pub(crate) fn build_terminal_proxy_env(
    proxy: &ProxyPreset,
    bridge_addr: Option<SocketAddr>,
) -> Vec<(String, String)> {
    if proxy.proxy_type != ProxyType::Https {
        return build_proxy_env(
            &proxy.proxy_type,
            &proxy.server,
            proxy.username.as_deref(),
            proxy.password.as_deref(),
        );
    }

    let Some(bridge_addr) = bridge_addr else {
        return build_proxy_env(
            &proxy.proxy_type,
            &proxy.server,
            proxy.username.as_deref(),
            proxy.password.as_deref(),
        );
    };

    build_proxy_env(
        &ProxyType::Http,
        &bridge_addr.to_string(),
        Some(BRIDGE_USERNAME),
        Some(&proxy.id),
    )
}

fn configured_bridge_addr(webclx_addr: SocketAddr) -> Result<SocketAddr> {
    if let Ok(value) = env::var(BRIDGE_ADDR_ENV) {
        let addr: SocketAddr = value
            .parse()
            .with_context(|| format!("{BRIDGE_ADDR_ENV} `{value}` 无效"))?;
        if !addr.ip().is_loopback() {
            anyhow::bail!("{BRIDGE_ADDR_ENV} 必须绑定 loopback 地址");
        }
        return Ok(addr);
    }

    let port = webclx_addr
        .port()
        .checked_add(DEFAULT_PORT_OFFSET)
        .context("webClx 监听端口过高，无法分配 HTTPS 代理桥接端口")?;
    Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port))
}

fn build_tls_config() -> ClientConfig {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

async fn run_bridge(
    listener: TcpListener,
    proxy_manager: ProxyManager,
    tls_config: Arc<ClientConfig>,
) {
    loop {
        let (stream, peer_addr) = match listener.accept().await {
            Ok(connection) => connection,
            Err(error) => {
                warn!("accept HTTPS proxy bridge connection failed: {error}");
                continue;
            }
        };
        let proxy_manager = proxy_manager.clone();
        let tls_config = tls_config.clone();
        tokio::spawn(async move {
            if let Err(error) = handle_bridge_connection(stream, proxy_manager, tls_config).await {
                warn!("HTTPS proxy bridge connection from {peer_addr} failed: {error}");
            }
        });
    }
}

async fn handle_bridge_connection(
    mut client: TcpStream,
    proxy_manager: ProxyManager,
    tls_config: Arc<ClientConfig>,
) -> Result<()> {
    client.set_nodelay(true)?;
    let initial_request = match read_initial_proxy_request(&mut client).await {
        Ok(request) => request,
        Err(error) => {
            write_proxy_error(&mut client, 400, "Bad Request").await;
            return Err(error);
        }
    };
    let preset_id = match bridge_preset_id(&initial_request) {
        Some(preset_id) => preset_id,
        None => {
            write_proxy_error(&mut client, 407, "Proxy Authentication Required").await;
            anyhow::bail!("缺少有效的 webClx HTTPS 代理桥接身份");
        }
    };
    let Some(proxy) = proxy_manager.get(&preset_id) else {
        write_proxy_error(&mut client, 407, "Proxy Authentication Required").await;
        anyhow::bail!("HTTPS 代理桥接预设 `{preset_id}` 不存在");
    };
    if proxy.proxy_type != ProxyType::Https {
        write_proxy_error(&mut client, 502, "Bad Gateway").await;
        anyhow::bail!("代理预设 `{preset_id}` 不是 HTTPS 代理");
    }

    let (host, port) = https_proxy_host_port(&proxy)?;
    let upstream_tcp = match timeout(
        Duration::from_secs(CONNECT_TIMEOUT_SECS),
        TcpStream::connect((host.as_str(), port)),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            write_proxy_error(&mut client, 502, "Bad Gateway").await;
            return Err(error).context("连接远端 HTTPS 代理失败");
        }
        Err(_) => {
            write_proxy_error(&mut client, 504, "Gateway Timeout").await;
            anyhow::bail!("连接远端 HTTPS 代理超时");
        }
    };
    upstream_tcp.set_nodelay(true)?;

    let server_name = ServerName::try_from(host.clone())
        .map_err(|error| anyhow::anyhow!("HTTPS 代理域名 `{host}` 无效: {error}"))?;
    let connector = TlsConnector::from(tls_config);
    let mut upstream = match timeout(
        Duration::from_secs(CONNECT_TIMEOUT_SECS),
        connector.connect(server_name, upstream_tcp),
    )
    .await
    {
        Ok(Ok(stream)) => stream,
        Ok(Err(error)) => {
            write_proxy_error(&mut client, 502, "Bad Gateway").await;
            return Err(error).context("与远端 HTTPS 代理建立 TLS 失败");
        }
        Err(_) => {
            write_proxy_error(&mut client, 504, "Gateway Timeout").await;
            anyhow::bail!("与远端 HTTPS 代理建立 TLS 超时");
        }
    };

    let upstream_request = rewrite_proxy_authorization(&initial_request, &proxy)?;
    upstream
        .write_all(&upstream_request)
        .await
        .context("向远端 HTTPS 代理发送首个请求失败")?;
    tokio::io::copy_bidirectional(&mut client, &mut upstream)
        .await
        .context("HTTPS 代理桥接传输失败")?;
    Ok(())
}

async fn read_initial_proxy_request(client: &mut TcpStream) -> Result<Vec<u8>> {
    timeout(Duration::from_secs(INITIAL_REQUEST_TIMEOUT_SECS), async {
        let mut request = Vec::with_capacity(4096);
        let mut chunk = [0_u8; 4096];
        loop {
            let read = client.read(&mut chunk).await?;
            if read == 0 {
                anyhow::bail!("代理客户端在请求头完成前断开");
            }
            request.extend_from_slice(&chunk[..read]);
            if find_header_end(&request).is_some() {
                return Ok(request);
            }
            if request.len() > MAX_INITIAL_REQUEST_BYTES {
                anyhow::bail!("代理请求头超过 {} 字节", MAX_INITIAL_REQUEST_BYTES);
            }
        }
    })
    .await
    .context("等待代理客户端请求头超时")?
}

fn bridge_preset_id(request: &[u8]) -> Option<String> {
    let header_end = find_header_end(request)?;
    let headers = std::str::from_utf8(&request[..header_end]).ok()?;
    let encoded = headers.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.trim().eq_ignore_ascii_case("proxy-authorization") {
            return None;
        }
        value
            .trim()
            .strip_prefix("Basic ")
            .or_else(|| value.trim().strip_prefix("basic "))
    })?;
    let decoded = STANDARD.decode(encoded).ok()?;
    let decoded = std::str::from_utf8(&decoded).ok()?;
    let (username, preset_id) = decoded.split_once(':')?;
    (username == BRIDGE_USERNAME && !preset_id.trim().is_empty()).then(|| preset_id.to_string())
}

fn rewrite_proxy_authorization(request: &[u8], proxy: &ProxyPreset) -> Result<Vec<u8>> {
    let header_end = find_header_end(request).context("代理请求缺少完整请求头")?;
    let headers =
        std::str::from_utf8(&request[..header_end - 4]).context("代理请求头不是 UTF-8")?;
    let mut lines = headers.split("\r\n");
    let request_line = lines.next().context("代理请求缺少请求行")?;
    let mut rewritten = Vec::with_capacity(request.len() + 128);
    rewritten.extend_from_slice(request_line.as_bytes());
    rewritten.extend_from_slice(b"\r\n");
    for line in lines {
        let is_proxy_auth = line
            .split_once(':')
            .is_some_and(|(name, _)| name.trim().eq_ignore_ascii_case("proxy-authorization"));
        if !is_proxy_auth {
            rewritten.extend_from_slice(line.as_bytes());
            rewritten.extend_from_slice(b"\r\n");
        }
    }

    match (proxy.username.as_deref(), proxy.password.as_deref()) {
        (Some(username), Some(password)) => {
            let encoded = STANDARD.encode(format!("{username}:{password}"));
            rewritten.extend_from_slice(b"Proxy-Authorization: Basic ");
            rewritten.extend_from_slice(encoded.as_bytes());
            rewritten.extend_from_slice(b"\r\n");
        }
        (None, None) => {}
        _ => anyhow::bail!("代理用户名和密码必须同时配置"),
    }

    rewritten.extend_from_slice(b"\r\n");
    rewritten.extend_from_slice(&request[header_end..]);
    Ok(rewritten)
}

fn https_proxy_host_port(proxy: &ProxyPreset) -> Result<(String, u16)> {
    let url =
        reqwest::Url::parse(&format!("https://{}", proxy.server)).context("HTTPS 代理地址无效")?;
    let host = url
        .host_str()
        .map(str::to_string)
        .context("HTTPS 代理地址缺少域名")?;
    let port = url.port_or_known_default().unwrap_or(443);
    Ok((host, port))
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .map(|index| index + 4)
}

async fn write_proxy_error(client: &mut TcpStream, status: u16, reason: &str) {
    let response =
        format!("HTTP/1.1 {status} {reason}\r\nConnection: close\r\nContent-Length: 0\r\n\r\n");
    let _ = client.write_all(response.as_bytes()).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::process::Command;

    fn https_proxy() -> ProxyPreset {
        ProxyPreset {
            id: "proxy-https-test".to_string(),
            name: "HTTPS proxy".to_string(),
            proxy_type: ProxyType::Https,
            server: "us.fpsq.xyz:17891".to_string(),
            enabled: true,
            username: Some("proxy-user".to_string()),
            password: Some("proxy-secret".to_string()),
        }
    }

    #[test]
    fn https_terminal_env_uses_loopback_http_bridge_identity() {
        let env =
            build_terminal_proxy_env(&https_proxy(), Some("127.0.0.1:12111".parse().unwrap()));

        assert!(env.iter().all(|(_, value)| value.starts_with("http://")));
        assert!(
            env.iter()
                .all(|(_, value)| value.contains("webclx-https-proxy:proxy-https-test@"))
        );
        assert!(env.iter().all(|(_, value)| !value.contains("proxy-secret")));
        assert!(env.iter().all(|(_, value)| !value.contains("us.fpsq.xyz")));
    }

    #[test]
    fn non_https_terminal_env_keeps_original_proxy_url() {
        let mut proxy = https_proxy();
        proxy.proxy_type = ProxyType::Http;
        proxy.server = "proxy.example.com:8080".to_string();
        let env = build_terminal_proxy_env(&proxy, Some("127.0.0.1:12111".parse().unwrap()));

        assert!(
            env.iter()
                .all(|(_, value)| value.starts_with("http://proxy-user:proxy-secret@"))
        );
        assert!(
            env.iter()
                .all(|(_, value)| value.contains("proxy.example.com:8080"))
        );
    }

    #[test]
    fn bridge_identity_selects_preset_and_rewrites_real_basic_auth() {
        let synthetic = STANDARD.encode(format!("{BRIDGE_USERNAME}:proxy-https-test"));
        let request = format!(
            "CONNECT chatgpt.com:443 HTTP/1.1\r\nHost: chatgpt.com:443\r\nProxy-Authorization: Basic {synthetic}\r\n\r\n"
        );
        assert_eq!(bridge_preset_id(request.as_bytes()).as_deref(), Some("proxy-https-test"));

        let rewritten = rewrite_proxy_authorization(request.as_bytes(), &https_proxy()).unwrap();
        let rewritten = String::from_utf8(rewritten).unwrap();
        let expected = STANDARD.encode("proxy-user:proxy-secret");
        assert!(rewritten.contains(&format!("Proxy-Authorization: Basic {expected}\r\n")));
        assert!(!rewritten.contains(&synthetic));
    }

    #[tokio::test]
    #[ignore = "requires a live Codex OAuth account and explicit proxy config directory"]
    async fn live_https_bridge_codex_oauth_probe() {
        let config_dir = std::env::var("WEBCLX_LIVE_PROXY_CONFIG_DIR")
            .expect("set WEBCLX_LIVE_PROXY_CONFIG_DIR to the webClx runtime directory");
        let manager = ProxyManager::load(std::path::Path::new(&config_dir))
            .expect("proxy config should load");
        let active = manager.get_active().expect("an active proxy is required");
        assert_eq!(active.proxy_type, ProxyType::Https);

        let bridge_addr =
            spawn_https_proxy_bridge_on(manager.clone(), "127.0.0.1:0".parse().unwrap())
                .await
                .expect("live HTTPS proxy bridge should start");
        manager.set_https_proxy_bridge_addr(bridge_addr);

        let output_path = std::env::temp_dir().join(format!(
            "webclx-https-bridge-live-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let mut command = Command::new("codex");
        command
            .arg("exec")
            .arg("--skip-git-repo-check")
            .arg("--color")
            .arg("never")
            .arg("--output-last-message")
            .arg(&output_path)
            .arg("Reply with exactly WEBCLX_PROXY_OK and nothing else.");
        for key in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "NO_PROXY",
            "no_proxy",
        ] {
            command.env_remove(key);
        }
        for (key, value) in manager.get_terminal_proxy_env() {
            command.env(key, value);
        }

        let output = timeout(Duration::from_secs(90), command.output())
            .await
            .expect("live Codex OAuth probe should not time out")
            .expect("codex exec should start");
        let last_message = std::fs::read_to_string(&output_path).unwrap_or_default();
        let _ = std::fs::remove_file(&output_path);
        assert!(
            output.status.success(),
            "stdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(last_message.trim(), "WEBCLX_PROXY_OK");
    }
}
