//! Windows 资源管理器和桌面中的空格打开处理。

use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicIsize, Ordering},
        mpsc::{self, Sender},
        OnceLock,
    },
    thread,
};

use tauri::{AppHandle, Emitter, Manager};
use windows::{
    core::Interface,
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, WPARAM},
        System::{
            Com::{
                CoCreateInstance, CoInitializeEx, CoTaskMemFree, IDataObject, IDispatch,
                IServiceProvider, CLSCTX_ALL, COINIT_APARTMENTTHREADED,
            },
            Variant::{VARIANT, VT_I4},
        },
        UI::{
            Input::KeyboardAndMouse::{VK_ESCAPE, VK_SPACE},
            Shell::{
                FolderItems, IShellBrowser, IShellFolderViewDual, IShellItemArray, IShellWindows,
                IWebBrowserApp, ShellWindows, CSIDL_DESKTOP, SHCreateShellItemArrayFromDataObject,
                SID_STopLevelBrowser, SIGDN_FILESYSPATH, SVGIO_SELECTION, SWC_DESKTOP,
                SWFO_NEEDDISPATCH,
            },
            WindowsAndMessaging::{
                CallNextHookEx, GetClassNameW, GetForegroundWindow, GetGUIThreadInfo,
                GetWindowThreadProcessId, IsChild, SetWindowsHookExW, UnhookWindowsHookEx, GUITHREADINFO, HHOOK,
                KBDLLHOOKSTRUCT, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
            },
        },
    },
};

use super::{ensure_main_window, DesktopState, SpaceOpenRequest};

enum SpaceOpenCommand {
    Open { source: isize, focused: isize },
    RequestWindowClose,
}

static COMMANDS: OnceLock<Sender<SpaceOpenCommand>> = OnceLock::new();
static KEYBOARD_HOOK: AtomicIsize = AtomicIsize::new(0);
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
                SpaceOpenCommand::Open { source, focused } => {
                    if let Some(path) = selected_file(source, focused) {
                        open_with_space(&app, path);
                    }
                }
                SpaceOpenCommand::RequestWindowClose => request_window_close(&app),
            }
        }
    });

    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), None, 0) }
        .map_err(|error| error.to_string())?;
    KEYBOARD_HOOK.store(hook.0 as isize, Ordering::SeqCst);
    Ok(())
}

