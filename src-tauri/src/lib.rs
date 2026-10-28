mod file_access;
mod text_encoding;
mod log_reader;
mod installer;
mod binary_structure;
mod presentation;
mod image_formats;
mod live_photo;
mod epub_reader;
mod formats;
mod command_error;
mod diagnostics;
mod settings;
mod updates;
mod archive;
mod app_identity;
mod html_preview;
#[cfg(target_os = "windows")]
mod windows_space_open;
mod file_associations;

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
    time::Duration,
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
use file_access::{display_path, local_file_metadata, modified_millis};
use text_encoding::{decode_text, detect_line_ending, encode_text, utf16_unit, DecodeTextError};

#[derive(Default)]
struct DesktopState {
    pending_paths: Mutex<Vec<String>>,
    pending_about: AtomicBool,
    pending_space_open: Mutex<Option<SpaceOpenRequest>>,
    exiting: AtomicBool,
    large_text_sessions: Mutex<HashMap<String, Arc<LargeTextSession>>>,
    next_large_text_session_id: AtomicU64,
    always_on_top: AtomicBool,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    bom: Option<bool>,
}

const LARGE_TEXT_CHUNK_BYTES: u64 = 2 * 1024 * 1024;
const LARGE_TEXT_CHECKPOINT_LINES: u64 = 1024;
const LARGE_TEXT_INITIAL_LINES: u64 = 2048;
const MAX_IMAGE_BYTES: u64 = 100 * 1024 * 1024;
const MAX_REMOTE_BYTES: u64 = 200 * 1024 * 1024;
const MAX_REMOTE_TEXT_BYTES: u64 = 10 * 1024 * 1024;

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

impl RemoteFormat {
    fn is_text(self) -> bool {
        matches!(self.kind, "text" | "markdown" | "json" | "yaml")
    }

    fn byte_limit(self) -> u64 {
        if self.is_text() { MAX_REMOTE_TEXT_BYTES }
        else if self.kind == "image" { MAX_IMAGE_BYTES }
        else { MAX_REMOTE_BYTES }
    }
}

