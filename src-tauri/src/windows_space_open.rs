//! Windows 资源管理器中的空格打开处理。

use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Sender},
        OnceLock,
    },
    thread,
};

use tauri::{AppHandle, Emitter, Manager};
use windows::{
    core::{w, Interface, PCWSTR},
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::{
            Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_APARTMENTTHREADED},
            Variant::{VARIANT, VT_I4},
        },
        UI::{
            Input::KeyboardAndMouse::{VK_ESCAPE, VK_SPACE},
            Shell::{FolderItems, IShellFolderViewDual, IShellWindows, IWebBrowserApp, ShellWindows},
            WindowsAndMessaging::{
                CallNextHookEx, FindWindowExW, GetClassNameW, GetForegroundWindow,
                GetGUIThreadInfo, GetWindowThreadProcessId, SetWindowsHookExW, GUITHREADINFO,
                HHOOK, KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
                WM_SYSKEYUP,
            },
        },
    },
};

use super::{ensure_main_window, DesktopState, SpaceOpenRequest};

enum SpaceOpenCommand {
    Open(isize),
    RequestWindowClose,
}

static COMMANDS: OnceLock<Sender<SpaceOpenCommand>> = OnceLock::new();
static SPACE_OPEN_ACTIVE: AtomicBool = AtomicBool::new(false);
static SPACE_DOWN: AtomicBool = AtomicBool::new(false);
static SPACE_HANDLED: AtomicBool = AtomicBool::new(false);
static ESCAPE_HANDLED: AtomicBool = AtomicBool::new(false);

pub fn install(app: AppHandle) -> Result<(), String> {
    let (sender, receiver) = mpsc::channel();
    COMMANDS
        .set(sender)
        .map_err(|_| "空格打开键盘监听已安装".to_string())?;

    thread::spawn(move || {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        while let Ok(command) = receiver.recv() {
            match command {
                SpaceOpenCommand::Open(source) => {
                    if let Some(path) = selected_file(source) {
                        open_with_space(&app, path);
                    }
                }
                SpaceOpenCommand::RequestWindowClose => request_window_close(&app),
            }
        }
    });

    unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0)
            .map(|_: HHOOK| ())
            .map_err(|error| error.to_string())
    }
}

pub fn cancel(app: &AppHandle) {
    stop_space_open(app);
}

unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code < 0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }

    let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
    let key = event.vkCode as u16;
    let is_down = wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
    let is_up = wparam.0 as u32 == WM_KEYUP || wparam.0 as u32 == WM_SYSKEYUP;
    let foreground = unsafe { GetForegroundWindow() }.0 as isize;
    let foreground_window = HWND(foreground as *mut _);
    let explorer_context = is_explorer_window(foreground_window)
        && !is_explorer_search_focused(foreground_window);

    if key == VK_SPACE.0 {
        if is_down {
            if SPACE_DOWN.swap(true, Ordering::SeqCst) {
                if SPACE_HANDLED.load(Ordering::SeqCst) {
                    return LRESULT(1);
                }
            } else if explorer_context {
                SPACE_HANDLED.store(true, Ordering::SeqCst);
                send(SpaceOpenCommand::Open(foreground));
                return LRESULT(1);
            }
        } else if is_up {
            SPACE_DOWN.store(false, Ordering::SeqCst);
            if SPACE_HANDLED.swap(false, Ordering::SeqCst) {
                return LRESULT(1);
            }
        }
    }

    if key == VK_ESCAPE.0 {
        if is_down {
            if ESCAPE_HANDLED.load(Ordering::SeqCst) {
                return LRESULT(1);
            }
            if SPACE_OPEN_ACTIVE.load(Ordering::SeqCst) {
                ESCAPE_HANDLED.store(true, Ordering::SeqCst);
                send(SpaceOpenCommand::RequestWindowClose);
                return LRESULT(1);
            }
        } else if is_up && ESCAPE_HANDLED.swap(false, Ordering::SeqCst) {
            return LRESULT(1);
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn send(command: SpaceOpenCommand) {
    if let Some(sender) = COMMANDS.get() {
        let _ = sender.send(command);
    }
}

fn open_with_space(app: &AppHandle, path: String) {
    let app_handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        let Ok(window) = ensure_main_window(&app_handle, true) else { return };
        let _ = window.set_focus();
        SPACE_OPEN_ACTIVE.store(true, Ordering::SeqCst);
        queue_space_open(&app_handle, Some(path));
    });
}

