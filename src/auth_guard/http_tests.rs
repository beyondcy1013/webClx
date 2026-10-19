use super::*;
use axum::http::header::{COOKIE, LOCATION};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::{net::SocketAddr, sync::Arc, time::Duration};

struct TestApp {
    state: AppState,
    server: tokio::task::JoinHandle<()>,
    client: reqwest::Client,
    base: String,
}

impl TestApp {
    async fn start(addr: IpAddr) -> Self {
        let app_dir =
            std::env::temp_dir().join(format!("webclx-auth-http-{:032x}", rand::random::<u128>()));
        fs::create_dir(&app_dir).unwrap();
        let state = AppState {
            static_dir: app_dir.join("static"),
            listen_addr: "127.0.0.1:0".parse().unwrap(),
            version: "test".to_string(),
            app_dir: app_dir.clone(),
            local_api_token: Arc::from("test-local-api-token"),
            workspace_settings: crate::settings::SettingsManager::load(&app_dir).unwrap(),
            auth_manager: crate::auth::AuthPresetManager::load(&app_dir).unwrap(),
            codebuddy_manager: crate::codebuddy::CodeBuddyManager::load(
                app_dir.clone(),
                std::env::temp_dir(),
            )
            .unwrap(),
            agy_manager: crate::agy::Manager::load(app_dir.clone(), std::env::temp_dir()),
            devin_manager: crate::devin::DevinManager::load(app_dir.clone(), std::env::temp_dir()),
            codex_oauth_manager: crate::auth::CodexOAuthManager::new(),
            codex_proxy_history: crate::codex_proxy::CodexProxyHistory::new(),
            proxy_manager: crate::proxy::ProxyManager::load(&app_dir).unwrap(),
            quota_reset_cache: crate::quota_reset_cache::QuotaResetCache::new(),
            quota_manager: crate::quota::QuotaConfigManager::load(&app_dir),
            frpc_manager: crate::frpc::FrpcManager::load(&app_dir, 0).unwrap(),
            frps_manager: crate::frpc::FrpsManager::load(&app_dir).unwrap(),
            frp_role_manager: crate::frpc::FrpRoleManager::load(&app_dir, 0).unwrap(),
            terminal_manager: crate::terminal::TerminalManager::new(app_dir.join("sessions.json")),
            preset_test_scheduler: crate::auth::PresetTestScheduler::new(
                &app_dir.join("sessions.json"),
            ),
            agent_manager: crate::agent::AgentManager::new(&app_dir),
            agent_config: crate::agent::AgentConfigManager::new(&app_dir),
            collaboration_manager: crate::collaboration::CollaborationManager::new(&app_dir),
        };
        // All real routes, but with an explicit peer address so loopback and
        // remote policies can be exercised without external interfaces.
        let app = crate::routes::app(state.clone());
        // A remote policy needs a real bindable non-loopback source address;
        // never spoof ConnectInfo or bind an unroutable documentation address.
        let listener = tokio::net::TcpListener::bind(SocketAddr::new(addr, 0))
            .await
            .unwrap_or_else(|error| {
                panic!(
                    "test requires bindable address {addr}: {error}; configure a non-loopback interface"
                )
            });
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = tokio::spawn(async move {
            axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
                .await
                .unwrap()
        });
        Self {
            state,
            server,
            base,
            client: reqwest::Client::builder()
                .local_address(Some(addr))
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(3))
                .build()
                .unwrap(),
        }
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.client.get(format!("{}{path}", self.base))
    }

    fn cookie(&self) -> String {
        let cookie = login::issue_session_cookie("auth-test", &self.state).unwrap();
        format!("{}={cookie}", login::SESSION_COOKIE_NAME)
    }
}

impl Drop for TestApp {
    fn drop(&mut self) {
        self.server.abort();
        let _ = fs::remove_dir_all(&self.state.app_dir);
    }
}

