mod installer;
mod live_photo;
mod epub_reader;
mod formats;
mod command_error;
mod diagnostics;
mod settings;
mod archive;
#[cfg(target_os = "windows")]
mod windows_space_open;

use std::{
    cmp::Ordering as CompareOrdering,
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, UNIX_EPOCH},
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use reqwest::header::{CONTENT_DISPOSITION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use tauri::{
    menu::{CheckMenuItem, Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
};

use command_error::CommandResult;

#[derive(Default)]
struct DesktopState {
    pending_paths: Mutex<Vec<String>>,
    pending_space_open: Mutex<Option<SpaceOpenRequest>>,
    exiting: AtomicBool,
    large_text_sessions: Mutex<HashMap<String, Arc<LargeTextSession>>>,
    next_large_text_session_id: AtomicU64,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpaceOpenRequest {
    path: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SiblingFileList {
    paths: Vec<String>,
    current_index: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct OpenedFile {
    source: &'static str,
    path: String,
    name: String,
    extension: String,
    size: u64,
    modified_at: u64,
    kind: String,
    encoding: Option<&'static str>,
    line_ending: Option<&'static str>,
    read_only: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RemoteDocument {
    file: OpenedFile,
    content: String,
}

const LARGE_TEXT_CHUNK_BYTES: u64 = 2 * 1024 * 1024;
const LARGE_TEXT_CHECKPOINT_LINES: u64 = 1024;
const LARGE_TEXT_INITIAL_LINES: u64 = 2048;
const MAX_IMAGE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_REMOTE_BYTES: u64 = 200 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TextDocument {
    content: String,
    encoding: &'static str,
    line_ending: &'static str,
    bom: bool,
    read_only: bool,
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "kebab-case")]
enum TextReadResult {
    Success { document: TextDocument },
    LargeText { session: LargeTextSessionInfo },
    BinaryContent { message: &'static str },
    UnsupportedEncoding { message: &'static str },
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ImageDocument {
    data_url: String,
    mime_type: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HtmlDocument {
    html: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BinaryChunk {
    offset: u64,
    base64: String,
    bytes_read: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LargeTextLine {
    line_number: u64,
    content: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LargeTextLinesResult {
    start_line: u64,
    end_line: u64,
    lines: Vec<LargeTextLine>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LargeTextSessionInfo {
    session_id: String,
    encoding: &'static str,
    line_ending: &'static str,
    bom: bool,
    file_size: u64,
    modified_at: u64,
    initial_lines: Vec<LargeTextLine>,
    indexed_bytes: u64,
    indexed_line_count: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LargeTextIndexStatus {
    indexed_bytes: u64,
    file_size: u64,
    indexed_line_count: u64,
    complete: bool,
    total_lines: Option<u64>,
    error: Option<String>,
}

#[derive(Clone)]
struct LargeTextCheckpoint {
    line_number: u64,
    byte_offset: u64,
}

struct LargeTextIndexState {
    indexed_bytes: u64,
    indexed_line_count: u64,
    complete: bool,
    total_lines: Option<u64>,
    error: Option<String>,
    checkpoints: Vec<LargeTextCheckpoint>,
}

struct LargeTextSession {
    id: String,
    path: PathBuf,
    file_size: u64,
    modified_at: u64,
    encoding: &'static str,
    line_ending: &'static str,
    bom: bool,
    cancel: AtomicBool,
    index: Mutex<LargeTextIndexState>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SaveResult {
    path: String,
    name: String,
    modified_at: u64,
}

#[derive(Clone, Copy)]
struct RemoteFormat {
    extension: &'static str,
    mime_type: &'static str,
    kind: &'static str,
}

fn remote_format_hint(extension: &str, mime_type: &str) -> Option<RemoteFormat> {
    let format = match mime_type {
        "image/jpeg" => RemoteFormat { extension: "jpg", mime_type: "image/jpeg", kind: "image" },
        "image/png" => RemoteFormat { extension: "png", mime_type: "image/png", kind: "image" },
        "image/webp" => RemoteFormat { extension: "webp", mime_type: "image/webp", kind: "image" },
        "image/gif" => RemoteFormat { extension: "gif", mime_type: "image/gif", kind: "image" },
        "image/svg+xml" => RemoteFormat { extension: "svg", mime_type: "image/svg+xml", kind: "image" },
        "application/pdf" => RemoteFormat { extension: "pdf", mime_type: "application/pdf", kind: "pdf" },
        "video/mp4" => RemoteFormat { extension: "mp4", mime_type: "video/mp4", kind: "video" },
        "video/webm" => RemoteFormat { extension: "webm", mime_type: "video/webm", kind: "video" },
        "video/quicktime" => RemoteFormat { extension: "mov", mime_type: "video/quicktime", kind: "video" },
        _ => match extension {
            "jpg" | "jpeg" => RemoteFormat { extension: "jpg", mime_type: "image/jpeg", kind: "image" },
            "png" => RemoteFormat { extension: "png", mime_type: "image/png", kind: "image" },
            "webp" => RemoteFormat { extension: "webp", mime_type: "image/webp", kind: "image" },
            "gif" => RemoteFormat { extension: "gif", mime_type: "image/gif", kind: "image" },
            "svg" => RemoteFormat { extension: "svg", mime_type: "image/svg+xml", kind: "image" },
            "pdf" => RemoteFormat { extension: "pdf", mime_type: "application/pdf", kind: "pdf" },
            "mp4" => RemoteFormat { extension: "mp4", mime_type: "video/mp4", kind: "video" },
            "webm" => RemoteFormat { extension: "webm", mime_type: "video/webm", kind: "video" },
            "mov" => RemoteFormat { extension: "mov", mime_type: "video/quicktime", kind: "video" },
            _ => return None,
        },
    };
    Some(format)
}

fn detect_remote_format(bytes: &[u8], extension: &str, mime_type: &str) -> Option<RemoteFormat> {
    if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return remote_format_hint("jpg", "image/jpeg");
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return remote_format_hint("png", "image/png");
    }
    if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        return remote_format_hint("gif", "image/gif");
    }
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return remote_format_hint("webp", "image/webp");
    }
    if bytes.starts_with(b"%PDF-") {
        return remote_format_hint("pdf", "application/pdf");
    }
    if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return remote_format_hint("webm", "video/webm");
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        let quicktime = &bytes[8..12] == b"qt  " || extension == "mov" || mime_type == "video/quicktime";
        return if quicktime {
            remote_format_hint("mov", "video/quicktime")
        } else {
            remote_format_hint("mp4", "video/mp4")
        };
    }

    let hinted = remote_format_hint(extension, mime_type)?;
    if hinted.extension == "svg" {
        let prefix = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]).to_ascii_lowercase();
        if prefix.contains("<svg") {
            return Some(hinted);
        }
    }
    None
}

fn content_disposition_file_name(content_disposition: Option<&str>) -> Option<String> {
    content_disposition.and_then(|header| {
        header.split(';').find_map(|part| {
            let (key, value) = part.trim().split_once('=')?;
            if !key.eq_ignore_ascii_case("filename") {
                return None;
            }
            let value = value.trim().trim_matches(&['"', '\''][..]);
            value.rsplit(['/', '\\']).next().filter(|value| !value.is_empty()).map(str::to_string)
        })
    })
}

fn remote_file_name(header_name: Option<String>, final_url: &reqwest::Url, extension: &str) -> String {
    let url_name = final_url
        .path_segments()
        .and_then(|segments| segments.filter(|segment| !segment.is_empty()).last())
        .map(str::to_string);
    header_name.or(url_name).unwrap_or_else(|| {
        format!("{}.{}", final_url.host_str().unwrap_or("remote"), extension)
    })
}

#[tauri::command]
async fn fetch_remote_document(url: String) -> CommandResult<RemoteDocument> {
    let mut source_url = reqwest::Url::parse(url.trim()).map_err(|_| "URL 格式无效".to_string())?;
    if !matches!(source_url.scheme(), "http" | "https") {
        return Err("仅支持 HTTP 或 HTTPS URL".into());
    }
    if source_url.host_str().is_none() {
        return Err("URL 缺少有效的主机名".into());
    }
    if !source_url.username().is_empty() || source_url.password().is_some() {
        return Err("URL 不能包含账号或密码".into());
    }
    source_url.set_fragment(None);

    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(60))
        .redirect(reqwest::redirect::Policy::limited(5))
        .user_agent("KView/0.1")
        .build()
        .map_err(|error| format!("无法初始化网络请求：{error}"))?;
    let mut response = client
        .get(source_url.clone())
        .send()
        .await
        .map_err(|error| format!("网络请求失败：{error}"))?;
    if !response.status().is_success() {
        return Err(format!("服务器返回 HTTP {}", response.status().as_u16()).into());
    }
    if !matches!(response.url().scheme(), "http" | "https") {
        return Err("重定向后的 URL 协议不受支持".into());
    }

    let final_url = response.url().clone();
    let mime_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let content_disposition = response
        .headers()
        .get(CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_string);
    let header_name = content_disposition_file_name(content_disposition.as_deref());
    let url_extension = final_url
        .path_segments()
        .and_then(|segments| segments.filter(|segment| !segment.is_empty()).last())
        .and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase()))
        .unwrap_or_default();
    let path_extension = header_name
        .as_deref()
        .and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase()))
        .unwrap_or(url_extension);
    let hinted = remote_format_hint(&path_extension, &mime_type);
    let declared_limit = if hinted.is_some_and(|format| format.kind == "image") {
        MAX_IMAGE_BYTES
    } else {
        MAX_REMOTE_BYTES
    };
    if response.content_length().is_some_and(|length| length > declared_limit) {
        return Err(format!("远程内容超过 {} MiB 安全限制", declared_limit / 1024 / 1024).into());
    }

    let mut bytes = Vec::with_capacity(
        response.content_length().unwrap_or_default().min(declared_limit) as usize,
    );
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("读取远程内容失败：{error}"))?
    {
        if bytes.len().saturating_add(chunk.len()) > MAX_REMOTE_BYTES as usize {
            return Err("远程内容超过 200 MiB 安全限制".into());
        }
        bytes.extend_from_slice(&chunk);
    }

    let format = detect_remote_format(&bytes, &path_extension, &mime_type)
        .ok_or_else(|| "远程内容不是受支持的图片、PDF 或视频".to_string())?;
    if format.kind == "image" && bytes.len() > MAX_IMAGE_BYTES as usize {
        return Err("远程图片超过 100 MiB 安全限制".into());
    }
    let name = remote_file_name(header_name, &final_url, format.extension);
    let content = if format.kind == "image" {
        format!("data:{};base64,{}", format.mime_type, BASE64.encode(&bytes))
    } else {
        BASE64.encode(&bytes)
    };

    Ok(RemoteDocument {
        file: OpenedFile {
            source: "remote",
            path: source_url.to_string(),
            name,
            extension: format.extension.into(),
            size: bytes.len() as u64,
            modified_at: 0,
            kind: format.kind.into(),
            encoding: None,
            line_ending: None,
            read_only: true,
        },
        content,
    })
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

enum DecodeTextError {
    BinaryContent,
    UnsupportedEncoding,
}

fn complete_utf8_prefix_len(bytes: &[u8]) -> usize {
    if bytes.is_empty() {
        return 0;
    }
    let mut lead = bytes.len() - 1;
    while lead > 0 && bytes[lead] & 0b1100_0000 == 0b1000_0000 {
        lead -= 1;
    }
    let expected = match bytes[lead] {
        0x00..=0x7F => 1,
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 1,
    };
    if bytes.len() - lead < expected {
        lead
    } else {
        bytes.len()
    }
}

fn decode_text(
    bytes: &[u8],
    chunked: bool,
) -> Result<(String, &'static str, bool, usize), DecodeTextError> {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let payload = &bytes[3..];
        let consumed = if chunked {
            complete_utf8_prefix_len(payload)
        } else {
            payload.len()
        };
        let content = if chunked {
            String::from_utf8_lossy(&payload[..consumed]).into_owned()
        } else {
            String::from_utf8(payload.to_vec()).map_err(|_| DecodeTextError::UnsupportedEncoding)?
        };
        return Ok((content, "utf-8-bom", true, 3 + consumed));
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
        let complete_units = if chunked && units.last().is_some_and(|unit| (0xD800..=0xDBFF).contains(unit)) {
            &units[..units.len() - 1]
        } else {
            &units
        };
        let content = if chunked {
            String::from_utf16_lossy(complete_units)
        } else {
            String::from_utf16(complete_units).map_err(|_| DecodeTextError::UnsupportedEncoding)?
        };
        return Ok((
            content,
            if little_endian { "utf-16-le" } else { "utf-16-be" },
            true,
            2 + complete_units.len() * 2,
        ));
    }

    if bytes.iter().take(8192).any(|byte| *byte == 0) {
        return Err(DecodeTextError::BinaryContent);
    }

    let consumed = if chunked {
        complete_utf8_prefix_len(bytes)
    } else {
        bytes.len()
    };
    let content = if chunked {
        String::from_utf8_lossy(&bytes[..consumed]).into_owned()
    } else {
        String::from_utf8(bytes.to_vec()).map_err(|_| DecodeTextError::UnsupportedEncoding)?
    };
    Ok((content, "utf-8", false, consumed))
}

fn utf16_unit(pair: &[u8], little_endian: bool) -> u16 {
    if little_endian {
        u16::from_le_bytes([pair[0], pair[1]])
    } else {
        u16::from_be_bytes([pair[0], pair[1]])
    }
}

enum OpenLargeTextError {
    BinaryContent,
    UnsupportedEncoding,
    Message(String),
}

fn large_text_content_start(encoding: &str, bom: bool) -> u64 {
    if !bom {
        0
    } else if encoding == "utf-8-bom" {
        3
    } else {
        2
    }
}

fn large_text_unit_width(encoding: &str) -> usize {
    if matches!(encoding, "utf-16-le" | "utf-16-be") {
        2
    } else {
        1
    }
}

fn large_text_unit(bytes: &[u8], encoding: &str) -> u16 {
    match encoding {
        "utf-16-le" => utf16_unit(bytes, true),
        "utf-16-be" => utf16_unit(bytes, false),
        _ => bytes[0] as u16,
    }
}

fn decode_large_text_line(bytes: &[u8], encoding: &str) -> String {
    match encoding {
        "utf-16-le" | "utf-16-be" => {
            let little_endian = encoding == "utf-16-le";
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|pair| utf16_unit(pair, little_endian))
                .collect();
            String::from_utf16_lossy(&units)
        }
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

fn mark_large_text_error(session: &LargeTextSession, message: String) {
    session.cancel.store(true, Ordering::Release);
    if let Ok(mut index) = session.index.lock() {
        index.error = Some(message);
        index.complete = false;
    }
}

fn validate_large_text_file(session: &LargeTextSession) -> Result<(), String> {
    let metadata = session
        .path
        .metadata()
        .map_err(|error| format!("无法读取文件信息：{error}"))?;
    if metadata.len() != session.file_size || modified_millis(&metadata) != session.modified_at {
        let message = "文件已被外部修改，请重新打开后继续查看".to_string();
        mark_large_text_error(session, message.clone());
        return Err(message);
    }
    Ok(())
}

fn record_large_text_break(
    index: &mut LargeTextIndexState,
    line_number: &mut u64,
    next_line_offset: u64,
) {
    *line_number += 1;
    if (*line_number - 1) % LARGE_TEXT_CHECKPOINT_LINES == 0 {
        index.checkpoints.push(LargeTextCheckpoint {
            line_number: *line_number,
            byte_offset: next_line_offset,
        });
    }
}

fn run_large_text_index(session: Arc<LargeTextSession>) {
    let result = (|| -> Result<(), String> {
        let unit_width = large_text_unit_width(session.encoding);
        let content_start = large_text_content_start(session.encoding, session.bom);
        let mut file = File::open(&session.path).map_err(|error| format!("无法打开文件：{error}"))?;
        file.seek(SeekFrom::Start(content_start))
            .map_err(|error| format!("无法定位文件内容：{error}"))?;
        let mut buffer = vec![0; LARGE_TEXT_CHUNK_BYTES as usize];
        let mut byte_offset = content_start;
        let mut line_number = 1_u64;
        let mut pending_cr_end: Option<u64> = None;

        loop {
            if session.cancel.load(Ordering::Acquire) {
                return Ok(());
            }
            validate_large_text_file(&session)?;
            let read = file
                .read(&mut buffer)
                .map_err(|error| format!("无法读取文件：{error}"))?;
            if read == 0 {
                break;
            }
            let complete = read - read % unit_width;
            let mut index = session.index.lock().map_err(|_| "大文件索引状态不可用")?;
            for raw in buffer[..complete].chunks_exact(unit_width) {
                let unit = large_text_unit(raw, session.encoding);
                let next_offset = byte_offset + unit_width as u64;
                if let Some(cr_end) = pending_cr_end.take() {
                    if unit == 0x0A {
                        record_large_text_break(&mut index, &mut line_number, next_offset);
                        byte_offset = next_offset;
                        continue;
                    }
                    record_large_text_break(&mut index, &mut line_number, cr_end);
                }
                if unit == 0x0D {
                    pending_cr_end = Some(next_offset);
                } else if unit == 0x0A {
                    record_large_text_break(&mut index, &mut line_number, next_offset);
                }
                byte_offset = next_offset;
            }
            index.indexed_bytes = byte_offset;
            index.indexed_line_count = line_number;
        }

        let mut index = session.index.lock().map_err(|_| "大文件索引状态不可用")?;
        if let Some(cr_end) = pending_cr_end {
            record_large_text_break(&mut index, &mut line_number, cr_end);
        }
        index.indexed_bytes = session.file_size;
        index.indexed_line_count = line_number;
        index.total_lines = Some(line_number);
        index.complete = true;
        Ok(())
    })();

    if let Err(message) = result {
        mark_large_text_error(&session, message);
    }
}

fn append_large_text_line(
    session: &LargeTextSession,
    requested_start: u64,
    requested_count: u64,
    line_number: &mut u64,
    content: &[u8],
    response_bytes: &mut usize,
    lines: &mut Vec<LargeTextLine>,
) -> bool {
    if *line_number >= requested_start && (lines.len() as u64) < requested_count {
        if !lines.is_empty()
            && response_bytes.saturating_add(content.len()) > LARGE_TEXT_CHUNK_BYTES as usize
        {
            return true;
        }
        let decoded = decode_large_text_line(content, session.encoding);
        *response_bytes = response_bytes.saturating_add(decoded.len());
        lines.push(LargeTextLine {
            line_number: *line_number,
            content: decoded,
        });
    }
    *line_number += 1;
    lines.len() as u64 >= requested_count
}

fn read_large_text_lines_internal(
    session: &LargeTextSession,
    start_line: u64,
    line_count: u64,
) -> Result<LargeTextLinesResult, String> {
    if start_line == 0 || line_count == 0 {
        return Err("行号和读取行数必须大于 0".into());
    }
    validate_large_text_file(session)?;
    let checkpoint = {
        let index = session.index.lock().map_err(|_| "大文件索引状态不可用")?;
        if let Some(error) = &index.error {
            return Err(error.clone());
        }
        index
            .checkpoints
            .iter()
            .rev()
            .find(|checkpoint| checkpoint.line_number <= start_line)
            .cloned()
            .unwrap_or(LargeTextCheckpoint {
                line_number: 1,
                byte_offset: large_text_content_start(session.encoding, session.bom),
            })
    };

    let unit_width = large_text_unit_width(session.encoding);
    let mut file = File::open(&session.path).map_err(|error| format!("无法打开文件：{error}"))?;
    file.seek(SeekFrom::Start(checkpoint.byte_offset))
        .map_err(|error| format!("无法定位文件内容：{error}"))?;
    let mut buffer = vec![0; LARGE_TEXT_CHUNK_BYTES as usize];
    let mut line_number = checkpoint.line_number;
    let mut current_content = Vec::new();
    let mut pending_cr = false;
    let mut lines = Vec::new();
    let mut response_bytes = 0_usize;
    let mut stopped = false;

    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("无法读取文件：{error}"))?;
        if read == 0 {
            break;
        }
        let complete = read - read % unit_width;
        for raw in buffer[..complete].chunks_exact(unit_width) {
            let unit = large_text_unit(raw, session.encoding);
            if pending_cr {
                if unit == 0x0A {
                    stopped = append_large_text_line(
                        session,
                        start_line,
                        line_count,
                        &mut line_number,
                        &current_content,
                        &mut response_bytes,
                        &mut lines,
                    );
                    current_content.clear();
                    pending_cr = false;
                    if stopped {
                        break;
                    }
                    continue;
                }
                stopped = append_large_text_line(
                    session,
                    start_line,
                    line_count,
                    &mut line_number,
                    &current_content,
                    &mut response_bytes,
                    &mut lines,
                );
                current_content.clear();
                pending_cr = false;
                if stopped {
                    break;
                }
            }
            if unit == 0x0D {
                pending_cr = true;
            } else if unit == 0x0A {
                stopped = append_large_text_line(
                    session,
                    start_line,
                    line_count,
                    &mut line_number,
                    &current_content,
                    &mut response_bytes,
                    &mut lines,
                );
                current_content.clear();
                if stopped {
                    break;
                }
            } else if line_number >= start_line {
                current_content.extend_from_slice(raw);
            }
        }
        if stopped {
            break;
        }
    }

    if !stopped {
        if pending_cr {
            stopped = append_large_text_line(
                session,
                start_line,
                line_count,
                &mut line_number,
                &current_content,
                &mut response_bytes,
                &mut lines,
            );
            current_content.clear();
        }
        if !stopped {
            append_large_text_line(
                session,
                start_line,
                line_count,
                &mut line_number,
                &current_content,
                &mut response_bytes,
                &mut lines,
            );
        }
    }

    validate_large_text_file(session)?;
    let actual_start = lines.first().map(|line| line.line_number).unwrap_or(start_line);
    let end_line = lines.last().map(|line| line.line_number).unwrap_or(start_line.saturating_sub(1));
    Ok(LargeTextLinesResult {
        start_line: actual_start,
        end_line,
        lines,
    })
}

fn create_large_text_session(
    state: &tauri::State<'_, DesktopState>,
    canonical: PathBuf,
    metadata: std::fs::Metadata,
) -> Result<LargeTextSessionInfo, OpenLargeTextError> {
    let read_limit = metadata.len().min(LARGE_TEXT_CHUNK_BYTES);
    let mut bytes = Vec::with_capacity(read_limit as usize);
    File::open(&canonical)
        .map_err(|error| OpenLargeTextError::Message(format!("无法打开文件：{error}")))?
        .take(read_limit)
        .read_to_end(&mut bytes)
        .map_err(|error| OpenLargeTextError::Message(format!("无法读取文件：{error}")))?;
    let (content, encoding, bom, _) = decode_text(&bytes, true).map_err(|error| match error {
        DecodeTextError::BinaryContent => OpenLargeTextError::BinaryContent,
        DecodeTextError::UnsupportedEncoding => OpenLargeTextError::UnsupportedEncoding,
    })?;
    let line_ending = detect_line_ending(&content);
    let id = state
        .next_large_text_session_id
        .fetch_add(1, Ordering::Relaxed)
        .saturating_add(1)
        .to_string();
    let content_start = large_text_content_start(encoding, bom);
    let session = Arc::new(LargeTextSession {
        id: id.clone(),
        path: canonical,
        file_size: metadata.len(),
        modified_at: modified_millis(&metadata),
        encoding,
        line_ending,
        bom,
        cancel: AtomicBool::new(false),
        index: Mutex::new(LargeTextIndexState {
            indexed_bytes: content_start,
            indexed_line_count: 1,
            complete: false,
            total_lines: None,
            error: None,
            checkpoints: vec![LargeTextCheckpoint {
                line_number: 1,
                byte_offset: content_start,
            }],
        }),
    });
    let initial = read_large_text_lines_internal(&session, 1, LARGE_TEXT_INITIAL_LINES)
        .map_err(OpenLargeTextError::Message)?;
    state
        .large_text_sessions
        .lock()
        .map_err(|_| OpenLargeTextError::Message("大文件会话状态不可用".into()))?
        .insert(id.clone(), Arc::clone(&session));
    let index_session = Arc::clone(&session);
    std::thread::spawn(move || run_large_text_index(index_session));

    Ok(LargeTextSessionInfo {
        session_id: session.id.clone(),
        encoding,
        line_ending: session.line_ending,
        bom,
        file_size: session.file_size,
        modified_at: session.modified_at,
        initial_lines: initial.lines,
        indexed_bytes: content_start,
        indexed_line_count: 1,
    })
}

fn get_large_text_session(
    state: &tauri::State<'_, DesktopState>,
    session_id: &str,
) -> Result<Arc<LargeTextSession>, String> {
    state
        .large_text_sessions
        .lock()
        .map_err(|_| "大文件会话状态不可用".to_string())?
        .get(session_id)
        .cloned()
        .ok_or_else(|| "大文件会话不存在或已关闭".to_string())
}

#[tauri::command]
fn read_text(
    state: tauri::State<'_, DesktopState>,
    settings: tauri::State<'_, settings::SettingsState>,
    path: String,
) -> CommandResult<TextReadResult> {
    let canonical = Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;
    let metadata = canonical
        .metadata()
        .map_err(|error| format!("无法读取文件信息：{error}"))?;
    if !metadata.is_file() {
        return Err("所选路径不是文件".into());
    }
    let threshold_bytes = settings
        .0
        .lock()
        .map_err(|_| "设置状态不可用")?
        .large_file_threshold_mb
        .saturating_mul(1024 * 1024);
    if metadata.len() > threshold_bytes {
        return match create_large_text_session(&state, canonical, metadata) {
            Ok(session) => Ok(TextReadResult::LargeText { session }),
            Err(OpenLargeTextError::BinaryContent) => Ok(TextReadResult::BinaryContent {
                message: "文件包含二进制内容，不能作为文本打开。",
            }),
            Err(OpenLargeTextError::UnsupportedEncoding) => {
                Ok(TextReadResult::UnsupportedEncoding {
                    message: "文件可能是文本，但其编码暂不受支持。",
                })
            }
            Err(OpenLargeTextError::Message(message)) => Err(message.into()),
        };
    }

    let mut bytes = Vec::with_capacity(metadata.len().min(usize::MAX as u64) as usize);
    File::open(&canonical)
        .map_err(|error| format!("无法打开文件：{error}"))?
        .read_to_end(&mut bytes)
        .map_err(|error| format!("无法读取文件：{error}"))?;
    let (content, encoding, bom, _) = match decode_text(&bytes, false) {
        Ok(result) => result,
        Err(DecodeTextError::BinaryContent) => {
            return Ok(TextReadResult::BinaryContent {
                message: "文件包含二进制内容，不能作为文本打开。",
            });
        }
        Err(DecodeTextError::UnsupportedEncoding) => {
            return Ok(TextReadResult::UnsupportedEncoding {
                message: "文件可能是文本，但其编码暂不受支持。",
            });
        }
    };
    let line_ending = detect_line_ending(&content);
    Ok(TextReadResult::Success {
        document: TextDocument {
            content,
            encoding,
            line_ending,
            bom,
            read_only: false,
        },
    })
}

#[tauri::command]
fn open_large_text_session(
    state: tauri::State<'_, DesktopState>,
    path: String,
) -> CommandResult<LargeTextSessionInfo> {
    let canonical = Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;
    let metadata = canonical
        .metadata()
        .map_err(|error| format!("无法读取文件信息：{error}"))?;
    if !metadata.is_file() {
        return Err("所选路径不是文件".into());
    }
    Ok(create_large_text_session(&state, canonical, metadata).map_err(|error| match error {
        OpenLargeTextError::BinaryContent => "文件包含二进制内容，不能作为文本打开。".into(),
        OpenLargeTextError::UnsupportedEncoding => "文件可能是文本，但其编码暂不受支持。".into(),
        OpenLargeTextError::Message(message) => message,
    })?)
}

#[tauri::command]
fn get_large_text_index_status(
    state: tauri::State<'_, DesktopState>,
    session_id: String,
) -> CommandResult<LargeTextIndexStatus> {
    let session = get_large_text_session(&state, &session_id)?;
    validate_large_text_file(&session)?;
    let index = session.index.lock().map_err(|_| "大文件索引状态不可用")?;
    Ok(LargeTextIndexStatus {
        indexed_bytes: index.indexed_bytes,
        file_size: session.file_size,
        indexed_line_count: index.indexed_line_count,
        complete: index.complete,
        total_lines: index.total_lines,
        error: index.error.clone(),
    })
}

#[tauri::command]
fn read_text_lines(
    state: tauri::State<'_, DesktopState>,
    session_id: String,
    start_line: u64,
    line_count: u64,
) -> CommandResult<LargeTextLinesResult> {
    let session = get_large_text_session(&state, &session_id)?;
    Ok(read_large_text_lines_internal(&session, start_line, line_count)?)
}

#[tauri::command]
fn close_large_text_session(
    state: tauri::State<'_, DesktopState>,
    session_id: String,
) -> CommandResult<()> {
    let session = state
        .large_text_sessions
        .lock()
        .map_err(|_| "大文件会话状态不可用")?
        .remove(&session_id);
    if let Some(session) = session {
        session.cancel.store(true, Ordering::Release);
    }
    Ok(())
}

#[tauri::command]
fn read_image(path: String) -> CommandResult<ImageDocument> {
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
        "heic" | "heif" => return Err("HEIC/HEIF 图片需要使用二进制预览接口".into()),
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

#[tauri::command]
fn read_heic_preview(path: String) -> CommandResult<tauri::ipc::Response> {
    let canonical = Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;
    Ok(tauri::ipc::Response::new(live_photo::read_heic_preview(&canonical)?))
}

#[tauri::command]
fn read_live_photo(path: String) -> CommandResult<tauri::ipc::Response> {
    let canonical = Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;
    Ok(tauri::ipc::Response::new(live_photo::read_live_photo(&canonical)?))
}

#[tauri::command]
fn read_binary(path: String) -> CommandResult<tauri::ipc::Response> {
    const MAX_DOCUMENT_BYTES: u64 = 200 * 1024 * 1024;
    let canonical = Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;
    let metadata = canonical
        .metadata()
        .map_err(|error| format!("无法读取文件信息：{error}"))?;
    if !metadata.is_file() {
        return Err("所选路径不是文件".into());
    }
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err("文档超过 200 MiB 安全限制".into());
    }
    let bytes = std::fs::read(&canonical).map_err(|error| format!("无法读取文档：{error}"))?;
    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
fn read_mobi(path: String) -> CommandResult<tauri::ipc::Response> {
    const MAX_EBOOK_BYTES: u64 = 100 * 1024 * 1024;

    let canonical = Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("无法访问电子书：{error}"))?;
    let metadata = canonical
        .metadata()
        .map_err(|error| format!("无法读取电子书信息：{error}"))?;
    if !metadata.is_file() {
        return Err("所选路径不是文件".into());
    }
    if metadata.len() > MAX_EBOOK_BYTES {
        return Err("电子书超过 100 MiB 安全限制".into());
    }

    let extension = canonical
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension != "mobi" {
        return Err("MOBI 读取器仅支持 .mobi 文件".into());
    }

    let bytes = std::fs::read(&canonical).map_err(|error| format!("无法读取电子书：{error}"))?;
    if bytes.len() < 68 || &bytes[60..68] != b"BOOKMOBI" {
        return Err("文件不是有效的 MOBI 电子书".into());
    }

    Ok(tauri::ipc::Response::new(bytes))
}

#[tauri::command]
fn read_legacy_doc(path: String) -> CommandResult<HtmlDocument> {
    const MAX_DOCUMENT_BYTES: u64 = 200 * 1024 * 1024;
    let canonical = Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;
    let metadata = canonical
        .metadata()
        .map_err(|error| format!("无法读取文件信息：{error}"))?;
    if !metadata.is_file() {
        return Err("所选路径不是文件".into());
    }
    if metadata.len() > MAX_DOCUMENT_BYTES {
        return Err("文档超过 200 MiB 安全限制".into());
    }
    if canonical
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| !value.eq_ignore_ascii_case("doc"))
        .unwrap_or(true)
    {
        return Err("旧版 Word 解析器仅支持 .doc 文件".into());
    }

    let html = office_oxide::to_html(&canonical)
        .map_err(|error| format!("无法解析旧版 Word 文档：{error}"))?;
    Ok(HtmlDocument { html })
}

#[tauri::command]
fn reveal_file(path: String) -> CommandResult<()> {
    let canonical = Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("explorer.exe");
        command.arg("/select,").arg(display_path(&canonical));
        command
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = std::process::Command::new("open");
        command.arg("-R").arg(&canonical);
        command
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut command = std::process::Command::new("xdg-open");
        command.arg(canonical.parent().unwrap_or(&canonical));
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("无法打开文件位置：{error}").into())
}

