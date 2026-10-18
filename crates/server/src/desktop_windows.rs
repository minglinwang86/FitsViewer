//! Native Windows dialogs and per-user file associations. / Windows 对话框与用户文件关联。
use serde_json::{json, Value};
use std::{ffi::OsString, io, os::windows::ffi::OsStringExt, path::PathBuf, ptr};
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND, ERROR_SUCCESS},
    System::Registry::*,
    UI::{Controls::Dialogs::*, Shell::*, WindowsAndMessaging::*},
};

const APP: &str = "Software\\FitsViewer";
const PROGID: &str = "FitsViewer.FITS";
const EXTENSIONS: [&str; 4] = [".fits", ".fit", ".fts", ".fz"];
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

pub fn message(title: &str, text: &str) {
    unsafe {
        MessageBoxW(
            ptr::null_mut(),
            wide(text).as_ptr(),
            wide(title).as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
}

pub fn set_app_id() {
    unsafe {
        SetCurrentProcessExplicitAppUserModelID(wide("FitsViewer.Desktop").as_ptr());
    }
}

pub fn pick_file() -> Option<PathBuf> {
    let mut buffer = vec![0u16; 32768];
    let filter =
        wide("FITS images and spectra\0*.fits;*.fit;*.fts;*.fz;*.fits.gz;*.fit.gz;*.fts.gz;*.dat;*.txt;*.csv;*.ascii\0All files\0*.*\0");
    let title = wide("Open FITS image or spectrum");
    let mut dialog: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    dialog.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
    dialog.lpstrFilter = filter.as_ptr();
    dialog.lpstrFile = buffer.as_mut_ptr();
    dialog.nMaxFile = buffer.len() as u32;
    dialog.lpstrTitle = title.as_ptr();
    dialog.Flags = OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_EXPLORER | OFN_NOCHANGEDIR;
    if unsafe { GetOpenFileNameW(&mut dialog) } == 0 {
        return None;
    }
    let count = buffer.iter().position(|&c| c == 0)?;
    Some(PathBuf::from(OsString::from_wide(&buffer[..count])))
}

fn read(key: &str, name: &str) -> io::Result<Option<String>> {
    let (key, name) = (wide(key), wide(name));
    let mut size = 0;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            ptr::null_mut(),
            ptr::null_mut(),
            &mut size,
        )
    };
    if [ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND].contains(&result) {
        return Ok(None);
    }
    if result != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let mut buffer = vec![0u16; size as usize / 2 + 1];
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if result != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let count = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Ok(Some(String::from_utf16_lossy(&buffer[..count])))
}
fn write(key: &str, name: &str, value: &str) -> io::Result<()> {
    let mut handle = ptr::null_mut();
    let key = wide(key);
    let result = unsafe {
        RegCreateKeyExW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            0,
            ptr::null(),
            REG_OPTION_NON_VOLATILE,
            KEY_WRITE,
            ptr::null(),
            &mut handle,
            ptr::null_mut(),
        )
    };
    if result != ERROR_SUCCESS {
        return Err(io::Error::from_raw_os_error(result as i32));
    }
    let (name, value) = (wide(name), wide(value));
    let result = unsafe {
        RegSetValueExW(
            handle,
            name.as_ptr(),
            0,
            REG_SZ,
            value.as_ptr().cast(),
            (value.len() * 2) as u32,
        )
    };
    unsafe {
        RegCloseKey(handle);
    }
    if result == ERROR_SUCCESS {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(result as i32))
    }
}
fn delete_value(key: &str, name: &str) -> io::Result<()> {
    let result =
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, wide(key).as_ptr(), wide(name).as_ptr()) };
    if [ERROR_SUCCESS, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND].contains(&result) {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(result as i32))
    }
}
fn delete_tree(key: &str) -> io::Result<()> {
    let result = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, wide(key).as_ptr()) };
    if [ERROR_SUCCESS, ERROR_FILE_NOT_FOUND, ERROR_PATH_NOT_FOUND].contains(&result) {
        Ok(())
    } else {
        Err(io::Error::from_raw_os_error(result as i32))
    }
}
fn classes(tail: &str) -> String {
    format!("Software\\Classes\\{tail}")
}
fn user_choice(extension: &str) -> io::Result<Option<String>> {
    read(&format!("Software\\Microsoft\\Windows\\CurrentVersion\\Explorer\\FileExts\\{extension}\\UserChoice"), "ProgId")
}
fn effective(extension: &str) -> io::Result<Option<String>> {
    Ok(user_choice(extension)?.or(read(&classes(extension), "")?))
}
fn register() -> io::Result<()> {
    let executable = std::env::current_exe()?;
    let executable = executable.to_string_lossy();
    write(&classes(PROGID), "", "FITS image")?;
    // Keep the program icon without branding associated data files. Remove only
    // our own older registration; other applications' file icons stay intact.
    // 程序保留图标，数据文件不指定品牌图标；仅移除本程序旧版本的设置。
    delete_tree(&classes(&format!("{PROGID}\\DefaultIcon")))?;
    write(
        &classes(&format!("{PROGID}\\shell\\open\\command")),
        "",
        &format!("\"{executable}\" --native \"%1\""),
    )?;
    write(
        &format!("{APP}\\Capabilities"),
        "ApplicationName",
        "FitsViewer",
    )?;
    write(
        &format!("{APP}\\Capabilities"),
        "ApplicationDescription",
        "View astronomical FITS images",
    )?;
    write(
        "Software\\RegisteredApplications",
        "FitsViewer",
        &format!("{APP}\\Capabilities"),
    )?;
    for extension in EXTENSIONS {
        write(
            &classes(&format!("{extension}\\OpenWithProgids")),
            PROGID,
            "",
        )?;
        write(
            &format!("{APP}\\Capabilities\\FileAssociations"),
            extension,
            PROGID,
        )?;
    }
    Ok(())
}
fn restore() -> io::Result<()> {
    for extension in EXTENSIONS {
        if read(&classes(extension), "")?.as_deref() == Some(PROGID) {
            let previous =
                read(&format!("{APP}\\PreviousDefaults"), extension)?.unwrap_or_default();
            if previous.is_empty() {
                delete_value(&classes(extension), "")?;
            } else {
                write(&classes(extension), "", &previous)?;
            }
        }
        delete_value(&format!("{APP}\\PreviousDefaults"), extension)?;
    }
    write(APP, "DefaultRequested", "no")
}
pub fn settings() -> io::Result<()> {
    let result = unsafe {
        ShellExecuteW(
            ptr::null_mut(),
            wide("open").as_ptr(),
            wide("ms-settings:defaultapps?registeredAppUser=FitsViewer").as_ptr(),
            ptr::null(),
            ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if (result as isize) > 32 {
        Ok(())
    } else {
        Err(io::Error::other("Windows Default Apps could not be opened"))
    }
}
pub fn defaults(action: &str) -> io::Result<Value> {
    match action {
        "yes" => {
            register()?;
            for extension in EXTENSIONS {
                let current = read(&classes(extension), "")?;
                if current.as_deref() != Some(PROGID) {
                    if read(&format!("{APP}\\PreviousDefaults"), extension)?.is_none() {
                        write(
                            &format!("{APP}\\PreviousDefaults"),
                            extension,
                            current.as_deref().unwrap_or(""),
                        )?;
                    }
                    write(&classes(extension), "", PROGID)?;
                }
            }
            write(APP, "DefaultRequested", "yes")?;
        }
        "no" => restore()?,
        "register" => register()?,
        "remove" => {
            restore()?;
            for extension in EXTENSIONS {
                delete_value(&classes(&format!("{extension}\\OpenWithProgids")), PROGID)?;
            }
            delete_tree(&classes(PROGID))?;
            delete_value("Software\\RegisteredApplications", "FitsViewer")?;
            delete_tree(APP)?;
        }
        "settings" => settings()?,
        "status" => {}
        _ => {
            return Err(io::Error::other(
                "Choose yes, no, register, remove, settings or status",
            ))
        }
    }
    if action != "status" {
        unsafe {
            SHChangeNotify(
                SHCNE_ASSOCCHANGED as i32,
                SHCNF_IDLIST,
                ptr::null(),
                ptr::null(),
            );
        }
    }
    let mut associations = serde_json::Map::new();
    let mut all_default = true;
    let mut any_default = false;
    for extension in EXTENSIONS {
        let owner = effective(extension)?;
        all_default &= owner.as_deref() == Some(PROGID);
        any_default |= owner.as_deref() == Some(PROGID);
        associations.insert(extension.into(), json!(owner));
    }
    let requested = read(APP, "DefaultRequested")?.as_deref() == Some("yes");
    let registered = read(&classes(&format!("{PROGID}\\shell\\open\\command")), "")?.is_some();
    Ok(json!({"registered":registered,
        "default":registered && all_default,"defaults":associations,"default_requested":requested,
        "needs_windows_confirmation":registered && ((requested && !all_default)||(!requested && any_default)),"progid":PROGID}))
}
