// 仅识别交给系统 WebView 解码的新增图片，不在应用内解码像素。
pub(crate) fn detect_native_image(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(b"BM") {
        return Some(("bmp", "image/bmp"));
    }
    if bytes.starts_with(&[0, 0, 1, 0]) {
        return Some(("ico", "image/vnd.microsoft.icon"));
    }
    if bytes.starts_with(b"II\x2a\0") || bytes.starts_with(b"MM\0\x2a")
        || bytes.starts_with(b"II\x2b\0") || bytes.starts_with(b"MM\0\x2b")
    {
        return Some(("tiff", "image/tiff"));
    }
    if bytes.starts_with(&[0xff, 0x0a]) || bytes.starts_with(b"\0\0\0\x0cJXL \r\n\x87\n") {
        return Some(("jxl", "image/jxl"));
    }
    if bytes.len() < 16 || &bytes[4..8] != b"ftyp" {
        return None;
    }
    let size = u32::from_be_bytes(bytes[..4].try_into().ok()?);
    let (header, size) = match size {
        0 => (8, bytes.len()),
        1 => {
            let extended = u64::from_be_bytes(bytes.get(8..16)?.try_into().ok()?);
            (16, usize::try_from(extended).ok()?)
        }
        value => (8, value as usize),
    };
    let payload = bytes.get(header..size)?;
    if payload.len() < 8 || (payload.len() - 8) % 4 != 0 {
        return None;
    }
    let is_avif = |brand: &[u8]| brand == b"avif" || brand == b"avis";
    if is_avif(&payload[..4]) || payload[8..].chunks_exact(4).any(is_avif) {
        return Some(("avif", "image/avif"));
    }
    None
}