#[tauri::command]
fn open_file_with(path: String) -> CommandResult<()> {
    let canonical = Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;

    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::Shell::{
            SHOpenWithDialog, OPENASINFO, OAIF_ALLOW_REGISTRATION, OAIF_EXEC,
        };

        let windows_path = format!("{}\0", display_path(&canonical))
            .encode_utf16()
            .collect::<Vec<_>>();
        let open_as = OPENASINFO {
            pcszFile: windows_path.as_ptr(),
            pcszClass: std::ptr::null(),
            oaifInFlags: OAIF_ALLOW_REGISTRATION | OAIF_EXEC,
        };
        let result = unsafe { SHOpenWithDialog(std::ptr::null_mut(), &open_as) };
        if result < 0 {
            return Err(format!("系统打开方式选择器启动失败，HRESULT：0x{:08X}", result as u32).into());
        }
        return Ok(());
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = canonical;
        Err("当前平台暂不支持系统打开方式选择器".into())
    }
}

#[tauri::command]
fn open_file_association_settings() -> CommandResult<()> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::{
            Shell::ShellExecuteW,
            WindowsAndMessaging::SW_SHOWNORMAL,
        };

        let operation = "open\0".encode_utf16().collect::<Vec<_>>();
        let uri = "ms-settings:defaultapps?registeredAppUser=KView\0"
            .encode_utf16()
            .collect::<Vec<_>>();
        let result = unsafe {
            ShellExecuteW(
                std::ptr::null_mut(),
                operation.as_ptr(),
                uri.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result as isize <= 32 {
            return Err(format!("无法打开 Windows 默认应用设置，ShellExecuteW 返回：{}", result as isize).into());
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        Err("文件关联设置入口当前仅支持 Windows".into())
    }
}

#[tauri::command]
fn read_binary_chunk(
    path: String,
    offset: u64,
    length: u64,
    expected_modified_at: u64,
) -> CommandResult<BinaryChunk> {
    const MAX_CHUNK_BYTES: u64 = 64 * 1024;
    if length == 0 || length > MAX_CHUNK_BYTES {
        return Err("二进制分块长度必须在 1 到 64 KiB 之间".into());
    }
    if offset % MAX_CHUNK_BYTES != 0 {
        return Err("二进制分块偏移必须按 64 KiB 对齐".into());
    }

    let canonical = Path::new(&path)
        .canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;
    let metadata = canonical
        .metadata()
        .map_err(|error| format!("无法读取文件信息：{error}"))?;
    if !metadata.is_file() {
        return Err("所选路径不是文件".into());
    }
    if modified_millis(&metadata) != expected_modified_at {
        return Err("文件已被外部修改，请重新打开后继续查看".into());
    }

    let available = metadata.len().saturating_sub(offset);
    let read_length = length.min(available) as usize;
    let mut bytes = vec![0; read_length];
    if read_length > 0 {
        let mut file = File::open(&canonical).map_err(|error| format!("无法打开文件：{error}"))?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| format!("无法定位文件内容：{error}"))?;
        file.read_exact(&mut bytes)
            .map_err(|error| format!("无法读取文件：{error}"))?;
    }

    Ok(BinaryChunk {
        offset,
        bytes_read: bytes.len(),
        base64: BASE64.encode(bytes),
    })
}

