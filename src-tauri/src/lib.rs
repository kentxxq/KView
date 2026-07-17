use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde::{Deserialize, Serialize};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

#[cfg(desktop)]
use tauri_plugin_autostart::ManagerExt;

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppSettings {
    theme: String,
    residency_mode: String,
    autostart: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            residency_mode: "fast".into(),
            autostart: true,
        }
    }
}

#[derive(Default)]
struct DesktopState {
    pending_paths: Mutex<Vec<String>>,
    exiting: AtomicBool,
    settings: Mutex<AppSettings>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenedFile {
    path: String,
    name: String,
    extension: String,
    size: u64,
    modified_at: u64,
    kind: &'static str,
    encoding: Option<&'static str>,
    line_ending: Option<&'static str>,
    read_only: bool,
}

const MAX_EDITABLE_TEXT_BYTES: u64 = 20 * 1024 * 1024;
const LARGE_TEXT_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;
const MAX_IMAGE_BYTES: u64 = 100 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TextDocument {
    content: String,
    encoding: &'static str,
    line_ending: &'static str,
    bom: bool,
    read_only: bool,
    truncated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageDocument {
    data_url: String,
    mime_type: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveResult {
    path: String,
    name: String,
    modified_at: u64,
}

fn settings_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_config_dir()
        .map(|path| path.join("settings.json"))
        .map_err(|error| format!("无法定位设置目录：{error}"))
}

fn read_settings(app: &AppHandle) -> Result<AppSettings, String> {
    let path = settings_path(app)?;
    if !path.exists() {
        return Ok(AppSettings::default());
    }
    let content = std::fs::read_to_string(path).map_err(|error| format!("无法读取设置：{error}"))?;
    let settings: AppSettings =
        serde_json::from_str(&content).map_err(|error| format!("设置文件已损坏：{error}"))?;
    validate_settings(&settings)?;
    Ok(settings)
}

fn validate_settings(settings: &AppSettings) -> Result<(), String> {
    if !matches!(settings.theme.as_str(), "system" | "light" | "dark") {
        return Err("主题设置无效".into());
    }
    if !matches!(settings.residency_mode.as_str(), "fast" | "low-memory") {
        return Err("驻留模式设置无效".into());
    }
    Ok(())
}

fn write_settings(app: &AppHandle, settings: &AppSettings) -> Result<(), String> {
    validate_settings(settings)?;
    let path = settings_path(app)?;
    let parent = path.parent().ok_or_else(|| "设置路径无效".to_string())?;
    std::fs::create_dir_all(parent).map_err(|error| format!("无法创建设置目录：{error}"))?;
    let bytes = serde_json::to_vec_pretty(settings).map_err(|error| error.to_string())?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("无法创建设置临时文件：{error}"))?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("无法写入设置：{error}"))?;
    temporary
        .persist(path)
        .map_err(|error| format!("无法保存设置：{}", error.error))?;
    Ok(())
}

fn diagnostic_path(app: &AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_log_dir()
        .map(|path| path.join("diagnostics.jsonl"))
        .map_err(|error| format!("无法定位日志目录：{error}"))
}

fn append_diagnostic(app: &AppHandle, category: &str, code: &str, duration_ms: Option<f64>) {
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
fn get_settings(state: tauri::State<'_, DesktopState>) -> AppSettings {
    state.settings.lock().map(|value| value.clone()).unwrap_or_default()
}

#[tauri::command]
fn update_settings(
    app: AppHandle,
    state: tauri::State<'_, DesktopState>,
    settings: AppSettings,
) -> Result<AppSettings, String> {
    validate_settings(&settings)?;
    let previous_mode = state
        .settings
        .lock()
        .map_err(|_| "设置状态不可用")?
        .residency_mode
        .clone();
    #[cfg(desktop)]
    {
        let manager = app.autolaunch();
        let enabled = manager.is_enabled().map_err(|error| error.to_string())?;
        if settings.autostart != enabled {
            if settings.autostart {
                manager.enable()
            } else {
                manager.disable()
            }
            .map_err(|error| error.to_string())?;
        }
    }
    write_settings(&app, &settings)?;
    *state.settings.lock().map_err(|_| "设置状态不可用")? = settings.clone();
    append_diagnostic(&app, "settings", "updated", None);
    if previous_mode != settings.residency_mode && settings.residency_mode == "fast" {
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let _ = ensure_main_window(&handle, false);
        });
    }
    Ok(settings)
}

#[tauri::command]
fn record_diagnostic(app: AppHandle, category: String, code: String, duration_ms: Option<f64>) {
    append_diagnostic(&app, &category, &code, duration_ms);
}

