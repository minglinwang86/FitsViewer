//! Opaque local-file identities and a bounded original-byte LRU for offline tabs.
use std::collections::VecDeque;
pub const BYTE_BUDGET:usize=64*1024*1024;
pub const MAX_SOURCES:usize=32;
struct Source {key:String,bytes:Vec<u8>}
pub struct Store {sources:VecDeque<Source>,serial:u64,budget:usize}
impl Default for Store {fn default()->Self{Self{sources:VecDeque::new(),serial:0,budget:BYTE_BUDGET}}}
impl Store {
    pub fn key(&mut self,name:&str)->String{self.serial+=1;let name=name.rsplit(['/', '\\']).next().filter(|n|!n.is_empty()).unwrap_or("image.fits");format!("local://{:016x}/{name}",self.serial)}
    pub fn bytes(&self)->usize{self.sources.iter().map(|s|s.bytes.capacity()).sum()}
    pub fn insert(&mut self,key:String,bytes:Vec<u8>){
        self.sources.retain(|s|s.key!=key);
        // Oversized originals are not retained; the decoded active image remains valid.
        if bytes.capacity()>self.budget{return;}
        while self.bytes()+bytes.capacity()>self.budget || self.sources.len()>=MAX_SOURCES{self.sources.pop_front();}
        self.sources.push_back(Source{key,bytes});
    }
    pub fn load(&mut self,key:&str,options:fitsviewer_core::LoadOptions)->Result<fitsviewer_core::FitsImage,String>{
        let index=self.sources.iter().position(|s|s.key==key).ok_or("Reselect the original local file to reload this document")?;
        let source=self.sources.remove(index).unwrap();let result=fitsviewer_core::load_bytes_with_options(&source.bytes,key,options);self.sources.push_back(source);result
    }
    pub fn retain(&mut self,paths:&[String]){self.sources.retain(|s|paths.iter().any(|path|path==&s.key));}
    pub fn clear(&mut self){self.sources.clear();}
}
#[cfg(test)]mod tests{
    use super::*;
    fn hdu(extension:bool,value:f64)->Vec<u8>{
        let mut cards=vec![];
        for (key,value) in if extension{vec![("XTENSION","'IMAGE   '"),("BITPIX","-64"),("NAXIS","2"),("NAXIS1","2"),("NAXIS2","2"),("PCOUNT","0"),("GCOUNT","1")]}else{vec![("SIMPLE","T"),("BITPIX","-64"),("NAXIS","2"),("NAXIS1","2"),("NAXIS2","2"),("EXTEND","T")]}{cards.push(format!("{key:<8}= {value:>20}"));}
        cards.push("END".into());let mut bytes=vec![];for card in cards{bytes.extend(format!("{card:<80}").bytes());}bytes.resize(2880,b' ');for _ in 0..4{bytes.extend(value.to_be_bytes());}bytes.resize(5760,0);bytes
    }
    fn file(a:f64,b:f64)->Vec<u8>{let mut bytes=hdu(false,a);bytes.extend(hdu(true,b));bytes}
    #[test]fn same_names_keep_distinct_pixels_and_hdu_sources(){
        let mut store=Store::default();let a=store.key("IMG.fits");let b=store.key("IMG.fits");assert_ne!(a,b);store.insert(a.clone(),file(11.,111.));store.insert(b.clone(),file(22.,222.));
        assert_eq!(store.load(&a,Default::default()).unwrap().physical(0),Some(11.));assert_eq!(store.load(&b,Default::default()).unwrap().physical(0),Some(22.));
        assert_eq!(store.load(&a,fitsviewer_core::LoadOptions{hdu:Some(1),..Default::default()}).unwrap().physical(0),Some(111.));assert_eq!(store.load(&b,fitsviewer_core::LoadOptions{hdu:Some(1),..Default::default()}).unwrap().physical(0),Some(222.));
    }
    #[test]fn eviction_and_pruning_never_fall_back_to_same_name_bytes(){
        let mut store=Store{budget:12000,..Default::default()};let a=store.key("IMG.fits");let b=store.key("IMG.fits");store.insert(a.clone(),file(1.,2.));store.insert(b.clone(),file(3.,4.));assert!(store.bytes()<=12000);
        assert!(store.load(&a,Default::default()).unwrap_err().contains("original local file"));assert_eq!(store.load(&b,Default::default()).unwrap().physical(0),Some(3.));store.retain(&[a]);assert_eq!(store.bytes(),0);assert!(store.load(&b,Default::default()).is_err());
    }
    #[test]fn same_name_offline_tabs_and_hdu_reload_keep_the_original_identity(){
        let ctx=eframe::egui::Context::default();let mut store=Store::default();let a=store.key("IMG.fits");let b=store.key("IMG.fits");store.insert(a.clone(),file(11.,111.));store.insert(b.clone(),file(22.,222.));
        let mut app=crate::FitsApp::new(String::new(),None,String::new(),None);app.tools.prefs.minimal_open_destination=crate::document_tabs::Destination::Tab;
        app.accept_offline_image(&ctx,store.load(&a,Default::default()).unwrap()).unwrap();app.accept_offline_image(&ctx,store.load(&b,Default::default()).unwrap()).unwrap();assert_eq!(app.documents.len(),2);
        app.select_document(&ctx,0).unwrap();assert_eq!(app.image.as_ref().unwrap().physical(0),Some(11.));
        let selected=app.path.clone();app.accept(store.load(&selected,fitsviewer_core::LoadOptions{hdu:Some(1),..Default::default()}).unwrap());assert_eq!(app.path,a);assert_eq!(app.image.as_ref().unwrap().physical(0),Some(111.));
        app.select_document(&ctx,1).unwrap();assert_eq!(app.path,b);assert_eq!(app.image.as_ref().unwrap().physical(0),Some(22.));app.select_document(&ctx,0).unwrap();assert_eq!(app.image.as_ref().unwrap().info.hdu,1);assert_eq!(app.image.as_ref().unwrap().physical(0),Some(111.));
        app.close_document_tab(&ctx,1).unwrap();store.retain(&app.scientific_paths());assert_eq!(store.load(&a,Default::default()).unwrap().physical(0),Some(11.));assert!(store.load(&b,Default::default()).is_err());
    }
}