fn remote_format_hint(extension: &str, mime_type: &str) -> Option<RemoteFormat> {
    let extension = match mime_type {
        "image/avif" => "avif",
        "image/bmp" | "image/x-ms-bmp" => "bmp",
        "image/x-icon" | "image/vnd.microsoft.icon" => "ico",
        "image/jxl" => "jxl",
        "image/tiff" => "tiff",
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        "application/pdf" => "pdf",
        "text/html" => "html",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        "video/quicktime" => "mov",
        "audio/mpeg" | "audio/mp3" => "mp3",
        "audio/wav" | "audio/wave" | "audio/x-wav" => "wav",
        "audio/mp4" | "audio/x-m4a" => "m4a",
        "audio/aac" | "audio/aacp" => "aac",
        "audio/flac" | "audio/x-flac" => "flac",
        "audio/ogg" | "application/ogg" => "ogg",
        "audio/opus" => "opus",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document" => "docx",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet" => "xlsx",
        "application/vnd.ms-excel" => "xls",
        "application/json" => return Some(RemoteFormat { extension: "json", mime_type: "application/json", kind: "json" }),
        "application/yaml" | "application/x-yaml" | "text/yaml" => return Some(RemoteFormat { extension: "yaml", mime_type: "application/yaml", kind: "yaml" }),
        "text/markdown" => return Some(RemoteFormat { extension: "md", mime_type: "text/markdown", kind: "markdown" }),
        _ => extension,
    };
    let format = match extension {
        "avif" => RemoteFormat { extension: "avif", mime_type: "image/avif", kind: "image" },
        "bmp" => RemoteFormat { extension: "bmp", mime_type: "image/bmp", kind: "image" },
        "ico" => RemoteFormat { extension: "ico", mime_type: "image/vnd.microsoft.icon", kind: "image" },
        "jxl" => RemoteFormat { extension: "jxl", mime_type: "image/jxl", kind: "image" },
        "tif" | "tiff" => RemoteFormat { extension: "tiff", mime_type: "image/tiff", kind: "image" },
        "jpg" | "jpeg" => RemoteFormat { extension: "jpg", mime_type: "image/jpeg", kind: "image" },
        "png" => RemoteFormat { extension: "png", mime_type: "image/png", kind: "image" },
        "webp" => RemoteFormat { extension: "webp", mime_type: "image/webp", kind: "image" },
        "gif" => RemoteFormat { extension: "gif", mime_type: "image/gif", kind: "image" },
        "svg" => RemoteFormat { extension: "svg", mime_type: "image/svg+xml", kind: "image" },
        "pdf" => RemoteFormat { extension: "pdf", mime_type: "application/pdf", kind: "pdf" },
        "html" | "htm" => RemoteFormat { extension: "html", mime_type: "text/html", kind: "text" },
        "mp4" => RemoteFormat { extension: "mp4", mime_type: "video/mp4", kind: "video" },
        "webm" => RemoteFormat { extension: "webm", mime_type: "video/webm", kind: "video" },
        "mov" => RemoteFormat { extension: "mov", mime_type: "video/quicktime", kind: "video" },
        "mp3" => RemoteFormat { extension: "mp3", mime_type: "audio/mpeg", kind: "audio" },
        "wav" => RemoteFormat { extension: "wav", mime_type: "audio/wav", kind: "audio" },
        "m4a" => RemoteFormat { extension: "m4a", mime_type: "audio/mp4", kind: "audio" },
        "aac" => RemoteFormat { extension: "aac", mime_type: "audio/aac", kind: "audio" },
        "flac" => RemoteFormat { extension: "flac", mime_type: "audio/flac", kind: "audio" },
        "ogg" => RemoteFormat { extension: "ogg", mime_type: "audio/ogg", kind: "audio" },
        "opus" => RemoteFormat { extension: "opus", mime_type: "audio/ogg", kind: "audio" },
        "docx" => RemoteFormat { extension: "docx", mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document", kind: "docx" },
        "xlsx" => RemoteFormat { extension: "xlsx", mime_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", kind: "xlsx" },
        "xls" => RemoteFormat { extension: "xls", mime_type: "application/vnd.ms-excel", kind: "xlsx" },
        _ => {
            if let Some((extension, kind)) = formats::text_format(extension, "") {
                RemoteFormat { extension, mime_type: "text/plain", kind }
            } else if mime_type.starts_with("text/") || matches!(mime_type, "application/xml" | "application/javascript" | "application/x-javascript") {
                RemoteFormat { extension: "txt", mime_type: "text/plain", kind: "text" }
            } else {
                return None;
            }
        },
    };
    Some(format)
}

fn detect_remote_format(bytes: &[u8], extension: &str, mime_type: &str) -> CommandResult<Option<RemoteFormat>> {
    if bytes.starts_with(b"PK\x03\x04") {
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
            .map_err(|error| format!("远程 Office 容器已损坏：{error}"))?;
        let has_types = archive.file_names().any(|name| name == "[Content_Types].xml");
        if has_types && archive.file_names().any(|name| name == "word/document.xml") {
            return Ok(remote_format_hint("docx", ""));
        }
        if has_types && archive.file_names().any(|name| name == "xl/workbook.xml") {
            return Ok(remote_format_hint("xlsx", ""));
        }
        return Err("远程 ZIP 容器不是受支持的 DOCX 或 XLSX 文档".into());
    }
    Ok(detect_remote_media_or_text(bytes, extension, mime_type))
}

fn id3_audio_offset(bytes: &[u8]) -> Option<usize> {
    let header = bytes.get(..10)?;
    if !header.starts_with(b"ID3") || header[4] == 0xFF || header[6..10].iter().any(|byte| byte & 0x80 != 0) {
        return None;
    }
    let reserved_flags = match header[3] {
        2 => 0x3F,
        3 => 0x1F,
        4 => 0x0F,
        _ => return None,
    };
    if header[5] & reserved_flags != 0 { return None; }
    let tag_size = header[6..10].iter().fold(0usize, |size, byte| (size << 7) | usize::from(*byte));
    let footer_size = if header[3] == 4 && header[5] & 0x10 != 0 { 10 } else { 0 };
    Some(10 + tag_size + footer_size)
}

fn detect_remote_audio_frame(bytes: &[u8]) -> Option<RemoteFormat> {
    let audio = if bytes.starts_with(b"ID3") {
        bytes.get(id3_audio_offset(bytes)?..)?
    } else {
        bytes
    };
    let header = audio.get(..4)?;
    if header[0] == 0xFF && header[1] & 0xE0 == 0xE0
        && header[1] & 0x18 != 0x08 && header[1] & 0x06 != 0
        && header[2] & 0xF0 != 0xF0 && header[2] & 0x0C != 0x0C
        && header[3] & 0x03 != 0x02
    {
        return remote_format_hint("mp3", "audio/mpeg");
    }
    if audio.len() >= 7 && header[0] == 0xFF && header[1] & 0xF6 == 0xF0
        && (header[2] >> 2) & 0x0F <= 12
    {
        return remote_format_hint("aac", "audio/aac");
    }
    None
}

fn remote_prefix_byte_limit(bytes: &[u8], extension: &str, mime_type: &str) -> u64 {
    // ZIP 的目录在文件尾，ID3 后的音频帧也可能尚未接收，先采用二进制上限。
    if bytes.starts_with(b"PK\x03\x04") || id3_audio_offset(bytes).is_some() {
        return MAX_REMOTE_BYTES;
    }
    detect_remote_media_or_text(bytes, extension, mime_type)
        .map(RemoteFormat::byte_limit).unwrap_or(MAX_REMOTE_BYTES)
}

fn validate_remote_size(actual: u64, declared: Option<u64>, limit: u64) -> CommandResult<()> {
    if actual > limit || declared.is_some_and(|length| length > limit) {
        return Err(format!("远程内容超过 {} MiB 安全限制", limit / 1024 / 1024).into());
    }
    Ok(())
}

fn detect_remote_media_or_text(bytes: &[u8], extension: &str, mime_type: &str) -> Option<RemoteFormat> {
    if let Some((extension, mime)) = image_formats::detect_native_image(bytes) {
        return remote_format_hint(extension, mime);
    }
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
    if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WAVE" {
        return remote_format_hint("wav", "audio/wav");
    }
    if bytes.starts_with(b"fLaC") {
        return remote_format_hint("flac", "audio/flac");
    }
    if bytes.starts_with(b"OggS") {
        let segment_count = usize::from(*bytes.get(26)?);
        let packet = bytes.get(27 + segment_count..)?;
        if packet.starts_with(b"OpusHead") {
            return remote_format_hint("opus", "audio/opus");
        }
        if packet.starts_with(b"\x01vorbis") {
            return remote_format_hint("ogg", "audio/ogg");
        }
        return None;
    }
    if let Some(format) = detect_remote_audio_frame(bytes) {
        return Some(format);
    }
    if bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]) {
        return remote_format_hint(extension, mime_type).filter(|format| format.extension == "xls");
    }
    if bytes.starts_with(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return remote_format_hint("webm", "video/webm");
    }
    if bytes.len() >= 12 && &bytes[4..8] == b"ftyp" {
        let audio = &bytes[8..12] == b"M4A " || extension == "m4a" || matches!(mime_type, "audio/mp4" | "audio/x-m4a");
        if audio { return remote_format_hint("m4a", "audio/mp4"); }
        let quicktime = &bytes[8..12] == b"qt  " || extension == "mov" || mime_type == "video/quicktime";
        return if quicktime {
            remote_format_hint("mov", "video/quicktime")
        } else {
            remote_format_hint("mp4", "video/mp4")
        };
    }

    let hinted = remote_format_hint(extension, mime_type)?;
    if hinted.is_text() { return Some(hinted); }
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
    let mut path_extension = header_name
        .as_deref()
        .and_then(|name| name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase()))
        .unwrap_or(url_extension);
    let file_name = header_name.as_deref().or_else(|| final_url.path_segments().and_then(|segments| segments.filter(|segment| !segment.is_empty()).last())).unwrap_or_default();
    if let Some((extension, _)) = formats::text_format(&path_extension, file_name) {
        path_extension = extension.to_string();
    }
    let content_length = response.content_length();
    validate_remote_size(0, content_length, MAX_REMOTE_BYTES)?;

    let mut bytes = Vec::with_capacity(
        content_length.unwrap_or_default().min(MAX_REMOTE_TEXT_BYTES) as usize,
    );
    let mut download_limit = None;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("读取远程内容失败：{error}"))?
    {
        let limit = download_limit.unwrap_or(MAX_REMOTE_BYTES);
        validate_remote_size(bytes.len().saturating_add(chunk.len()) as u64, None, limit)?;
        bytes.extend_from_slice(&chunk);
        if download_limit.is_none() && bytes.len() >= 4096 {
            let limit = remote_prefix_byte_limit(&bytes[..4096], &path_extension, &mime_type);
            validate_remote_size(bytes.len() as u64, content_length, limit)?;
            download_limit = Some(limit);
        }
    }

    let format = detect_remote_format(&bytes, &path_extension, &mime_type)?
        .ok_or_else(|| "远程内容格式不受支持或文件已损坏；支持图片、PDF、音视频、DOCX、XLS/XLSX、文本与 HTML".to_string())?;
    validate_remote_size(bytes.len() as u64, None, format.byte_limit())?;
    let name = remote_file_name(header_name, &final_url, format.extension);
    let size = bytes.len() as u64;
    let mut encoding = None;
    let mut line_ending = None;
    let mut bom = None;
    let content = if format.is_text() {
        let (content, detected_encoding, detected_bom, consumed) = decode_text(&bytes, false).map_err(|error| match error {
            DecodeTextError::BinaryContent => "远程文本包含二进制内容",
            DecodeTextError::UnsupportedEncoding => "远程文本编码不受支持；支持 UTF-8 或带 BOM 的 UTF-16",
        })?;
        if consumed != bytes.len() { return Err("远程文本包含不完整的编码字符".into()); }
        encoding = Some(detected_encoding);
        line_ending = Some(detect_line_ending(&content));
        bom = Some(detected_bom);
        content
    } else if format.kind == "image" {
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
            size,
            modified_at: 0,
            kind: format.kind.into(),
            encoding,
            line_ending,
            read_only: true,
        },
        content,
        bom,
    })
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
    let (canonical, metadata) = local_file_metadata(&path)?;
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
    let (canonical, metadata) = local_file_metadata(&path)?;
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
        "avif" => "image/avif",
        "bmp" => "image/bmp",
        "ico" => "image/vnd.microsoft.icon",
        "jxl" => "image/jxl",
        "tif" | "tiff" => "image/tiff",
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
    let (canonical, metadata) = local_file_metadata(&path)?;
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
    let (canonical, metadata) = local_file_metadata(&path)?;
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
        let uri = if file_associations::is_registered()? {
            "ms-settings:defaultapps?registeredAppUser=KView\0"
        } else {
            "ms-settings:defaultapps\0"
        }
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

    let (canonical, metadata) = local_file_metadata(&path)?;
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

