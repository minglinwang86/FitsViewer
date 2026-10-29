//! PRO controls are instantiated only after selecting PRO.
//! PRO 模式才创建工作区、查询状态与叠加缓存。
use eframe::egui::{self,Color32,Pos2,Stroke};
use serde_json::{json,Value};
use crate::science_client::{Client,Reply};
use fitsviewer_core::{FitsImage,regions::{Aperture,Shape}};
use std::sync::{Arc,Mutex};
pub const OPERATIONS:&[(&str,&str)]=&[
    ("spectral_candidates","Spectral reference lines"),
    ("spectral_analysis","Spectral analysis"),
    ("image_layers","Layers"),("image_flux_match","Match image brightness"),
    ("identify_stars","Find stars"),("detect_stars","Local source candidates"),("detect_lines","Detect straight traces"),("trace_curve","Track seeded curve"),
    ("catalogue","Catalogue overlay"),("xmatch","Local XMatch"),("xmatch_cds","CDS XMatch"),
    ("reproject","Reprojection"),("rgb","RGB composition"),("export","Figure export"),("math_preview","MathText preview"),
    ("line_profile","Line / trace profile"),("archive_search","Nearby observations"),("vo_download","Download selected resource"),("samp","SAMP")];
pub fn spectrum_operation(operation:&str)->bool{["spectral_candidates","spectral_analysis","samp"].contains(&operation)}
#[derive(Default)]
struct Download {source:Option<String>,persist:bool,pending:Option<Value>,saving:bool,cancel_pending:bool,cutout:Value,notice:Option<String>}
pub struct Workspace {
    pub operation:String,pub parameters:String,pub result:Value,pub message:String,pub busy:bool,
    pub markers:Vec<Value>,pub lines:Vec<Vec<[f64;2]>>,pub annotations:Vec<Value>,
    pub artifact:Option<Value>,pub open_artifact:Option<String>,pub open_path:Option<String>,pub new_apertures:Vec<Aperture>,
    pub job:Option<String>,pub last_poll:f64,pub heartbeat:f64,pub table_row:Option<usize>,status_pending:bool,
    annotation_text:String,annotation_position:[f64;2],title:String,
    preview_id:String,preview_pending:Arc<Mutex<Option<Result<Vec<u8>,String>>>>,preview:Option<egui::TextureHandle>,pub preview_open:bool,
    samp_remote:bool,samp_connected:bool,samp_after:u64,last_samp:f64,pending_sky:Option<[f64;2]>,pub focus_pixel:Option<[f64;2]>,pending_download:Option<String>,auto_open_download:bool,
    marker_query:String,marker_labels:bool,manual_marker:[f64;2],selected_rows:Vec<usize>,samp_table_id:String,pending_table:bool,
    annotation_anchor:String,pub annotations_dirty:bool,annotation_revision:u64,annotation_generation:u64,annotation_wcs:Value,
    pub spectral_lines:Option<Box<crate::spectral_lines::Controls>>,
    pub spectral_analysis:Option<Box<crate::spectral_analysis::Analysis>>,
    pub panel_width:f32,
    points:crate::pro_points::Points,parameters_by_tool:std::collections::BTreeMap<String,String>,
    services:Value,services_requested:bool,marker_source:String,line_source:String,coordinates_table:Value,
    products:Vec<Value>,product_row:Option<usize>,product_links:Value,result_stale:bool,download:Download,

