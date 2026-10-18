//! PRO controls are instantiated only after selecting PRO.
//! PRO 模式才创建工作区、查询状态与叠加缓存。
use eframe::egui::{self,Color32,Pos2,Rect,Stroke};
use serde_json::{json,Value};
use crate::science_client::{Client,Reply};
use fitsviewer_core::{FitsImage,regions::{Aperture,Shape}};
use std::sync::{Arc,Mutex};
pub const OPERATIONS:&[(&str,&str)]=&[
    ("capabilities","Selected environment"),
    ("spectral_candidates","Spectral reference lines"),
    ("spectral_analysis","Spectral analysis"),
    ("image_layers","Layers"),("image_flux_match","Match image brightness"),
    ("detect_stars","Find stars"),("detect_lines","Detect straight traces"),("trace_curve","Track seeded curve"),
    ("catalogue","Catalogue overlay"),("xmatch","Local XMatch"),("xmatch_cds","CDS XMatch"),
    ("reproject","Reprojection"),("rgb","RGB composition"),("export","Figure export"),("math_preview","MathText preview"),
    ("line_profile","Line / trace profile"),("vo_query","VO query"),("vo_download","Download selected resource"),("samp","SAMP")];
pub fn spectrum_operation(operation:&str)->bool{["spectral_candidates","spectral_analysis","samp","capabilities"].contains(&operation)}
pub struct Workspace {
    pub operation:String,pub parameters:String,pub result:Value,pub message:String,pub busy:bool,
    pub markers:Vec<Value>,pub lines:Vec<Vec<[f64;2]>>,pub annotations:Vec<Value>,pub compass:bool,pub scale_arcsec:f64,
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
    pub image_layers:Option<Box<crate::image_layers::Renderer>>,pending_flux_match:Option<Value>,
}
impl Default for Workspace {
    fn default()->Self{Self{operation:"detect_stars".into(),parameters:"{}".into(),result:Value::Null,message:String::new(),busy:false,markers:vec![],lines:vec![],annotations:vec![],compass:false,scale_arcsec:10.,artifact:None,open_artifact:None,open_path:None,new_apertures:vec![],job:None,status_pending:false,last_poll:0.,heartbeat:-100.,table_row:None,annotation_text:String::new(),annotation_position:[1.,1.],title:String::new(),preview_id:String::new(),preview_pending:Arc::new(Mutex::new(None)),preview:None,preview_open:false,samp_remote:false,samp_connected:false,samp_after:0,last_samp:0.,pending_sky:None,focus_pixel:None,pending_download:None,auto_open_download:false,marker_query:String::new(),marker_labels:false,manual_marker:[1.,1.],selected_rows:vec![],samp_table_id:String::new(),pending_table:false,annotation_anchor:"Pixel".into(),annotations_dirty:true,annotation_revision:0,annotation_generation:0,annotation_wcs:Value::Null,spectral_lines:None,spectral_analysis:None,panel_width:0.,image_layers:None,pending_flux_match:None}}
}
fn defaults(operation:&str)->Value{match operation{
    "detect_stars"=>json!({"fwhm":3.,"threshold_sigma":5.,"min_separation":4.,"limit":2000,"mask_path":""}),
    "spectral_candidates"=>json!({"unit":"Select unit","method":"derivative","sigma":5.,"continuum_window":101}),
    "detect_lines"=>json!({"threshold_sigma":4.,"min_length":20,"gap":4,"votes":12,"width":5.,"seed":20261017}),
    "trace_curve"=>json!({"seed":[10.,30.],"axis":"x","direction":1,"search_half_width":8.,"centroid_half_width":3.,"order":3}),
    "catalogue"=>json!({"catalogue":"","ra_column":"ra","dec_column":"dec","ra_unit":"deg","dec_unit":"deg","frame":"icrs","marker_limit":5000}),
    "xmatch"=>json!({"first":{"catalogue":"","ra_column":"ra","dec_column":"dec","frame":"icrs"},"second":{"catalogue":"","ra_column":"ra","dec_column":"dec","frame":"icrs"},"radius_arcsec":1.,"strategy":"nearest"}),
    "xmatch_cds"=>json!({"catalogue":"","ra_column":"ra","dec_column":"dec","remote_catalogue":"vizier:I/355/gaiadr3","radius_arcsec":1.}),
    "reproject"=>json!({"projection":"TAN","ra_deg":0.,"dec_deg":0.,"scale_arcsec":1.,"rotation_deg":0.,"width":512,"height":512}),
    "rgb"=>json!({"channels":[{"path":"","weight":1.,"stretch":"Linear"},{"path":"","weight":1.,"stretch":"Linear"},{"path":"","weight":1.,"stretch":"Linear"}],"assume_aligned":false,"reproject":true}),
    "export"=>json!({"format":"png","width":1600,"height":1200,"dpi":150.,"wcs_axes":false,"colorbar":true,"compass":false}),
    "math_preview"=>json!({"expression":"$F_{\\lambda} / \\sigma$"}),
    "vo_query"=>json!({"protocol":"tap","service":"https://tapvizier.cds.unistra.fr/TAPVizieR/tap","adql":"SELECT TOP 10 * FROM \"I/355/gaiadr3\"","max_rows":100}),
    "vo_download"=>json!({"url":""}),
    "samp"=>json!({"action":"status"}),
    "line_profile"=>json!({"points":[[10.,10.],[50.,50.]],"width":5.,"background_gap":2.,"background_width":4.}),
    _=>json!({}),
}}
impl Workspace {
    pub fn sync_spectrum(&mut self,image:Option<&FitsImage>){
        let Some(image)=image.filter(|image|image.spectrum.is_some()) else{
            self.spectral_analysis=None;if self.spectral_lines.take().is_some(){self.select("detect_stars");}return;
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
    pub fn select(&mut self,operation:&str){self.operation=operation.into();self.parameters=serde_json::to_string_pretty(&defaults(operation)).unwrap_or_default();}
    pub fn select_analysis(&mut self,task:&str){self.select("spectral_analysis");if let Some(analysis)=&mut self.spectral_analysis{analysis.task=task.into();}}
    pub fn clear_document_results(&mut self){
        self.markers.clear();self.lines.clear();self.annotations.clear();self.job=None;self.busy=false;self.status_pending=false;self.samp_connected=false;self.result=Value::Null;self.message.clear();self.artifact=None;self.preview=None;self.preview_id.clear();self.pending_flux_match=None;
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
        self.busy=true;self.message=format!("Running {operation}…");self.job=None;self.status_pending=false;
        if ["reproject","rgb","export","vo_query","vo_download","detect_stars","detect_lines","trace_curve","xmatch_cds","spectral_candidates","spectral_analysis","image_flux_match"].contains(&operation){
            client.request(ctx,generation,"job_start",json!({"operation":operation,"params":params}));
        }else{client.request(ctx,generation,operation,params);}
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
        let now=ctx.input(|i|i.time);
        if now-self.heartbeat>45.{client.request(ctx,generation,"pro_heartbeat",json!({}));self.heartbeat=now;}
        if let Some(job)=&self.job {if !self.status_pending && now-self.last_poll>0.5{self.status_pending=true;client.request(ctx,generation,"job_status",json!({"job_id":job}));self.last_poll=now;}ctx.request_repaint_after(std::time::Duration::from_millis(500));}
        if self.samp_connected && now-self.last_samp>1.{client.request(ctx,generation,if self.samp_remote{"samp_bridge"}else{"samp"},json!({"action":"events","after":self.samp_after}));self.last_samp=now;ctx.request_repaint_after(std::time::Duration::from_millis(1000));}
        ctx.request_repaint_after(std::time::Duration::from_secs(40));
    }
    pub fn reply(&mut self,reply:Reply){
        if reply.operation=="job_status"{self.status_pending=false;if self.job.is_none(){return;}}
        if ["pro_acquire","pro_heartbeat","release_pro","release_view"].contains(&reply.operation.as_str()){return;}
        if reply.operation=="annotation_positions"{
            match reply.result{Ok(value)=>{if value["request_id"].as_u64()==Some(self.annotation_revision) && !self.annotations_dirty{if let (Some(indices),Some(pixels))=(value["indices"].as_array(),value["pixels"].as_array()){for (index,pixel) in indices.iter().zip(pixels){if let Some(annotation)=index.as_u64().and_then(|i|self.annotations.get_mut(i as usize)){annotation["x"]=pixel[0].clone();annotation["y"]=pixel[1].clone();}}}}},Err(error)=>self.message=error,}return;
        }
        match reply.result {
            Err(error)=>{self.busy=false;self.job=None;self.message=error;},
            Ok(value)=>{
                if reply.operation=="sky_point"{self.focus_pixel=pair(&value["pixel"]);return;}
                if reply.operation=="samp" || reply.operation=="samp_bridge"{
                    if let Some(connected)=value["connected"].as_bool(){self.samp_connected=connected;}
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
                        "consumed"=>{self.job=None;self.busy=false;self.message="Task result expired; run the task again".into();return;},
                        _=>{self.busy=false;self.job=None;self.message=value["error"].as_str().unwrap_or("Job cancelled").into();return;},
                    }
                }else{value};
                self.busy=false;self.message="Completed".into();
                if value["artifact_available"]==false{self.artifact=None;self.message=value["artifact_error"].as_str().unwrap_or("Task result expired; run the task again").into();self.result=value;return;}
                if value["operation"]=="image_flux_match"{self.pending_flux_match=Some(value);return;}
                if value["operation"]=="spectral_candidates"{if let Some(lines)=&mut self.spectral_lines{lines.accept(&value);}}
                if value["operation"]=="spectral_analysis" && !self.spectral_analysis.as_mut().is_some_and(|analysis|analysis.accept(&value)){self.message="Discarded stale spectral analysis result".into();return;}
                if let Some(markers)=value["markers"].as_array().or_else(||value["sources"].as_array()){self.markers=markers.clone();}
                if let Some(lines)=value["lines"].as_array(){self.lines=lines.iter().filter_map(|l|Some(vec![pair(&l["start"])?,pair(&l["end"])?])).collect();}
                if let Some(points)=value["points"].as_array(){let points=points.iter().filter_map(pair).collect::<Vec<_>>();if points.len()>1{self.lines=vec![points];}}
                if value["artifact_id"].is_string(){self.artifact=Some(value.clone());if self.auto_open_download && (value["kind"]=="fits" || value["name"].as_str().is_some_and(|n|[".txt",".dat",".csv"].iter().any(|suffix|n.ends_with(suffix)))){self.open_path=Some(value["_path"].as_str().map(str::to_string).unwrap_or_else(||format!("artifact:{}",value["artifact_id"].as_str().unwrap())));self.auto_open_download=false;}}
                if self.pending_table && value["artifact_id"].is_string(){self.pending_table=false;self.select("catalogue");self.parameters=json!({"catalogue":value["_path"].as_str().map(str::to_string).unwrap_or_else(||format!("artifact:{}",value["artifact_id"].as_str().unwrap()))}).to_string();self.message="SAMP table received. Run Catalogue overlay to inspect its coordinate columns.".into();}
                self.result=value;
            }
        }
    }
    pub fn tick(&mut self,ctx:&egui::Context,client:&Client,generation:u64,image:Option<&FitsImage>,view:&Value,multi:&mut crate::multiview::Workspace){
        self.sync_spectrum(image);
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
            if image.is_none_or(|image|image.spectrum.is_none()){
                ui.horizontal_wrapped(|ui|{ui.checkbox(&mut self.compass,crate::i18n::t("N / E"));ui.label(crate::i18n::t("Scale [arcsec]"));ui.add(egui::DragValue::new(&mut self.scale_arcsec).range(0.001..=36000.));});
            }
            let old=self.operation.clone();
            ui.horizontal(|ui|{egui::ComboBox::from_id_salt("pro_operation").width((ui.available_width()-32.).max(80.)).selected_text(OPERATIONS.iter().find(|(o,_)|*o==self.operation).map(|(_,n)|crate::i18n::t(n)).unwrap_or(&self.operation)).show_ui(ui,|ui|{for (operation,label) in OPERATIONS{
                if (spectrum && !spectrum_operation(operation)) || (!spectrum && ["spectral_candidates","spectral_analysis"].contains(operation)){continue;}
                ui.selectable_value(&mut self.operation,operation.to_string(),crate::i18n::t(label));
            }});information(ui,"operation-info",|ui|{
                ui.label(crate::i18n::t("Drag the left edge to resize; Work area restores this panel."));
                for text in operation_hints(&self.operation){ui.label(crate::i18n::t(text));}
            });});
            if old!=self.operation || self.parameters=="{}"{let operation=self.operation.clone();self.select(&operation);}
            ui.scope(|ui|{
                if ["image_layers","image_flux_match"].contains(&self.operation.as_str()){
                    if let Some(mut params)=multi.image_layers_ui(ui,image,self.busy){if !view["wcs_override"].is_null(){params["reference_wcs_override"]=view["wcs_override"].clone();}self.send(ctx,client,generation,"image_flux_match",params);}
                    if let Some(job)=&self.job{if ui.button(crate::i18n::t("Cancel")).clicked(){client.request(ctx,generation,"job_cancel",json!({"job_id":job}));}}
                    if self.message!="Use Layers to choose a source image and run brightness matching"{ui.label(crate::i18n::t(&self.message));}
                    if ui.button(crate::i18n::t("Copy result")).clicked(){ctx.copy_text(serde_json::to_string_pretty(&self.result).unwrap_or_default());}return;
                }
                if self.operation=="spectral_analysis"{
                    let request=if let (Some(analysis),Some(lines),Some(image))=(&mut self.spectral_analysis,&mut self.spectral_lines,image){
                        analysis.ui(ui,image,lines,self.busy,multi).map(|action|analysis.request(&action,image,lines))
                    }else{None};
                    if let Some(request)=request{match request{Ok(params)=>self.send(ctx,client,generation,"spectral_analysis",params),Err(error)=>self.message=error,}}
                    if let Some(job)=&self.job{if ui.button(crate::i18n::t("Cancel")).clicked(){client.request(ctx,generation,"job_cancel",json!({"job_id":job}));}}
                    ui.label(crate::i18n::t(&self.message));
                    if ui.button(crate::i18n::t("Copy result")).clicked(){ctx.copy_text(serde_json::to_string_pretty(&self.result).unwrap_or_default());}return;
                }
                if self.operation=="spectral_candidates"{
                    let request=if let Some(lines)=&mut self.spectral_lines{
                        if lines.ui(ui,self.busy){image.map(|image|lines.request(image))}else{None}
                    }else{ui.label(crate::i18n::t("Open a spectrum first"));None};
                    if let Some(request)=request{match request{Ok(params)=>self.send(ctx,client,generation,"spectral_candidates",params),Err(error)=>self.message=error,}}
                    if let Some(job)=&self.job{if ui.button(crate::i18n::t("Cancel")).clicked(){client.request(ctx,generation,"job_cancel",json!({"job_id":job}));}}
                    ui.label(crate::i18n::t(&self.message));
                    if ui.button(crate::i18n::t("Copy result")).clicked(){ctx.copy_text(serde_json::to_string_pretty(&self.result).unwrap_or_default());}
                    return;
                }
                if self.operation=="samp"{
                    ui.horizontal(|ui|{ui.checkbox(&mut self.samp_remote,crate::i18n::t("Use local bridge for remote Jupyter"));
                        if spectrum{information(ui,"samp-info",|ui|{for text in operation_hints("samp"){ui.label(crate::i18n::t(text));}});}
                    });
                    ui.horizontal_wrapped(|ui|{for action in ["connect","status","events","disconnect"]{if ui.add_enabled(!self.busy,egui::Button::new(crate::i18n::t(action))).clicked(){client.request(ctx,generation,if self.samp_remote{"samp_bridge"}else{"samp"},json!({"action":action,"after":self.samp_after}));}}});
                    if ui.button(crate::i18n::t("Send active FITS to Hub clients")).clicked(){if let Some(image)=image{client.request(ctx,generation,if self.samp_remote{"samp_bridge"}else{"samp"},json!({"action":"send","mtype":if image.spectrum.is_some(){"spectrum.load.ssa-generic"}else{"image.load.fits"},"path":image.info.path,"params":{"name":image.info.path}}));}}
                }
                if self.operation=="catalogue"{
                    if let Ok(parameters)=serde_json::from_str::<Value>(&self.parameters){if let Some(path)=parameters["catalogue"].as_str().filter(|p|!p.is_empty()){
                        if ui.add_enabled(self.samp_connected,egui::Button::new(crate::i18n::t("Send current table to SAMP"))).clicked(){self.samp_table_id=path.into();client.request(ctx,generation,if self.samp_remote{"samp_bridge"}else{"samp"},json!({"action":"send","mtype":if path.ends_with(".fits"){"table.load.fits"}else{"table.load.votable"},"path":path,"params":{"table-id":path,"name":path}}));}
                    }}
                }
                if ["detect_stars","detect_lines","trace_curve"].contains(&self.operation.as_str()) && ui.button(crate::i18n::t("Use visible image as detection ROI")).clicked(){if let (Some(image),Some(bounds))=(image,view["crop"].as_array()){let mut parameters=serde_json::from_str::<Value>(&self.parameters).unwrap_or(json!({}));parameters["roi"]=json!([bounds[0].as_f64().unwrap_or(0.).floor().max(0.) as usize,bounds[1].as_f64().unwrap_or(0.).floor().max(0.) as usize,bounds[2].as_f64().unwrap_or(image.info.width as f64).ceil().min(image.info.width as f64) as usize,bounds[3].as_f64().unwrap_or(image.info.height as f64).ceil().min(image.info.height as f64) as usize]);self.parameters=parameters.to_string();}}
                // Common scalar controls plus a lossless editor for advanced protocol fields.
                // 常用参数提供直接控件，高级字段保留无损 JSON 编辑。
                if let Ok(mut params)=serde_json::from_str::<Value>(&self.parameters){if parameter_controls(ui,"",&mut params,0){self.parameters=serde_json::to_string_pretty(&params).unwrap_or_default();}}
                egui::CollapsingHeader::new(crate::i18n::t("Advanced parameters / reproducible JSON")).id_salt("Advanced parameters / reproducible JSON").show(ui,|ui|{ui.add(egui::TextEdit::multiline(&mut self.parameters).code_editor().desired_rows(10));});
                ui.horizontal(|ui|{
                    if ui.add_enabled(!self.busy,egui::Button::new(crate::i18n::t("Run"))).clicked(){self.submit(ctx,client,generation,image,view);}
                    if let Some(job)=&self.job{if ui.button(crate::i18n::t("Cancel")).clicked(){client.request(ctx,generation,"job_cancel",json!({"job_id":job}));}}
                    if ui.button(crate::i18n::t("Copy result")).clicked(){ctx.copy_text(serde_json::to_string_pretty(&self.result).unwrap_or_default());}
                });
                ui.label(if let Some(operation)=self.message.strip_prefix("Running ").and_then(|s|s.strip_suffix("…")){format!("{} {}…",crate::i18n::t("Running"),OPERATIONS.iter().find(|(o,_)|*o==operation).map(|(_,label)|crate::i18n::t(label)).unwrap_or(operation))}else{crate::i18n::t(&self.message).into()});
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
                if !self.lines.is_empty() && ui.button(crate::i18n::t("Create line / curve apertures")).clicked(){for (i,points) in self.lines.iter().take(64).enumerate(){
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
                egui::CollapsingHeader::new(crate::i18n::t("Result details")).id_salt("Result details").show(ui,|ui|{let text=serde_json::to_string_pretty(&self.result).unwrap_or_default();ui.add(egui::Label::new(if text.len()>32000{format!("{}\n… copy result for complete data",&text[..text.floor_char_boundary(32000)])}else{text}).wrap());});
                ui.separator();egui::CollapsingHeader::new(crate::i18n::t("Annotations / export title")).id_salt("Annotations / export title").show(ui,|ui|{
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
                });
            });
        if let (Some(analysis),Some(lines))=(&mut self.spectral_analysis,&self.spectral_lines){analysis.sync_context(lines);}
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
    pub fn overlay(&mut self,painter:&egui::Painter,rect:Rect,zoom:f32,wcs:Option<&Value>,pointer:Option<Pos2>){
        let screen=|x:f64,y:f64|Pos2::new(rect.left()+(x as f32+0.5)*zoom,rect.bottom()-(y as f32+0.5)*zoom);
        for marker in &self.markers{if !self.marker_query.is_empty() && !marker["id"].to_string().contains(&self.marker_query){continue;}if let (Some(x),Some(y))=(marker["x"].as_f64(),marker["y"].as_f64()){
            let p=screen(x,y);let selected=marker["row"].as_u64().is_some_and(|v|self.selected_rows.contains(&(v as usize)));painter.circle_stroke(p,if selected{8.}else{5.},Stroke::new(if selected{2_f32}else{1_f32},if selected{Color32::YELLOW}else{Color32::LIGHT_GREEN}));
            if self.marker_labels || selected || pointer.is_some_and(|cursor|cursor.distance(p)<8.){painter.text(p,egui::Align2::LEFT_BOTTOM,format!("row {} · {}",marker["row"],marker["id"]),egui::FontId::proportional(11.),Color32::YELLOW);}
        }}
        for points in &self.lines{painter.add(egui::Shape::line(points.iter().map(|p|screen(p[0],p[1])).collect(),Stroke::new(1.5_f32,Color32::LIGHT_RED)));}
        for annotation in &self.annotations{if let (Some(x),Some(y),Some(text))=(annotation["x"].as_f64(),annotation["y"].as_f64(),annotation["text"].as_str()){let point=if annotation["anchor"]=="canvas"{let area=painter.clip_rect();Pos2::new(area.left()+x as f32*area.width(),area.bottom()-y as f32*area.height())}else{screen(x,y)};painter.text(point,egui::Align2::LEFT_BOTTOM,text,egui::FontId::proportional(14.),Color32::YELLOW);}}
        if let Some(wcs)=wcs {let compass=&wcs["compass"];let base=Pos2::new(painter.clip_rect().left()+55.,painter.clip_rect().bottom()-55.);
            if self.compass {for (name,key) in [("N","north"),("E","east")]{if let Some(v)=pair(&compass[key]){let vector=egui::vec2(v[0] as f32,-v[1] as f32).normalized()*30.;painter.arrow(base,vector,Stroke::new(1.4_f32,Color32::YELLOW));painter.text(base+vector,egui::Align2::CENTER_BOTTOM,name,egui::FontId::proportional(12.),Color32::YELLOW);}}}
            if let Some(scale)=compass["arcsec_per_pixel_x"].as_f64().filter(|s|*s>0.){let length=(self.scale_arcsec/scale) as f32*zoom;if length<rect.width()*0.8{let base=base+egui::vec2(55.,25.);painter.line_segment([base,base+egui::vec2(length,0.)],Stroke::new(2.0_f32,Color32::YELLOW));painter.text(base,egui::Align2::LEFT_BOTTOM,format!("{} arcsec (local)",self.scale_arcsec),egui::FontId::proportional(11.),Color32::YELLOW);}}
        }
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
    _=>&[],
}}
fn pair_xy(value:&Value)->Option<[f64;2]>{Some([value["x"].as_f64()?,value["y"].as_f64()?])}
fn pair(value:&Value)->Option<[f64;2]>{Some([value[0].as_f64()?,value[1].as_f64()?])}



// Keep nested catalogue/RGB settings usable without requiring JSON editing.
// 星表和 RGB 的嵌套参数提供常规控件，不要求用户手写 JSON。
fn parameter_controls(ui:&mut egui::Ui,key:&str,value:&mut Value,depth:usize)->bool{
    if depth>5 || key.starts_with('_'){return false;}let mut changed=false;let label=crate::i18n::t(&key.replace('_'," ")).to_string();
    match value {
        Value::Object(object)=>{
            let mut show=|ui:&mut egui::Ui|{let mut changed=false;
                egui::Grid::new(("tool-parameters",key,depth)).num_columns(2).spacing(egui::vec2(8.,6.)).show(ui,|ui|{
                    for (key,value) in object.iter_mut().filter(|(key,value)|(*key!="path" || depth>0) && (value.is_string() || value.is_boolean() || value.is_number())){
                        ui.label(crate::i18n::t(&key.replace('_'," ")));changed|=scalar_control(ui,value);ui.end_row();
                    }
                });
                for (key,value) in object.iter_mut().filter(|(_,value)|value.is_object() || value.is_array()){changed|=parameter_controls(ui,key,value,depth+1);}changed};
            if depth==0{changed=show(ui);}else{egui::CollapsingHeader::new(&label).id_salt(key).show(ui,|ui|{changed=show(ui);});}
        },
        Value::Array(items) if items.len()<=8=>{egui::CollapsingHeader::new(format!("{label} ({} {})",items.len(),crate::i18n::t("entries"))).id_salt(key).show(ui,|ui|{for (index,item) in items.iter_mut().enumerate(){changed|=parameter_controls(ui,&index.to_string(),item,depth+1);}});},
        Value::String(text)=>{ui.horizontal(|ui|{ui.label(crate::i18n::t(&label));changed=ui.text_edit_singleline(text).changed();});},
        Value::Bool(value)=>changed=ui.checkbox(value,crate::i18n::t(&label)).changed(),
        Value::Number(number)=>{let mut n=number.as_f64().unwrap_or(0.);let integer=number.is_i64() || number.is_u64();ui.horizontal(|ui|{ui.label(crate::i18n::t(&label));if ui.add(egui::DragValue::new(&mut n).speed(if integer{1.}else{0.1})).changed(){*value=if integer{json!(n.round() as i64)}else{json!(n)};changed=true;}});},
        _=>{},
    }changed
}

fn scalar_control(ui:&mut egui::Ui,value:&mut Value)->bool{match value{
    Value::String(text)=>ui.add(egui::TextEdit::singleline(text).desired_width(110.)).changed(),
    Value::Bool(enabled)=>ui.checkbox(enabled,"").changed(),
    Value::Number(number)=>{let integer=number.is_i64() || number.is_u64();let mut n=number.as_f64().unwrap_or(0.);let changed=ui.add(egui::DragValue::new(&mut n).speed(if integer{1.}else{0.1})).changed();if changed{*value=if integer{json!(n.round() as i64)}else{json!(n)};}changed},
    _=>false,
}}


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
        workspace.sync_spectrum(None);assert_eq!(workspace.operation,"detect_stars");assert!(workspace.spectral_lines.is_none());assert_eq!(workspace.annotations.len(),1);
    }
    #[test]fn spectrum_operations_exclude_image_only_tools(){
        let operations=OPERATIONS.iter().filter(|(operation,_)|spectrum_operation(operation)).map(|(operation,_)|*operation).collect::<Vec<_>>();
        assert_eq!(operations,vec!["capabilities","spectral_candidates","spectral_analysis","samp"]);
        for operation in ["detect_stars","catalogue","reproject","rgb","line_profile","export"]{assert!(!spectrum_operation(operation));}
    }
}
