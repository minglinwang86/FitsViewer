//! Own the worker's descendant processes as well as the Python process itself.
//! 科研 worker 的 packet 子进程属于同一进程组 / Windows Job，不遗留孤儿进程。
use std::process::{Child,Command};

/// Keep the creating thread alive for Linux's thread-scoped parent-death signal.
/// Linux 的父线程退出也会触发 PDEATHSIG，因此创建线程必须和 worker 同寿命。
pub struct SpawnOwner {
    keepalive: Option<std::sync::mpsc::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for SpawnOwner {
    fn drop(&mut self) {
        self.keepalive.take();
        if let Some(thread) = self.thread.take() { let _ = thread.join(); }
    }
}
pub fn spawn(mut command: Command) -> Result<(Child, Tree, SpawnOwner), String> {
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let (keepalive, lifetime) = std::sync::mpsc::channel::<()>();
    let thread = std::thread::Builder::new().name("fitsviewer-worker-owner".into()).spawn(move || {
        let result: Result<(Child, Tree), String> = (|| {
            prepare(&mut command);
            let mut child = command.spawn().map_err(|e| e.to_string())?;
            let tree = Tree::attach(&mut child)?;
            Ok((child, tree))
        })();
        let started = result.is_ok();
        if let Err(error) = sender.send(result) {
            if let Ok((mut child, mut tree)) = error.0 { tree.stop(); let _ = child.kill(); let _ = child.wait(); }
            return;
        }
        if started { let _ = lifetime.recv(); }
    }).map_err(|e| e.to_string())?;
    let owner = SpawnOwner { keepalive: Some(keepalive), thread: Some(thread) };
    let (child, tree) = receiver.recv().map_err(|_| "Scientific process owner stopped")??;
    Ok((child, tree, owner))
}

#[cfg(unix)]
pub struct Tree{group:Option<i32>}
#[cfg(windows)]
pub struct Tree{job:Option<std::os::windows::io::OwnedHandle>}
#[cfg(not(any(unix,windows)))]
pub struct Tree;

pub fn prepare(command:&mut Command){
    #[cfg(unix)]{use std::os::unix::process::CommandExt;command.process_group(0);}
    #[cfg(target_os="linux")]{use std::os::unix::process::CommandExt;let parent=unsafe{getpid()};unsafe{command.pre_exec(move||{if prctl(1,9_usize,0_usize,0_usize,0_usize)!=0{return Err(std::io::Error::last_os_error());}if getppid()!=parent{return Err(std::io::Error::from_raw_os_error(10));}Ok(())});}}
    #[cfg(windows)]{use std::os::windows::process::CommandExt;command.creation_flags(0x08000000 | windows_sys::Win32::System::Threading::CREATE_SUSPENDED);}
    let _=command;
}
#[cfg(target_os="linux")]
unsafe extern "C"{fn prctl(option:i32,...)->i32;fn getppid()->i32;fn getpid()->i32;}
pub fn guard_packet_parent()->Result<(),String>{
    #[cfg(target_os="linux")]{let parent=std::env::var("FITSVIEWER_PACKET_PARENT").ok().and_then(|s|s.parse::<i32>().ok()).ok_or("Missing owned packet parent")?;unsafe{if prctl(1,9_usize,0_usize,0_usize,0_usize)!=0 || getppid()!=parent{return Err("Owned packet parent is unavailable".into());}}}
    Ok(())
}
impl Tree {
    pub fn attach(child:&mut Child)->Result<Self,String>{
        #[cfg(unix)]{Ok(Self{group:Some(i32::try_from(child.id()).map_err(|_|"Invalid worker process group")?)})}
        #[cfg(windows)]{
            use std::os::windows::io::{AsRawHandle,FromRawHandle,OwnedHandle};
            use windows_sys::Win32::System::JobObjects::*;
            // Start suspended: a venv redirector can spawn Python even before stdin.
            // 暂停创建后先关联 Job，防止 venv 启动器提前创建未纳管的 Python 子进程。
            unsafe {
                let handle=CreateJobObjectW(std::ptr::null(),std::ptr::null());
                if handle.is_null(){let error=std::io::Error::last_os_error();let _=child.kill();let _=child.wait();return Err(error.to_string());}
                let job=OwnedHandle::from_raw_handle(handle);
                let mut limits:JOBOBJECT_EXTENDED_LIMIT_INFORMATION=std::mem::zeroed();
                limits.BasicLimitInformation.LimitFlags=JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
                if SetInformationJobObject(handle,JobObjectExtendedLimitInformation,&limits as *const _ as *const std::ffi::c_void,std::mem::size_of_val(&limits) as u32)==0 || AssignProcessToJobObject(handle,child.as_raw_handle())==0 {
                    let error=std::io::Error::last_os_error();let _=child.kill();let _=child.wait();return Err(format!("Cannot own scientific process tree: {error}"));
                }
                if let Err(error)=resume_initial_thread(child.id()){let _=child.kill();let _=child.wait();return Err(error);}
                Ok(Self{job:Some(job)})
            }
        }
        #[cfg(not(any(unix,windows)))]{let _=child;Ok(Self)}
    }
    pub fn stop(&mut self){
        #[cfg(unix)]{
            unsafe extern "C"{fn kill(pid:i32,signal:i32)->i32;}
            if let Some(group)=self.group.take(){unsafe{kill(-group,9);}}
        }
        #[cfg(windows)]{
            use std::os::windows::io::AsRawHandle;
            if let Some(job)=self.job.take(){unsafe{windows_sys::Win32::System::JobObjects::TerminateJobObject(job.as_raw_handle(),1);}drop(job);}
        }
    }
}
impl Drop for Tree{fn drop(&mut self){self.stop();}}

