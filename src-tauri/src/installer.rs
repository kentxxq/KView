use std::{collections::HashMap, fs::File, io::{Read, Seek, SeekFrom}, path::Path};

use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use pelite::PeFile;
use serde::Serialize;
use crate::command_error::CommandResult;

const MAX_LOGO_BYTES: usize = 10 * 1024 * 1024;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallerDocument {
    format: &'static str,
    name: Option<String>,
    description: Option<String>,
    publisher: Option<String>,
    version: Option<String>,
    architecture: Option<String>,
    logo_data_url: Option<String>,
}

fn data_url(bytes: &[u8], name: &str) -> Option<String> {
    if bytes.is_empty() || bytes.len() > MAX_LOGO_BYTES {
        return None;
    }
    let lower = name.to_ascii_lowercase();
    let mime = if bytes.starts_with(b"\x89PNG") || lower.ends_with(".png") {
        "image/png"
    } else if bytes.starts_with(b"\xff\xd8\xff") || lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF8") || lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "image/x-icon"
    };
    Some(format!("data:{mime};base64,{}", BASE64.encode(bytes)))
}

fn icon_dimension(value: u8) -> u32 {
    if value == 0 { 256 } else { value as u32 }
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 24 || !bytes.starts_with(b"\x89PNG\r\n\x1a\n") || &bytes[12..16] != b"IHDR" {
        return None;
    }
    Some((
        u32::from_be_bytes(bytes[16..20].try_into().ok()?),
        u32::from_be_bytes(bytes[20..24].try_into().ok()?),
    ))
}

