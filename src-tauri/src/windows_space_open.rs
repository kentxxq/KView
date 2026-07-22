//! Windows 资源管理器中的空格打开与键盘切换处理。

use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering},
        mpsc::{self, Sender},
        OnceLock,
    },
    thread,
    time::Duration,
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
            Input::KeyboardAndMouse::{VK_DOWN, VK_ESCAPE, VK_LEFT, VK_RIGHT, VK_SPACE, VK_UP},
            Shell::{FolderItems, IShellFolderViewDual, IShellWindows, IWebBrowserApp, ShellWindows},
            WindowsAndMessaging::{
                CallNextHookEx, FindWindowExW, GetClassNameW, GetForegroundWindow,
                GetGUIThreadInfo, GetWindowThreadProcessId, SetWindowPos, SetWindowsHookExW,
                ShowWindow, GUITHREADINFO, HHOOK, HWND_NOTOPMOST, HWND_TOPMOST,
                KBDLLHOOKSTRUCT, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
                SW_SHOWNOACTIVATE, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN,
                WM_SYSKEYUP,
            },
        },
    },
};

use super::{ensure_main_window, DesktopState, SpaceOpenRequest};

enum SpaceOpenCommand {
    Toggle(isize),
    Switch(isize),
    Close,
}

static COMMANDS: OnceLock<Sender<SpaceOpenCommand>> = OnceLock::new();
static SPACE_OPEN_ACTIVE: AtomicBool = AtomicBool::new(false);
static SPACE_DOWN: AtomicBool = AtomicBool::new(false);
static SPACE_HANDLED: AtomicBool = AtomicBool::new(false);
static SOURCE_WINDOW: AtomicIsize = AtomicIsize::new(0);
static MAIN_WINDOW: AtomicIsize = AtomicIsize::new(0);
static SWITCH_GENERATION: AtomicU64 = AtomicU64::new(0);

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
                SpaceOpenCommand::Toggle(source) => {
                    if SPACE_OPEN_ACTIVE.load(Ordering::SeqCst) {
                        close_space_open(&app);
                    } else if let Some(path) = selected_file(source) {
                        open_with_space(&app, source, path);
                    }
                }
                SpaceOpenCommand::Switch(source) => schedule_switch(app.clone(), source),
                SpaceOpenCommand::Close => close_space_open(&app),
            }
        }
    });

    unsafe {
        SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0)
            .map(|_: HHOOK| ())
            .map_err(|error| error.to_string())
    }
}

pub fn close(_app: &AppHandle) {
    let _ = COMMANDS.get().map(|sender| sender.send(SpaceOpenCommand::Close));
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
    let source = SOURCE_WINDOW.load(Ordering::SeqCst);
    let main = MAIN_WINDOW.load(Ordering::SeqCst);
    let foreground_window = HWND(foreground as *mut _);
    let explorer_context = is_explorer_window(foreground_window)
        && !is_explorer_search_focused(foreground_window);
    let space_open_context = SPACE_OPEN_ACTIVE.load(Ordering::SeqCst)
        && (explorer_context || foreground == main);

    if key == VK_SPACE.0 {
        if is_down {
            if SPACE_DOWN.swap(true, Ordering::SeqCst) {
                if SPACE_HANDLED.load(Ordering::SeqCst) {
                    return LRESULT(1);
                }
            } else if space_open_context {
                SPACE_HANDLED.store(true, Ordering::SeqCst);
                send(SpaceOpenCommand::Close);
                return LRESULT(1);
            } else if explorer_context {
                SPACE_HANDLED.store(true, Ordering::SeqCst);
                send(SpaceOpenCommand::Toggle(foreground));
                return LRESULT(1);
            }
        } else if is_up {
            SPACE_DOWN.store(false, Ordering::SeqCst);
            if SPACE_HANDLED.swap(false, Ordering::SeqCst) {
                return LRESULT(1);
            }
        }
    }

    if SPACE_OPEN_ACTIVE.load(Ordering::SeqCst) && key == VK_ESCAPE.0 && is_down {
        send(SpaceOpenCommand::Close);
    }

    if SPACE_OPEN_ACTIVE.load(Ordering::SeqCst)
        && foreground == source
        && is_up
        && matches!(key, value if value == VK_LEFT.0 || value == VK_RIGHT.0 || value == VK_UP.0 || value == VK_DOWN.0)
    {
        send(SpaceOpenCommand::Switch(source));
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn send(command: SpaceOpenCommand) {
    if let Some(sender) = COMMANDS.get() {
        let _ = sender.send(command);
    }
}

fn open_with_space(app: &AppHandle, source: isize, path: String) {
    let app_handle = app.clone();
    let source_window = source;
    let _ = app.run_on_main_thread(move || {
        let Ok(window) = ensure_main_window(&app_handle, false) else { return };
        if let Ok(hwnd) = window.hwnd() {
            MAIN_WINDOW.store(hwnd.0 as isize, Ordering::SeqCst);
            unsafe {
                let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_NOTOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
            }
        }
        SOURCE_WINDOW.store(source_window, Ordering::SeqCst);
        SPACE_OPEN_ACTIVE.store(true, Ordering::SeqCst);
        queue_space_open(&app_handle, Some(path));
    });
}

fn schedule_switch(app: AppHandle, source: isize) {
    let generation = SWITCH_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(45));
        if generation != SWITCH_GENERATION.load(Ordering::SeqCst)
            || !SPACE_OPEN_ACTIVE.load(Ordering::SeqCst)
            || SOURCE_WINDOW.load(Ordering::SeqCst) != source
        {
            return;
        }
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }
        queue_space_open(&app, selected_file(source));
    });
}

fn close_space_open(app: &AppHandle) {
    if !SPACE_OPEN_ACTIVE.swap(false, Ordering::SeqCst) {
        return;
    }
    SWITCH_GENERATION.fetch_add(1, Ordering::SeqCst);
    SOURCE_WINDOW.store(0, Ordering::SeqCst);
    if let Ok(mut pending) = app.state::<DesktopState>().pending_space_open.lock() {
        *pending = None;
    }
    let _ = app.emit("space-open-closed", ());
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

fn queue_space_open(app: &AppHandle, path: Option<String>) {
    if !SPACE_OPEN_ACTIVE.load(Ordering::SeqCst) {
        return;
    }
    if let Ok(mut pending) = app.state::<DesktopState>().pending_space_open.lock() {
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
