//! Lightweight document tabs shared by Mini and Full. No PRO controller is stored here.
use eframe::egui;
use fitsviewer_core::{FitsImage,LoadOptions,Limits};
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::sync::Arc;

pub const MAX_TABS:usize=32;
pub const CACHE_BUDGET:usize=128*1024*1024;
#[derive(Clone,Copy,Debug,Default,PartialEq,Eq,Serialize,Deserialize)]
#[serde(rename_all="snake_case")]
pub enum Destination {#[default] Replace,Tab,Layer}
impl Destination {
    pub fn parse(value:&str)->Option<Self>{match value{"replace"=>Some(Self::Replace),"tab"=>Some(Self::Tab),"layer"=>Some(Self::Layer),_=>None}}
    pub fn label(self)->&'static str{match self{Self::Replace=>"Replace current",Self::Tab=>"Open in new tab",Self::Layer=>"Open in new layer"}}
}
#[derive(Clone)]
pub struct Snapshot {
    pub image:Option<Arc<FitsImage>>,pub view:Value,pub regions:crate::regions_ui::Regions,
    pub options:LoadOptions,pub limits:Limits,pub auto_fit:bool,
}
struct Document {path:String,hdu:Option<usize>,snapshot:Option<Snapshot>,used:u64}
#[derive(Default)]
pub struct Tabs {slots:Vec<Document>,pub selected:Option<usize>,clock:u64,reveal_selected:bool,row_rect:Option<[f32;4]>,buttons:Vec<Value>}
pub enum Action {Select(usize),Close(usize)}
pub(super) fn dataset_bytes(image:&FitsImage)->usize{
    use fitsviewer_core::RawPixels;
    // Spectrum ordinates are the RawPixels storage; x and error are additional.
    let raw=match &image.pixels{RawPixels::Unavailable=>0,RawPixels::U8(v)=>v.capacity(),RawPixels::I16(v)=>v.capacity()*2,RawPixels::I32(v)=>v.capacity()*4,RawPixels::I64(v)=>v.capacity()*8,RawPixels::F32(v)=>v.capacity()*4,RawPixels::F64(v)=>v.capacity()*8};
    raw+image.spectrum.as_ref().map_or(0,|s|s.x.capacity()*8+s.error.as_ref().map_or(0,|e|e.capacity()*8))
}
impl Tabs {
    pub fn len(&self)->usize{self.slots.len()}
    pub fn clear(&mut self){self.slots.clear();self.selected=None;self.row_rect=None;self.buttons.clear();}
    pub fn paths(&self)->Vec<String>{self.slots.iter().map(|s|s.path.clone()).collect()}
    pub fn find(&self,path:&str,hdu:Option<usize>)->Option<usize>{
        let matches=|s:&Document|s.path==path && (hdu.is_none() || s.hdu==hdu);
        self.selected.filter(|&i|self.slots.get(i).is_some_and(matches)).or_else(||self.slots.iter().position(matches))
    }
    pub fn inactive_bytes(&self)->usize{self.slots.iter().enumerate().filter(|(i,_)|Some(*i)!=self.selected).filter_map(|(_,s)|s.snapshot.as_ref()?.image.as_ref()).map(|i|dataset_bytes(i)).sum()}
    fn trim(&mut self){self.trim_to(CACHE_BUDGET);}
    pub fn trim_to(&mut self,budget:usize){
        while self.inactive_bytes()>budget {
            let next=self.slots.iter().enumerate().filter(|(i,s)|Some(*i)!=self.selected && s.snapshot.as_ref().is_some_and(|s|s.image.is_some())).min_by_key(|(_,s)|s.used).map(|(i,_)|i);
            let Some(index)=next else{break;};self.slots[index].snapshot.as_mut().unwrap().image=None;
        }
    }
    pub fn add(&mut self,path:String,hdu:Option<usize>)->Result<usize,String>{
        if self.slots.len()>=MAX_TABS{return Err("At most 32 documents; close unused tabs".into());}
        self.clock+=1;self.slots.push(Document{path,hdu,snapshot:None,used:self.clock});self.selected=Some(self.slots.len()-1);self.reveal_selected=true;self.trim();Ok(self.slots.len()-1)
    }
    pub fn replace(&mut self,path:String,hdu:Option<usize>)->Result<(),String>{
        if let Some(index)=self.selected{self.slots[index]=Document{path,hdu,snapshot:None,used:self.clock};Ok(())}else{self.add(path,hdu).map(|_|())}
    }
    pub fn remember(&mut self,path:String,hdu:usize,snapshot:Snapshot){
        let index=match self.selected{Some(i)=>i,None=>match self.add(path.clone(),Some(hdu)){Ok(i)=>i,Err(_)=>return}};
        self.clock+=1;self.slots[index]=Document{path,hdu:Some(hdu),snapshot:Some(snapshot),used:self.clock};self.trim();
    }
    pub fn select(&mut self,index:usize)->Result<(String,Option<usize>,Option<Snapshot>),String>{
        let slot=self.slots.get_mut(index).ok_or("Unknown document tab")?;self.clock+=1;slot.used=self.clock;
        let result=(slot.path.clone(),slot.hdu,slot.snapshot.clone());self.selected=Some(index);self.reveal_selected=true;self.trim();Ok(result)
    }
    pub fn remove(&mut self,index:usize)->Result<(bool,String,Option<usize>),String>{
        if index>=self.slots.len(){return Err("Unknown document tab".into());}
        let active=self.selected==Some(index);let path=self.slots.remove(index).path;
        self.selected=if active{(!self.slots.is_empty()).then(||index.min(self.slots.len()-1))}else{self.selected.map(|i|i-usize::from(i>index))};
        Ok((active,path,self.selected))
    }
    pub fn summary(&self)->Value{json!({"selected":self.selected,"views":self.slots.iter().map(|s|json!({"path":s.path,"hdu":s.hdu,"cached":s.snapshot.as_ref().is_some_and(|s|s.image.is_some())})).collect::<Vec<_>>(),"inactive_cache_bytes":self.inactive_bytes(),"cache_budget":CACHE_BUDGET,"max_tabs":MAX_TABS,"rect":self.row_rect,"buttons":self.buttons})}
    pub fn remember_row(&mut self,rect:egui::Rect){self.row_rect=Some([rect.min.x,rect.min.y,rect.max.x,rect.max.y]);}
    pub fn toolbar(&mut self,ctx:&egui::Context)->Option<Action>{
        if self.slots.is_empty(){self.row_rect=None;return None;}
        let mut action=None;
        let panel=egui::TopBottomPanel::top("document-tabs").show(ctx,|ui|{crate::ribbon::align_controls(ui);ui.horizontal(|ui|{
            ui.weak(crate::i18n::t("Documents"));
            action=self.ui(ui,170.);
        });});self.remember_row(panel.response.rect);action
    }
    /// Draw only tab contents so PRO can share its existing comparison row.
    pub fn ui(&mut self,ui:&mut egui::Ui,label_width:f32)->Option<Action>{
        self.buttons.clear();if self.slots.is_empty(){return None;}
        let mut action=None;let reveal=std::mem::take(&mut self.reveal_selected);let mut buttons=vec![];
            egui::ScrollArea::horizontal().id_salt("document-tab-strip").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(ui,|ui|{ui.horizontal(|ui|{
            for (index,slot) in self.slots.iter().enumerate(){ui.push_id(index,|ui|{
                let name=slot.path.rsplit(['/', '\\']).next().unwrap_or(&slot.path);
                let height=ui.spacing().interact_size.y;let label=format!("{name} [{}]",slot.hdu.map(|h|h.to_string()).unwrap_or_else(||"auto".into()));
                let tooltip=if slot.path.starts_with("local://"){name}else{&slot.path};
                let response=ui.add_sized([label_width,height],egui::Button::selectable(self.selected==Some(index),label).truncate()).on_hover_text(tooltip);
                if reveal && self.selected==Some(index){response.scroll_to_me(Some(egui::Align::Center));}
                if response.clicked(){action=Some(Action::Select(index));}
                let close=ui.add_sized([height,height],egui::Button::new("×")).on_hover_text(crate::i18n::t("Close document tab"));
                if close.clicked(){action=Some(Action::Close(index));}
                let rect=|r:egui::Rect|[r.min.x,r.min.y,r.max.x,r.max.y];buttons.push(json!({"index":index,"select":rect(response.rect),"close":rect(close.rect)}));
            });}
            });});
        self.buttons=buttons;action
    }
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn one_and_two_documents_keep_the_strip_below_ribbon_without_height_changes(){
        let mut app=crate::FitsApp::new(String::new(),None,String::new(),None);
        app.documents.add("a.fits".into(),Some(0)).unwrap();let ctx=egui::Context::default();let mut positions=vec![];
        for count in [1,2,1]{
            if count==2{app.documents.add("b.fits".into(),Some(0)).unwrap();}else if app.documents.len()==2{app.documents.remove(1).unwrap();}
            for _ in 0..2{let _=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(900.,600.))),..Default::default()},|ctx|{
                app.ribbon(ctx);app.documents.toolbar(ctx);let row=app.documents.row_rect.unwrap();
                assert!(row[1]>=app.ribbon_rect[3]-0.5);assert!(ctx.available_rect().top()>=row[3]-0.5);positions.push((row[1],row[3]));
            });}
        }
        assert!(positions.iter().all(|p|(p.0-positions[0].0).abs()<0.5 && (p.1-positions[0].1).abs()<0.5),"{positions:?}");
        app.documents.clear();let _=ctx.run(Default::default(),|ctx|{app.documents.toolbar(ctx);});assert!(app.documents.row_rect.is_none());
    }
    #[test]fn tab_limit_and_close_are_bounded(){let mut tabs=Tabs::default();for i in 0..MAX_TABS{tabs.add(i.to_string(),Some(i)).unwrap();}assert!(tabs.add("overflow".into(),None).is_err());tabs.select(0).unwrap();assert!(tabs.remove(0).unwrap().0);assert_eq!(tabs.len(),31);assert_eq!(tabs.selected,Some(0));tabs.clear();assert_eq!(tabs.len(),0);}
    #[test]fn replace_keeps_one_document_and_explicit_hdu(){let mut tabs=Tabs::default();tabs.add("first".into(),Some(1)).unwrap();tabs.replace("second".into(),Some(2)).unwrap();let(path,hdu,_)=tabs.select(0).unwrap();assert_eq!((path.as_str(),hdu),("second",Some(2)));assert_eq!(tabs.len(),1);}
    #[test]fn destinations_reject_unknown_values(){assert_eq!(Destination::parse("tab"),Some(Destination::Tab));assert!(Destination::parse("window-ish").is_none());}
    #[test]fn allocation_budget_uses_raw_variant_and_spectrum_capacities(){
        let mut pixels=Vec::with_capacity(32);pixels.push(1.);let mut x=Vec::with_capacity(16);x.push(1.);let mut error=Vec::with_capacity(8);error.push(1.);
        let image=FitsImage{info:fitsviewer_core::ImageInfo{path:"budget-fixture".into(),hdu:0,width:1,height:1,bitpix:8,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:None},pixels:fitsviewer_core::RawPixels::F64(pixels),spectrum:Some(fitsviewer_core::SpectrumData{x,error:Some(error)})};
        assert_eq!(dataset_bytes(&image),(32+16+8)*8);
    }
    #[test]fn long_document_names_keep_a_single_compact_tab_row(){
        let mut tabs=Tabs::default();for i in 0..MAX_TABS{tabs.add(format!("folder/{i}-{}.fits","long-name-".repeat(30)),Some(i)).unwrap();}
        let ctx=egui::Context::default();let mut remaining=egui::Rect::NOTHING;
        for _ in 0..2{let _=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(450.,300.))),..Default::default()},|ctx|{tabs.toolbar(ctx);remaining=ctx.available_rect();});}
        assert!(remaining.top()<45.,"document tabs consumed {} points",remaining.top());
    }
    #[test]fn eviction_releases_inactive_samples_but_keeps_document_metadata(){
        let image=Arc::new(FitsImage{info:fitsviewer_core::ImageInfo{path:"first".into(),hdu:3,width:4,height:4,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:None},pixels:fitsviewer_core::RawPixels::F64(vec![1.;16]),spectrum:None});
        let weak=Arc::downgrade(&image);let mut tabs=Tabs::default();tabs.add("first".into(),Some(3)).unwrap();
        tabs.remember("first".into(),3,Snapshot{image:Some(image.clone()),view:json!({"zoom":3}),regions:Default::default(),options:Default::default(),limits:Limits::Manual,auto_fit:false});drop(image);
        tabs.add("second".into(),Some(0)).unwrap();tabs.trim_to(0);assert!(weak.upgrade().is_none());
        let(path,hdu,snapshot)=tabs.select(0).unwrap();let snapshot=snapshot.unwrap();assert_eq!(path,"first");assert_eq!(hdu,Some(3));assert!(snapshot.image.is_none());assert_eq!(snapshot.view["zoom"],3);
    }
}
