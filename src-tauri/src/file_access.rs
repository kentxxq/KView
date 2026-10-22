use std::{fs::Metadata, path::{Path, PathBuf}, time::UNIX_EPOCH};

pub(crate) fn local_file_metadata(path: &str) -> Result<(PathBuf, Metadata), String> {
    let canonical = Path::new(path).canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;
    let metadata = canonical.metadata()
        .map_err(|error| format!("无法读取文件信息：{error}"))?;
    if !metadata.is_file() {
        return Err("所选路径不是文件".into());
    }
    Ok((canonical, metadata))
}

pub(crate) fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
    }
}

pub(crate) fn modified_millis(metadata: &Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}
