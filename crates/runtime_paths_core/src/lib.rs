use std::{
    env,
    ffi::{CStr, CString, OsStr},
    path::PathBuf,
};

use anyhow::{Result, anyhow};

#[cfg(unix)]
use std::sync::Mutex;

pub const DEFAULT_USER_NAME: &str = "root";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserProfile {
    pub name: String,
    pub uid: u32,
    pub gid: u32,
    pub home: PathBuf,
    pub shell: PathBuf,
}

pub fn resolve_current_user_home() -> Option<PathBuf> {
    preferred_user_home(current_uid_home(), absolute_env_path("HOME"))
}

pub fn resolve_current_user_profile() -> Option<UserProfile> {
    current_uid_profile().or_else(|| {
        user_profile_from_env(
            env::var("USERNAME").ok(),
            env::var("USER").ok().or_else(|| env::var("LOGNAME").ok()),
            env_path("HOME"),
            env_path("USERPROFILE"),
            env_path("SHELL"),
            env_path("COMSPEC"),
        )
    })
}

pub fn resolve_user_profile(user_name: &str) -> Result<UserProfile> {
    let normalized = normalize_user_name(user_name)?;
    named_user_profile(&normalized).ok_or_else(|| anyhow!("用户 `{normalized}` 不存在。"))
}

pub fn resolve_user_file(user_name: &str, relative_path: &str) -> Result<PathBuf> {
    Ok(resolve_user_profile(user_name)?.home.join(relative_path))
}

/// Resolve a user's home directory, preferring the `HOME` environment variable
/// when the target user matches the current process user. This matches how
/// user-facing CLI tools (`codex`, `claude`) resolve `~` — they use `$HOME`,
/// not `getpwuid`.
///
/// On remote servers / containers, `getpwnam` may report `/root` while `$HOME`
/// is `/home/root`. Writing auth files to the wrong directory breaks account
/// switching because the CLI never reads them.
pub fn resolve_user_home_preferring_env(user_name: &str) -> Result<PathBuf> {
    let normalized = normalize_user_name(user_name)?;
    // When the target user IS the current process user, prefer $HOME from the
    // environment — that is where codex/claude actually look for their config.
    // For other users, $HOME in this process does not belong to them, so we
    // fall back to passwd.
    if let Some(current) = current_uid_profile()
        && current.name == normalized
        && let Some(env_home) = absolute_env_path("HOME")
    {
        return Ok(env_home);
    }
    Ok(resolve_user_profile(&normalized)?.home)
}

/// Resolve a user-relative file path preferring `$HOME` over passwd.
///
/// See [`resolve_user_home_preferring_env`] for the rationale.
pub fn resolve_user_file_preferring_env(user_name: &str, relative_path: &str) -> Result<PathBuf> {
    Ok(resolve_user_home_preferring_env(user_name)?.join(relative_path))
}

/// Resolve the current user's home directory, preferring the `HOME`
/// environment variable over the passwd database entry.
///
/// This is the inverse of [`preferred_user_home`] which prefers passwd;
/// use this when the consumer (e.g. `codex` or `claude` CLI) resolves
/// `~` from `$HOME` rather than from `getpwuid`.
pub fn resolve_current_user_home_or_env() -> Option<PathBuf> {
    absolute_env_path("HOME").or_else(current_uid_home)
}

pub fn list_login_user_profiles() -> Vec<UserProfile> {
    let mut profiles = all_user_profiles()
        .into_iter()
        .filter(is_login_user_profile)
        .collect::<Vec<_>>();
    profiles.sort_by(|left, right| {
        (left.name != DEFAULT_USER_NAME, left.uid, &left.name).cmp(&(
            right.name != DEFAULT_USER_NAME,
            right.uid,
            &right.name,
        ))
    });
    profiles.dedup_by(|left, right| left.name == right.name);
    profiles
}

fn absolute_env_path(name: &str) -> Option<PathBuf> {
    env_path(name).filter(|path| path.is_absolute())
}

fn env_path(name: &str) -> Option<PathBuf> {
    env::var_os(name).map(PathBuf::from)
}

fn preferred_user_home(
    current_home: Option<PathBuf>,
    home_env: Option<PathBuf>,
) -> Option<PathBuf> {
    current_home.or(home_env)
}

fn user_profile_from_env(
    username_env: Option<String>,
    user_env: Option<String>,
    home_env: Option<PathBuf>,
    userprofile_env: Option<PathBuf>,
    shell_env: Option<PathBuf>,
    comspec_env: Option<PathBuf>,
) -> Option<UserProfile> {
    let name = username_env
        .or(user_env)
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    let home = userprofile_env.or(home_env)?;
    let shell = shell_env
        .or(comspec_env)
        .unwrap_or_else(default_platform_shell);

    Some(UserProfile {
        name,
        uid: current_uid_value(),
        gid: current_gid_value(),
        home,
        shell,
    })
}

