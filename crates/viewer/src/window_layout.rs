//! Small popup layouts shared by Mini/native/WASM; no scientific state is saved.
//! 极简模式共用的小型弹窗布局，不保存科学数据、任务或文件路径。
use eframe::egui::{self,Pos2,Rect,Vec2};
use serde::{Deserialize,Serialize};
use std::collections::{BTreeMap,BTreeSet};

#[derive(Clone,Copy,Debug,Eq,PartialEq,Ord,PartialOrd,Hash,Serialize,Deserialize)]
pub enum Key {Settings,Histogram,Coordinates,Photometry,Growth,Header}
impl Key {pub const ALL:[Self;6]=[Self::Settings,Self::Histogram,Self::Coordinates,Self::Photometry,Self::Growth,Self::Header];}
#[derive(Clone,Copy,Debug,Eq,PartialEq,Hash)]
pub enum Context {Empty,Image,Spectrum}
#[derive(Clone,Debug,Default,PartialEq,Serialize,Deserialize)]
#[serde(default)]
pub struct State {pub open:bool,pub collapsed:bool,pub rect:Option<[f32;4]>,pub size:Option<[f32;2]>}
#[derive(Clone,Debug,Default,PartialEq,Serialize,Deserialize)]
#[serde(default)]
pub struct Layouts {pub empty:BTreeMap<Key,State>,pub image:BTreeMap<Key,State>,pub spectrum:BTreeMap<Key,State>}
impl Layouts {
    fn get(&self,context:Context)->&BTreeMap<Key,State>{match context{Context::Empty=>&self.empty,Context::Image=>&self.image,Context::Spectrum=>&self.spectrum}}
    fn get_mut(&mut self,context:Context)->&mut BTreeMap<Key,State>{match context{Context::Empty=>&mut self.empty,Context::Image=>&mut self.image,Context::Spectrum=>&mut self.spectrum}}
    pub fn validate(&self)->Result<(),String>{
        for state in [&self.empty,&self.image,&self.spectrum].into_iter().flat_map(|map|map.values()) {
            if state.rect.is_some_and(|r|r.iter().any(|v|!v.is_finite() || v.abs()>1e6) || r[2]<r[0] || r[3]<r[1]) || state.size.is_some_and(|s|s.iter().any(|v|!v.is_finite() || *v<=0. || *v>10000.)){return Err("Invalid popup window geometry".into());}
        }Ok(())
    }
}

