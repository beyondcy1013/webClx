use std::{
    fs, io,
    path::{Path, PathBuf},
};

use include_dir::{Dir, DirEntry, include_dir};
use runtime_paths_core::UserProfile;

const MANAGED_MARKER: &str = ".webclx-managed-skill";
const BUNDLED_ROOT_NAME: &str = ".webclx-builtin-skills";
const USER_SKILL_ROOTS: [&str; 3] = [".codex/skills", ".claude/skills", ".dsh/skills"];
static BUILTIN_SKILLS: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/builtin-skills");

pub fn root_dir(app_dir: &Path) -> PathBuf {
    app_dir.join(BUNDLED_ROOT_NAME)
}

pub fn install_for_user(app_dir: &Path, user: &UserProfile) -> io::Result<()> {
    let root = root_dir(app_dir);
    replace_managed_tree(&root, &BUILTIN_SKILLS)?;
    for relative in USER_SKILL_ROOTS {
        let parent = user.home.join(relative);
        fs::create_dir_all(&parent)?;
        for skill in BUILTIN_SKILLS.dirs() {
            let Some(name) = skill.path().file_name() else {
                continue;
            };
            let target = parent.join(name);
            if target.exists() && !target.join(MANAGED_MARKER).is_file() {
                tracing::info!(path = %target.display(), "preserving user-managed Skill");
                continue;
            }
            replace_managed_tree(&target, skill)?;
        }
    }
    set_user_ownership(&root, user)?;
    for relative in USER_SKILL_ROOTS {
        for skill in BUILTIN_SKILLS.dirs() {
            if let Some(name) = skill.path().file_name() {
                let target = user.home.join(relative).join(name);
                if target.join(MANAGED_MARKER).is_file() {
                    set_user_ownership(&target, user)?;
                }
            }
        }
    }
    Ok(())
}

fn replace_managed_tree(target: &Path, source: &Dir<'_>) -> io::Result<()> {
    if target.exists() && !target.join(MANAGED_MARKER).is_file() && target.file_name().is_some() {
        return Ok(());
    }
    let parent = target
        .parent()
        .ok_or_else(|| io::Error::other("Skill target has no parent"))?;
    fs::create_dir_all(parent)?;
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("skills");
    let temporary = parent.join(format!(".{name}.webclx-tmp-{}", std::process::id()));
    let backup = parent.join(format!(".{name}.webclx-old-{}", std::process::id()));
    if temporary.exists() {
        fs::remove_dir_all(&temporary)?;
    }
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    fs::create_dir_all(&temporary)?;
    extract_contents(source, &temporary)?;
    make_scripts_executable(&temporary)?;
    if target.exists() {
        fs::rename(target, &backup)?;
    }
    if let Err(error) = fs::rename(&temporary, target) {
        if backup.exists() {
            let _ = fs::rename(&backup, target);
        }
        return Err(error);
    }
    if backup.exists() {
        fs::remove_dir_all(backup)?;
    }
    Ok(())
}

fn extract_contents(source: &Dir<'_>, target: &Path) -> io::Result<()> {
    for entry in source.entries() {
        let relative = entry
            .path()
            .strip_prefix(source.path())
            .unwrap_or(entry.path());
        let destination = target.join(relative);
        match entry {
            DirEntry::Dir(directory) => {
                fs::create_dir_all(&destination)?;
                extract_contents(directory, &destination)?;
            }
            DirEntry::File(file) => {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::write(destination, file.contents())?;
            }
        }
    }
    Ok(())
}

#[cfg(unix)]
fn make_scripts_executable(root: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    visit_tree(root, &mut |path| {
        if path.components().any(|part| part.as_os_str() == "scripts") && path.is_file() {
            fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        }
        Ok(())
    })
}

#[cfg(not(unix))]
fn make_scripts_executable(_root: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn set_user_ownership(root: &Path, user: &UserProfile) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    visit_tree(root, &mut |path| {
        let encoded = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))?;
        if unsafe { libc::chown(encoded.as_ptr(), user.uid, user.gid) } != 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::PermissionDenied {
                return Err(error);
            }
        }
        Ok(())
    })
}

#[cfg(not(unix))]
fn set_user_ownership(_root: &Path, _user: &UserProfile) -> io::Result<()> {
    Ok(())
}

fn visit_tree(root: &Path, visit: &mut impl FnMut(&Path) -> io::Result<()>) -> io::Result<()> {
    visit(root)?;
    if root.is_dir() {
        for entry in fs::read_dir(root)? {
            visit_tree(&entry?.path(), visit)?;
        }
    }
    Ok(())
}
