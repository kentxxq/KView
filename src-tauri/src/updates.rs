use std::{sync::{atomic::Ordering, Mutex}, time::Duration};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_updater::{Update, UpdaterExt};

use crate::{command_error::CommandResult, diagnostics, DesktopState};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateStatus {
    revision: u64,
    phase: &'static str,
    current_version: String,
    version: Option<String>,
    notes: Option<String>,
    downloaded_bytes: u64,
    total_bytes: Option<u64>,
    error_code: Option<&'static str>,
    error_message: Option<&'static str>,
    dismissed_version: Option<String>,
}

struct UpdateData {
    status: UpdateStatus,
    pending: Option<Update>,
    bytes: Option<Vec<u8>>,
}

pub(crate) struct UpdateState {
    data: Mutex<UpdateData>,
    operation: tokio::sync::Mutex<()>,
}

impl UpdateState {
    pub(crate) fn new(version: String) -> Self {
        Self {
            data: Mutex::new(UpdateData {
                status: UpdateStatus {
                    revision: 0, phase: "idle", current_version: version,
                    version: None, notes: None, downloaded_bytes: 0, total_bytes: None,
                    error_code: None, error_message: None, dismissed_version: None,
                },
                pending: None, bytes: None,
            }),
            operation: tokio::sync::Mutex::new(()),
        }
    }

    fn change(&self, app: &AppHandle, change: impl FnOnce(&mut UpdateData)) -> UpdateStatus {
        let status = {
            let mut data = match self.data.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            change(&mut data);
            data.status.revision += 1;
            data.status.clone()
        };
        let _ = app.emit("update-status-changed", &status);
        status
    }

    pub(crate) fn installing(&self) -> bool {
        self.data.lock().map(|data| data.status.phase == "installing").unwrap_or(true)
    }
}

fn clear_error(data: &mut UpdateData) {
    data.status.error_code = None;
    data.status.error_message = None;
}

fn fail(app: &AppHandle, state: &UpdateState, code: &'static str, message: &'static str) {
    state.change(app, |data| {
        data.status.phase = "error";
        data.status.error_code = Some(code);
        data.status.error_message = Some(message);
    });
    // 不记录插件原始错误，避免其中的 URL 或本机路径进入日志。
    diagnostics::append(app, "updater", code, None);
}