fn viewer_kind(extension: &str) -> &'static str {
    match extension {
        "jpg" | "jpeg" | "png" | "webp" | "gif" | "svg" => "image",
        "md" | "markdown" => "markdown",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "txt" | "log" | "xml" | "toml" | "ini" | "env" | "conf" | "sql" => "text",
        "pdf" => "pdf",
        "docx" => "docx",
        "xlsx" => "xlsx",
        "mp4" | "webm" | "mov" => "video",
        _ => "unsupported",
    }
}

fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    if let Some(unc) = value.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
    }
}

fn modified_millis(metadata: &std::fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default()
}

fn detect_line_ending(content: &str) -> &'static str {
    let crlf = content.matches("\r\n").count();
    let without_crlf = content.replace("\r\n", "");
    let lf = without_crlf.matches('\n').count();
    let cr = without_crlf.matches('\r').count();

    if crlf >= lf && crlf >= cr && crlf > 0 {
        "crlf"
    } else if cr > lf && cr > 0 {
        "cr"
    } else {
        "lf"
    }
}

fn decode_text(bytes: &[u8], truncated: bool) -> Result<(String, &'static str, bool), String> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let content = if truncated {
            String::from_utf8_lossy(&bytes[3..]).into_owned()
        } else {
            String::from_utf8(bytes[3..].to_vec()).map_err(|_| "UTF-8 BOM 文件包含无效文本")?
        };
        return Ok((content, "utf-8-bom", true));
    }

    if bytes.starts_with(&[0xFF, 0xFE]) || bytes.starts_with(&[0xFE, 0xFF]) {
        let little_endian = bytes.starts_with(&[0xFF, 0xFE]);
        let mut payload = &bytes[2..];
        if payload.len() % 2 != 0 {
            payload = &payload[..payload.len() - 1];
        }
        let units: Vec<u16> = payload
            .chunks_exact(2)
            .map(|pair| {
                if little_endian {
                    u16::from_le_bytes([pair[0], pair[1]])
                } else {
                    u16::from_be_bytes([pair[0], pair[1]])
                }
            })
            .collect();
        let content = String::from_utf16(&units).map_err(|_| "UTF-16 文件包含无效文本")?;
        return Ok((
            content,
            if little_endian { "utf-16-le" } else { "utf-16-be" },
            true,
        ));
    }

    if bytes.iter().take(8192).any(|byte| *byte == 0) {
        return Err("文件包含二进制内容，不能作为文本打开".into());
    }

    let content = if truncated {
        String::from_utf8_lossy(bytes).into_owned()
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|_| "文件不是有效的 UTF-8 文本")?
    };
    Ok((content, "utf-8", false))
}

#[tauri::command]
fn read_text(path: String) -> Result<TextDocument, String> {
    let canonical = Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;
    let metadata = canonical
        .metadata()
        .map_err(|error| format!("无法读取文件信息：{error}"))?;
    if !metadata.is_file() {
        return Err("所选路径不是文件".into());
    }

    let truncated = metadata.len() > MAX_EDITABLE_TEXT_BYTES;
    let read_limit = if truncated {
        LARGE_TEXT_PREVIEW_BYTES
    } else {
        metadata.len()
    };
    let mut bytes = Vec::with_capacity(read_limit.min(usize::MAX as u64) as usize);
    File::open(&canonical)
        .map_err(|error| format!("无法打开文件：{error}"))?
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("无法读取文件：{error}"))?;
    let (content, encoding, bom) = decode_text(&bytes, truncated)?;
    let line_ending = detect_line_ending(&content);

    Ok(TextDocument {
        content,
        encoding,
        line_ending,
        bom,
        read_only: truncated,
        truncated,
    })
}

#[tauri::command]
fn read_image(path: String) -> Result<ImageDocument, String> {
    let canonical = Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;
    let metadata = canonical
        .metadata()
        .map_err(|error| format!("无法读取文件信息：{error}"))?;
    if metadata.len() > MAX_IMAGE_BYTES {
        return Err("图片超过 100 MiB 安全限制".into());
    }

    let extension = canonical
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let mime_type = match extension.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        _ => return Err("不支持的图片格式".into()),
    };
    let bytes = std::fs::read(&canonical).map_err(|error| format!("无法读取图片：{error}"))?;

    Ok(ImageDocument {
        data_url: format!("data:{mime_type};base64,{}", BASE64.encode(bytes)),
        mime_type,
    })
}