#[cfg(windows)]
fn resume_initial_thread(process:u32)->Result<(),String>{
    use std::os::windows::io::{FromRawHandle,OwnedHandle,AsRawHandle};
    use windows_sys::Win32::{Foundation::INVALID_HANDLE_VALUE,System::{Diagnostics::ToolHelp::{CreateToolhelp32Snapshot,Thread32First,Thread32Next,THREADENTRY32,TH32CS_SNAPTHREAD},Threading::{OpenThread,ResumeThread,GetProcessIdOfThread,THREAD_SUSPEND_RESUME,THREAD_QUERY_LIMITED_INFORMATION}}};
    unsafe{
        let snapshot=CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD,0);
        if snapshot==INVALID_HANDLE_VALUE{return Err(format!("Cannot inspect suspended worker: {}",std::io::Error::last_os_error()));}
        let snapshot=OwnedHandle::from_raw_handle(snapshot);let mut entry:THREADENTRY32=std::mem::zeroed();entry.dwSize=std::mem::size_of::<THREADENTRY32>() as u32;
        let mut found=None;let mut more=Thread32First(snapshot.as_raw_handle(),&mut entry);
        while more!=0{
            if entry.th32OwnerProcessID==process{if found.replace(entry.th32ThreadID).is_some(){return Err("Suspended worker has more than one initial thread".into());}}
            entry.dwSize=std::mem::size_of::<THREADENTRY32>() as u32;more=Thread32Next(snapshot.as_raw_handle(),&mut entry);
        }
        let id=found.ok_or("Suspended worker initial thread is unavailable")?;
        let thread=OpenThread(THREAD_SUSPEND_RESUME|THREAD_QUERY_LIMITED_INFORMATION,0,id);
        if thread.is_null(){return Err(format!("Cannot open suspended worker thread: {}",std::io::Error::last_os_error()));}
        let thread=OwnedHandle::from_raw_handle(thread);
        if GetProcessIdOfThread(thread.as_raw_handle())!=process{return Err("Suspended worker thread identity changed".into());}
        if ResumeThread(thread.as_raw_handle())!=1{return Err("Cannot safely resume the owned worker's initial thread".into());}
    }
    Ok(())
}

#[cfg(all(test,unix))]
mod tests {
    use super::*;
    #[test]
    fn worker_survives_the_request_thread_and_is_reaped() {
        let request=std::thread::spawn(|| {
            let mut command=Command::new("sh");command.args(["-c","exec sleep 10"]);
            spawn(command).unwrap()
        });
        let (mut child,mut tree,owner)=request.join().unwrap();
        std::thread::sleep(std::time::Duration::from_millis(150));
        assert!(child.try_wait().unwrap().is_none());
        tree.stop();assert!(!child.wait().unwrap().success());drop(owner);
    }
}