#[tauri::command]
fn save_annotated_pdf(path: String, source_path: Option<String>, bytes: Vec<u8>) -> CommandResult<()> {
    let target = PathBuf::from(&path);
    if !target.is_absolute() || target.extension().and_then(|value| value.to_str()).is_none_or(|ext| !ext.eq_ignore_ascii_case("pdf")) {
        return Err("请选择有效的 PDF 绝对保存路径".into());
    }
    if !bytes.starts_with(b"%PDF-") {
        return Err("导出的数据不是有效的 PDF 文档".into());
    }
    if let Some(source) = source_path {
        let original = PathBuf::from(source).canonicalize()
            .map_err(|error| format!("无法确认原文件路径：{error}"))?;
        if target.exists() && target.canonicalize()
            .map_err(|error| format!("无法确认保存路径：{error}"))? == original {
            return Err("标注 PDF 必须另存为，不能覆盖原文件".into());
        }
    }
    let parent = target.parent().ok_or("保存路径无效")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)
        .map_err(|error| format!("无法创建临时文件：{error}"))?;
    temporary.write_all(&bytes).and_then(|_| temporary.as_file().sync_all())
        .map_err(|error| format!("无法写入 PDF：{error}"))?;
    temporary.persist(&target).map_err(|error| format!("无法保存 PDF：{}", error.error))?;
    Ok(())
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
    let (canonical, metadata) = local_file_metadata(&path)?;

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
    if kind == "text" {
        let mut signature = [0; 4];
        let mut file = File::open(&canonical).map_err(|error| format!("无法打开文件：{error}"))?;
        let length = file.read(&mut signature).map_err(|error| format!("无法读取文件头：{error}"))?;
        if length == signature.len() && &signature == b"\x7fELF" {
            kind = "binary-structure".into();
        }
    }
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
    state
        .pending_paths
        .lock()
        .map(|mut paths| std::mem::take(&mut *paths))
        .unwrap_or_default()
}

