//! Bounded image preparation and sampled display tiles; never a second science pipeline.
//! 有界文件准备与显示图块；测光等科学操作继续使用原始数据。
use fitsviewer_core::{remote::{self,Manifest,Plane},FitsImage,LoadOptions,Statistics};
use std::{collections::VecDeque,io::{Read,Write},path::{Path,PathBuf},sync::{Arc,Mutex,atomic::{AtomicUsize,Ordering}}};

const CACHE_BYTES:usize=192*1024*1024;
const MAX_ENTRIES:usize=24;
const DISK_BYTES:u64=1024*1024*1024;

struct Prepared {
    key:String, original:PathBuf, source_version:String, path:PathBuf,
    temporary:bool, plane:Option<Plane>, decoded:Option<Arc<FitsImage>>,
    manifest:Manifest, preview:Option<Arc<FitsImage>>, memory:usize, disk:u64,
}
impl Drop for Prepared {fn drop(&mut self){if self.temporary{let _=std::fs::remove_file(&self.path);}}}

struct Permit<'a>(&'a AtomicUsize);
impl Drop for Permit<'_>{fn drop(&mut self){self.0.fetch_sub(1,Ordering::AcqRel);}}
pub struct Store {root:PathBuf,entries:Mutex<VecDeque<Arc<Prepared>>>,io:Mutex<()>,requests:AtomicUsize}
impl Store {
    pub fn new(root:PathBuf)->Self{
        reap_stale(&root);
        let root=root.join(format!("session-{}",uuid::Uuid::new_v4().simple()));
        Self{root,entries:Mutex::new(VecDeque::new()),io:Mutex::new(()),requests:AtomicUsize::new(0)}
    }
    fn enter(&self)->Result<Permit<'_>,String>{self.requests.fetch_update(Ordering::AcqRel,Ordering::Acquire,|n|(n<16).then_some(n+1)).map_err(|_|"Remote image queue is busy; retry shortly")?;Ok(Permit(&self.requests))}
    fn check_version(prepared:&Prepared)->Result<(),String>{
        if remote::source_version(&prepared.original)?!=prepared.source_version{return Err("Source file changed; reload image metadata".into());}Ok(())
    }
    fn trim(&self,additional:usize,disk:u64){
        let mut entries=self.entries.lock().unwrap();
        while !entries.is_empty() && (entries.len()>=MAX_ENTRIES || entries.iter().map(|e|e.memory).sum::<usize>().saturating_add(additional)>CACHE_BYTES || entries.iter().map(|e|e.disk).sum::<u64>().saturating_add(disk)>DISK_BYTES){entries.pop_front();}
    }
    fn unpack(&self,path:&Path)->Result<PathBuf,String>{
        if path.metadata().map_err(|e|e.to_string())?.len()>fitsviewer_core::MAX_FILE_BYTES{return Err("Compressed input exceeds 512 MiB".into());}
        self.trim(0,fitsviewer_core::MAX_FILE_BYTES);
        std::fs::create_dir_all(&self.root).map_err(|e|e.to_string())?;
        #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(&self.root,std::fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;}
        let marker=self.root.join(".owner");
        if !marker.exists(){let mut options=std::fs::OpenOptions::new();options.write(true).create_new(true);#[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}let mut file=options.open(&marker).map_err(|e|e.to_string())?;write!(file,"{}",std::process::id()).map_err(|e|e.to_string())?;}
        // Include retained sessions after a crash. A live or unknown owner is never deleted.
        // 崩溃残留也计入总磁盘上限；不删除活动或身份不明确的会话。
        let retained=self.root.parent().map_or(0,spool_bytes);
        if retained.saturating_add(fitsviewer_core::MAX_FILE_BYTES)>DISK_BYTES{return Err("Prepared-image disk cache is full; close other viewers or clear inactive task caches".into());}
        let destination=remote::prepared_path(&self.root,&uuid::Uuid::new_v4().simple().to_string());
        let result=(||{
            let mut options=std::fs::OpenOptions::new();options.write(true).create_new(true);
            #[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
            let mut output=options.open(&destination).map_err(|e|e.to_string())?;
            let input=std::fs::File::open(path).map_err(|e|e.to_string())?;
            let mut decoder=flate2::read::MultiGzDecoder::new(input).take(fitsviewer_core::MAX_FILE_BYTES+1);
            let n=std::io::copy(&mut decoder,&mut output).map_err(|e|e.to_string())?;
            if n>fitsviewer_core::MAX_FILE_BYTES{return Err("Decoded input exceeds 512 MiB; use uncompressed FITS for bounded ROI access".into());}
            output.flush().map_err(|e|e.to_string())?;Ok(destination.clone())
        })();
        if result.is_err(){let _=std::fs::remove_file(&destination);}result
    }
    fn prepare(&self,path:&Path,logical:&str,options:LoadOptions)->Result<Arc<Prepared>,String>{
        options.validate()?;
        let version=remote::source_version(path)?;
        let key=format!("{}\n{}\n{}\n{}",path.display(),version,logical,serde_json::to_string(&options).map_err(|e|e.to_string())?);
        {
            let mut entries=self.entries.lock().unwrap();
            if let Some(index)=entries.iter().position(|e|e.key==key){let value=entries.remove(index).unwrap();entries.push_back(value.clone());return Ok(value);}
        }
        let mut first=std::fs::File::open(path).map_err(|e|e.to_string())?;let mut magic=[0u8;2];let _=first.read(&mut magic);
        let gzip=magic==[0x1f,0x8b];
        let indexed=remote::inspect(path,options);
        // The prefix inspection establishes the decoded threshold before preparation.
        // gzip 首段元数据先确定载荷；准备文件只占有界磁盘，不缓存整条解压流。
        let actual=if gzip{self.unpack(path)?}else{path.to_path_buf()};
        let result=(||{
            let mut plane=if gzip{remote::inspect(&actual,options).ok()}else{indexed.ok()};
            let mut decoded=None;
            let mut info=if let Some(metadata)=&plane{metadata.info.clone()}else{
                let image=Arc::new(fitsviewer_core::load_file_with_options(&actual,options)?);
                let info=image.info.clone();decoded=Some(image);info
            };
            info.path=logical.into();
            let full=remote::packet_bytes(&info)?;
            if let Some(metadata)=&mut plane{metadata.info=info.clone();}
            if plane.as_ref().is_some_and(|p|p.compression=="tile"){
                let estimate=info.width.checked_mul(info.height).and_then(|n|n.checked_mul((info.bitpix.unsigned_abs()/8) as usize)).ok_or("Decoded image size overflow")?;
                if estimate>CACHE_BYTES-4*1024*1024{return Err("Compressed image exceeds the bounded decoder cache; decompress externally for ROI access".into());}
                self.trim(estimate,0);
                decoded=Some(Arc::new(fitsviewer_core::load_file_with_options(&actual,options)?));
            }
            let memory=decoded.as_ref().map_or(0,|image|image.pixels.len()*(image.info.bitpix.unsigned_abs()/8) as usize+image.spectrum.as_ref().map_or(0,|s|s.x.len()*8+s.error.as_ref().map_or(0,|v|v.len()*8)));
            if memory>CACHE_BYTES{return Err("Dataset exceeds the bounded preparation cache".into());}
            if full>remote::FULL_TRANSFER_BYTES && info.spectrum.is_some(){return Err("This spectrum exceeds the 20 MB full-view limit; select a smaller spectral product".into());}
            let level=remote::preview_level(info.width,info.height);
            let preview=if full>remote::FULL_TRANSFER_BYTES{
                Some(Arc::new(if let Some(image)=&decoded{remote::sample_decoded(image,level,0,0)?}else{remote::read_tile(&actual,plane.as_ref().ok_or("Missing image metadata")?,level,0,0)?}))
            }else{None};
            let statistics=preview.as_ref().map_or_else(Statistics::default,|image|Statistics::new(image));
            let disk=if gzip{actual.metadata().map_err(|e|e.to_string())?.len()}else{0};
            let manifest=Manifest{schema:1,info,source_version:version.clone(),full_packet_bytes:Some(full),threshold_bytes:remote::FULL_TRANSFER_BYTES,
                transport:if full<=remote::FULL_TRANSFER_BYTES{"full"}else{"tiles"}.into(),compression:if gzip{"gzip"}else if plane.as_ref().is_some_and(|p|p.compression=="tile"){"tile"}else{"none"}.into(),
                hdus_complete:plane.as_ref().is_none_or(|p|p.hdus_complete),statistics,statistics_sampled:full>remote::FULL_TRANSFER_BYTES,preview_level:level};
            let metadata=serde_json::to_vec(&manifest.info).map_err(|e|e.to_string())?.len();
            let preview_bytes=preview.as_ref().map_or(0,|image|image.pixels.len()*(image.info.bitpix.unsigned_abs()/8) as usize);
            let memory=memory.saturating_add(metadata.saturating_mul(3)).saturating_add(preview_bytes).saturating_add(65536);
            if memory>CACHE_BYTES{return Err("Dataset metadata and pixels exceed the bounded preparation cache".into());}
            let prepared=Arc::new(Prepared{key,original:path.into(),source_version:version,path:actual.clone(),temporary:gzip,plane,decoded,manifest,preview,memory,disk});
            Self::check_version(&prepared)?;
            self.trim(prepared.memory,disk);self.entries.lock().unwrap().push_back(prepared.clone());Ok(prepared)
        })();
        if result.is_err() && gzip{let _=std::fs::remove_file(&actual);}result
    }
    pub fn manifest(&self,path:&Path,logical:&str,options:LoadOptions)->Result<Manifest,String>{
        let _permit=self.enter()?;
        let _guard=self.io.lock().map_err(|_|"Remote image cache lock failed")?;
        let prepared=self.prepare(path,logical,options)?;Self::check_version(&prepared)?;Ok(prepared.manifest.clone())
    }
    pub fn full(&self,path:&Path,logical:&str,options:LoadOptions,version:&str)->Result<Vec<u8>,String>{
        let _permit=self.enter()?;
        let _guard=self.io.lock().map_err(|_|"Remote image cache lock failed")?;
        let prepared=self.prepare(path,logical,options)?;
        if prepared.source_version!=version{return Err("Source revision expired; reload metadata".into());}
        if prepared.manifest.full_packet_bytes.is_none_or(|n|n>remote::FULL_TRANSFER_BYTES){return Err("Full transfer exceeds 20 MB; request display tiles".into());}
        let mut image=if let Some(image)=&prepared.decoded{(**image).clone()}else{fitsviewer_core::load_file_with_options(&prepared.path,options)?};
        image.info.path=logical.into();let bytes=image.to_packet()?;
        if bytes.len() as u64>remote::FULL_TRANSFER_BYTES{return Err("Full response exceeds the negotiated 20 MB limit".into());}
        Self::check_version(&prepared)?;Ok(bytes)
    }
    pub fn tile(&self,path:&Path,logical:&str,options:LoadOptions,version:&str,level:u8,x:usize,y:usize)->Result<Vec<u8>,String>{
        let _permit=self.enter()?;
        let _guard=self.io.lock().map_err(|_|"Remote image cache lock failed")?;
        let prepared=self.prepare(path,logical,options)?;
        if prepared.source_version!=version{return Err("Source revision expired; reload metadata".into());}
        let tile=if level==prepared.manifest.preview_level && x==0 && y==0{
            prepared.preview.clone().map(Ok).unwrap_or_else(||self.tile_image(&prepared,level,x,y).map(Arc::new))?
        }else{Arc::new(self.tile_image(&prepared,level,x,y)?)};
        let bytes=tile.to_packet()?;Self::check_version(&prepared)?;Ok(bytes)
    }
    fn tile_image(&self,prepared:&Prepared,level:u8,x:usize,y:usize)->Result<FitsImage,String>{
        if prepared.manifest.info.spectrum.is_some(){return Err("Spectrum does not use two-dimensional tiles".into());}
        if let Some(image)=&prepared.decoded{remote::sample_decoded(image,level,x,y)}else{remote::read_tile(&prepared.path,prepared.plane.as_ref().ok_or("Missing prepared image")?,level,x,y)}
    }
    pub fn pixel(&self,path:&Path,logical:&str,options:LoadOptions,version:&str,x:usize,y:usize)->Result<serde_json::Value,String>{
        let _permit=self.enter()?;
        let _guard=self.io.lock().map_err(|_|"Remote image cache lock failed")?;
        let prepared=self.prepare(path,logical,options)?;
        if prepared.source_version!=version{return Err("Source revision expired; reload metadata".into());}
        if x>=prepared.manifest.info.width || y>=prepared.manifest.info.height{return Err("Pixel outside image".into());}
        let mut value=if let Some(image)=&prepared.decoded{let i=y*image.info.width+x;serde_json::json!({"x":x,"y":y,"raw":image.pixels.raw_string(i),"value":image.physical(i),"original_samples":true})}
            else{remote::read_pixel(&prepared.path,prepared.plane.as_ref().ok_or("Missing image metadata")?,x,y)?};
        value["source_version"]=version.into();Self::check_version(&prepared)?;Ok(value)
    }
    pub fn status(&self)->serde_json::Value{let entries=self.entries.lock().unwrap();serde_json::json!({"entries":entries.len(),"decoded_bytes":entries.iter().map(|e|e.memory).sum::<usize>(),"decoded_budget":CACHE_BYTES,"temporary_bytes":entries.iter().map(|e|e.disk).sum::<u64>(),"disk_budget":DISK_BYTES})}
    pub fn clear(&self){self.entries.lock().unwrap().clear();}
}

fn is_spool(name:&str)->bool{name.strip_suffix(".fits").is_some_and(|s|s.len()==32 && s.bytes().all(|c|c.is_ascii_hexdigit()))}
fn spool_bytes(root:&Path)->u64{
    let mut total=0u64;
    if let Ok(directories)=std::fs::read_dir(root){for directory in directories.flatten().take(1024){if !directory.file_type().is_ok_and(|t|t.is_dir()){continue;}if let Ok(files)=std::fs::read_dir(directory.path()){for file in files.flatten().take(MAX_ENTRIES+2){if !is_spool(&file.file_name().to_string_lossy()){continue;}if let Ok(metadata)=file.path().symlink_metadata(){if metadata.is_file(){total=total.saturating_add(metadata.len());}}}}}}
    total
}
fn definitely_exited(pid:u32)->bool{
    if pid==0 || pid>i32::MAX as u32 || pid==std::process::id(){return false;}
    #[cfg(unix)]{unsafe extern "C"{fn kill(pid:i32,signal:i32)->i32;}return unsafe{kill(pid as i32,0)}!=0 && std::io::Error::last_os_error().raw_os_error()==Some(3);}
    #[cfg(windows)]{use windows_sys::Win32::System::Threading::{OpenProcess,PROCESS_QUERY_LIMITED_INFORMATION};let handle=unsafe{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION,0,pid)};if !handle.is_null(){unsafe{windows_sys::Win32::Foundation::CloseHandle(handle);}return false;}return std::io::Error::last_os_error().raw_os_error().is_some_and(|e|e==87 || e==1168);}
    #[cfg(not(any(unix,windows)))]false
}
fn clean_owned(root:&Path){
    if let Ok(files)=std::fs::read_dir(root){for file in files.flatten().take(MAX_ENTRIES+2){let name=file.file_name();let name=name.to_string_lossy();if (name==".owner" || is_spool(&name)) && file.file_type().is_ok_and(|t|t.is_file()){let _=std::fs::remove_file(file.path());}}}
    let _=std::fs::remove_dir(root);
}
fn reap_stale(root:&Path){
    if let Ok(directories)=std::fs::read_dir(root){for entry in directories.flatten().take(1024){
        let name=entry.file_name();let name=name.to_string_lossy();
        if !name.strip_prefix("session-").is_some_and(|s|s.len()==32 && s.bytes().all(|c|c.is_ascii_hexdigit())) || !entry.file_type().is_ok_and(|t|t.is_dir()){continue;}
        let marker=entry.path().join(".owner");
        if !marker.symlink_metadata().is_ok_and(|m|m.is_file() && m.len()<=16){continue;}
        if std::fs::read_to_string(marker).ok().and_then(|s|s.trim().parse::<u32>().ok()).is_some_and(definitely_exited){clean_owned(&entry.path());}
    }}
}
impl Drop for Store {fn drop(&mut self){self.clear();clean_owned(&self.root);}}
