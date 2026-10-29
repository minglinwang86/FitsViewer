//! Persist only an owned, host-validated FITS beside an explicitly selected source.
//! 仅宿主提交：唯一安全文件名、原子不覆盖，用户保存文件不进入 artifact 回收表。
use std::{fs::{File,OpenOptions},io::{Read,Write},path::{Path,PathBuf},sync::{Mutex,atomic::{AtomicBool,Ordering}}};
use crate::remote_sources::Identity;

const LIMIT:u64=512*1024*1024;
pub enum Saved {File(PathBuf),ArtifactOnly(String)}
/// Cancellation and the no-overwrite commit share one ordering boundary.
/// 取消请求与原子提交互斥；已提交的文件永远不会因迟到取消而删除。
pub struct SaveControl {
    cancelled:AtomicBool,
    committed:Mutex<bool>,
    #[cfg(test)] checkpoint:Option<std::sync::Arc<TestCheckpoint>>,
}
impl SaveControl {
    pub fn new(cancelled:bool)->Self{Self{cancelled:AtomicBool::new(cancelled),committed:Mutex::new(false),#[cfg(test)] checkpoint:None}}
    pub fn request_cancel(&self)->bool{
        let committed=self.committed.lock().unwrap_or_else(|error|error.into_inner());
        let accepted=!*committed;
        if accepted{self.cancelled.store(true,Ordering::Release);}
        #[cfg(test)] if let Some(checkpoint)=&self.checkpoint{let _=checkpoint.cancel_requests.send(accepted);}
        accepted
    }
    pub fn is_cancelled(&self)->bool{self.cancelled.load(Ordering::Acquire)}
    #[cfg(test)] pub(crate) fn with_checkpoint(mut self,checkpoint:Option<std::sync::Arc<TestCheckpoint>>)->Self{self.checkpoint=checkpoint;self}
    #[cfg(test)] fn pause(&self,phase:TestPhase){if let Some(checkpoint)=&self.checkpoint{if checkpoint.phase==phase{
        checkpoint.reached.send(()).expect("save test observer dropped");
        checkpoint.resume.lock().unwrap().recv_timeout(std::time::Duration::from_secs(10)).expect("save test did not resume");
    }}}
}
#[cfg(test)]
#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub(crate) enum TestPhase {Copying,BeforeCommit,Committed}
#[cfg(test)]
pub(crate) struct TestCheckpoint {
    phase:TestPhase,reached:std::sync::mpsc::Sender<()>,resume:Mutex<std::sync::mpsc::Receiver<()>>,cancel_requests:std::sync::mpsc::Sender<bool>,
}
#[cfg(test)]
pub(crate) struct TestGate {pub reached:std::sync::mpsc::Receiver<()>,pub resume:std::sync::mpsc::Sender<()>,pub cancel_requests:std::sync::mpsc::Receiver<bool>}
#[cfg(test)]
impl TestCheckpoint {
    pub(crate) fn new(phase:TestPhase)->(std::sync::Arc<Self>,TestGate){
        let(reached_tx,reached)=std::sync::mpsc::channel();let(resume,resume_rx)=std::sync::mpsc::channel();let(cancel_tx,cancel_requests)=std::sync::mpsc::channel();
        (std::sync::Arc::new(Self{phase,reached:reached_tx,resume:Mutex::new(resume_rx),cancel_requests:cancel_tx}),TestGate{reached,resume,cancel_requests})
    }
}
struct Temporary(PathBuf);
impl Drop for Temporary {fn drop(&mut self){let _=std::fs::remove_file(&self.0);}}
fn create_temporary(path:PathBuf)->Result<(File,Temporary),std::io::Error>{
    let file=OpenOptions::new().write(true).create_new(true).open(&path)?;
    Ok((file,Temporary(path)))
}
fn cancelled(stop:&SaveControl)->Result<(),String>{if stop.is_cancelled(){Err("Download save cancelled".into())}else{Ok(())}}
fn unchanged(path:&Path,identity:&Identity)->Result<(),String>{
    if Identity::of(&File::open(path).map_err(|_|"Source changed before download save")?)?!=*identity{return Err("Source changed before download save".into());}Ok(())
}
pub fn save(artifact:&Path,source:&Path,cache:&Path,stop:&SaveControl)->Result<Saved,String>{
    let name=format!("fitsviewer-download-{}.fits",uuid::Uuid::new_v4().simple());
    save_named(artifact,source,cache,stop,&name)
}
fn save_named(artifact:&Path,source:&Path,cache:&Path,stop:&SaveControl,name:&str)->Result<Saved,String>{
    cancelled(stop)?;
    let cache=cache.canonicalize().map_err(|e|e.to_string())?;
    let artifact=artifact.canonicalize().map_err(|e|e.to_string())?;
    if !artifact.starts_with(&cache){return Err("Download artifact is outside the owned cache".into());}
    let mut input=File::open(&artifact).map_err(|e|e.to_string())?;
    let identity=Identity::of(&input)?;
    let length=input.metadata().map_err(|e|e.to_string())?.len();
    if length==0 || length>LIMIT{return Err("FITS save exceeds the 512 MiB limit or is empty".into());}
    let mut signature=[0;80];input.read_exact(&mut signature).map_err(|_|"Download is not a FITS product")?;
    if !signature.starts_with(b"SIMPLE  =") || length%2880!=0{return Err("Only a validated FITS product can be saved and added".into());}
    // Parse in the shared core as well: UI kind/validated flags are never authority.
    // 必须由宿主重新解析 FITS 头，不相信客户端声明的 kind/validated 标志。
    fitsviewer_core::load_header(&artifact,0).map_err(|e|format!("Download is not a supported FITS product: {e}"))?;
    unchanged(&artifact,&identity)?;
    if !source.is_absolute(){return Ok(Saved::ArtifactOnly("No persistent original source directory; opening the temporary FITS result".into()));}
    let source=source.canonicalize().map_err(|_|"Original source is no longer available")?;
    if source.starts_with(&cache){return Ok(Saved::ArtifactOnly("Current source is a temporary artifact; opening the temporary FITS result".into()));}
    let original=File::open(&source).map_err(|_|"Cannot inspect the original source")?;
    let original_id=Identity::of(&original)?;
    let parent=source.parent().ok_or("Original source has no parent directory")?;
    let temporary_path=parent.join(format!(".fitsviewer-{}.part",uuid::Uuid::new_v4().simple()));
    let (mut output,temporary)=match create_temporary(temporary_path){
        Ok(owned)=>owned,Err(_)=>return Ok(Saved::ArtifactOnly("Original directory is not writable; opening the temporary FITS result".into())),
    };
    // Only a successful create_new gives this operation cleanup ownership.
    // 创建失败绝不能删除碰巧已存在的路径；清理守卫只在成功创建后建立。
    output.write_all(&signature).map_err(|e|e.to_string())?;
    let mut bytes=80u64;let mut buffer=[0;1024*1024];
    loop {cancelled(stop)?;let count=input.read(&mut buffer).map_err(|e|e.to_string())?;if count==0{break;}
        bytes+=count as u64;if bytes>LIMIT{return Err("FITS save exceeded the 512 MiB limit".into());}
        output.write_all(&buffer[..count]).map_err(|e|e.to_string())?;
        #[cfg(test)] if bytes==80+count as u64{stop.pause(TestPhase::Copying);}
    }
    output.sync_all().map_err(|e|e.to_string())?;drop(output);
    unchanged(&artifact,&identity)?;unchanged(&source,&original_id)?;
    if bytes!=length{return Err("Download artifact changed during save".into());}
    let final_path=parent.join(name);
    #[cfg(test)] stop.pause(TestPhase::BeforeCommit);
    // A cancellation cannot slip between the final check and the hard link.
    // 最后一次取消检查和提交持同一把锁，提交后的请求明确返回未接受。
    let mut committed=stop.committed.lock().map_err(|_|"Save commit state lock failed")?;
    cancelled(stop)?;
    // Same-directory hard-link is atomic and fails if the destination already exists.
    // 不能用可能覆盖用户文件的 rename；不支持此提交方式的文件系统明确降级。
    if let Err(error)=std::fs::hard_link(&temporary.0,&final_path){
        return Ok(Saved::ArtifactOnly(if error.kind()==std::io::ErrorKind::AlreadyExists{"Unique save name already exists; opening the temporary FITS result"}else{"Atomic save is unavailable in the original directory; opening the temporary FITS result"}.into()));
    }
    *committed=true;drop(committed);
    #[cfg(test)] stop.pause(TestPhase::Committed);
    // The successful no-overwrite link is the commit point. A later cancel does
    // not unlink a persistent path which the user may already have replaced.
    // 提交后视为完成；迟到取消不能按路径回滚，避免误删随后由用户替换的文件。
    Ok(Saved::File(final_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(crate) fn fits()->Vec<u8>{let mut bytes=vec![b' ';2880];for(index,text)in["SIMPLE  =                    T","BITPIX  =                    8","NAXIS   =                    0","END"].iter().enumerate(){bytes[index*80..index*80+text.len()].copy_from_slice(text.as_bytes());}bytes}
    fn fixture()->(PathBuf,PathBuf,PathBuf){let root=std::env::temp_dir().join(format!("fv-save-{}",uuid::Uuid::new_v4()));std::fs::create_dir_all(root.join("cache")).unwrap();let source=root.join("original.fits");let artifact=root.join("cache/product");std::fs::write(&source,fits()).unwrap();std::fs::write(&artifact,fits()).unwrap();(root,source,artifact)}
    #[test]fn unique_atomic_save_preserves_inputs_and_survives_cache_cleanup(){let(root,source,artifact)=fixture();let expected=std::fs::read(&source).unwrap();let stop=SaveControl::new(false);let Saved::File(saved)=save(&artifact,&source,&root.join("cache"),&stop).unwrap()else{panic!()};assert!(!stop.request_cancel());assert_eq!(std::fs::read(&saved).unwrap(),fits());assert_eq!(std::fs::read(&source).unwrap(),expected);std::fs::remove_dir_all(root.join("cache")).unwrap();assert!(saved.is_file());assert_eq!(std::fs::read_dir(&root).unwrap().count(),2);std::fs::remove_dir_all(root).unwrap();}
    #[test]fn collisions_never_overwrite_and_leave_no_part(){let(root,source,artifact)=fixture();let target=root.join("existing.fits");std::fs::write(&target,b"keep user data").unwrap();assert!(matches!(save_named(&artifact,&source,&root.join("cache"),&SaveControl::new(false),"existing.fits").unwrap(),Saved::ArtifactOnly(_)));assert_eq!(std::fs::read(target).unwrap(),b"keep user data");assert!(!std::fs::read_dir(&root).unwrap().flatten().any(|e|e.file_name().to_string_lossy().ends_with(".part")));std::fs::remove_dir_all(root).unwrap();}
    #[test]fn arbitrary_owned_artifacts_are_not_fits(){let(root,source,artifact)=fixture();std::fs::write(&artifact,b"<html>not FITS</html>").unwrap();assert!(save(&artifact,&source,&root.join("cache"),&SaveControl::new(false)).is_err());std::fs::remove_dir_all(root).unwrap();}
    #[test]fn failed_temporary_creation_never_removes_an_existing_file(){
        let(root,_,_)=fixture();let existing=root.join("occupied.part");std::fs::write(&existing,b"existing user file").unwrap();
        assert!(create_temporary(existing.clone()).is_err());assert_eq!(std::fs::read(existing).unwrap(),b"existing user file");std::fs::remove_dir_all(root).unwrap();
    }
    #[test]fn oversized_artifact_fails_before_copy(){let(root,source,artifact)=fixture();OpenOptions::new().write(true).open(&artifact).unwrap().set_len(LIMIT+1).unwrap();assert!(save(&artifact,&source,&root.join("cache"),&SaveControl::new(false)).is_err());assert_eq!(std::fs::read_dir(&root).unwrap().count(),2);std::fs::remove_dir_all(root).unwrap();}
    #[test]fn cancellation_leaves_no_output(){let(root,source,artifact)=fixture();assert!(save(&artifact,&source,&root.join("cache"),&SaveControl::new(true)).is_err());assert_eq!(std::fs::read_dir(&root).unwrap().count(),2);std::fs::remove_dir_all(root).unwrap();}
    #[test]fn artifact_source_explicitly_falls_back(){let(root,_,artifact)=fixture();assert!(matches!(save(&artifact,&artifact,&root.join("cache"),&SaveControl::new(false)).unwrap(),Saved::ArtifactOnly(_)));std::fs::remove_dir_all(root).unwrap();}
    #[cfg(target_os="linux")]
    #[test]fn unwritable_directory_explicitly_falls_back(){let(root,_,artifact)=fixture();assert!(matches!(save(&artifact,Path::new("/proc/version"),&root.join("cache"),&SaveControl::new(false)).unwrap(),Saved::ArtifactOnly(_)));std::fs::remove_dir_all(root).unwrap();}
}
