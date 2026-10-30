use std::{fs::File, io::Read, path::{Path, PathBuf}};

use serde::Serialize;

use crate::command_error::CommandResult;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArchiveEntry {
    path: String,
    size: u64,
    compressed_size: Option<u64>,
    is_directory: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ArchiveDocument {
    format: String,
    entries: Vec<ArchiveEntry>,
    truncated: bool,
}

fn archive_path(path: &str) -> Result<PathBuf, String> {
    const MAX_ARCHIVE_BYTES: u64 = 2 * 1024 * 1024 * 1024;
    let canonical = Path::new(path).canonicalize()
        .map_err(|error| format!("无法访问压缩包：{error}"))?;
    let metadata = canonical.metadata()
        .map_err(|error| format!("无法读取压缩包信息：{error}"))?;
    if !metadata.is_file() { return Err("所选路径不是文件".into()); }
    if metadata.len() > MAX_ARCHIVE_BYTES { return Err("压缩包超过 2 GiB 安全限制".into()); }
    Ok(canonical)
}

fn read_tar_entries<R: Read>(reader: R) -> Result<(Vec<ArchiveEntry>, bool), String> {
    const MAX_ARCHIVE_ENTRIES: usize = 5_000;
    let mut archive = tar::Archive::new(reader);
    let mut entries = Vec::new();
    let mut truncated = false;
    for entry_result in archive.entries().map_err(|error| format!("无法读取 TAR 目录：{error}"))? {
        if entries.len() >= MAX_ARCHIVE_ENTRIES {
            truncated = true;
            break;
        }
        let entry = entry_result.map_err(|error| format!("无法读取 TAR 条目：{error}"))?;
        entries.push(ArchiveEntry {
            path: entry.path().map_err(|error| format!("无法读取 TAR 条目路径：{error}"))?
                .to_string_lossy().into_owned(),
            size: entry.size(),
            compressed_size: None,
            is_directory: entry.header().entry_type().is_dir(),
        });
    }
    Ok((entries, truncated))
}

#[tauri::command]
pub(crate) async fn read_archive(path: String) -> CommandResult<ArchiveDocument> {
    tauri::async_runtime::spawn_blocking(move || read_archive_file(path))
        .await
        .map_err(|error| format!("无法解析压缩包：{error}"))?
}

fn read_archive_file(path: String) -> CommandResult<ArchiveDocument> {
    const MAX_ARCHIVE_ENTRIES: usize = 5_000;
    let canonical = archive_path(&path)?;
    let name = canonical.file_name().and_then(|value| value.to_str())
        .unwrap_or_default().to_ascii_lowercase();
    if name.ends_with(".zip") {
        let file = File::open(&canonical).map_err(|error| format!("无法打开 ZIP：{error}"))?;
        let archive = zip::ZipArchive::new(file)
            .map_err(|error| format!("无法读取 ZIP 目录：{error}"))?;
        let mut entries = Vec::new();
        let truncated = archive.len() > MAX_ARCHIVE_ENTRIES;
        for index in 0..archive.len().min(MAX_ARCHIVE_ENTRIES) {
            let entry = archive.by_index_data(index).map_err(|error| format!("无法读取 ZIP 条目：{error}"))?;
            entries.push(ArchiveEntry {
                path: entry.name().map_err(|error| format!("无法读取 ZIP 条目名称：{error}"))?.into_owned(),
                size: entry.size(),
                compressed_size: Some(entry.compressed_size()),
                is_directory: entry.is_dir(),
            });
        }
        return Ok(ArchiveDocument { format: "ZIP".into(), entries, truncated });
    }
    let file = File::open(&canonical).map_err(|error| format!("无法打开 TAR：{error}"))?;
    let (format, entries, truncated) = if name.ends_with(".tar") {
        let (entries, truncated) = read_tar_entries(file)?;
        ("TAR", entries, truncated)
    } else if name.ends_with(".tgz") || name.ends_with(".tar.gz") {
        let (entries, truncated) = read_tar_entries(flate2::read::GzDecoder::new(file))?;
        ("TAR.GZ", entries, truncated)
    } else {
        return Err("暂不支持此压缩包格式".into());
    };
    Ok(ArchiveDocument { format: format.into(), entries, truncated })
}
