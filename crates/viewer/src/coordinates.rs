//! Shared pixel/J2000 and Gaia controls, available in Minimal and PRO.
//! 极简与 PRO 共用坐标面板；Gaia 只在用户点击后调用外部环境。
use eframe::egui;
use fitsviewer_core::FitsImage;
use serde_json::{json,Value};
use crate::{i18n::t,science_client::{Client,Reply}};

pub struct Coordinates {
    pub open:bool,pub busy:bool,pub result:Value,pub solution:Option<String>,pub use_solution:bool,pub changed:bool,
    pub job:Option<String>,pub message:String,pub key:String,
    ra:String,dec:String,scale:String,rotation:f64,tolerance:f64,catalogue:String,last_poll:f64,heartbeat:f64,lease:bool,status_pending:bool,generation:u64,run:u64,source_hdu:Option<usize>,active_tolerance:f64,
}
impl Default for Coordinates {fn default()->Self{Self{open:false,busy:false,result:Value::Null,solution:None,use_solution:false,changed:false,job:None,message:String::new(),key:String::new(),ra:String::new(),dec:String::new(),scale:String::new(),rotation:0.,tolerance:3.,catalogue:String::new(),last_poll:-100.,heartbeat:-100.,lease:false,status_pending:false,generation:0,run:0,source_hdu:None,active_tolerance:3.}}}
impl Coordinates {
    pub fn header(&self)->Option<&str>{self.use_solution.then_some(self.solution.as_deref()).flatten().filter(|header|valid_header(header))}
    pub fn cancel(&mut self,ctx:&egui::Context,client:&Client,_generation:u64){
        let generation=self.generation;
        let active=self.busy || self.lease || self.job.is_some();
        if let Some(job)=self.job.take(){client.request_labeled(ctx,generation,"job_cancel","gaia.cancel",json!({"job_id":job}));}
        if active{client.request_labeled(ctx,generation,"gaia_release","gaia.release",json!({}));}self.busy=false;self.lease=false;self.status_pending=false;self.run=self.run.saturating_add(1);
    }
    pub fn bind(&mut self,ctx:&egui::Context,client:&Client,generation:u64,key:String,saved:Option<String>){
        if self.key==key && self.generation==generation{return;}self.cancel(ctx,client,generation);let open=self.open;let run=self.run;*self=Self::default();self.open=open;self.run=run;self.key=key;self.generation=generation;
        if let Some(header)=saved.filter(|header|valid_header(header)){self.solution=Some(header);self.use_solution=true;self.changed=true;self.lease=true;self.heartbeat=ctx.input(|i|i.time);client.request_labeled(ctx,generation,"gaia_acquire","gaia.acquire",json!({}));}
    }
    pub fn start(&mut self,ctx:&egui::Context,client:&Client,generation:u64,image:&FitsImage,mut params:Value)->Result<(),String>{
        if self.busy{return Err("Gaia calibration is already running".into());}
        if image.spectrum.is_some(){return Err("Gaia calibration needs a two-dimensional stellar image".into());}
        if !params.is_object(){params=json!({});}
        let tolerance=if params["max_residual_arcsec"].is_null(){3.}else{params["max_residual_arcsec"].as_f64().ok_or("Maximum residual must be between 0.02 and 10 arcsec")?};
        if !tolerance.is_finite() || !(0.02..=10.).contains(&tolerance){return Err("Maximum residual must be between 0.02 and 10 arcsec".into());}
        params["max_residual_arcsec"]=json!(tolerance);
        params["path"]=json!(image.info.path);params["hdu"]=json!(image.info.hdu);
        params["_coordinate_key"]=json!(self.key);
        if let Some(header)=self.header(){params["wcs_override"]=json!(header);}
        self.generation=generation;self.run=self.run.saturating_add(1);self.source_hdu=Some(image.info.hdu);self.active_tolerance=tolerance;self.open=true;self.busy=true;self.lease=true;self.heartbeat=ctx.input(|i|i.time);self.status_pending=false;self.message="Running Gaia calibration…".into();self.result=Value::Null;
        client.request_labeled(ctx,generation,"job_start",&format!("gaia.start.{}",self.run),json!({"operation":"gaia_solve","params":params}));Ok(())
    }
    pub fn reply(&mut self,ctx:&egui::Context,client:&Client,generation:u64,reply:Reply){
        if ["gaia.acquire","gaia.release","gaia.cancel","gaia.heartbeat"].contains(&reply.operation.as_str()){return;}
        if reply.generation!=self.generation || generation!=self.generation || !self.busy{return;}
        let start=reply.operation==format!("gaia.start.{}",self.run);
        let status=reply.operation==format!("gaia.status.{}",self.run);
        if !start && !status{return;}
        if status{self.status_pending=false;}
        match reply.result{
            Err(error)=>{self.message=error;self.busy=false;self.job=None;},
            Ok(value)=>{
                if start{self.job=value["job_id"].as_str().filter(|id|!id.is_empty()).map(str::to_string);if self.job.is_none(){self.busy=false;self.message="Gaia calibration did not return a job identifier".into();self.cancel(ctx,client,generation);}return;}
                match value["state"].as_str().unwrap_or(""){
                    "queued"|"running"=>return,
                    "completed"=>{self.result=value["result"].clone();if self.result["accepted"]==true && valid_solution(&self.result,&self.key,self.source_hdu,self.generation,self.active_tolerance){self.solution=self.result["solution_header"].as_str().map(str::to_string);self.use_solution=true;self.changed=true;self.message="Gaia solution validated on independent stars".into();}else if self.result["accepted"]==true{self.result["accepted"]=json!(false);self.message="Gaia solution does not match the current image or failed validation".into();self.result["reason"]=json!(self.message);}else{self.message=self.result["reason"].as_str().unwrap_or("Approximate pointing is required").into();}},
                    _=>self.message=value["error"].as_str().unwrap_or("Gaia calibration cancelled").into(),
                }self.busy=false;self.job=None;
            },
        }
        if self.result["accepted"]!=true{self.lease=false;client.request_labeled(ctx,generation,"gaia_release","gaia.release",json!({}));}
    }
    pub fn tick(&mut self,ctx:&egui::Context,client:&Client,generation:u64){
        let now=ctx.input(|i|i.time);
        if self.lease && now-self.heartbeat>30. && self.job.is_some(){client.request_labeled(ctx,generation,"gaia_heartbeat","gaia.heartbeat",json!({}));self.heartbeat=now;}
        if self.lease && !self.busy{if now-self.heartbeat>30.{client.request_labeled(ctx,generation,"gaia_heartbeat","gaia.heartbeat",json!({}));self.heartbeat=now;}ctx.request_repaint_after(std::time::Duration::from_secs(30));}
        if self.busy{
            if let Some(job)=&self.job{if !self.status_pending && now-self.last_poll>0.5{self.status_pending=true;client.request_labeled(ctx,generation,"job_status",&format!("gaia.status.{}",self.run),json!({"job_id":job}));self.last_poll=now;}}
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }
    pub fn panel(&mut self,ctx:&egui::Context,client:&Client,generation:u64,image:Option<&FitsImage>,wcs:Option<&Value>,cursor:Option<[f64;2]>,available:bool,windows:&mut crate::window_layout::Runtime){
        self.tick(ctx,client,generation);
        if !self.open{return;}let mut open=self.open;
        windows.show(ctx,crate::window_layout::Key::Coordinates,t("Coordinates"),&mut open,egui::vec2(460.,250.),|ui|self.ui(ui,ctx,client,generation,image,wcs,cursor,available));self.open=open;
    }
    pub fn ui(&mut self,ui:&mut egui::Ui,ctx:&egui::Context,client:&Client,generation:u64,image:Option<&FitsImage>,wcs:Option<&Value>,cursor:Option<[f64;2]>,available:bool){
        crate::ribbon::align_controls(ui);
            ui.heading(t("Pixel / J2000 coordinates"));
            let pixel=image.map(|image|cursor.unwrap_or([(image.info.width as f64-1.)/2.,(image.info.height as f64-1.)/2.]));
            if let Some(pixel)=pixel{ui.label(format!("{}: x={:.3}  y={:.3}  ({})",t(if cursor.is_some(){"Cursor"}else{"Image center"}),pixel[0]+1.,pixel[1]+1.,t("one-based")));}
            if let Some(point)=pixel.and_then(|pixel|wcs.and_then(|value|crate::wcs_cache::point_at(value,pixel))){
                if let (Some(x),Some(y))=(point["pixel"][0].as_f64(),point["pixel"][1].as_f64()){ui.weak(format!("J2000 @ x={:.3}, y={:.3}",x+1.,y+1.));}
                if let Some(sexagesimal)=point["sexagesimal"].as_str(){ui.label(format!("FK5 J2000: {sexagesimal}"));if ui.button(t("Copy J2000")).clicked(){ctx.copy_text(sexagesimal.into());}}
                if let (Some(ra),Some(dec))=(point["ra_deg"].as_f64(),point["dec_deg"].as_f64()){ui.label(format!("RA={ra:.10}°  Dec={dec:.10}°"));if ui.button(t("Copy decimal degrees")).clicked(){ctx.copy_text(format!("{ra:.10} {dec:.10}"));}}
            }else{ui.weak(t("Move over the image, or use the image center. No usable WCS is available yet."));}
            if self.solution.is_some(){if ui.checkbox(&mut self.use_solution,t("Use Gaia WCS for coordinates and overlays")).changed(){self.changed=true;}ui.weak(t("The Gaia WCS is saved in this view/session; exact cursor samples are cached in memory."));}
            ui.separator();ui.horizontal_wrapped(|ui|{ui.strong(t("Gaia coordinate recognition / calibration"));crate::controls::information(ui,"gaia-help",|ui|{
                ui.label(t("Uses the selected Python environment. Only the search field is sent to Gaia; images stay local."));
                ui.label(t("Leave blank to use the FITS WCS or RA/DEC and PIXSCALE metadata. No all-sky blind index is bundled."));
            });});
            let eligible=available && image.is_some_and(|i|i.spectrum.is_none());
            if ui.add_enabled(eligible && !self.busy,egui::Button::new(t("Recognize / calibrate with Gaia"))).clicked(){
                let mut params=json!({"rotation_deg":self.rotation,"max_residual_arcsec":self.tolerance});
                let mut valid=true;for (key,text) in [("ra_deg",&self.ra),("dec_deg",&self.dec),("scale_arcsec",&self.scale)]{if !text.trim().is_empty(){match text.trim().parse::<f64>(){Ok(value) if value.is_finite()=>params[key]=json!(value),_=>{self.message="Approximate coordinates and scale must be finite numbers".into();valid=false;}}}}
                if !self.catalogue.trim().is_empty(){params["catalogue"]=json!(self.catalogue.trim());}
                if valid{if let Some(image)=image{if let Err(error)=self.start(ctx,client,generation,image,params){self.message=error;}}}
            }
            if self.busy{ui.spinner();if ui.button(t("Cancel")).clicked(){self.cancel(ctx,client,generation);self.message="Gaia calibration cancelled".into();}}
            egui::CollapsingHeader::new(crate::i18n::t("Approximate pointing / Gaia options")).id_salt("Approximate pointing / Gaia options").show(ui,|ui|{
                egui::Grid::new("gaia-pointing").num_columns(2).spacing(egui::vec2(8.,6.)).show(ui,|ui|{for (label,value) in [("RA [deg]",&mut self.ra),("Dec [deg]",&mut self.dec),("Pixel scale [arcsec/pixel]",&mut self.scale)]{ui.label(t(label));ui.add(egui::TextEdit::singleline(value).desired_width(100.));ui.end_row();}});
                ui.horizontal(|ui|{ui.label(t("Rotation [deg]"));ui.add(egui::DragValue::new(&mut self.rotation));});
                ui.horizontal(|ui|{ui.label(t("Maximum residual [arcsec]"));ui.add(egui::DragValue::new(&mut self.tolerance).range(0.02..=10.).speed(0.05));});
                ui.label(t("Optional local Gaia reference table"));ui.text_edit_singleline(&mut self.catalogue);
            });
            crate::controls::status_message(ui,&self.message);
            if self.result["accepted"]==true{
                ui.label(format!("{}: {} · RMS {:.4}″ · {} RMS {:.4}″",t("Matched stars"),self.result["matched"],self.result["rms_arcsec"].as_f64().unwrap_or(0.),t("Validation"),self.result["validation_rms_arcsec"].as_f64().unwrap_or(0.)));
                if let Some(center)=self.result["center_j2000"]["sexagesimal"].as_str(){ui.label(format!("{}: {center}",t("Image center J2000")));}
                if ui.button(t("Copy calibration result JSON")).clicked(){ctx.copy_text(serde_json::to_string_pretty(&self.result).unwrap_or_default());}
                if let Some(id)=self.result["solution_artifact"]["artifact_id"].as_str(){if let Some(url)=client.artifact_url(id){ui.hyperlink_to(t("Download calibration result"),url);}}
            }
    }
}

fn valid_header(header:&str)->bool{!header.trim().is_empty() && header.len()<=65536}

/// The worker fits WCS; the UI checks provenance and validation before adopting it.
/// 科学拟合只在外部环境执行；前端核对文档归属、HDU 和独立验证后才接纳新解。
fn valid_solution(result:&Value,key:&str,hdu:Option<usize>,generation:u64,tolerance:f64)->bool{
    result["document_key"].as_str()==Some(key)
        && result["document_generation"].as_u64()==Some(generation)
        && hdu.is_some_and(|hdu|result["hdu"].as_u64()==Some(hdu as u64))
        && result["coordinate_origin"].as_u64()==Some(0)
        && result["solution_header"].as_str().is_some_and(valid_header)
        && result["matched"].as_u64().is_some_and(|count|count>=10)
        && result["fit_stars"].as_u64().is_some_and(|count|count>=6)
        && result["validation_stars"].as_u64().is_some_and(|count|count>=3)
        && ["rms_arcsec","validation_rms_arcsec"].iter().all(|key|result[*key].as_f64().is_some_and(|rms|rms.is_finite() && rms>=0. && rms<=tolerance))
}

#[cfg(test)]
mod tests{
    use super::*;
    fn accepted()->Value{json!({"document_key":"view:field:2","document_generation":7,"hdu":2,
        "coordinate_origin":0,"solution_header":"CTYPE1  = 'RA---TAN'\nCTYPE2  = 'DEC--TAN'",
        "matched":20,"fit_stars":15,"validation_stars":5,"rms_arcsec":0.1,"validation_rms_arcsec":0.2})}
    #[test]
    fn solution_must_belong_to_document_hdu_and_have_independent_validation(){
        let value=accepted();assert!(valid_solution(&value,"view:field:2",Some(2),7,1.));
        for (key,value) in [("document_key",json!("old-view")),("document_generation",json!(6)),
            ("hdu",json!(1)),("solution_header",json!(" ")),("validation_stars",json!(0)),
            ("validation_rms_arcsec",json!(2.)),("rms_arcsec",Value::Null)]{
            let mut invalid=accepted();invalid[key]=value;
            assert!(!valid_solution(&invalid,"view:field:2",Some(2),7,1.),"{key}");
        }
    }
    #[test]
    fn saved_header_has_a_strict_memory_bound(){
        assert!(!valid_header(""));assert!(!valid_header(&"x".repeat(65537)));
        assert!(valid_header("CTYPE1  = 'RA---TAN'"));
    }
}
