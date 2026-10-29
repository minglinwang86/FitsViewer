//! Small popup layouts shared by Mini/native/WASM; no scientific state is saved.
//! 极简模式共用的小型弹窗布局，不保存科学数据、任务或文件路径。
use eframe::egui::{self,Pos2,Rect,Vec2};
use serde::{Deserialize,Serialize};
use std::collections::{BTreeMap,BTreeSet};

/// About 16:9 including the compact title bar and frame; the user may resize freely.
/// 含标题栏与边框约为 16:9；用户仍可自由调整尺寸。
pub const HISTOGRAM_DEFAULT_SIZE:Vec2=Vec2::new(320.,152.);

#[derive(Clone,Copy,Debug,Eq,PartialEq,Ord,PartialOrd,Hash,Serialize,Deserialize)]
pub enum Key {Settings,Histogram,Coordinates,Photometry,Growth,Header}
impl Key {pub const ALL:[Self;6]=[Self::Settings,Self::Histogram,Self::Coordinates,Self::Photometry,Self::Growth,Self::Header];}
#[derive(Clone,Copy,Debug,Eq,PartialEq,Hash)]
pub enum Context {Empty,Image,Spectrum,ProImage,ProSpectrum}
#[derive(Clone,Debug,Default,PartialEq,Serialize,Deserialize)]
#[serde(default)]
pub struct State {pub open:bool,pub collapsed:bool,pub rect:Option<[f32;4]>,pub size:Option<[f32;2]>,pub layout_revision:u8}
#[derive(Clone,Debug,Default,PartialEq,Serialize,Deserialize)]
#[serde(default)]
pub struct Layouts {pub empty:BTreeMap<Key,State>,pub image:BTreeMap<Key,State>,pub spectrum:BTreeMap<Key,State>,pub pro_image:BTreeMap<Key,State>,pub pro_spectrum:BTreeMap<Key,State>}
impl Layouts {
    fn get(&self,context:Context)->&BTreeMap<Key,State>{match context{Context::Empty=>&self.empty,Context::Image=>&self.image,Context::Spectrum=>&self.spectrum,Context::ProImage=>&self.pro_image,Context::ProSpectrum=>&self.pro_spectrum}}
    fn get_mut(&mut self,context:Context)->&mut BTreeMap<Key,State>{match context{Context::Empty=>&mut self.empty,Context::Image=>&mut self.image,Context::Spectrum=>&mut self.spectrum,Context::ProImage=>&mut self.pro_image,Context::ProSpectrum=>&mut self.pro_spectrum}}
    pub fn validate(&self)->Result<(),String>{
        for state in [&self.empty,&self.image,&self.spectrum,&self.pro_image,&self.pro_spectrum].into_iter().flat_map(|map|map.values()) {
            if state.rect.is_some_and(|r|r.iter().any(|v|!v.is_finite() || v.abs()>1e6) || r[2]<r[0] || r[3]<r[1]) || state.size.is_some_and(|s|s.iter().any(|v|!v.is_finite() || *v<=0. || *v>10000.)){return Err("Invalid popup window geometry".into());}
        }Ok(())
    }
}