fn classify(error: &tauri_plugin_updater::Error, fallback: &'static str) -> &'static str {
    use tauri_plugin_updater::Error;
    match error {
        Error::Minisign(_) | Error::Base64(_) | Error::SignatureUtf8(_)
        | Error::SignedVersionMismatch { .. } | Error::MissingSignedVersion => "update-signature",
        Error::Reqwest(_) | Error::Network(_) => "update-network",
        Error::TargetNotFound(_) | Error::TargetsNotFound(_) => "update-platform",
        _ => fallback,
    }
}

#[tauri::command]
pub(crate) fn get_update_status(state: State<'_, UpdateState>) -> UpdateStatus {
    match state.data.lock() {
        Ok(guard) => guard.status.clone(),
        Err(poisoned) => poisoned.into_inner().status.clone(),
    }
}

#[tauri::command]
pub(crate) fn dismiss_update(app: AppHandle, state: State<'_, UpdateState>, version: String) {
    state.change(&app, |data| {
        if data.status.version.as_ref() == Some(&version) {
            data.status.dismissed_version = Some(version);
        }
    });
}

async fn check(app: &AppHandle, state: &UpdateState) -> CommandResult<UpdateStatus> {
    let _operation = state.operation.try_lock().map_err(|_| "update-busy: 更新操作正在进行")?;
    {
        let data = state.data.lock().map_err(|_| "update-failed: 更新状态不可用")?;
        // 已下载的包保留到用户决定安装，定时检查不覆盖它。
        if data.bytes.is_some() { return Ok(data.status.clone()); }
    }
    state.change(app, |data| { clear_error(data); data.status.phase = "checking"; });
    let result = async {
        app.updater_builder().timeout(Duration::from_secs(20)).build()?.check().await
    }.await;
    match result {
        Ok(update) => Ok(state.change(app, |data| {
            data.status.version = update.as_ref().map(|update| update.version.clone());
            data.status.notes = update.as_ref().and_then(|update| update.body.clone());
            data.status.phase = if update.is_some() { "available" } else { "up-to-date" };
            data.status.downloaded_bytes = 0;
            data.status.total_bytes = None;
            data.pending = update;
        })),
        Err(error) => {
            fail(app, state, classify(&error, "update-check"), "检查更新失败，请检查网络或稍后重试。");
            Err("update-check: 检查更新失败，请检查网络或稍后重试。".into())
        }
    }
}

#[tauri::command]
pub(crate) async fn check_for_updates(app: AppHandle, state: State<'_, UpdateState>) -> CommandResult<UpdateStatus> {
    check(&app, &state).await
}

#[tauri::command]
pub(crate) async fn download_update(app: AppHandle, state: State<'_, UpdateState>) -> CommandResult<UpdateStatus> {
    let _operation = state.operation.try_lock().map_err(|_| "update-busy: 更新操作正在进行")?;
    let mut update = {
        let data = state.data.lock().map_err(|_| "update-failed: 更新状态不可用")?;
        if data.bytes.is_some() { return Ok(data.status.clone()); }
        data.pending.clone().ok_or("update-missing: 请先检查更新")?
    };
    if update.download_url.scheme() != "https" {
        fail(&app, &state, "update-transport", "更新下载地址必须使用 HTTPS。");
        return Err("update-transport: 更新下载地址必须使用 HTTPS。".into());
    }
    update.timeout = Some(Duration::from_secs(30 * 60));
    state.change(&app, |data| {
        clear_error(data);
        data.status.phase = "downloading";
        data.status.downloaded_bytes = 0;
        data.status.total_bytes = None;
    });
    let mut last_report = std::time::Instant::now();
    let result = update.download(|length, total| {
        {
            if let Ok(mut data) = state.data.lock() {
                data.status.downloaded_bytes += length as u64;
                data.status.total_bytes = total;
            }
        }
        if last_report.elapsed() >= Duration::from_millis(150) {
            state.change(&app, |_| {});
            last_report = std::time::Instant::now();
        }
    }, || {}).await;
    match result {
        Ok(bytes) => Ok(state.change(&app, |data| {
            data.bytes = Some(bytes);
            data.status.phase = "ready";
        })),
        Err(error) => {
            let code = classify(&error, "update-download");
            let message = if code == "update-signature" {
                "更新包签名校验失败，已阻止安装，请重新下载或联系发布者。"
            } else { "下载更新失败，请检查网络后重试。" };
            fail(&app, &state, code, message);
            Err(format!("{code}: {message}").into())
        }
    }
}

#[tauri::command]
pub(crate) async fn install_update(app: AppHandle, state: State<'_, UpdateState>) -> CommandResult<()> {
    let _operation = state.operation.try_lock().map_err(|_| "update-busy: 更新操作正在进行")?;
    let (update, bytes) = {
        let mut data = state.data.lock().map_err(|_| "update-failed: 更新状态不可用")?;
        let update = data.pending.clone().ok_or("update-missing: 请先检查更新")?;
        let bytes = data.bytes.take().ok_or("update-missing: 请先下载更新")?;
        (update, bytes)
    };
    state.change(&app, |data| { clear_error(data); data.status.phase = "installing"; });
    app.state::<DesktopState>().exiting.store(true, Ordering::SeqCst);
    // 安装可能执行磁盘操作，不阻塞桌面事件线程。
    let result = tauri::async_runtime::spawn_blocking(move || {
        let result = update.install(&bytes);
        (result, bytes)
    }).await;
    match result {
        Ok((Ok(()), _)) => { app.restart(); }
        result => {
            app.state::<DesktopState>().exiting.store(false, Ordering::SeqCst);
            if let Ok((_, bytes)) = result {
                if let Ok(mut data) = state.data.lock() {
                    data.bytes = Some(bytes);
                }
            }
            fail(&app, &state, "update-install", "安装更新失败，可重试或前往 Releases 手动下载安装包。");
            Err("update-install: 安装更新失败，可重试或手动下载安装包。".into())
        }
    }
}

pub(crate) fn start(app: &AppHandle) {
    #[cfg(not(debug_assertions))]
    {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let _ = check(&app, &app.state::<UpdateState>()).await;
                tokio::time::sleep(Duration::from_secs(6 * 60 * 60)).await;
            }
        });
    }
    #[cfg(debug_assertions)]
    let _ = app;
}
