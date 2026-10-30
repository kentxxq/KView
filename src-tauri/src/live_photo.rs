use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::UNIX_EPOCH,
};

use hpvcd::Decoder;
use serde::Serialize;
use zip::ZipArchive;

#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::RPC_E_CHANGED_MODE,
    Graphics::Imaging::{
        CLSID_WICImagingFactory, GUID_WICPixelFormat24bppBGR, IWICImagingFactory,
        IWICPalette, WICBitmapDitherTypeNone, WICBitmapPaletteTypeCustom,
        WICDecodeMetadataCacheOnLoad,
    },
    System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    },
    UI::Shell::SHCreateMemStream,
};

pub(crate) const MAX_STILL_BYTES: u64 = 100 * 1024 * 1024;
pub(crate) const MAX_VIDEO_BYTES: u64 = 200 * 1024 * 1024;
const MAX_LIVP_BYTES: u64 = 300 * 1024 * 1024;
const MAX_IMAGE_PIXELS: u64 = 64_000_000;
const MAX_IMAGE_DIMENSION: u32 = 16_384;
const METADATA_SCAN_BYTES: u64 = 4 * 1024 * 1024;
const PREVIEW_CACHE_BYTES: usize = 256 * 1024 * 1024;
const PREVIEW_CACHE_ENTRIES: usize = 8;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct LivePhotoBinaryHeader {
    image_length: usize,
    image_mime_type: &'static str,
    video_mime_type: &'static str,
    source_kind: &'static str,
}

#[derive(Clone, PartialEq, Eq)]
struct PreviewCacheKey {
    path: PathBuf,
    size: u64,
    modified_nanos: u128,
}

struct PreviewCacheEntry {
    key: PreviewCacheKey,
    bytes: Arc<Vec<u8>>,
}

#[derive(Default)]
struct PreviewCache {
    entries: VecDeque<PreviewCacheEntry>,
    total_bytes: usize,
}

static PREVIEW_CACHE: OnceLock<Mutex<PreviewCache>> = OnceLock::new();

struct MediaResource {
    name: String,
    extension: String,
    bytes: Vec<u8>,
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

pub(crate) fn is_still_extension(extension: &str) -> bool {
    matches!(extension, "jpg" | "jpeg" | "heic" | "heif")
}

fn is_video_extension(extension: &str) -> bool {
    matches!(extension, "mov" | "mp4")
}

fn media_stem(name: &str) -> String {
    Path::new(name)
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn is_uuid_at(bytes: &[u8], start: usize) -> bool {
    if start + 36 > bytes.len() {
        return false;
    }
    for (index, byte) in bytes[start..start + 36].iter().enumerate() {
        if matches!(index, 8 | 13 | 18 | 23) {
            if *byte != b'-' {
                return false;
            }
        } else if !byte.is_ascii_hexdigit() {
            return false;
        }
    }
    true
}

fn asset_identifiers(bytes: &[u8]) -> HashSet<String> {
    let mut identifiers = HashSet::new();
    for start in 0..bytes.len().saturating_sub(35) {
        if is_uuid_at(bytes, start) {
            identifiers.insert(
                String::from_utf8_lossy(&bytes[start..start + 36]).to_ascii_lowercase(),
            );
        }
    }
    identifiers
}

fn read_metadata_regions(path: &Path) -> Result<Vec<u8>, String> {
    let mut file = File::open(path).map_err(|error| format!("无法读取媒体元数据：{error}"))?;
    let size = file
        .metadata()
        .map_err(|error| format!("无法读取媒体信息：{error}"))?
        .len();
    let head_len = size.min(METADATA_SCAN_BYTES) as usize;
    let mut bytes = vec![0; head_len];
    file.read_exact(&mut bytes)
        .map_err(|error| format!("无法读取媒体元数据：{error}"))?;
    if size > METADATA_SCAN_BYTES {
        let tail_len = (size - METADATA_SCAN_BYTES).min(METADATA_SCAN_BYTES) as usize;
        file.seek(SeekFrom::End(-(tail_len as i64)))
            .map_err(|error| format!("无法定位媒体元数据：{error}"))?;
        let mut tail = vec![0; tail_len];
        file.read_exact(&mut tail)
            .map_err(|error| format!("无法读取媒体元数据：{error}"))?;
        bytes.extend_from_slice(&tail);
    }
    Ok(bytes)
}

fn identifiers_from_path(path: &Path) -> HashSet<String> {
    read_metadata_regions(path)
        .map(|bytes| asset_identifiers(&bytes))
        .unwrap_or_default()
}

fn identifiers_match(still: &HashSet<String>, video: &HashSet<String>) -> bool {
    still.iter().any(|identifier| video.contains(identifier))
}

fn identifiers_conflict(still: &HashSet<String>, video: &HashSet<String>) -> bool {
    !still.is_empty() && !video.is_empty() && !identifiers_match(still, video)
}

fn candidate_videos(still_path: &Path) -> Vec<PathBuf> {
    let Some(parent) = still_path.parent() else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(parent) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.metadata()
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= MAX_VIDEO_BYTES)
                && is_video_extension(&extension(path))
        })
        .collect()
}

