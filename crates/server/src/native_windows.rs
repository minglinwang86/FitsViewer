//! Same-user native window mailboxes. Never enumerate or signal arbitrary processes.
//! 同一用户下的查看窗口消息；只请求自有窗口正常退出，不终止 Jupyter 或按进程名杀程序。
use std::{fs::{self,OpenOptions},io::{Read,Write},path::{Path,PathBuf},sync::{Arc,atomic::{AtomicU64,Ordering}},time::{Duration,SystemTime,UNIX_EPOCH}};
use serde::{Deserialize,Serialize};
use fitsviewer_web::{WindowControl,WindowEvent};

const LIMIT:u64=32*1024;
fn now()->u64{SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64}
fn identifier(value:&str)->bool{value.len()==32 && value.bytes().all(|c|c.is_ascii_hexdigit())}
fn read<T:serde::de::DeserializeOwned>(path:&Path)->Result<T,String>{
    if fs::symlink_metadata(path).map_err(|e|e.to_string())?.file_type().is_symlink(){return Err("Window mailbox cannot be a symbolic link".into());}
    let file=fs::File::open(path).map_err(|e|e.to_string())?;
    if file.metadata().map_err(|e|e.to_string())?.len()>LIMIT{return Err("Window command exceeds its limit".into());}
    serde_json::from_reader(file.take(LIMIT+1)).map_err(|e|e.to_string())
}
fn write_new(path:&Path,value:&impl Serialize)->Result<(),String>{
    let bytes=serde_json::to_vec(value).map_err(|e|e.to_string())?;if bytes.len() as u64>LIMIT{return Err("Window command exceeds its limit".into());}
    let mut options=OpenOptions::new();options.write(true).create_new(true);
    #[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
    // Publish only a complete message. Polling must never consume a partially
    // written request; a hard link also preserves create-new semantics.
    let pending=path.with_file_name(format!("{}.pending",uuid::Uuid::new_v4().simple()));
    let result=(||{
        let mut file=options.open(&pending).map_err(|e|e.to_string())?;
        file.write_all(&bytes).map_err(|e|e.to_string())?;drop(file);
        fs::hard_link(&pending,path).map_err(|e|e.to_string())
    })();
    let _=fs::remove_file(pending);result
}
fn replace_state(path:&Path,value:&impl Serialize)->Result<(),String>{
    let pending=path.with_file_name(format!("{}.pending",uuid::Uuid::new_v4().simple()));
    let result=(||{write_new(&pending,value)?;fs::rename(&pending,path).map_err(|e|e.to_string())})();
    let _=fs::remove_file(pending);result
}
fn message_file(path:&Path)->bool{
    path.file_stem().and_then(|s|s.to_str()).is_some_and(identifier)
        && matches!(path.extension().and_then(|s|s.to_str()),Some("request"|"ack"|"pending"))
}
fn remove_expired_messages(directory:&Path){
    if let Ok(entries)=fs::read_dir(directory){for entry in entries.flatten(){
        let path=entry.path();
        if message_file(&path) && entry.file_type().is_ok_and(|kind|kind.is_file())
            && entry.metadata().and_then(|m|m.modified()).ok().and_then(|t|t.elapsed().ok()).is_some_and(|age|age>Duration::from_secs(10)){
            let _=fs::remove_file(path);
        }
    }}
}
#[derive(Serialize,Deserialize)]struct State{schema:u8,heartbeat:u64,focused:u64,#[serde(default)]local_files:bool}
#[derive(Serialize,Deserialize)]struct Command{schema:u8,expires:u64,event:WindowEvent}
pub struct Instance{root:PathBuf,path:PathBuf,updated:AtomicU64,focused:AtomicU64,local_files:bool}
impl Instance{
    pub fn new(root:PathBuf)->Result<Arc<Self>,String>{
        Self::register(root,true)
    }
    pub fn remote(root:PathBuf)->Result<Arc<Self>,String>{Self::register(root,false)}
    fn register(root:PathBuf,local_files:bool)->Result<Arc<Self>,String>{
        fs::create_dir_all(&root).map_err(|e|e.to_string())?;
        if fs::symlink_metadata(&root).map_err(|e|e.to_string())?.file_type().is_symlink(){return Err("Window registry cannot be a symbolic link".into());}
        #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;fs::set_permissions(&root,fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;}
        let path=root.join(uuid::Uuid::new_v4().simple().to_string());fs::create_dir(&path).map_err(|e|e.to_string())?;
        #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;fs::set_permissions(&path,fs::Permissions::from_mode(0o700)).map_err(|e|e.to_string())?;}
        let instance=Arc::new(Self{root,path,updated:AtomicU64::new(0),focused:AtomicU64::new(now()),local_files});instance.touch(true)?;Ok(instance)
    }
    fn touch(&self,focused:bool)->Result<(),String>{
        let time=now();if focused{self.focused.store(time,Ordering::Release);}
        if time.saturating_sub(self.updated.load(Ordering::Acquire))<1000{return Ok(());}
        replace_state(&self.path.join("state.json"),&State{schema:1,heartbeat:time,focused:self.focused.load(Ordering::Acquire),local_files:self.local_files})?;
        self.updated.store(time,Ordering::Release);Ok(())
    }
    fn targets(root:&Path)->Vec<(PathBuf,u64,bool)>{
        let mut targets=vec![];
        // Stale directories left by a crash must not hide live windows that
        // happen to appear later in the directory enumeration.
        if let Ok(entries)=fs::read_dir(root){for entry in entries.flatten(){
            if !identifier(&entry.file_name().to_string_lossy()) || !entry.file_type().is_ok_and(|kind|kind.is_dir()){continue;}
            if let Ok(state)=read::<State>(&entry.path().join("state.json")){if state.schema==1 && now().saturating_sub(state.heartbeat)<10000{targets.push((entry.path(),state.focused,state.local_files));}}
        }}targets.sort_by_key(|(_,focus,_)|std::cmp::Reverse(*focus));targets
    }
    fn send(target:&Path,event:WindowEvent)->Result<PathBuf,String>{
        remove_expired_messages(target);
        if fs::read_dir(target).map_err(|e|e.to_string())?.take(32).count()>=32{return Err("Window command queue is full".into());}
        let path=target.join(format!("{}.request",uuid::Uuid::new_v4().simple()));
        write_new(&path,&Command{schema:1,expires:now()+5000,event})?;Ok(path)
    }
    pub fn forward(root:&Path,path:String,options:fitsviewer_core::LoadOptions)->bool{
        if options.validate().is_err(){return false;}
        let path=match Path::new(&path).canonicalize(){Ok(path)=>path.to_string_lossy().into_owned(),Err(_)=>return false};
        for (target,_,local_files) in Self::targets(root){
            if !local_files{continue;}
            let Ok(command)=Self::send(&target,WindowEvent::Open{path:path.clone(),options})else{continue;};let ack=command.with_extension("ack");
            let deadline=std::time::Instant::now()+Duration::from_millis(2500);
            while std::time::Instant::now()<deadline{if read::<bool>(&ack).ok()==Some(true){let _=fs::remove_file(ack);return true;}std::thread::sleep(Duration::from_millis(30));}
            let _=fs::remove_file(command);let _=fs::remove_file(ack);
        }false
    }
    pub fn poll(&self,focused:bool)->Vec<WindowEvent>{
        let _=self.touch(focused);remove_expired_messages(&self.path);let mut events=vec![];
        if let Ok(entries)=fs::read_dir(&self.path){for entry in entries.take(32).flatten(){
            let path=entry.path();if path.extension().and_then(|s|s.to_str())!=Some("request") || !path.file_stem().and_then(|s|s.to_str()).is_some_and(identifier){continue;}
            let command=read::<Command>(&path);let _=fs::remove_file(&path);
            let Ok(command)=command else{continue;};if command.schema!=1 || command.expires<now() || command.expires>now()+6000{continue;}
            if let WindowEvent::Open{path,options}=&command.event{if !self.local_files || path.len()>16384 || options.validate().is_err(){continue;}}
            // Exit broadcasts have no waiting sender and must not leave ACKs
            // that eventually fill another window's command queue.
            if matches!(&command.event,WindowEvent::Open{..}){let _=write_new(&path.with_extension("ack"),&true);}
            events.push(command.event);
        }}events
    }
    fn exit_all(&self)->Result<(),String>{
        let mut error=None;
        for (target,_,_) in Self::targets(&self.root){if target!=self.path{
            if let Err(failure)=Self::send(&target,WindowEvent::Exit){if error.is_none(){error=Some(failure);}}
        }}
        error.map_or(Ok(()),Err)
    }
    pub fn control(self:&Arc<Self>)->WindowControl{
        let poll=self.clone();let exit=self.clone();
        WindowControl{poll:Arc::new(move|focused|poll.poll(focused)),exit_all:Arc::new(move||exit.exit_all())}
    }
}
impl Drop for Instance{fn drop(&mut self){
        if let Ok(entries)=fs::read_dir(&self.path){for entry in entries.flatten(){if entry.file_name()=="state.json" || message_file(&entry.path()){let _=fs::remove_file(entry.path());}}}
    let _=fs::remove_dir(&self.path);
}}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn stale_registry_entries_do_not_hide_live_windows(){
        let root=std::env::temp_dir().join(format!("fv-registry-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();
        for n in 0..160{fs::create_dir(root.join(format!("{n:032x}"))).unwrap();}
        let last=fs::read_dir(&root).unwrap().last().unwrap().unwrap().path();
        replace_state(&last.join("state.json"),&State{schema:1,heartbeat:now(),focused:now(),local_files:true}).unwrap();
        let targets=Instance::targets(&root);assert_eq!(targets.len(),1);assert_eq!(targets[0].0,last);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]fn expired_acknowledgements_and_abandoned_writes_do_not_fill_queue(){
        let root=std::env::temp_dir().join(format!("fv-expired-{}",uuid::Uuid::new_v4()));
        let instance=Instance::new(root.clone()).unwrap();
        for suffix in ["ack","pending","request"]{for _ in 0..12{
            let path=instance.path.join(format!("{}.{suffix}",uuid::Uuid::new_v4().simple()));
            fs::write(&path,b"old").unwrap();
            fs::File::options().write(true).open(path).unwrap().set_times(fs::FileTimes::new().set_modified(SystemTime::now()-Duration::from_secs(30))).unwrap();
        }}
        let fresh=instance.path.join(format!("{}.ack",uuid::Uuid::new_v4().simple()));write_new(&fresh,&true).unwrap();
        Instance::send(&instance.path,WindowEvent::Exit).unwrap();
        assert!(matches!(instance.poll(false).as_slice(),[WindowEvent::Exit]));assert!(fresh.is_file());
        assert_eq!(fs::read_dir(&instance.path).unwrap().count(),2);
        drop(instance);assert_eq!(fs::read_dir(&root).unwrap().count(),0);fs::remove_dir(root).unwrap();
    }
    #[test]fn heartbeat_replacement_keeps_a_complete_state(){
        let root=std::env::temp_dir().join(format!("fv-heartbeat-{}",uuid::Uuid::new_v4()));
        let instance=Instance::new(root.clone()).unwrap();
        let stop=Arc::new(std::sync::atomic::AtomicBool::new(false));let reader_stop=stop.clone();let path=instance.path.join("state.json");
        let (ready_tx,ready_rx)=std::sync::mpsc::channel();
        let reader=std::thread::spawn(move||{
            let first:State=read(&path).unwrap();assert_eq!(first.schema,1);ready_tx.send(()).unwrap();let mut count=1;
            while !reader_stop.load(Ordering::Acquire){let state:State=read(&path).unwrap();assert_eq!(state.schema,1);assert!(state.local_files);count+=1;}
            count
        });
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        for _ in 0..100{instance.updated.store(0,Ordering::Release);instance.touch(true).unwrap();}
        stop.store(true,Ordering::Release);assert!(reader.join().unwrap()>1);
        assert_eq!(fs::read_dir(&instance.path).unwrap().count(),1);drop(instance);fs::remove_dir(root).unwrap();
    }
    #[test]fn published_messages_are_complete_and_never_replace_existing_files(){
        let root=std::env::temp_dir().join(format!("fv-publish-{}",uuid::Uuid::new_v4()));fs::create_dir(&root).unwrap();
        let path=root.join("message.request");write_new(&path,&"original").unwrap();
        assert!(write_new(&path,&"replacement").is_err());assert_eq!(read::<String>(&path).unwrap(),"original");
        assert_eq!(fs::read_dir(&root).unwrap().count(),1);fs::remove_dir_all(root).unwrap();
    }
    #[test]fn exit_broadcasts_do_not_accumulate_unclaimed_acknowledgements(){
        let root=std::env::temp_dir().join(format!("fv-exit-{}",uuid::Uuid::new_v4()));
        let first=Instance::new(root.clone()).unwrap();let second=Instance::new(root.clone()).unwrap();
        for _ in 0..40{first.exit_all().unwrap();assert!(matches!(second.poll(false).as_slice(),[WindowEvent::Exit]));}
        assert_eq!(fs::read_dir(&second.path).unwrap().count(),1);
        drop(first);drop(second);fs::remove_dir_all(root).unwrap();
    }
    #[test]fn a_full_window_queue_does_not_prevent_other_windows_exiting(){
        let root=std::env::temp_dir().join(format!("fv-broadcast-{}",uuid::Uuid::new_v4()));
        let first=Instance::new(root.clone()).unwrap();let blocked=Instance::new(root.clone()).unwrap();let healthy=Instance::new(root.clone()).unwrap();
        fs::write(blocked.path.join("state.json"),serde_json::to_vec(&State{schema:1,heartbeat:now(),focused:u64::MAX,local_files:true}).unwrap()).unwrap();
        for i in 0..32{fs::write(blocked.path.join(format!("{i:032x}.ack")),b"true").unwrap();}
        assert!(first.exit_all().is_err());assert!(matches!(healthy.poll(false).as_slice(),[WindowEvent::Exit]));
        drop(first);drop(blocked);drop(healthy);fs::remove_dir_all(root).unwrap();
    }
    #[test]fn exit_targets_only_registered_windows_and_keeps_unrelated_files(){
        let root=std::env::temp_dir().join(format!("fv-windows-{}",uuid::Uuid::new_v4()));
        let first=Instance::new(root.clone()).unwrap();let second=Instance::new(root.clone()).unwrap();
        fs::write(root.join("unrelated.txt"),b"keep").unwrap();first.exit_all().unwrap();
        assert!(matches!(second.poll(false).as_slice(),[WindowEvent::Exit]));assert!(first.poll(false).is_empty());
        drop(first);drop(second);assert_eq!(fs::read(root.join("unrelated.txt")).unwrap(),b"keep");fs::remove_dir_all(root).unwrap();
    }
    #[test]fn acknowledged_open_preserves_unicode_and_load_options(){
        let root=std::env::temp_dir().join(format!("fv-windows-{}",uuid::Uuid::new_v4()));let window=Instance::new(root.clone()).unwrap();
        let source=root.join("图像.fits");fs::write(&source,b"fixture").unwrap();let target=source.to_string_lossy().into_owned();
        let thread=std::thread::spawn(move||Instance::forward(&root,target,fitsviewer_core::LoadOptions{hdu:Some(2),..Default::default()}));
        let deadline=std::time::Instant::now()+Duration::from_secs(3);let mut received=vec![];
        while received.is_empty() && std::time::Instant::now()<deadline{received=window.poll(false);std::thread::sleep(Duration::from_millis(20));}
        assert!(thread.join().unwrap());assert!(matches!(&received[0],WindowEvent::Open{path,options} if path.contains("图像") && options.hdu==Some(2)));
        let root=window.root.clone();drop(window);fs::remove_dir_all(root).unwrap();
    }
    #[test]fn remote_helpers_can_exit_but_never_accept_a_local_file_forward(){
        let root=std::env::temp_dir().join(format!("fv-windows-{}",uuid::Uuid::new_v4()));let remote=Instance::remote(root.clone()).unwrap();
        let source=root.join("local.fits");fs::write(&source,b"fixture").unwrap();assert!(!Instance::forward(&root,source.to_string_lossy().into_owned(),Default::default()));
        let local=Instance::new(root.clone()).unwrap();local.exit_all().unwrap();assert!(matches!(remote.poll(false).as_slice(),[WindowEvent::Exit]));
        drop(local);drop(remote);fs::remove_dir_all(root).unwrap();
    }
}
