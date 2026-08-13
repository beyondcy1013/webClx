use std::{
    io::{Cursor, Read},
    path::{Component, Path},
};

use anyhow::{Context, Result};
use auth_core::{ImportedAccount, parse_imported_accounts};
use flate2::read::GzDecoder;
use zip::ZipArchive;

pub(crate) const API_ACCOUNT_IMPORT_MAX_UPLOAD_BYTES: usize = 32 * 1024 * 1024;
pub(crate) const API_ACCOUNT_IMPORT_MAX_ARCHIVE_DEPTH: usize = 12;
const API_ACCOUNT_IMPORT_MAX_EXPANDED_BYTES: usize = 128 * 1024 * 1024;
const API_ACCOUNT_IMPORT_MAX_ENTRY_BYTES: usize = 64 * 1024 * 1024;
const API_ACCOUNT_IMPORT_MAX_ENTRIES: usize = 2_048;

#[derive(Debug)]
pub(super) struct CollectedAccountImport {
    pub(super) accounts: Vec<ImportedAccount>,
    pub(super) errors: Vec<String>,
}

#[derive(Default)]
struct ImportBudget {
    entries: usize,
    expanded_bytes: usize,
}

#[cfg(test)]
pub(super) fn collect_accounts_from_upload(
    file_name: &str,
    bytes: &[u8],
) -> Result<CollectedAccountImport> {
    collect_accounts_from_uploads([(file_name, bytes)])
}

pub(super) fn collect_accounts_from_uploads<'a>(
    uploads: impl IntoIterator<Item = (&'a str, &'a [u8])>,
) -> Result<CollectedAccountImport> {
    let mut has_upload = false;
    let mut upload_bytes = 0_usize;

    let mut imported = CollectedAccountImport {
        accounts: Vec::new(),
        errors: Vec::new(),
    };
    let mut budget = ImportBudget::default();
    for (file_name, bytes) in uploads {
        has_upload = true;
        upload_bytes = upload_bytes
            .checked_add(bytes.len())
            .ok_or_else(|| anyhow::anyhow!("导入文件总大小溢出。"))?;
        if upload_bytes > API_ACCOUNT_IMPORT_MAX_UPLOAD_BYTES {
            anyhow::bail!("导入文件总大小不能超过 32 MiB。");
        }
        budget.reserve_entry()?;
        if bytes.is_empty() {
            imported.errors.push(format!("{file_name}: 导入文件为空。"));
            continue;
        }
        process_payload(file_name, bytes, 0, &mut budget, &mut imported)?;
    }

    if !has_upload {
        anyhow::bail!("请选择导入文件。");
    }

    if imported.accounts.is_empty() {
        if imported.errors.is_empty() {
            anyhow::bail!("文件中没有找到可导入的账号 JSON。");
        }
        anyhow::bail!(
            "文件中没有可导入账号：{}",
            imported
                .errors
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("；")
        );
    }

    Ok(imported)
}

fn process_payload(
    name: &str,
    bytes: &[u8],
    depth: usize,
    budget: &mut ImportBudget,
    imported: &mut CollectedAccountImport,
) -> Result<()> {
    if depth > API_ACCOUNT_IMPORT_MAX_ARCHIVE_DEPTH {
        anyhow::bail!("压缩包嵌套层数不能超过 {} 层。", API_ACCOUNT_IMPORT_MAX_ARCHIVE_DEPTH);
    }

    let lower_name = name.to_ascii_lowercase();
    if is_zip(bytes) || lower_name.ends_with(".zip") {
        return process_zip(name, bytes, depth, budget, imported);
    }
    if is_gzip(bytes) || lower_name.ends_with(".gz") || lower_name.ends_with(".tgz") {
        return process_gzip(name, bytes, depth, budget, imported);
    }
    if is_tar(bytes) || lower_name.ends_with(".tar") {
        return process_tar(name, bytes, depth, budget, imported);
    }
    if lower_name.ends_with(".json") || looks_like_json(bytes) {
        process_json(name, bytes, imported);
        return Ok(());
    }

    if depth == 0 {
        anyhow::bail!("只支持 JSON、ZIP、TAR、TAR.GZ、TGZ 或 GZ 文件。");
    }
    Ok(())
}

fn process_zip(
    name: &str,
    bytes: &[u8],
    depth: usize,
    budget: &mut ImportBudget,
    imported: &mut CollectedAccountImport,
) -> Result<()> {
    let mut archive =
        ZipArchive::new(Cursor::new(bytes)).with_context(|| format!("无法打开 ZIP：{name}"))?;
    for index in 0..archive.len() {
        let mut entry = archive
            .by_index(index)
            .with_context(|| format!("无法读取 ZIP 条目：{name}#{index}"))?;
        if entry.is_dir() {
            continue;
        }
        budget.reserve_entry()?;
        let path = entry
            .enclosed_name()
            .ok_or_else(|| anyhow::anyhow!("ZIP 包含不安全路径：{}", entry.name()))?;
        ensure_safe_archive_path(&path)?;
        let entry_name = nested_name(name, &path);
        let entry_bytes = read_expanded(&mut entry, &entry_name, budget)?;
        process_payload(&entry_name, &entry_bytes, depth + 1, budget, imported)?;
    }
    Ok(())
}