#[tauri::command]
fn existing_paths(paths: Vec<String>) -> Vec<String> {
    paths.into_iter().filter(|path| Path::new(path).is_file()).collect()
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
) -> CommandResult<SaveResult> {
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
fn inspect_file(path: String) -> CommandResult<OpenedFile> {
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
    let mut kind = formats::viewer_kind(&extension, &name);
    if matches!(extension.as_str(), "livp" | "jpg" | "jpeg" | "heic" | "heif")
        && live_photo::is_live_photo(&canonical)
    {
        kind = "live-photo".into();
    }

    Ok(OpenedFile {
        source: "local",
        path: display_path(&canonical),
        name,
        kind,
        extension,
        size: metadata.len(),
        modified_at,
        encoding: None,
        line_ending: None,
        read_only: true,
    })
}

fn natural_file_name_cmp(left: &str, right: &str) -> CompareOrdering {
    let left_lower = left.to_lowercase();
    let right_lower = right.to_lowercase();
    let left_bytes = left_lower.as_bytes();
    let right_bytes = right_lower.as_bytes();
    let mut left_index = 0;
    let mut right_index = 0;

    while left_index < left_bytes.len() && right_index < right_bytes.len() {
        if left_bytes[left_index].is_ascii_digit() && right_bytes[right_index].is_ascii_digit() {
            let left_end = (left_index..left_bytes.len())
                .find(|index| !left_bytes[*index].is_ascii_digit())
                .unwrap_or(left_bytes.len());
            let right_end = (right_index..right_bytes.len())
                .find(|index| !right_bytes[*index].is_ascii_digit())
                .unwrap_or(right_bytes.len());
            let left_number = &left_lower[left_index..left_end];
            let right_number = &right_lower[right_index..right_end];
            let left_significant = left_number.trim_start_matches('0');
            let right_significant = right_number.trim_start_matches('0');
            let left_significant = if left_significant.is_empty() { "0" } else { left_significant };
            let right_significant = if right_significant.is_empty() { "0" } else { right_significant };

            let numeric_order = left_significant
                .len()
                .cmp(&right_significant.len())
                .then_with(|| left_significant.cmp(right_significant));
            if numeric_order != CompareOrdering::Equal {
                return numeric_order;
            }
            left_index = left_end;
            right_index = right_end;
            continue;
        }

        let byte_order = left_bytes[left_index].cmp(&right_bytes[right_index]);
        if byte_order != CompareOrdering::Equal {
            return byte_order;
        }
        left_index += 1;
        right_index += 1;
    }

    left_bytes
        .len()
        .cmp(&right_bytes.len())
        .then_with(|| left.cmp(right))
}

fn collect_sibling_files(path: &str) -> Result<SiblingFileList, String> {
    let current = Path::new(path)
        .canonicalize()
        .map_err(|error| format!("无法访问文件：{error}"))?;
    if !current.is_file() {
        return Err("所选路径不是文件".into());
    }
    let parent = current
        .parent()
        .ok_or_else(|| "无法定位文件所在文件夹".to_string())?;
    let entries = std::fs::read_dir(parent)
        .map_err(|error| format!("无法读取文件所在文件夹：{error}"))?;
    let mut files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|entry_path| entry_path.metadata().is_ok_and(|metadata| metadata.is_file()))
        .filter(|entry_path| entry_path.file_name().and_then(|name| name.to_str()).is_some())
        .collect();

    let paired_videos = live_photo::paired_video_paths(&files);
    files.retain(|entry_path| !paired_videos.contains(entry_path) || entry_path == &current);

    if !files.iter().any(|entry_path| entry_path == &current) {
        files.push(current.clone());
    }
    files.sort_by(|left, right| {
        let left_name = left.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        let right_name = right.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        natural_file_name_cmp(left_name, right_name)
    });

    let current_index = files
        .iter()
        .position(|entry_path| entry_path == &current)
        .unwrap_or_default();
    Ok(SiblingFileList {
        paths: files.iter().map(|entry_path| display_path(entry_path)).collect(),
        current_index,
    })
}