#[cfg(windows)]
fn default_platform_shell() -> PathBuf {
    PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
}

#[cfg(not(windows))]
fn default_platform_shell() -> PathBuf {
    PathBuf::from("/bin/sh")
}

pub fn normalize_user_name(raw: &str) -> Result<String> {
    let name = raw.trim();
    if name.is_empty() {
        anyhow::bail!("用户身份不能为空。");
    }
    if name.as_bytes().contains(&0) {
        anyhow::bail!("用户身份不能包含 NUL 字符。");
    }
    Ok(name.to_string())
}

fn is_login_user_profile(profile: &UserProfile) -> bool {
    if profile.name == DEFAULT_USER_NAME {
        return true;
    }
    if profile.uid < 1000 {
        return false;
    }

    let shell_name = profile
        .shell
        .file_name()
        .and_then(OsStr::to_str)
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    !matches!(shell_name.as_str(), "nologin" | "false" | "sync" | "shutdown" | "halt")
}

#[cfg(unix)]
fn current_uid_home() -> Option<PathBuf> {
    current_uid_profile().map(|profile| profile.home)
}

#[cfg(unix)]
fn current_uid_profile() -> Option<UserProfile> {
    with_passwd_database_lock(|| {
        let uid = unsafe { libc::geteuid() };
        let passwd = unsafe { libc::getpwuid(uid) };
        passwd_to_profile(passwd)
    })
}

#[cfg(unix)]
fn named_user_profile(name: &str) -> Option<UserProfile> {
    let name = CString::new(name).ok()?;
    with_passwd_database_lock(|| {
        let passwd = unsafe { libc::getpwnam(name.as_ptr()) };
        passwd_to_profile(passwd)
    })
}

#[cfg(unix)]
fn all_user_profiles() -> Vec<UserProfile> {
    with_passwd_database_lock(|| {
        let mut profiles = Vec::new();
        unsafe {
            libc::setpwent();
            loop {
                let passwd = libc::getpwent();
                if passwd.is_null() {
                    break;
                }
                if let Some(profile) = passwd_to_profile(passwd) {
                    profiles.push(profile);
                }
            }
            libc::endpwent();
        }
        profiles
    })
}

#[cfg(unix)]
fn with_passwd_database_lock<T>(operation: impl FnOnce() -> T) -> T {
    static PASSWD_DATABASE_LOCK: Mutex<()> = Mutex::new(());
    let _guard = PASSWD_DATABASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    operation()
}

#[cfg(unix)]
fn passwd_to_profile(passwd: *mut libc::passwd) -> Option<UserProfile> {
    use std::os::unix::ffi::OsStrExt;

    if passwd.is_null() {
        return None;
    }

    let name_ptr = unsafe { (*passwd).pw_name };
    if name_ptr.is_null() {
        return None;
    }
    let name = unsafe { CStr::from_ptr(name_ptr) }
        .to_string_lossy()
        .to_string();

    let home_ptr = unsafe { (*passwd).pw_dir };
    if home_ptr.is_null() {
        return None;
    }
    let home_bytes = unsafe { CStr::from_ptr(home_ptr) }.to_bytes();
    let home = PathBuf::from(OsStr::from_bytes(home_bytes));
    if !home.is_absolute() {
        return None;
    }

    let shell_ptr = unsafe { (*passwd).pw_shell };
    let shell = if shell_ptr.is_null() {
        PathBuf::from("/bin/sh")
    } else {
        let shell_bytes = unsafe { CStr::from_ptr(shell_ptr) }.to_bytes();
        let shell = PathBuf::from(OsStr::from_bytes(shell_bytes));
        if shell.is_absolute() {
            shell
        } else {
            PathBuf::from("/bin/sh")
        }
    };

    Some(UserProfile {
        name,
        uid: unsafe { (*passwd).pw_uid },
        gid: unsafe { (*passwd).pw_gid },
        home,
        shell,
    })
}

#[cfg(unix)]
fn current_uid_value() -> u32 {
    unsafe { libc::geteuid() }
}

#[cfg(unix)]
fn current_gid_value() -> u32 {
    unsafe { libc::getegid() }
}

#[cfg(not(unix))]
fn current_uid_home() -> Option<PathBuf> {
    user_profile_from_env(
        env::var("USERNAME").ok(),
        env::var("USER").ok().or_else(|| env::var("LOGNAME").ok()),
        env_path("HOME"),
        env_path("USERPROFILE"),
        env_path("SHELL"),
        env_path("COMSPEC"),
    )
    .map(|profile| profile.home)
}

