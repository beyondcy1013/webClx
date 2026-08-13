//! In-memory cache of upstream quota-reset times captured by the API proxy.
//!
//! Codex drops the body of a 429 response and only prints the status line
//! (`exceeded retry limit, last status: 429 ...`), so the terminal tail never
//! contains the real "限额将在 {time} 重置" text that the auto-continue time
//! patterns rely on. The upstream proxy (`upstream_proxy.rs`) instead sees the
//! full Zhipu error body and records the parsed reset time here, keyed by the
//! preset id of the request. The terminal error auto-continue scanner then
//! reads this authoritative value when the terminal text has none.
//!
//! See `docs/codex/tasks/terminal-session-activity.md` and the API preset
//! routing boundaries doc for why reset-time capture lives in the proxy layer
//! rather than being inferred from terminal output.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;
use std::sync::Mutex;

/// Entries older than this are considered stale and ignored. Quota windows are
/// at most a few hours, so a few hours of retention is plenty.
const MAX_ENTRY_AGE: Duration = Duration::from_secs(6 * 60 * 60);

#[derive(Debug, Clone)]
struct CachedReset {
    reset_at: String,
    captured_at: Instant,
}

/// Shared map of preset id -> most recent parsed quota reset time.
///
/// A single preset/account quota reset applies to every terminal using that
/// preset, so reads must be non-consuming. Per-session schedule dedupe is
/// handled later with the terminal session id and error signature.
#[derive(Clone, Default)]
pub struct QuotaResetCache {
    inner: Arc<Mutex<HashMap<String, CachedReset>>>,
}

impl QuotaResetCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a parsed reset time by both preset id and upstream base URL.
    pub fn record_for_preset(&self, preset_id: &str, base_url: &str, reset_at: String) {
        self.record_key(preset_id, reset_at.clone());
        if let Some(key) = base_url_cache_key(base_url) {
            self.record_key(&key, reset_at);
        }
    }

    fn record_key(&self, key: &str, reset_at: String) {
        if key.trim().is_empty() || reset_at.trim().is_empty() {
            return;
        }
        if let Ok(mut guard) = self.inner.lock() {
            guard.insert(
                key.to_string(),
                CachedReset {
                    reset_at,
                    captured_at: Instant::now(),
                },
            );
        }
    }

    /// Return the reset time for a preset if one is still fresh.
    pub fn get_for_preset(&self, preset_id: &str) -> Option<String> {
        self.get_for_key(preset_id)
    }

    /// Return the reset time for a base URL if one is still fresh.
    pub fn get_for_base_url(&self, base_url: &str) -> Option<String> {
        let key = base_url_cache_key(base_url)?;
        self.get_for_key(&key)
    }

    fn get_for_key(&self, key: &str) -> Option<String> {
        let guard = self.inner.lock().ok()?;
        let entry = guard.get(key)?;
        if entry.captured_at.elapsed() > MAX_ENTRY_AGE {
            return None;
        }
        Some(entry.reset_at.clone())
    }
}

fn base_url_cache_key(base_url: &str) -> Option<String> {
    let normalized = base_url.trim().trim_end_matches('/').to_ascii_lowercase();
    (!normalized.is_empty()).then(|| format!("base_url:{normalized}"))
}

/// Returns true when the base url belongs to a Zhipu / BigModel upstream.
/// Reuses the same host matching the quota dialog and the Codex_API auto-proxy
/// provider list use (`open.bigmodel.cn`), generalized to any bigmodel.cn host.
pub fn base_url_is_zhipu_upstream(base_url: &str) -> bool {
    let normalized = base_url.trim().to_ascii_lowercase();
    normalized.contains("bigmodel.cn")
        || normalized.contains("/api/codex-proxy/zhipu/")
        || normalized.contains("bigmodel")
        || normalized.contains("zhipu")
}

