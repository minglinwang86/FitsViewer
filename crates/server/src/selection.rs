//! Bounded, authenticated manual image-coordinate sessions. No scientific calculation.
//! 有界人工选星会话；原图坐标与来源显式，不执行质心或测光算法。
use crate::{AppState,OpenRequest,authorize,private_only,resolve,browser_token};
use axum::{extract::{Query,State},http::{HeaderMap,StatusCode},response::{IntoResponse,Response},Json};
use serde::Deserialize;
use serde_json::{json,Value};
use std::{collections::VecDeque,path::Path,time::{Duration,Instant}};

fn terminal(status:&str)->bool{matches!(status,"completed"|"cancelled"|"expired"|"failed")}
struct Session {
    value:Value,deadline:Instant,finished:Option<Instant>,writer:Option<String>,
}
#[derive(Default)]pub struct Store{sessions:VecDeque<Session>}
impl Store {
    fn expire(&mut self){
        let now=Instant::now();
        for session in &mut self.sessions{
            if !terminal(session.value["status"].as_str().unwrap_or("")) && now>=session.deadline{
                session.value["status"]=json!("expired");session.finished=Some(now);
            }
        }
        self.sessions.retain(|s|s.finished.is_none_or(|when|now.duration_since(when)<Duration::from_secs(60)));
    }
    fn add(&mut self,path:String,source:Value,max_points:u64,seconds:u64,options:&Value)->Result<Value,String>{
        self.expire();
        if self.sessions.iter().filter(|s|!terminal(s.value["status"].as_str().unwrap_or(""))).count()>=16{return Err("Too many active manual-selection sessions; finish or cancel an existing session".into());}
        while self.sessions.len()>=64{if let Some(index)=self.sessions.iter().position(|s|terminal(s.value["status"].as_str().unwrap_or(""))){self.sessions.remove(index);}else{return Err("Manual-selection history is full".into());}}
        let mut value=json!({"schema":1,"id":uuid::Uuid::new_v4().to_string(),"path":path,"hdu":source["hdu"],"identity":source["identity"],"source":source,
            "status":"waiting","coordinate_system":"image","origin":0,"points":[],"revision":0,"max_points":max_points,"timeout_seconds":seconds,
            "config_revision":0,"requested_view":{},"view":{},"view_revision":0,"prompt":"","markers":[]});
        configure(&mut value,options)?;
        self.sessions.push_back(Session{value:value.clone(),deadline:Instant::now()+Duration::from_secs(seconds),finished:None,writer:None});Ok(value)
    }
    fn find(&mut self,id:&str)->Result<&mut Session,String>{self.expire();self.sessions.iter_mut().find(|s|s.value["id"]==id).ok_or_else(||"Manual-selection session expired or does not exist".into())}
}
#[derive(Deserialize)]pub struct SessionQuery{ id:String }
fn failure(status:StatusCode,message:impl Into<String>)->Response{(status,message.into()).into_response()}
fn allowed(state:&AppState,headers:&HeaderMap)->Result<(),Response>{
    private_only(state).map_err(IntoResponse::into_response)?;
    authorize(state,headers,None).map_err(IntoResponse::into_response)
}
fn refresh_source(value:&mut Value){
    if terminal(value["status"].as_str().unwrap_or("")){return;}
    let path=value["path"].as_str().unwrap_or("");
    if fitsviewer_core::remote::source_version(Path::new(path)).ok().as_deref()!=value["identity"].as_str(){
        value["status"]=json!("failed");value["error"]=json!("Source image changed; start manual selection again");
    }
}
pub async fn get(State(state):State<AppState>,headers:HeaderMap,Query(query):Query<SessionQuery>)->Response{
    if let Err(error)=allowed(&state,&headers){return error;}
    let mut store=state.selections.lock().unwrap();
    match store.find(&query.id){Ok(session)=>{refresh_source(&mut session.value);if terminal(session.value["status"].as_str().unwrap_or("")) && session.finished.is_none(){session.finished=Some(Instant::now());}Json(session.value.clone()).into_response()},Err(error)=>failure(StatusCode::NOT_FOUND,error)}
}
pub async fn post(State(state):State<AppState>,headers:HeaderMap,Json(input):Json<Value>)->Response{
    if let Err(error)=allowed(&state,&headers){return error;}
    if input.to_string().len()>512*1024{return failure(StatusCode::PAYLOAD_TOO_LARGE,"Manual-selection request exceeds 512 KiB");}
    let action=input["action"].as_str().unwrap_or("");
    if action=="start"{
        let Some(path)=input["path"].as_str()else{return failure(StatusCode::BAD_REQUEST,"Selection requires a FITS path");};
        let max_points=if input.get("max_points").is_none(){4096}else{input["max_points"].as_u64().unwrap_or(0)};
        let seconds=if input.get("timeout_seconds").is_none(){300}else{input["timeout_seconds"].as_u64().unwrap_or(0)};
        if !(1..=4096).contains(&max_points) || !(1..=1800).contains(&seconds){return failure(StatusCode::BAD_REQUEST,"max_points must be 1–4096 and timeout_seconds must be 1–1800");}
        let hdu=if input["hdu"].is_null(){None}else{match input["hdu"].as_u64().filter(|v|*v<=4095){Some(value)=>Some(value as usize),None=>return failure(StatusCode::BAD_REQUEST,"Invalid selection HDU")}};
        let resolved=match resolve(&state,path){Ok(path)=>path,Err(error)=>return error.into_response()};
        let path=resolved.to_string_lossy().into_owned();let logical=path.clone();let images=state.images.clone();
        let manifest=match tokio::task::spawn_blocking(move||images.manifest(&resolved,&logical,fitsviewer_core::LoadOptions{hdu,..Default::default()})).await{
            Ok(Ok(manifest))=>manifest,Ok(Err(error))=>return failure(StatusCode::UNPROCESSABLE_ENTITY,error),Err(_)=>return failure(StatusCode::INTERNAL_SERVER_ERROR,"Unable to inspect selection source"),
        };
        if manifest.info.spectrum.is_some() || manifest.info.width==0 || manifest.info.height==0{return failure(StatusCode::BAD_REQUEST,"Manual star selection requires a two-dimensional image");}
        let source=json!({"path":path,"hdu":manifest.info.hdu,"width":manifest.info.width,"height":manifest.info.height,"identity":manifest.source_version});
        let mut value=match state.selections.lock().unwrap().add(path.clone(),source,max_points,seconds,&input){Ok(value)=>value,Err(error)=>return failure(if error.starts_with("Too many") || error.ends_with("is full"){StatusCode::TOO_MANY_REQUESTS}else{StatusCode::BAD_REQUEST},error)};
        let id=value["id"].as_str().unwrap().to_string();
        let mut url=format!("{}?path={}&hdu={}&selection={id}&window=1",state.runtime.public_url,urlencoding::encode(&path),manifest.info.hdu);
        if let Some(token)=browser_token(&state.runtime){url.push_str(&format!("#fv_token={}",urlencoding::encode(token)));}
        // The browser authorization link is returned once, never stored in session status.
        // 授权链接只返回创建者，不写入状态记录或日志；Jupyter 不含后端密钥。
        value["url"]=json!(url);
        let mut queue=state.queue.lock().unwrap();queue.seq+=1;let seq=queue.seq;
        queue.items.push_back(OpenRequest{seq,path,hdu:Some(manifest.info.hdu),session:None,selection_id:Some(id)});
        if queue.items.len()>32{queue.items.pop_front();}
        return Json(value).into_response();
    }
    let Some(id)=input["id"].as_str()else{return failure(StatusCode::BAD_REQUEST,"Selection session id is required");};
    let mut store=state.selections.lock().unwrap();let session=match store.find(id){Ok(session)=>session,Err(error)=>return failure(StatusCode::NOT_FOUND,error)};
    refresh_source(&mut session.value);
    let result=match action{
        "cancel"=>{if !terminal(session.value["status"].as_str().unwrap_or("")){session.value["status"]=json!("cancelled");}Ok(())},
        "configure"=>configure(&mut session.value,&input),
        "update"=>update(session,&input),
        _=>Err("Unknown manual-selection action".into()),
    };
    if terminal(session.value["status"].as_str().unwrap_or("")) && session.finished.is_none(){session.finished=Some(Instant::now());}
    match result{Ok(())=>Json(session.value.clone()).into_response(),Err(error)=>failure(StatusCode::CONFLICT,error)}
}
fn configure(value:&mut Value,input:&Value)->Result<(),String>{
    if terminal(value["status"].as_str().unwrap_or("")){return Err("Selection has ended; start a new session".into());}
    let mut next=value.clone();
    if let Some(view)=input.get("view"){
        let object=view.as_object().ok_or("Selection view must be an object")?;
        if next["requested_view"].is_null(){next["requested_view"]=json!({});}
        if object.contains_key("limits"){
            if object.contains_key("low") || object.contains_key("high"){return Err("Choose automatic or manual display limits, not both".into());}
            next["requested_view"].as_object_mut().unwrap().remove("low");next["requested_view"].as_object_mut().unwrap().remove("high");
        }
        if object.contains_key("low") || object.contains_key("high"){
            if !object.contains_key("low") || !object.contains_key("high"){return Err("Manual display limits need both low and high".into());}
            next["requested_view"].as_object_mut().unwrap().remove("limits");
        }
        for (key,item) in object{
            let valid=match key.as_str(){"low"|"high"=>item.as_f64().is_some_and(f64::is_finite),"invert"|"fit"=>item.is_boolean(),"limits"=>item=="zscale","stretch"=>item=="Linear","cmap"=>item=="grey",_=>false};
            if !valid{return Err(format!("Unsupported selection view setting: {key}"));}
            next["requested_view"][key]=item.clone();
        }
        if let (Some(low),Some(high))=(next["requested_view"]["low"].as_f64(),next["requested_view"]["high"].as_f64()){if low>=high{return Err("Display limits require low < high".into());}}
    }
    if let Some(prompt)=input.get("prompt"){let text=prompt.as_str().filter(|text|text.chars().count()<=256).ok_or("Selection prompt exceeds 256 characters")?;next["prompt"]=json!(text);}
    if let Some(markers)=input.get("markers"){
        let markers=markers.as_array().filter(|items|items.len()<=1024).ok_or("At most 1024 selection markers are supported")?;
        for marker in markers{
            let valid=["x","y"].into_iter().all(|key|marker[key].as_f64().is_some_and(f64::is_finite))
                && marker["radius"].as_f64().is_some_and(|radius|radius.is_finite() && radius>0. && radius<=4096.)
                && marker["shape"].as_str().is_some_and(|shape|matches!(shape,"circle"|"box"))
                && marker["color"].as_str().is_some_and(|color|matches!(color,"white"|"red"|"green"|"blue"|"cyan"|"magenta"|"yellow"))
                && marker["label"].as_str().is_none_or(|label|label.chars().count()<=128);
            if !valid{return Err("Invalid selection marker geometry, color or label".into());}
        }
        next["markers"]=json!(markers);
    }
    next["config_revision"]=json!(value["config_revision"].as_u64().unwrap_or(0)+1);*value=next;Ok(())
}
fn update(session:&mut Session,input:&Value)->Result<(),String>{
    let writer=input["viewer_id"].as_str().filter(|id|!id.is_empty() && id.len()<=128).ok_or("Selection update needs a viewer id")?;
    if session.writer.as_deref().is_some_and(|previous|previous!=writer){return Err("This selection is already active in another viewer window".into());}
    let revision=input["revision"].as_u64().filter(|v|*v>0).ok_or("Selection revision must be positive")?;
    let previous=session.value["revision"].as_u64().unwrap_or(0);
    if revision<previous{return Err("Stale selection update".into());}
    if revision==previous && session.writer.is_some(){return Ok(());}
    if terminal(session.value["status"].as_str().unwrap_or("")){return Err("Selection has ended; discard this late update".into());}
    let status=input["status"].as_str().filter(|status|matches!(*status,"selecting"|"completed"|"cancelled"|"failed")).ok_or("Invalid selection state")?;
    if matches!(status,"selecting"|"completed"){
        if input["source"]!=session.value["source"]{return Err("Selection source identity or dimensions changed".into());}
        let points=input["points"].as_array().filter(|p|p.len()<=session.value["max_points"].as_u64().unwrap_or(4096) as usize).ok_or("Invalid selection point count")?;
        let width=session.value["source"]["width"].as_f64().unwrap_or(0.);let height=session.value["source"]["height"].as_f64().unwrap_or(0.);
        for point in points{if !point.as_array().is_some_and(|p|p.len()==2) || !point[0].as_f64().is_some_and(|x|x.is_finite() && x>=0. && x<width) || !point[1].as_f64().is_some_and(|y|y.is_finite() && y>=0. && y<height){return Err("Selection point is outside the original image".into());}}
        session.value["points"]=json!(points);
    }
    if let Some(view)=input.get("view"){
        if let (Some(low),Some(high),Some(applied))=(view["low"].as_f64(),view["high"].as_f64(),input["applied_config_revision"].as_u64()){
            if low.is_finite() && high.is_finite() && low<high && applied<=session.value["config_revision"].as_u64().unwrap_or(0) && applied>=session.value["view_revision"].as_u64().unwrap_or(0){
                session.value["view"]=json!({"low":low,"high":high});session.value["view_revision"]=json!(applied);
            }
        }
    }
    if let Some(error)=input["error"].as_str(){session.value["error"]=json!(error.chars().take(1024).collect::<String>());}
    session.writer=Some(writer.into());session.value["revision"]=json!(revision);session.value["status"]=json!(status);Ok(())
}