pub(crate) fn paired_video_for_still(still_path: &Path) -> Option<PathBuf> {
    if !is_still_extension(&extension(still_path)) {
        return None;
    }
    let videos = candidate_videos(still_path);
    if videos.is_empty() {
        return None;
    }
    let still_stem = media_stem(
        still_path
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default(),
    );
    let still_ids = identifiers_from_path(still_path);
    let same_stem: Vec<_> = videos
        .iter()
        .filter(|path| {
            media_stem(path.file_name().and_then(|value| value.to_str()).unwrap_or_default())
                == still_stem
        })
        .collect();
    if same_stem.len() == 1 {
        let video_ids = identifiers_from_path(same_stem[0]);
        if !identifiers_conflict(&still_ids, &video_ids) {
            return Some(same_stem[0].clone());
        }
    }
    if still_ids.is_empty() {
        return None;
    }
    let matching: Vec<_> = videos
        .into_iter()
        .filter(|path| identifiers_match(&still_ids, &identifiers_from_path(path)))
        .collect();
    (matching.len() == 1).then(|| matching[0].clone())
}

pub(crate) fn paired_video_paths(paths: &[PathBuf]) -> HashSet<PathBuf> {
    let stills: Vec<_> = paths
        .iter()
        .filter(|path| is_still_extension(&extension(path)))
        .collect();
    let videos: Vec<_> = paths
        .iter()
        .filter(|path| is_video_extension(&extension(path)))
        .collect();
    let mut paired = HashSet::new();
    let mut unmatched_videos = Vec::new();
    let still_stems: HashSet<_> = stills
        .iter()
        .map(|path| {
            media_stem(path.file_name().and_then(|value| value.to_str()).unwrap_or_default())
        })
        .collect();
    for video in &videos {
        let stem =
            media_stem(video.file_name().and_then(|value| value.to_str()).unwrap_or_default());
        if still_stems.contains(&stem) {
            paired.insert((*video).clone());
        } else {
            unmatched_videos.push(*video);
        }
    }

    let mut videos_by_identifier: HashMap<String, Vec<&PathBuf>> = HashMap::new();
    for video in unmatched_videos {
        for identifier in identifiers_from_path(video) {
            videos_by_identifier
                .entry(identifier)
                .or_default()
                .push(video);
        }
    }
    if videos_by_identifier.is_empty() {
        return paired;
    }
    for still in stills {
        for identifier in identifiers_from_path(still) {
            if let Some(matches) = videos_by_identifier.get(&identifier) {
                if matches.len() == 1 {
                    paired.insert((*matches[0]).clone());
                }
            }
        }
    }
    paired
}

fn parse_attribute_number(tag: &str, local_name: &str) -> Option<usize> {
    for quote in ['"', '\''] {
        let marker = format!(":{local_name}={quote}");
        if let Some(start) = tag.find(&marker) {
            let value_start = start + marker.len();
            let value_end = tag[value_start..].find(quote)? + value_start;
            return tag[value_start..value_end].parse().ok();
        }
    }
    None
}

fn parse_element_number(xmp: &str, local_name: &str) -> Option<usize> {
    let marker = format!(":{local_name}>");
    let start = xmp.find(&marker)? + marker.len();
    let end = xmp[start..].find('<')? + start;
    xmp[start..end].trim().parse().ok()
}

