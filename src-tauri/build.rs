fn main() {
    println!("cargo:rerun-if-changed=../shared/formats.json");
    println!("cargo:rerun-if-changed=tauri.conf.json");
    println!("cargo:rerun-if-changed=tauri.windows.conf.json");
    println!("cargo:rerun-if-changed=windows/installer-hooks.nsh");
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

    let windows: serde_json::Value = serde_json::from_str(include_str!("tauri.windows.conf.json"))
        .expect("Windows 配置必须是有效 JSON");
    assert_eq!(windows.pointer("/bundle/fileAssociations"), Some(&serde_json::json!([])),
        "Windows 安装包不得自动注册文件关联");
    let mut cleanup_extensions = include_str!("windows/installer-hooks.nsh").lines()
        .filter_map(|line| line.trim().strip_prefix("!insertmacro ${ACTION} \""))
        .filter_map(|line| line.strip_suffix('"'))
        .map(str::to_owned).collect::<Vec<_>>();
    cleanup_extensions.sort();
    cleanup_extensions.dedup();
    let mut windows_extensions = Vec::new();
    for item in catalog.as_array().into_iter().flatten() {
        if item.get("associable").and_then(serde_json::Value::as_bool) != Some(true)
            || !item["platforms"].as_array().is_some_and(|platforms| platforms.iter().any(|value| value == "windows")) {
            continue;
        }
        for key in ["extensions", "associationExtensions"] {
            windows_extensions.extend(item[key].as_array().into_iter().flatten()
                .filter_map(serde_json::Value::as_str).map(str::to_owned));
        }
    }
    windows_extensions.sort();
    windows_extensions.dedup();
    assert_eq!(cleanup_extensions, windows_extensions, "卸载清理格式必须与 Windows 支持注册的格式一致");
}