fn process_gzip(
    name: &str,
    bytes: &[u8],
    depth: usize,
    budget: &mut ImportBudget,
    imported: &mut CollectedAccountImport,
) -> Result<()> {
    budget.reserve_entry()?;
    let inner_name = gzip_inner_name(name);
    let mut decoder = GzDecoder::new(bytes);
    let inner_bytes = read_expanded(&mut decoder, &inner_name, budget)
        .with_context(|| format!("无法解压 GZ：{name}"))?;
    process_payload(&inner_name, &inner_bytes, depth + 1, budget, imported)
}

fn process_tar(
    name: &str,
    bytes: &[u8],
    depth: usize,
    budget: &mut ImportBudget,
    imported: &mut CollectedAccountImport,
) -> Result<()> {
    let mut archive = tar::Archive::new(Cursor::new(bytes));
    let entries = archive
        .entries()
        .with_context(|| format!("无法打开 TAR：{name}"))?;
    for (index, entry) in entries.enumerate() {
        let mut entry = entry.with_context(|| format!("无法读取 TAR 条目：{name}#{index}"))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        budget.reserve_entry()?;
        let path = entry
            .path()
            .with_context(|| format!("TAR 条目路径无效：{name}#{index}"))?
            .into_owned();
        ensure_safe_archive_path(&path)?;
        let entry_name = nested_name(name, &path);
        let entry_bytes = read_expanded(&mut entry, &entry_name, budget)?;
        process_payload(&entry_name, &entry_bytes, depth + 1, budget, imported)?;
    }
    Ok(())
}

fn process_json(name: &str, bytes: &[u8], imported: &mut CollectedAccountImport) {
    let result = std::str::from_utf8(bytes)
        .map_err(|error| anyhow::anyhow!("不是 UTF-8 文本: {error}"))
        .and_then(parse_imported_accounts);
    match result {
        Ok(accounts) => imported.accounts.extend(accounts),
        Err(error) => imported.errors.push(format!("{name}: {error}")),
    }
}

impl ImportBudget {
    fn reserve_entry(&mut self) -> Result<()> {
        self.entries += 1;
        if self.entries > API_ACCOUNT_IMPORT_MAX_ENTRIES {
            anyhow::bail!("压缩包条目数不能超过 {API_ACCOUNT_IMPORT_MAX_ENTRIES} 个。");
        }
        Ok(())
    }
}

fn read_expanded<R: Read>(
    reader: &mut R,
    name: &str,
    budget: &mut ImportBudget,
) -> Result<Vec<u8>> {
    let total_remaining =
        API_ACCOUNT_IMPORT_MAX_EXPANDED_BYTES.saturating_sub(budget.expanded_bytes);
    let limit = total_remaining.min(API_ACCOUNT_IMPORT_MAX_ENTRY_BYTES);
    if limit == 0 {
        anyhow::bail!("压缩包展开后的总大小不能超过 128 MiB。");
    }

    let mut bytes = Vec::new();
    reader
        .take(limit as u64 + 1)
        .read_to_end(&mut bytes)
        .with_context(|| format!("无法读取压缩包条目：{name}"))?;
    if bytes.len() > limit {
        if total_remaining <= API_ACCOUNT_IMPORT_MAX_ENTRY_BYTES {
            anyhow::bail!("压缩包展开后的总大小不能超过 128 MiB。");
        }
        anyhow::bail!("单个压缩包条目不能超过 64 MiB：{name}");
    }
    budget.expanded_bytes += bytes.len();
    Ok(bytes)
}

fn ensure_safe_archive_path(path: &Path) -> Result<()> {
    if path.components().any(|component| {
        matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
    }) {
        anyhow::bail!("压缩包包含不安全路径：{}", path.display());
    }
    Ok(())
}

fn nested_name(parent: &str, path: &Path) -> String {
    format!("{parent}/{}", path.to_string_lossy())
}

fn gzip_inner_name(name: &str) -> String {
    let lower_name = name.to_ascii_lowercase();
    if lower_name.ends_with(".tgz") {
        return format!("{}.tar", &name[..name.len() - 4]);
    }
    if lower_name.ends_with(".gz") {
        let stripped = &name[..name.len() - 3];
        return if stripped.is_empty() {
            "payload".to_string()
        } else {
            stripped.to_string()
        };
    }
    format!("{name}.expanded")
}

fn is_zip(bytes: &[u8]) -> bool {
    matches!(bytes.get(..4), Some(b"PK\x03\x04" | b"PK\x05\x06" | b"PK\x07\x08"))
}

fn is_gzip(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1f, 0x8b])
}

fn is_tar(bytes: &[u8]) -> bool {
    bytes.get(257..262) == Some(b"ustar")
}

fn looks_like_json(bytes: &[u8]) -> bool {
    matches!(
        bytes
            .iter()
            .copied()
            .find(|byte| !byte.is_ascii_whitespace()),
        Some(b'{' | b'[' | b'"')
    )
}