fn encode_text(content: &str, encoding: &str, bom: bool) -> Result<Vec<u8>, String> {
    match encoding {
        "utf-8" | "utf-8-bom" => {
            let mut bytes = Vec::with_capacity(content.len() + 3);
            if bom || encoding == "utf-8-bom" {
                bytes.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
            }
            bytes.extend_from_slice(content.as_bytes());
            Ok(bytes)
        }
        "utf-16-le" | "utf-16-be" => {
            let little_endian = encoding == "utf-16-le";
            let mut bytes = Vec::with_capacity(content.len() * 2 + 2);
            if bom {
                bytes.extend_from_slice(if little_endian { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] });
            }
            for unit in content.encode_utf16() {
                let encoded = if little_endian {
                    unit.to_le_bytes()
                } else {
                    unit.to_be_bytes()
                };
                bytes.extend_from_slice(&encoded);
            }
            Ok(bytes)
        }
        _ => Err("不支持的文本编码".into()),
    }
}

#[tauri::command]
fn save_text(
    path: String,
    content: String,
    encoding: String,
    line_ending: String,
    bom: bool,
    expected_modified_at: Option<u64>,
) -> Result<SaveResult, String> {
    let target = PathBuf::from(&path);
    let parent = target.parent().ok_or_else(|| "保存路径无效".to_string())?;

    if target.exists() {
        let metadata = target
            .metadata()
            .map_err(|error| format!("无法读取目标文件信息：{error}"))?;
        if let Some(expected) = expected_modified_at {
            if modified_millis(&metadata) != expected {
                return Err("external-modified: 文件已被其他程序修改，请另存为或重新打开".into());
            }
        }
    }

    let normalized = content.replace("\r\n", "\n").replace('\r', "\n");
    let restored = match line_ending.as_str() {
        "crlf" => normalized.replace('\n', "\r\n"),
        "cr" => normalized.replace('\n', "\r"),
        _ => normalized,
    };
    let bytes = encode_text(&restored, &encoding, bom)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("无法创建同目录临时文件：{error}"))?;
    temporary
        .write_all(&bytes)
        .and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("无法写入临时文件：{error}"))?;
    temporary
        .persist(&target)
        .map_err(|error| format!("无法原子替换目标文件：{}", error.error))?;

    let metadata = target
        .metadata()
        .map_err(|error| format!("保存后无法读取文件信息：{error}"))?;
    let name = target
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_string();

    Ok(SaveResult {
        path: display_path(&target),
        name,
        modified_at: modified_millis(&metadata),
    })
}

#[tauri::command]
fn inspect_file(path: String) -> Result<OpenedFile, String> {
    let canonical = Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;
    let metadata = canonical
        .metadata()
        .map_err(|error| format!("无法读取文件信息：{error}"))?;

    if !metadata.is_file() {
        return Err("所选路径不是文件".into());
    }

    let name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "文件名不是有效的 Unicode 文本".to_string())?
        .to_string();
    let mut extension = canonical
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    if extension.is_empty() && name.starts_with('.') && name.matches('.').count() == 1 {
        extension = name.trim_start_matches('.').to_lowercase();
    }
    let modified_at = modified_millis(&metadata);

    Ok(OpenedFile {
        path: display_path(&canonical),
        name,
        kind: viewer_kind(&extension),
        extension,
        size: metadata.len(),
        modified_at,
        encoding: None,
        line_ending: None,
        read_only: true,
    })
}

#[tauri::command]
fn take_pending_paths(state: tauri::State<'_, DesktopState>) -> Vec<String> {
    std::mem::take(&mut *state.pending_paths.lock().unwrap())
}

#[tauri::command]
fn close_main_window(app: AppHandle, state: tauri::State<'_, DesktopState>) -> Result<(), String> {
    let residency_mode = state
        .settings
        .lock()
        .map(|settings| settings.residency_mode.clone())
        .unwrap_or_else(|_| "fast".into());
    if let Some(window) = app.get_webview_window("main") {
        if residency_mode == "low-memory" {
            window.destroy().map_err(|error| error.to_string())?;
            append_diagnostic(&app, "residency", "window-destroyed", None);
        } else {
            window.hide().map_err(|error| error.to_string())?;
            append_diagnostic(&app, "residency", "window-hidden", None);
        }
    }
    Ok(())
}

fn collect_paths(args: &[String], cwd: &str) -> Vec<String> {
    args.iter()
        .skip(1)
        .filter(|argument| !argument.starts_with('-'))
        .map(|argument| {
            let path = PathBuf::from(argument);
            if path.is_absolute() {
                path
            } else {
                Path::new(cwd).join(path)
            }
        })
        .filter(|path| path.is_file())
        .map(|path| path.to_string_lossy().into_owned())
        .collect()
}

