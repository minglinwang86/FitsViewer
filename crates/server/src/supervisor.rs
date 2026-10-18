//! Observe a Jupyter supervisor without depending on its proxy's exit hooks.
//! 直接观察 Jupyter 宿主生命周期，不依赖第三方代理的退出清理。

pub struct Parent {
    #[cfg(windows)] handle:std::os::windows::io::OwnedHandle,
    #[cfg(not(windows))] pid:i32,
    #[cfg(target_os="linux")] started:String,
}
impl Parent {
    pub fn observe(pid:u32)->Result<Self,String>{
        if pid==0 || pid==std::process::id() || pid>i32::MAX as u32{return Err("Invalid Jupyter supervisor PID".into());}
        #[cfg(windows)] let parent={
            use std::os::windows::io::{FromRawHandle,OwnedHandle};
            use windows_sys::Win32::System::Threading::{OpenProcess,PROCESS_QUERY_LIMITED_INFORMATION,PROCESS_SYNCHRONIZE};
            let handle=unsafe{OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION|PROCESS_SYNCHRONIZE,0,pid)};
            if handle.is_null(){return Err(format!("Cannot observe Jupyter supervisor: {}",std::io::Error::last_os_error()));}
            Self{handle:unsafe{OwnedHandle::from_raw_handle(handle)}}
        };
        #[cfg(target_os="linux")] let parent=Self{pid:pid as i32,started:identity(pid as i32).ok_or("Jupyter supervisor is unavailable")?.1};
        #[cfg(not(any(windows,target_os="linux")))] let parent=Self{pid:pid as i32};
        if !parent.alive(){return Err("Jupyter supervisor already exited".into());}
        Ok(parent)
    }
    fn alive(&self)->bool{
        #[cfg(windows)]{
            use std::os::windows::io::AsRawHandle;
            return unsafe{windows_sys::Win32::System::Threading::WaitForSingleObject(self.handle.as_raw_handle(),0)}==windows_sys::Win32::Foundation::WAIT_TIMEOUT;
        }
        #[cfg(target_os="linux")] return identity(self.pid).is_some_and(|(state,started)|state!='Z' && state!='X' && started==self.started);
        #[cfg(all(unix,not(target_os="linux")))]{
            unsafe extern "C"{fn kill(pid:i32,signal:i32)->i32;}
            return unsafe{kill(self.pid,0)}==0 || std::io::Error::last_os_error().raw_os_error()==Some(1);
        }
        #[cfg(not(any(unix,windows)))] return true;
    }
}
#[cfg(target_os="linux")]
fn identity(pid:i32)->Option<(char,String)>{
    let stat=std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let (_,fields)=stat.rsplit_once(')')?;let mut fields=fields.split_ascii_whitespace();
    let state=fields.next()?.chars().next()?;let started=fields.nth(18)?.to_owned();
    Some((state,started))
}
pub async fn exited(parent:Option<Parent>){
    if let Some(parent)=parent{
        while parent.alive(){tokio::time::sleep(std::time::Duration::from_millis(500)).await;}
    }else{std::future::pending::<()>().await;}
}