fn motion_video_length_in_xmp(xmp: &str) -> Option<usize> {
    let mut offset = 0;
    while let Some(relative) = xmp[offset..].find("MotionPhoto") {
        let position = offset + relative;
        let tag_start = xmp[..position].rfind('<')?;
        let tag_end = xmp[position..].find('>')? + position;
        if let Some(length) = parse_attribute_number(&xmp[tag_start..=tag_end], "Length") {
            return Some(length);
        }
        offset = position + "MotionPhoto".len();
    }
    parse_attribute_number(xmp, "MicroVideoOffset")
        .or_else(|| parse_element_number(xmp, "MicroVideoOffset"))
}

fn find_bytes(haystack: &[u8], needle: &[u8], start: usize) -> Option<usize> {
    haystack
        .get(start..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|position| start + position)
}

fn motion_video_length(bytes: &[u8]) -> Option<usize> {
    for marker in [b"MotionPhoto".as_slice(), b"MicroVideoOffset".as_slice()] {
        let mut offset = 0;
        while let Some(position) = find_bytes(bytes, marker, offset) {
            let window_start = position.saturating_sub(2048);
            let window_end = (position + 8192).min(bytes.len());
            let xmp = String::from_utf8_lossy(&bytes[window_start..window_end]);
            if let Some(length) = motion_video_length_in_xmp(&xmp) {
                return Some(length);
            }
            offset = position + marker.len();
        }
    }
    None
}

fn video_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 12 || &bytes[4..8] != b"ftyp" {
        return None;
    }
    if &bytes[8..12] == b"qt  " {
        Some("video/quicktime")
    } else {
        Some("video/mp4")
    }
}

fn motion_video(bytes: &[u8]) -> Option<(usize, &'static str)> {
    let length = motion_video_length(bytes)?;
    if length == 0 || length > bytes.len() || length as u64 > MAX_VIDEO_BYTES {
        return None;
    }
    let start = bytes.len() - length;
    video_mime(&bytes[start..]).map(|mime| (start, mime))
}

fn motion_video_length_from_file(file: &mut File, size: usize) -> Result<Option<usize>, String> {
    const CHUNK_BYTES: usize = 64 * 1024;
    const BEFORE: usize = 2048;
    const AFTER: usize = 8192;
    let markers = [b"MotionPhoto".as_slice(), b"MicroVideoOffset".as_slice()];
    let mut lengths = [None, None];
    let mut bytes = Vec::with_capacity(CHUNK_BYTES + BEFORE + AFTER);
    let mut region_start = 0;
    for start in (0..size).step_by(CHUNK_BYTES) {
        let end = (start + CHUNK_BYTES).min(size);
        let region_end = (end + AFTER).min(size);
        let retained = bytes.len();
        bytes.resize(region_end - region_start, 0);
        file.read_exact(&mut bytes[retained..])
            .map_err(|error| format!("无法读取动态照片元数据：{error}"))?;
        for (index, marker) in markers.iter().enumerate() {
            if lengths[index].is_some() {
                continue;
            }
            let mut offset = start - region_start;
            while let Some(position) = find_bytes(&bytes, marker, offset) {
                if region_start + position >= end {
                    break;
                }
                let window_start = position.saturating_sub(BEFORE);
                let window_end = (position + AFTER).min(bytes.len());
                let xmp = String::from_utf8_lossy(&bytes[window_start..window_end]);
                if let Some(length) = motion_video_length_in_xmp(&xmp) {
                    lengths[index] = Some(length);
                    break;
                }
                offset = position + marker.len();
            }
        }
        // 保持原有优先级：全文件中的 MotionPhoto 优先于 MicroVideoOffset。
        if lengths[0].is_some() {
            return Ok(lengths[0]);
        }
        let next_region_start = end.saturating_sub(BEFORE);
        bytes.drain(..next_region_start - region_start);
        region_start = next_region_start;
    }
    Ok(lengths[1])
}

fn has_motion_video(path: &Path) -> Result<bool, String> {
    let mut file = File::open(path).map_err(|error| format!("无法读取图片：{error}"))?;
    let metadata = file.metadata().map_err(|error| format!("无法读取图片信息：{error}"))?;
    if !metadata.is_file() || metadata.len() > MAX_STILL_BYTES {
        return Ok(false);
    }
    let size = metadata.len() as usize;
    let Some(length) = motion_video_length_from_file(&mut file, size)? else {
        return Ok(false);
    };
    if length < 12 || length > size || length as u64 > MAX_VIDEO_BYTES {
        return Ok(false);
    }
    file.seek(SeekFrom::Start((size - length) as u64))
        .map_err(|error| format!("无法定位动态照片视频：{error}"))?;
    let mut header = [0; 12];
    file.read_exact(&mut header)
        .map_err(|error| format!("无法读取动态照片视频头：{error}"))?;
    Ok(video_mime(&header).is_some())
}

