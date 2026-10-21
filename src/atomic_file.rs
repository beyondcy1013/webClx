use std::{
    fs, io,
    io::Write,
    path::{Path, PathBuf},
};

use rand::RngCore;

/// Atomically replaces `path` with `contents`: the payload is written to a
/// unique temporary file in the same directory, fsynced, then renamed over the
/// destination so a crash never exposes a truncated file. When `path` already
/// exists its permissions are carried over to the replacement. Parent
/// directories must already exist.
///
/// 目录 fsync 在 rename 之后执行：此处的 Err 只表示替换的持久性尚未
/// 确认，不代表目标不可见——调用方不得据此假设旧内容仍然存在。
pub fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let temporary = temporary_path(path);
    write_then_replace(&temporary, path, contents)
}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("atomic-file");
    let mut random = [0_u8; 8];
    rand::thread_rng().fill_bytes(&mut random);
    path.with_file_name(format!(".{name}.{}-{}.tmp", std::process::id(), hex::encode(random)))
}

/// `create_new` 成功之后才进入清理作用域：open 失败意味着临时文件不是
/// 本次创建的（例如撞名），不得删除。
fn write_then_replace(temporary: &Path, destination: &Path, contents: &[u8]) -> io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary)?;
    let result = (|| {
        if let Some(metadata) = destination_metadata(destination)? {
            fs::set_permissions(temporary, metadata.permissions())?;
        }
        file.write_all(contents)?;
        file.sync_all()?;
        // 先关闭句柄再替换：Windows 不允许 rename 仍被打开的文件。
        drop(file);
        crate::private_file::replace(temporary, destination)?;
        sync_parent_directory(destination)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn destination_metadata(destination: &Path) -> io::Result<Option<fs::Metadata>> {
    match fs::metadata(destination) {
        Ok(metadata) => Ok(Some(metadata)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

#[cfg(unix)]
pub(crate) fn sync_parent_directory(path: &Path) -> io::Result<()> {
    fs::File::open(parent_directory(path))?.sync_all()
}

/// 纯文件名（如 `data.json`）的 `parent()` 返回空路径，映射为 `.`。
#[cfg(unix)]
fn parent_directory(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(not(unix))]
pub(crate) fn sync_parent_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestDirectory(PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let nonce = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let directory = std::env::temp_dir()
                .join(format!("webclx-atomic-file-{label}-{}-{nonce}", std::process::id()));
            fs::create_dir_all(&directory).unwrap();
            Self(directory)
        }

        fn temporary_leftovers(&self) -> Vec<String> {
            fs::read_dir(&self.0)
                .unwrap()
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".tmp"))
                .collect()
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn write_atomic_overwrites_existing_content() {
        let directory = TestDirectory::new("overwrite");
        let path = directory.0.join("data.json");

        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"second");
    }

    #[cfg(unix)]
    #[test]
    fn write_atomic_preserves_existing_permissions() {
        use std::os::unix::fs::PermissionsExt;

        let directory = TestDirectory::new("permissions");
        let path = directory.0.join("data.json");
        fs::write(&path, b"old").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();

        write_atomic(&path, b"new").unwrap();

        assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o640);
        assert_eq!(fs::read(&path).unwrap(), b"new");
    }

    #[test]
    fn write_atomic_leaves_no_temporary_files() {
        let directory = TestDirectory::new("clean");
        let path = directory.0.join("data.json");

        write_atomic(&path, b"contents").unwrap();

        assert_eq!(fs::read(&path).unwrap(), b"contents");
        assert!(directory.temporary_leftovers().is_empty());
    }

    #[test]
    fn write_then_replace_refuses_existing_temporary_and_preserves_it() {
        let directory = TestDirectory::new("temp-collision");
        let temporary = directory.0.join(".data.json.1-00.tmp");
        let destination = directory.0.join("data.json");
        fs::write(&temporary, b"sentinel").unwrap();

        let error = write_then_replace(&temporary, &destination, b"new").unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        // 清理只允许作用于本次 create_new 成功创建的文件：撞名的既有
        // 临时文件必须原样保留。
        assert_eq!(fs::read(&temporary).unwrap(), b"sentinel");
        assert!(!destination.exists());
    }

    #[test]
    fn write_then_replace_failure_removes_only_its_own_temporary() {
        let directory = TestDirectory::new("replace-fails");
        let temporary = directory.0.join(".subdir.1-00.tmp");
        let destination = directory.0.join("subdir");
        fs::create_dir(&destination).unwrap();
        fs::write(destination.join("previous.txt"), b"intact").unwrap();

        // 文件 rename 到已存在的目录必然失败；具体错误种类依平台而定。
        let _error = write_then_replace(&temporary, &destination, b"new").unwrap_err();

        assert!(!temporary.exists());
        assert_eq!(fs::read(destination.join("previous.txt")).unwrap(), b"intact");
    }

    #[cfg(unix)]
    #[test]
    fn parent_directory_falls_back_to_current_directory_for_basename() {
        assert_eq!(parent_directory(Path::new("data.json")), Path::new("."));
        assert_eq!(parent_directory(Path::new("some/dir/data.json")), Path::new("some/dir"));
    }

    #[test]
    fn write_atomic_fails_without_parent_directory() {
        let directory = TestDirectory::new("missing-parent");
        let missing_parent = directory.0.join("missing");
        let path = missing_parent.join("data.json");

        let error = write_atomic(&path, b"contents").unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::NotFound);
        assert!(!missing_parent.exists());
        assert!(directory.temporary_leftovers().is_empty());
    }
}
