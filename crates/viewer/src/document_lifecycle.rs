//! File requests, cached selections and closing share one release path.
//! 文件请求、缓存切换和关闭共用释放路径，避免 UI 入口绕过后台取消。
use crate::{FitsApp,coordinates,regions_ui};
use eframe::egui;
use serde_json::json;

pub(super) struct State{heartbeat:f64,refs_at:f64,refs:Vec<String>,generation:u64}
impl Default for State{fn default()->Self{Self{heartbeat:-100.,refs_at:-100.,refs:vec![],generation:u64::MAX}}}
impl FitsApp {
    fn scientific_paths(&self)->Vec<String>{
        let mut paths=vec![];
        if let Some(image)=&self.image{paths.push(image.info.path.clone());}
        #[cfg(feature="pro")]{
            if let Some(multi)=&self.multi{paths.extend(multi.paths());}
            if let Some(artifact)=self.pro.as_ref().and_then(|pro|pro.artifact.as_ref()).and_then(|artifact|artifact["artifact_id"].as_str()){paths.push(format!("artifact:{artifact}"));}
        }
        #[cfg(feature="pro")] if self.multi.is_none(){if let Some(views)=self.saved_multi["views"].as_array(){for view in views{if let Some(path)=view["path"].as_str(){paths.push(path.to_owned());}}}}
        paths.sort();paths.dedup();paths
    }
    pub(super) fn abandon_document(&mut self,ctx:&egui::Context,preserve:Option<&str>){
        let mut paths=self.scientific_paths();if let Some(path)=preserve{paths.push(path.to_owned());}paths.sort();paths.dedup();
        self.coordinates.cancel(ctx,&self.science,self.generation);
        self.science.request(ctx,self.generation,"release_document",json!({"preserve_paths":paths}));
        self.generation+=1;self.science.bind_document(self.generation);self.wcs_cache.clear();
        #[cfg(feature="pro")] if let Some(pro)=&mut self.pro{pro.clear_document_results();}
        self.header=Default::default();self.lifecycle.refs_at=-100.;
    }
    pub(super) fn replace_shared(&mut self,ctx:&egui::Context,image:std::sync::Arc<fitsviewer_core::FitsImage>){self.abandon_document(ctx,Some(&image.info.path));self.accept_shared(image);}
    pub(super) fn close_document(&mut self,ctx:&egui::Context){
        self.coordinates.cancel(ctx,&self.science,self.generation);self.coordinates=coordinates::Coordinates::default();
        self.wcs_override=None;self.wcs_result=None;self.wcs_pixel=None;self.wcs_pending=false;self.wcs_disabled=false;self.wcs_text.clear();self.wcs_cache.clear();
        let mut slot=self.pending.lock().unwrap();slot.0+=1;slot.1=None;drop(slot);
        self.loading=false;self.error.clear();self.image=None;self.texture=None;self.scale_mapping=None;self.stats=Default::default();self.path.clear();self.cursor.clear();self.tools.histogram=None;self.regions=regions_ui::Regions::default();
        #[cfg(feature="pro")] {self.pro=None;self.multi=None;self.pro_dock=None;}
        self.header=Default::default();self.saved_multi=serde_json::Value::Null;self.pending_view=None;
        self.generation+=1;self.science.bind_document(self.generation);self.science.request(ctx,self.generation,"release_view",json!({}));self.lifecycle=State::default();
    }
    pub(super) fn scientific_heartbeat(&mut self,ctx:&egui::Context){
        if self.image.is_none(){return;}
        let now=ctx.input(|input|input.time);
        if now-self.lifecycle.heartbeat>=30.{self.science.request(ctx,self.generation,"view_heartbeat",json!({}));self.lifecycle.heartbeat=now;}
        if now-self.lifecycle.refs_at>=1. || self.lifecycle.generation!=self.generation{
            let paths=self.scientific_paths();if paths!=self.lifecycle.refs || self.lifecycle.generation!=self.generation{self.science.request(ctx,self.generation,"artifact_refs",json!({"paths":paths}));self.lifecycle.refs=paths;}
            self.lifecycle.refs_at=now;self.lifecycle.generation=self.generation;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(25));
    }
}
