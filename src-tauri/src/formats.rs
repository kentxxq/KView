use serde::Deserialize;
use std::sync::OnceLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FormatCapability {
    viewer_kind: String,
    extensions: Vec<String>,
    #[serde(default)]
    file_names: Vec<String>,
    #[serde(default)]
    file_suffixes: Vec<String>,
    platforms: Vec<String>,
}

fn catalog() -> &'static Vec<FormatCapability> {
    static CATALOG: OnceLock<Vec<FormatCapability>> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("../../shared/formats.json"))
            .expect("shared/formats.json must be valid")
    })
}

pub(crate) fn viewer_kind(extension: &str, file_name: &str) -> String {
    let lower_name = file_name.to_ascii_lowercase();
    #[cfg(target_os = "windows")]
    let platform = "windows";
    #[cfg(target_os = "macos")]
    let platform = "macos";
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let platform = "unsupported";
    catalog()
        .iter()
        .find(|capability| {
            capability.platforms.iter().any(|value| value == platform)
                && (capability.extensions.iter().any(|value| value == extension)
                    || capability.file_names.iter().any(|value| value.eq_ignore_ascii_case(file_name))
                    || capability.file_suffixes.iter().any(|value| lower_name.ends_with(value)))
        })
        .map(|capability| capability.viewer_kind.clone())
        .unwrap_or_else(|| "text".into())
}
