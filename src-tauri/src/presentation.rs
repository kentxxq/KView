use office_oxide::{Document, format::DocumentFormat, ir::DocumentIR, ir_render::{HtmlOptions, ImageEmbed}};
use serde::Serialize;

use crate::command_error::CommandResult;
use crate::file_access::{local_file_metadata, modified_millis};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationSlide {
    index: usize,
    title: Option<String>,
    html: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationDocument {
    slides: Vec<PresentationSlide>,
}

#[tauri::command]
pub async fn read_presentation(path: String, expected_modified_at: u64) -> CommandResult<PresentationDocument> {
    tauri::async_runtime::spawn_blocking(move || parse_presentation(&path, expected_modified_at))
        .await.map_err(|error| format!("演示文稿后台解析任务失败：{error}"))?
}

fn parse_presentation(path: &str, expected_modified_at: u64) -> CommandResult<PresentationDocument> {
    let (canonical, metadata) = local_file_metadata(path)?;
    if metadata.len() > 200 * 1024 * 1024 {
        return Err("演示文稿超过 200 MiB 安全限制".into());
    }
    if modified_millis(&metadata) != expected_modified_at {
        return Err("文件已被外部修改，请重新打开".into());
    }
    if !canonical.extension().and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("ppt") || value.eq_ignore_ascii_case("pptx"))
    {
        return Err("演示文稿读取器仅支持 .ppt 和 .pptx 文件".into());
    }
    let document = Document::open(&canonical).map_err(|error| format!("无法解析演示文稿：{error}"))?;
    if !matches!(document.format(), DocumentFormat::Ppt | DocumentFormat::Pptx) {
        return Err("文件内容不是有效的 PPT/PPTX 演示文稿".into());
    }
    let ir = document.to_ir();
    if ir.sections.is_empty() {
        return Err("演示文稿不包含幻灯片".into());
    }
    let mut slides = Vec::with_capacity(ir.sections.len());
    for (index, mut section) in ir.sections.into_iter().enumerate() {
        // 演讲者备注不能进入面向观众的预览，也不通过 IPC 传输。
        section.speaker_notes = None;
        let title = section.title.clone();
        let page = DocumentIR { metadata: ir.metadata.clone(), sections: vec![section] };
        slides.push(PresentationSlide {
            index: index + 1,
            title,
            html: page.to_html_with(HtmlOptions { image_embed: ImageEmbed::Base64 }),
        });
    }
    let after = canonical.metadata().map_err(|error| format!("无法复核文件信息：{error}"))?;
    if modified_millis(&after) != expected_modified_at || after.len() != metadata.len() {
        return Err("文件在解析期间被外部修改，请重新打开".into());
    }
    Ok(PresentationDocument { slides })
}