#[tokio::test]
async fn management_routes_require_auth_for_remote_and_loopback_callers() {
    tokio::time::timeout(Duration::from_secs(15), async {
        for addr in management_test_addresses() {
            clear_unauth_failures(addr);
            let app = TestApp::start(addr).await;
            for path in [
                "/api/terminal/sessions",
                "/api/proxy/presets",
                "/api/quota/config",
                "/api/auth/api-presets",
            ] {
                let response = app.get(path).send().await.unwrap();
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{addr} {path}");
                assert!(response.headers().get(LOCATION).is_none());
                let body: serde_json::Value = response.json().await.unwrap();
                assert!(body.is_object(), "API errors must be JSON");
            }
            // Mutating endpoints must reject before any handler side effects.
            for path in [
                "/api/service/deploy",
                "/api/build/compile",
                "/api/system/restart",
                "/api/terminal/sessions/s1/close",
            ] {
                let response = app
                    .client
                    .post(format!("{}{path}", app.base))
                    .json(&serde_json::json!({}))
                    .send()
                    .await
                    .unwrap();
                assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{addr} {path}");
            }
            let response = app
                .get("/api/proxy/presets")
                .header(COOKIE, app.cookie())
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK, "valid session at {addr}");
            for token in ["wrong", "test-local-api-token"] {
                let response = app
                    .get("/api/proxy/presets")
                    .header(LOCAL_API_TOKEN_HEADER, token)
                    .header("X-Forwarded-For", "127.0.0.1")
                    .send()
                    .await
                    .unwrap();
                let allowed = addr.is_loopback() && token == app.state.local_api_token.as_ref();
                assert_eq!(
                    response.status(),
                    if allowed {
                        StatusCode::OK
                    } else {
                        StatusCode::UNAUTHORIZED
                    }
                );
            }
            assert_eq!(
                app.get("/api/proxy/presets")
                    .header(COOKIE, format!("{}=invalid", login::SESSION_COOKIE_NAME),)
                    .send()
                    .await
                    .unwrap()
                    .status(),
                StatusCode::UNAUTHORIZED
            );
            assert_eq!(app.get("/").send().await.unwrap().status(), StatusCode::SEE_OTHER);
            assert_eq!(app.get("/api/auth/session").send().await.unwrap().status(), StatusCode::OK);
        }
    })
    .await
    .expect("auth HTTP regression timed out");
}

#[tokio::test]
async fn gateway_keeps_its_own_address_policy() {
    tokio::time::timeout(Duration::from_secs(10), async {
        for addr in gateway_test_addresses() {
            clear_unauth_failures(addr);
            let app = TestApp::start(addr).await;
            let response = app
                .get("/api/upstream/openai/v1/models")
                .send()
                .await
                .unwrap();
            if !addr.is_loopback() {
                assert_eq!(response.status(), StatusCode::FORBIDDEN);
            } else {
                // No upstream preset in the fixture: resolution fails without
                // calling a provider, but management authentication is bypassed.
                assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            }
            let response = app
                .get("/api/upstream/admin-not-a-gateway")
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "no broad prefix exemptions");
        }
    })
    .await
    .expect("gateway HTTP regression timed out");
}

#[tokio::test]
async fn unauthenticated_api_requests_trigger_rate_limit_circuit_breaker() {
    tokio::time::timeout(Duration::from_secs(15), async {
        let test_ip = Ipv4Addr::new(127, 0, 0, 99);
        clear_unauth_failures(test_ip.into());
        let app = TestApp::start(test_ip.into()).await;
        // 前 9 次未认证请求返回 401
        for i in 1..=9 {
            let response = app.get("/api/terminal/sessions").send().await.unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "request {i} must return 401");
        }

        // 第 10 次请求触发熔断，返回 429 Too Many Requests
        let response = app.get("/api/terminal/sessions").send().await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "10th unauthenticated request must trigger 429 circuit breaker"
        );
        assert!(
            response
                .headers()
                .get(axum::http::header::RETRY_AFTER)
                .is_some()
        );

        // 随后同一 IP 的任何未认证 API 探测均直接返回 429
        let response = app.get("/api/proxy/presets").send().await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::TOO_MANY_REQUESTS,
            "subsequent probe must be blocked with 429"
        );

        // 使用有效会话登录访问，应当正常通过并重置熔断计数
        let response = app
            .get("/api/proxy/presets")
            .header(COOKIE, app.cookie())
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "authenticated request must succeed and clear failures"
        );

        // 熔断清除后，下一次单次未认证探测恢复为正常的 401
        let response = app.get("/api/terminal/sessions").send().await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "after clear, single failure returns 401 instead of 429"
        );
    })
    .await
    .expect("rate limit circuit breaker test timed out");
}

fn management_test_addresses() -> Vec<IpAddr> {
    let mut addresses = vec![Ipv4Addr::LOCALHOST.into(), Ipv6Addr::LOCALHOST.into()];
    addresses.extend(
        local_non_loopback_addresses()
            .into_iter()
            .take(1)
            .map(IpAddr::from),
    );
    addresses
}

fn gateway_test_addresses() -> Vec<IpAddr> {
    local_non_loopback_addresses()
        .into_iter()
        .take(1)
        .chain([Ipv4Addr::LOCALHOST.into()])
        .collect()
}

fn local_non_loopback_addresses() -> Vec<IpAddr> {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").expect("create route probe socket");
    // UDP connect chooses the real route's source address without sending a
    // packet, which is safer than enumerating interfaces with another crate.
    socket
        .connect("192.0.2.1:9")
        .ok()
        .and_then(|_| socket.local_addr().ok())
        .map(|address| vec![address.ip()])
        .unwrap_or_default()
}