#[tauri::command]
fn take_pending_space_open(state: tauri::State<'_, DesktopState>) -> Option<SpaceOpenRequest> {
    state.pending_space_open.lock().ok()?.take()
}

#[tauri::command]
fn take_pending_about(state: tauri::State<'_, DesktopState>) -> CommandResult<bool> {
    Ok(state.pending_about.swap(false, Ordering::SeqCst))
}

#[tauri::command]
fn cancel_space_open(_app: AppHandle) {
    #[cfg(target_os = "windows")]
    windows_space_open::cancel(&_app);
}

#[tauri::command]
fn close_main_window(
    app: AppHandle,
    settings: tauri::State<'_, settings::SettingsState>,
) -> CommandResult<()> {
    if app.state::<updates::UpdateState>().installing() {
        return Err("update-busy: 正在安装更新，请稍候".into());
    }
    let residency_mode = settings
        .0
        .lock()
        .map(|settings| settings.residency_mode.clone())
        .unwrap_or_else(|_| "fast".into());
    if let Some(window) = app.get_webview_window("main") {
        if residency_mode == "low-memory" {
            window.destroy().map_err(|error| error.to_string())?;
            log_reader::end_window(&app)?;
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
    if app.state::<updates::UpdateState>().installing() { return; }
    if paths.is_empty() {
        return;
    }

    if let Ok(mut pending) = app.state::<DesktopState>().pending_paths.lock() {
        pending.extend(paths);
    }
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

    log_reader::begin_window(app).map_err(|error| std::io::Error::other(error))?;
    let result = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
        .title(app_identity::APP_NAME)
        .decorations(false)
        .inner_size(default_width, default_height)
        .min_inner_size(640.0, 480.0)
        .center()
        .visible(activate)
        .always_on_top(app.state::<DesktopState>().always_on_top.load(Ordering::SeqCst))
        .build();
    if result.is_err() {
        log_reader::end_window(app).map_err(|error| std::io::Error::other(error))?;
    }
    result
}

#[tauri::command]
fn get_window_pin(state: tauri::State<'_, DesktopState>) -> bool {
    state.always_on_top.load(Ordering::SeqCst)
}

#[tauri::command]
fn set_window_pin(window: WebviewWindow, state: tauri::State<'_, DesktopState>, pinned: bool) -> CommandResult<bool> {
    window.set_always_on_top(pinned).map_err(|error| error.to_string())?;
    state.always_on_top.store(pinned, Ordering::SeqCst);
    Ok(pinned)
}

fn schedule_main_window(app: &AppHandle) {
    if app.state::<updates::UpdateState>().installing() { return; }
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

    let open_item = MenuItem::with_id(app, "open", format!("打开 {}", app_identity::APP_NAME), true, None::<&str>)?;
    let autostart_item = CheckMenuItem::with_id(
        app,
        "autostart",
        "开机启动",
        autostart_available,
        autostart_enabled,
        None::<&str>,
    )?;
    let quit_label = if cfg!(debug_assertions) { "退出 KView DEV" } else { "退出" };
    let quit_item = MenuItem::with_id(app, "quit", quit_label, true, None::<&str>)?;
    #[cfg(target_os = "windows")]
    let about_item = MenuItem::with_id(app, "about", "关于", true, None::<&str>)?;
    #[cfg(target_os = "windows")]
    let menu = Menu::with_items(app, &[&open_item, &autostart_item, &about_item, &quit_item])?;
    #[cfg(not(target_os = "windows"))]
    let menu = Menu::with_items(app, &[&open_item, &autostart_item, &quit_item])?;
    let autostart_for_menu = autostart_item.clone();

    let mut tray_builder = TrayIconBuilder::with_id("kview-tray")
        .tooltip(format!("{} 快速文件查看器", app_identity::APP_NAME))
        .menu(&menu)
        .show_menu_on_left_click(false);

    if let Some(icon) = app.default_window_icon() {
        tray_builder = tray_builder.icon(icon.clone());
    }

    tray_builder
        .on_menu_event(move |app, event| match event.id.as_ref() {
            "open" => schedule_main_window(app),
            #[cfg(target_os = "windows")]
            "about" => {
                if app.state::<updates::UpdateState>().installing() { return; }
                app.state::<DesktopState>().pending_about.store(true, Ordering::SeqCst);
                schedule_main_window(app);
                if let Err(error) = app.emit("about-requested", ()) {
                    diagnostics::append(app, "tray", "about-notification-failed", None);
                    eprintln!("发送关于入口通知失败：{error}");
                }
            }
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
                if app.state::<updates::UpdateState>().installing() { return; }
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
    let mut context = tauri::generate_context!();
    app_identity::configure(&mut context);
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
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(DesktopState::default())
        .manage(log_reader::LogSessions::default())
        .manage(html_preview::HtmlPreviews::default())
        .register_asynchronous_uri_scheme_protocol("kview-html", |context, request, responder| {
            let app = context.app_handle().clone();
            tauri::async_runtime::spawn_blocking(move || responder.respond(html_preview::serve(&app, request)));
        })
        .manage(settings::SettingsState::default())
        .manage(epub_reader::EpubSessions::default())
        .invoke_handler(tauri::generate_handler![
            get_window_pin,
            set_window_pin,
            log_reader::open_log_session,
            log_reader::get_log_window_generation,
            log_reader::read_log_increment,
            log_reader::close_log_session,
            html_preview::create_html_preview,
            html_preview::update_html_preview,
            html_preview::close_html_preview,
            updates::get_update_status,
            updates::check_for_updates,
            updates::download_update,
            updates::install_update,
            updates::dismiss_update,
            inspect_file,
            file_access::resolve_pasted_path,
            list_sibling_files,
            fetch_remote_document,
            installer::read_installer,
            binary_structure::read_binary_structure,
            presentation::read_presentation,
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
            file_associations::register_file_associations,
            file_associations::unregister_file_associations,
            existing_paths,
            save_text,
            save_annotated_pdf,
            settings::get_settings,
            settings::update_settings,
            diagnostics::record_diagnostic,
            diagnostics::open_log_directory,
            take_pending_paths,
            take_pending_space_open,
            take_pending_about,
            cancel_space_open,
            close_main_window
        ])
        .setup(|app| {
            app.manage(updates::UpdateState::new(app.package_info().version.to_string()));
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
            updates::start(app.handle());
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
        .build(context)
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
