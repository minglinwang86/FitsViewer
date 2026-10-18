//! Stable private read names for explicit-file grants, across native and Python readers.
//! 仅从授权时持有的文件建立稳定别名；换路径后绝不重新读取新文件的内容。
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::SystemTime,
};
const BUDGET: u64 = 1024 * 1024 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Identity {
    length: u64,
    modified: Option<SystemTime>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    index: (u32, u32, u32),
}
impl Identity {
    pub(crate) fn of(file: &File) -> Result<Self, String> {
        let meta = file
            .metadata()
            .map_err(|_| "Cannot inspect authorized source")?;
        if !meta.is_file() {
            return Err("Select a regular file".into());
        }
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            length: meta.len(),
            modified: meta.modified().ok(),
            #[cfg(unix)]
            device: meta.dev(),
            #[cfg(unix)]
            inode: meta.ino(),
            #[cfg(windows)]
            index: super::remote_broker::file_identity(file)?,
        })
    }
}
struct Root {
    path: PathBuf,
    created: AtomicBool,
    used: AtomicU64,
    pending: Mutex<Vec<(PathBuf, u64)>>,
}
impl Root {
    fn retry(&self) {
        self.pending
            .lock()
            .unwrap()
            .retain(|(path, bytes)| match std::fs::remove_file(path) {
                Ok(()) => {
                    self.used.fetch_sub(*bytes, Ordering::AcqRel);
                    false
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    self.used.fetch_sub(*bytes, Ordering::AcqRel);
                    false
                }
                Err(_) => true,
            });
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        if self.created.load(Ordering::Acquire) {
            self.retry();
            if std::fs::read_dir(&self.path)
                .is_ok_and(|files| files.flatten().any(|file| file.file_name() != ".owner"))
            {
                return;
            }
            let _ = std::fs::remove_file(self.path.join(".owner"));
            let _ = std::fs::remove_dir(&self.path);
        }
    }
}
pub struct Source {
    logical: PathBuf,
    path: PathBuf,
    held: File,
    original: Identity,
    alias: Identity,
    charged: u64,
    root: Arc<Root>,
}
impl Source {
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn logical(&self) -> &Path {
        &self.logical
    }
    pub fn verify(&self) -> Result<(), String> {
        let current = File::open(&self.logical)
            .map_err(|_| "source_changed: authorized path is unavailable")?;
        let alias =
            File::open(&self.path).map_err(|_| "source_changed: private source lease ended")?;
        if Identity::of(&current)? != self.original
            || Identity::of(&self.held)? != self.original
            || Identity::of(&alias)? != self.alias
        {
            return Err("source_changed: file identity or contents changed; run fver again".into());
        }
        Ok(())
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {
                self.root.used.fetch_sub(self.charged, Ordering::AcqRel);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                self.root.used.fetch_sub(self.charged, Ordering::AcqRel);
            }
            Err(_) => self
                .root
                .pending
                .lock()
                .unwrap()
                .push((self.path.clone(), self.charged)),
        }
    }
}
pub struct Store {
    root: Arc<Root>,
    io: Mutex<()>,
}
impl Store {
    pub fn new(parent: PathBuf) -> Self {
        Self {
            root: Arc::new(Root {
                path: parent.join(format!("session-{}", uuid::Uuid::new_v4().simple())),
                created: AtomicBool::new(false),
                used: AtomicU64::new(0),
                pending: Mutex::new(Vec::new()),
            }),
            io: Mutex::new(()),
        }
    }
    fn ensure(&self) -> Result<(), String> {
        if self.root.created.load(Ordering::Acquire) {
            return Ok(());
        }
        let parent = self
            .root
            .path
            .parent()
            .ok_or("Missing private source directory")?;
        std::fs::create_dir_all(parent).map_err(|_| "Cannot create private source directory")?;
        if parent
            .symlink_metadata()
            .map_err(|e| e.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err("Private source directory must not be a symlink".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        reap(parent);
        std::fs::create_dir(&self.root.path)
            .map_err(|_| "Cannot create an exclusive private source session")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&self.root.path, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut marker = options
            .open(self.root.path.join(".owner"))
            .map_err(|e| e.to_string())?;
        write!(marker, "{}", std::process::id()).map_err(|e| e.to_string())?;
        self.root.created.store(true, Ordering::Release);
        let root = Arc::downgrade(&self.root);
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(250));
            let Some(root) = root.upgrade() else {
                break;
            };
            root.retry();
        });
        Ok(())
    }
    pub fn capture(&self, logical: &Path, held: &File) -> Result<Arc<Source>, String> {
        self.capture_mode(logical, held, false)
    }
    fn capture_mode(
        &self,
        logical: &Path,
        held: &File,
        force_copy: bool,
    ) -> Result<Arc<Source>, String> {
        let _lock = self
            .io
            .lock()
            .map_err(|_| "Private source cache lock failed")?;
        self.ensure()?;
        self.root.retry();
        let identity = Identity::of(held)?;
        let name = logical.to_string_lossy().to_lowercase();
        let gzip = name.ends_with(".gz");
        let text = ["txt", "dat", "csv", "ascii"]
            .into_iter()
            .find(|s| name.ends_with(&format!(".{s}")) || name.ends_with(&format!(".{s}.gz")));
        let suffix = format!(
            ".{}{}",
            text.unwrap_or("fits"),
            if gzip { ".gz" } else { "" }
        );
        let mut path =
            self.root
                .path
                .join(format!("{}.link{}", uuid::Uuid::new_v4().simple(), suffix));
        let result = (|| {
            let charged;
            if !force_copy && std::fs::hard_link(logical, &path).is_ok() {
                charged = 0;
                // Do not trust the path used by hard_link: compare the link's actual
                // inode/file-index to the originally held file before publishing it.
                // 建链路径可能已更换；发布别名前必须核验它确实指向原授权句柄。
                let alias = File::open(&path).map_err(|e| e.to_string())?;
                if Identity::of(&alias)? != identity {
                    return Err(
                        "source_changed: selected path changed while creating its private alias"
                            .into(),
                    );
                }
            } else {
                if identity.length > fitsviewer_core::MAX_FILE_BYTES {
                    return Err("A cross-filesystem source snapshot exceeds 512 MiB; place FITSVIEWER_RUNTIME_DIR on the source filesystem for verified hard-link access".into());
                }
                if retained(self.root.path.parent().unwrap()).saturating_add(identity.length)
                    > BUDGET
                {
                    return Err("Private source snapshot budget is 1 GiB; close unused fver requests before opening more files".into());
                }
                path = self.root.path.join(format!(
                    "{}.copy{}",
                    uuid::Uuid::new_v4().simple(),
                    suffix
                ));
                charged = identity.length;
                let mut input = held.try_clone().map_err(|e| e.to_string())?;
                input.seek(SeekFrom::Start(0)).map_err(|e| e.to_string())?;
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut output = options.open(&path).map_err(|e| e.to_string())?;
                let n = std::io::copy(&mut input.take(identity.length + 1), &mut output)
                    .map_err(|e| e.to_string())?;
                if n != identity.length || Identity::of(held)? != identity {
                    return Err(
                        "source_changed: file changed during its bounded source snapshot".into(),
                    );
                }
                output.flush().map_err(|e| e.to_string())?;
                if let Some(time) = identity.modified {
                    output.set_modified(time).map_err(|e| e.to_string())?;
                }
            }
            let alias = Identity::of(&File::open(&path).map_err(|e| e.to_string())?)?;
            let held = held.try_clone().map_err(|e| e.to_string())?;
            self.root.used.fetch_add(charged, Ordering::AcqRel);
            let source = Source {
                logical: logical.into(),
                path: path.clone(),
                held,
                original: identity.clone(),
                alias,
                charged,
                root: self.root.clone(),
            };
            source.verify()?;
            Ok(Arc::new(source))
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&path);
        }
        result
    }
    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({"bytes":self.root.used.load(Ordering::Acquire),"budget":BUDGET,"cross_filesystem_copy_limit":fitsviewer_core::MAX_FILE_BYTES})
    }
}
fn known(name: &str) -> bool {
    name.get(..32)
        .is_some_and(|prefix| prefix.bytes().all(|b| b.is_ascii_hexdigit()))
        && matches!(
            name.get(32..).map(|suffix| suffix
                .strip_prefix(".copy")
                .or_else(|| suffix.strip_prefix(".link"))
                .unwrap_or(suffix)),
            Some(
                ".fits"
                    | ".fits.gz"
                    | ".txt"
                    | ".txt.gz"
                    | ".dat"
                    | ".dat.gz"
                    | ".csv"
                    | ".csv.gz"
                    | ".ascii"
                    | ".ascii.gz"
            )
        )
}
fn retained(parent: &Path) -> u64 {
    std::fs::read_dir(parent)
        .into_iter()
        .flatten()
        .flatten()
        .take(1024)
        .filter(|d| d.file_type().is_ok_and(|t| t.is_dir()))
        .flat_map(|d| {
            std::fs::read_dir(d.path())
                .into_iter()
                .flatten()
                .flatten()
                .take(1024)
        })
        .filter(|f| {
            let name = f.file_name();
            let name = name.to_string_lossy();
            known(&name) && !name.get(32..).is_some_and(|s| s.starts_with(".link."))
        })
        .filter_map(|f| f.path().symlink_metadata().ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .fold(0, u64::saturating_add)
}
fn gone(pid: u32) -> bool {
    if pid == 0 || pid > i32::MAX as u32 || pid == std::process::id() {
        return false;
    }
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn kill(pid: i32, signal: i32) -> i32;
        }
        return unsafe { kill(pid as i32, 0) } != 0
            && std::io::Error::last_os_error().raw_os_error() == Some(3);
    }
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::{
            OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
        };
        let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if !process.is_null() {
            unsafe {
                windows_sys::Win32::Foundation::CloseHandle(process);
            }
            return false;
        }
        return std::io::Error::last_os_error()
            .raw_os_error()
            .is_some_and(|e| e == 87 || e == 1168);
    }
    #[cfg(not(any(unix, windows)))]
    false
}
fn reap(parent: &Path) {
    for directory in std::fs::read_dir(parent)
        .into_iter()
        .flatten()
        .flatten()
        .take(1024)
    {
        let name = directory.file_name();
        let name = name.to_string_lossy();
        if !name
            .strip_prefix("session-")
            .is_some_and(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            || !directory.file_type().is_ok_and(|t| t.is_dir())
        {
            continue;
        }
        let marker = directory.path().join(".owner");
        if !marker
            .symlink_metadata()
            .is_ok_and(|m| m.is_file() && m.len() <= 16)
        {
            continue;
        }
        if !std::fs::read_to_string(&marker)
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .is_some_and(gone)
        {
            continue;
        }
        for file in std::fs::read_dir(directory.path())
            .into_iter()
            .flatten()
            .flatten()
            .take(1024)
        {
            if known(&file.file_name().to_string_lossy())
                && file.file_type().is_ok_and(|t| t.is_file())
            {
                let _ = std::fs::remove_file(file.path());
            }
        }
        let _ = std::fs::remove_file(marker);
        let _ = std::fs::remove_dir(directory.path());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn private_alias_never_follows_a_replaced_authorized_path() {
        let directory =
            std::env::temp_dir().join(format!("fver-stable-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("original.dat");
        std::fs::write(&path, b"authorized samples").unwrap();
        let file = File::open(&path).unwrap();
        let store = Store::new(directory.join("caps"));
        let source = store.capture(&path, &file).unwrap();
        assert!(source.path().to_string_lossy().ends_with(".dat"));
        std::fs::rename(&path, directory.join("old.dat")).unwrap();
        std::fs::write(&path, b"different file, never authorized").unwrap();
        assert_eq!(std::fs::read(source.path()).unwrap(), b"authorized samples");
        assert!(source.verify().is_err());
        drop(source);
        assert_eq!(store.status()["bytes"], 0);
        drop(store);
        drop(file);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn copied_sources_preserve_text_suffix_and_are_removed_on_drop() {
        let directory =
            std::env::temp_dir().join(format!("fver-copy-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("spectrum.dat.gz");
        std::fs::write(&path, b"synthetic encoded fixture").unwrap();
        let file = File::open(&path).unwrap();
        let store = Store::new(directory.join("caps"));
        let source = store.capture_mode(&path, &file, true).unwrap();
        let alias = source.path().to_path_buf();
        assert!(alias.to_string_lossy().ends_with(".dat.gz"));
        assert_eq!(std::fs::read(&alias).unwrap(), b"synthetic encoded fixture");
        drop(source);
        store.root.retry();
        assert!(!alias.exists());
        assert_eq!(store.status()["bytes"], 0);
        drop(store);
        drop(file);
        std::fs::remove_dir_all(directory).unwrap();
    }
    #[test]
    fn cache_cleanup_names_do_not_slice_invalid_utf8_boundaries() {
        assert!(!known("中文文件名不是受管缓存名xxxxxxxxxxxxxx.fits"));
        assert!(!known(".owner"));
        assert!(!known("00112233445566778899001122334455/../../file"));
    }
    #[cfg(windows)]
    #[test]
    fn windows_shared_read_handles_delay_unlink_until_the_reader_releases() {
        use std::os::windows::fs::OpenOptionsExt;
        let directory =
            std::env::temp_dir().join(format!("fver-unlink-{}", uuid::Uuid::new_v4().simple()));
        std::fs::create_dir(&directory).unwrap();
        let path = directory.join("source.fits");
        std::fs::write(&path, b"fixture").unwrap();
        let file = File::open(&path).unwrap();
        let store = Store::new(directory.join("caps"));
        let source = store.capture_mode(&path, &file, true).unwrap();
        let alias = source.path().to_path_buf();
        let reader = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(3)
            .open(&alias)
            .unwrap();
        drop(source);
        assert!(alias.exists());
        assert!(store.status()["bytes"].as_u64().unwrap() > 0);
        drop(reader);
        store.root.retry();
        assert!(!alias.exists());
        assert_eq!(store.status()["bytes"], 0);
        drop(store);
        drop(file);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
