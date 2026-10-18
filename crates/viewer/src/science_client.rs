use eframe::egui;
use serde_json::Value;
use std::sync::{Arc,Mutex,atomic::{AtomicU64,Ordering}};
pub type LocalScience=Arc<dyn Fn(&str,Value)->Result<Value,String>+Send+Sync>;
pub struct Reply {pub generation:u64,pub operation:String,pub result:Result<Value,String>}
#[derive(Clone)]
pub struct Client {pub local:Option<LocalScience>,api:Option<String>,token:String,pub session:String,pub replies:Arc<Mutex<Vec<Reply>>>,document:Arc<AtomicU64>,pro_epoch:Arc<AtomicU64>,gaia_epoch:Arc<AtomicU64>}
impl Client {
    pub fn new(api:Option<String>,token:String)->Self{
        #[cfg(target_arch="wasm32")] let session=format!("view-{}-{}",js_sys::Date::now(),js_sys::Math::random());
        #[cfg(not(target_arch="wasm32"))] let session=format!("view-{}-{:?}",std::process::id(),std::time::SystemTime::now());
        Self{local:None,api,token,session,replies:Arc::new(Mutex::new(Vec::new())),document:Arc::new(AtomicU64::new(0)),pro_epoch:Arc::new(AtomicU64::new(0)),gaia_epoch:Arc::new(AtomicU64::new(0))}
    }
    pub fn request(&self,ctx:&egui::Context,generation:u64,operation:&str,params:Value){self.request_labeled(ctx,generation,operation,operation,params);}
    pub fn request_labeled(&self,ctx:&egui::Context,generation:u64,operation:&str,label:&str,mut params:Value){
        self.bind_document(generation);
        let gaia=label.starts_with("gaia.") || operation.starts_with("gaia_") || operation=="job_start" && params["operation"]=="gaia_solve";
        let role=if gaia{"gaia"}else if ["wcs","capabilities","artifact_refs","view_heartbeat","release_view","release_document","measure","header"].contains(&operation){"view"}else{"pro"};
        let counter=if gaia{self.gaia_epoch.clone()}else{self.pro_epoch.clone()};
        let epoch=if operation=="pro_acquire" || operation=="gaia_acquire" || gaia && operation=="job_start"{counter.fetch_add(1,Ordering::AcqRel)+1}else if operation=="release_pro" || operation=="gaia_release"{counter.fetch_add(1,Ordering::AcqRel)}else{counter.load(Ordering::Acquire)};
        if let Some(object)=params.as_object_mut(){object.insert("_viewer_session".into(),Value::String(self.session.clone()));object.insert("_document_generation".into(),generation.into());object.insert("_role_epoch".into(),epoch.into());object.insert("_science_role".into(),role.into());}
        if operation=="job_start"{params["params"]["_document_generation"]=generation.into();params["params"]["_role_epoch"]=epoch.into();params["params"]["_science_role"]=role.into();}
        if label.starts_with("gaia."){
            params["_gaia_generation"]=serde_json::json!(generation);
            if operation=="job_start"{params["params"]["_gaia_generation"]=serde_json::json!(generation);}
        }
        let replies=self.replies.clone();let document=self.document.clone();let role_guard=(role!="view").then_some((counter,epoch));let operation=operation.to_string();let label=label.to_string();let ctx=ctx.clone();
        let current=move||document.load(Ordering::Acquire)==generation && role_guard.as_ref().is_none_or(|(counter,epoch)|counter.load(Ordering::Acquire)==*epoch);
        if let Some(api)=&self.api {
            let mut request=ehttp::Request::post(format!("{api}/api/science"),serde_json::to_vec(&serde_json::json!({"operation":operation,"params":params})).unwrap_or_default());
            request.headers.insert("Content-Type","application/json");if !self.token.is_empty(){request.headers.insert("X-Fitsviewer-Token",&self.token);}
            #[cfg(target_arch="wasm32")]
            if let Some(cookie)=web_sys::window().and_then(|w|js_sys::Reflect::get(&w,&"document".into()).ok()).and_then(|d|js_sys::Reflect::get(&d,&"cookie".into()).ok()).and_then(|c|c.as_string()) {
                if let Some(xsrf)=cookie.split(';').map(str::trim).find_map(|s|s.strip_prefix("_xsrf=")){request.headers.insert("X-XSRFToken",xsrf);}
            }
            ehttp::fetch(request,move|r|{if !current(){return;}let result=r.and_then(|r|if r.ok{serde_json::from_slice(&r.bytes).map_err(|e|e.to_string())}else{Err(String::from_utf8_lossy(&r.bytes).into_owned())});replies.lock().unwrap().push(Reply{generation,operation:label,result});ctx.request_repaint();});
        }else{
            #[cfg(not(target_arch="wasm32"))]
            if let Some(local)=&self.local {let local=local.clone();std::thread::spawn(move||{let result=local(&operation,params);if current(){replies.lock().unwrap().push(Reply{generation,operation:label,result});ctx.request_repaint();}});return;}
            replies.lock().unwrap().push(Reply{generation,operation:label,result:Err("Select a Python environment for WCS/scientific tools".into())});
        }
    }
    pub fn bind_document(&self,generation:u64){self.document.fetch_max(generation,Ordering::AcqRel);}
    pub fn drain(&self)->Vec<Reply>{std::mem::take(&mut *self.replies.lock().unwrap())}
    pub fn artifact_url(&self,id:&str)->Option<String>{self.api.as_ref().map(|api|format!("{api}/api/artifact?id={}&token={}",urlencoding::encode(id),urlencoding::encode(&self.token)))}
}