pub fn uninstall() -> Result<(), String> {
    let hook = KEYBOARD_HOOK.load(Ordering::SeqCst);
    if hook != 0 {
        unsafe { UnhookWindowsHookEx(HHOOK(hook as *mut _)) }
            .map_err(|error| error.to_string())?;
        KEYBOARD_HOOK.store(0, Ordering::SeqCst);
    }
    Ok(())
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
    if key != VK_SPACE.0 && key != VK_ESCAPE.0 {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    let is_down = wparam.0 as u32 == WM_KEYDOWN || wparam.0 as u32 == WM_SYSKEYDOWN;
    let is_up = wparam.0 as u32 == WM_KEYUP || wparam.0 as u32 == WM_SYSKEYUP;
    if is_up {
        let handled = if key == VK_SPACE.0 {
            SPACE_DOWN.store(false, Ordering::SeqCst);
            SPACE_HANDLED.swap(false, Ordering::SeqCst)
        } else {
            ESCAPE_HANDLED.swap(false, Ordering::SeqCst)
        };
        return if handled { LRESULT(1) } else {
            unsafe { CallNextHookEx(None, code, wparam, lparam) }
        };
    }
    if !is_down {
        return unsafe { CallNextHookEx(None, code, wparam, lparam) };
    }
    if key == VK_SPACE.0 && SPACE_DOWN.swap(true, Ordering::SeqCst) {
        return if SPACE_HANDLED.load(Ordering::SeqCst) { LRESULT(1) } else {
            unsafe { CallNextHookEx(None, code, wparam, lparam) }
        };
    }
    if key == VK_ESCAPE.0 {
        if ESCAPE_HANDLED.load(Ordering::SeqCst) {
            return LRESULT(1);
        }
        if !SPACE_OPEN_ACTIVE.load(Ordering::SeqCst) {
            return unsafe { CallNextHookEx(None, code, wparam, lparam) };
        }
    }
    let foreground = unsafe { GetForegroundWindow() }.0 as isize;
    let foreground_window = HWND(foreground as *mut _);
    let space_open_context = is_desktop_window(foreground_window)
        || (is_explorer_window(foreground_window)
            && !is_explorer_search_focused(foreground_window));

    if space_open_context {
        if key == VK_SPACE.0 {
            SPACE_HANDLED.store(true, Ordering::SeqCst);
            let focused = explorer_focused_window(foreground_window)
                .map(|window| window.0 as isize)
                .unwrap_or_default();
            send(SpaceOpenCommand::Open { source: foreground, focused });
            return LRESULT(1);
        }
        ESCAPE_HANDLED.store(true, Ordering::SeqCst);
        send(SpaceOpenCommand::RequestWindowClose);
        return LRESULT(1);
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

fn selected_file(source: isize, focused: isize) -> Option<String> {
    let path = selected_path(source, focused)?;
    Path::new(&path).is_file().then_some(path)
}

fn selected_path(source: isize, focused: isize) -> Option<String> {
    unsafe {
        let shell_windows: IShellWindows =
            CoCreateInstance(&ShellWindows, None, CLSCTX_ALL).ok()?;
        let source_window = HWND(source as *mut _);
        if is_desktop_window(source_window) {
            return desktop_selected_path(&shell_windows);
        }
        let focused_window = HWND(focused as *mut _);
        let mut fallback_path = None;
        let mut candidate_count = 0;
        for index in 0..shell_windows.Count().ok()? {
            let variant = index_variant(index);
            let Ok(dispatch) = shell_windows.Item(&variant) else { continue };
            let Ok(browser): Result<IWebBrowserApp, _> = dispatch.cast() else { continue };
            let Ok(browser_window) = browser.HWND() else { continue };
            let browser_window = HWND(browser_window.0 as *mut _);
            if browser_window != source_window
                && !IsChild(source_window, browser_window).as_bool()
            {
                continue;
            }
            let Ok(document) = browser.Document() else { continue };
            let Ok(view): Result<IShellFolderViewDual, _> = document.cast() else { continue };
            let selected_path = view.SelectedItems().ok().and_then(|items| single_path(items));

            candidate_count += 1;
            fallback_path = if candidate_count == 1 {
                selected_path.clone()
            } else {
                None
            };

            // Win11 同一窗口内的标签共享顶层 HWND，必须用各自的 Shell View 匹配焦点。
            let matches_focused_view = shell_view_window(&dispatch)
                .map(|view_window| {
                    view_window == focused_window
                        || IsChild(view_window, focused_window).as_bool()
                })
                .unwrap_or(false);
            if matches_focused_view {
                return selected_path;
            }
        }
        // 旧版资源管理器只有一个候选视图时，保留原有兼容行为。
        if candidate_count == 1 {
            return fallback_path;
        }
    }
    None
}

fn shell_view_window(dispatch: &IDispatch) -> Option<HWND> {
    unsafe {
        let service_provider: IServiceProvider = dispatch.cast().ok()?;
        let browser: IShellBrowser = service_provider
            .QueryService(&SID_STopLevelBrowser)
            .ok()?;
        browser.QueryActiveShellView().ok()?.GetWindow().ok()
    }
}

fn desktop_selected_path(shell_windows: &IShellWindows) -> Option<String> {
    unsafe {
        let location = index_variant(CSIDL_DESKTOP as i32);
        let root = VARIANT::default();
        let mut desktop_window = 0;
        let dispatch = shell_windows
            .FindWindowSW(
                &location,
                &root,
                SWC_DESKTOP,
                &mut desktop_window,
                SWFO_NEEDDISPATCH,
            )
            .ok()?;
        let service_provider: IServiceProvider = dispatch.cast().ok()?;
        let browser: IShellBrowser = service_provider
            .QueryService(&SID_STopLevelBrowser)
            .ok()?;
        let view = browser.QueryActiveShellView().ok()?;
        let selection: IDataObject = view.GetItemObject(SVGIO_SELECTION).ok()?;
        let items: IShellItemArray = SHCreateShellItemArrayFromDataObject(&selection).ok()?;
        if items.GetCount().ok()? != 1 {
            return None;
        }

        let item = items.GetItemAt(0).ok()?;
        let display_name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let path = display_name.to_string().ok();
        CoTaskMemFree(Some(display_name.0.cast()));
        path
    }
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

fn is_desktop_window(hwnd: HWND) -> bool {
    matches!(window_class(hwnd).as_str(), "Progman" | "WorkerW")
}

fn is_explorer_search_focused(hwnd: HWND) -> bool {
    let Some(focused_window) = explorer_focused_window(hwnd) else {
        return false;
    };
    let class = window_class(focused_window);
    class == "Windows.UI.Core.CoreWindow" || class == "Edit"
}

fn explorer_focused_window(hwnd: HWND) -> Option<HWND> {
    unsafe {
        let thread_id = GetWindowThreadProcessId(hwnd, None);
        let mut info = GUITHREADINFO {
            cbSize: std::mem::size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if GetGUIThreadInfo(thread_id, &mut info).is_err() {
            return None;
        }
        Some(info.hwndFocus)
    }
}

fn window_class(hwnd: HWND) -> String {
    let mut buffer = [0u16; 256];
    let length = unsafe { GetClassNameW(hwnd, &mut buffer) };
    String::from_utf16_lossy(&buffer[..length.max(0) as usize])
}