    pub image_layers:Option<Box<crate::image_layers::Renderer>>,pending_flux_match:Option<Value>,
}
impl Default for Workspace {
    fn default()->Self{Self{operation:"identify_stars".into(),parameters:"{}".into(),result:Value::Null,message:String::new(),busy:false,markers:vec![],lines:vec![],annotations:vec![],artifact:None,open_artifact:None,open_path:None,new_apertures:vec![],job:None,status_pending:false,last_poll:0.,heartbeat:-100.,table_row:None,annotation_text:String::new(),annotation_position:[1.,1.],title:String::new(),preview_id:String::new(),preview_pending:Arc::new(Mutex::new(None)),preview:None,preview_open:false,samp_remote:false,samp_connected:false,samp_after:0,last_samp:0.,pending_sky:None,focus_pixel:None,pending_download:None,auto_open_download:false,marker_query:String::new(),marker_labels:false,manual_marker:[1.,1.],selected_rows:vec![],samp_table_id:String::new(),pending_table:false,annotation_anchor:"Pixel".into(),annotations_dirty:true,annotation_revision:0,annotation_generation:0,annotation_wcs:Value::Null,spectral_lines:None,spectral_analysis:None,panel_width:0.,points:Default::default(),parameters_by_tool:Default::default(),services:Value::Null,services_requested:false,marker_source:String::new(),line_source:String::new(),coordinates_table:Value::Null,products:vec![],product_row:None,product_links:Value::Null,result_stale:false,download:Default::default(),image_layers:None,pending_flux_match:None}}
}
fn defaults(operation:&str)->Value{match operation{
    "identify_stars"=>json!({"catalogue_source":"gaia_dr3","magnitude_limit":20.,"max_rows":1000,"timeout":60.}),
    "archive_search"=>json!({"service_id":"aladin_hips","use_image_center":true,"frame":"icrs","ra_deg":null,"dec_deg":null,"radius_deg":0.01,"max_rows":20,"timeout":60.}),
    "vo_datalink"=>json!({"url":"","max_rows":100}),
    "detect_stars"=>json!({"fwhm":3.,"threshold_sigma":5.,"min_separation":4.,"limit":2000,"mask_path":""}),
    "spectral_candidates"=>json!({"unit":"Select unit","method":"derivative","sigma":5.,"continuum_window":101}),
    "detect_lines"=>json!({"threshold_sigma":4.,"min_length":20,"gap":4,"votes":12,"width":5.,"seed":20261017}),
    "trace_curve"=>json!({"seed":null,"axis":"x","direction":1,"search_half_width":8.,"centroid_half_width":3.,"order":3}),
    "catalogue"=>json!({"catalogue":"","ra_column":"ra","dec_column":"dec","ra_unit":"deg","dec_unit":"deg","frame":"icrs","marker_limit":5000}),
    "xmatch"=>json!({"first":{"catalogue":"","ra_column":"ra","dec_column":"dec","frame":"icrs"},"second":{"catalogue":"","ra_column":"ra","dec_column":"dec","frame":"icrs"},"radius_arcsec":1.,"strategy":"nearest"}),
    "xmatch_cds"=>json!({"catalogue":"","ra_column":"ra","dec_column":"dec","remote_catalogue":"vizier:I/355/gaiadr3","radius_arcsec":1.,"strategy":"all"}),
    "reproject"=>json!({"projection":"TAN","use_image_center":true,"ra_deg":null,"dec_deg":null,"scale_arcsec":null,"rotation_deg":0.,"width":512,"height":512}),
    "rgb"=>json!({"channels":[{"path":"","weight":1.,"stretch":"Linear"},{"path":"","weight":1.,"stretch":"Linear"},{"path":"","weight":1.,"stretch":"Linear"}],"assume_aligned":false,"reproject":true}),
    "export"=>json!({"format":"png","width":1600,"height":1200,"dpi":150.,"wcs_axes":false,"colorbar":true,"compass":false}),
    "math_preview"=>json!({"expression":"$F_{\\lambda} / \\sigma$"}),
    "vo_query"=>json!({"protocol":"tap","service":"https://tapvizier.cds.unistra.fr/TAPVizieR/tap","adql":"SELECT TOP 10 * FROM \"I/355/gaiadr3\"","max_rows":100}),
    "vo_download"=>json!({"url":"","maximum_bytes":536870912}),
    "samp"=>json!({"action":"status"}),
    "line_profile"=>json!({"points":[],"width":5.,"background_gap":2.,"background_width":4.}),
    _=>json!({}),
}}
impl Workspace {
    pub fn sync_spectrum(&mut self,image:Option<&FitsImage>){
        let Some(image)=image.filter(|image|image.spectrum.is_some()) else{
            self.spectral_analysis=None;if self.spectral_lines.take().is_some(){self.select("identify_stars");}return;
        };
        let source=crate::spectral_lines::Controls::source(image);
        if self.spectral_lines.as_ref().is_none_or(|lines|lines.source!=source){
            self.spectral_analysis=Some(Box::new(crate::spectral_analysis::Analysis::new(image,self.spectral_analysis.as_deref())));
            self.spectral_lines=Some(Box::new(crate::spectral_lines::Controls::new(image)));
            self.select("spectral_candidates");
        }
        if let (Some(analysis),Some(lines))=(&mut self.spectral_analysis,&self.spectral_lines){analysis.sync_context(lines);}
        if !spectrum_operation(&self.operation){self.select("spectral_candidates");}
    }
    pub fn select(&mut self,operation:&str){
        if self.busy && operation!=self.operation{return;}
        if operation!=self.operation{
            self.parameters_by_tool.insert(self.operation.clone(),self.parameters.clone());
            self.result=Value::Null;self.result_stale=false;self.message.clear();self.artifact=None;self.table_row=None;self.selected_rows.clear();
            self.preview=None;self.preview_id.clear();self.preview_open=false;self.preview_pending=Arc::new(Mutex::new(None));self.points.cancel();
        }
        self.operation=operation.into();self.parameters=self.parameters_by_tool.get(operation).filter(|value|value.as_str()!="{}").cloned().unwrap_or_else(||serde_json::to_string_pretty(&defaults(operation)).unwrap_or_default());
    }
    pub fn picking(&self)->bool{self.points.active && ["trace_curve","line_profile"].contains(&self.operation.as_str())}
    pub fn image_interaction(&mut self,ctx:&egui::Context,response:&egui::Response,painter:&egui::Painter,transform:crate::image_transform::ImageTransform,interactive:bool){
        if let Ok(mut params)=serde_json::from_str::<Value>(&self.parameters){if self.points.canvas(ctx,response,painter,transform,&self.operation,&mut params,interactive){self.parameters=params.to_string();if !self.result.is_null(){self.result_stale=true;}}}
    }
    pub fn select_analysis(&mut self,task:&str){if self.busy{return;}self.select("spectral_analysis");if let Some(analysis)=&mut self.spectral_analysis{analysis.task=task.into();}}
    pub fn clear_document_results(&mut self){
        self.download=Download::default();self.auto_open_download=false;
        self.markers.clear();self.lines.clear();self.marker_source.clear();self.line_source.clear();self.coordinates_table=Value::Null;self.products.clear();self.product_row=None;self.product_links=Value::Null;self.points.cancel();self.parameters_by_tool.clear();self.parameters=serde_json::to_string_pretty(&defaults(&self.operation)).unwrap_or_default();self.annotations.clear();self.job=None;self.busy=false;self.status_pending=false;self.samp_connected=false;self.result=Value::Null;self.result_stale=false;self.message.clear();self.artifact=None;self.preview=None;self.preview_id.clear();self.pending_flux_match=None;
        if let Some(renderer)=&mut self.image_layers{renderer.clear();}
        // A fresh inbox detaches a late preview callback from the next document.
        // 换用新收件箱，旧文档稍后完成的预览回调不能污染新文档。
        self.preview_pending=Arc::new(Mutex::new(None));
    }
    pub fn select_row(&mut self,ctx:&egui::Context,client:&Client,generation:u64,row:usize,notify:bool){
        self.table_row=Some(row);self.selected_rows=vec![row];
        if let Some(marker)=self.markers.iter().find(|m|m["row"].as_u64()==Some(row as u64)){self.focus_pixel=pair_xy(marker);}
        if notify && self.samp_connected && !self.samp_table_id.is_empty(){client.request(ctx,generation,if self.samp_remote{"samp_bridge"}else{"samp"},json!({"action":"send","mtype":"table.highlight.row","params":{"table-id":self.samp_table_id,"row":row.to_string()}}));}
    }
    pub fn submit(&mut self,ctx:&egui::Context,client:&Client,generation:u64,image:Option<&FitsImage>,view:&Value){
        if self.busy{return;}
        if self.operation=="image_layers"{self.message="Use Layers to choose a source image and run brightness matching".into();return;}
        if image.is_some_and(|image|image.spectrum.is_some()) && !spectrum_operation(&self.operation){self.message="This tool requires a two-dimensional image".into();return;}
        let mut params=match serde_json::from_str::<Value>(&self.parameters){Ok(Value::Object(v))=>Value::Object(v),_=>{self.message="Parameters must be a JSON object".into();return;}};
        if let Some(error)=crate::pro_forms::validate(&self.operation,&params){self.message=error.into();return;}
        if let Some(image)=image.filter(|_|self.operation!="image_flux_match") {params["path"]=json!(image.info.path);params["hdu"]=json!(image.info.hdu);}
        if self.operation=="spectral_analysis"{
            self.sync_spectrum(image);let action=params["action"].as_str().unwrap_or("continuum").to_owned();
            if let (Some(analysis),Some(lines),Some(image))=(&mut self.spectral_analysis,&mut self.spectral_lines,image){
                let mut next_lines=lines.clone();if let Err(error)=next_lines.configure(&params).and_then(|_|next_lines.request(image).map(|_|())){self.message=error;return;}
                let configuration=if params.get("parameters").is_some(){params.clone()}else{json!({"parameters":params,"action":action})};
                if let Err(error)=analysis.configure(&configuration){self.message=error;return;}
                *lines=next_lines;analysis.sync_context(lines);
                match analysis.request(&action,image,lines){Ok(request)=>{self.send(ctx,client,generation,"spectral_analysis",request);},Err(error)=>self.message=error}
            }else{self.message="Open a spectrum first".into();}return;
        }
        if self.operation=="spectral_candidates"{
            self.sync_spectrum(image);
            let Some(lines)=&mut self.spectral_lines else{self.message="Open a spectrum first".into();return;};
            if let Err(error)=lines.configure(&params){self.message=error;return;}
            let request=match lines.request(image.unwrap()){Ok(request)=>request,Err(error)=>{self.message=error;return;}};
            for key in ["path","hdu","x_column","y_column","error_column","error_hdu","error_kind","no_error","unit","air","source"]{params[key]=request[key].clone();}
        }
        if !view["wcs_override"].is_null(){params[if self.operation=="image_flux_match"{"reference_wcs_override"}else{"wcs_override"}]=view["wcs_override"].clone();}
        if self.operation=="catalogue" && params.get("include_coordinates").is_none(){params["include_coordinates"]=json!(false);}
        if self.operation=="export" {params["view"]=view.clone();params["annotations"]=json!(self.annotations);params["apertures"]=view["apertures"].clone();params["title"]=json!(self.title);}
        let operation=self.operation.clone();self.send(ctx,client,generation,&operation,params);
    }
    pub fn send(&mut self,ctx:&egui::Context,client:&Client,generation:u64,operation:&str,params:Value){
        self.clear_save_chain();
        self.busy=true;self.message=format!("Running {operation}…");self.job=None;self.status_pending=false;
        self.result=Value::Null;self.result_stale=false;self.artifact=None;self.preview=None;self.preview_id.clear();self.preview_open=false;self.preview_pending=Arc::new(Mutex::new(None));self.points.cancel();
        if ["identify_stars","archive_search","reproject","rgb","export","vo_query","vo_download","detect_stars","detect_lines","trace_curve","xmatch_cds","spectral_candidates","spectral_analysis","image_flux_match"].contains(&operation){
            client.request(ctx,generation,"job_start",json!({"operation":operation,"params":params}));
        }else{client.request(ctx,generation,operation,params);}
    }
    fn clear_save_chain(&mut self){
        // A failed download must never save a later tool's FITS beside an old source.
        // 只清本次保存链；保留用户的切图尺寸，避免下一工具沿用过期保存目标。
        self.download.source=None;self.download.persist=false;self.download.pending=None;
        self.download.saving=false;self.download.cancel_pending=false;self.download.notice=None;
    }
    pub fn summary(&self)->Value{
        // The per-frame state must not clone an entire catalogue or trace.
        // 每帧状态只复制预览，避免复制完整星表和线迹。
        fn preview(value:&Value,depth:usize)->Value{if depth>8{return Value::Null;}match value{
            Value::Array(values)=>Value::Array(values.iter().take(64).map(|v|preview(v,depth+1)).collect()),
            Value::Object(values)=>{let mut object=serde_json::Map::new();for (key,value) in values {if key=="coordinates"{continue;}object.insert(key.clone(),preview(value,depth+1));if let Some(array)=value.as_array(){object.insert(format!("{key}_count"),json!(array.len()));}}Value::Object(object)},
            _=>value.clone(),
        }}preview(&self.result,0)
    }
    pub fn poll(&mut self,ctx:&egui::Context,client:&Client,generation:u64){
        if let Some(params)=self.download.pending.take(){self.busy=true;self.download.saving=true;self.message="Saving downloaded FITS…".into();client.request(ctx,generation,"persist_download",params);}
        let now=ctx.input(|i|i.time);
        if self.heartbeat<0.{self.heartbeat=now;}else if now-self.heartbeat>45.{client.request(ctx,generation,"pro_heartbeat",json!({}));self.heartbeat=now;}
        if let Some(job)=&self.job {if !self.status_pending && now-self.last_poll>0.5{self.status_pending=true;client.request(ctx,generation,"job_status",json!({"job_id":job}));self.last_poll=now;}ctx.request_repaint_after(std::time::Duration::from_millis(500));}
        if self.samp_connected && now-self.last_samp>1.{client.request(ctx,generation,if self.samp_remote{"samp_bridge"}else{"samp"},json!({"action":"events","after":self.samp_after}));self.last_samp=now;ctx.request_repaint_after(std::time::Duration::from_millis(1000));}
        ctx.request_repaint_after(std::time::Duration::from_secs(40));
    }
    pub fn reply(&mut self,reply:Reply){
        if reply.operation=="persist_cancel"{
            self.download.cancel_pending=false;
            // Cancellation is a request; only the original save response ends
            // the operation. 已提交文件不会被稍晚的取消请求撤销。
            if self.download.saving{self.message=match reply.result{
                Ok(value)=>if value["cancel_requested"]==true{"Save cancellation requested…".into()}else{"Save state changed; waiting for its result…".into()},
                Err(error)=>error,
            };}return;
        }
        if reply.operation=="persist_download"{
            self.busy=false;self.clear_save_chain();
            match reply.result{
                Ok(value)=>{self.open_path=value["saved_path"].as_str().or_else(||value["path"].as_str()).map(str::to_owned);self.message=if value["persistent"]==true{"Saved beside the original FITS".into()}else{value["reason"].as_str().unwrap_or("Opening the temporary FITS result").into()};self.download.notice=Some(self.message.clone());self.result=value;},
                Err(error)=>{self.message=error;},
            }return;
        }
        if reply.operation=="service_presets"{match reply.result{Ok(value)=>self.services=value,Err(error)=>self.message=error}return;}
        if reply.operation=="vo_datalink"{match reply.result{Ok(value)=>{self.product_links=value;self.busy=false;self.message="Product links resolved".into();},Err(error)=>{self.message=error;self.busy=false;}}return;}
        if reply.operation=="job_status"{self.status_pending=false;if self.job.is_none(){return;}}
        if ["pro_acquire","pro_heartbeat","release_pro","release_view"].contains(&reply.operation.as_str()){return;}
        if reply.operation=="annotation_positions"{
            match reply.result{Ok(value)=>{if value["request_id"].as_u64()==Some(self.annotation_revision) && !self.annotations_dirty{if let (Some(indices),Some(pixels))=(value["indices"].as_array(),value["pixels"].as_array()){for (index,pixel) in indices.iter().zip(pixels){if let Some(annotation)=index.as_u64().and_then(|i|self.annotations.get_mut(i as usize)){annotation["x"]=pixel[0].clone();annotation["y"]=pixel[1].clone();}}}}},Err(error)=>self.message=error,}return;
        }
        match reply.result {
            Err(error)=>{self.busy=false;self.job=None;self.clear_save_chain();self.message=error;},
            Ok(value)=>{
                if reply.operation=="sky_point"{self.focus_pixel=pair(&value["pixel"]);return;}
                if reply.operation=="samp" || reply.operation=="samp_bridge"{
                    if let Some(connected)=value["connected"].as_bool(){self.samp_connected=connected;if self.operation=="samp"{self.message=if connected{"SAMP connected"}else{"No SAMP Hub connection"}.into();}}
                    if let Some(latest)=value["latest"].as_u64(){self.samp_after=latest;}
                    if let Some(events)=value["events"].as_array(){for event in events{
                        let params=&event["params"];let path=event["path"].as_str();
                        match event["mtype"].as_str().unwrap_or("") {
                            "coord.pointAt.sky"=>{let number=|v:&Value|v.as_f64().or_else(||v.as_str().and_then(|s|s.parse().ok()));if let (Some(ra),Some(dec))=(number(&params["ra"]),number(&params["dec"])){self.pending_sky=Some([ra,dec]);}},
                            "table.highlight.row"=>{if params["table-id"].as_str()==Some(&self.samp_table_id){self.table_row=params["row"].as_str().and_then(|s|s.parse().ok()).or_else(||params["row"].as_u64().map(|v|v as usize));if let Some(row)=self.table_row{self.selected_rows=vec![row];if let Some(marker)=self.markers.iter().find(|m|m["row"].as_u64()==Some(row as u64)){self.focus_pixel=pair_xy(marker);}}}},
                            "image.load.fits"|"spectrum.load.ssa-generic"=>{if let Some(path)=path{self.open_path=Some(path.into());}else if let Some(url)=params["url"].as_str().filter(|s|s.starts_with("http://") || s.starts_with("https://")){self.pending_download=Some(url.into());self.auto_open_download=true;}},
                            "table.load.votable"|"table.load.fits"=>{self.samp_table_id=params["table-id"].as_str().or_else(||params["url"].as_str()).unwrap_or("").into();if let Some(path)=path{self.select("catalogue");self.parameters=json!({"catalogue":path}).to_string();self.message="SAMP table received. Run Catalogue overlay to inspect its coordinate columns.".into();}else if let Some(url)=params["url"].as_str(){self.pending_download=Some(url.into());self.pending_table=true;}},
                            "table.select.rowList"=>{if params["table-id"].as_str()==Some(&self.samp_table_id){self.selected_rows=params["row-list"].as_array().into_iter().flatten().filter_map(|v|v.as_u64().map(|v|v as usize).or_else(||v.as_str()?.parse().ok())).take(20000).collect();self.table_row=self.selected_rows.first().copied();if let Some(row)=self.table_row{if let Some(marker)=self.markers.iter().find(|m|m["row"].as_u64()==Some(row as u64)){self.focus_pixel=pair_xy(marker);}}}},_=>{},
                        }
                    }}
                    if self.operation=="samp" && self.job.is_none(){self.busy=false;}
                    if self.operation=="samp" && !value["events"].as_array().is_some_and(Vec::is_empty){self.result=value;}return;
                }
                if reply.operation=="job_start"{self.job=value["job_id"].as_str().map(str::to_string);return;}
                let value=if reply.operation=="job_status" {
                    match value["state"].as_str().unwrap_or("") {
                        "running"|"queued"=>return,
                        "completed"=>{self.job=None;value["result"].clone()},
                        "consumed"=>{self.job=None;self.busy=false;self.clear_save_chain();self.message="Task result expired; run the task again".into();return;},
                        _=>{self.busy=false;self.job=None;self.clear_save_chain();self.message=value["error"].as_str().unwrap_or("Job cancelled").into();return;},
                    }
                }else{value};
                self.busy=false;self.message=if value["status"]=="coverage_mismatch"{String::new()}else{"Completed".into()};
                if value["artifact_available"]==false{self.artifact=None;self.message=value["artifact_error"].as_str().unwrap_or("Task result expired; run the task again").into();self.result=value;return;}
                if value["operation"]=="image_flux_match"{self.pending_flux_match=Some(value);return;}
                if value["operation"]=="spectral_candidates"{if let Some(lines)=&mut self.spectral_lines{lines.accept(&value);}}
                if value["operation"]=="spectral_analysis" && !self.spectral_analysis.as_mut().is_some_and(|analysis|analysis.accept(&value)){self.message="Discarded stale spectral analysis result".into();return;}
                if let Some(markers)=value["markers"].as_array().or_else(||value["sources"].as_array()){
                    self.markers=markers.clone();self.marker_source=self.operation.clone();
                    let coordinates=self.markers.iter().filter_map(|marker|Some((marker["ra_deg"].as_f64()?,marker["dec_deg"].as_f64()?))).take(20000).collect::<Vec<_>>();
                    if value["coordinates"].is_object(){self.coordinates_table=value["coordinates"].clone();}
                    else if !coordinates.is_empty() && value["frame"].is_string(){self.coordinates_table=json!({"ra_deg":coordinates.iter().map(|p|p.0).collect::<Vec<_>>(),"dec_deg":coordinates.iter().map(|p|p.1).collect::<Vec<_>>(),"frame":value["frame"].as_str().unwrap_or("fk5").to_lowercase()});}
                }
                if let Some(products)=value["products"].as_array(){
                    self.products=products.iter().take(1000).map(|product|{let mut product=product.clone();product["query_region"]=value["query_region"].clone();product}).collect();
                    if !self.products.is_empty(){self.choose_product(0);}else{self.product_row=None;self.product_links=Value::Null;}
                }
                if let Some(lines)=value["lines"].as_array(){self.line_source=self.operation.clone();self.lines=lines.iter().filter_map(|l|Some(vec![pair(&l["start"])?,pair(&l["end"])?])).collect();}
                if let Some(points)=value["points"].as_array(){let points=points.iter().filter_map(pair).collect::<Vec<_>>();if points.len()>1{self.line_source=self.operation.clone();self.lines=vec![points];}}
                if value["artifact_id"].is_string(){self.artifact=Some(value.clone());
                    if self.download.persist && value["kind"]=="fits"{let mut params=json!({"artifact_id":value["artifact_id"]});if let Some(source)=&self.download.source{params["source_path"]=json!(source);}self.download.pending=Some(params);self.download.persist=false;self.busy=true;}
                    else if self.auto_open_download && (value["kind"]=="fits" || value["name"].as_str().is_some_and(|n|[".txt",".dat",".csv"].iter().any(|suffix|n.ends_with(suffix)))){self.open_path=Some(value["_path"].as_str().map(str::to_string).unwrap_or_else(||format!("artifact:{}",value["artifact_id"].as_str().unwrap())));self.auto_open_download=false;}}
                if self.pending_table && value["artifact_id"].is_string(){self.pending_table=false;self.select("catalogue");self.parameters=json!({"catalogue":value["_path"].as_str().map(str::to_string).unwrap_or_else(||format!("artifact:{}",value["artifact_id"].as_str().unwrap()))}).to_string();self.message="SAMP table received. Run Catalogue overlay to inspect its coordinate columns.".into();}
                self.result=value;
            }
        }
    }
    pub fn tick(&mut self,ctx:&egui::Context,client:&Client,generation:u64,image:Option<&FitsImage>,view:&Value,multi:&mut crate::multiview::Workspace){
        self.sync_spectrum(image);
        if !self.services_requested{self.services_requested=true;client.request(ctx,generation,"service_presets",json!({}));}
        self.poll(ctx,client,generation);
        if let Some(value)=self.pending_flux_match.take(){match multi.apply_flux_match(&value,image){
            Ok(())=>{if value["status"]=="ok"{if let Some(renderer)=&mut self.image_layers{renderer.clear();}}self.result=value;self.message=multi.message.clone();},
            Err(error)=>self.message=error,
        }}
        if image.is_some_and(|image|image.spectrum.is_some()){self.image_layers=None;}
        if self.spectral_lines.is_none(){self.preview(ctx,client);}
        if self.annotation_generation!=generation || self.annotation_wcs!=view["wcs_override"]{self.annotation_generation=generation;self.annotation_wcs=view["wcs_override"].clone();self.annotations_dirty=true;}
        if self.annotations_dirty && self.spectral_lines.is_none(){
            self.annotations_dirty=false;self.annotation_revision+=1;
            let indices=self.annotations.iter().enumerate().filter(|(_,a)|a["anchor"]=="sky").map(|(i,_)|i).collect::<Vec<_>>();
            if !indices.is_empty(){if let Some(image)=image{let points=indices.iter().map(|&i|json!([self.annotations[i]["ra_deg"],self.annotations[i]["dec_deg"]])).collect::<Vec<_>>();for &index in &indices{self.annotations[index]["x"]=Value::Null;self.annotations[index]["y"]=Value::Null;}client.request_labeled(ctx,generation,"sky_point","annotation_positions",json!({"path":image.info.path,"hdu":image.info.hdu,"points":points,"indices":indices,"request_id":self.annotation_revision,"wcs_override":view["wcs_override"]}));}}
        }
        if self.spectral_lines.is_none(){if let (Some(sky),Some(image))=(self.pending_sky.take(),image){client.request(ctx,generation,"sky_point",json!({"path":image.info.path,"hdu":image.info.hdu,"ra_deg":sky[0],"dec_deg":sky[1],"wcs_override":view["wcs_override"]}));}}
        if !self.busy{if let Some(url)=self.pending_download.take(){self.send(ctx,client,generation,"vo_download",json!({"url":url}));}}
    }
    pub fn ui(&mut self,ui:&mut egui::Ui,ctx:&egui::Context,client:&Client,generation:u64,image:Option<&FitsImage>,view:&Value,multi:&mut crate::multiview::Workspace){
        let spectrum=self.spectral_lines.is_some();crate::ribbon::align_controls(ui);
            let old=self.operation.clone();
            ui.horizontal(|ui|{ui.add_enabled_ui(!self.busy,|ui|egui::ComboBox::from_id_salt("pro_operation").width((ui.available_width()-32.).max(80.)).selected_text(OPERATIONS.iter().find(|(o,_)|*o==self.operation).map(|(_,n)|crate::i18n::t(n)).unwrap_or(&self.operation)).show_ui(ui,|ui|{for (operation,label) in OPERATIONS{
                if (spectrum && !spectrum_operation(operation)) || (!spectrum && ["spectral_candidates","spectral_analysis"].contains(operation)){continue;}
                ui.selectable_value(&mut self.operation,operation.to_string(),crate::i18n::t(label));
            }}));information(ui,"operation-info",|ui|{
                ui.label(crate::i18n::t("Drag the left edge to resize; Work area restores this panel."));
                for text in operation_hints(&self.operation){ui.label(crate::i18n::t(text));}
                ui.add_enabled_ui(!self.busy,|ui|{if let Ok(mut params)=serde_json::from_str::<Value>(&self.parameters){let before=params.clone();crate::pro_forms::advanced(ui,&self.operation,&mut params);if params!=before{self.parameters=params.to_string();self.result_stale=!self.result.is_null();}}});
                egui::CollapsingHeader::new(crate::i18n::t("Advanced parameters / reproducible JSON")).id_salt("tool-json").show(ui,|ui|{
                    if ui.button(crate::i18n::t("Copy parameters")).clicked(){ctx.copy_text(self.parameters.clone());}
                    if ui.add_enabled(!self.busy,egui::TextEdit::multiline(&mut self.parameters).desired_width(360.).code_editor().desired_rows(10)).changed(){self.result_stale=!self.result.is_null();}
                });
                if ui.button(crate::i18n::t("Copy result")).clicked(){ctx.copy_text(serde_json::to_string_pretty(&self.result).unwrap_or_default());}
                if !self.result.is_null(){egui::CollapsingHeader::new(crate::i18n::t("Result details")).id_salt("overflow-result").show(ui,|ui|{ui.add(egui::Label::new(serde_json::to_string_pretty(&self.summary()).unwrap_or_default()).wrap());});}
            });});
            if old!=self.operation || self.parameters=="{}"{let operation=self.operation.clone();self.operation=old;self.select(&operation);}
            if let Some(image)=image{ui.add(egui::Label::new(format!("{} · HDU {}",image.info.path.rsplit(['/', '\\']).next().unwrap_or(&image.info.path),image.info.hdu)).truncate()).on_hover_text(&image.info.path);}
            self.status_ui(ui,ctx,client,generation);
            ui.scope(|ui|{
                if ["image_layers","image_flux_match"].contains(&self.operation.as_str()){
                    if let Some(mut params)=multi.image_layers_ui(ui,image,self.busy){if !view["wcs_override"].is_null(){params["reference_wcs_override"]=view["wcs_override"].clone();}self.send(ctx,client,generation,"image_flux_match",params);}
                    if let Some(job)=&self.job{if ui.button(crate::i18n::t("Cancel")).clicked(){client.request(ctx,generation,"job_cancel",json!({"job_id":job}));}}
                    if ui.button(crate::i18n::t("Copy result")).clicked(){ctx.copy_text(serde_json::to_string_pretty(&self.result).unwrap_or_default());}return;
                }
                if self.operation=="spectral_analysis"{
                    let quick=if let Some(analysis)=&self.spectral_analysis{match analysis.task.as_str(){"Measure"=>Some(("▶ Measure spectrum","measure")),"Process"=>Some(("▶ Preview smoothing","smooth")),"Fit"=>Some(("▶ Fit spectrum","fit")),_=>None}}else{None};
                    if let Some((label,action))=quick{if ui.add_enabled(!self.busy,egui::Button::new(crate::i18n::t(label))).clicked(){if let (Some(analysis),Some(lines),Some(image))=(&mut self.spectral_analysis,&self.spectral_lines,image){match analysis.request(action,image,lines){Ok(params)=>self.send(ctx,client,generation,"spectral_analysis",params),Err(error)=>self.message=error,}}}}
                    if self.current_spectral_result() && ui.button(crate::i18n::t("Copy result")).clicked(){ctx.copy_text(serde_json::to_string_pretty(&self.result).unwrap_or_default());}
                    let request=if let (Some(analysis),Some(lines),Some(image))=(&mut self.spectral_analysis,&mut self.spectral_lines,image){
                        analysis.ui(ui,image,lines,self.busy,multi).map(|action|analysis.request(&action,image,lines))
                    }else{None};
                    if let Some(request)=request{match request{Ok(params)=>self.send(ctx,client,generation,"spectral_analysis",params),Err(error)=>self.message=error,}}
                    if let Some(job)=&self.job{if ui.button(crate::i18n::t("Cancel")).clicked(){client.request(ctx,generation,"job_cancel",json!({"job_id":job}));}}
                    return;
                }
                if self.operation=="spectral_candidates"{
                    if ui.add_enabled(!self.busy && self.spectral_lines.as_ref().is_some_and(|lines|lines.ready()),egui::Button::new(crate::i18n::t("▶ Detect spectral candidates"))).clicked(){if let (Some(lines),Some(image))=(&self.spectral_lines,image){match lines.request(image){Ok(params)=>self.send(ctx,client,generation,"spectral_candidates",params),Err(error)=>self.message=error,}}}
                    let request=if let Some(lines)=&mut self.spectral_lines{
                        if lines.ui(ui,self.busy){image.map(|image|lines.request(image))}else{None}
                    }else{ui.label(crate::i18n::t("Open a spectrum first"));None};
                    if let Some(request)=request{match request{Ok(params)=>self.send(ctx,client,generation,"spectral_candidates",params),Err(error)=>self.message=error,}}
                    if let Some(job)=&self.job{if ui.button(crate::i18n::t("Cancel")).clicked(){client.request(ctx,generation,"job_cancel",json!({"job_id":job}));}}
                    if ui.button(crate::i18n::t("Copy result")).clicked(){ctx.copy_text(serde_json::to_string_pretty(&self.result).unwrap_or_default());}
                    return;
                }
                if self.operation=="samp"{
                    ui.horizontal(|ui|{ui.checkbox(&mut self.samp_remote,crate::i18n::t("Use local bridge for remote Jupyter"));
                        if spectrum{information(ui,"samp-info",|ui|{for text in operation_hints("samp"){ui.label(crate::i18n::t(text));}});}
                    });
                    ui.horizontal_wrapped(|ui|{for action in ["connect","status","events","disconnect"]{if ui.add_enabled(!self.busy,egui::Button::new(crate::i18n::t(action))).clicked(){self.busy=true;self.message="Running…".into();client.request(ctx,generation,if self.samp_remote{"samp_bridge"}else{"samp"},json!({"action":action,"after":self.samp_after}));}}});
                    if ui.add_enabled(self.samp_connected && !self.busy,egui::Button::new(crate::i18n::t("Send active FITS to Hub clients"))).clicked(){if let Some(image)=image{client.request(ctx,generation,if self.samp_remote{"samp_bridge"}else{"samp"},json!({"action":"send","mtype":if image.spectrum.is_some(){"spectrum.load.ssa-generic"}else{"image.load.fits"},"path":image.info.path,"params":{"name":image.info.path}}));}}
                }
                if self.operation=="samp"{ui.label(crate::i18n::t(if self.samp_connected{"SAMP connected"}else{"No SAMP Hub connection"}));return;}
                if self.operation=="catalogue"{
                    if let Ok(parameters)=serde_json::from_str::<Value>(&self.parameters){if let Some(path)=parameters["catalogue"].as_str().filter(|p|!p.is_empty()){
                        if ui.add_enabled(self.samp_connected,egui::Button::new(crate::i18n::t("Send current table to SAMP"))).clicked(){self.samp_table_id=path.into();client.request(ctx,generation,if self.samp_remote{"samp_bridge"}else{"samp"},json!({"action":"send","mtype":if path.ends_with(".fits"){"table.load.fits"}else{"table.load.votable"},"path":path,"params":{"table-id":path,"name":path}}));}
                    }}
                }
                if ["detect_stars","detect_lines","trace_curve"].contains(&self.operation.as_str()) && ui.add_enabled(!self.busy,egui::Button::new(crate::i18n::t("Use visible image as detection ROI"))).clicked(){if let (Some(image),Some(bounds))=(image,view["crop"].as_array()){let mut parameters=serde_json::from_str::<Value>(&self.parameters).unwrap_or(json!({}));parameters["roi"]=json!([bounds[0].as_f64().unwrap_or(0.).floor().max(0.) as usize,bounds[1].as_f64().unwrap_or(0.).floor().max(0.) as usize,bounds[2].as_f64().unwrap_or(image.info.width as f64).ceil().min(image.info.width as f64) as usize,bounds[3].as_f64().unwrap_or(image.info.height as f64).ceil().min(image.info.height as f64) as usize]);self.parameters=parameters.to_string();self.result_stale=!self.result.is_null();self.message="Detection ROI selected; run explicitly when ready.".into();}}
                let special=["image_layers","image_flux_match","spectral_analysis","spectral_candidates","samp"].contains(&self.operation.as_str());
                if !special{
                    if let Ok(params)=serde_json::from_str::<Value>(&self.parameters){if let Some(error)=crate::pro_forms::validate(&self.operation,&params){ui.weak(crate::i18n::t(error));}}
                    if ui.add_enabled(!self.busy,egui::Button::new(crate::i18n::t(crate::pro_forms::action(&self.operation)))).clicked(){self.submit(ctx,client,generation,image,view);}
                    ui.add_enabled_ui(!self.busy,|ui|{if let Ok(mut params)=serde_json::from_str::<Value>(&self.parameters){
                        let before=params.clone();
                        if self.operation=="xmatch_cds" && ui.add_enabled(!self.coordinates_table.is_null(),egui::Button::new(crate::i18n::t("Reuse identified / imported coordinates"))).clicked(){params["coordinates"]=self.coordinates_table.clone();params["catalogue"]=json!("");}
                        if self.operation=="line_profile"{
                            let aperture=view["selected_aperture"].as_u64().and_then(|index|view["apertures"].get(index as usize)).and_then(|value|serde_json::from_value::<Aperture>(value.clone()).ok());
                            let path=aperture.as_ref().and_then(|aperture|if let Shape::Path{points,width,..}=aperture.shape.flattened(){Some((points,width))}else{None});
                            if ui.add_enabled(path.is_some(),egui::Button::new(crate::i18n::t("Use selected line / path aperture"))).clicked(){if let Some((points,width))=path{params["points"]=json!(points);params["width"]=json!(width);}}
                        }
                        self.points.ui(ui,&self.operation,&mut params,&self.lines);
                        crate::pro_forms::common(ui,&self.operation,&mut params,&multi.paths(),&self.services,&view["sky_position"]);
                        if params!=before && !self.result.is_null(){self.result_stale=true;}
                        self.parameters=params.to_string();
                    }});
                }
                if self.operation=="archive_search"{self.archive_results_ui(ui,ctx,client,generation,image,view);return;}
                if spectrum{
                    egui::CollapsingHeader::new(crate::i18n::t("Result details")).id_salt("spectrum-result-details").show(ui,|ui|{
                        ui.add(egui::Label::new(serde_json::to_string_pretty(&self.summary()).unwrap_or_default()).wrap());
                    });
                    return;
                }
                if let Some(artifact)=&self.artifact {
                    ui.label(format!("{} · {} bytes",artifact["name"].as_str().unwrap_or("Result"),artifact["bytes"]));
                    if let Some(id)=artifact["artifact_id"].as_str(){
                        if let Some(url)=client.artifact_url(id){ui.hyperlink_to(crate::i18n::t("Download / open result"),url);}
                        #[cfg(not(target_arch="wasm32"))]
                        if let Some(path)=artifact.get("_path").and_then(Value::as_str){if ui.button(crate::i18n::t("Open result file")).clicked(){self.open_artifact=Some(path.into());}}
                        if artifact["kind"]=="fits" && ui.button(crate::i18n::t("Open generated FITS")).clicked(){self.open_path=artifact.get("_path").and_then(Value::as_str).map(str::to_string);if self.open_path.is_none(){self.open_path=Some(format!("artifact:{id}"));}}
                        if artifact["kind"]=="png" && ui.button(crate::i18n::t("Show preview")).clicked(){self.preview_open=true;}
                    }
                }
                if let Some(path)=self.result["reference_path"].as_str(){if ui.button(crate::i18n::t("Open R reference FITS")).clicked(){self.open_path=Some(path.into());}}
                if self.operation=="identify_stars" && self.marker_source=="identify_stars" && !self.markers.is_empty(){
                    ui.strong(crate::i18n::t("Catalogue objects in this image"));
                    egui::ScrollArea::vertical().id_salt("identified-objects").max_height(210.).show(ui,|ui|{for marker in self.markers.iter().take(200){
                        let text=format!("{} · RA {:.6}° · Dec {:.6}° · {} {}",marker["id"].as_str().unwrap_or(""),marker["ra_deg"].as_f64().unwrap_or(f64::NAN),marker["dec_deg"].as_f64().unwrap_or(f64::NAN),marker["band"].as_str().unwrap_or(""),marker["magnitude"].as_f64().map(|v|format!("{v:.3}")).unwrap_or_else(||"—".into()));
                        if ui.selectable_label(false,text).clicked(){self.focus_pixel=pair_xy(marker);}
                    }});
                }
                if ["identify_stars","detect_stars","catalogue"].contains(&self.operation.as_str()){
                    if !self.marker_source.is_empty() && self.marker_source!=self.operation{ui.weak(format!("{}: {}",crate::i18n::t("Overlay source"),self.marker_source));}
                egui::CollapsingHeader::new(crate::i18n::t("Source markers / manual correction")).id_salt("source-markers").show(ui,|ui|{
                    ui.text_edit_singleline(&mut self.marker_query);ui.checkbox(&mut self.marker_labels,crate::i18n::t("Show source labels"));
                    ui.horizontal(|ui|{ui.label(crate::i18n::t("x / y (one-based)"));for value in &mut self.manual_marker{ui.add(egui::DragValue::new(value).speed(0.1));}});
                    if ui.button(crate::i18n::t("Add manual source")).clicked() && self.manual_marker.iter().all(|v|v.is_finite() && *v>=1. && *v<=1e7) && self.markers.len()<20000{self.markers.push(json!({"x":self.manual_marker[0]-1.,"y":self.manual_marker[1]-1.,"id":"manual","manual":true}));}
                    let mut remove=None;
                    egui::ScrollArea::vertical().max_height(120.).show(ui,|ui|{for (index,marker) in self.markers.iter().enumerate().filter(|(_,m)|self.marker_query.is_empty() || m["id"].to_string().contains(&self.marker_query)).take(200){ui.horizontal(|ui|{
                        if ui.selectable_label(self.table_row==marker["row"].as_u64().map(|v|v as usize),format!("{}: {:.2}, {:.2}",marker["id"],marker["x"].as_f64().unwrap_or(0.)+1.,marker["y"].as_f64().unwrap_or(0.)+1.)).clicked(){self.focus_pixel=pair_xy(marker);self.table_row=marker["row"].as_u64().map(|v|v as usize);}
                        if ui.small_button("×").clicked(){remove=Some(index);}
                    });}});if let Some(index)=remove{self.markers.remove(index);}
                });
                if !self.markers.is_empty(){
                    ui.label(format!("{} {}",self.markers.len(),crate::i18n::t("visible detections / catalogue markers")));
                    if ui.button(crate::i18n::t("Create circular apertures from markers")).clicked(){for (i,m) in self.markers.iter().take(64).enumerate(){if let (Some(x),Some(y))=(m["x"].as_f64(),m["y"].as_f64()){self.new_apertures.push(Aperture{name:format!("S{}",i+1),shape:Shape::Circle{center:[x,y],radius:5.},background:Some(Shape::Ring{center:[x,y],inner:8.,outer:12.}),exclude:vec![]});}}}
                    if ui.button(crate::i18n::t("Clear markers")).clicked(){self.markers.clear();}
                }
                }
                if ["detect_lines","trace_curve","line_profile"].contains(&self.operation.as_str()) && !self.lines.is_empty() && ui.button(crate::i18n::t("Create line / curve apertures")).clicked(){for (i,points) in self.lines.iter().take(64).enumerate(){
                    let step=(points.len()/120).max(1);let points=points.iter().step_by(step).copied().take(128).collect::<Vec<_>>();
                    self.new_apertures.push(Aperture{name:format!("Trace{}",i+1),shape:Shape::Path{points:points.clone(),width:5.,inner_width:0.,smooth:false},background:Some(Shape::Path{points,width:17.,inner_width:9.,smooth:false}),exclude:vec![]});
                }}
                if let Some(columns)=self.result["preview"].as_object(){
                    egui::CollapsingHeader::new(crate::i18n::t("Table preview (first 100 rows)")).id_salt("Table preview (first 100 rows)").show(ui,|ui|{egui::ScrollArea::both().max_height(220.).show(ui,|ui|{egui::Grid::new("catalogue_preview").striped(true).show(ui,|ui|{
                        ui.label(crate::i18n::t("row"));for name in columns.keys().take(8){ui.strong(name);}ui.end_row();
                        let count=columns.values().next().and_then(Value::as_array).map(Vec::len).unwrap_or(0);
                        let start=self.result["preview_start"].as_u64().unwrap_or(0) as usize;
                        for row in 0..count {let global=start+row;if ui.selectable_label(self.table_row==Some(global),global.to_string()).clicked(){self.table_row=Some(global);self.selected_rows=vec![global];if let Some(marker)=self.markers.iter().find(|m|m["row"].as_u64()==Some(global as u64)){self.focus_pixel=pair_xy(marker);}if self.samp_connected && !self.samp_table_id.is_empty(){client.request(ctx,generation,if self.samp_remote{"samp_bridge"}else{"samp"},json!({"action":"send","mtype":"table.highlight.row","params":{"table-id":self.samp_table_id,"row":global.to_string()}}));}}for column in columns.values().take(8){ui.add(egui::Label::new(column[row].to_string()).truncate());}ui.end_row();}
                    });});});
                }
                if let Some(row)=self.table_row{
                    if let Some(columns)=self.result["preview"].as_object(){let start=self.result["preview_start"].as_u64().unwrap_or(0) as usize;if let Some(index)=row.checked_sub(start){
                        for name in self.result["download_url_columns"].as_array().into_iter().flatten().filter_map(Value::as_str){if let Some(url)=columns.get(name).and_then(|v|v.get(index)).and_then(Value::as_str){
                            if ui.add_enabled(!self.busy,egui::Button::new(format!("{} ({name})",crate::i18n::t("Download / open selected row")))).clicked(){self.pending_download=Some(url.into());self.auto_open_download=true;}
                        }}
                    }}
                    if ui.add_enabled(self.samp_connected && !self.samp_table_id.is_empty(),egui::Button::new(crate::i18n::t("Send selected row list to SAMP"))).clicked(){client.request(ctx,generation,if self.samp_remote{"samp_bridge"}else{"samp"},json!({"action":"send","mtype":"table.select.rowList","params":{"table-id":self.samp_table_id,"row-list":self.selected_rows.iter().map(|v|v.to_string()).collect::<Vec<_>>()}}));}
                }
                if let (Some(distance),Some(flux))=(self.result["distance_pixels"].as_array(),self.result["boxcar_net"].as_array()){
                    profile_plot(ui,distance,flux);
                    if let Some(fwhm)=self.result["fwhm_estimate_pixels"].as_f64(){ui.label(format!("{}: {fwhm:.3} {}",crate::i18n::t("Single-peak FWHM estimate"),crate::i18n::t("pixels (not a PSF fit)")));}
                }
                if ["export","math_preview"].contains(&self.operation.as_str()){ui.separator();egui::CollapsingHeader::new(crate::i18n::t("Annotations / export title")).id_salt("Annotations / export title").show(ui,|ui|{
                    ui.text_edit_singleline(&mut self.title);ui.text_edit_singleline(&mut self.annotation_text);
                    egui::ComboBox::new("annotation-anchor",crate::i18n::t("Text anchor")).selected_text(crate::i18n::t(&self.annotation_anchor)).show_ui(ui,|ui|{for name in ["Pixel","Sky","Canvas"]{ui.selectable_value(&mut self.annotation_anchor,name.into(),crate::i18n::t(name));}});
                    ui.horizontal(|ui|{ui.label(crate::i18n::t(match self.annotation_anchor.as_str(){"Sky"=>"RA / Dec [deg, FK5 J2000]","Canvas"=>"Viewport x / y [0–1]",_=>"x / y (one-based)"}));for value in &mut self.annotation_position{ui.add(egui::DragValue::new(value).speed(if self.annotation_anchor=="Canvas"{0.01}else{0.1}));}});
                    if ui.button(crate::i18n::t("Add text annotation")).clicked() && !self.annotation_text.is_empty() && self.annotations.len()<256 && self.annotation_position.iter().all(|v|v.is_finite() && v.abs()<=1e7){
                        let [x,y]=self.annotation_position;
                        let annotation=match self.annotation_anchor.as_str(){"Sky" if y.abs()<=90.=>Some(json!({"kind":"text","anchor":"sky","ra_deg":x,"dec_deg":y,"text":self.annotation_text})),"Canvas" if (0. ..=1.).contains(&x) && (0. ..=1.).contains(&y)=>Some(json!({"kind":"text","anchor":"canvas","x":x,"y":y,"text":self.annotation_text})),"Pixel"=>Some(json!({"kind":"text","anchor":"pixel","x":x-1.,"y":y-1.,"text":self.annotation_text})),_=>None};
                        if let Some(annotation)=annotation{self.annotations.push(annotation);self.annotations_dirty=true;}else{self.message="Invalid annotation coordinates".into();}
                    }
                    let mut remove=None;for (index,annotation) in self.annotations.iter_mut().enumerate(){ui.horizontal(|ui|{let mut text=annotation["text"].as_str().unwrap_or("").to_string();if ui.text_edit_singleline(&mut text).changed(){annotation["text"]=json!(text);}if ui.small_button("×").clicked(){remove=Some(index);}});}if let Some(index)=remove{self.annotations.remove(index);self.annotations_dirty=true;}
                    if ui.button(crate::i18n::t("Clear annotations")).clicked(){self.annotations.clear();self.annotations_dirty=true;}
                });}
            });
        if let (Some(analysis),Some(lines))=(&mut self.spectral_analysis,&self.spectral_lines){analysis.sync_context(lines);}
    }
    fn current_spectral_result(&self)->bool{
        self.spectral_analysis.as_ref().is_some_and(|analysis|self.result["source"]==analysis.source
            && self.result["revision"].as_u64()==Some(analysis.revision)
            && match analysis.task.as_str(){"Measure"=>self.result["action"]=="measure","Fit"=>self.result["action"]=="fit","Process"=>matches!(self.result["action"].as_str(),Some("smooth"|"continuum")),_=>false})
    }
    fn status_ui(&mut self,ui:&mut egui::Ui,ctx:&egui::Context,client:&Client,generation:u64){
        let current=self.operation!="spectral_analysis" || self.current_spectral_result();
        if self.busy{ui.horizontal_wrapped(|ui|{ui.spinner();ui.label(crate::i18n::t("Running…"));if self.download.saving{if ui.add_enabled(!self.download.cancel_pending,egui::Button::new(crate::i18n::t("Cancel"))).clicked(){self.download.cancel_pending=true;client.request(ctx,generation,"persist_cancel",json!({}));}}else if let Some(job)=&self.job{if ui.button(crate::i18n::t("Cancel")).clicked(){client.request(ctx,generation,"job_cancel",json!({"job_id":job}));}}});}
        if !self.message.is_empty() && !(self.busy && self.message.starts_with("Running ")) && (current || self.message!="Completed"){crate::controls::status_message(ui,&self.message);}
        if self.result_stale{ui.colored_label(Color32::YELLOW,crate::i18n::t("Parameters changed; displayed results belong to previous inputs."));}
        // A task switch must not present another task's status/warnings as its own.
        // 切换任务时保留原结果，但不把测量的状态/警告贴到尚未运行的拟合页面。
        if !current{return;}
        if let Some(status)=self.result["status"].as_str(){
            if status=="coverage_mismatch"{ui.colored_label(Color32::YELLOW,crate::i18n::t("Coverage mismatch: returned footprints do not overlap the requested field."));}
            else{ui.strong(format!("{}: {}",crate::i18n::t("Result status"),crate::i18n::t(status)));}
        }
        if let Some(count)=self.result["spatial_checks"]["outside_query"].as_u64().filter(|count|*count>0){ui.colored_label(Color32::YELLOW,format!("{}: {count}",crate::i18n::t("Products outside the requested field")));}
        ui.horizontal_wrapped(|ui|{for key in ["rows","visible_rows","used","matched","ambiguous","factor"]{if let Some(value)=self.result.get(key).filter(|v|v.is_number()){ui.label(format!("{}: {value}",crate::i18n::t(&key.replace('_'," "))));}}});
        for key in ["scientific_note","reason","warning"]{if let Some(note)=self.result[key].as_str(){ui.add(egui::Label::new(crate::i18n::t(note)).wrap());}}
        if let Some(warnings)=self.result["warnings"].as_array(){
            if self.operation=="spectral_analysis" && !warnings.is_empty(){
                egui::CollapsingHeader::new(format!("{} ({})",crate::i18n::t("Scientific warnings"),warnings.len())).id_salt("spectral-result-warnings").show(ui,|ui|{for warning in warnings.iter().filter_map(Value::as_str){ui.label(crate::i18n::t(warning));}});
            }else{for warning in warnings.iter().filter_map(Value::as_str){ui.label(crate::i18n::t(warning));}}
        }
    }
    pub fn take_open_notice(&mut self)->Option<String>{self.download.notice.take()}
    fn choose_product(&mut self,index:usize){
        let Some(product)=self.products.get(index)else{return;};self.product_row=Some(index);self.product_links=Value::Null;
        let region=&product["query_region"];self.download.cutout=json!({"use_current_field":true,"width":product["cutout_width"].as_u64().unwrap_or(512),"height":product["cutout_height"].as_u64().unwrap_or(512),"ra_deg":region["ra_deg"].as_f64().unwrap_or(0.),"dec_deg":region["dec_deg"].as_f64().unwrap_or(0.),"fov_deg":(2.*region["radius_deg"].as_f64().unwrap_or(0.01)).clamp(0.000001,3.)});
    }
    fn download_and_add(&mut self,ctx:&egui::Context,client:&Client,generation:u64,image:Option<&FitsImage>,view:&Value,mut params:Value){
        if let Some(image)=image{params["path"]=json!(image.info.path);params["hdu"]=json!(image.info.hdu);}
        if !view["wcs_override"].is_null(){params["wcs_override"]=view["wcs_override"].clone();}
        self.send(ctx,client,generation,"vo_download",params);
        self.download.source=image.map(|image|image.info.path.clone());self.download.persist=true;self.download.pending=None;self.download.saving=false;self.download.cancel_pending=false;self.auto_open_download=false;
    }
    fn archive_results_ui(&mut self,ui:&mut egui::Ui,ctx:&egui::Context,client:&Client,generation:u64,image:Option<&FitsImage>,view:&Value){
        use crate::i18n::t;
        if !self.products.is_empty(){ui.separator();ui.strong(format!("{} ({})",t("Retrieved archive products"),self.products.len()));}
        else if self.result["diagnostics"].is_object(){
            ui.add(egui::Label::new(t("No verified image product meets the current field limits in this bounded response. See result details or try another archive.")).wrap());
            ui.horizontal_wrapped(|ui|{for(service,label)in[("sdss_dr17","Search SDSS image products"),("mast","Search MAST image products")]{if ui.add_enabled(!self.busy && image.is_some(),egui::Button::new(t(label))).clicked(){let mut params=json!({"service_id":service,"use_image_center":true,"max_rows":20,"timeout":60.});if let Some(image)=image{params["path"]=json!(image.info.path);params["hdu"]=json!(image.info.hdu);}if !view["wcs_override"].is_null(){params["wcs_override"]=view["wcs_override"].clone();}self.send(ctx,client,generation,"archive_search",params);}}});
        }
        let mut selected=None;
        // Reserve the selected title, primary action and short summary below.
        // 给标题、主动作和简短天区摘要留空间；大面板列表可展开到560px。
        let list_height=(ui.clip_rect().height()-360.).clamp(128.,560.);
        if !self.products.is_empty(){egui::ScrollArea::vertical().id_salt("observation-products").min_scrolled_height(list_height).max_height(list_height).auto_shrink([false,false]).show(ui,|ui|{
            for (index,product) in self.products.iter().enumerate(){
                let label=format!("{} · {}",product["source"].as_str().unwrap_or(""),product["title"].as_str().or_else(||product["id"].as_str()).unwrap_or(""));
                if ui.selectable_label(self.product_row==Some(index),label).clicked(){selected=Some(index);}
            }
        });}
        if let Some(index)=selected{self.choose_product(index);}
        if let Some(product)=self.product_row.and_then(|index|self.products.get(index)).cloned(){
            let outside=product["coverage_check"]=="outside_query";
            let title=product["title"].as_str().or_else(||product["id"].as_str()).unwrap_or("");ui.add(egui::Label::new(egui::RichText::new(title).strong()).truncate()).on_hover_text(title);
            if product["download_kind"]=="hips_cutout" && product["cutout_supported"]==true{
                ui.horizontal_wrapped(|ui|{
                    let settings=&mut self.download.cutout;
                    for(key,label)in[("width","Cutout width (pixels)"),("height","Cutout height (pixels)")]{let mut value=settings[key].as_u64().unwrap_or(512) as u32;ui.label(t(label));if ui.add(egui::DragValue::new(&mut value).range(16..=4096)).changed(){settings[key]=json!(value);}}
                    information(ui,"cutout-options",|ui|{
                        let mut current=settings["use_current_field"].as_bool().unwrap_or(true);ui.checkbox(&mut current,t("Match current image footprint"));settings["use_current_field"]=json!(current);
                        if !current{for(key,label,min,max)in[("fov_deg","Cutout FOV (deg)",0.000001,3.),("ra_deg","Cutout center RA (deg)",0.,360.),("dec_deg","Cutout center Dec (deg)",-90.,90.)]{let mut value=settings[key].as_f64().unwrap_or(0.);ui.horizontal(|ui|{ui.label(t(label));if ui.add(egui::DragValue::new(&mut value).range(min..=max).speed(0.001)).changed(){settings[key]=json!(value);}});}}
                    });
                });
                if ui.add_enabled(!self.busy && !outside,egui::Button::new(t("Generate FITS cutout and add"))).clicked(){let mut cutout=self.download.cutout.clone();cutout["hips_id"]=product["hips_id"].clone();self.download_and_add(ctx,client,generation,image,view,json!({"hips_cutout":cutout,"expect_fits":true,"maximum_bytes":536870912,"timeout":120.}));}
                ui.add(egui::Label::new(t("HiPS images are resampled cutouts, not original exposures.")).wrap());
            }else if let Some(url)=product["access_url"].as_str().filter(|v|!v.is_empty()){
                if product["download_kind"]=="fits" && ui.add_enabled(!self.busy && !outside,egui::Button::new(t("Download and add to viewer"))).clicked(){self.download_and_add(ctx,client,generation,image,view,json!({"url":url,"access_format":product["access_format"],"expect_fits":true,"maximum_bytes":536870912}));}
                if product["download_kind"]=="datalink" && ui.add_enabled(!self.busy && !outside,egui::Button::new(t("Resolve scientific products"))).clicked(){self.send(ctx,client,generation,"vo_datalink",json!({"url":url,"max_rows":100}));}
            }
            if product["product_query_available"]==true && ui.add_enabled(!self.busy && !outside,egui::Button::new(t("List observation products"))).clicked(){
                let mut params=json!({"service_id":"mast","observation_id":product["observation_id"],"use_image_center":true,"max_rows":50,"timeout":60.});
                if let Some(image)=image{params["path"]=json!(image.info.path);params["hdu"]=json!(image.info.hdu);}if !view["wcs_override"].is_null(){params["wcs_override"]=view["wcs_override"].clone();}
                self.send(ctx,client,generation,"archive_search",params);
            }
            ui.horizontal_wrapped(|ui|{
                if let Some(ratio)=product["area_ratio"].as_f64(){ui.label(format!("{}: {:.1}%",t("Candidate area / current image area"),100.*ratio));}
                if let Some(overlap)=product["overlap_fraction"].as_f64(){ui.label(format!("{}: {:.1}%",t("Overlap / current image area"),100.*overlap));}
                ui.label(format!("{}: {}",t("Product size"),product["estimated_bytes"].as_u64().map(|size|format!("{:.2} MB",size as f64/1e6)).unwrap_or_else(||t("unknown").into())));
            });
            if product["area_selection"]=="partial_fallback_below_40_percent"{ui.add(egui::Label::new(t("Partial field fallback: no 40%-150% product in this bounded response.")).wrap());}
            if outside{ui.colored_label(Color32::YELLOW,t("This product footprint is outside the requested field."));}
            egui::CollapsingHeader::new(t("Product details")).id_salt("archive-product-details").show(ui,|ui|{
                if let (Some(ra),Some(dec),Some(radius))=(product["query_region"]["ra_deg"].as_f64(),product["query_region"]["dec_deg"].as_f64(),product["query_region"]["radius_deg"].as_f64()){ui.label(format!("{}: RA {ra:.6}° · Dec {dec:.6}° · r {radius:.5}°",t("Query field (ICRS)")));}
                for key in ["facility","instrument","collection","product_type","processing","band","time_start_mjd","time_end_mjd","semantics","cutout_note","discovery_geometry","footprint_source","copyright"]{if let Some(value)=product[key].as_str().filter(|v|!v.is_empty()){ui.add(egui::Label::new(format!("{}: {value}",t(&key.replace('_'," ")))).wrap());}}
                if let Some(url)=product["provenance_url"].as_str(){ui.hyperlink_to(t("Provenance"),url);}
                ui.add(egui::Label::new(t("Save beside original FITS when possible; otherwise open the temporary result.")).wrap());
            });
        }
        if let Some(links)=self.product_links["links"].as_array().cloned(){for link in links{
            if link["download_kind"]!="fits" || link["downloadable"]!=true{continue;}
            if let Some(url)=link["access_url"].as_str().filter(|s|!s.is_empty()){
                let title=link["description"].as_str().or_else(||link["semantics"].as_str()).unwrap_or("");ui.add(egui::Label::new(title).truncate()).on_hover_text(title);
                if ui.add_enabled(!self.busy,egui::Button::new(t("Download and add to viewer"))).clicked(){self.download_and_add(ctx,client,generation,image,view,json!({"url":url,"access_format":link["content_type"],"expect_fits":true,"maximum_bytes":536870912}));}
            }
        }}
        if let Some(artifact)=&self.artifact{ui.separator();ui.label(artifact["name"].as_str().unwrap_or(""));
            if let Some(id)=artifact["artifact_id"].as_str(){if let Some(url)=client.artifact_url(id){ui.hyperlink_to(t("Download / open result"),url);}if artifact["kind"]=="fits" && ui.add_enabled(!self.busy,egui::Button::new(t("Open generated FITS"))).clicked(){self.open_path=Some(artifact["_path"].as_str().map(str::to_owned).unwrap_or_else(||format!("artifact:{id}")));}}
        }
    }
    fn preview(&mut self,ctx:&egui::Context,client:&Client){
        if let Some(artifact)=&self.artifact {
            if artifact["kind"]=="png" {
                let id=artifact["artifact_id"].as_str().unwrap_or("");
                if !id.is_empty() && self.preview_id!=id {
                    self.preview_id=id.into();self.preview=None;self.preview_open=true;
                    #[cfg(not(target_arch="wasm32"))]
                    if let Some(path)=artifact["_path"].as_str(){*self.preview_pending.lock().unwrap()=Some(std::fs::read(path).map_err(|e|e.to_string()));}
                    if let Some(url)=client.artifact_url(id){let pending=self.preview_pending.clone();let ctx=ctx.clone();ehttp::fetch(ehttp::Request::get(url),move|response|{*pending.lock().unwrap()=Some(response.and_then(|r|if r.ok && r.bytes.len()<=64*1024*1024{Ok(r.bytes)}else{Err("Preview is unavailable or exceeds 64 MiB".into())}));ctx.request_repaint();});}
                }
            }
        }
        if let Some(result)=self.preview_pending.lock().unwrap().take(){match result {
            Ok(bytes)=>{
                let dimensions=bytes.get(16..24).filter(|_|bytes.starts_with(b"\x89PNG\r\n\x1a\n")).map(|b|(u32::from_be_bytes(b[..4].try_into().unwrap()),u32::from_be_bytes(b[4..].try_into().unwrap())));
                if dimensions.is_some_and(|(w,h)|w>0 && h>0 && u64::from(w)*u64::from(h)<=16*1024*1024){match image::load_from_memory(&bytes).map(|image|image.into_rgba8()){Ok(png)=>self.preview=Some(ctx.load_texture("pro-result",egui::ColorImage::from_rgba_unmultiplied([png.width() as usize,png.height() as usize],png.as_raw()),egui::TextureOptions::LINEAR)),Err(error)=>self.message=error.to_string(),}}else{self.message="Preview dimensions exceed 16 Mi pixels or are invalid".into();}
            },Err(error)=>self.message=error,
        }}

    }
    pub fn preview_ui(&mut self,ui:&mut egui::Ui){
        if let Some(texture)=&self.preview{let size=texture.size_vec2();let height=(ui.clip_rect().bottom()-ui.cursor().min.y).clamp(96.,1600.);let scale=(ui.available_width()/size.x).min(height/size.y).min(1.);ui.image((texture.id(),size*scale));}else if self.busy{ui.spinner();}else{ui.label(crate::i18n::t("No result preview yet"));}
    }
    pub fn overlay(&mut self,painter:&egui::Painter,transform:crate::image_transform::ImageTransform,_wcs:Option<&Value>,pointer:Option<Pos2>){
        let screen=|x:f64,y:f64|transform.pixel_to_screen([x,y]);
        for marker in &self.markers{if !self.marker_query.is_empty() && !marker["id"].to_string().contains(&self.marker_query){continue;}if let (Some(x),Some(y))=(marker["x"].as_f64(),marker["y"].as_f64()){
            let p=screen(x,y);let selected=marker["row"].as_u64().is_some_and(|v|self.selected_rows.contains(&(v as usize)));painter.circle_stroke(p,if selected{8.}else{5.},Stroke::new(if selected{2_f32}else{1_f32},if selected{Color32::YELLOW}else{Color32::LIGHT_GREEN}));
            if self.marker_labels || selected || pointer.is_some_and(|cursor|cursor.distance(p)<8.){painter.text(p,egui::Align2::LEFT_BOTTOM,format!("row {} · {}",marker["row"],marker["id"]),egui::FontId::proportional(11.),Color32::YELLOW);}
        }}
        for points in &self.lines{painter.add(egui::Shape::line(points.iter().map(|p|screen(p[0],p[1])).collect(),Stroke::new(1.5_f32,Color32::LIGHT_RED)));}
        for annotation in &self.annotations{if let (Some(x),Some(y),Some(text))=(annotation["x"].as_f64(),annotation["y"].as_f64(),annotation["text"].as_str()){let point=if annotation["anchor"]=="canvas"{let area=painter.clip_rect();Pos2::new(area.left()+x as f32*area.width(),area.bottom()-y as f32*area.height())}else{screen(x,y)};painter.text(point,egui::Align2::LEFT_BOTTOM,text,egui::FontId::proportional(14.),Color32::YELLOW);}}

    }
}
// Explanations are explicit popovers, while live results and errors stay visible.
// 说明文字按需展开；实时结果与错误仍直接显示。
pub(crate) fn information(ui:&mut egui::Ui,id:impl std::hash::Hash,body:impl FnOnce(&mut egui::Ui)){
    crate::workspace_ui::information(ui,id,body);
}
fn operation_hints(operation:&str)->&'static [&'static str]{match operation{
    "image_layers"|"image_flux_match"=>&[
        "Preview textures are limited to 1 Mi pixels per layer; measurements use the active original image.",
        "Layers: eye, order and opacity affect display only.",
        "The active original image is the reference canvas and measurement source.",
        "Use Layers to choose a source image and run brightness matching",
        "Matching radius is arcsec in WCS mode, pixels in aligned-pixel mode.",
        "Relative native-count scaling from matched stars; no absolute zero point or exposure correction."],
    "samp"=>&["This connects the selected environment to its local Standard Profile hub. For a remote server, run the optional local SAMP bridge."],
    "xmatch_cds"=>&["CDS receives only row identifiers and J2000 coordinates, plus any explicitly selected upload_columns."],
    "rgb"=>&["RGB is a display composite. Photometry uses the active original FITS, never RGB intensity."],
    "line_profile"=>&["Interpolated boxcar samples; band aperture total is measured separately. No wavelength calibration is applied."],
    "archive_search"=>&[
        "Area ratio = candidate celestial area / current image celestial area; actual overlap is checked separately.",
        "Image footprints use 32 WCS boundary samples. Complex fields remain unverified; conservative cones only discover candidates.",
        "Only 40%-150% image areas are preferred; smaller verified products appear only when this bounded response has no preferred candidate.",
        "HiPS images are resampled cutouts, not original exposures."],
    _=>&[],
}}
fn pair_xy(value:&Value)->Option<[f64;2]>{Some([value["x"].as_f64()?,value["y"].as_f64()?])}
fn pair(value:&Value)->Option<[f64;2]>{Some([value[0].as_f64()?,value[1].as_f64()?])}



