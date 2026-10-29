//! Read original headers independently from the displayed HDU.
//! 原始头按 HDU 单独读取，不把 primary 元数据合并进科学图像头。
use crate::{FitsApp,i18n::t};
use eframe::egui;
use serde_json::{json,Value};
#[derive(Default)]pub(super) struct Headers{selected:Option<usize>,pending:bool,text:Option<String>,error:String}
impl Headers{
    pub fn select(&mut self,hdu:usize){self.selected=Some(hdu);self.text=None;self.error.clear();self.pending=false;}
    pub fn accept(&mut self,hdu:usize,result:Result<Value,String>){if self.selected!=Some(hdu){return;}self.pending=false;match result{Ok(value)=>{if value["hdu"].as_u64()==Some(hdu as u64){self.text=value["header"].as_str().map(str::to_owned);self.error.clear();}},Err(error)=>self.error=error}}
}
impl FitsApp{
    pub(super) fn header_ui(&mut self,ui:&mut egui::Ui,ctx:&egui::Context){
        let Some(image)=&self.image else{return;};let current=image.info.hdu;
        let selected=self.header.selected.get_or_insert(current);
        #[cfg(all(target_arch="wasm32",feature="offline"))]
        {*selected=current;}
        let previous=*selected;
        ui.horizontal(|ui|{
            #[cfg(not(all(target_arch="wasm32",feature="offline")))]
            egui::ComboBox::from_id_salt("original-header-hdu").selected_text(format!("HDU {}",*selected)).show_ui(ui,|ui|{for hdu in &image.info.hdus{ui.selectable_value(selected,hdu.index,format!("{} · {}",hdu.index,hdu.name));}});
            #[cfg(all(target_arch="wasm32",feature="offline"))]
            ui.add_enabled_ui(false,|ui|{egui::ComboBox::from_id_salt("original-header-hdu").selected_text(format!("HDU {}",current)).show_ui(ui,|_|{});}).response.on_disabled_hover_text("Shows the current HDU header. Switch the image HDU to inspect another viewable header.");
            if ui.button(t("Copy JSON")).clicked(){ctx.copy_text(json!({"path":image.info.path,"hdu":*selected,"header":if *selected==current{Some(image.info.header.as_str())}else{self.header.text.as_deref()}}).to_string());}
        });
        if previous!=*selected{self.header.text=None;self.header.error.clear();self.header.pending=false;}
        if *selected!=current && self.header.text.is_none() && !self.header.pending && self.header.error.is_empty(){self.header.pending=true;self.science.request_labeled(ctx,self.generation,"header",&format!("header.{}",*selected),json!({"path":image.info.path,"hdu":*selected}));}
        if self.header.pending{ui.spinner();}if !self.header.error.is_empty(){ui.label(&self.header.error);}
        let text=if *selected==current{image.info.header.as_str()}else{self.header.text.as_deref().unwrap_or("")};
        egui::ScrollArea::both().id_salt("original-header-text").show(ui,|ui|{ui.monospace(text);});
    }
}
