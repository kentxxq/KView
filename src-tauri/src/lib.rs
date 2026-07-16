use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Mutex,
    },
    time::UNIX_EPOCH,
};

use serde::Serialize;
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

#[cfg(desktop)]
use tauri_plugin_autostart::ManagerExt;

#[derive(Default)]
struct DesktopState {
    pending_paths: Mutex<Vec<String>>,
    exiting: AtomicBool,
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
    let extension = canonical
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let modified_at = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_millis() as u64)
        .unwrap_or_default();

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
fn destroy_main_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        window.destroy().map_err(|error| error.to_string())?;
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

fn ensure_main_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(window);
    }

    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("KView")
        .inner_size(960.0, 680.0)
        .min_inner_size(640.0, 480.0)
        .center()
        .build()
}

fn schedule_main_window(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if ensure_main_window(&handle).is_ok() {
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
            take_pending_paths,
            destroy_main_window
        ])
        .setup(|app| {
            #[cfg(desktop)]
            if !app.autolaunch().is_enabled().unwrap_or(false) {
                let _ = app.autolaunch().enable();
            }

            create_tray(app)?;

            let args: Vec<String> = std::env::args().collect();
            let cwd = std::env::current_dir()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            let paths = collect_paths(&args, &cwd);
            let background = args.iter().any(|argument| argument == "--background");
            queue_paths(app.handle(), paths.clone());

            if !background || !paths.is_empty() {
                ensure_main_window(app.handle())?;
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