fn profile_plot(ui:&mut egui::Ui,x:&[Value],y:&[Value]){
    let (response,painter)=ui.allocate_painter(egui::vec2(ui.available_width(),170.),egui::Sense::hover());let area=response.rect.shrink2(egui::vec2(12.,18.));
    let pairs=x.iter().zip(y).filter_map(|(x,y)|Some((x.as_f64()?,y.as_f64()?))).collect::<Vec<_>>();
    if pairs.len()<2{return;}let xmin=pairs.first().unwrap().0;let xmax=pairs.last().unwrap().0;let ymin=pairs.iter().map(|p|p.1).fold(f64::INFINITY,f64::min);let ymax=pairs.iter().map(|p|p.1).fold(f64::NEG_INFINITY,f64::max);
    if xmax<=xmin || ymax<=ymin{return;}let mut line=Vec::new();
    for (x,y) in x.iter().zip(y){if let (Some(x),Some(y))=(x.as_f64(),y.as_f64()){line.push(egui::pos2(area.left()+((x-xmin)/(xmax-xmin)) as f32*area.width(),area.bottom()-((y-ymin)/(ymax-ymin)) as f32*area.height()));}else if !line.is_empty(){painter.add(egui::Shape::line(std::mem::take(&mut line),Stroke::new(1.3_f32,Color32::LIGHT_BLUE)));}}
    if !line.is_empty(){painter.add(egui::Shape::line(line,Stroke::new(1.3_f32,Color32::LIGHT_BLUE)));}
    painter.text(area.left_bottom(),egui::Align2::LEFT_TOP,format!("Distance {xmin:.1}–{xmax:.1} px · net {ymin:.3e}–{ymax:.3e}"),egui::FontId::proportional(10.),Color32::GRAY);
}

