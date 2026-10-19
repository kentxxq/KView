use crate::command_error::CommandResult;

#[tauri::command]
pub(crate) fn register_file_associations() -> CommandResult<()> {
    #[cfg(target_os = "windows")]
    {
        windows::register().map_err(Into::into)
    }
    #[cfg(not(target_os = "windows"))]
    Err("文件关联注册仅支持 Windows".into())
}

#[tauri::command]
pub(crate) fn unregister_file_associations() -> CommandResult<()> {
    #[cfg(target_os = "windows")]
    {
        windows::unregister().map_err(Into::into)
    }
    #[cfg(not(target_os = "windows"))]
    Err("取消文件关联注册仅支持 Windows".into())
}

#[cfg(target_os = "windows")]
pub(crate) fn is_registered() -> CommandResult<bool> {
    windows::read("Software\\RegisteredApplications", "KView")
        .map(|value| value.as_deref() == Some("Software\\KView\\Capabilities"))
        .map_err(Into::into)
}

#[cfg(target_os = "windows")]
mod windows {
    use std::{ffi::c_void, ptr};
    use windows_sys::Win32::{
        Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS},
        System::Registry::{
            RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegDeleteValueW, RegOpenKeyExW,
            RegQueryValueExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, KEY_WRITE,
            REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
        },
        UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST},
    };

    const PROG_ID: &str = "KView 支持的文件";
    const CAPABILITIES: &str = "Software\\KView\\Capabilities";

    struct Key(HKEY);

    impl Drop for Key {
        fn drop(&mut self) {
            unsafe { RegCloseKey(self.0); }
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }

    fn check(status: u32) -> Result<(), String> {
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!("文件关联注册表操作失败，系统错误码：{status}"))
        }
    }

    fn absent(status: u32) -> bool {
        status == ERROR_FILE_NOT_FOUND || status == ERROR_PATH_NOT_FOUND
    }

    fn open(path: &str, create: bool) -> Result<Option<Key>, String> {
        let mut key = ptr::null_mut();
        let path = wide(path);
        let status = unsafe {
            if create {
                RegCreateKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, ptr::null(),
                    REG_OPTION_NON_VOLATILE, KEY_READ | KEY_WRITE, ptr::null(), &mut key, ptr::null_mut())
            } else {
                RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, KEY_READ | KEY_WRITE, &mut key)
            }
        };
        if !create && absent(status) { return Ok(None); }
        check(status)?;
        Ok(Some(Key(key)))
    }

    pub(super) fn read(path: &str, name: &str) -> Result<Option<String>, String> {
        let Some(key) = open(path, false)? else { return Ok(None); };
        let name = wide(name);
        let mut size = 0;
        let mut kind = 0;
        let status = unsafe {
            RegQueryValueExW(key.0, name.as_ptr(), ptr::null(), &mut kind, ptr::null_mut(), &mut size)
        };
        if absent(status) { return Ok(None); }
        check(status)?;
        if kind != REG_SZ { return Err("文件关联注册表值类型异常".into()); }
        let mut value = vec![0u16; (size as usize + 1) / 2];
        check(unsafe {
            RegQueryValueExW(key.0, name.as_ptr(), ptr::null(), &mut kind, value.as_mut_ptr().cast(), &mut size)
        })?;
        value.truncate(size as usize / 2);
        while value.last() == Some(&0) { value.pop(); }
        Ok(Some(String::from_utf16_lossy(&value)))
    }

    fn write(path: &str, name: &str, value: &str) -> Result<(), String> {
        let key = open(path, true)?.ok_or("无法创建文件关联注册表项")?;
        let value = wide(value);
        check(unsafe {
            RegSetValueExW(key.0, wide(name).as_ptr(), 0, REG_SZ,
                value.as_ptr().cast(), (value.len() * 2) as u32)
        })
    }

    fn delete_value(path: &str, name: &str) -> Result<(), String> {
        let Some(key) = open(path, false)? else { return Ok(()); };
        let status = unsafe { RegDeleteValueW(key.0, wide(name).as_ptr()) };
        if absent(status) { Ok(()) } else { check(status) }
    }

    fn delete_tree(path: &str) -> Result<(), String> {
        let status = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, wide(path).as_ptr()) };
        if absent(status) { Ok(()) } else { check(status) }
    }

    fn refresh() {
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST, ptr::null::<c_void>(), ptr::null::<c_void>()); }
    }

    pub(super) fn register() -> Result<(), String> {
        let result = (|| {
            let executable = std::env::current_exe().map_err(|_| "无法获取 KView 程序位置")?;
            let executable = executable.to_str().ok_or("KView 程序路径无法编码")?;
            let class = format!("Software\\Classes\\{PROG_ID}");
            write(&class, "", "使用 KView 快速查看文件")?;
            write(&format!("{class}\\shell\\open\\command"), "", &format!("\"{executable}\" \"%1\""))?;
            write(&format!("{class}\\DefaultIcon"), "", &format!("\"{executable}\",0"))?;
            let key = open(&class, true)?.ok_or("无法创建文件关联注册表项")?;
            let flags = 0x00010000u32;
            check(unsafe {
                RegSetValueExW(key.0, wide("EditFlags").as_ptr(), 0, REG_DWORD,
                    (&flags as *const u32).cast(), 4)
            })?;
            write(CAPABILITIES, "ApplicationName", "KView")?;
            write(CAPABILITIES, "ApplicationDescription", "使用 KView 快速查看文件")?;
            write(CAPABILITIES, "ApplicationIcon", &format!("\"{executable}\",0"))?;
            for extension in crate::formats::association_extensions() {
                write(&format!("{CAPABILITIES}\\FileAssociations"), &format!(".{extension}"), PROG_ID)?;
                write(&format!("Software\\Classes\\.{extension}\\OpenWithProgids"), PROG_ID, "")?;
            }
            write("Software\\RegisteredApplications", "KView", CAPABILITIES)
        })();
        refresh();
        result
    }

    fn clean_extension(extension: &str) -> Result<(), String> {
        let path = format!("Software\\Classes\\.{extension}");
        if read(&path, "")?.as_deref() == Some(PROG_ID) {
            let backup = read(&path, &format!("{PROG_ID}_backup"))?
                .filter(|value| !value.is_empty() && value != PROG_ID);
            let backup = match backup {
                Some(value) => Some(value),
                None => read(&path, &format!("{PROG_ID}_upgrade_backup"))?
                    .filter(|value| !value.is_empty() && value != PROG_ID),
            };
            match backup {
                Some(value) => write(&path, "", &value)?,
                None => delete_value(&path, "")?,
            }
        }
        delete_value(&format!("{path}\\OpenWithProgids"), PROG_ID)?;
        for suffix in ["_backup", "_upgrade_backup", "_upgrade_marker"] {
            delete_value(&path, &format!("{PROG_ID}{suffix}"))?;
        }
        Ok(())
    }

    pub(super) fn unregister() -> Result<(), String> {
        let mut failures = 0;
        for extension in crate::formats::association_extensions() {
            if clean_extension(&extension).is_err() { failures += 1; }
        }
        // 扩展名清理失败时保留文件类，避免残留的打开命令失效，并允许重试。
        if failures == 0 {
            for result in [
                delete_value("Software\\RegisteredApplications", "KView"),
                delete_tree(CAPABILITIES),
                delete_tree(&format!("Software\\Classes\\{PROG_ID}")),
            ] {
                if result.is_err() { failures += 1; }
            }
        }
        refresh();
        if failures == 0 { Ok(()) } else {
            Err(format!("部分文件关联注册未能清理（{failures} 项失败），请重试"))
        }
    }
}
