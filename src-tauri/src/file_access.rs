use std::{fs::Metadata, path::{Path, PathBuf}, time::UNIX_EPOCH};

#[cfg(target_os = "windows")]
fn expand_path_environment(path: &str) -> Result<String, String> {
    let mut expanded = String::new();
    let mut remaining = path;
    while let Some(start) = remaining.find('%') {
        expanded.push_str(&remaining[..start]);
        let variable = &remaining[start + 1..];
        let end = variable.find('%')
            .ok_or_else(|| "路径中的环境变量缺少结束百分号".to_string())?;
        let name = &variable[..end];
        let value = std::env::var(name)
            .map_err(|_| format!("无法读取环境变量：{name}"))?;
        expanded.push_str(&value);
        remaining = &variable[end + 1..];
    }
    expanded.push_str(remaining);
    Ok(expanded)
}

#[tauri::command]
pub(crate) fn resolve_pasted_path(path: String) -> crate::command_error::CommandResult<String> {
    if path.contains(['\r', '\n']) {
        return Err("仅支持单行文件路径".into());
    }
    #[cfg(target_os = "windows")]
    let path = expand_path_environment(&path)?;
    if !Path::new(&path).is_absolute() {
        return Err("请提供当前系统支持的绝对文件路径".into());
    }
    let (canonical, _) = local_file_metadata(&path)?;
    Ok(display_path(&canonical))
}

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