fn single_icon_data_url(
    width: u8,
    height: u8,
    color_count: u8,
    planes: u16,
    bit_count: u16,
    image: &[u8],
) -> Option<String> {
    let image_size = u32::try_from(image.len()).ok()?;
    let mut ico = Vec::with_capacity(22usize.checked_add(image.len())?);
    ico.extend_from_slice(&0u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&1u16.to_le_bytes());
    ico.extend_from_slice(&[width, height, color_count, 0]);
    ico.extend_from_slice(&planes.to_le_bytes());
    ico.extend_from_slice(&bit_count.to_le_bytes());
    ico.extend_from_slice(&image_size.to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(image);
    data_url(&ico, "app.ico")
}

fn best_icon_data_url(resources: pelite::resources::Resources<'_>) -> Option<String> {
    for (_, group) in resources.icons().filter_map(Result::ok) {
        let candidate = group.entries().iter().filter_map(|entry| {
            let image = group.image(entry.nId).ok()?;
            if image.len() != entry.bytes_in_resource() as usize || image.len() > MAX_LOGO_BYTES {
                return None;
            }
            let width = icon_dimension(entry.bWidth);
            let height = icon_dimension(entry.bHeight);
            let is_png = image.starts_with(b"\x89PNG");
            if is_png && png_dimensions(image) != Some((width, height)) {
                return None;
            }
            Some(((width * height, entry.wBitCount, is_png), entry, image, is_png))
        }).max_by_key(|(quality, _, _, _)| *quality);

        if let Some((_, entry, image, is_png)) = candidate {
            return if is_png {
                data_url(image, "app.png")
            } else {
                single_icon_data_url(
                    entry.bWidth,
                    entry.bHeight,
                    entry.bColorCount,
                    entry.wPlanes,
                    entry.wBitCount,
                    image,
                )
            };
        }
    }
    None
}

fn exe_info(path: &Path) -> Result<InstallerDocument, String> {
    let file_map = pelite::FileMap::open(path).map_err(|error| format!("无法读取 EXE：{error}"))?;
    let pe = PeFile::from_bytes(&file_map).map_err(|error| format!("无法解析 PE 文件：{error}"))?;
    let architecture = match pe.file_header().Machine {
        0x014c => Some("x86".to_string()),
        0x8664 => Some("x64".to_string()),
        0xaa64 => Some("ARM64".to_string()),
        0x01c0 | 0x01c4 => Some("ARM".to_string()),
        machine => Some(format!("未知 (0x{machine:04X})")),
    };

    let resources = pe.resources().ok();
    let file_info = resources
        .and_then(|value| value.version_info().ok())
        .map(|value| value.file_info());
    let strings = file_info
        .as_ref()
        .and_then(|info| info.strings.values().next());
    let string_value = |key: &str| strings.and_then(|values| values.get(key)).cloned().filter(|value| !value.trim().is_empty());
    let version = string_value("ProductVersion")
        .or_else(|| string_value("FileVersion"))
        .or_else(|| file_info.as_ref().and_then(|info| info.fixed.map(|fixed| fixed.dwProductVersion.to_string())));
    let logo_data_url = resources.and_then(best_icon_data_url);

    Ok(InstallerDocument {
        format: "EXE",
        name: string_value("ProductName").or_else(|| string_value("FileDescription")),
        description: string_value("FileDescription"),
        publisher: string_value("CompanyName"),
        version,
        architecture,
        logo_data_url,
    })
}

fn msi_info(path: &Path) -> Result<InstallerDocument, String> {
    let mut package = msi::open(path).map_err(|error| format!("无法解析 MSI：{error}"))?;
    let architecture = package.summary_info().arch().map(|value| match value.to_ascii_lowercase().as_str() {
        "intel" | "x86" => "x86".to_string(),
        "intel64" | "amd64" | "x64" => "x64".to_string(),
        "arm64" => "ARM64".to_string(),
        _ => value.to_string(),
    });
    let mut properties = HashMap::new();
    if package.get_table("Property").is_some() {
        let rows = package
            .select_rows(msi::Select::table("Property").columns(&["Property", "Value"]))
            .map_err(|error| format!("无法读取 MSI 属性：{error}"))?;
        for row in rows {
            if let (Some(key), Some(value)) = (row[0].as_str(), row[1].as_str()) {
                properties.insert(key.to_string(), value.to_string());
            }
        }
    }

    let stream_names: Vec<String> = package.streams().collect();
    let preferred = stream_names.iter().find(|name| {
        let lower = name.to_ascii_lowercase();
        lower.ends_with(".ico") || lower.ends_with(".png") || lower.contains("icon") || lower.contains("logo")
    });
    let logo_data_url = preferred.and_then(|name| {
        let mut bytes = Vec::new();
        package.read_stream(name).ok()?.take((MAX_LOGO_BYTES + 1) as u64).read_to_end(&mut bytes).ok()?;
        data_url(&bytes, name)
    });

    Ok(InstallerDocument {
        format: "MSI",
        name: properties.get("ProductName").cloned().or_else(|| package.summary_info().title().map(str::to_string)),
        description: properties.get("ARPCOMMENTS").cloned().or_else(|| package.summary_info().subject().map(str::to_string)),
        publisher: properties.get("Manufacturer").cloned().or_else(|| package.summary_info().author().map(str::to_string)),
        version: properties.get("ProductVersion").cloned(),
        architecture,
        logo_data_url,
    })
}

fn xml_attribute(xml: &str, element: &str, attribute: &str) -> Option<String> {
    let start = xml.find(&format!("<{element}"))?;
    let tag = &xml[start..xml[start..].find('>')? + start];
    let marker = format!("{attribute}=\"");
    let value_start = tag.find(&marker)? + marker.len();
    let value = &tag[value_start..value_start + tag[value_start..].find('"')?];
    Some(decode_xml(value))
}

fn xml_text(xml: &str, element: &str) -> Option<String> {
    let marker = format!("<{element}>");
    let start = xml.find(&marker)? + marker.len();
    let end = xml[start..].find(&format!("</{element}>"))? + start;
    Some(decode_xml(xml[start..end].trim()))
}

fn decode_xml(value: &str) -> String {
    value.replace("&amp;", "&").replace("&quot;", "\"").replace("&apos;", "'").replace("&lt;", "<").replace("&gt;", ">")
}

fn read_zip_text<R: Read + Seek>(archive: &mut zip::ZipArchive<R>, suffix: &str) -> Option<String> {
    let index = (0..archive.len()).find(|index| {
        archive
            .by_index(*index)
            .map(|entry| entry.name().eq_ignore_ascii_case(suffix))
            .unwrap_or(false)
    })?;
    let mut entry = archive.by_index(index).ok()?;
    if entry.size() > 2 * 1024 * 1024 {
        return None;
    }
    let mut text = String::new();
    entry.read_to_string(&mut text).ok()?;
    Some(text)
}

fn read_msix_logo<R: Read + Seek>(archive: &mut zip::ZipArchive<R>, manifest: &str) -> Option<String> {
    let wanted = xml_text(manifest, "Logo")?.replace('\\', "/");
    let wanted_lower = wanted.to_ascii_lowercase();
    let stem = wanted_lower.rsplit_once('.').map(|value| value.0).unwrap_or(&wanted_lower);
    let index = (0..archive.len()).find(|index| {
        let Ok(entry) = archive.by_index(*index) else { return false };
        let name = entry.name().replace('\\', "/").to_ascii_lowercase();
        name == wanted_lower || (name.starts_with(stem) && name.ends_with(".png"))
    })?;
    let entry = archive.by_index(index).ok()?;
    let entry_name = entry.name().to_string();
    let mut bytes = Vec::new();
    entry.take((MAX_LOGO_BYTES + 1) as u64).read_to_end(&mut bytes).ok()?;
    data_url(&bytes, &entry_name)
}

fn msix_bundle_info(path: &Path) -> Result<InstallerDocument, String> {
    let file = File::open(path).map_err(|error| format!("无法打开 MSIX Bundle：{error}"))?;
    let mut bundle = zip::ZipArchive::new(file).map_err(|error| format!("无法解析 MSIX Bundle：{error}"))?;
    let bundle_manifest = read_zip_text(&mut bundle, "AppxMetadata/AppxBundleManifest.xml")
        .ok_or_else(|| "MSIX Bundle 缺少 AppxBundleManifest.xml".to_string())?;
    let version = xml_attribute(&bundle_manifest, "Identity", "Version");
    let publisher = xml_attribute(&bundle_manifest, "Identity", "Publisher");
    let bundle_name = xml_attribute(&bundle_manifest, "Identity", "Name");

    let mut architectures = Vec::new();
    let mut rest = bundle_manifest.as_str();
    while let Some(position) = rest.find("<Package") {
        rest = &rest[position..];
        if let Some(architecture) = xml_attribute(rest, "Package", "Architecture") {
            if !architectures.contains(&architecture) {
                architectures.push(architecture);
            }
        }
        rest = &rest["<Package".len()..];
    }

    let package_index = (0..bundle.len()).find(|index| {
        let Ok(entry) = bundle.by_index(*index) else { return false };
        let lower = entry.name().to_ascii_lowercase();
        lower.ends_with(".msix") || lower.ends_with(".appx")
    });
    let mut display_name = None;
    let mut description = None;
    let mut publisher_display_name = None;
    let mut logo_data_url = None;
    if let Some(index) = package_index {
        let mut temporary = tempfile::tempfile().map_err(|error| format!("无法创建包解析临时文件：{error}"))?;
        std::io::copy(&mut bundle.by_index(index).map_err(|error| error.to_string())?, &mut temporary)
            .map_err(|error| format!("无法读取内嵌 MSIX：{error}"))?;
        temporary.seek(SeekFrom::Start(0)).map_err(|error| error.to_string())?;
        if let Ok(mut package) = zip::ZipArchive::new(temporary) {
            if let Some(manifest) = read_zip_text(&mut package, "AppxManifest.xml") {
                display_name = xml_text(&manifest, "DisplayName");
                description = xml_text(&manifest, "Description");
                publisher_display_name = xml_text(&manifest, "PublisherDisplayName");
                logo_data_url = read_msix_logo(&mut package, &manifest);
            }
        }
    }

    Ok(InstallerDocument {
        format: "MSIX Bundle",
        name: display_name.or(bundle_name),
        description,
        publisher: publisher_display_name.or(publisher),
        version,
        architecture: (!architectures.is_empty()).then(|| architectures.join(" / ")),
        logo_data_url,
    })
}

#[tauri::command]
pub fn read_installer(path: String) -> CommandResult<InstallerDocument> {
    let canonical = Path::new(&path).canonicalize().map_err(|error| format!("无法访问文件：{error}"))?;
    let extension = canonical.extension().and_then(|value| value.to_str()).unwrap_or_default().to_ascii_lowercase();
    Ok(match extension.as_str() {
        "exe" => exe_info(&canonical),
        "msi" => msi_info(&canonical),
        "msixbundle" => msix_bundle_info(&canonical),
        _ => Err("不支持的安装包格式".into()),
    }?)
}
