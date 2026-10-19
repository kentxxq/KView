use std::{io::Write, sync::Mutex};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

#[cfg(all(desktop, not(debug_assertions)))]
use tauri_plugin_autostart::ManagerExt;

use crate::{command_error::CommandResult, diagnostics};

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AppSettings {
    #[serde(default = "current_schema_version")]
    schema_version: u32,
    pub(crate) theme: String,
    pub(crate) residency_mode: String,
    #[serde(default = "default_close_behavior")]
    pub(crate) close_behavior: String,
    pub(crate) autostart: bool,
    #[serde(default = "default_large_file_threshold_mb")]
    pub(crate) large_file_threshold_mb: u64,
}

fn current_schema_version() -> u32 { 3 }
fn default_close_behavior() -> String { "keep-tabs".into() }
fn default_large_file_threshold_mb() -> u64 { 2 }

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            schema_version: current_schema_version(),
            theme: "system".into(),
            residency_mode: "fast".into(),
            close_behavior: default_close_behavior(),
            autostart: true,
            large_file_threshold_mb: default_large_file_threshold_mb(),
        }
    }
}

#[derive(Default)]
pub(crate) struct SettingsState(pub(crate) Mutex<AppSettings>);

pub(crate) fn path(app: &AppHandle) -> Result<std::path::PathBuf, String> {
    app.path().app_config_dir()
        .map(|path| path.join("settings.json"))
        .map_err(|error| format!("无法定位设置目录：{error}"))
}

pub(crate) fn read(app: &AppHandle) -> Result<AppSettings, String> {
    let path = path(app)?;
    if !path.exists() { return Ok(AppSettings::default()); }
    let content = std::fs::read_to_string(path).map_err(|error| format!("无法读取设置：{error}"))?;
    let value: serde_json::Value = serde_json::from_str(&content)
        .map_err(|error| format!("设置文件已损坏：{error}"))?;
    let mut needs_migration = value.get("schemaVersion").is_none()
        || value.get("closeBehavior").is_none()
        || value.get("largeFileThresholdMb").is_none();
    let mut settings: AppSettings = serde_json::from_value(value)
        .map_err(|error| format!("设置文件已损坏：{error}"))?;
    if settings.residency_mode == "low-memory" && settings.close_behavior == "keep-tabs" {
        settings.close_behavior = "close-tabs".into();
        needs_migration = true;
    }
    if settings.schema_version < 3 {
        if settings.large_file_threshold_mb == 100 {
            settings.large_file_threshold_mb = default_large_file_threshold_mb();
        }
        settings.schema_version = current_schema_version();
        needs_migration = true;
    }
    validate(&settings)?;
    if needs_migration { write(app, &settings)?; }
    Ok(settings)
}

fn validate(settings: &AppSettings) -> Result<(), String> {
    if settings.schema_version != current_schema_version() { return Err("设置版本无效".into()); }
    if !matches!(settings.theme.as_str(), "system" | "light" | "dark") { return Err("主题设置无效".into()); }
    if !matches!(settings.residency_mode.as_str(), "fast" | "low-memory") { return Err("驻留模式设置无效".into()); }
    if !matches!(settings.close_behavior.as_str(), "keep-tabs" | "close-tabs") { return Err("窗口关闭行为设置无效".into()); }
    if settings.residency_mode == "low-memory" && settings.close_behavior == "keep-tabs" {
        return Err("低内存模式不能保留标签页".into());
    }
    if !(1..=1024).contains(&settings.large_file_threshold_mb) {
        return Err("大文件阈值必须是 1–1024 之间的整数 MiB".into());
    }
    Ok(())
}

pub(crate) fn write(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    validate(settings)?;
    let path = path(app)?;
    let parent = path.parent().ok_or_else(|| "设置路径无效".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| format!("无法创建设置目录：{error}"))?;
    let bytes = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("无法创建设置临时文件：{error}"))?;
    temporary.write_all(&bytes).and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("无法写入设置：{error}"))?;
    temporary.persist(path).map_err(|error| format!("无法保存设置：{}", error.error))?;
    Ok(())
}

pub(crate) const fn autostart_available() -> bool {
    cfg!(all(desktop, not(debug_assertions)))
}

#[cfg(all(desktop, not(debug_assertions)))]
pub(crate) fn autostart_enabled(app: &AppHandle) -> Result<bool, String> {
    app.autolaunch().is_enabled().map_err(|error| error.to_string())
}

#[cfg(not(all(desktop, not(debug_assertions))))]
pub(crate) fn autostart_enabled(_app: &AppHandle) -> Result<bool, String> {
    Ok(false)
}

#[cfg(all(desktop, not(debug_assertions)))]
pub(crate) fn sync_autostart(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let manager = app.autolaunch();
    if enabled {
        // 始终刷新启动命令，修复升级或开发版遗留的过期可执行文件路径。
        manager.enable().map_err(|error| error.to_string())?;
    } else if manager.is_enabled().map_err(|error| error.to_string())? {
        manager.disable().map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(not(all(desktop, not(debug_assertions))))]
pub(crate) fn sync_autostart(_app: &AppHandle, _enabled: bool) -> Result<(), String> {
    // 开发构建不接触用户的系统开机启动配置。
    Ok(())
}

#[tauri::command]
pub(crate) fn get_settings(state: State<'_, SettingsState>) -> AppSettings {
    state.0.lock().map(|value| value.clone()).unwrap_or_default()
}

#[tauri::command]
pub(crate) fn update_settings(
    app: AppHandle,
    state: State<'_, SettingsState>,
    settings: AppSettings,
) -> CommandResult<AppSettings> {
    validate(&settings)?;
    let previous_mode = state.0.lock().map_err(|_| "设置状态不可用")?.residency_mode.clone();
    sync_autostart(&app, settings.autostart)?;
    write(&app, &settings)?;
    *state.0.lock().map_err(|_| "设置状态不可用")? = settings.clone();
    diagnostics::append(&app, "settings", "updated", None);
    if previous_mode != settings.residency_mode && settings.residency_mode == "fast" {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || { let _ = crate::ensure_main_window(&handle, false); });
    }
    Ok(settings)
}
