//! Per-host scientific worker, document leases and explicitly pinned result files.
//! 宿主独占科研子进程树和已交付产物；文档引用独立于 PRO 面板生命周期。
use serde_json::{json,Value};
use std::{collections::{HashMap,HashSet,VecDeque},io::{BufRead,BufReader,Read,Write},path::{Path,PathBuf},process::{Child,ChildStdin,Command,Stdio},sync::{mpsc,Arc,Mutex,atomic::{AtomicBool,Ordering}},time::{Duration,Instant}};
use crate::{science_process,science_sessions::{Sessions,Role,Release}};
type OwnedChild=Arc<Mutex<Child>>;
type OwnedTree=Arc<Mutex<science_process::Tree>>;
struct Process {child:OwnedChild,tree:OwnedTree,input:ChildStdin,output:mpsc::Receiver<Result<String,String>>,_owner:science_process::SpawnOwner}
struct Bus {sequence:u64,events:VecDeque<Value>,heartbeat:Option<Instant>}
struct Artifact {path:PathBuf,owner:String,group:String,epoch:u64,document:u64,pins:HashSet<String>,created:Instant}
pub struct Bridge {
    python:PathBuf,script:PathBuf,cache:PathBuf,instance:String,
    serial:Mutex<()>,process:Mutex<Option<Process>>,child:Mutex<Option<(OwnedChild,OwnedTree)>>,
    sessions:Mutex<Sessions>,artifacts:Mutex<HashMap<String,Artifact>>,bus:Mutex<Bus>,stopping:AtomicBool,
}
impl Bridge {
    pub fn new(python:Option<PathBuf>,project:&Path,cache:PathBuf)->Self {
        let linked=if cfg!(windows){std::env::var_os("LOCALAPPDATA").map(PathBuf::from).and_then(|p|std::fs::read(p.join("FitsViewer/environment.json")).ok()).and_then(|b|serde_json::from_slice::<Value>(&b).ok()).and_then(|v|v["python"].as_str().map(PathBuf::from)).filter(|p|p.is_file())}else{None};
        let python=python.or_else(||std::env::var_os("FITSVIEWER_PYTHON").map(PathBuf::from)).or(linked).unwrap_or_else(||PathBuf::from(if cfg!(windows){"python"}else{"python3"}));
        let instance=format!("host-{}-{}",std::process::id(),uuid::Uuid::new_v4().simple());
        Self{python,script:project.join("web/python/fitsviewer_science.py"),cache:cache.join(&instance),instance,serial:Mutex::new(()),process:Mutex::new(None),child:Mutex::new(None),sessions:Mutex::new(Sessions::default()),artifacts:Mutex::new(HashMap::new()),bus:Mutex::new(Bus{sequence:0,events:VecDeque::new(),heartbeat:None}),stopping:AtomicBool::new(false)}
    }
    pub fn cache_root(&self)->&Path{&self.cache}
    pub fn ensure_cache(&self)->Result<(),String>{std::fs::create_dir_all(&self.cache).map_err(|e|e.to_string())?;std::fs::write(self.cache.join(".owner"),&self.instance).map_err(|e|e.to_string())}
    pub fn start_reaper(this:&Arc<Self>){
        let weak=Arc::downgrade(this);
        std::thread::spawn(move||loop{
            std::thread::sleep(Duration::from_secs(2));
            let Some(bridge)=weak.upgrade()else{break;};if bridge.stopping.load(Ordering::Acquire){break;}
            if let Ok(_guard)=bridge.serial.try_lock(){bridge.reap_locked();};
        });
    }
    fn start(&self)->Result<Process,String>{
        if self.stopping.load(Ordering::Acquire){return Err("Scientific host has stopped".into());}
        if !self.script.is_file(){return Err("The scientific adapter is missing from this installation".into());}
        self.ensure_cache()?;
        let log=std::fs::OpenOptions::new().create(true).append(true).open(self.cache.join("worker.log")).map_err(|e|e.to_string())?;
        let mut command=Command::new(&self.python);
        command.arg("-u").arg(&self.script).arg("--cache").arg(&self.cache).env("PYTHONIOENCODING","utf-8").env("MPLBACKEND","Agg").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(log);
        if let Ok(binary)=std::env::current_exe(){command.env("FITSVIEWER_CORE_BINARY",binary);}
        let (mut child,mut owned_tree,owner)=science_process::spawn(command).map_err(|e|format!("Cannot start selected Python environment: {e}. Configure --python or FITSVIEWER_PYTHON."))?;
        let (Some(input),Some(stdout))=(child.stdin.take(),child.stdout.take())else{owned_tree.stop();let _=child.kill();let _=child.wait();return Err("Missing scientific worker pipes".into());};
        let tree=Arc::new(Mutex::new(owned_tree));
        let (sender,output)=mpsc::sync_channel(2);
        std::thread::spawn(move||{
            let mut reader=BufReader::new(stdout);
            loop{
                let mut bytes=Vec::new();
                match reader.by_ref().take(32*1024*1024+1).read_until(b'\n',&mut bytes){
                    Ok(0)=>break,
                    Ok(_) if bytes.len()>32*1024*1024=>{let _=sender.send(Err("Scientific response exceeds 32 MiB".into()));break;},
                    Ok(_)=>{let line=String::from_utf8(bytes).map_err(|e|e.to_string());if sender.send(line).is_err(){break;}},
                    Err(error)=>{let _=sender.send(Err(error.to_string()));break;},
                }
            }
        });
        let child=Arc::new(Mutex::new(child));*self.child.lock().unwrap()=Some((child.clone(),tree.clone()));let mut process=Process{child,tree,input,output,_owner:owner};
        if self.stopping.load(Ordering::Acquire){stop(&mut process);return Err("Scientific host stopped while the worker was starting".into());}Ok(process)
    }
    fn exchange(process:&mut Process,operation:&str,params:Value)->Result<Value,String>{
        let encoded=serde_json::to_vec(&json!({"id":1,"operation":operation,"params":params})).map_err(|e|e.to_string())?;
        if encoded.len()>8*1024*1024{return Err("Scientific request exceeds 8 MiB".into());}
        process.input.write_all(&encoded).and_then(|_|process.input.write_all(b"\n")).and_then(|_|process.input.flush()).map_err(|e|e.to_string())?;
        let line=process.output.recv_timeout(Duration::from_secs(120)).map_err(|_|"Scientific worker timed out or stopped".to_string())??;
        serde_json::from_str(&line).map_err(|e|format!("Invalid scientific response: {e}"))
    }
    fn result(response:Value)->Result<Value,String>{if response["ok"]==true{Ok(response["result"].clone())}else{Err(response["error"].as_str().unwrap_or("Scientific operation failed").into())}}
    fn apply_releases(&self,releases:Vec<Release>)->Result<(),String>{
        if releases.is_empty(){return Ok(());}
        let mut failure=None;
        for release in releases {
            let operation=if release.role==Role::View{if release.close_view{"release_owner"}else{"release_document"}}else if release.role==Role::Gaia{"release_gaia_owner"}else{"release_pro_owner"};
            let mut params=json!({"_viewer_session":release.owner,"_science_role":if release.role==Role::Gaia{"gaia"}else if release.role==Role::Pro{"pro"}else{"view"}});
            if let Some(document)=release.document{params["_document_generation"]=document.into();}
            if let Some(epoch)=release.epoch{params["_role_epoch"]=epoch.into();}
            if let Ok(mut slot)=self.process.lock(){if let Some(process)=slot.as_mut(){if let Err(error)=Self::exchange(process,operation,params).and_then(Self::result){failure=Some(error);if let Some(mut process)=slot.take(){stop(&mut process);}}}}
            let mut artifacts=self.artifacts.lock().unwrap();
            artifacts.retain(|_,artifact|{
                if release.close_view{artifact.pins.remove(&release.owner);}
                let matches=artifact.owner==release.owner && match release.role {
                    Role::View=>release.document.is_none_or(|generation|artifact.document<=generation),
                    Role::Pro=>artifact.group=="pro" && release.epoch.is_none_or(|epoch|artifact.epoch<=epoch),
                    Role::Gaia=>artifact.group=="gaia" && release.epoch.is_none_or(|epoch|artifact.epoch<=epoch),
                };
                if matches && artifact.pins.is_empty(){let _=std::fs::remove_file(&artifact.path);false}else{artifact.path.is_file()}
            });
        }
        failure.map_or(Ok(()),|error|Err(format!("Scientific session cleanup failed; worker stopped: {error}")))
    }
    fn reap_locked(&self){
        let releases=self.sessions.lock().unwrap().expire();let expired=!releases.is_empty();if let Err(error)=self.apply_releases(releases){eprintln!("{error}");}
        if expired && !self.sessions.lock().unwrap().scientific_roles(){self.stop_worker();}
        self.artifacts.lock().unwrap().retain(|_,artifact|{
            if artifact.pins.is_empty() && artifact.created.elapsed()>Duration::from_secs(15){let _=std::fs::remove_file(&artifact.path);false}else{artifact.path.is_file()}
        });
    }
    pub fn call(&self,operation:&str,params:Value)->Result<Value,String>{
        if ["artifact_detach","release_owner","release_pro_owner","release_gaia_owner"].contains(&operation){return Err("Internal scientific lifecycle operation".into());}
        #[cfg(not(feature="pro"))] crate::mini_science(operation,&params)?;
        let _guard=self.serial.lock().map_err(|_|"Scientific session lock failed")?;
        if self.stopping.load(Ordering::Acquire){return Err("Scientific host has stopped".into());}
        self.reap_locked();
        if operation=="runtime_status"{
            let pid=self.child.lock().unwrap().as_ref().and_then(|(child,_)|child.lock().ok().and_then(|mut c|if c.try_wait().ok().flatten().is_none(){Some(c.id())}else{None}));
            let artifacts=self.artifacts.lock().unwrap();let (views,pro,gaia)=self.sessions.lock().unwrap().counts();
            return Ok(json!({"worker_running":pid.is_some(),"worker_pid":pid,"view_sessions":views,"pro_sessions":pro,"gaia_sessions":gaia,"artifacts":artifacts.len(),"artifact_bytes":artifacts.values().filter_map(|a|a.path.metadata().ok()).map(|m|m.len()).sum::<u64>()}));
        }
        let (owner,releases)=self.sessions.lock().unwrap().prepare(operation,&params)?;
        if operation=="release_document"{if let Some(paths)=params["preserve_paths"].as_array(){self.pin_paths(&owner,paths,false)?;}}
        self.apply_releases(releases)?;
        if ["release_view","release_pro","gaia_release","release_document"].contains(&operation){
            if !self.sessions.lock().unwrap().scientific_roles(){self.stop_worker();}
            return Ok(json!({"released":true}));
        }
        if ["pro_acquire","pro_heartbeat","gaia_acquire","gaia_heartbeat","view_heartbeat"].contains(&operation){return Ok(json!({"acquired":true}));}
        if operation=="artifact_refs"{self.pin_paths(&owner,params["paths"].as_array().ok_or("Artifact references require paths")?,true)?;return Ok(json!({"updated":true}));}
        #[cfg(feature="pro")]
        if operation.starts_with("bridge_") || operation=="samp_bridge"{return self.bridge_command(operation,&params);}
        let mut slot=self.process.lock().map_err(|_|"Scientific worker lock failed")?;
        if slot.is_none() && ["job_cancel","job_status"].contains(&operation){return Ok(json!({"state":"cancelled","cancel_requested":true}));}
        if slot.is_none(){*slot=Some(self.start()?);}
        let response=match Self::exchange(slot.as_mut().unwrap(),operation,params.clone()){
            Ok(response)=>response,Err(error)=>{if let Some(mut process)=slot.take(){stop(&mut process);}return Err(error);},
        };
        let mut result=Self::result(response)?;let mut detached=vec![];
        self.register_artifacts(&mut result,&owner,&params,&mut detached)?;
        if !detached.is_empty(){
            let response=Self::exchange(slot.as_mut().unwrap(),"artifact_detach",json!({"artifact_ids":detached}));
            if response.and_then(Self::result).is_err(){if let Some(mut process)=slot.take(){stop(&mut process);}return Err("Failed to transfer scientific artifacts to the host".into());}
        }
        Ok(result)
    }
    fn register_artifacts(&self,value:&mut Value,owner:&str,params:&Value,detached:&mut Vec<String>)->Result<(),String>{
        match value{
            Value::Object(object)=>{
                if let Some(path)=object.remove("_path").and_then(|v|v.as_str().map(PathBuf::from)){
                    let id=object.get("artifact_id").and_then(Value::as_str).ok_or("Invalid session artifact")?.to_owned();
                    let path=match path.canonicalize(){Ok(path)=>path,Err(_) if !self.artifacts.lock().unwrap().contains_key(&id)=>{object.insert("artifact_available".into(),false.into());object.insert("artifact_id".into(),Value::Null);object.insert("artifact_error".into(),"Temporary result expired; run the task again".into());return Ok(());},Err(error)=>return Err(error.to_string())};let root=self.cache.canonicalize().map_err(|e|e.to_string())?;
                    if !path.starts_with(root) || !path.is_file(){return Err("Artifact is outside the host cache".into());}
                    let mut artifacts=self.artifacts.lock().unwrap();artifacts.retain(|_,a|a.path.is_file());
                    if !artifacts.contains_key(&id){
                        if artifacts.len()>=64 || artifacts.values().filter_map(|a|a.path.metadata().ok()).map(|m|m.len()).sum::<u64>()+path.metadata().map_err(|e|e.to_string())?.len()>512*1024*1024{let _=std::fs::remove_file(&path);return Err("Session export cache is full; close unused views".into());}
                        artifacts.insert(id.clone(),Artifact{path,owner:owner.into(),group:object.get("artifact_group").and_then(Value::as_str).unwrap_or("pro").into(),epoch:object.get("role_epoch").and_then(Value::as_u64).or(params["_role_epoch"].as_u64()).unwrap_or(0),document:object.get("document_generation").and_then(Value::as_u64).or(params["_document_generation"].as_u64()).unwrap_or(0),pins:HashSet::new(),created:Instant::now()});
                    }
                    detached.push(id);
                }
                for child in object.values_mut(){self.register_artifacts(child,owner,params,detached)?;}
            },Value::Array(array)=>for child in array{self.register_artifacts(child,owner,params,detached)?;},_=>{},
        }Ok(())
    }
    fn pin_paths(&self,owner:&str,paths:&[Value],replace:bool)->Result<(),String>{
        if paths.len()>128 || paths.iter().any(|p|p.as_str().is_none_or(|p|p.len()>32768)){return Err("Invalid artifact references".into());}
        let mut artifacts=self.artifacts.lock().unwrap();
        for (id,artifact) in &mut *artifacts{
            if replace{artifact.pins.remove(owner);}
            if paths.iter().filter_map(Value::as_str).any(|p|p.strip_prefix("artifact:")==Some(id.as_str()) || Path::new(p).canonicalize().is_ok_and(|p|p==artifact.path)){artifact.pins.insert(owner.into());}
        }Ok(())
    }
    #[cfg(feature="pro")]
fn bridge_command(&self,operation:&str,params:&Value)->Result<Value,String>{
        let mut bus=self.bus.lock().unwrap();
        let action=if operation=="samp_bridge"{params["action"].as_str().unwrap_or("status")}else{operation.strip_prefix("bridge_").unwrap_or("")};
        if action=="heartbeat"{bus.heartbeat=Some(Instant::now());return Ok(json!({"connected":true}));}
        if action=="disconnect"{bus.heartbeat=None;bus.events.clear();return Ok(json!({"connected":false}));}
        if action=="status" || action=="connect"{return Ok(json!({"connected":bus.heartbeat.is_some_and(|t|t.elapsed()<Duration::from_secs(15)),"transport":"local bridge over existing forwarded HTTP","hint":"Run fitsviewer samp-bridge --url <forwarded viewer URL> on the machine running TOPCAT/Aladin."}));}
        if action=="poll" || action=="events"{
            let after=params["after"].as_u64().unwrap_or(0);let direction=if action=="poll"{"to_hub"}else{"from_hub"};
            return Ok(json!({"events":bus.events.iter().filter(|e|e["sequence"].as_u64().unwrap_or(0)>after && e["direction"]==direction).collect::<Vec<_>>(),"latest":bus.sequence}));
        }
        if action=="send" || action=="publish"{
            if params.to_string().len()>1024*1024{return Err("SAMP message exceeds 1 MiB".into());}
            let mtype=params["mtype"].as_str().ok_or("Missing SAMP mtype")?;
            if !["image.load.fits","table.load.votable","table.load.fits","coord.pointAt.sky","table.highlight.row","table.select.rowList","spectrum.load.ssa-generic"].contains(&mtype){return Err("Unsupported SAMP mtype".into());}
            bus.sequence+=1;let event=json!({"sequence":bus.sequence,"direction":if action=="send"{"to_hub"}else{"from_hub"},"mtype":mtype,"params":params["params"],"path":params["path"],"recipient":params["recipient"]});
            bus.events.push_back(event);while bus.events.len()>128{bus.events.pop_front();}
            return Ok(json!({"sent":true,"sequence":bus.sequence}));
        }
        Err("Bridge action must be status, send, events or connect".into())
    }
    #[cfg(feature="pro")]
    pub fn import(&self,bytes:&[u8],suffix:&str,owner:&str)->Result<Value,String>{
        let _guard=self.serial.lock().map_err(|_|"Scientific session lock failed")?;
        if self.stopping.load(Ordering::Acquire){return Err("Scientific host has stopped".into());}
        if bytes.len()>64*1024*1024{return Err("Bridge upload exceeds 64 MiB".into());}
        self.sessions.lock().unwrap().prepare_upload(owner)?;
        let suffix=if [".fits",".fit",".fts",".fz",".gz",".vot",".xml",".csv",".dat",".txt"].contains(&suffix){suffix}else{".fits"};
        self.ensure_cache()?;let id=uuid::Uuid::new_v4().simple().to_string();let path=self.cache.join(format!("upload-{id}{suffix}"));std::fs::write(&path,bytes).map_err(|e|e.to_string())?;
        let mut value=json!({"artifact_id":id,"_path":path,"kind":"upload","bytes":bytes.len(),"path":format!("artifact:{id}")});self.register_artifacts(&mut value,owner,&Value::Null,&mut vec![])?;self.pin_paths(owner,&[json!(format!("artifact:{id}"))],false)?;Ok(value)
    }
    pub fn artifact(&self,id:&str)->Result<PathBuf,String>{self.artifacts.lock().unwrap().get(id).map(|a|a.path.clone()).filter(|p|p.is_file()).ok_or_else(||"Session artifact is unavailable".into())}
    pub fn claim(&self,id:&str,owner:&str,generation:Option<u64>)->Result<(),String>{let _guard=self.serial.lock().map_err(|_|"Scientific session lock failed")?;if self.stopping.load(Ordering::Acquire){return Err("Scientific host has stopped".into());}let mut params=json!({"_viewer_session":owner});if let Some(generation)=generation{params["_document_generation"]=generation.into();}self.sessions.lock().unwrap().prepare("artifact_refs",&params)?;self.pin_paths(owner,&[json!(format!("artifact:{id}"))],false)}
    fn stop_worker(&self){
        if let Ok(mut slot)=self.process.try_lock(){if let Some(mut process)=slot.take(){stop(&mut process);}}
        if let Ok(mut owned)=self.child.lock(){if let Some((child,tree))=owned.take(){if let Ok(mut tree)=tree.lock(){tree.stop();}if let Ok(mut child)=child.lock(){let _=child.kill();let _=child.wait();}}}
        self.cleanup_unregistered();
    }
    fn cleanup_unregistered(&self){
        if self.process.lock().is_ok_and(|slot|slot.is_some()){return;}
        if std::fs::read_to_string(self.cache.join(".owner")).ok().as_deref()!=Some(&self.instance){return;}
        let artifacts=self.artifacts.lock().unwrap();
        if let Ok(entries)=std::fs::read_dir(&self.cache){for entry in entries.flatten(){let path=entry.path();if path.is_file() && path.file_name().is_some_and(|name|name!=".owner" && name!="worker.log") && !artifacts.values().any(|artifact|path.canonicalize().is_ok_and(|path|artifact.path==path)){let _=std::fs::remove_file(path);}}}
    }
    pub fn shutdown(&self){
        self.stopping.store(true,Ordering::Release);self.stop_worker();
        self.artifacts.lock().unwrap().clear();
        // Delete only this host's unique, marked directory, never the shared cache parent.
        if std::fs::read_to_string(self.cache.join(".owner")).ok().as_deref()==Some(&self.instance) && self.cache.file_name().is_some_and(|n|n.to_string_lossy()==self.instance){let _=std::fs::remove_dir_all(&self.cache);}
    }
}
fn stop(process:&mut Process){
    let _=process.input.write_all(b"{\"operation\":\"shutdown\"}\n");let _=process.input.flush();
    if let Ok(mut child)=process.child.lock(){for _ in 0..10{if child.try_wait().ok().flatten().is_some(){break;}std::thread::sleep(Duration::from_millis(30));}}
    if let Ok(mut tree)=process.tree.lock(){tree.stop();}
    if let Ok(mut child)=process.child.lock(){let _=child.kill();let _=child.wait();}
}
impl Drop for Bridge{fn drop(&mut self){self.shutdown();}}
