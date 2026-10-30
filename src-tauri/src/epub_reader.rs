use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use rbook::{
    epub::{
        rewrite::{EpubRewriteOptions, PathRewrite},
        toc::EpubTocEntry,
    },
    Epub,
};
use serde::Serialize;
use tauri::State;
use crate::command_error::CommandResult;

const MAX_EBOOK_BYTES: u64 = 100 * 1024 * 1024;
const MAX_EPUB_ENTRIES: usize = 10_000;
const MAX_EPUB_EXPANDED_BYTES: u64 = 300 * 1024 * 1024;
const MAX_EPUB_IMAGE_BYTES: usize = 50 * 1024 * 1024;

#[derive(Default)]
pub struct EpubSessions {
    sessions: Mutex<HashMap<String, Arc<EpubSession>>>,
    next_id: AtomicU64,
}

struct EpubSession {
    epub: Epub,
    chapters: HashMap<String, EpubChapterRecord>,
}

#[derive(Clone)]
struct EpubChapterRecord {
    manifest_id: String,
    href: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpubDocument {
    session_id: String,
    title: Option<String>,
    spine: Vec<EpubSpineItem>,
    toc: Vec<EpubTocItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EpubSpineItem {
    id: String,
    href: String,
    linear: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EpubTocItem {
    label: String,
    href: String,
    chapter_id: Option<String>,
    fragment: Option<String>,
    children: Vec<EpubTocItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpubChapter {
    id: String,
    href: String,
    html: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EpubResource {
    mime_type: String,
    base64: String,
}

fn validate_epub(path: &str) -> Result<File, String> {
    let canonical = Path::new(path)
        .canonicalize()
        .map_err(|error| format!("无法访问 EPUB：{error}"))?;
    let metadata = canonical
        .metadata()
        .map_err(|error| format!("无法读取 EPUB 文件信息：{error}"))?;
    if !metadata.is_file() {
        return Err("所选路径不是文件".into());
    }
    if metadata.len() > MAX_EBOOK_BYTES {
        return Err("电子书超过 100 MiB 安全限制".into());
    }
    if !canonical
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("epub"))
    {
        return Err("文件扩展名不是 EPUB".into());
    }

    let file = File::open(&canonical).map_err(|error| format!("无法读取 EPUB：{error}"))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|error| format!("EPUB 容器无效：{error}"))?;
    if archive.len() > MAX_EPUB_ENTRIES {
        return Err("EPUB 条目数量超过 10,000 个安全限制".into());
    }

    let mut expanded_size = 0_u64;
    let mut encrypted = false;
    for index in 0..archive.len() {
        let entry = archive
            .by_index_data(index)
            .map_err(|error| format!("无法检查 EPUB 条目：{error}"))?;
        expanded_size = expanded_size
            .checked_add(entry.size())
            .filter(|size| *size <= MAX_EPUB_EXPANDED_BYTES)
            .ok_or_else(|| "EPUB 解压后内容超过 300 MiB 安全限制".to_string())?;
        if entry.encrypted() || entry
            .name()
            .map_err(|error| format!("无法读取 EPUB 条目名称：{error}"))?
            .trim_start_matches('/')
            .eq_ignore_ascii_case("META-INF/encryption.xml")
        {
            encrypted = true;
        }
    }
    if expanded_size == 0 {
        return Err("EPUB 不包含可读取内容".into());
    }
    if encrypted {
        return Err("不支持加密或受 DRM 保护的 EPUB".into());
    }

    let mut mime_type = String::new();
    archive
        .by_name("mimetype")
        .map_err(|_| "EPUB 缺少 mimetype 标识".to_string())?
        .take(64)
        .read_to_string(&mut mime_type)
        .map_err(|error| format!("无法读取 EPUB mimetype：{error}"))?;
    if mime_type.trim() != "application/epub+zip" {
        return Err("文件不是有效的 EPUB 电子书".into());
    }
    archive
        .by_name("META-INF/container.xml")
        .map_err(|_| "EPUB 缺少 META-INF/container.xml".to_string())?;

    let mut file = archive.into_inner();
    file.seek(SeekFrom::Start(0))
        .map_err(|error| format!("无法重置 EPUB 读取位置：{error}"))?;
    Ok(file)
}

fn toc_item(
    entry: EpubTocEntry<'_>,
    chapter_by_href: &HashMap<String, String>,
) -> EpubTocItem {
    let href = entry.href();
    let href_value = href.map(|value| value.as_str().to_string()).unwrap_or_default();
    let chapter_id = href
        .map(|value| value.path().as_str())
        .and_then(|path| chapter_by_href.get(path))
        .cloned();
    let fragment = href
        .and_then(|value| value.fragment())
        .map(ToOwned::to_owned);
    EpubTocItem {
        label: entry.label().to_string(),
        href: href_value,
        chapter_id,
        fragment,
        children: entry
            .iter()
            .map(|child| toc_item(child, chapter_by_href))
            .collect(),
    }
}

fn session(
    sessions: &State<'_, EpubSessions>,
    session_id: &str,
) -> Result<Arc<EpubSession>, String> {
    sessions
        .sessions
        .lock()
        .map_err(|_| "EPUB 会话状态不可用".to_string())?
        .get(session_id)
        .cloned()
        .ok_or_else(|| "EPUB 会话已关闭，请重新打开文件".to_string())
}

fn normalize_resource_href(base: &str, href: &str) -> Option<String> {
    let href = href.trim();
    if href.is_empty() || href.starts_with("//") {
        return None;
    }
    let path = href.split(['?', '#']).next().unwrap_or_default();
    if path.is_empty() || path.contains(':') {
        return None;
    }
    let combined = if path.starts_with('/') {
        path.to_string()
    } else {
        let directory = base.rsplit_once('/').map(|(value, _)| value).unwrap_or_default();
        format!("{directory}/{path}")
    };
    let mut segments = Vec::new();
    for segment in combined.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop()?;
            }
            value => segments.push(value),
        }
    }
    Some(format!("/{}", segments.join("/")))
}