#[cfg(test)]mod tests{
    use super::*;
    fn session()->Session{Session{value:json!({"status":"waiting","revision":0,"max_points":4,"source":{"path":"fixture.fits","hdu":0,"width":8,"height":6,"identity":"fixture"},"points":[],"view":{},"config_revision":1}),deadline:Instant::now()+Duration::from_secs(10),finished:None,writer:None}}
    fn change(session:&Session)->Value{json!({"viewer_id":"owned-view","revision":1,"status":"selecting","source":session.value["source"],"points":[[0.,0.],[7.9,5.9]]})}
    #[test]fn source_coordinates_revision_and_writer_are_enforced(){let mut s=session();let mut value=change(&s);update(&mut s,&value).unwrap();assert_eq!(s.value["points"],value["points"]);update(&mut s,&value).unwrap();value["viewer_id"]=json!("other");value["revision"]=json!(2);assert!(update(&mut s,&value).is_err());value["viewer_id"]=json!("owned-view");value["source"]["hdu"]=json!(1);assert!(update(&mut s,&value).is_err());}
    #[test]fn out_of_bounds_points_and_late_completions_are_rejected(){let mut s=session();let mut value=change(&s);value["points"]=json!([[8.,0.]]);assert!(update(&mut s,&value).is_err());value["points"]=json!([[2.,3.]]);value["status"]=json!("completed");update(&mut s,&value).unwrap();value["revision"]=json!(2);value["points"]=json!([[4.,5.]]);assert!(update(&mut s,&value).is_err());assert_eq!(s.value["points"],json!([[2.,3.]]));}
    #[test]fn configuration_validation_is_atomic(){let mut s=session();let before=s.value.clone();assert!(configure(&mut s.value,&json!({"view":{"low":2.,"high":1.}})).is_err());assert_eq!(s.value,before);configure(&mut s.value,&json!({"view":{"limits":"zscale","invert":true},"prompt":"Select comparison stars"})).unwrap();assert_eq!(s.value["config_revision"],2);assert!(configure(&mut s.value,&json!({"view":{"shell":"unsupported"}})).is_err());}
    #[test]fn requested_limits_are_not_reported_as_actual_until_the_viewer_applies_them(){let mut s=session();configure(&mut s.value,&json!({"view":{"low":1.,"high":5.}})).unwrap();assert_eq!(s.value["view"],json!({}));let mut value=change(&s);value["view"]=json!({"low":1.,"high":5.});value["applied_config_revision"]=json!(2);update(&mut s,&value).unwrap();assert_eq!(s.value["view_revision"],2);configure(&mut s.value,&json!({"view":{"limits":"zscale"}})).unwrap();assert!(s.value["requested_view"].get("low").is_none());assert_eq!(s.value["view"]["low"],1.);assert_eq!(s.value["view_revision"],2);}
    #[test]fn waiting_sessions_expire_without_background_processes(){let mut store=Store::default();let mut s=session();s.deadline=Instant::now()-Duration::from_millis(1);store.sessions.push_back(s);store.expire();assert_eq!(store.sessions[0].value["status"],"expired");assert!(store.sessions[0].finished.is_some());}
    #[test]fn initial_presentation_is_validated_before_session_publication(){let mut store=Store::default();let source=session().value["source"].clone();assert!(store.add("fixture.fits".into(),source.clone(),4,60,&json!({"view":{"low":5.,"high":1.}})).is_err());assert!(store.sessions.is_empty());let value=store.add("fixture.fits".into(),source,4,60,&json!({"view":{"limits":"zscale","invert":true},"prompt":"Choose stars"})).unwrap();assert_eq!(value["requested_view"]["invert"],true);assert_eq!(value["prompt"],"Choose stars");assert_eq!(value["config_revision"],1);assert_eq!(value["view"],json!({}));}
}