#[cfg(test)]
mod context_tests{
    use super::*;
    fn spectrum()->FitsImage{
        let meta=fitsviewer_core::SpectrumMeta{x_label:"Wavelength".into(),x_unit:"Angstrom".into(),y_label:"Flux".into(),y_unit:String::new(),columns:vec![],x_column:0,y_column:1,error_column:None,error_hdu:None,uncertainty_kind:String::new(),note:String::new()};
        FitsImage{info:fitsviewer_core::ImageInfo{path:"context-test".into(),hdu:0,width:2,height:1,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:Some(meta)},
            pixels:fitsviewer_core::RawPixels::F64(vec![1.,2.]),spectrum:Some(fitsviewer_core::SpectrumData{x:vec![4000.,5000.],error:None})}
    }
    #[test]fn spectrum_context_resets_tools_and_preserves_shared_data(){
        let mut workspace=Workspace::default();assert!(workspace.spectral_lines.is_none());
        workspace.annotations.push(json!({"text":"preserved"}));let spectrum=spectrum();workspace.sync_spectrum(Some(&spectrum));
        assert_eq!(workspace.operation,"spectral_candidates");assert!(workspace.spectral_lines.is_some());
        workspace.select("samp");workspace.sync_spectrum(Some(&spectrum));assert_eq!(workspace.operation,"samp");
        workspace.select("detect_stars");workspace.sync_spectrum(Some(&spectrum));assert_eq!(workspace.operation,"spectral_candidates");
        workspace.sync_spectrum(None);assert_eq!(workspace.operation,"identify_stars");assert!(workspace.spectral_lines.is_none());assert_eq!(workspace.annotations.len(),1);
    }
    #[test]fn spectrum_operations_exclude_image_only_tools(){
        let operations=OPERATIONS.iter().filter(|(operation,_)|spectrum_operation(operation)).map(|(operation,_)|*operation).collect::<Vec<_>>();
        assert_eq!(operations,vec!["spectral_candidates","spectral_analysis","samp"]);
        for operation in ["detect_stars","catalogue","reproject","rgb","line_profile","export"]{assert!(!spectrum_operation(operation));}
    }
    #[test]fn switching_tools_preserves_inputs_and_never_relabels_old_results(){
        let mut workspace=Workspace::default();workspace.select("detect_stars");workspace.parameters=json!({"fwhm":7.}).to_string();workspace.result=json!({"sources":[{"id":"old"}]});workspace.message="Completed".into();
        workspace.select("rgb");assert!(workspace.result.is_null());assert!(workspace.message.is_empty());
        workspace.select("detect_stars");assert_eq!(serde_json::from_str::<Value>(&workspace.parameters).unwrap()["fwhm"],7.);
        workspace.busy=true;workspace.select("rgb");assert_eq!(workspace.operation,"detect_stars");
    }
    #[test]fn save_cancel_waits_for_original_result_and_late_cancel_cannot_undo_success(){
        let mut workspace=Workspace::default();workspace.busy=true;workspace.download.saving=true;workspace.download.cancel_pending=true;
        workspace.reply(Reply{generation:1,operation:"persist_cancel".into(),result:Ok(json!({"cancel_requested":true}))});
        assert!(workspace.busy);assert!(workspace.download.saving);assert!(!workspace.download.cancel_pending);assert!(workspace.open_path.is_none());
        workspace.reply(Reply{generation:1,operation:"persist_download".into(),result:Ok(json!({"persistent":true,"saved_path":"/owned/unique.fits"}))});
        assert!(!workspace.busy);assert!(!workspace.download.saving);assert_eq!(workspace.open_path.as_deref(),Some("/owned/unique.fits"));
        let message=workspace.message.clone();workspace.reply(Reply{generation:1,operation:"persist_cancel".into(),result:Ok(json!({"cancel_requested":false}))});
        assert_eq!(workspace.message,message);assert_eq!(workspace.open_path.as_deref(),Some("/owned/unique.fits"));
    }
    #[test]fn failed_download_cannot_save_a_later_reprojection(){
        for state in ["cancelled","failed","consumed","transport_error"]{
            let ctx=egui::Context::default();let client=Client::new(None,String::new());let mut workspace=Workspace::default();
            workspace.operation="archive_search".into();workspace.busy=true;workspace.job=Some("download-job".into());
            workspace.download.persist=true;workspace.download.source=Some("old-original.fits".into());workspace.download.cutout=json!({"width":128,"height":64});
            let result=if state=="transport_error"{Err("Download transport failed".into())}else{Ok(json!({"state":state,"error":"Download stopped"}))};
            workspace.reply(Reply{generation:1,operation:"job_status".into(),result});
            assert!(!workspace.busy);assert!(!workspace.download.persist);assert!(workspace.download.source.is_none());
            workspace.select("reproject");workspace.send(&ctx,&client,1,"reproject",json!({"path":"current.fits"}));
            workspace.reply(Reply{generation:1,operation:"job_start".into(),result:Ok(json!({"job_id":"reprojection-job"}))});
            workspace.reply(Reply{generation:1,operation:"job_status".into(),result:Ok(json!({"state":"completed","result":{"artifact_id":"reprojection-result","kind":"fits"}}))});
            assert!(workspace.artifact.is_some());assert!(workspace.download.pending.is_none());assert!(workspace.open_path.is_none());
            assert_eq!(workspace.download.cutout,json!({"width":128,"height":64}));
        }
    }
    #[test]fn fresh_pro_workspace_waits_before_heartbeat_even_in_an_old_iframe(){
        let ctx=egui::Context::default();let client=Client::new(None,String::new());let mut workspace=Workspace::default();
        let _=ctx.run(egui::RawInput{time:Some(120.),..Default::default()},|ctx|workspace.poll(ctx,&client,1));
        assert!(client.drain().is_empty());assert_eq!(workspace.heartbeat,120.);
        let _=ctx.run(egui::RawInput{time:Some(166.),..Default::default()},|ctx|workspace.poll(ctx,&client,1));
        let replies=client.drain();assert_eq!(replies.len(),1);assert_eq!(replies[0].operation,"pro_heartbeat");
    }
    #[test]fn changing_document_clears_chained_save_and_artifact_fallback_stays_explicit(){
        let mut workspace=Workspace::default();workspace.download.pending=Some(json!({"artifact_id":"old"}));workspace.download.source=Some("old.fits".into());workspace.busy=true;
        workspace.clear_document_results();assert!(workspace.download.pending.is_none());assert!(workspace.download.source.is_none());assert!(!workspace.busy);
        workspace.reply(Reply{generation:2,operation:"persist_download".into(),result:Ok(json!({"persistent":false,"path":"artifact:owned","reason":"Source is temporary; opening artifact only"}))});
        assert_eq!(workspace.open_path.as_deref(),Some("artifact:owned"));assert!(workspace.message.contains("artifact only"));assert!(workspace.take_open_notice().is_some());
    }
}