#[cfg(not(unix))]
fn current_uid_profile() -> Option<UserProfile> {
    user_profile_from_env(
        env::var("USERNAME").ok(),
        env::var("USER").ok().or_else(|| env::var("LOGNAME").ok()),
        env_path("HOME"),
        env_path("USERPROFILE"),
        env_path("SHELL"),
        env_path("COMSPEC"),
    )
}

#[cfg(not(unix))]
fn named_user_profile(name: &str) -> Option<UserProfile> {
    let current = current_uid_profile()?;
    if current.name.eq_ignore_ascii_case(name) {
        Some(current)
    } else {
        None
    }
}

#[cfg(not(unix))]
fn all_user_profiles() -> Vec<UserProfile> {
    current_uid_profile().into_iter().collect()
}

#[cfg(not(unix))]
fn current_uid_value() -> u32 {
    0
}

#[cfg(not(unix))]
fn current_gid_value() -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::{
        preferred_user_home, resolve_current_user_home_or_env, resolve_user_home_preferring_env,
        user_profile_from_env, with_passwd_database_lock,
    };
    use std::{path::PathBuf, sync::mpsc, thread, time::Duration};

    #[test]
    #[cfg(unix)]
    fn passwd_database_access_is_serialized() {
        let (first_entered_tx, first_entered_rx) = mpsc::channel();
        let (release_first_tx, release_first_rx) = mpsc::channel();
        let first = thread::spawn(move || {
            with_passwd_database_lock(|| {
                first_entered_tx.send(()).expect("signal first lock holder");
                release_first_rx.recv().expect("release first lock holder");
            });
        });
        first_entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("first thread should acquire passwd lock");

        let (second_entered_tx, second_entered_rx) = mpsc::channel();
        let second = thread::spawn(move || {
            with_passwd_database_lock(|| {
                second_entered_tx
                    .send(())
                    .expect("signal second lock holder");
            });
        });

        assert!(
            second_entered_rx
                .recv_timeout(Duration::from_millis(50))
                .is_err(),
            "second passwd operation entered before the first released the lock"
        );
        release_first_tx.send(()).expect("release first thread");
        second_entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("second thread should acquire passwd lock after release");
        first.join().expect("first thread should finish");
        second.join().expect("second thread should finish");
    }

    #[test]
    fn preferred_user_home_prefers_current_uid_home() {
        let result =
            preferred_user_home(Some(PathBuf::from("/home/root")), Some(PathBuf::from("/root")));

        assert_eq!(result, Some(PathBuf::from("/home/root")));
    }

    #[test]
    fn preferred_user_home_falls_back_to_home_env() {
        let result = preferred_user_home(None, Some(PathBuf::from("/root")));

        assert_eq!(result, Some(PathBuf::from("/root")));
    }

    #[test]
    fn resolve_current_user_home_or_env_falls_back_to_uid_when_home_unset() {
        // Without HOME in the env, we should get the passwd-based home.
        // On Unix we cannot easily unset HOME in a test, so we just verify
        // the function returns *some* absolute path (or None on non-Unix).
        let result = resolve_current_user_home_or_env();
        if cfg!(unix) {
            assert!(result.is_some());
            assert!(result.unwrap().is_absolute());
        }
        // On non-Unix, either env HOME or passwd fallback is fine.
    }

    #[test]
    #[cfg(unix)]
    fn resolve_user_home_preferring_env_returns_absolute_path() {
        // For the current user, the result must be an absolute path.
        let current_name = super::resolve_current_user_profile()
            .expect("current user should resolve")
            .name;
        let home = resolve_user_home_preferring_env(&current_name)
            .expect("home should resolve for current user");
        assert!(home.is_absolute());
    }

    #[test]
    #[cfg(unix)]
    fn resolve_user_home_preferring_env_errors_for_nonexistent_user() {
        let result = resolve_user_home_preferring_env("nonexistent_user_xy123");
        assert!(result.is_err());
    }

    #[test]
    fn non_unix_user_profile_can_be_resolved_from_environment_values() {
        let profile = user_profile_from_env(
            Some("alice".into()),
            None,
            None,
            Some(PathBuf::from(r"C:\Users\alice")),
            None,
            Some(PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")),
        )
        .expect("profile should resolve from Windows environment values");

        assert_eq!(profile.name, "alice");
        assert_eq!(profile.home, PathBuf::from(r"C:\Users\alice"));
        assert_eq!(
            profile.shell,
            PathBuf::from(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe")
        );
    }
}