/// Parse the Zhipu quota-limit reset time out of an upstream 429 body.
///
/// Example body: `{"error":{"code":"1308","message":"已达到 5 小时的使用上限。您的限额将在 2026-07-05 02:16:19 重置。"}}`
/// Returns the datetime string (`2026-07-05 02:16:19`) when the body is a
/// Zhipu quota-exceeded error with a reset time, else None.
pub fn parse_zhipu_quota_reset(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    let err = value.get("error")?;
    let code = err.get("code").and_then(Value::as_str).unwrap_or("");
    let message = err.get("message").and_then(Value::as_str).unwrap_or("");
    // 1308 is Zhipu's quota-exceeded code; also accept the Chinese text in case
    // the code shape changes, since the message is the authoritative source.
    let is_quota_error = code == "1308"
        || message.contains("使用上限")
        || message.contains("额度")
        || message.contains("限额");
    if !is_quota_error {
        return None;
    }
    extract_reset_datetime(message)
}

/// Extract a `YYYY-MM-DD HH:MM:SS` datetime substring from a message.
fn extract_reset_datetime(message: &str) -> Option<String> {
    // Scan for a 19-char run matching YYYY-MM-DD HH:MM:SS.
    let chars: Vec<char> = message.chars().collect();
    if chars.len() < 19 {
        return None;
    }
    let pattern = [
        true, true, true, true, false, true, true, false, true, true, false, true, true, false,
        true, true, false, true, true,
    ];
    // pattern[k] == true => digit expected at this position; false => separator.
    // separators: index 4='-', 7='-', 10=' ', 13=':', 16=':'.
    let sep_at = |k: usize| -> char {
        match k {
            4 | 7 => '-',
            10 => ' ',
            13 | 16 => ':',
            _ => '\u{0}',
        }
    };
    for start in 0..=(chars.len() - 19) {
        let mut ok = true;
        for (k, ch) in chars[start..start + 19].iter().enumerate() {
            if pattern[k] {
                if !ch.is_ascii_digit() {
                    ok = false;
                    break;
                }
            } else if *ch != sep_at(k) {
                ok = false;
                break;
            }
        }
        if ok {
            // Require a non-digit boundary after, so we don't truncate a longer
            // digit run.
            let after = chars.get(start + 19).copied().unwrap_or(' ');
            if !after.is_ascii_digit() {
                return Some(chars[start..start + 19].iter().collect());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_zhipu_host() {
        assert!(base_url_is_zhipu_upstream("https://open.bigmodel.cn/api/coding/paas/v4"));
        assert!(base_url_is_zhipu_upstream("https://Open.BIGMODEL.cn/"));
        assert!(!base_url_is_zhipu_upstream("https://api.deepseek.com"));
    }

    #[test]
    fn parses_zhipu_1308_reset() {
        let body = r#"{"error":{"code":"1308","message":"已达到 5 小时的使用上限。您的限额将在 2026-07-05 02:16:19 重置。"}}"#;
        assert_eq!(parse_zhipu_quota_reset(body), Some("2026-07-05 02:16:19".to_string()));
    }

    #[test]
    fn ignores_non_quota_429() {
        let body = r#"{"error":{"code":"429","message":"Too Many Requests"}}"#;
        assert_eq!(parse_zhipu_quota_reset(body), None);
    }

    #[test]
    fn cache_get_is_shareable_across_sessions() {
        let cache = QuotaResetCache::new();
        cache.record_for_preset("p1", "", "2026-07-05 02:16:19".to_string());
        assert_eq!(cache.get_for_preset("p1"), Some("2026-07-05 02:16:19".to_string()));
        assert_eq!(cache.get_for_preset("p1"), Some("2026-07-05 02:16:19".to_string()));
    }

    #[test]
    fn cache_records_base_url_fallback() {
        let cache = QuotaResetCache::new();
        cache.record_for_preset(
            "p1",
            "https://open.bigmodel.cn/api/coding/paas/v4/",
            "2026-07-05 02:16:19".to_string(),
        );
        assert_eq!(cache.get_for_preset("p1"), Some("2026-07-05 02:16:19".to_string()));
        assert_eq!(
            cache.get_for_base_url("https://OPEN.bigmodel.cn/api/coding/paas/v4"),
            Some("2026-07-05 02:16:19".to_string())
        );
    }
}