fn read_limited(path: &Path, limit: u64, description: &str) -> Result<Vec<u8>, String> {
    let metadata = path
        .metadata()
        .map_err(|error| format!("无法读取{description}信息：{error}"))?;
    if !metadata.is_file() {
        return Err(format!("所选{description}不是文件"));
    }
    if metadata.len() > limit {
        return Err(format!("{description}超过安全大小限制"));
    }
    std::fs::read(path).map_err(|error| format!("无法读取{description}：{error}"))
}

fn preview_cache_key(path: &Path) -> Result<PreviewCacheKey, String> {
    let metadata = path
        .metadata()
        .map_err(|error| format!("无法读取图片缓存信息：{error}"))?;
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Ok(PreviewCacheKey {
        path: path.to_path_buf(),
        size: metadata.len(),
        modified_nanos,
    })
}

fn cached_preview(key: &PreviewCacheKey) -> Option<Arc<Vec<u8>>> {
    let cache = PREVIEW_CACHE.get_or_init(|| Mutex::new(PreviewCache::default()));
    let mut cache = cache.lock().ok()?;
    let index = cache.entries.iter().position(|entry| &entry.key == key)?;
    let entry = cache.entries.remove(index)?;
    let bytes = Arc::clone(&entry.bytes);
    cache.entries.push_back(entry);
    Some(bytes)
}

fn cache_preview(key: PreviewCacheKey, bytes: Vec<u8>) -> Arc<Vec<u8>> {
    let bytes = Arc::new(bytes);
    if bytes.len() > PREVIEW_CACHE_BYTES {
        return bytes;
    }
    let cache = PREVIEW_CACHE.get_or_init(|| Mutex::new(PreviewCache::default()));
    let Ok(mut cache) = cache.lock() else {
        return bytes;
    };
    while !cache.entries.is_empty()
        && (cache.entries.len() >= PREVIEW_CACHE_ENTRIES
            || cache.total_bytes + bytes.len() > PREVIEW_CACHE_BYTES)
    {
        if let Some(entry) = cache.entries.pop_front() {
            cache.total_bytes = cache.total_bytes.saturating_sub(entry.bytes.len());
        }
    }
    cache.total_bytes += bytes.len();
    cache.entries.push_back(PreviewCacheEntry {
        key,
        bytes: Arc::clone(&bytes),
    });
    bytes
}

fn checked_bitmap_layout(width: u32, height: u32) -> Result<(usize, usize), String> {
    if width == 0
        || height == 0
        || width > MAX_IMAGE_DIMENSION
        || height > MAX_IMAGE_DIMENSION
        || u64::from(width) * u64::from(height) > MAX_IMAGE_PIXELS
    {
        return Err("HEIC/HEIF 图片尺寸超过安全限制".into());
    }
    let row_bytes = usize::try_from(width)
        .ok()
        .and_then(|value| value.checked_mul(3))
        .ok_or_else(|| "图片行大小溢出".to_string())?;
    let stride = row_bytes
        .checked_add(3)
        .map(|value| value & !3)
        .ok_or_else(|| "图片步长溢出".to_string())?;
    let pixel_bytes = stride
        .checked_mul(height as usize)
        .ok_or_else(|| "图片像素大小溢出".to_string())?;
    Ok((stride, pixel_bytes))
}

fn bmp_from_bgr(width: u32, height: u32, stride: usize, pixels: &[u8]) -> Result<Vec<u8>, String> {
    const HEADER_BYTES: usize = 54;
    let expected = stride
        .checked_mul(height as usize)
        .ok_or_else(|| "BMP 像素大小溢出".to_string())?;
    if pixels.len() != expected {
        return Err("BMP 像素数据长度无效".into());
    }
    let file_size = HEADER_BYTES
        .checked_add(expected)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| "BMP 文件大小溢出".to_string())?;
    let mut bmp = vec![0_u8; HEADER_BYTES];
    bmp[0..2].copy_from_slice(b"BM");
    bmp[2..6].copy_from_slice(&file_size.to_le_bytes());
    bmp[10..14].copy_from_slice(&(HEADER_BYTES as u32).to_le_bytes());
    bmp[14..18].copy_from_slice(&40_u32.to_le_bytes());
    bmp[18..22].copy_from_slice(&(width as i32).to_le_bytes());
    bmp[22..26].copy_from_slice(&(-(height as i32)).to_le_bytes());
    bmp[26..28].copy_from_slice(&1_u16.to_le_bytes());
    bmp[28..30].copy_from_slice(&24_u16.to_le_bytes());
    bmp[34..38].copy_from_slice(&(expected as u32).to_le_bytes());
    bmp.extend_from_slice(pixels);
    Ok(bmp)
}