/// Session bookkeeping stays outside serialized preferences. A changed import wins
/// over observations from the old frame; PRO never writes the popup maps.
/// 会话状态不序列化；导入的新布局优先于旧帧采样，PRO 不写入弹窗布局。
#[derive(Default)]
pub struct Runtime {context:Option<Context>,source:Layouts,current:Layouts,applied:BTreeSet<Key>,initialized:bool}
impl Runtime {
    pub fn begin(&mut self,prefs:&Layouts,context:Option<Context>)->Option<[bool;6]>{
        if self.initialized && self.context==context && &self.source==prefs{return None;}
        self.context=context;self.source=prefs.clone();self.current=prefs.clone();self.applied.clear();self.initialized=true;
        context.map(|context|{let saved=self.current.get(context);std::array::from_fn(|i|saved.get(&Key::ALL[i]).is_some_and(|state|state.open))})
    }
    pub fn finish(&mut self,prefs:&mut Layouts,context:Option<Context>,open:[bool;6]){
        if self.context!=context || *prefs!=self.source {self.initialized=false;return;}
        let Some(context)=context else{return;};
        for (key,open) in Key::ALL.into_iter().zip(open){self.current.get_mut(context).entry(key).or_default().open=open;}
        *prefs=self.current.clone();self.source=prefs.clone();
    }
    pub fn show(&mut self,ctx:&egui::Context,key:Key,title:&str,open:&mut bool,default_size:Vec2,body:impl FnOnce(&mut egui::Ui)){
        if !*open{return;}
        let context=self.context.unwrap_or(Context::Empty);
        let saved=self.current.get(context).get(&key).cloned().unwrap_or_default();
        let first=self.applied.insert(key);
        let id=egui::Id::new(("viewer-popup-v1",context,key));
        let screen=ctx.screen_rect();let margin=8_f32.min(screen.width().max(0.)/4.).min(screen.height().max(0.)/4.);let bounds=screen.shrink(margin);
        let (position,size)=fit(&saved,bounds,default_size);
        let maximum=(bounds.size()-egui::vec2(16.,40.)).max(egui::vec2(1.,1.));
        let caption=crate::ui_style::window_caption(ctx,title);
        let padded=|size|crate::ui_style::padded_window_size(ctx,size);
        let mut window=egui::Window::new(egui::WidgetText::Galley(caption.clone())).id(id).open(open).resizable(true).constrain_to(bounds)
            .frame(crate::ui_style::compact_window_frame(ctx))
            .min_size(padded(maximum.min(egui::vec2(180.,64.)))).max_size(padded(maximum)).default_size(padded(size));
        if first {
            // Explicitly reset egui's in-session area/resize memory too when an
            // imported/default layout has no geometry for this window.
            // 导入或恢复默认未提供几何时，也重置 egui 本次会话的旧位置与尺寸。
            let position=position.unwrap_or_else(||bounds.min+(bounds.size()-size-egui::vec2(16.,40.)).max(Vec2::ZERO)/2.);
            window=window.current_pos(position).fixed_size(padded(size));
            let mut collapsing=egui::collapsing_header::CollapsingState::load_with_default_open(ctx,id.with("collapsing"),!saved.collapsed);
            collapsing.set_open(!saved.collapsed);collapsing.store(ctx);
        }
        if let Some(response)=window.show(ctx,|ui|crate::ui_style::window_body(ui,|ui|{
            egui::ScrollArea::both().id_salt("popup-content").auto_shrink([false,false]).show(ui,body);
            ui.min_rect().size()
        })) {
            crate::ui_style::weight_window_title(ctx,&response.response,&caption);
            // Keep content size while collapsed, and avoid disk/localStorage writes
            // during pointer movement. Logical points survive DPI changes.
            // 折叠时保留内容尺寸；拖动期间不落盘。使用逻辑点兼容 DPI 变化。
            if !ctx.input(|input|input.pointer.primary_down()) {
                let state=self.current.get_mut(context).entry(key).or_default();
                state.collapsed=response.inner.is_none();
                let rect=response.response.rect;let next=[rect.min.x,rect.min.y,rect.max.x,rect.max.y];
                if state.rect.is_none_or(|old|old.iter().zip(next).any(|(a,b)|(a-b).abs()>0.5)){state.rect=Some(next);}
                if let Some(size)=response.inner {let next=[size.x.max(1.),size.y.max(1.)];if state.size.is_none_or(|old|old.iter().zip(next).any(|(a,b)|(a-b).abs()>0.5)){state.size=Some(next);}}
            }
        }
    }
}

fn fit(saved:&State,bounds:Rect,default_size:Vec2)->(Option<Pos2>,Vec2){
    let maximum=(bounds.size()-egui::vec2(16.,40.)).max(egui::vec2(1.,1.));
    let size=saved.size.map_or(default_size,|s|egui::vec2(s[0],s[1])).max(egui::vec2(1.,1.)).min(maximum);
    let outer=(size+egui::vec2(16.,40.)).min(bounds.size().max(egui::vec2(1.,1.)));
    let position=saved.rect.map(|r|egui::pos2(r[0].clamp(bounds.left(),(bounds.right()-outer.x).max(bounds.left())),r[1].clamp(bounds.top(),(bounds.bottom()-outer.y).max(bounds.top()))));
    (position,size)
}

