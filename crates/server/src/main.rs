#![cfg_attr(all(windows,feature="desktop"), windows_subsystem = "windows")]

#[cfg(all(windows,feature="desktop"))]
mod desktop_windows;
#[cfg(all(not(windows),feature="desktop"))]
mod desktop_unix;
mod science;
#[cfg(feature="pro")]
mod download_save;
mod science_sessions;
mod science_process;
mod supervisor;
mod measurement;
mod remote_data;
mod environment;
mod remote_broker;
mod remote_sources;
#[cfg(feature="desktop")]
mod remote_native;
use measurement::{measure_json,header_json};

use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex,atomic::{AtomicUsize,Ordering}},
    time::{Duration, SystemTime},
};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "FitsViewer: fitsrs-powered desktop and JupyterLab FITS viewer"
)]
struct Args {
    file: Option<PathBuf>,
    #[arg(long,help="Print compiled module flags as JSON without opening a viewer")]
    build_info:bool,
    #[arg(long,help="Inspect FITS metadata and physical-value statistics as JSON without a display")]
    inspect_json:bool,
    #[arg(long,help="Read one original FITS header as JSON, including an empty primary HDU")]
    header_json:bool,
    #[arg(long,help="Write the typed binary dataset packet to stdout without a display")]
    packet:bool,
    #[arg(long,help="Read bounded aperture JSON from stdin and return measurements without a display")]
    measure_json:bool,
    #[arg(long,help="Validate a view JSON document read from stdin")]
    validate_view:bool,
    #[arg(long)]
    x_column:Option<usize>,
    #[arg(long)]
    y_column:Option<usize>,
    #[arg(long)]
    error_column:Option<usize>,
    #[arg(long,conflicts_with="error_column")]
    error_hdu:Option<usize>,
    #[arg(long,default_value="auto",value_parser=["auto","sigma","variance","ivar"])]
    error_kind:String,
    #[arg(long,conflicts_with_all=["error_column","error_hdu"])]
    no_error:bool,
    #[cfg(feature="desktop")]
    #[arg(long)]
    native: bool,
    #[arg(
        long,
        help = "Use the browser viewer instead of the Windows desktop window"
    )]
    browser: bool,
    #[cfg(all(windows,feature="desktop"))]
    #[arg(long, value_parser = ["yes", "no", "status", "register", "remove", "settings"])]
    desktop_default: Option<String>,
    #[arg(long)]
    serve: bool,
    #[arg(long,requires="serve",conflicts_with="jupyter",help="Use the isolated explicit-file fver broker")]
    fver_broker:bool,
    #[cfg(feature="desktop")]
    #[arg(long,requires="native",conflicts_with_all=["file","serve","session"],help="Read a bounded remote connection capability from stdin")]
    connect_stdin:bool,
    #[arg(long, default_value_t = 8765)]
    port: u16,
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long)]
    web_dir: Option<PathBuf>,
    #[arg(long)]
    hdu: Option<usize>,
    #[arg(long)]
    jupyter: bool,
    #[arg(long,requires="jupyter",help="Exit when this supervising Jupyter process exits")]
    supervisor_pid:Option<u32>,
    #[arg(long)]
    no_browser: bool,
    #[arg(long)]
    public_url: Option<String>,
    #[arg(long,help="Existing Python/Conda interpreter for optional scientific tools")]
    python: Option<PathBuf>,
    #[arg(long,help="Restore a schema-1 viewer session JSON (native or browser)")]
    session:Option<PathBuf>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Runtime {
    port: u16,
    pid: u32,
    root: String,
    token: String,
    public_url: String,
    epoch: String,
    #[serde(default)]fver:bool,
}
#[derive(Clone, Serialize)]
struct OpenRequest {
    seq: u64,
    path: String,
    hdu: Option<usize>,
    session:Option<serde_json::Value>,
}
struct Queue {
    seq: u64,
    items: VecDeque<OpenRequest>,
}
#[derive(Clone)]
struct AppState {
    runtime: Runtime,
    roots: Vec<PathBuf>,
    queue: Arc<Mutex<Queue>>,
    science: Arc<science::Bridge>,
    images: Arc<remote_data::Store>,
    broker:Arc<Mutex<remote_broker::Broker>>,
}
#[derive(Deserialize)]
struct ScienceInput {operation:String,#[serde(default)]params:serde_json::Value}
#[derive(Deserialize)]
struct ImageQuery {
    path: String,
    hdu: Option<usize>,
    token: Option<String>,
    xcol: Option<usize>,
    ycol: Option<usize>,
    ecol: Option<usize>,
    ehdu: Option<usize>,
    ekind: Option<String>,
    no_error:Option<bool>,
    session:Option<String>,
    generation:Option<u64>,
}
impl ImageQuery {
    fn options(&self)->Result<fitsviewer_core::LoadOptions,String>{
        let options=fitsviewer_core::LoadOptions{hdu:self.hdu,x_column:self.xcol,y_column:self.ycol,error_column:self.ecol,error_hdu:self.ehdu,error_kind:fitsviewer_core::ErrorKind::parse(self.ekind.as_deref().unwrap_or("auto")).ok_or("Unknown uncertainty convention")?,ignore_errors:self.no_error.unwrap_or(false)};
        options.validate()?;Ok(options)
    }
}
#[derive(Deserialize)]
struct TileQuery {
    version:String,
    #[serde(default)]level:u8,
    #[serde(default)]x:usize,
    #[serde(default)]y:usize,
}
// Extract both flat query views directly from the URI. serde(flatten) buffers
// query values as strings, bypassing serde_urlencoded's numeric conversion.
// 两个 Query 各自读 URI，避免 flatten 将 HDU 等数值留成字符串而整图/图块均报 400。
#[cfg(test)]
mod image_query_tests {
    use super::*;
    #[test]
    fn selected_hdu_and_tile_coordinates_parse_from_real_query_strings() {
        let uri="/api/image/tile?path=%E6%98%9F%E5%9B%A2.fits&hdu=1&version=revision-1&level=2&x=3&y=4&xcol=0&ecol=2&ehdu=3&ekind=ivar&no_error=false&session=view-a&generation=7".parse().unwrap();
        let Query(image)=Query::<ImageQuery>::try_from_uri(&uri).unwrap();
        let Query(tile)=Query::<TileQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(image.path,"星团.fits");assert_eq!(image.hdu,Some(1));
        assert_eq!(image.xcol,Some(0));assert_eq!(image.ecol,Some(2));assert_eq!(image.ehdu,Some(3));
        assert_eq!(image.no_error,Some(false));assert_eq!(image.generation,Some(7));
        assert_eq!((tile.level,tile.x,tile.y),(2,3,4));assert_eq!(tile.version,"revision-1");
        // Both uncertainty sources parse, then the scientific option validator
        // rejects their ambiguous combination rather than the URI extractor.
        assert!(image.options().is_err());
    }
    #[test]
    fn full_defaults_and_invalid_numeric_queries_have_explicit_outcomes() {
        let uri="/api/image/full?path=small.fits&hdu=0&version=revision-1".parse().unwrap();
        let Query(image)=Query::<ImageQuery>::try_from_uri(&uri).unwrap();
        let Query(tile)=Query::<TileQuery>::try_from_uri(&uri).unwrap();
        assert_eq!(image.hdu,Some(0));assert_eq!((tile.level,tile.x,tile.y),(0,0,0));assert!(image.options().is_ok());
        for query in ["hdu=-1","hdu=broken"] {
            let uri=format!("/api/image/pixel?path=a.fits&version=v&{query}").parse().unwrap();
            assert!(Query::<ImageQuery>::try_from_uri(&uri).is_err());
        }
        for query in ["level=256","x=-1","y=broken"] {
            let uri=format!("/api/image/tile?path=a.fits&version=v&{query}").parse().unwrap();
            assert!(Query::<TileQuery>::try_from_uri(&uri).is_err());
        }
    }
}
#[derive(Deserialize)]
struct RequestQuery {
    after: Option<u64>,
    token: Option<String>,
}
#[derive(Deserialize, Serialize)]
struct OpenInput {
    path: String,
    hdu: Option<usize>,
    #[serde(default)]session:Option<serde_json::Value>,
}
#[derive(Deserialize)]
struct SessionQuery {seq:u64,token:Option<String>}
#[derive(Serialize)]
struct Requests {
    epoch: String,
    latest: u64,
    requests: Vec<OpenRequest>,
}

fn project() -> PathBuf {
    if let Some(home) = std::env::var_os("FITSVIEWER_HOME") {
        return PathBuf::from(home);
    }
    // Discover installed or source resources without embedding the build machine's path.
    // 从运行目录查找资源，避免将构建机器的绝对路径写进发行程序。
    if let Ok(executable) = std::env::current_exe() {
        for candidate in executable.ancestors().skip(1) {
            if candidate.join("web/index.html").is_file() {
                return candidate.to_path_buf();
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
fn runtime_path() -> PathBuf {
    if let Some(directory) = std::env::var_os("FITSVIEWER_RUNTIME_DIR") {
        return PathBuf::from(directory).join("server.json");
    }
    project().join(".runtime/server.json")
}
fn authorize(
    state: &AppState,
    headers: &HeaderMap,
    token: Option<&str>,
) -> Result<(), (StatusCode, String)> {
    if state.runtime.fver {
        remote_broker::verify_transport(state,headers)?;
        let key=remote_broker::credential(headers).unwrap_or("");
        if !headers.contains_key("origin") && !key.is_empty() && key==state.runtime.token{return Ok(());}
        if state.broker.lock().unwrap().scope(key).is_some(){return Ok(());}
        return Err((StatusCode::UNAUTHORIZED,"A live file capability in the request header is required".into()));
    }
    if state.runtime.token.is_empty()
        || token == Some(state.runtime.token.as_str())
        || headers
            .get("x-fitsviewer-token")
            .and_then(|v| v.to_str().ok())
            == Some(state.runtime.token.as_str())
    {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            "FitsViewer session token required".into(),
        ))
    }
}
fn scoped<'a>(state:&AppState,headers:&'a HeaderMap)->Option<&'a str>{
    remote_broker::credential(headers).filter(|token|state.runtime.fver && *token!=state.runtime.token)
}
fn resolve_for(state:&AppState,headers:&HeaderMap,path:&str)->Result<PathBuf,(StatusCode,String)>{
    if let Some(token)=scoped(state,headers){let checked=state.broker.lock().unwrap().resolve(token,path)?;if !path.starts_with("artifact:"){return Ok(checked);}}
    resolve(state,path)
}
fn private_only(state:&AppState)->Result<(),(StatusCode,String)>{if state.runtime.fver{Err((StatusCode::FORBIDDEN,"Use the isolated fver control and target endpoints".into()))}else{Ok(())}}
fn claim_image(state:&AppState,headers:&HeaderMap,q:&ImageQuery)->Result<(),(StatusCode,String)>{
    if let (Some(id),Some(session))=(q.path.strip_prefix("artifact:"),q.session.as_deref()){
        let owner=if let Some(token)=scoped(state,headers){state.broker.lock().unwrap().owner(token,session,q.generation)?}else{session.into()};
        state.science.claim(id,&owner,q.generation).map_err(|error|(StatusCode::CONFLICT,error))?;
    }Ok(())
}
fn image_lease(state:&AppState,headers:&HeaderMap,path:&str)->Result<Option<remote_broker::ReadLease>,(StatusCode,String)>{if let Some(token)=scoped(state,headers){state.broker.lock().unwrap().image_lease(token,path)}else{Ok(None)}}
fn stable_source(lease:&Option<remote_broker::ReadLease>)->Result<Option<Arc<remote_sources::Source>>,String>{lease.as_ref().map(remote_broker::ReadLease::prepare).transpose()}
fn replace_names(value:&mut serde_json::Value,names:&[(String,String)]){match value{serde_json::Value::String(path)=>{if let Some((_,to))=names.iter().find(|(from,_)|from==path){*path=to.clone();}},serde_json::Value::Object(map)=>for value in map.values_mut(){replace_names(value,names);},serde_json::Value::Array(values)=>for value in values{replace_names(value,names);},_=>{}}}
async fn compensate_scoped(state:&AppState,lifecycle:&serde_json::Value,starting:bool,value:Option<&serde_json::Value>){
    let mut tags=serde_json::json!({});for key in ["_viewer_session","_document_generation","_role_epoch","_science_role","_gaia_generation"]{if let Some(value)=lifecycle.get(key){tags[key]=value.clone();}}
    let job=starting.then(||value.and_then(|v|v["job_id"].as_str()).map(str::to_owned)).flatten();let bridge=state.science.clone();
    let _=tokio::task::spawn_blocking(move||{if let Some(job)=job{let mut cancel=tags.clone();cancel["job_id"]=job.into();if bridge.call("job_cancel",cancel).is_ok(){return;}}let _=bridge.call("release_document",tags);}).await;
}
fn artifact_result(value:&serde_json::Value)->bool{match value{serde_json::Value::Object(map)=>map.contains_key("artifact_id") || map.values().any(artifact_result),serde_json::Value::Array(values)=>values.iter().any(artifact_result),_=>false}}
static SCOPED_IO:AtomicUsize=AtomicUsize::new(0);
struct ScopedIo;
impl Drop for ScopedIo{fn drop(&mut self){SCOPED_IO.fetch_sub(1,Ordering::AcqRel);}}
fn scoped_io(state:&AppState,headers:&HeaderMap)->Result<Option<ScopedIo>,(StatusCode,String)>{if scoped(state,headers).is_none(){return Ok(None);}SCOPED_IO.fetch_update(Ordering::AcqRel,Ordering::Acquire,|n|(n<16).then_some(n+1)).map_err(|_|(StatusCode::TOO_MANY_REQUESTS,"Remote source/image queue has 16 active reads; retry shortly".into()))?;Ok(Some(ScopedIo))}
fn resolve(state: &AppState, path: &str) -> Result<PathBuf, (StatusCode, String)> {
    if let Some(id)=path.strip_prefix("artifact:"){return state.science.artifact(id).map_err(|e|(StatusCode::NOT_FOUND,e));}
    let p = Path::new(path);
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err((
            StatusCode::FORBIDDEN,
            "Parent-directory traversal is not allowed".into(),
        ));
    }
    let candidate = if p.is_absolute() {
        p.to_path_buf()
    } else {
        Path::new(&state.runtime.root).join(p)
    };
    let resolved = match candidate.canonicalize() {
        Ok(resolved) => resolved,
        Err(error) => {
            let unquoted=fitsviewer_core::pasted_path(path);
            if unquoted!=path {return resolve(state,unquoted);}
            return Err((StatusCode::NOT_FOUND,error.to_string()));
        }
    };
    if !state.roots.iter().any(|root| resolved.starts_with(root)) || !resolved.is_file() {
        return Err((
            StatusCode::FORBIDDEN,
            "File is outside the configured viewer roots".into(),
        ));
    }
    Ok(resolved)
}
async fn image(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ImageQuery>,
) -> Response {
    if let Err(error) = authorize(&state, &headers, q.token.as_deref()) {
        return error.into_response();
    }
    if scoped(&state,&headers).is_some(){return (StatusCode::FORBIDDEN,"Remote file capabilities use metadata/full/tile/pixel; the legacy unbounded image route is disabled").into_response();}
    let path = match resolve_for(&state,&headers, &q.path) {
        Ok(path) => path,
        Err(error) => return error.into_response(),
    };
    if let Err(error)=claim_image(&state,&headers,&q){return error.into_response();}
    let result = tokio::task::spawn_blocking(move || {
        let options=fitsviewer_core::LoadOptions {hdu:q.hdu,x_column:q.xcol,y_column:q.ycol,error_column:q.ecol,error_hdu:q.ehdu,error_kind:fitsviewer_core::ErrorKind::parse(q.ekind.as_deref().unwrap_or("auto")).ok_or("Unknown uncertainty convention")?,ignore_errors:q.no_error.unwrap_or(false)};
        let mut image = fitsviewer_core::load_file_with_options(&path, options)?;
        image.info.path = q.path;
        image.to_packet()
    })
    .await;
    match result {
        Ok(Ok(bytes)) => (
            [
                ("content-type", "application/octet-stream"),
                ("cache-control", "no-store"),
            ],
            bytes,
        )
            .into_response(),
        Ok(Err(error)) => (StatusCode::UNPROCESSABLE_ENTITY, error).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}
async fn image_manifest(State(state):State<AppState>,headers:HeaderMap,Query(q):Query<ImageQuery>)->Response{
    if let Err(error)=authorize(&state,&headers,q.token.as_deref()){return error.into_response();}
    let _permit=match scoped_io(&state,&headers){Ok(permit)=>permit,Err(error)=>return error.into_response()};
    let path=match resolve_for(&state,&headers,&q.path){Ok(path)=>path,Err(error)=>return error.into_response()};
    if let Err(error)=claim_image(&state,&headers,&q){return error.into_response();}
    let options=match q.options(){Ok(options)=>options,Err(error)=>return (StatusCode::BAD_REQUEST,error).into_response()};
    let lease=match image_lease(&state,&headers,&q.path){Ok(lease)=>lease,Err(error)=>return error.into_response()};
    let images=state.images.clone();
    let result=tokio::task::spawn_blocking(move||{let source=stable_source(&lease)?;let path=source.as_ref().map_or(path.as_path(),|s|s.path());let value=images.manifest(path,&q.path,options)?;if let Some(source)=source{source.verify()?;}Ok::<_,String>(value)}).await;
    if state.runtime.fver{if let Err(error)=authorize(&state,&headers,None){return error.into_response();}}
    match result{
        Ok(Ok(value))=>Json(value).into_response(),Ok(Err(error))=>(if error.contains("source_changed"){StatusCode::CONFLICT}else{StatusCode::UNPROCESSABLE_ENTITY},error).into_response(),Err(_)=>(StatusCode::INTERNAL_SERVER_ERROR,"Image metadata task failed").into_response(),
    }
}
async fn image_full(State(state):State<AppState>,headers:HeaderMap,Query(image):Query<ImageQuery>,Query(q):Query<TileQuery>)->Response{
    image_piece(state,headers,image,q,"full").await
}
async fn image_tile(State(state):State<AppState>,headers:HeaderMap,Query(image):Query<ImageQuery>,Query(q):Query<TileQuery>)->Response{
    image_piece(state,headers,image,q,"tile").await
}
async fn image_pixel(State(state):State<AppState>,headers:HeaderMap,Query(image):Query<ImageQuery>,Query(q):Query<TileQuery>)->Response{
    image_piece(state,headers,image,q,"pixel").await
}
async fn image_piece(state:AppState,headers:HeaderMap,image:ImageQuery,q:TileQuery,kind:&'static str)->Response{
    if let Err(error)=authorize(&state,&headers,image.token.as_deref()){return error.into_response();}
    let _permit=match scoped_io(&state,&headers){Ok(permit)=>permit,Err(error)=>return error.into_response()};
    let path=match resolve_for(&state,&headers,&image.path){Ok(path)=>path,Err(error)=>return error.into_response()};
    if let Err(error)=claim_image(&state,&headers,&image){return error.into_response();}
    let options=match image.options(){Ok(options)=>options,Err(error)=>return (StatusCode::BAD_REQUEST,error).into_response()};
    let lease=match image_lease(&state,&headers,&image.path){Ok(lease)=>lease,Err(error)=>return error.into_response()};
    let images=state.images.clone();
    let result=tokio::task::spawn_blocking(move||{
        let source=stable_source(&lease)?;let path=source.as_ref().map_or(path.as_path(),|s|s.path());
        let value=if kind=="pixel"{images.pixel(path,&image.path,options,&q.version,q.x,q.y).and_then(|v|serde_json::to_vec(&v).map_err(|e|e.to_string()))}
        else if kind=="full"{images.full(path,&image.path,options,&q.version)}
        else{images.tile(path,&image.path,options,&q.version,q.level,q.x,q.y)}?;if let Some(source)=source{source.verify()?;}Ok::<_,String>(value)
    }).await;
    if state.runtime.fver{if let Err(error)=authorize(&state,&headers,None){return error.into_response();}}
    match result{
        Ok(Ok(bytes))=>([("content-type",if kind=="pixel"{"application/json"}else{"application/octet-stream"}),("cache-control","no-store")],bytes).into_response(),
        Ok(Err(error))=>(if error.contains("changed") || error.contains("revision expired"){StatusCode::CONFLICT}else{StatusCode::UNPROCESSABLE_ENTITY},error).into_response(),
        Err(_)=>(StatusCode::INTERNAL_SERVER_ERROR,"Image tile task failed").into_response(),
    }
}
async fn scientific(State(state):State<AppState>,headers:HeaderMap,Json(mut input):Json<ScienceInput>)->Response{
    if let Err(error)=authorize(&state,&headers,None){return error.into_response();}
    fn validate(state:&AppState,headers:&HeaderMap,value:&mut serde_json::Value)->Result<(),(StatusCode,String)>{
        match value {
            serde_json::Value::Object(object)=>{
                for (key,value) in object {
                    if key=="output"{return Err((StatusCode::BAD_REQUEST,"Browser exports use the private session cache; choose a download destination in the browser".into()));}
                    if key=="uploads" {
                        let uploads=value.as_object_mut().ok_or((StatusCode::BAD_REQUEST,"TAP uploads must be a name-to-path object".into()))?;
                        for path in uploads.values_mut(){let input=path.as_str().ok_or((StatusCode::BAD_REQUEST,"TAP upload paths must be strings".into()))?;*path=serde_json::Value::String(resolve_for(state,headers,input)?.to_string_lossy().into_owned());}
                    }else if key.ends_with("path") || key=="catalogue" {
                        if let Some(path)=value.as_str(){if key=="mask_path" && path.trim().is_empty(){continue;}*value=serde_json::Value::String(resolve_for(state,headers,path)?.to_string_lossy().into_owned());}else{validate(state,headers,value)?;}
                    }else{validate(state,headers,value)?;}
                }
            },
            serde_json::Value::Array(array)=>for value in array {validate(state,headers,value)?;},
            _=>{},
        }Ok(())
    }
    #[cfg(not(feature="pro"))] if let Err(error)=mini_science(&input.operation,&input.params){return (StatusCode::FORBIDDEN,error).into_response();}
    let capability=scoped(&state,&headers).map(str::to_owned);
    if let Some(token)=&capability{let mut broker=state.broker.lock().unwrap();broker.refresh(token,&state.science);if let Err(error)=broker.science(token,&input.operation,&mut input.params){return error.into_response();}}
    let lifecycle_only=remote_broker::lifecycle_only(&input.operation);
    let _permit=if lifecycle_only{None}else{match scoped_io(&state,&headers){Ok(permit)=>permit,Err(error)=>return error.into_response()}};
    if !(capability.is_some() && lifecycle_only){if let Err(error)=validate(&state,&headers,&mut input.params){return error.into_response();}}
    let leases=if lifecycle_only{Vec::new()}else{capability.as_ref().map(|token|state.broker.lock().unwrap().leases(token,&input.params)).unwrap_or_default()};
    let work_leases=leases.clone();let starting=input.operation=="job_start";let lifecycle=capability.as_ref().map(|token|state.broker.lock().unwrap().lifecycle(token,&input.params)).unwrap_or_else(||input.params.clone());
    let may_artifact=starting || ["job_status","export","rgb","math_preview","reproject","gaia_solve"].contains(&input.operation.as_str());
    let bridge=state.science.clone();
    let result=tokio::task::spawn_blocking(move||{let sources=work_leases.iter().map(remote_broker::ReadLease::prepare).collect::<Result<Vec<_>,_>>().map_err(|e|(e,false))?;let names=sources.iter().map(|s|(s.logical().to_string_lossy().into_owned(),s.path().to_string_lossy().into_owned())).collect::<Vec<_>>();replace_names(&mut input.params,&names);let mut value=match input.operation.as_str(){"measure"=>measure_json(&input.params),"header"=>header_json(&input.params),_=>bridge.call(&input.operation,input.params)}.map_err(|e|(e,true))?;replace_names(&mut value,&names.into_iter().map(|(from,to)|(to,from)).collect::<Vec<_>>());let verification=sources.iter().try_for_each(|source|source.verify());Ok::<_,(String,bool)>((value,verification))}).await;
    match result {
        Ok(Ok((mut value,verification)))=>{if let Err(error)=authorize(&state,&headers,None){if capability.is_some() && (may_artifact || artifact_result(&value)){compensate_scoped(&state,&lifecycle,starting,Some(&value)).await;}return error.into_response();}
          if let Err(error)=verification{if capability.is_some() && (may_artifact || artifact_result(&value)){compensate_scoped(&state,&lifecycle,starting,Some(&value)).await;}return (StatusCode::CONFLICT,error).into_response();}
          if let Some(token)=&capability{
            let result=state.broker.lock().unwrap().record(token,&value,&state.science,starting,&leases,&lifecycle);
            if let Err(error)=result{if may_artifact || artifact_result(&value){compensate_scoped(&state,&lifecycle,starting,Some(&value)).await;}return (StatusCode::TOO_MANY_REQUESTS,error).into_response();}
            state.broker.lock().unwrap().logicalize(token,&mut value);
        }Json(value).into_response()},
        Ok(Err((error,called)))=>{if called && capability.is_some() && may_artifact{compensate_scoped(&state,&lifecycle,starting,None).await;}(if error.contains("source_changed"){StatusCode::CONFLICT}else{StatusCode::UNPROCESSABLE_ENTITY},error).into_response()},
        Err(_)=>{if capability.is_some() && may_artifact{compensate_scoped(&state,&lifecycle,starting,None).await;}(StatusCode::INTERNAL_SERVER_ERROR,"Scientific worker failed").into_response()},
    }
}
#[derive(Deserialize)]
struct ArtifactQuery {id:String,token:Option<String>}
async fn artifact(State(state):State<AppState>,headers:HeaderMap,Query(q):Query<ArtifactQuery>)->Response{
    if let Err(error)=authorize(&state,&headers,q.token.as_deref()){return error.into_response();}
    if let Err(error)=resolve_for(&state,&headers,&format!("artifact:{}",q.id)){return error.into_response();}
    let bridge=state.science.clone();
    match tokio::task::spawn_blocking(move||bridge.artifact(&q.id).and_then(|path|{let extension=path.extension().and_then(|e|e.to_str()).filter(|e|e.len()<=8 && e.bytes().all(|c|c.is_ascii_alphanumeric())).unwrap_or("bin");let id=q.id.chars().filter(|c|c.is_ascii_alphanumeric() || *c=='-').take(64).collect::<String>();let filename=format!("attachment; filename=\"fitsviewer-{id}.{extension}\"");std::fs::read(path).map(|bytes|(bytes,filename)).map_err(|e|e.to_string())})).await{
        Ok(Ok((bytes,filename)))=> ([("content-type","application/octet-stream"),("cache-control","no-store"),("content-disposition",filename.as_str()),("referrer-policy","no-referrer")],bytes).into_response(),
        _=>(StatusCode::NOT_FOUND,"Session artifact is unavailable").into_response(),
    }
}
#[cfg(feature="pro")]
#[derive(Deserialize)]
struct FileQuery{path:String,token:Option<String>}
#[cfg(feature="pro")]
async fn bridge_file(State(state):State<AppState>,headers:HeaderMap,Query(q):Query<FileQuery>)->Response{
    if let Err(error)=authorize(&state,&headers,q.token.as_deref()){return error.into_response();}
    if scoped(&state,&headers).is_some(){return (StatusCode::FORBIDDEN,"Raw file transfer needs a separate explicit download capability; use display metadata/full/tile/pixel").into_response();}
    let path=match resolve_for(&state,&headers,&q.path){Ok(path)=>path,Err(error)=>return error.into_response()};
    if !path.metadata().is_ok_and(|m|m.len()<=64*1024*1024){return (StatusCode::PAYLOAD_TOO_LARGE,"Bridge transfer limit is 64 MiB").into_response();}
    match std::fs::read(path){Ok(bytes)=>([("content-type","application/octet-stream"),("cache-control","no-store")],bytes).into_response(),Err(_)=>(StatusCode::NOT_FOUND,"File unavailable").into_response()}
}
#[cfg(feature="pro")]
#[derive(Deserialize)]
struct UploadQuery{#[serde(default)]suffix:String,#[serde(default)]owner:String}
#[cfg(feature="pro")]
async fn bridge_upload(State(state):State<AppState>,headers:HeaderMap,Query(q):Query<UploadQuery>,body:axum::body::Bytes)->Response{
    if let Err(error)=authorize(&state,&headers,None){return error.into_response();}
    if let Err(error)=private_only(&state){return error.into_response();}
    match state.science.import(&body,&q.suffix,&q.owner){Ok(value)=>Json(value).into_response(),Err(error)=>(StatusCode::UNPROCESSABLE_ENTITY,error).into_response()}
}
async fn requests(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RequestQuery>,
) -> Response {
    if let Err(error)=private_only(&state){return error.into_response();}
    if let Err(error) = authorize(&state, &headers, q.token.as_deref()) {
        return error.into_response();
    }
    let queue = state.queue.lock().unwrap();
    Json(Requests {
        epoch: state.runtime.epoch.clone(),
        latest: queue.seq,
        requests: queue
            .items
            .iter()
            .filter(|r| r.seq > q.after.unwrap_or(0))
            .cloned()
            .collect(),
    })
    .into_response()
}
async fn open(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(q): Json<OpenInput>,
) -> Response {
    if let Err(error)=private_only(&state){return error.into_response();}
    if let Err(error) = authorize(&state, &headers, None) {
        return error.into_response();
    }
    #[cfg(not(feature="pro"))] if let Some(session)=&q.session{if let Err(error)=mini_session(session){return (StatusCode::FORBIDDEN,error).into_response();}}
    if let Some(session)=&q.session{
        let Some(views)=session["views"].as_array().filter(|v|!v.is_empty() && v.len()<=32)else{return (StatusCode::BAD_REQUEST,"Session needs one to 32 views").into_response();};
        if session.to_string().len()>1024*1024{return (StatusCode::BAD_REQUEST,"Session exceeds 1 MiB").into_response();}
        for entry in views{
            let Some(path)=entry["path"].as_str()else{return (StatusCode::BAD_REQUEST,"Session view requires path").into_response();};
            if let Err(error)=resolve(&state,path){return error.into_response();}
            if let Err(error)=fitsviewer_core::view::View::parse(&entry["view"].to_string()){return (StatusCode::BAD_REQUEST,error).into_response();}
        }
    }
    let resolved = match resolve(&state, &q.path) {
        Ok(path) => path,
        Err(error) => return error.into_response(),
    };
    let virtual_path = if Path::new(&q.path).is_absolute() {
        let root = Path::new(&state.runtime.root);
        Path::new(&q.path)
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| {
                for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
                    if entry.file_type().is_ok_and(|t| t.is_symlink()) {
                        if let Ok(target) = entry.path().canonicalize() {
                            if let Ok(relative) = resolved.strip_prefix(target) {
                                return Path::new(&entry.file_name())
                                    .join(relative)
                                    .to_string_lossy()
                                    .into_owned();
                            }
                        }
                    }
                }
                resolved.to_string_lossy().into_owned()
            })
    } else {
        q.path
    };
    let virtual_path = virtual_path.replace(std::path::MAIN_SEPARATOR, "/");
    let mut queue = state.queue.lock().unwrap();
    queue.seq += 1;
    let request = OpenRequest {
        seq: queue.seq,
        path: virtual_path.clone(),
        hdu: q.hdu,
        session:q.session.clone(),
    };
    queue.items.push_back(request);
    if queue.items.len() > 32 {
        queue.items.pop_front();
    }
    let mut url = format!(
        "{}?path={}",
        state.runtime.public_url,
        urlencoding::encode(&virtual_path)
    );
    if let Some(hdu) = q.hdu {
        url.push_str(&format!("&hdu={hdu}"));
    }
    if q.session.is_some(){url.push_str(&format!("&session_seq={}",queue.seq));}
    if !state.runtime.token.is_empty() {
        url.push_str(&format!("&token={}", state.runtime.token));
    }
    Json(serde_json::json!({"url":url,"path":virtual_path,"seq":queue.seq})).into_response()
}
async fn session(State(state):State<AppState>,headers:HeaderMap,Query(q):Query<SessionQuery>)->Response{
    if let Err(error)=private_only(&state){return error.into_response();}
    if let Err(error)=authorize(&state,&headers,q.token.as_deref()){return error.into_response();}
    let value=state.queue.lock().unwrap().items.iter().find(|item|item.seq==q.seq).and_then(|item|item.session.clone());
    match value{
        Some(value)=>Json(value).into_response(),None=>(StatusCode::NOT_FOUND,"Session request expired; apply it again").into_response(),
    }
}
async fn health(State(state):State<AppState>,headers:HeaderMap) -> Json<serde_json::Value> {
    let mut value=serde_json::json!({"ok":true,"viewer":"fitsviewer","version":env!("CARGO_PKG_VERSION"),"fits_backend":"fitsrs 0.4.1","features":build_features(),"fver":state.runtime.fver,"epoch":state.runtime.epoch,"instance":state.broker.lock().unwrap().instance()});
    if authorize(&state,&headers,None).is_ok(){value["remote_image_cache"]=state.images.status();value["remote_source_capabilities"]=state.broker.lock().unwrap().source_status();}Json(value)
}