fn decode_heic_with_rust(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let decoded = Decoder::new()
        .primary_only()
        .with_max_dimension(MAX_IMAGE_DIMENSION)
        .with_max_pixels(MAX_IMAGE_PIXELS)
        .decode_rgb8(bytes)
        .map_err(|error| format!("纯 Rust 解码失败：{error}"))?;
    let (stride, pixel_bytes) = checked_bitmap_layout(decoded.width, decoded.height)?;
    let row_bytes = decoded.width as usize * 3;
    let mut bgr = vec![0_u8; pixel_bytes];
    for (source_row, target_row) in decoded
        .pixels
        .chunks_exact(row_bytes)
        .zip(bgr.chunks_exact_mut(stride))
    {
        for (rgb, bgr) in source_row.chunks_exact(3).zip(target_row.chunks_exact_mut(3)) {
            bgr.copy_from_slice(&[rgb[2], rgb[1], rgb[0]]);
        }
    }
    bmp_from_bgr(decoded.width, decoded.height, stride, &bgr)
}

#[cfg(target_os = "windows")]
struct ComInitialization(bool);

#[cfg(target_os = "windows")]
impl Drop for ComInitialization {
    fn drop(&mut self) {
        if self.0 {
            unsafe { CoUninitialize() };
        }
    }
}

#[cfg(target_os = "windows")]
fn decode_heic_with_wic(bytes: &[u8]) -> Result<Vec<u8>, String> {
    unsafe {
        let result = CoInitializeEx(None, COINIT_MULTITHREADED);
        let initialized = result.is_ok();
        if !initialized && result != RPC_E_CHANGED_MODE {
            return Err(format!("无法初始化 Windows 图片解码环境：{result:?}"));
        }
        let _initialization = ComInitialization(initialized);
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)
                .map_err(|error| format!("无法创建 Windows 图片解码器：{error}"))?;
        let stream = SHCreateMemStream(Some(bytes))
            .ok_or_else(|| "无法创建 HEIC 内存流".to_string())?;
        let decoder = factory
            .CreateDecoderFromStream(
                &stream,
                std::ptr::null(),
                WICDecodeMetadataCacheOnLoad,
            )
            .map_err(|error| format!("系统未提供可用的 HEIC 解码器：{error}"))?;
        let frame = decoder
            .GetFrame(0)
            .map_err(|error| format!("无法读取 HEIC 主图像：{error}"))?;
        let converter = factory
            .CreateFormatConverter()
            .map_err(|error| format!("无法创建 HEIC 像素转换器：{error}"))?;
        converter
            .Initialize(
                &frame,
                &GUID_WICPixelFormat24bppBGR,
                WICBitmapDitherTypeNone,
                None::<&IWICPalette>,
                0.0,
                WICBitmapPaletteTypeCustom,
            )
            .map_err(|error| format!("系统 HEIC 像素转换失败：{error}"))?;
        let mut width = 0;
        let mut height = 0;
        converter
            .GetSize(&mut width, &mut height)
            .map_err(|error| format!("无法读取 HEIC 图片尺寸：{error}"))?;
        let (stride, pixel_bytes) = checked_bitmap_layout(width, height)?;
        let mut pixels = vec![0_u8; pixel_bytes];
        converter
            .CopyPixels(std::ptr::null(), stride as u32, &mut pixels)
            .map_err(|error| format!("Windows HEIC 解码失败：{error}"))?;
        bmp_from_bgr(width, height, stride, &pixels)
    }
}

fn decode_heic_preview(bytes: &[u8]) -> Result<Vec<u8>, String> {
    #[cfg(target_os = "windows")]
    {
        if let Ok(preview) = decode_heic_with_wic(bytes) {
            return Ok(preview);
        }
    }
    decode_heic_with_rust(bytes).map_err(|error| format!("无法解码 HEIC/HEIF 图片：{error}"))
}