/// Session bookkeeping stays outside serialized preferences. A changed import wins
/// over observations from the old frame; PRO independent windows use separate maps.
/// 会话状态不序列化；导入布局优先于旧帧采样，PRO 独立窗与极简弹窗分开记忆。
#[derive(Default)]
pub struct Runtime {context:Option<Context>,source:Layouts,current:Layouts,applied:BTreeSet<Key>,initialized:bool,title_widths:BTreeMap<Key,f32>,headers:BTreeMap<Key,(Rect,egui::LayerId)>}
pub(crate) fn usable_viewport(screen:Rect)->bool{screen.is_finite() && screen.width()>1. && screen.height()>1.}
impl Runtime {
    /// A hidden browser canvas is not a user-selected layout. Restore the saved
    /// geometry when it becomes visible, rather than egui's temporary tiny area.
    /// 隐藏画布不代表用户布局；恢复可见时重新应用保存的几何。
    pub fn suspend(&mut self){self.applied.clear();self.headers.clear();}
    pub fn reserve_title_controls(&mut self,key:Key,width:f32){self.title_widths.insert(key,width);}
    pub fn title_controls(&self,ctx:&egui::Context,key:Key,body:impl FnOnce(&mut egui::Ui)){
        let Some((rect,layer))=self.headers.get(&key)else{return;};
        let id=egui::Id::new(("viewer-popup-title-controls",self.context,key));
        let mut ui=egui::Ui::new(ctx.clone(),id,egui::UiBuilder::new().layer_id(*layer).max_rect(*rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
        ui.set_clip_rect(*rect);ui.spacing_mut().item_spacing.x=4.;body(&mut ui);
    }
    pub fn begin(&mut self,prefs:&Layouts,context:Option<Context>)->Option<[bool;6]>{
        if self.initialized && self.context==context && &self.source==prefs{return None;}
        self.context=context;self.source=prefs.clone();self.current=prefs.clone();self.applied.clear();self.initialized=true;
        context.map(|context|{let saved=self.current.get(context);std::array::from_fn(|i|saved.get(&Key::ALL[i]).is_some_and(|state|state.open))})
    }
    pub fn finish(&mut self,prefs:&mut Layouts,context:Option<Context>,open:[bool;6]){
        if self.context!=context || *prefs!=self.source {self.initialized=false;return;}
        let Some(context)=context else{return;};
        for (key,open) in Key::ALL.into_iter().zip(open){
            self.current.get_mut(context).entry(key).or_default().open=open;
            // egui may auto-place an area after it has been closed. Reopening
            // must explicitly restore our saved position instead of auto-place.
            // 重新打开时明确恢复保存位置，避免自动排布逐次把窗口向右移。
            if !open{self.applied.remove(&key);}
        }
        *prefs=self.current.clone();self.source=prefs.clone();
    }
    pub fn show(&mut self,ctx:&egui::Context,key:Key,title:&str,open:&mut bool,default_size:Vec2,body:impl FnOnce(&mut egui::Ui)){
        if !*open{return;}
        if !usable_viewport(ctx.screen_rect()){self.applied.remove(&key);return;}
        let context=self.context.unwrap_or(Context::Empty);
        let mut saved=self.current.get(context).get(&key).cloned().unwrap_or_default();
        let minimum=if key==Key::Histogram{egui::vec2(300.,96.)}else{egui::vec2(180.,64.)};
        if key==Key::Histogram && saved.layout_revision<2{
            // Apply the new compact layout once. Later user resizes are preserved.
            // 新紧凑布局只迁移一次；之后完整保存用户调整的尺寸。
            let previous_default=saved.size.is_some_and(|size|(size[0]-360.).abs()<=1. && (size[1]-132.).abs()<=1.);
            if saved.layout_revision==0 || previous_default{saved.size=Some([default_size.x,default_size.y]);}
            saved.layout_revision=2;
            self.current.get_mut(context).insert(key,saved.clone());self.applied.remove(&key);
        }
        let first=self.applied.insert(key);
        let id=egui::Id::new(("viewer-popup-v1",context,key));
        let screen=ctx.screen_rect();let margin=8_f32.min(screen.width().max(0.)/4.).min(screen.height().max(0.)/4.);let bounds=screen.shrink(margin);
        let (position,size)=fit(&saved,bounds,default_size);
        let maximum=(bounds.size()-egui::vec2(16.,40.)).max(egui::vec2(1.,1.));
        let caption=crate::ui_style::window_caption(ctx,title);
        let padded=|size|crate::ui_style::padded_window_size(ctx,size);
        let mut window=egui::Window::new(egui::WidgetText::Galley(caption.clone())).id(id).open(open).resizable(true).constrain_to(bounds)
            .frame(crate::ui_style::compact_window_frame(ctx))
            .min_size(padded(maximum.min(minimum))).max_size(padded(maximum)).default_size(padded(size));
        if first {
            // Explicitly reset egui's in-session area/resize memory too when an
            // imported/default layout has no geometry for this window.
            // 导入或恢复默认未提供几何时，也重置 egui 本次会话的旧位置与尺寸。
            let position=position.unwrap_or_else(||{
                let free=(bounds.size()-size-egui::vec2(16.,40.)).max(Vec2::ZERO);
                if key==Key::Photometry && context==Context::Image {
                    // Keep the central target visible on first use. A saved position
                    // always wins; Settings/Header retain their centered defaults.
                    // 首次测光靠右，避免挡住中心目标；保存位置优先，设置/文件头仍居中。
                    egui::pos2(bounds.left()+free.x,ctx.available_rect().top().max(bounds.top()).min(bounds.top()+free.y))
                }else{bounds.min+free/2.}
            });
            window=window.current_pos(position).fixed_size(padded(size));
            let mut collapsing=egui::collapsing_header::CollapsingState::load_with_default_open(ctx,id.with("collapsing"),!saved.collapsed);
            collapsing.set_open(!saved.collapsed);collapsing.store(ctx);
        }
        if let Some(response)=window.show(ctx,|ui|crate::ui_style::window_body(ui,|ui|{
            if key==Key::Histogram{
                let allocated=ui.available_size();
                // Preserve the Resize area's allocated extent, even for a short
                // body; never force the old saved size while the user drags.
                // 记录实际分配空间，不把内容最小尺寸当成用户窗口尺寸。
                // Set the minimum before content advances the layout cursor.
                if allocated.is_finite(){ui.set_min_size(allocated);}
                body(ui);
            }else{egui::ScrollArea::both().id_salt("popup-content").auto_shrink([false,false]).show(ui,body);}
            ui.min_rect().size()
        })) {
            let reserved=self.title_widths.get(&key).copied().unwrap_or(0.);
            crate::ui_style::weight_window_title_reserved(ctx,&response.response,&caption,reserved);
            if reserved>0. && *open{
                let row=crate::ui_style::window_title_rect(ctx,response.response.rect);
                let right=row.right()-row.height()-4.;let left=(right-reserved).max(row.left()+row.height()+4.);
                self.headers.insert(key,(Rect::from_min_max(egui::pos2(left,row.top()),egui::pos2(right,row.bottom())),response.response.layer_id));
            }else{self.headers.remove(&key);}
            // Keep content size while collapsed, and avoid disk/localStorage writes
            // during pointer movement. Logical points survive DPI changes.
            // 折叠时保留内容尺寸；拖动期间不落盘。使用逻辑点兼容 DPI 变化。
            if *open && !ctx.input(|input|input.pointer.primary_down()) {
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
    #[test]fn histogram_proportion_migration_preserves_user_resizes(){
        for (revision,size,expected) in [(0,[418.,310.],[320.,152.]),(1,[360.,132.],[320.,152.]),(1,[480.,208.],[480.,208.]),(2,[335.,190.],[335.,190.])] {
            let ctx=egui::Context::default();let mut prefs=Layouts::default();let mut runtime=Runtime::default();
            prefs.image.insert(Key::Histogram,State{open:true,layout_revision:revision,size:Some(size),rect:Some([80.,90.,600.,500.]),..Default::default()});
            for _ in 0..3 {
                let _=ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(1200.,900.))),..Default::default()},|ctx|{
                    runtime.begin(&prefs,Some(Context::Image));let mut open=true;
                    runtime.show(ctx,Key::Histogram,"Histogram",&mut open,HISTOGRAM_DEFAULT_SIZE,|ui|{ui.label("Histogram");});
                    runtime.finish(&mut prefs,Some(Context::Image),[false,true,false,false,false,false]);
                });
            }
            let state=&prefs.image[&Key::Histogram];let actual=state.size.unwrap();
            assert!(actual.iter().zip(expected).all(|(a,b)|(a-b).abs()<=0.5),"revision {revision}: {actual:?}");
            assert_eq!(state.layout_revision,2);let rect=state.rect.unwrap();assert!((rect[0]-80.).abs()<=0.5 && (rect[1]-90.).abs()<=0.5);
        }
    }
    #[test]fn popup_geometry_survives_hidden_canvas_and_becomes_visible_at_the_same_place(){
        let ctx=egui::Context::default();let mut prefs=Layouts::default();let mut runtime=Runtime::default();
        prefs.image.insert(Key::Histogram,State{open:true,rect:Some([350.,280.,896.,630.]),size:Some([520.,310.]),..Default::default()});
        let mut baseline=None;
        for (frame,viewport) in [egui::vec2(1400.,950.),egui::vec2(1400.,950.),Vec2::ZERO,egui::vec2(0.,950.),egui::vec2(1400.,0.),egui::vec2(1.,1.),egui::vec2(1400.,950.),egui::vec2(1400.,950.)].into_iter().enumerate(){
            let _=ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,viewport)),time:Some(frame as f64/60.),..Default::default()},|ctx|{
                crate::ui_style::apply_theme(ctx,"Dark");runtime.begin(&prefs,Some(Context::Image));
                let mut open=true;runtime.show(ctx,Key::Histogram,"Physical-value histogram",&mut open,egui::vec2(360.,132.),|ui|{ui.label("Histogram");});
                runtime.finish(&mut prefs,Some(Context::Image),[false,true,false,false,false,false]);
            });
            let current=prefs.image[&Key::Histogram].clone();
            if frame==1 {baseline=Some(current.clone());}
            if frame>=2 {assert_eq!(Some(&current),baseline.as_ref(),"popup changed at frame {frame} with viewport {viewport:?}");}
        }
    }
    #[test]fn histogram_recovers_a_legacy_truncated_content_size(){
        let ctx=egui::Context::default();let mut prefs=Layouts::default();let mut runtime=Runtime::default();
        prefs.image.insert(Key::Histogram,State{open:true,rect:Some([8.,8.,202.,110.]),size:Some([180.,64.]),..Default::default()});
        let _=ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(1400.,950.))),..Default::default()},|ctx|{
            runtime.begin(&prefs,Some(Context::Image));let mut open=true;
            runtime.show(ctx,Key::Histogram,"Physical-value histogram",&mut open,egui::vec2(360.,132.),|ui|{ui.label("Histogram");});
            runtime.finish(&mut prefs,Some(Context::Image),[false,true,false,false,false,false]);
        });
        let saved=&prefs.image[&Key::Histogram];assert!(!saved.collapsed);
        let size=saved.size.unwrap();assert!(size[0]>=300. && size[1]>=132.,"{size:?}");
    }
    #[test]fn closing_and_reopening_a_popup_keeps_its_saved_position(){
        let ctx=egui::Context::default();let mut prefs=Layouts::default();let mut runtime=Runtime::default();
        prefs.image.insert(Key::Histogram,State{open:true,rect:Some([350.,280.,896.,630.]),size:Some([520.,310.]),..Default::default()});
        let mut expected=None;
        for (frame,mut open) in [true,true,false,false,true,true,false,true].into_iter().enumerate(){
            let _=ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(1400.,950.))),time:Some(frame as f64/60.),..Default::default()},|ctx|{
                runtime.begin(&prefs,Some(Context::Image));
                runtime.show(ctx,Key::Histogram,"Physical-value histogram",&mut open,egui::vec2(360.,132.),|ui|{ui.label("Histogram");});
                runtime.finish(&mut prefs,Some(Context::Image),[false,open,false,false,false,false]);
            });
            let saved=&prefs.image[&Key::Histogram];
            if frame==1{expected=Some((saved.rect,saved.size));}
            if frame>=2 && open{assert_eq!(Some((saved.rect,saved.size)),expected,"popup moved when reopened at frame {frame}");}
        }
    }
    #[test]fn clicking_the_close_button_does_not_save_egui_auto_placement(){
        let ctx=egui::Context::default();let mut prefs=Layouts::default();let mut runtime=Runtime::default();let mut open=true;
        prefs.image.insert(Key::Histogram,State{open:true,rect:Some([350.,280.,896.,630.]),size:Some([520.,310.]),..Default::default()});
        let draw=|ctx:&egui::Context,prefs:&mut Layouts,runtime:&mut Runtime,open:&mut bool,events:Vec<egui::Event>|{
            let _=ctx.run(egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(1400.,950.))),events,..Default::default()},|ctx|{
                crate::ui_style::apply_theme(ctx,"Dark");runtime.begin(prefs,Some(Context::Image));
                runtime.show(ctx,Key::Histogram,"Physical-value histogram",open,egui::vec2(360.,132.),|ui|{ui.label("Histogram");});
                runtime.finish(prefs,Some(Context::Image),[false,*open,false,false,false,false]);
            });
        };
        draw(&ctx,&mut prefs,&mut runtime,&mut open,vec![]);draw(&ctx,&mut prefs,&mut runtime,&mut open,vec![]);
        let saved=prefs.image[&Key::Histogram].clone();let rect=saved.rect.unwrap();let pos=egui::pos2(rect[2]-12.,rect[1]+12.);
        draw(&ctx,&mut prefs,&mut runtime,&mut open,vec![egui::Event::PointerMoved(pos),egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:true,modifiers:Default::default()}]);
        draw(&ctx,&mut prefs,&mut runtime,&mut open,vec![egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:false,modifiers:Default::default()}]);
        assert!(!open,"the actual close control must have been clicked");
        assert_eq!(prefs.image[&Key::Histogram].rect,saved.rect,"closing must not save a temporary placement");
        open=true;draw(&ctx,&mut prefs,&mut runtime,&mut open,vec![]);
        assert_eq!(prefs.image[&Key::Histogram].rect,saved.rect,"reopening must restore the last visible position");
    }
    #[test]fn pro_independent_windows_do_not_overwrite_minimal_popups(){
        let mut prefs=Layouts::default();prefs.image.insert(Key::Coordinates,State{open:true,..Default::default()});
        let mut runtime=Runtime::default();let open=runtime.begin(&prefs,Some(Context::ProImage)).unwrap();assert_eq!(open,[false;6]);
        runtime.finish(&mut prefs,Some(Context::ProImage),[true,false,false,false,false,true]);
        assert!(prefs.image[&Key::Coordinates].open);assert!(!prefs.image.contains_key(&Key::Settings));
        assert!(prefs.pro_image[&Key::Settings].open);assert!(prefs.pro_image[&Key::Header].open);
        let open=runtime.begin(&prefs,Some(Context::Image)).unwrap();assert!(open[2]);assert!(!open[0] && !open[5]);
    }
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
        let mut layouts=Layouts::default();layouts.image.insert(Key::Header,State{open:true,collapsed:true,rect:Some([10.,20.,410.,320.]),size:Some([384.,260.]),..Default::default()});
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
