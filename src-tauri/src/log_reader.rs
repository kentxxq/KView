use std::{collections::HashMap, fs::File, io::{Read, Seek, SeekFrom}, path::PathBuf, sync::{atomic::{AtomicU64, Ordering}, Arc, Mutex}};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};
use crate::{command_error::CommandResult, file_access::local_file_metadata};

const WINDOW_BYTES: u64 = 10 * 1024 * 1024;
const INCREMENT_BYTES: u64 = 1024 * 1024;

#[derive(Default)]
pub(crate) struct LogSessions {
    registry: Mutex<LogRegistry>,
    sequence: AtomicU64,
}

#[derive(Default)]
struct LogRegistry {
    generation: u64,
    active: bool,
    sessions: HashMap<String, Arc<Mutex<LogSession>>>,
}

impl LogRegistry {
    fn validate(&self, generation: &str) -> Result<(), String> {
        if !self.active || self.generation.to_string() != generation {
            return Err("日志所属窗口已关闭，请在当前窗口重新连接".into());
        }
        Ok(())
    }
}

pub(crate) fn begin_window(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<LogSessions>();
    let mut registry = state.registry.lock().map_err(|_| "日志会话锁失败")?;
    registry.generation += 1;
    registry.active = true;
    registry.sessions.clear();
    Ok(())
}

pub(crate) fn end_window(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<LogSessions>();
    let mut registry = state.registry.lock().map_err(|_| "日志会话锁失败")?;
    registry.active = false;
    registry.sessions.clear();
    Ok(())
}

#[tauri::command]
pub(crate) fn get_log_window_generation(state: State<'_, LogSessions>) -> CommandResult<String> {
    let registry = state.registry.lock().map_err(|_| "日志会话锁失败".to_string())?;
    if !registry.active { return Err("日志所属窗口已关闭".to_string().into()); }
    Ok(registry.generation.to_string())
}

struct LogSession {
    file: File,
    path: PathBuf,
    identity: (u64, u64),
    offset: u64,
    observed_size: u64,
    encoding: &'static str,
    anchor: Vec<u8>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogChunk {
    content: String,
    encoding: &'static str,
    truncated: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct OpenLogResult {
    session_id: String,
    chunk: LogChunk,
}

#[cfg(windows)]
fn identity(file: &File) -> Result<(u64, u64), String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION};
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(format!("无法识别日志文件：{}", std::io::Error::last_os_error()));
    }
    Ok((u64::from(info.dwVolumeSerialNumber), (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow)))
}

#[cfg(unix)]
fn identity(file: &File) -> Result<(u64, u64), String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    Ok((metadata.dev(), metadata.ino()))
}

fn decode(bytes: &[u8], encoding: &str) -> Result<(String, usize), String> {
    if encoding.starts_with("utf-16") {
        let le = encoding == "utf-16-le";
        let mut units: Vec<u16> = bytes.chunks_exact(2).map(|pair| crate::text_encoding::utf16_unit(pair, le)).collect();
        if units.last().is_some_and(|unit| (0xD800..=0xDBFF).contains(unit)) { units.pop(); }
        let consumed = units.len() * 2;
        let text = String::from_utf16(&units).map_err(|_| "日志包含无效 UTF-16 数据".to_string())?;
        Ok((text, consumed))
    } else {
        match std::str::from_utf8(bytes) {
            Ok(text) => Ok((text.to_string(), bytes.len())),
            Err(error) if error.error_len().is_none() => {
                let consumed = error.valid_up_to();
                let text = std::str::from_utf8(&bytes[..consumed]).map_err(|error| error.to_string())?;
                Ok((text.to_string(), consumed))
            }
            Err(_) => Err("日志包含无效 UTF-8 数据".into()),
        }
    }
}

fn read_chunk(session: &mut LogSession, limit: u64, truncated: bool) -> Result<LogChunk, String> {
    let mut current = File::open(&session.path).map_err(|error| format!("日志文件无法访问：{error}"))?;
    if identity(&current)? != session.identity { return Err("日志文件已被替换，请重新连接".into()); }
    let metadata = current.metadata().map_err(|error| error.to_string())?;
    if metadata.len() < session.observed_size {
        return Err("日志文件已被截断，请重新连接".into());
    }
    if !session.anchor.is_empty() {
        current.seek(SeekFrom::Start(session.offset - session.anchor.len() as u64)).map_err(|error| error.to_string())?;
        let mut anchor = vec![0; session.anchor.len()];
        current.read_exact(&mut anchor).map_err(|error| error.to_string())?;
        if anchor != session.anchor { return Err("日志已有内容被改写，请重新连接".into()); }
    }
    current.seek(SeekFrom::Start(session.offset)).map_err(|error| error.to_string())?;
    let mut bytes = vec![0; (metadata.len().saturating_sub(session.offset).min(limit)) as usize];
    current.read_exact(&mut bytes).map_err(|error| format!("读取日志失败：{error}"))?;
    let (mut content, consumed) = decode(&bytes, session.encoding)?;
    session.offset += consumed as u64;
    if consumed > 0 {
        session.anchor = bytes[consumed.saturating_sub(64)..consumed].to_vec();
    }
    session.observed_size = metadata.len();
    session.file = current;
    let mut truncated = truncated;
    if limit == WINDOW_BYTES {
        let mut breaks = 0;
        let mut next = '\0';
        let mut start = None;
        for (index, character) in content.char_indices().rev() {
            if character == '\n' || (character == '\r' && next != '\n') {
                breaks += 1;
                if breaks == 50_000 { start = Some(index + character.len_utf8()); break; }
            }
            next = character;
        }
        if let Some(start) = start { content = content[start..].to_string(); truncated = true; }
    }
    Ok(LogChunk { content, encoding: session.encoding, truncated })
}

