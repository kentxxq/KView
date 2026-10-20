use std::{collections::HashMap, path::{Component, Path, PathBuf}, sync::{Arc, Mutex}};

use percent_encoding::{percent_decode_str, utf8_percent_encode, NON_ALPHANUMERIC};
use serde::{Deserialize, Serialize};
use tauri::{http::{header, HeaderValue, Method, Request, Response, StatusCode}, Emitter, Manager};

use crate::command_error::CommandResult;

const SCHEME: &str = "kview-html";
const PREVIEW_CSP: &str = "sandbox allow-scripts; default-src http: https: kview-html: data: blob: 'unsafe-inline' 'unsafe-eval'; object-src 'none'; worker-src 'none'";

#[derive(Default)]
pub(crate) struct HtmlPreviews(Mutex<HashMap<String, Arc<Preview>>>);

struct Preview {
    root: Option<PathBuf>,
    entry: String,
    content: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum PreviewSource { Local, Remote, Clipboard }

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PreviewHandle { session_id: String, url: String }

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ResourceError { session_id: String, message: String }

#[tauri::command]
pub(crate) fn create_html_preview(
    state: tauri::State<'_, HtmlPreviews>, source: PreviewSource, path: String, content: String,
) -> CommandResult<PreviewHandle> {
    let (root, entry) = match source {
        PreviewSource::Local => {
            let path = Path::new(&path).canonicalize().map_err(|error| format!("无法定位 HTML 文件：{error}"))?;
            if !path.is_file() || !path.extension().and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("html") || value.eq_ignore_ascii_case("htm")) {
                return Err("预览来源必须为 HTML 文件".into());
            }
            let root = path.parent().ok_or("HTML 文件没有所属目录")?.to_path_buf();
            let entry = path.file_name().and_then(|value| value.to_str()).ok_or("HTML 文件名无效")?.to_owned();
            (Some(root), entry)
        }
        PreviewSource::Remote => {
            let url = tauri::Url::parse(&path).map_err(|error| format!("HTML 来源 URL 无效：{error}"))?;
            if !matches!(url.scheme(), "http" | "https") { return Err("HTML 来源仅支持 HTTP/HTTPS".into()); }
            (None, "index.html".into())
        }
        PreviewSource::Clipboard => (None, "index.html".into()),
    };
    let mut token = [0u8; 16];
    getrandom::fill(&mut token).map_err(|error| format!("无法生成 HTML 预览标识：{error}"))?;
    let session_id: String = token.iter().map(|byte| format!("{byte:02x}")).collect();
    let host = format!("{session_id}.localhost");
    let origin = if cfg!(windows) { format!("http://{SCHEME}.{host}") } else { format!("{SCHEME}://{host}") };
    let url = format!("{origin}/{}", utf8_percent_encode(&entry, NON_ALPHANUMERIC));
    state.0.lock().map_err(|_| "HTML 预览会话锁不可用")?
        .insert(session_id.clone(), Arc::new(Preview { root, entry, content }));
    Ok(PreviewHandle { session_id, url })
}

#[tauri::command]
pub(crate) fn update_html_preview(
    state: tauri::State<'_, HtmlPreviews>, session_id: String, content: String,
) -> CommandResult<()> {
    let mut sessions = state.0.lock().map_err(|_| "HTML 预览会话锁不可用")?;
    let preview = sessions.get_mut(&session_id).ok_or("HTML 预览会话已关闭")?;
    *preview = Arc::new(Preview { root: preview.root.clone(), entry: preview.entry.clone(), content });
    Ok(())
}

#[tauri::command]
pub(crate) fn close_html_preview(state: tauri::State<'_, HtmlPreviews>, session_id: String) -> CommandResult<()> {
    state.0.lock().map_err(|_| "HTML 预览会话锁不可用")?.remove(&session_id);
    Ok(())
}

fn read_resource(state: &HtmlPreviews, session_id: &str, encoded_path: &str) -> Result<(Vec<u8>, String), String> {
    let preview = state.0.lock().map_err(|_| "HTML 预览会话锁不可用")?
        .get(session_id).cloned().ok_or("HTML 预览会话已关闭")?;
    let decoded = percent_decode_str(encoded_path).decode_utf8().map_err(|_| "资源路径不是有效的 UTF-8")?;
    let relative = decoded.strip_prefix('/').ok_or("资源路径无效")?;
    if relative.contains(['\\', ':', '\0']) || Path::new(relative).components().any(|part| !matches!(part, Component::Normal(_))) {
        return Err("资源路径不允许越过 HTML 所在目录".into());
    }
    if relative == preview.entry {
        return Ok((preview.content.as_bytes().to_vec(), "text/html; charset=utf-8".into()));
    }
    let root = preview.root.as_ref().ok_or("当前 HTML 没有本地资源目录")?;
    let path = root.join(relative).canonicalize().map_err(|error| format!("无法定位 HTML 资源：{error}"))?;
    if !path.starts_with(root) { return Err("资源路径不允许越过 HTML 所在目录".into()); }
    let bytes = std::fs::read(&path).map_err(|error| format!("无法读取 HTML 资源：{error}"))?;
    let mime = tauri::utils::mime_type::MimeType::parse_with_fallback(
        &bytes, &path.to_string_lossy().to_lowercase(), tauri::utils::mime_type::MimeType::OctetStream,
    );
    Ok((bytes, mime))
}

pub(crate) fn serve(app: &tauri::AppHandle, request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let host = request.uri().host().unwrap_or_default();
    let session_id = host.strip_suffix(".localhost").unwrap_or_default();
    let result = match *request.method() {
        Method::GET | Method::HEAD => read_resource(&app.state::<HtmlPreviews>(), session_id, request.uri().path()),
        Method::OPTIONS => Ok((Vec::new(), "text/plain".into())),
        _ => Err("HTML 资源只允许读取".into()),
    };
    let (status, bytes, mime) = match result {
        Ok((bytes, mime)) => (StatusCode::OK, bytes, mime),
        Err(message) => {
            if let Err(error) = app.emit_to("main", "html-preview-error", ResourceError { session_id: session_id.into(), message: message.clone() }) {
                crate::diagnostics::append(app, "viewer", "html-preview-error-delivery-failed", None);
                eprintln!("HTML 预览错误通知失败：{error}");
            }
            (StatusCode::BAD_REQUEST, message.into_bytes(), "text/plain; charset=utf-8".into())
        }
    };
    let mut response = Response::new(if request.method() == Method::HEAD { Vec::new() } else { bytes });
    *response.status_mut() = status;
    let headers = response.headers_mut();
    match HeaderValue::from_str(&mime) {
        Ok(value) => { headers.insert(header::CONTENT_TYPE, value); }
        Err(error) => {
            *response.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
            *response.body_mut() = format!("HTML 资源 MIME 无效：{error}").into_bytes();
            return response;
        }
    }
    headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_static("*"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_static("GET, HEAD, OPTIONS"));
    headers.insert(header::ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_static("*"));
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    headers.insert("referrer-policy", HeaderValue::from_static("no-referrer"));
    headers.insert("content-security-policy", HeaderValue::from_static(PREVIEW_CSP));
    headers.insert("x-content-type-options", HeaderValue::from_static("nosniff"));
    response
}