#[cfg(test)]mod tests {
    use super::*;
    #[test]fn popup_padding_stays_stable_across_repaints_reopen_and_restore(){
        for (key,title,expected) in [(Key::Coordinates,"Coordinates",egui::vec2(460.,250.)),(Key::Photometry,"Aperture photometry",egui::vec2(430.,300.))] {
            let mut ctx=egui::Context::default();let mut prefs=Layouts::default();let mut runtime=Runtime::default();
            for frame in 0..80 {
                if frame==40 {
                    prefs=serde_json::from_str(&serde_json::to_string(&prefs).unwrap()).unwrap();
                    runtime=Runtime::default();ctx=egui::Context::default();
                }
                let mut open=frame%10!=8;
                let input=egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(1200.,900.))),time:Some(frame as f64/60.),..Default::default()};
                let _=ctx.run(input,|ctx|{
                    crate::ui_style::apply_theme(ctx,"Light");
                    runtime.begin(&prefs,Some(Context::Image));
                    runtime.show(ctx,key,title,&mut open,expected,|ui|{ui.label("Coordinates and measurements");let _=ui.button("Run");});
                    let flags=std::array::from_fn(|index|Key::ALL[index]==key && open);
                    runtime.finish(&mut prefs,Some(Context::Image),flags);
                });
                if open {
                    let size=prefs.image[&key].size.expect("open window has content geometry");
                    assert!((size[0]-expected.x).abs()<0.5 && (size[1]-expected.y).abs()<0.5,"{key:?} frame {frame}: {size:?}");
                }
            }
        }
    }
    #[test]fn layouts_round_trip_and_keep_image_spectrum_independent(){
        let mut layouts=Layouts::default();layouts.image.insert(Key::Header,State{open:true,collapsed:true,rect:Some([10.,20.,410.,320.]),size:Some([384.,260.])});
        let restored:Layouts=serde_json::from_str(&serde_json::to_string(&layouts).unwrap()).unwrap();restored.validate().unwrap();assert_eq!(layouts,restored);assert!(restored.spectrum.is_empty());
        let mut invalid=restored;invalid.image.get_mut(&Key::Header).unwrap().size=Some([f32::NAN,1.]);assert!(invalid.validate().is_err());
    }
    #[test]fn import_wins_and_empty_start_does_not_replace_image_open(){
        let mut prefs=Layouts::default();prefs.image.insert(Key::Coordinates,State{open:true,..Default::default()});let mut runtime=Runtime::default();
        runtime.begin(&prefs,Some(Context::Empty));runtime.finish(&mut prefs,Some(Context::Empty),[false;6]);assert!(prefs.image[&Key::Coordinates].open);
        runtime.begin(&prefs,None);runtime.finish(&mut prefs,None,[true;6]);assert!(prefs.image[&Key::Coordinates].open);assert!(prefs.spectrum.is_empty());
        let restored=runtime.begin(&prefs,Some(Context::Image)).unwrap();assert!(restored[2]);
        prefs.image.get_mut(&Key::Coordinates).unwrap().open=false;runtime.finish(&mut prefs,Some(Context::Image),[true;6]);assert!(!prefs.image[&Key::Coordinates].open);
        assert!(!runtime.begin(&prefs,Some(Context::Image)).unwrap()[2]);
    }
    #[test]fn offscreen_geometry_fits_small_viewport(){
        let bounds=Rect::from_min_max(egui::pos2(8.,8.),egui::pos2(312.,192.));let saved=State{rect:Some([-900.,2000.,1500.,4000.]),size:Some([2400.,1600.]),..Default::default()};
        let (position,size)=fit(&saved,bounds,egui::vec2(430.,300.));let position=position.unwrap();assert!(bounds.contains(position));assert!(position.x+size.x+16.<=bounds.right()+0.01);assert!(position.y+size.y+40.<=bounds.bottom()+0.01);
    }
}
