//! Embed Windows shell icons and version metadata. / 嵌入 Windows 图标和版本资源。
use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    // A browser/CLI-only host has no native desktop resources or RC dependency.
    // 纯浏览器/命令行宿主不包含桌面资源，不应要求安装 Windows 资源编译器。
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") || env::var_os("CARGO_FEATURE_DESKTOP").is_none() {
        return;
    }
    let root = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let icon = root
        .join("windows/app.ico")
        .canonicalize()
        .expect("Windows icon is missing");
    println!("cargo:rerun-if-changed={}", icon.display());
    let out = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let rc = out.join("fitsviewer.rc");
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let major = env::var("CARGO_PKG_VERSION_MAJOR").unwrap();
    let minor = env::var("CARGO_PKG_VERSION_MINOR").unwrap();
    let patch = env::var("CARGO_PKG_VERSION_PATCH").unwrap();
    let prerelease = env::var("CARGO_PKG_VERSION_PRE").unwrap_or_default();
    let revision = prerelease
        .rsplit('.')
        .next()
        .unwrap_or("0")
        .parse::<u16>()
        .unwrap_or(0);
    let resource = format!(
        r#"1 ICON "{}"
1 VERSIONINFO
FILEVERSION {major},{minor},{patch},{revision}
PRODUCTVERSION {major},{minor},{patch},{revision}
FILEOS 0x40004
FILETYPE 1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "FitsViewer contributors\0"
      VALUE "FileDescription", "FitsViewer astronomical FITS viewer\0"
      VALUE "FileVersion", "{version}\0"
      VALUE "InternalName", "fitsviewer\0"
      VALUE "OriginalFilename", "fitsviewer.exe\0"
      VALUE "ProductName", "FitsViewer\0"
      VALUE "ProductVersion", "{version}\0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x0409, 1200
  END
END
"#,
        icon.to_string_lossy().replace('\\', "/")
    );
    fs::write(&rc, resource).unwrap();
    let target = env::var("TARGET").unwrap();
    if target.ends_with("gnu") {
        let object = out.join("fitsviewer-icon.o");
        let compiler = env::var("RC_x86_64_pc_windows_gnu")
            .or_else(|_| env::var("RC"))
            .unwrap_or_else(|_| "x86_64-w64-mingw32-windres".into());
        assert!(Command::new(compiler)
            .args(["-i"])
            .arg(&rc)
            .args(["-O", "coff", "-o"])
            .arg(&object)
            .status()
            .expect("MinGW windres is required")
            .success());
        println!("cargo:rustc-link-arg-bin=fitsviewer={}", object.display());
    } else {
        let resource = out.join("fitsviewer.res");
        assert!(Command::new("rc.exe")
            .args(["/nologo", "/fo"])
            .arg(&resource)
            .arg(&rc)
            .status()
            .expect("Windows SDK resource compiler is required")
            .success());
        println!("cargo:rustc-link-arg-bin=fitsviewer={}", resource.display());
    }
}