fn queue_paths(app: &AppHandle, paths: Vec<String>) {
    if paths.is_empty() {
        return;
    }

    app.state::<DesktopState>()
        .pending_paths
        .lock()
        .unwrap()
        .extend(paths);
}

fn ensure_main_window(app: &AppHandle, activate: bool) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window("main") {
        if activate {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        return Ok(window);
    }

    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("KView")
        .inner_size(960.0, 680.0)
        .min_inner_size(640.0, 480.0)
        .center()
        .visible(activate)
        .build()
}

fn schedule_main_window(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if ensure_main_window(&handle, true).is_ok() {
            let _ = handle.emit("open-requested", ());
        }
    });
}

fn create_tray(app: &tauri::App) -> tauri::Result<()> {
    #[cfg(desktop)]
    let autostart_enabled = app.autolaunch().is_enabled().unwrap_or(false);
    #[cfg(not(desktop))]
    let autostart_enabled = false;

    let open_item = MenuItem::with_id(app, "open", "打开 KView", true, None::<&str>)?;
    let autostart_item = CheckMenuItem::with_id(
        app,
        "autostart",
        "开机启动",
        true,
        autostart_enabled,
        None::<&str>,
    )?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&open_item, &autostart_item, &quit_item])?;
    let autostart_for_menu = autostart_item.clone();

    TrayIconBuilder::with_id("kview-tray")
        .icon(app.default_window_icon().unwrap().clone())
        .tooltip("KView 快速文件查看器")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => schedule_main_window(app),
            "autostart" => {
                #[cfg(desktop)]
                {
                    let manager = app.autolaunch();
                    let enabled = manager.is_enabled().unwrap_or(false);
                    let result = if enabled {
                        manager.disable()
                    } else {
                        manager.enable()
                    };
                    if result.is_ok() {
                        let _ = autostart_for_menu.set_checked(!enabled);
                        let state = app.state::<DesktopState>();
                        if let Ok(mut settings) = state.settings.lock() {
                            settings.autostart = !enabled;
                            let _ = write_settings(app, &settings);
                        };
                    }
                }
            }
            "quit" => {
                app.state::<DesktopState>()
                    .exiting
                    .store(true, Ordering::SeqCst);
                app.exit(0);
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            }
            | TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                schedule_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            queue_paths(app, collect_paths(&args, &cwd));
            schedule_main_window(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .plugin(tauri_plugin_dialog::init())
        .manage(DesktopState::default())
        .invoke_handler(tauri::generate_handler![
            inspect_file,
            read_text,
            read_image,
            save_text,
            get_settings,
            update_settings,
            record_diagnostic,
            take_pending_paths,
            close_main_window
        ])
        .setup(|app| {
            let settings_file_exists = settings_path(app.handle())
                .map(|path| path.exists())
                .unwrap_or(false);
            let settings = read_settings(app.handle()).unwrap_or_else(|_| AppSettings::default());
            #[cfg(desktop)] {
                let manager = app.autolaunch();
                let enabled = manager.is_enabled().unwrap_or(false);
                if settings.autostart != enabled || !settings_file_exists {
                    let _ = if settings.autostart { manager.enable() } else { manager.disable() };
                }
            }
            if let Ok(mut current) = app.state::<DesktopState>().settings.lock() {
                *current = settings.clone();
            }
            if !settings_file_exists {
                let _ = write_settings(app.handle(), &settings);
            }

            create_tray(app)?;
            append_diagnostic(app.handle(), "lifecycle", "started", None);

            let args: Vec<String> = std::env::args().collect();
            let cwd = std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let paths = collect_paths(&args, &cwd);
            let background = args.iter().any(|argument| argument == "--background");
            queue_paths(app.handle(), paths.clone());

            if !background || !paths.is_empty() {
                ensure_main_window(app.handle(), true)?;
            } else if settings.residency_mode == "fast" {
                ensure_main_window(app.handle(), false)?;
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building tauri application");

    app.run(|app, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Opened { urls } = &event {
            let paths = urls
                .iter()
                .filter_map(|url| url.to_file_path().ok())
                .map(|path| path.to_string_lossy().into_owned())
                .collect();
            queue_paths(app, paths);
            schedule_main_window(app);
        }

        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            if !app
                .state::<DesktopState>()
                .exiting
                .load(Ordering::SeqCst)
            {
                api.prevent_exit();
            }
        }
    });
}