fn heic_preview(path: &Path, bytes: &[u8]) -> Result<Arc<Vec<u8>>, String> {
    let key = preview_cache_key(path)?;
    if let Some(preview) = cached_preview(&key) {
        return Ok(preview);
    }
    let preview = decode_heic_preview(bytes)?;
    Ok(cache_preview(key, preview))
}

pub(crate) fn read_heic_preview(path: &Path) -> Result<Vec<u8>, String> {
    if !matches!(extension(path).as_str(), "heic" | "heif") {
        return Err("仅支持生成 HEIC/HEIF 预览".into());
    }
    let bytes = read_limited(path, MAX_STILL_BYTES, "图片")?;
    Ok((*heic_preview(path, &bytes)?).clone())
}

fn still_preview(
    cache_path: &Path,
    bytes: &[u8],
    extension: &str,
) -> Result<(Vec<u8>, &'static str), String> {
    match extension {
        "jpg" | "jpeg" => Ok((bytes.to_vec(), "image/jpeg")),
        "heic" | "heif" => Ok(((*heic_preview(cache_path, bytes)?).clone(), "image/bmp")),
        _ => Err("不支持的 Live Photo 静态图片格式".into()),
    }
}

fn pack_live_photo(
    image: &[u8],
    image_mime_type: &'static str,
    video: &[u8],
    video_mime_type: &'static str,
    source_kind: &'static str,
) -> Result<Vec<u8>, String> {
    let header = serde_json::to_vec(&LivePhotoBinaryHeader {
        image_length: image.len(),
        image_mime_type,
        video_mime_type,
        source_kind,
    })
    .map_err(|error| format!("无法创建动态照片数据头：{error}"))?;
    let header_length =
        u32::try_from(header.len()).map_err(|_| "动态照片数据头过大".to_string())?;
    let capacity = 4_usize
        .checked_add(header.len())
        .and_then(|value| value.checked_add(image.len()))
        .and_then(|value| value.checked_add(video.len()))
        .ok_or_else(|| "动态照片数据大小溢出".to_string())?;
    let mut payload = Vec::with_capacity(capacity);
    payload.extend_from_slice(&header_length.to_le_bytes());
    payload.extend_from_slice(&header);
    payload.extend_from_slice(image);
    payload.extend_from_slice(video);
    Ok(payload)
}

fn livp_resources(path: &Path, include_bytes: bool) -> Result<Vec<MediaResource>, String> {
    let metadata = path
        .metadata()
        .map_err(|error| format!("无法读取 LIVP 信息：{error}"))?;
    if metadata.len() > MAX_LIVP_BYTES {
        return Err("LIVP 文件超过 300 MiB 安全限制".into());
    }
    let file = File::open(path).map_err(|error| format!("无法读取 LIVP：{error}"))?;
    let mut archive =
        ZipArchive::new(file).map_err(|error| format!("LIVP 不是有效的 ZIP 容器：{error}"))?;
    let mut resources = Vec::new();
    let mut total_size = 0_u64;
    for index in 0..archive.len() {
        let entry = archive
            .by_index_data(index)
            .map_err(|error| format!("无法读取 LIVP 条目：{error}"))?;
        if entry.is_dir() {
            continue;
        }
        if entry.encrypted() {
            return Err("不支持加密的 LIVP 媒体资源".into());
        }
        let name = entry.name().map_err(|error| format!("无法读取 LIVP 条目名称：{error}"))?.into_owned();
        if entry.enclosed_name().is_none() {
            return Err("LIVP 包含不安全的文件路径".into());
        }
        let resource_extension = extension(Path::new(&name));
        let limit = if is_still_extension(&resource_extension) {
            MAX_STILL_BYTES
        } else if is_video_extension(&resource_extension) {
            MAX_VIDEO_BYTES
        } else {
            continue;
        };
        if entry.size() > limit {
            return Err(format!("LIVP 内的 {name} 超过安全大小限制"));
        }
        total_size = total_size
            .checked_add(entry.size())
            .ok_or_else(|| "LIVP 解压大小溢出".to_string())?;
        if total_size > MAX_LIVP_BYTES {
            return Err("LIVP 解压后的媒体总量超过 300 MiB 安全限制".into());
        }
        let mut bytes = Vec::new();
        if include_bytes {
            archive.by_index(index)
                .map_err(|error| format!("无法读取 LIVP 条目：{error}"))?
                .take(limit + 1)
                .read_to_end(&mut bytes)
                .map_err(|error| format!("无法读取 LIVP 内的 {name}：{error}"))?;
            if bytes.len() as u64 > limit {
                return Err(format!("LIVP 内的 {name} 解压后超过安全大小限制"));
            }
        }
        resources.push(MediaResource {
            name,
            extension: resource_extension,
            bytes,
        });
    }
    Ok(resources)
}