#[tauri::command]
async fn list_sibling_files(path: String) -> CommandResult<SiblingFileList> {
    Ok(tauri::async_runtime::spawn_blocking(move || collect_sibling_files(&path))
        .await
        .map_err(|error| format!("无法读取文件所在文件夹：{error}"))??)
}

#[tauri::command]
fn take_pending_paths(state: tauri::State<'_, DesktopState>) -> Vec<String> {
    std::mem::take(&mut *state.pending_paths.lock().unwrap())
}

#[tauri::command]
fn take_pending_space_open(state: tauri::State<'_, DesktopState>) -> Option<SpaceOpenRequest> {
    state.pending_space_open.lock().ok()?.take()
}

#[tauri::command]
fn cancel_space_open(app: AppHandle) {
    #[cfg(target_os = "windows")]
    windows_space_open::cancel(&app);
}

#[tauri::command]
fn close_main_window(
    app: AppHandle,
    settings: tauri::State<'_, settings::SettingsState>,
) -> CommandResult<()> {
    let residency_mode = settings
        .0
        .lock()
        .map(|settings| settings.residency_mode.clone())
        .unwrap_or_else(|_| "fast".into());
    if let Some(window) = app.get_webview_window("main") {
        if residency_mode == "low-memory" {
            window.destroy().map_err(|error| error.to_string())?;
            diagnostics::append(&app, "residency", "window-destroyed", None);
        } else {
            window.hide().map_err(|error| error.to_string())?;
            diagnostics::append(&app, "residency", "window-hidden", None);
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

pub(crate) fn ensure_main_window(app: &AppHandle, activate: bool) -> tauri::Result<WebviewWindow> {
    if let Some(window) = app.get_webview_window("main") {
        if activate {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
        return Ok(window);
    }

    let (default_width, default_height) = app
        .primary_monitor()?
        .map(|monitor| {
            let screen = monitor
                .size()
                .to_logical::<f64>(monitor.scale_factor());
            let width = (screen.width * 0.82)
                .max(960.0)
                .min((screen.width * 0.9).min(1440.0));
            let height = (screen.height * 0.82)
                .max(640.0)
                .min((screen.height * 0.9).min(900.0));
            (width, height)
        })
        .unwrap_or((1200.0, 760.0));

    WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title("KView")
        .decorations(false)
        .inner_size(default_width, default_height)
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
    let autostart_available = settings::autostart_available();
    let autostart_enabled = settings::autostart_enabled(app.handle()).unwrap_or(false);

    let open_item = MenuItem::with_id(app, "open", "打开 KView", true, None::<&str>)?;
    let autostart_item = CheckMenuItem::with_id(
        app,
        "autostart",
        "开机启动",
        autostart_available,
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
                if settings::autostart_available() {
                    let enabled = settings::autostart_enabled(app).unwrap_or(false);
                    let next_enabled = !enabled;
                    if settings::sync_autostart(app, next_enabled).is_ok() {
                        let _ = autostart_for_menu.set_checked(next_enabled);
                        let state = app.state::<settings::SettingsState>();
                        if let Ok(mut settings) = state.0.lock() {
                            settings.autostart = next_enabled;
                            let _ = settings::write(app, &settings);
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
        .manage(settings::SettingsState::default())
        .manage(epub_reader::EpubSessions::default())
        .invoke_handler(tauri::generate_handler![
            inspect_file,
            list_sibling_files,
            fetch_remote_document,
            installer::read_installer,
            read_text,
            open_large_text_session,
            get_large_text_index_status,
            read_text_lines,
            close_large_text_session,
            read_image,
            read_heic_preview,
            read_live_photo,
            read_binary,
            read_mobi,
            epub_reader::open_epub,
            epub_reader::read_epub_chapter,
            epub_reader::read_epub_resource,
            epub_reader::close_epub,
            read_legacy_doc,
            archive::read_archive,
            read_binary_chunk,
            reveal_file,
            open_file_with,
            open_file_association_settings,
            existing_paths,
            save_text,
            settings::get_settings,
            settings::update_settings,
            diagnostics::record_diagnostic,
            diagnostics::open_log_directory,
            take_pending_paths,
            take_pending_space_open,
            cancel_space_open,
            close_main_window
        ])
        .setup(|app| {
            let settings_file_exists = settings::path(app.handle())
                .map(|path| path.exists())
                .unwrap_or(false);
            let settings = settings::read(app.handle()).unwrap_or_default();
            let _ = settings::sync_autostart(app.handle(), settings.autostart);
            if let Ok(mut current) = app.state::<settings::SettingsState>().0.lock() {
                *current = settings.clone();
            }
            if !settings_file_exists {
                let _ = settings::write(app.handle(), &settings);
            }

            create_tray(app)?;
            diagnostics::append(app.handle(), "lifecycle", "started", None);

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

            #[cfg(target_os = "windows")]
            if windows_space_open::install(app.handle().clone()).is_err() {
                diagnostics::append(app.handle(), "space-open", "hook-install-failed", None);
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