#[tauri::command]
pub fn open_epub(
    path: String,
    sessions: State<'_, EpubSessions>,
) -> CommandResult<EpubDocument> {
    let file = validate_epub(&path)?;
    let mut options = Epub::options();
    options.strict(false);
    let epub = options
        .read(file)
        .map_err(|error| format!("无法解析 EPUB：{error}"))?;

    let mut chapters = HashMap::new();
    let mut chapter_by_href = HashMap::new();
    let mut spine = Vec::new();
    for (index, entry) in epub.spine().iter().enumerate() {
        let manifest = entry
            .manifest_entry()
            .ok_or_else(|| format!("EPUB 书脊条目缺少资源：{}", entry.idref()))?;
        let id = format!("spine-{index}");
        let href = manifest.href().path().as_str().to_string();
        chapter_by_href.entry(href.clone()).or_insert_with(|| id.clone());
        chapters.insert(
            id.clone(),
            EpubChapterRecord {
                manifest_id: manifest.id().to_string(),
                href: href.clone(),
            },
        );
        spine.push(EpubSpineItem {
            id,
            href,
            linear: entry.is_linear(),
        });
    }
    if spine.is_empty() {
        return Err("EPUB 不包含可阅读书脊".into());
    }

    let toc = epub
        .toc()
        .contents()
        .map(|root| {
            root.iter()
                .map(|entry| toc_item(entry, &chapter_by_href))
                .collect()
        })
        .unwrap_or_default();
    let title = epub
        .metadata()
        .title()
        .map(|value| value.value().to_string());
    let session_id = format!(
        "epub-{}",
        sessions.next_id.fetch_add(1, Ordering::Relaxed) + 1
    );
    sessions
        .sessions
        .lock()
        .map_err(|_| "EPUB 会话状态不可用".to_string())?
        .insert(session_id.clone(), Arc::new(EpubSession { epub, chapters }));

    Ok(EpubDocument {
        session_id,
        title,
        spine,
        toc,
    })
}

#[tauri::command]
pub fn read_epub_chapter(
    session_id: String,
    chapter_id: String,
    sessions: State<'_, EpubSessions>,
) -> CommandResult<EpubChapter> {
    let session = session(&sessions, &session_id)?;
    let chapter = session
        .chapters
        .get(&chapter_id)
        .ok_or_else(|| "EPUB 章节不存在".to_string())?;
    let manifest = session
        .epub
        .manifest()
        .by_id(&chapter.manifest_id)
        .ok_or_else(|| "EPUB 章节资源不存在".to_string())?;
    let rewrite = EpubRewriteOptions::default()
        .rewrite_paths(PathRewrite::root_relative());
    let html = manifest
        .read_str_with(&rewrite)
        .map_err(|error| format!("无法读取 EPUB 章节：{error}"))?;
    Ok(EpubChapter {
        id: chapter_id,
        href: chapter.href.clone(),
        html,
    })
}

#[tauri::command]
pub fn read_epub_resource(
    session_id: String,
    chapter_id: String,
    href: String,
    sessions: State<'_, EpubSessions>,
) -> CommandResult<EpubResource> {
    let session = session(&sessions, &session_id)?;
    let chapter = session
        .chapters
        .get(&chapter_id)
        .ok_or_else(|| "EPUB 章节不存在".to_string())?;
    let normalized = normalize_resource_href(&chapter.href, &href)
        .ok_or_else(|| "EPUB 图片路径无效".to_string())?;
    let manifest = session
        .epub
        .manifest()
        .by_href(&normalized)
        .ok_or_else(|| "EPUB 图片资源不存在".to_string())?;
    if !matches!(
        manifest.media_type(),
        "image/png"
            | "image/jpeg"
            | "image/gif"
            | "image/webp"
            | "image/bmp"
            | "image/avif"
    ) {
        return Err("EPUB 资源不是受支持的图片".into());
    }
    let bytes = manifest
        .read_bytes()
        .map_err(|error| format!("无法读取 EPUB 图片：{error}"))?;
    if bytes.len() > MAX_EPUB_IMAGE_BYTES {
        return Err("EPUB 单张图片超过 50 MiB 安全限制".into());
    }
    Ok(EpubResource {
        mime_type: manifest.media_type().to_string(),
        base64: BASE64.encode(bytes),
    })
}

#[tauri::command]
pub fn close_epub(session_id: String, sessions: State<'_, EpubSessions>) -> CommandResult<()> {
    sessions
        .sessions
        .lock()
        .map_err(|_| "EPUB 会话状态不可用".to_string())?
        .remove(&session_id);
    Ok(())
}