fn choose_resource_pair(
    resources: &[MediaResource],
) -> Result<(&MediaResource, &MediaResource), String> {
    let stills: Vec<_> = resources
        .iter()
        .filter(|resource| is_still_extension(&resource.extension))
        .collect();
    let videos: Vec<_> = resources
        .iter()
        .filter(|resource| is_video_extension(&resource.extension))
        .collect();
    if stills.is_empty() || videos.is_empty() {
        return Err("LIVP 中缺少静态图片或动态视频".into());
    }

    let mut identifier_matches = Vec::new();
    for still in &stills {
        let still_ids = asset_identifiers(&still.bytes);
        for video in &videos {
            if identifiers_match(&still_ids, &asset_identifiers(&video.bytes)) {
                identifier_matches.push((*still, *video));
            }
        }
    }
    if identifier_matches.len() == 1 {
        return Ok(identifier_matches[0]);
    }

    let same_name_matches: Vec<_> = stills
        .iter()
        .flat_map(|still| {
            videos
                .iter()
                .filter(move |video| media_stem(&still.name) == media_stem(&video.name))
                .map(move |video| (*still, *video))
        })
        .collect();
    if same_name_matches.len() == 1 {
        return Ok(same_name_matches[0]);
    }
    if stills.len() == 1 && videos.len() == 1 {
        return Ok((stills[0], videos[0]));
    }
    Err("LIVP 中存在多组媒体，无法确定对应的图片和视频".into())
}

fn livp_has_pair(path: &Path) -> bool {
    livp_resources(path, false)
        .and_then(|resources| choose_resource_pair(&resources).map(|_| ()))
        .is_ok()
}

pub(crate) fn is_live_photo(path: &Path) -> Result<bool, String> {
    Ok(match extension(path).as_str() {
        "livp" => livp_has_pair(path),
        extension if is_still_extension(extension) => {
            let has_motion_video = has_motion_video(path)?;
            has_motion_video || paired_video_for_still(path).is_some()
        }
        _ => false,
    })
}

pub(crate) fn read_live_photo(path: &Path) -> Result<Vec<u8>, String> {
    match extension(path).as_str() {
        "livp" => {
            let resources = livp_resources(path, true)?;
            let (still, video) = choose_resource_pair(&resources)?;
            let (image, image_mime_type) =
                still_preview(path, &still.bytes, &still.extension)?;
            let video_mime_type = video_mime(&video.bytes)
                .ok_or_else(|| "LIVP 内的动态资源不是有效的 MOV/MP4 视频".to_string())?;
            pack_live_photo(
                &image,
                image_mime_type,
                &video.bytes,
                video_mime_type,
                "livp",
            )
        }
        still_extension if is_still_extension(still_extension) => {
            let still_bytes = read_limited(path, MAX_STILL_BYTES, "图片")?;
            if let Some((video_start, video_mime_type)) = motion_video(&still_bytes) {
                let (image, image_mime_type) =
                    still_preview(path, &still_bytes[..video_start], still_extension)?;
                return pack_live_photo(
                    &image,
                    image_mime_type,
                    &still_bytes[video_start..],
                    video_mime_type,
                    "motion-photo",
                );
            }
            let video_path = paired_video_for_still(path)
                .ok_or_else(|| "没有找到与图片配对的 MOV/MP4 文件".to_string())?;
            let video_bytes = read_limited(&video_path, MAX_VIDEO_BYTES, "动态视频")?;
            let video_mime_type = video_mime(&video_bytes)
                .ok_or_else(|| "配对资源不是有效的 MOV/MP4 视频".to_string())?;
            let (image, image_mime_type) =
                still_preview(path, &still_bytes, still_extension)?;
            pack_live_photo(
                &image,
                image_mime_type,
                &video_bytes,
                video_mime_type,
                "paired",
            )
        }
        _ => Err("不支持的 Live Photo 格式".into()),
    }
}
