use std::{
    fs::OpenOptions,
    io::Write,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use tauri::{AppHandle, Manager};

use crate::command_error::CommandResult;

fn diagnostic_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_log_dir()
        .map(|path| path.join("diagnostics.jsonl"))
        .map_err(|error| format!("无法定位日志目录：{error}"))
}

pub(crate) fn append(app: &AppHandle, category: &str, code: &str, duration_ms: Option<f64>) {
    let valid = |value: &str| {
        !value.is_empty()
            && value.len() <= 64
            && value.chars().all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
            })
    };
    if !valid(category) || !valid(code) {
        return;
    }
    let Ok(path) = diagnostic_path(app) else { return };
    let Some(parent) = path.parent() else { return };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    if path.metadata().map(|value| value.len() > 256 * 1024).unwrap_or(false) {
        let previous = path.with_extension("jsonl.1");
        let _ = std::fs::remove_file(&previous);
        let _ = std::fs::rename(&path, previous);
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis())
        .unwrap_or_default();
    let entry = serde_json::json!({
        "timestamp": timestamp,
        "category": category,
        "code": code,
        "durationMs": duration_ms.map(|value| (value * 10.0).round() / 10.0),
    });
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{entry}");
    }
}

#[tauri::command]
pub(crate) fn record_diagnostic(
    app: AppHandle,
    category: String,
    code: String,
    duration_ms: Option<f64>,
) {
    append(&app, &category, &code, duration_ms);
}

#[tauri::command]
pub(crate) fn open_log_directory(app: AppHandle) -> CommandResult<()> {
    let log_file = diagnostic_path(&app)?;
    let directory = log_file.parent().ok_or_else(|| "日志目录无效".to_string())?;
    std::fs::create_dir_all(directory).map_err(|error| format!("无法创建日志目录：{error}"))?;

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg(directory);
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg(directory);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(directory);
        command
    };

    command.spawn().map(|_| ()).map_err(|error| format!("无法打开日志目录：{error}").into())
}
