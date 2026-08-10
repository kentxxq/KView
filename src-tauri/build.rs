fn main() {
    validate_format_associations();
    tauri_build::build()
}

fn validate_format_associations() {
    let catalog: serde_json::Value = serde_json::from_str(include_str!("../shared/formats.json"))
        .expect("shared/formats.json must be valid JSON");
    let config: serde_json::Value = serde_json::from_str(include_str!("tauri.conf.json"))
        .expect("tauri.conf.json must be valid JSON");

    let mut expected = Vec::new();
    for item in catalog.as_array().into_iter().flatten() {
        if item.get("associable").and_then(serde_json::Value::as_bool) != Some(true) {
            continue;
        }
        for key in ["extensions", "associationExtensions"] {
            for extension in item.get(key).and_then(serde_json::Value::as_array).into_iter().flatten() {
                if let Some(extension) = extension.as_str() {
                    expected.push(extension.to_owned());
                }
            }
        }
    }
    expected.sort();
    expected.dedup();

    let mut actual = config.pointer("/bundle/fileAssociations/0/ext")
        .and_then(serde_json::Value::as_array)
        .into_iter().flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    actual.sort();
    actual.dedup();

    assert_eq!(actual, expected, "tauri.conf.json file associations must match shared/formats.json");
}