fn request_window_close(app: &AppHandle) {
    if SPACE_OPEN_ACTIVE.load(Ordering::SeqCst) {
        let _ = app.emit("space-open-close-requested", ());
    }
}

fn stop_space_open(app: &AppHandle) -> bool {
    if !SPACE_OPEN_ACTIVE.swap(false, Ordering::SeqCst) {
        return false;
    }
    if let Ok(mut pending) = app.state::<DesktopState>().pending_space_open.lock() {
        *pending = None;
    }
    true
}

fn queue_space_open(app: &AppHandle, path: Option<String>) {
    if !SPACE_OPEN_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    if let Ok(mut pending) = app.state::<DesktopState>().pending_space_open.lock() {
        if !SPACE_OPEN_ACTIVE.load(Ordering::SeqCst) {
            return;
        }
        *pending = Some(SpaceOpenRequest { path });
    }
    let _ = app.emit("space-open-path-changed", ());
}

fn selected_file(source: isize) -> Option<String> {
    let path = selected_path(source)?;
    Path::new(&path).is_file().then_some(path)
}

fn selected_path(source: isize) -> Option<String> {
    unsafe {
        let shell_windows: IShellWindows =
            CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).ok()?;
        let source_window = HWND(source as *mut _);
        let tab_window = FindWindowExW(
            Some(source_window),
            None,
            w!("ShellTabWindowClass"),
            PCWSTR::null(),
        )
        .ok();
        for index in 0..shell_windows.Count().ok()? {
            let variant = index_variant(index);
            let Ok(dispatch) = shell_windows.Item(&variant) else { continue };
            let Ok(browser): Result<IWebBrowserApp, _> = dispatch.cast() else { continue };
            let Ok(browser_window) = browser.HWND() else { continue };
            let matches_tab = tab_window
                .map(|tab| browser_window.0 == tab.0 as isize)
                .unwrap_or(false);
            if browser_window.0 as isize != source && !matches_tab {
                continue;
            }
            let Ok(document) = browser.Document() else { continue };
            let Ok(view): Result<IShellFolderViewDual, _> = document.cast() else { continue };
            let Ok(items) = view.SelectedItems() else { return None };
            return single_path(items);
        }
    }
    None
}

unsafe fn single_path(items: FolderItems) -> Option<String> {
    if unsafe { items.Count().ok()? } != 1 {
        return None;
    }
    let index = index_variant(0);
    let item = unsafe { items.Item(&index).ok()? };
    if unsafe { item.IsFolder().ok()?.as_bool() } {
        return None;
    }
    unsafe { item.Path().ok() }.map(|path| path.to_string())
}

fn index_variant(index: i32) -> VARIANT {
    let mut variant = VARIANT::default();
    unsafe {
        let value = &mut *variant.Anonymous.Anonymous;
        value.vt = VT_I4;
        value.Anonymous.lVal = index;
    }
    variant
}

fn is_explorer_window(hwnd: HWND) -> bool {
    matches!(window_class(hwnd).as_str(), "CabinetWClass" | "ExploreWClass")
}

fn is_explorer_search_focused(hwnd: HWND) -> bool {
    unsafe {
        let thread_id = GetWindowThreadProcessId(hwnd, None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread_id, &mut info).is_err() {
            return false;
        }
        let class = window_class(info.hwndFocus);
        class == "Windows.UI.Core.CoreWindow" || class == "Edit"
    }
}

fn window_class(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    let length = unsafe { GetClassNameW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}
