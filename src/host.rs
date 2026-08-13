pub fn current_host_name() -> String {
    host_name_from_env()
        .or_else(host_name_from_libc)
        .unwrap_or_else(|| "unknown-host".to_string())
}

fn host_name_from_env() -> Option<String> {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .and_then(|value| normalize_host_name(&value))
}

#[cfg(unix)]
fn host_name_from_libc() -> Option<String> {
    let mut buffer = [0_u8; 256];
    let rc = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) };
    if rc != 0 {
        return None;
    }

    let length = buffer
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(buffer.len());
    let value = String::from_utf8_lossy(&buffer[..length]);
    normalize_host_name(&value)
}

#[cfg(not(unix))]
fn host_name_from_libc() -> Option<String> {
    None
}

fn normalize_host_name(raw: &str) -> Option<String> {
    let trimmed = raw.split(char::from(0)).next().unwrap_or("").trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_host_name;

    #[test]
    fn normalize_host_name_trims_whitespace_and_nul() {
        assert_eq!(normalize_host_name("  openeuler\0\0 "), Some("openeuler".to_string()));
    }

    #[test]
    fn normalize_host_name_rejects_blank_values() {
        assert_eq!(normalize_host_name("  \0 "), None);
    }
}