#[tauri::command]
pub(crate) async fn open_log_session(app: AppHandle, path: String, window_generation: String) -> CommandResult<OpenLogResult> {
    app.state::<LogSessions>().registry.lock().map_err(|_| "日志会话锁失败".to_string())?.validate(&window_generation)?;
    Ok(tauri::async_runtime::spawn_blocking(move || -> Result<OpenLogResult, String> {
        let (path, metadata) = local_file_metadata(&path)?;
        let mut file = File::open(&path).map_err(|error| error.to_string())?;
        let mut header = [0; 3];
        let count = file.read(&mut header).map_err(|error| error.to_string())?;
        let (encoding, bom) = if header[..count].starts_with(&[0xff, 0xfe]) { ("utf-16-le", 2) }
            else if header[..count].starts_with(&[0xfe, 0xff]) { ("utf-16-be", 2) }
            else if header[..count].starts_with(&[0xef, 0xbb, 0xbf]) { ("utf-8-bom", 3) }
            else { ("utf-8", 0) };
        if encoding == "utf-8" {
            file.seek(SeekFrom::Start(0)).map_err(|error| error.to_string())?;
            let mut probe = vec![0; metadata.len().min(8192) as usize];
            file.read_exact(&mut probe).map_err(|error| error.to_string())?;
            if probe.contains(&0) { return Err("日志模式只支持文本文件".into()); }
            decode(&probe, encoding)?;
        }
        let mut offset = metadata.len().saturating_sub(WINDOW_BYTES).max(bom);
        let truncated = offset > bom;
        if encoding.starts_with("utf-16") {
            offset += (offset - bom) % 2;
            file.seek(SeekFrom::Start(offset)).map_err(|error| error.to_string())?;
            let mut first = [0; 2];
            if file.read(&mut first).map_err(|error| error.to_string())? == 2 && (0xDC00..=0xDFFF).contains(&crate::text_encoding::utf16_unit(&first, encoding == "utf-16-le")) { offset += 2; }
        } else if truncated {
            file.seek(SeekFrom::Start(offset)).map_err(|error| error.to_string())?;
            let mut first = [0];
            while file.read(&mut first).map_err(|error| error.to_string())? == 1 && first[0] & 0xc0 == 0x80 { offset += 1; }
        }
        let mut session = LogSession { identity: identity(&file)?, file, path, offset, observed_size: metadata.len(), encoding, anchor: Vec::new() };
        let chunk = read_chunk(&mut session, WINDOW_BYTES, truncated)?;
        let state = app.state::<LogSessions>();
        let session_id = format!("log-{}", state.sequence.fetch_add(1, Ordering::Relaxed));
        let mut registry = state.registry.lock().map_err(|_| "日志会话锁失败")?;
        registry.validate(&window_generation)?;
        registry.sessions.insert(session_id.clone(), Arc::new(Mutex::new(session)));
        Ok(OpenLogResult { session_id, chunk })
    }).await.map_err(|error| error.to_string())??)
}

#[tauri::command]
pub(crate) async fn read_log_increment(app: AppHandle, session_id: String) -> CommandResult<LogChunk> {
    let session = app.state::<LogSessions>().registry.lock().map_err(|_| "日志会话锁失败".to_string())?
        .sessions.get(&session_id).cloned().ok_or_else(|| "日志会话已关闭".to_string())?;
    Ok(tauri::async_runtime::spawn_blocking(move || {
        let mut session = session.lock().map_err(|_| "日志会话锁失败".to_string())?;
        read_chunk(&mut session, INCREMENT_BYTES, false)
    }).await.map_err(|error| error.to_string())??)
}

#[tauri::command]
pub(crate) fn close_log_session(state: State<'_, LogSessions>, session_id: String) -> CommandResult<()> {
    state.registry.lock().map_err(|_| "日志会话锁失败".to_string())?.sessions.remove(&session_id);
    Ok(())
}