fn main() {
    if let Err(error) = run() {
        #[cfg(all(windows,feature="desktop"))]
        if !std::env::args().any(|a| ["--serve","--browser","--desktop-default","--inspect-json","--header-json","--packet","--measure-json","--validate-view","--build-info"].contains(&a.as_str()))
        {
            desktop_windows::message("FitsViewer", &error.to_string());
        }
        eprintln!("FitsViewer: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = Args::parse();
    #[cfg(feature="desktop")]
    if args.connect_stdin{return remote_native::run(args.web_dir.as_deref());}
    if args.build_info{println!("{}",serde_json::json!({"schema":1,"version":env!("CARGO_PKG_VERSION"),"features":build_features()}));return Ok(());}
    let restored_session=if let Some(path)=&args.session{
        let bytes=std::fs::read(path)?;if bytes.len()>1024*1024{return Err("Session exceeds 1 MiB".into());}
        let value:serde_json::Value=serde_json::from_slice(&bytes)?;
        #[cfg(not(feature="pro"))] mini_session(&value).map_err(std::io::Error::other)?;
        let views=value["views"].as_array().filter(|v|!v.is_empty() && v.len()<=32).ok_or("Session requires one to 32 views")?;
        let selected=value["selected"].as_u64().map(|i|i as usize).unwrap_or(0);let first=views.get(selected).ok_or("Invalid selected session view")?;
        args.file=Some(PathBuf::from(first["path"].as_str().ok_or("Session view requires path")?));args.hdu=first["hdu"].as_u64().map(|v|v as usize);if first["view"]["load_options"].is_object(){let options:fitsviewer_core::LoadOptions=serde_json::from_value(first["view"]["load_options"].clone())?;options.validate().map_err(std::io::Error::other)?;args.x_column=options.x_column;args.y_column=options.y_column;args.error_column=options.error_column;args.error_hdu=options.error_hdu;args.error_kind=options.error_kind.as_str().into();args.no_error=options.ignore_errors;}Some(value)
    }else{None};
    if args.measure_json || args.validate_view {
        use std::io::Read;
        let mut text=String::new();std::io::stdin().take(1024*1024+1).read_to_string(&mut text)?;
        if text.len()>1024*1024{return Err("JSON input exceeds 1 MiB".into());}
        let value=if args.validate_view {serde_json::to_value(fitsviewer_core::view::View::parse(&text).map_err(std::io::Error::other)?)?}else{let mut params:serde_json::Value=serde_json::from_str(&text)?;if let Some(path)=&args.file{params["path"]=serde_json::json!(path);}measure_json(&params).map_err(std::io::Error::other)?};
        println!("{value}");return Ok(());
    }
    if args.header_json{let path=args.file.as_ref().ok_or("Pass a FITS file to inspect its header")?;println!("{}",header_json(&serde_json::json!({"path":path,"hdu":args.hdu.unwrap_or(0)})).map_err(std::io::Error::other)?);return Ok(());}
    if args.inspect_json || args.packet {
        if args.packet && std::env::var_os("FITSVIEWER_OWNED_PACKET").is_some(){science_process::guard_packet_parent().map_err(std::io::Error::other)?;}
        let path=args.file.as_ref().ok_or("Pass a dataset to inspect")?;
        let image=fitsviewer_core::load_file_with_options(path,fitsviewer_core::LoadOptions{hdu:args.hdu,x_column:args.x_column,y_column:args.y_column,error_column:args.error_column,error_hdu:args.error_hdu,error_kind:fitsviewer_core::ErrorKind::parse(&args.error_kind).unwrap(),ignore_errors:args.no_error}).map_err(std::io::Error::other)?;
        if args.packet {use std::io::Write;std::io::stdout().lock().write_all(&image.to_packet().map_err(std::io::Error::other)?)?;}
        else {let stats=fitsviewer_core::Statistics::new(&image);println!("{}",serde_json::json!({"schema":1,"dataset":image.info,"statistics":{"valid":stats.finite,"minimum":stats.min,"maximum":stats.max},"scientific_values":"BSCALE/BZERO applied once; BLANK/NaN excluded","wcs_header_present":image.info.header.contains("CTYPE1") || image.info.header.contains("PLTRAH"),"wcs_transform_validated":false}));}
        return Ok(());
    }
    #[cfg(all(windows,feature="desktop"))]
    if let Some(action) = &args.desktop_default {
        println!("{}", desktop_windows::defaults(action)?);
        return Ok(());
    }
    #[cfg(feature="desktop")]
    if args.native || (cfg!(windows) && !args.serve && !args.browser) {
        #[cfg(all(windows,feature="desktop"))]
        desktop_windows::set_app_id();
        #[cfg(all(not(windows),not(target_os="macos")))]
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return Err("No display is available. Use the default SSH/browser mode, or start an X11/X2Go session before --native.".into());
        }
        let app = if let Some(path) = args.file {
            let image =
                fitsviewer_core::load_file_with_options(&path,fitsviewer_core::LoadOptions{hdu:args.hdu,x_column:args.x_column,y_column:args.y_column,error_column:args.error_column,error_hdu:args.error_hdu,error_kind:fitsviewer_core::ErrorKind::parse(&args.error_kind).unwrap(),ignore_errors:args.no_error}).map_err(std::io::Error::other)?;
            fitsviewer_web::FitsApp::from_image(image)
        } else {
            fitsviewer_web::FitsApp::new(String::new(), None, String::new(), None)
        };
        let app=if let Some(session)=restored_session{app.with_session(session).map_err(std::io::Error::other)?}else{app};
        #[cfg(all(windows,feature="desktop"))]
        let app = app.with_file_picker(desktop_windows::pick_file);
        #[cfg(not(windows))]
        let app=if desktop_unix::has_picker(){app.with_file_picker(desktop_unix::pick_file)}else{app};
        let bridge=Arc::new(science::Bridge::new(args.python.clone(),&project(),runtime_path().parent().unwrap_or(Path::new(".")).join("science")));
        science::Bridge::start_reaper(&bridge);
        let callback_bridge=bridge.clone();
        let app=app.with_science(Arc::new(move|operation,params|{if operation=="measure"{return measure_json(&params);}if operation=="header"{return header_json(&params);}let mut value=callback_bridge.call(operation,params)?;attach_native_artifacts(&callback_bridge,&mut value);Ok(value)}));
        let viewport = eframe::egui::ViewportBuilder::default()
            .with_inner_size([1150.0, 820.0])
            .with_app_id("fitsviewer")
            .with_title("FitsViewer");
        let viewport = viewport.with_icon(eframe::icon_data::from_png_bytes(include_bytes!(
            "../../../windows/app.png"
        ))?);
        let options = eframe::NativeOptions {
            viewport,
            ..Default::default()
        };
        let cjk_font=std::fs::read(args.web_dir.clone().unwrap_or_else(||project().join("web")).join("fonts/cjk.ttf"))?;
        eframe::run_native(
            if cfg!(target_os="linux") { "fitsviewer" } else { "FitsViewer" },
            options,
            Box::new(move |cc| {
                cc.egui_ctx.set_visuals(eframe::egui::Visuals::dark());
                fitsviewer_web::install_cjk_font(&cc.egui_ctx,cjk_font);
                Ok(Box::new(app))
            }),
        )?;
        bridge.shutdown();
        return Ok(());
    }
    if args.serve {
        return tokio::runtime::Runtime::new()?.block_on(serve(args));
    }
    let path = args
        .file
        .as_ref()
        .ok_or("Pass a FITS file, or use --serve")?
        .canonicalize()?;
    let mut active = std::fs::read(runtime_path())
        .ok()
        .and_then(|b| serde_json::from_slice::<Runtime>(&b).ok())
        .filter(|r| {
            ureq::get(&format!("http://127.0.0.1:{}/api/health", r.port))
                .timeout(Duration::from_secs(1))
                .call()
                .is_ok()
        });
    if active.is_none() {
        // Installed resources may be read-only. / 安装目录可为只读，运行记录放用户缓存。
        let logs = if std::env::var_os("FITSVIEWER_RUNTIME_DIR").is_some() {
            runtime_path().parent().unwrap().join("logs")
        } else {
            project().join("logs")
        };
        std::fs::create_dir_all(&logs)?;
        let root = args
            .root
            .clone()
            .unwrap_or_else(|| path.parent().unwrap().to_path_buf());
        let mut cmd = std::process::Command::new(std::env::current_exe()?);
        cmd.arg("--serve")
            .arg("--port")
            .arg(args.port.to_string())
            .arg("--root")
            .arg(root);
        if let Some(url) = &args.public_url {
            cmd.arg("--public-url").arg(url);
        }
        if let Some(web) = &args.web_dir {
            cmd.arg("--web-dir").arg(web);
        }
        if let Some(python)=&args.python {cmd.arg("--python").arg(python);}
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::fs::File::create(logs.join("standalone.log"))?)
            .stderr(
                std::fs::OpenOptions::new()
                    .append(true)
                    .open(logs.join("standalone.log"))?,
            );
        let mut child = cmd.spawn()?;
        for _ in 0..100 {
            std::thread::sleep(Duration::from_millis(100));
            if let Some(status) = child.try_wait()? {
                return Err(
                    format!("Viewer server exited: {status}. See logs/standalone.log").into(),
                );
            }
            if let Ok(bytes) = std::fs::read(runtime_path()) {
                if let Ok(r) = serde_json::from_slice::<Runtime>(&bytes) {
                    if r.pid == child.id()
                        && ureq::get(&format!("http://127.0.0.1:{}/api/health", r.port))
                            .timeout(Duration::from_secs(1))
                            .call()
                            .is_ok()
                    {
                        active = Some(r);
                        break;
                    }
                }
            }
        }
    }
    let active = active.ok_or("Viewer server did not become ready")?;
    let reply: serde_json::Value =
        ureq::post(&format!("http://127.0.0.1:{}/api/open", active.port))
            .set("X-Fitsviewer-Token", &active.token)
            .send_json(serde_json::json!({"path":path,"hdu":args.hdu,"session":restored_session}))?
            .into_json()?;
    let url = reply["url"].as_str().ok_or("Invalid open response")?;
    println!("{url}");
    if active.token.is_empty() {
        println!("Queued for the connected JupyterLab window. Use its Open in window button for a separate browser window.");
    }
    if !args.no_browser && (cfg!(target_os="macos") || std::env::var_os("DISPLAY").is_some()) {
        let _ = std::process::Command::new(if cfg!(target_os="macos"){"open"}else{"xdg-open"})
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    Ok(())
}

fn build_features()->serde_json::Value{
    serde_json::json!({"base":true,"pro":cfg!(feature="pro"),"desktop":cfg!(feature="desktop")})
}
#[cfg(feature="desktop")]
fn attach_native_artifacts(bridge:&science::Bridge,value:&mut serde_json::Value){match value {
    serde_json::Value::Object(object)=>{if let Some(id)=object.get("artifact_id").and_then(serde_json::Value::as_str){if let Ok(path)=bridge.artifact(id){object.insert("_path".into(),serde_json::json!(path));}}for child in object.values_mut(){attach_native_artifacts(bridge,child);}},
    serde_json::Value::Array(array)=>for child in array{attach_native_artifacts(bridge,child);},_=>{},
}}
async fn serve(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let root = args
        .root
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .canonicalize()?;
    let mut roots = vec![root.clone()];
    // Top-level links visible in Jupyter are explicit additional read roots.
    // Jupyter 根目录中的顶层符号链接作为明确的附加只读根目录。
    for entry in std::fs::read_dir(&root)?.flatten() {
        if entry.file_type()?.is_symlink() {
            if let Ok(target) = entry.path().canonicalize() {
                roots.push(target);
            }
        }
    }
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", args.port)).await?;
    let port = listener.local_addr()?.port();
    let runtime = Runtime {
        port,
        pid: std::process::id(),
        root: root.to_string_lossy().into_owned(),
        token: if args.jupyter {
            String::new()
        } else {
            uuid::Uuid::new_v4().simple().to_string()
        },
        public_url: args
            .public_url
            .unwrap_or_else(|| format!("http://127.0.0.1:{port}/")),
        epoch: format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_secs()
        ),
        fver:args.fver_broker,
    };
    let supervisor=args.supervisor_pid.map(supervisor::Parent::observe).transpose().map_err(std::io::Error::other)?;
    std::fs::create_dir_all(runtime_path().parent().unwrap())?;
    #[cfg(unix)]{use std::os::unix::fs::PermissionsExt;std::fs::set_permissions(runtime_path().parent().unwrap(),std::fs::Permissions::from_mode(0o700))?;}
    let mut runtime_file=std::fs::OpenOptions::new();runtime_file.write(true).create(true).truncate(true);
    #[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;runtime_file.mode(0o600);}
    {use std::io::Write;runtime_file.open(runtime_path())?.write_all(&serde_json::to_vec_pretty(&runtime)?)?;}
    let science=Arc::new(science::Bridge::new(args.python.clone(),&project(),runtime_path().parent().unwrap_or(Path::new(".")).join("science")));
    science.ensure_cache().map_err(std::io::Error::other)?;
    roots.push(science.cache_root().canonicalize()?);
    science::Bridge::start_reaper(&science);
    let state = AppState {
        broker:Arc::new(Mutex::new(remote_broker::Broker::new(args.fver_broker))),
        runtime,
        roots,
        queue: Arc::new(Mutex::new(Queue {
            seq: 0,
            items: VecDeque::new(),
        })),
        science:science.clone(),
        images:Arc::new(remote_data::Store::new(runtime_path().parent().unwrap_or(Path::new(".")).join("image-cache"))),
    };
    let web = args.web_dir.unwrap_or_else(|| project().join("web"));
    if args.fver_broker{let broker=state.broker.clone();let bridge=state.science.clone();tokio::spawn(async move{let mut tick=tokio::time::interval(Duration::from_secs(3));loop{tick.tick().await;let owners=broker.lock().unwrap().take_releases();if !owners.is_empty(){let bridge=bridge.clone();let _=tokio::task::spawn_blocking(move||for release in owners{let mut params=serde_json::json!({"_viewer_session":release.owner});let operation=if let Some(generation)=release.generation{params["_document_generation"]=generation.into();"release_document"}else{"release_view"};let _=bridge.call(operation,params);}).await;}}});}
    let app = Router::new()
        .route("/api/fver/register",post(remote_broker::register))
        .route("/api/fver/pair",post(remote_broker::pair_ticket))
        .route("/api/fver/bootstrap",post(remote_broker::bootstrap))
        .route("/api/fver/browser-link",post(remote_broker::browser_link))
        .route("/api/fver/artifact-link",post(remote_broker::artifact_link))
        .route("/api/fver/release-download",post(remote_broker::release_download))
        .route("/api/fver/events",get(remote_broker::events))
        .route("/api/fver/ack",post(remote_broker::acknowledge))
        .route("/api/fver/status",get(remote_broker::status))
        .route("/api/fver/cancel",post(remote_broker::cancel))
        .route("/api/health", get(health))
        .route("/api/image", get(image))
        .route("/api/image/metadata",get(image_manifest))
        .route("/api/image/full",get(image_full))
        .route("/api/image/tile",get(image_tile))
        .route("/api/image/pixel",get(image_pixel))
        .route("/api/open", post(open))
        .route("/api/session", get(session))
        .route("/api/requests", get(requests))
        .route("/api/science",post(scientific))
        .route("/api/artifact",get(artifact));
    #[cfg(feature="pro")] let app=app.route("/api/file",get(bridge_file)).route("/api/bridge/upload",post(bridge_upload).layer(axum::extract::DefaultBodyLimit::max(64*1024*1024)));
    let app=app
        .layer(axum::extract::DefaultBodyLimit::max(8*1024*1024))
        .fallback_service(tower_http::services::ServeDir::new(web))
        .with_state(state);
    println!("FitsViewer listening on loopback port {port}");
    let shutdown_science=science.clone();
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            let signal=async{
                #[cfg(unix)]{if let Ok(mut terminate)=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()){tokio::select!{_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}}else{let _=tokio::signal::ctrl_c().await;}}
                #[cfg(not(unix))]{let _=tokio::signal::ctrl_c().await;}
            };
            tokio::select!{_=signal=>{},_=supervisor::exited(supervisor)=>{eprintln!("Supervising Jupyter process exited; shutting down FitsViewer");}}
            // Interrupt an in-flight worker before waiting for HTTP requests to drain.
            // 先回收运行中的 worker，再等待 HTTP 请求结束，避免退出时等待超时。
            let _=tokio::task::spawn_blocking(move||shutdown_science.shutdown()).await;
        })
        .await?;
    science.shutdown();
    Ok(())
}

#[cfg(not(feature="pro"))]
fn mini_measure(params:&serde_json::Value)->Result<(),String>{
 let apertures=params["apertures"].as_array().ok_or("Measurement requires apertures")?;
 if apertures.len()!=1 || params.get("growth_radii").is_some(){return Err("Mini supports one circular aperture and its sky ring".into());}
 for a in apertures{if a["shape"]["kind"]!="circle" || a["background"].as_object().is_some_and(|_|a["background"]["kind"]!="ring") || a["exclude"].as_array().is_some_and(|v|!v.is_empty()){return Err("Advanced region geometry is not included in Mini".into());}}Ok(())
}
#[cfg(not(feature="pro"))]
fn mini_science(operation:&str,params:&serde_json::Value)->Result<(),String>{
 if operation=="measure"{return mini_measure(params);}
 if operation=="job_start" && !["gaia_solve","dependency_probe","dependency_install"].iter().any(|name|params["operation"]==*name){return Err("This job is not included in this Mini build".into());}
 if !["capabilities","header","wcs","gaia_solve","job_start","job_status","job_cancel","gaia_acquire","gaia_heartbeat","gaia_release","release_view","release_document","view_heartbeat","artifact_refs","release_pro","runtime_status","environment_info","environment_candidates","environment_select","dependency_probe","dependency_install","environment_busy"].contains(&operation){return Err("PRO is not included in this Mini build".into());}Ok(())
}
#[cfg(not(feature="pro"))]
fn mini_session(value:&serde_json::Value)->Result<(),String>{
 let views=value["views"].as_array().ok_or("Session requires views")?;
 if views.len()!=1 || value["mode"].as_str().is_some_and(|s|s!="Minimal") || views[0]["view"]["annotations"].as_array().is_some_and(|v|!v.is_empty()){return Err("PRO sessions are not included in Mini".into());}
 if let Some(apertures)=views[0]["apertures"].as_array(){for a in apertures{mini_measure(&serde_json::json!({"apertures":[a]}))?;}}Ok(())
}
