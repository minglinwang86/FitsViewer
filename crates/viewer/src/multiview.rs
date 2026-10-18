//! Shared image cache and independent PRO views, with an explicit memory budget.
//! PRO 多视图共享像素数据，独立保存显示状态，缓存有明确上限。
use eframe::egui::{self,Color32,Pos2,Rect,Vec2};
use fitsviewer_core::{FitsImage,Statistics,Stretch,DisplayTone,display::{RenderOptions,render},regions::Aperture};
use serde_json::{json,Value};
use std::sync::Arc;
use crate::science_client::Client;
use serde::{Deserialize,Serialize};
pub enum Action {Select(usize),Export(Vec<Value>),Close(usize),CopySession}
pub struct Selection {pub path:String,pub hdu:Option<usize>,pub image:Option<Arc<FitsImage>>,pub view:Value,pub apertures:Vec<Aperture>}
#[derive(Clone,Serialize,Deserialize)]
#[serde(default)]
struct Overlay {enabled:bool,hidden:bool,opacity:f32,order:i32,label:String,epoch:String,scale:f64,offset:f64,unit:String,redshift:f64,air:bool}
impl Default for Overlay {fn default()->Self{Self{enabled:false,hidden:false,opacity:1.,order:0,label:String::new(),epoch:String::new(),scale:1.,offset:0.,unit:String::new(),redshift:0.,air:false}}}
/// A comparison holds the existing cached source, never a resampled pixel copy.
/// 对照谱复用缓存的原采样；显示比例与错位不修改源数据。
pub struct SpectrumLayer {pub image:Arc<FitsImage>,pub label:String,pub epoch:String,pub scale:f64,pub offset:f64,pub unit:String,pub redshift:f64,pub air:bool,pub opacity:f32,pub order:i32}
pub struct ImageLayer {pub index:usize,pub image:Arc<FitsImage>,pub preview:Option<Arc<FitsImage>>,pub label:String,pub opacity:f32,pub scale:f64,pub offset:f64,pub transform:[[f64;3];2],pub revision:u64,pub order:i32}
#[derive(Clone,Serialize,Deserialize)]
struct Alignment {reference:String,hdu:usize,matrix:[[f64;3];2]}
#[derive(Serialize,Deserialize)]
#[serde(default)]
struct ComparisonView{zoom:f32,pan:[f32;2],fit:bool}
impl Default for ComparisonView{fn default()->Self{Self{zoom:1.,pan:[0.,0.],fit:true}}}
impl ComparisonView{fn valid(&self)->bool{self.zoom.is_finite() && (0.005..=128.).contains(&self.zoom) && self.pan.iter().all(|v|v.is_finite() && v.abs()<=1e8)}}
struct Slot {path:String,hdu:Option<usize>,image:Option<Arc<FitsImage>>,preview:Option<Arc<FitsImage>>,view:Value,apertures:Vec<Aperture>,texture:Option<egui::TextureHandle>,zoom:f32,pan:Vec2,fit:bool,touched:u64,overlay:Overlay,revision:u64,alignment:Option<Alignment>,calibration:Value}
pub struct Workspace {slots:Vec<Slot>,pub selected:Option<usize>,secondary:Option<usize>,pub link:String,paths:String,open:bool,clock:u64,pub linked_center:Option<[f64;2]>,pub linked_ratio:f64,last_link:Option<[f64;3]>,pub message:String,pub pending_selection:Option<usize>,flux_mode:String,flux_aligned:bool,flux_allow_filter:bool,flux_options:Value,flux_target:Option<usize>}
impl Default for Workspace {fn default()->Self{Self{slots:vec![],selected:None,secondary:None,link:"Independent".into(),paths:String::new(),open:false,clock:0,linked_center:None,linked_ratio:1.,last_link:None,message:String::new(),pending_selection:None,flux_mode:"wcs".into(),flux_aligned:false,flux_allow_filter:false,
    flux_target:None,flux_options:json!({"aperture_radius":5.,"annulus_inner":8.,"annulus_outer":12.,"fwhm":3.,"threshold_sigma":5.,"match_radius":2.,"max_stars":200,"min_matches":5})}}}
impl Workspace {
    pub fn paths(&self)->Vec<String>{self.slots.iter().map(|slot|slot.path.clone()).collect()}
    pub fn files_open(&self)->bool{self.open}
    pub fn set_files_open(&mut self,open:bool){self.open=open;}
    pub fn secondary_open(&self)->bool{self.secondary.is_some()}
    pub fn close_secondary(&mut self){if let Some(index)=self.secondary.take(){if let Some(slot)=self.slots.get_mut(index){slot.texture=None;}}self.trim();}
    pub fn from_json(value:&Value)->Self{
        let mut workspace=Self::default();
        if let Some(views)=value["views"].as_array(){for view in views.iter().take(32){if let Some(path)=view["path"].as_str(){if let Some(index)=workspace.add(path.into(),view["hdu"].as_u64().map(|v|v as usize)){let slot=&mut workspace.slots[index];slot.view=view["view"].clone();slot.apertures=serde_json::from_value(view["apertures"].clone()).unwrap_or_default();if let Ok(v)=serde_json::from_value::<ComparisonView>(view["comparison_view"].clone()){if v.valid(){slot.zoom=v.zoom;slot.pan=egui::vec2(v.pan[0],v.pan[1]);slot.fit=v.fit;}}let _=workspace.configure_spectrum_overlay(index,&view["spectrum_overlay"]);}}}}
        workspace.selected=value["selected"].as_u64().map(|v|v as usize).filter(|i|*i<workspace.slots.len());
        workspace.secondary=value["secondary"].as_u64().map(|v|v as usize).filter(|i|*i<workspace.slots.len());
        workspace.link=value["link"].as_str().filter(|link|["Independent","Pixel","Sky"].contains(link)).unwrap_or("Independent").into();workspace
    }
    pub fn configuration(&self)->Value{json!({"schema":1,"selected":self.selected,"secondary":self.secondary,"link":self.link,"views":self.slots.iter().map(|s|json!({"path":s.path,"hdu":s.hdu,"view":s.view,"apertures":s.apertures,"spectrum_overlay":s.overlay,"comparison_view":ComparisonView{zoom:s.zoom,pan:[s.pan.x,s.pan.y],fit:s.fit}})).collect::<Vec<_>>()})}
    pub fn add_paths(&mut self,paths:&[String]){for path in paths{self.add(path.clone(),None);}}
    pub fn bytes(&self)->usize{self.slots.iter().map(|s|s.image.as_ref().map_or(0,|i|i.pixels.len()*(i.info.bitpix.unsigned_abs() as usize/8)+i.spectrum.as_ref().map_or(0,|p|p.x.len()*8+p.error.as_ref().map_or(0,|e|e.len()*8)))+s.preview.as_ref().map_or(0,|i|i.pixels.len()*(i.info.bitpix.unsigned_abs() as usize/8))+s.texture.as_ref().map_or(0,|t|t.size()[0]*t.size()[1]*4)).sum()}
    fn trim(&mut self){while self.bytes()>128*1024*1024 {let candidate=self.slots.iter().enumerate().filter(|(i,s)|Some(*i)!=self.selected && Some(*i)!=self.secondary && s.image.is_some()).min_by_key(|(_,s)|s.touched).map(|(i,_)|i);let Some(index)=candidate else{break;};self.slots[index].image=None;self.slots[index].preview=None;self.slots[index].texture=None;}}
    fn add(&mut self,path:String,hdu:Option<usize>)->Option<usize>{
        if let Some(index)=self.slots.iter().position(|s|s.path==path && (s.hdu==hdu || s.hdu.is_none())){return Some(index);}
        if self.slots.len()>=32{self.message="At most 32 views; close unused tabs".into();return None;}
        let overlay=Overlay{order:self.slots.len() as i32,..Default::default()};
        self.slots.push(Slot{path,hdu,image:None,preview:None,view:Value::Null,apertures:vec![],texture:None,zoom:1.,pan:Vec2::ZERO,fit:true,touched:0,overlay,revision:1,alignment:None,calibration:Value::Null});Some(self.slots.len()-1)
    }
    pub fn remember(&mut self,image:Arc<FitsImage>,view:Value,apertures:Vec<Aperture>){
        if let Some(index)=self.add(image.info.path.clone(),Some(image.info.hdu)){let slot=&mut self.slots[index];if slot.image.as_ref().is_none_or(|old|!Arc::ptr_eq(old,&image)){slot.revision+=1;slot.alignment=None;slot.preview=None;slot.calibration=Value::Null;}slot.hdu=Some(image.info.hdu);slot.image=Some(image);slot.texture=None;slot.view=view;slot.apertures=apertures;self.clock+=1;slot.touched=self.clock;self.selected=Some(index);}self.trim();
    }
    pub fn remember_preview(&mut self,preview:Option<Arc<FitsImage>>){if let Some(slot)=self.selected.and_then(|i|self.slots.get_mut(i)){if slot.image.as_ref().is_some_and(|i|matches!(i.pixels,fitsviewer_core::RawPixels::Unavailable)){slot.preview=preview;}}self.trim();}
    pub fn selection(&mut self,index:usize)->Option<Selection>{let slot=self.slots.get_mut(index)?;self.clock+=1;slot.touched=self.clock;self.selected=Some(index);Some(Selection{path:slot.path.clone(),hdu:slot.hdu,image:slot.image.clone().filter(|i|!matches!(i.pixels,fitsviewer_core::RawPixels::Unavailable)),view:slot.view.clone(),apertures:slot.apertures.clone()})}
    pub fn configure_spectrum_overlay(&mut self,index:usize,params:&Value)->Result<(),String>{
        let slot=self.slots.get(index).ok_or("Unknown view index")?;
        let mut next=slot.overlay.clone();
        if params.is_null(){return Ok(());}
        let entries=params.as_object().ok_or("Overlay settings must be an object")?;
        for (key,value) in entries{match key.as_str(){
            "enabled"=>next.enabled=value.as_bool().ok_or("Enabled must be boolean")?,
            "hidden"=>next.hidden=value.as_bool().ok_or("Hidden must be boolean")?,
            "opacity"=>next.opacity=value.as_f64().filter(|v|v.is_finite() && (0. ..=1.).contains(v)).ok_or("Opacity must be between 0 and 1")? as f32,
            "order"=>next.order=value.as_i64().filter(|v|(-10000..=10000).contains(v)).ok_or("Invalid layer order")? as i32,
            "label"=>next.label=value.as_str().filter(|v|v.chars().count()<=120).ok_or("Label must be at most 120 characters")?.into(),
            "epoch"=>next.epoch=value.as_str().filter(|v|v.chars().count()<=120).ok_or("Epoch must be at most 120 characters")?.into(),
            "scale"=>next.scale=value.as_f64().filter(|v|v.is_finite() && v.abs()<=1e12 && *v!=0.).ok_or("Scale must be finite, nonzero, at most 1e12")?,
            "offset"=>next.offset=value.as_f64().filter(|v|v.is_finite() && v.abs()<=1e30).ok_or("Offset must be finite and at most 1e30")?,
            "unit"=>next.unit=value.as_str().filter(|v|v.is_empty() || crate::spectral_lines::unit_factor(v).is_some()).ok_or("Choose Angstrom, nm or um")?.into(),
            "redshift"=>next.redshift=value.as_f64().filter(|v|v.is_finite() && *v> -1. && *v<=20.).ok_or("Redshift must be finite and -1 < z <= 20")?,
            "air"=>next.air=value.as_bool().ok_or("Air must be boolean")?,
            _=>return Err(format!("Unknown overlay setting: {key}")),
        }}
        if next.enabled && !slot.overlay.enabled && self.slots.iter().filter(|s|s.overlay.enabled).count()>=8{return Err("At most 8 enabled comparison spectra".into());}
        self.slots[index].overlay=next;self.slots[index].revision+=1;Ok(())
    }
    pub fn configure_layer(&mut self,index:usize,params:&Value)->Result<(),String>{self.configure_spectrum_overlay(index,params)}
    fn ordered_indices(&self)->Vec<usize>{let mut indices=(0..self.slots.len()).collect::<Vec<_>>();indices.sort_by_key(|&index|self.slots[index].overlay.order);indices}
    fn reorder_layer(&mut self,index:usize,up:bool){
        let indices=self.ordered_indices();let Some(position)=indices.iter().position(|&i|i==index)else{return;};
        let neighbor=if up{position+1}else{position.saturating_sub(1)};
        if neighbor>=indices.len() || neighbor==position{return;}
        // Re-number presentation order only. Stable file indices keep commands/owners valid.
        // 仅调整显示层序；文件索引保持稳定，避免后台请求落到另一文件。
        for (order,&i) in indices.iter().enumerate(){self.slots[i].overlay.order=order as i32;}
        self.slots[index].overlay.order=neighbor as i32;self.slots[indices[neighbor]].overlay.order=position as i32;
    }
    fn layer_is_active(slot:&Slot,active:&FitsImage)->bool{slot.path==active.info.path && slot.hdu==Some(active.info.hdu)}
    pub fn source_layer_style(&self,active:&FitsImage)->(bool,f32){self.slots.iter().find(|s|Self::layer_is_active(s,active)).map(|s|(!s.overlay.hidden,s.overlay.opacity)).unwrap_or((true,1.))}
    pub fn source_layer_order(&self,active:&FitsImage)->i32{self.slots.iter().find(|s|Self::layer_is_active(s,active)).map(|s|s.overlay.order).unwrap_or(-1)}
    pub fn invalidate_wcs(&mut self){
        // A changed celestial solution invalidates both cached geometry and in-flight matches.
        // 天球坐标解改变后清除旧配准，并使进行中的匹配结果失效；保留手动显示比例。
        for slot in &mut self.slots{slot.revision=slot.revision.saturating_add(1);slot.alignment=None;slot.calibration=Value::Null;}
    }
    pub fn image_layers(&self,active:&FitsImage)->Vec<ImageLayer>{
        self.ordered_indices().into_iter().filter_map(|index|{
            let slot=&self.slots[index];let image=slot.image.as_ref()?;
            if !slot.overlay.enabled || slot.overlay.hidden || image.spectrum.is_some() || Self::layer_is_active(slot,active){return None;}
            if image.pixels.is_empty() && slot.preview.is_none(){return None;}
            let transform=slot.alignment.as_ref().filter(|alignment|alignment.reference==active.info.path && alignment.hdu==active.info.hdu)
                .map(|alignment|alignment.matrix).unwrap_or([[1.,0.,0.],[0.,1.,0.]]);
            Some(ImageLayer{index,image:image.clone(),preview:slot.preview.clone(),label:if slot.overlay.label.is_empty(){slot.path.rsplit(['/', '\\']).next().unwrap_or(&slot.path).into()}else{slot.overlay.label.clone()},
                opacity:slot.overlay.opacity,scale:slot.overlay.scale,offset:slot.overlay.offset,transform,revision:slot.revision,order:slot.overlay.order})
        }).take(8).collect()
    }
    fn flux_source(&self,index:usize,active:&FitsImage)->String{let slot=&self.slots[index];format!("layer:{index}:{}#{}=>{}#{}",slot.path,slot.hdu.unwrap_or(0),active.info.path,active.info.hdu)}
    pub fn image_flux_request(&self,index:usize,active:&FitsImage,options:&Value)->Result<Value,String>{
        let slot=self.slots.get(index).ok_or("Unknown view index")?;let image=slot.image.as_ref().ok_or("Load the target layer first")?;
        if image.spectrum.is_some() || active.spectrum.is_some() || Self::layer_is_active(slot,active){return Err("Choose a different two-dimensional target layer".into());}
        let mut params=self.flux_options.clone();params["mode"]=json!(self.flux_mode);params["assume_aligned"]=json!(self.flux_aligned);params["allow_different_filters"]=json!(self.flux_allow_filter);
        if let Some(options)=options.as_object(){for (key,value) in options{params[key]=value.clone();}}
        params["path"]=json!(slot.path);params["hdu"]=json!(image.info.hdu);params["reference_path"]=json!(active.info.path);params["reference_hdu"]=json!(active.info.hdu);
        if !slot.view["wcs_override"].is_null(){params["wcs_override"]=slot.view["wcs_override"].clone();}
        params["source"]=json!(self.flux_source(index,active));params["revision"]=json!(slot.revision);Ok(params)
    }
    pub fn apply_flux_match(&mut self,value:&Value,active:Option<&FitsImage>)->Result<(),String>{
        let active=active.ok_or("Reference image is no longer open")?;
        let index=self.slots.iter().enumerate().find_map(|(index,slot)|(Some(self.flux_source(index,active).as_str())==value["source"].as_str() && value["revision"].as_u64()==Some(slot.revision)).then_some(index)).ok_or("Discarded an outdated layer result")?;
        let slot=&self.slots[index];
        let matches_path=|value:&Value,expected:&str|{let Some(actual)=value.as_str()else{return false;};let actual=actual.replace('\\',"/");let expected=fitsviewer_core::pasted_path(expected).replace('\\',"/");
            actual==expected || (!std::path::Path::new(&expected).is_absolute() && actual.ends_with(&format!("/{}",expected.trim_start_matches("./"))))};
        if value["hdu"].as_u64()!=slot.hdu.map(|h|h as u64) || value["reference_hdu"].as_u64()!=Some(active.info.hdu as u64)
            || !matches_path(&value["path"],&slot.path) || !matches_path(&value["reference_path"],&active.info.path){return Err("Layer result does not match its source and reference".into());}
        self.slots[index].calibration=value.clone();
        if value["status"]!="ok"{self.message=format!("{}: {}",crate::i18n::t("Flux match was not applied"),value["status"]);return Ok(());}
        let factor=value["factor"].as_f64().filter(|v|v.is_finite() && *v>0. && *v<=1e12).ok_or("Invalid relative flux factor")?;
        let mapping=&value["mapping"];let alignment=if mapping["status"]=="ok"{
            let matrix: [[f64;3];2]=serde_json::from_value(mapping["matrix"].clone()).map_err(|_|"Invalid layer geometry")?;
            if matrix.iter().flatten().any(|v|!v.is_finite() || v.abs()>1e9) || (matrix[0][0]*matrix[1][1]-matrix[0][1]*matrix[1][0]).abs()<1e-12{return Err("Invalid layer geometry".into());}
            Some(Alignment{reference:active.info.path.clone(),hdu:active.info.hdu,matrix})
        }else{None};
        let slot=&mut self.slots[index];slot.overlay.scale=factor;slot.overlay.offset=0.;slot.overlay.enabled=true;slot.overlay.hidden=false;slot.alignment=alignment;slot.revision+=1;
        self.message=format!("{}: ×{factor:.6} · {} {}",crate::i18n::t("Relative field-star scale applied"),value["used"],crate::i18n::t("stars; original data unchanged"));Ok(())
    }
    pub fn spectrum_layers(&self,active:&FitsImage)->Vec<SpectrumLayer>{
        self.ordered_indices().into_iter().map(|index|&self.slots[index]).filter(|s|s.overlay.enabled && !s.overlay.hidden).filter_map(|slot|{
            let image=slot.image.as_ref()?;let meta=image.info.spectrum.as_ref()?;
            if image.spectrum.is_none() || (image.info.path==active.info.path && image.info.hdu==active.info.hdu) || crate::spectral_lines::sample_axis(meta){return None;}
            let unit=if slot.overlay.unit.is_empty(){meta.x_unit.clone()}else{slot.overlay.unit.clone()};
            crate::spectral_lines::unit_factor(&unit)?;
            Some(SpectrumLayer{image:image.clone(),label:if slot.overlay.label.is_empty(){slot.path.rsplit(['/', '\\']).next().unwrap_or(&slot.path).into()}else{slot.overlay.label.clone()},
                epoch:slot.overlay.epoch.clone(),scale:slot.overlay.scale,offset:slot.overlay.offset,unit,redshift:slot.overlay.redshift,air:slot.overlay.air,opacity:slot.overlay.opacity,order:slot.overlay.order})
        }).take(8).collect()
    }
    pub fn spectrum_comparison_ui(&mut self,ui:&mut egui::Ui){
        self.layer_rows(ui,true,false);
        if !self.message.is_empty(){ui.label(crate::i18n::t(&self.message));}
    }
    pub fn image_layers_ui(&mut self,ui:&mut egui::Ui,active:Option<&FitsImage>,busy:bool)->Option<Value>{
        use crate::i18n::t;
        let mut reference=self.selected;let old_reference=reference;
        ui.label(t("Reference image"));
        egui::ComboBox::from_id_salt("brightness-reference").selected_text(reference.and_then(|i|self.slots.get(i)).map(|s|s.path.rsplit(['/', '\\']).next().unwrap_or(&s.path)).unwrap_or(t("Open a reference image"))).show_ui(ui,|ui|{
            for (index,slot) in self.slots.iter().enumerate().filter(|(_,s)|s.image.as_ref().is_some_and(|image|image.spectrum.is_none())){ui.selectable_value(&mut reference,Some(index),slot.path.rsplit(['/', '\\']).next().unwrap_or(&slot.path));}
        });
        if reference!=old_reference{self.pending_selection=reference;}
        if self.flux_target.is_none_or(|index|Some(index)==self.selected || self.slots.get(index).is_none_or(|s|s.image.as_ref().is_none_or(|image|image.spectrum.is_some()))){self.flux_target=self.slots.iter().enumerate().find(|(i,s)|Some(*i)!=self.selected && s.image.as_ref().is_some_and(|image|image.spectrum.is_none())).map(|(i,_)|i);}
        ui.label(t("Target image"));
        egui::ComboBox::from_id_salt("brightness-target").selected_text(self.flux_target.and_then(|i|self.slots.get(i)).map(|s|s.path.rsplit(['/', '\\']).next().unwrap_or(&s.path)).unwrap_or(t("Load a second image first"))).show_ui(ui,|ui|{
            for (index,slot) in self.slots.iter().enumerate().filter(|(i,s)|Some(*i)!=self.selected && s.image.as_ref().is_some_and(|image|image.spectrum.is_none())){ui.selectable_value(&mut self.flux_target,Some(index),slot.path.rsplit(['/', '\\']).next().unwrap_or(&slot.path));}
        });
        let ready=!busy && reference==old_reference && self.flux_target.is_some() && active.is_some() && (self.flux_mode!="pixel" || self.flux_aligned);
        let target=if ui.add_enabled(ready,egui::Button::new(t("▶ Match image brightness"))).clicked(){self.flux_target}else{None};
        ui.horizontal_wrapped(|ui|{ui.label(t("Matching coordinates"));egui::ComboBox::from_id_salt("layer-match-mode").selected_text(t(&self.flux_mode)).show_ui(ui,|ui|{for mode in ["wcs","pixel"]{ui.selectable_value(&mut self.flux_mode,mode.into(),t(mode));}});});
        if self.flux_mode=="pixel"{ui.checkbox(&mut self.flux_aligned,t("I confirm the pixel grids are already aligned"));}
        ui.weak(t("Relative native-count scaling from matched stars; no absolute zero point or exposure correction."));
        if self.flux_allow_filter{ui.colored_label(egui::Color32::YELLOW,t("Different / unverified filters allowed: display approximation only."));}
        if let Some(slot)=self.flux_target.and_then(|index|self.slots.get(index)){if !slot.calibration.is_null(){ui.label(format!("{} · {} {} · {} {}",slot.calibration["status"].as_str().unwrap_or("unknown"),slot.calibration["used"],t("stars"),t("Relative scale"),slot.calibration["factor"]));}}
        if !self.message.is_empty(){ui.label(t(&self.message));}
        egui::CollapsingHeader::new(t("Advanced matching settings")).id_salt("layer-flux-options").show(ui,|ui|{
            ui.checkbox(&mut self.flux_allow_filter,t("Allow unverified or different filters (display approximation)"));
            egui::Grid::new("field-star-parameters").num_columns(2).spacing(egui::vec2(8.,6.)).show(ui,|ui|{
                for (key,label,minimum,maximum) in [("fwhm","FWHM (pixels)",1.,30.),("aperture_radius","Aperture radius",1.,64.),("annulus_inner","Background inner radius",1.,127.),("annulus_outer","Background outer radius",2.,128.),("threshold_sigma","Detection threshold sigma",3.,30.),("match_radius","Matching radius",0.1,30.)]{
                    let mut value=self.flux_options[key].as_f64().unwrap_or(minimum);ui.label(t(label));if ui.add(egui::DragValue::new(&mut value).speed(0.1).range(minimum..=maximum)).changed(){self.flux_options[key]=json!(value);}ui.end_row();
                }
                let mut minimum=self.flux_options["min_matches"].as_u64().unwrap_or(5) as usize;ui.label(t("Minimum matched stars"));if ui.add(egui::DragValue::new(&mut minimum).range(3..=200)).changed(){self.flux_options["min_matches"]=json!(minimum);}ui.end_row();
            });
        });
        ui.separator();self.layer_rows(ui,false,busy);
        target.and_then(|index|match active.map(|active|self.image_flux_request(index,active,&json!({}))){Some(Ok(params))=>Some(params),Some(Err(error))=>{self.message=error;None},None=>None})
    }
    fn layer_rows(&mut self,ui:&mut egui::Ui,spectrum:bool,_busy:bool)->Option<usize>{
        use crate::i18n::t;
        let mut changes=Vec::new();let mut moves=Vec::new();let mut selection=None;let match_target=None;
        for index in self.ordered_indices().into_iter().rev(){
            let slot=&self.slots[index];if slot.image.as_ref().is_some_and(|image|image.spectrum.is_some()!=spectrum){continue;}
            let active=Some(index)==self.selected;
            ui.push_id(("layer",index),|ui|{egui::Frame::group(ui.style()).inner_margin(6.).show(ui,|ui|{
                ui.set_width(ui.available_width());crate::ribbon::align_controls(ui);
                let filename=slot.path.rsplit(['/', '\\']).next().unwrap_or(&slot.path);let mut visible=if active{!slot.overlay.hidden}else{slot.overlay.enabled && !slot.overlay.hidden};
                ui.horizontal(|ui|{
                    if ui.checkbox(&mut visible,"").on_hover_text(t("Layer visibility")).changed(){changes.push((index,json!({"hidden":!visible,"enabled":if active{slot.overlay.enabled}else{visible}})));}
                    let size=crate::workspace_ui::CONTROL.max(ui.spacing().interact_size.y);let width=(ui.available_width()-3.*size-3.*ui.spacing().item_spacing.x).max(24.);
                    let name=if active{format!("{} · {filename}",t("Reference"))}else{filename.into()};
                    if ui.add_sized([width,size],egui::Button::selectable(active,name).truncate()).on_hover_text(format!("{}\n{}",slot.path,t("Select as reference / load layer"))).clicked(){selection=Some(index);}
                    if crate::workspace_ui::icon_button(ui,crate::workspace_ui::Icon::Up,t("Raise layer")).clicked(){moves.push((index,true));}
                    if crate::workspace_ui::icon_button(ui,crate::workspace_ui::Icon::Down,t("Lower layer")).clicked(){moves.push((index,false));}
                    crate::pro::information(ui,"layer-info",|ui|{
                        ui.label(&slot.path);
                        if active{ui.label(t("Reference layer; measurements use its original samples."));}
                        if !spectrum && !active{
                            let registered=slot.alignment.as_ref().is_some_and(|a|self.selected.and_then(|i|self.slots[i].image.as_ref()).is_some_and(|reference|a.reference==reference.info.path && a.hdu==reference.info.hdu));
                            ui.label(t(if registered{"Geometry mapped to the current reference"}else{"Pixel preview only; alignment has not been established"}));
                        }
                    });
                });
                let mut opacity=slot.overlay.opacity;
                if ui.add(egui::Slider::new(&mut opacity,0. ..=1.).text(t("Opacity"))).changed(){changes.push((index,json!({"opacity":opacity})));}
                let Some(image)=&slot.image else{if ui.small_button(t("Load layer / make reference")).clicked(){selection=Some(index);}return;};
                egui::CollapsingHeader::new(t("Layer properties")).id_salt("layer-properties").show(ui,|ui|{
                    let mut label=slot.overlay.label.clone();ui.horizontal(|ui|{ui.label(t("Label"));if ui.add(egui::TextEdit::singleline(&mut label).char_limit(120)).changed(){changes.push((index,json!({"label":label})));}});
                    if spectrum{let mut epoch=slot.overlay.epoch.clone();ui.horizontal(|ui|{ui.label(t("Epoch / phase"));if ui.add(egui::TextEdit::singleline(&mut epoch).char_limit(120)).changed(){changes.push((index,json!({"epoch":epoch})));}});}
                    if !active{let mut scale=slot.overlay.scale;let mut offset=slot.overlay.offset;
                        ui.horizontal(|ui|{ui.label(t("Scale"));let speed=scale.abs()*0.01;if ui.add(egui::DragValue::new(&mut scale).speed(speed).custom_formatter(|v,_|format!("{v:.5e}")).custom_parser(|text|text.trim().parse().ok())).changed(){changes.push((index,json!({"scale":scale})));}});
                        let sample_scale=(0..image.pixels.len().min(128)).filter_map(|i|image.physical(i)).map(f64::abs).fold(0_f64,f64::max);
                        ui.horizontal(|ui|{ui.label(t("Offset"));let speed=offset.abs().max(sample_scale*0.001).max(f64::MIN_POSITIVE*1024.)*0.01;if ui.add(egui::DragValue::new(&mut offset).speed(speed).custom_formatter(|v,_|format!("{v:.5e}")).custom_parser(|text|text.trim().parse().ok())).changed(){changes.push((index,json!({"offset":offset})));}});
                    }
                    if spectrum && !active{
                        let meta=image.info.spectrum.as_ref().unwrap();let mut unit=if slot.overlay.unit.is_empty(){meta.x_unit.clone()}else{slot.overlay.unit.clone()};let old=unit.clone();
                        egui::ComboBox::from_id_salt("overlay-unit").selected_text(if unit.is_empty(){t("Select unit")}else{&unit}).show_ui(ui,|ui|{for value in ["Angstrom","nm","um"]{ui.selectable_value(&mut unit,value.into(),value);}});
                        if old!=unit{changes.push((index,json!({"unit":unit})));}if crate::spectral_lines::unit_factor(&unit).is_none(){ui.weak(t("Confirm wavelength unit before drawing this comparison."));}
                        let mut z=slot.overlay.redshift;let mut air=slot.overlay.air;ui.horizontal(|ui|{ui.label(t("Redshift z"));if ui.add(egui::DragValue::new(&mut z).speed(0.00001).range(-0.999..=20.)).changed(){changes.push((index,json!({"redshift":z})));}});
                        if ui.checkbox(&mut air,t("Observed axis is air wavelength")).changed(){changes.push((index,json!({"air":air})));}
                    }
                });
                if !spectrum && !active{
                    if !slot.calibration.is_null(){
                        ui.label(format!("{} · {} {} · {} {:.5}",slot.calibration["status"].as_str().unwrap_or("unknown"),slot.calibration["used"],t("stars"),t("Relative scale"),slot.calibration["factor"].as_f64().unwrap_or(f64::NAN)));
                        egui::CollapsingHeader::new(t("Calibration details")).id_salt("layer-calibration-details").show(ui,|ui|{
                        ui.add(egui::Label::new(serde_json::to_string_pretty(&slot.calibration).unwrap_or_default()).wrap());
                        if ui.button(t("Copy result")).clicked(){ui.ctx().copy_text(serde_json::to_string_pretty(&slot.calibration).unwrap_or_default());}
                    });}
                }
            });});
        }
        for (index,params) in changes{if let Err(error)=self.configure_layer(index,&params){self.message=error;}}
        for (index,up) in moves{self.reorder_layer(index,up);}if selection.is_some(){self.pending_selection=selection;}match_target
    }
    pub fn remove(&mut self,index:usize)->Result<(bool,Option<Selection>),String>{
        if index>=self.slots.len(){return Err("Unknown view index".into());}
        let active=self.selected==Some(index);self.slots.remove(index);
        self.selected=self.selected.and_then(|s|if s==index{None}else{Some(s-usize::from(s>index))});
        self.secondary=self.secondary.and_then(|s|if s==index{None}else{Some(s-usize::from(s>index))});
        self.last_link=None;self.linked_center=None;
        let next=if active && !self.slots.is_empty(){self.selection(index.min(self.slots.len()-1))}else{None};Ok((active,next))
    }
    pub fn toolbar(&mut self,ctx:&egui::Context)->Option<Action>{
        let mut action=None;let mut close=None;
        egui::TopBottomPanel::top("multi_tabs").show(ctx,|ui|{
            crate::ribbon::align_controls(ui);
            ui.horizontal_wrapped(|ui|{
                if ui.button(crate::i18n::t("Add files…")).clicked(){self.open=true;}
                if ui.button(crate::i18n::t("Copy session JSON")).clicked(){action=Some(Action::CopySession);}
                for (index,slot) in self.slots.iter().enumerate(){
                    let label=slot.path.rsplit(['/', '\\']).next().unwrap_or(&slot.path);
                    if ui.selectable_label(self.selected==Some(index),format!("{label} [{}]",slot.hdu.map(|h|h.to_string()).unwrap_or_else(||"auto".into()))).clicked(){action=Some(Action::Select(index));}
                    if ui.button("×").on_hover_text(format!("{}: {}",crate::i18n::t("Remove view"),slot.path)).clicked(){close=Some(index);}
                }
                ui.label(format!("{} {:.1} MiB",crate::i18n::t("Cache"),self.bytes() as f64/1048576.)).on_hover_text(crate::i18n::t("128 MiB LRU budget; active main and comparison images may exceed it."));
                if !self.message.is_empty(){ui.label(crate::i18n::t(&self.message));}
            });
        });
        if let Some(index)=close{action=Some(Action::Close(index));}
        action
    }
    /// A fixed-width group keeps long filenames from making the ribbon jump rows.
    /// 比较控件整体上移；固定下拉框宽度，长文件名不会引发工具栏反复换行。
    pub fn comparison_width(ui:&egui::Ui)->f32 {
        let font=egui::TextStyle::Button.resolve(ui.style());let color=ui.visuals().text_color();
        let width=|text:&str|ui.fonts(|fonts|fonts.layout_no_wrap(crate::i18n::t(text).into(),font.clone(),color).size().x);
        let button=|text:&str|(width(text)+2.*ui.spacing().button_padding.x).max(ui.spacing().interact_size.x);
        button("Secondary view")+170.+Self::link_width(ui)+button("Export all views")+3.*ui.spacing().item_spacing.x
    }
    fn link_width(ui:&egui::Ui)->f32 {
        let font=egui::TextStyle::Button.resolve(ui.style());let color=ui.visuals().text_color();
        ["Independent","Pixel","Sky"].into_iter().map(|text|ui.fonts(|fonts|fonts.layout_no_wrap(crate::i18n::t(text).into(),font.clone(),color).size().x)).fold(0.,f32::max)
            +2.*ui.spacing().button_padding.x+ui.spacing().icon_width+ui.spacing().icon_spacing+2.
    }
    // Nested choices must use a submenu: a ComboBox would replace its parent popup.
    // 菜单内选项使用原生子菜单；嵌套 ComboBox 会替换并关闭父弹窗。
    fn comparison_choice(ui:&mut egui::Ui,compact:bool,id:&str,width:f32,caption:String,body:impl FnOnce(&mut egui::Ui))->egui::Response{
        if compact{ui.menu_button(caption,body).response}
        else{egui::ComboBox::from_id_salt(id).width(width).selected_text(caption).show_ui(ui,body).response}
    }
    pub fn comparison_controls(&mut self,ui:&mut egui::Ui,compact:bool)->Option<Action>{
        let mut action=None;
        ui.push_id("image-comparison-controls",|ui|{
            if ui.button(crate::i18n::t("Secondary view")).clicked(){self.secondary=self.slots.iter().enumerate().find(|(i,s)|Some(*i)!=self.selected && s.image.as_ref().is_some_and(|image|image.spectrum.is_none())).map(|(i,_)|i);if self.secondary.is_none(){self.message="Open a second file first, then select the main tab.".into();}}
            let title=self.secondary.and_then(|i|self.slots.get(i)).map(|s|s.path.rsplit(['/', '\\']).next().unwrap_or(&s.path)).unwrap_or(crate::i18n::t("Choose comparison"));
            let available=170.-2.*ui.spacing().button_padding.x-ui.spacing().icon_width-ui.spacing().icon_spacing-2.;
            let caption=crate::ribbon::path_caption(ui,title,available);
            Self::comparison_choice(ui,compact,"secondary-file",170.,caption,|ui|{for (i,s) in self.slots.iter().enumerate().filter(|(_,s)|s.image.as_ref().is_some_and(|image|image.spectrum.is_none())){ui.selectable_value(&mut self.secondary,Some(i),&s.path);}}).on_hover_text(title);
            Self::comparison_choice(ui,compact,"view-link",Self::link_width(ui),crate::i18n::t(&self.link).into(),|ui|{for mode in ["Independent","Pixel","Sky"]{if ui.selectable_value(&mut self.link,mode.into(),crate::i18n::t(mode)).changed(){self.last_link=None;self.linked_center=None;}}});
            if ui.button(crate::i18n::t("Export all views")).clicked(){action=Some(Action::Export(self.slots.iter().map(|s|json!({"path":s.path,"hdu":s.hdu,"view":s.view,"apertures":s.apertures})).collect()));}
        });action
    }
    /// Inline spectrum file tabs share the caller's single compact toolbar row.
    /// 光谱文件标签嵌入调用者的紧凑单行，不再另建顶部面板。
    pub fn inline_spectrum(&mut self,ui:&mut egui::Ui)->Option<Action>{
        crate::ribbon::align_controls(ui);
        let mut action=None;
        ui.menu_button(crate::i18n::t("Files"),|ui|{
            if ui.button(crate::i18n::t("Add files…")).clicked(){self.open=true;ui.close();}
            if ui.button(crate::i18n::t("Copy session JSON")).clicked(){action=Some(Action::CopySession);ui.close();}
        });
        for (index,slot) in self.slots.iter().enumerate(){
            let label=slot.path.rsplit(['/', '\\']).next().unwrap_or(&slot.path);
            if ui.selectable_label(self.selected==Some(index),format!("{label} [{}]",slot.hdu.map(|h|h.to_string()).unwrap_or_else(||"auto".into()))).on_hover_text(&slot.path).clicked(){action=Some(Action::Select(index));}
            if ui.button("×").on_hover_text(crate::i18n::t("Remove view")).clicked(){action=Some(Action::Close(index));}
        }
        ui.weak(format!("{:.0}M",self.bytes() as f64/1048576.)).on_hover_text(format!("{}: {:.1} MiB · {}",crate::i18n::t("Cache"),self.bytes() as f64/1048576.,crate::i18n::t("128 MiB LRU budget; active main and comparison images may exceed it.")));
        action
    }
    pub fn files_ui(&mut self,ui:&mut egui::Ui){
        crate::pro::information(ui,"files-help",|ui|{ui.label(crate::i18n::t("One path per line; files load only when selected."));});
            ui.add(egui::TextEdit::multiline(&mut self.paths).desired_width(500.).desired_rows(8));
            if ui.button(crate::i18n::t("Add to workspace")).clicked(){let paths=self.paths.lines().map(str::trim).filter(|s|!s.is_empty()).map(|s|fitsviewer_core::pasted_path(s).to_string()).collect::<Vec<_>>();for path in paths{self.add(path,None);}self.paths.clear();}
    }
    pub fn secondary_ui(&mut self,ui:&mut egui::Ui,ctx:&egui::Context,client:&Client,generation:u64,main:&FitsImage,center:[f64;2],zoom:f32,source_wcs:Option<&str>){
        if main.spectrum.is_some(){return;}
        let Some(index)=self.secondary.filter(|i|*i<self.slots.len())else{ui.label(crate::i18n::t("Choose comparison image"));return;};
        let slot=&mut self.slots[index];let Some(image)=slot.image.as_ref()else{ui.label(&slot.path);if ui.button(crate::i18n::t("Select as reference / load layer")).clicked(){self.pending_selection=Some(index);}return;};
        if image.spectrum.is_some(){self.message="Spectrum comparison uses separate tabs; image view linking needs two-dimensional images.".into();return;}
        let display=if image.pixels.is_empty(){let Some(preview)=slot.preview.as_deref() else{ui.label(crate::i18n::t("Select as reference / load layer"));return;};preview}else{image.as_ref()};
        if self.link=="Sky" && self.last_link.is_none_or(|v|(v[0]-center[0]).abs()>0.1 || (v[1]-center[1]).abs()>0.1 || (v[2]-f64::from(zoom)).abs()>0.001){
            self.last_link=Some([center[0],center[1],f64::from(zoom)]);
            client.request(ctx,generation,"view_link",json!({"source_path":main.info.path,"source_hdu":main.info.hdu,"source_wcs_override":source_wcs,"wcs_override":slot.view["wcs_override"],"path":slot.path,"hdu":slot.hdu,"center":center}));self.linked_center=None;
        }
            ui.horizontal(|ui|{ui.label(&slot.path);if ui.button(crate::i18n::t("Fit")).clicked(){slot.fit=true;}});
            let (response,painter)=ui.allocate_painter(Vec2::new(ui.available_width().max(100.),(ui.clip_rect().bottom()-ui.cursor().min.y).clamp(100.,1600.)),egui::Sense::click_and_drag());let area=response.rect;
            let (view_center,view_zoom)=if self.link=="Pixel"{(center,zoom)}else if self.link=="Sky" && self.linked_center.is_some(){(self.linked_center.unwrap(),(f64::from(zoom)/self.linked_ratio) as f32)}else{
                if slot.fit{slot.zoom=((area.width()-16.)/image.info.width as f32).min((area.height()-16.)/image.info.height as f32).max(0.005);}
                if response.dragged_by(egui::PointerButton::Primary) || response.dragged_by(egui::PointerButton::Middle){slot.pan+=response.drag_delta();slot.fit=false;}
                if response.hovered(){let delta=ctx.input(|i|i.smooth_scroll_delta.y);if delta!=0.{slot.zoom=(slot.zoom*(delta/200.).exp()).clamp(0.005,128.);slot.fit=false;}}
                ([(image.info.width as f64-1.)/2.-f64::from(slot.pan.x/slot.zoom),(image.info.height as f64-1.)/2.+f64::from(slot.pan.y/slot.zoom)],slot.zoom)
            };
            if slot.texture.is_none(){
                let stats=Statistics::new(display);let (lo,hi)=stats.bounds(fitsviewer_core::Limits::ZScale);
                let low=slot.view["low"].as_f64().unwrap_or(lo);let high=slot.view["high"].as_f64().unwrap_or(hi);
                let stretch=slot.view["stretch"].as_str().and_then(Stretch::parse).unwrap_or(Stretch::Linear);
                let render_options:RenderOptions=serde_json::from_value(slot.view["render"].clone()).unwrap_or_default();
                let bytes=render(display,low,high,stretch,slot.view["invert"].as_bool().unwrap_or(false),DisplayTone{bias:slot.view["bias"].as_f64().unwrap_or(0.5),contrast:slot.view["contrast"].as_f64().unwrap_or(1.)},&render_options);
                slot.texture=Some(ctx.load_texture(format!("compare-{index}"),egui::ColorImage::from_rgba_unmultiplied([display.info.width,display.info.height],&bytes),egui::TextureOptions::NEAREST));
            }
            if let Some(texture)=&slot.texture{
                let size=Vec2::new(image.info.width as f32,image.info.height as f32)*view_zoom;
                let top_left=area.center()-egui::vec2((view_center[0] as f32+0.5)*view_zoom,(image.info.height as f32-view_center[1] as f32-0.5)*view_zoom);
                let rect=Rect::from_min_size(top_left,size);
                painter.image(texture.id(),rect,Rect::from_min_max(Pos2::ZERO,Pos2::new(1.,1.)),Color32::WHITE);
                for aperture in &slot.apertures{crate::regions_ui::draw_shape(&painter,rect,view_zoom,&aperture.shape,Color32::YELLOW);}
            }
        self.trim();
    }
}

#[cfg(test)]
mod spectrum_comparison_tests{
    use super::*;
    fn spectrum(path:&str)->Arc<FitsImage>{
        let meta=fitsviewer_core::SpectrumMeta{x_label:"Wavelength".into(),x_unit:"Angstrom".into(),y_label:"Flux".into(),y_unit:String::new(),columns:vec![],x_column:0,y_column:1,error_column:Some(2),error_hdu:None,uncertainty_kind:"sigma".into(),note:String::new()};
        Arc::new(FitsImage{info:fitsviewer_core::ImageInfo{path:path.into(),hdu:1,width:2,height:1,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:Some(meta)},pixels:fitsviewer_core::RawPixels::F64(vec![1.,2.]),spectrum:Some(fitsviewer_core::SpectrumData{x:vec![4000.,5000.],error:Some(vec![0.1,0.2])})})
    }
    #[test]fn comparison_reuses_source_arrays_and_counts_axis_error_memory(){
        let a=spectrum("epoch1");let b=spectrum("epoch2");let mut multi=Workspace::default();
        multi.remember(a.clone(),json!({}),vec![]);multi.configure_spectrum_overlay(0,&json!({"enabled":true,"epoch":"-5 d","scale":2.,"offset":3.,"redshift":0.01})).unwrap();
        multi.remember(b.clone(),json!({}),vec![]);
        assert_eq!(multi.bytes(),96);let layers=multi.spectrum_layers(&b);assert_eq!(layers.len(),1);
        assert!(Arc::ptr_eq(&layers[0].image,&a));assert_eq!(layers[0].image.spectrum.as_ref().unwrap().x,vec![4000.,5000.]);
        assert_eq!(layers[0].scale,2.);assert_eq!(layers[0].offset,3.);assert_eq!(layers[0].epoch,"-5 d");
        assert_eq!(a.physical(0),Some(1.));assert_eq!(layers[0].redshift,0.01);
        let json=multi.configuration();let restored=Workspace::from_json(&json);assert_eq!(restored.configuration(),json);
        assert!(restored.spectrum_layers(&b).is_empty()); // Session restore does not eagerly read spectra.
    }
    #[test]fn invalid_overlay_updates_are_atomic_and_unknown_units_do_not_plot(){
        let mut multi=Workspace::default();let a=spectrum("source");let b=spectrum("active");multi.remember(a,json!({}),vec![]);
        let before=multi.configuration();
        assert!(multi.configure_spectrum_overlay(0,&json!({"scale":2.,"unit":"pixel"})).is_err());assert_eq!(multi.configuration(),before);
        assert!(multi.configure_spectrum_overlay(0,&json!({"enabled":true,"redshift":-1.})).is_err());assert_eq!(multi.configuration(),before);
        multi.configure_spectrum_overlay(0,&json!({"enabled":true})).unwrap();assert_eq!(multi.spectrum_layers(&b).len(),1);
        Arc::get_mut(multi.slots[0].image.as_mut().unwrap()).unwrap().info.spectrum.as_mut().unwrap().x_unit.clear();
        assert!(multi.spectrum_layers(&b).is_empty());multi.configure_spectrum_overlay(0,&json!({"unit":"nm"})).unwrap();assert_eq!(multi.spectrum_layers(&b).len(),1);
        multi.slots[0].image=None;assert!(multi.spectrum_layers(&b).is_empty());
    }
    fn image(path:&str)->Arc<FitsImage>{Arc::new(FitsImage{info:fitsviewer_core::ImageInfo{path:path.into(),hdu:0,width:2,height:2,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:None},pixels:fitsviewer_core::RawPixels::F64(vec![1.,2.,3.,4.]),spectrum:None})}
    #[test]fn layer_order_visibility_opacity_do_not_reorder_file_indices_or_copy_pixels(){
        let mut multi=Workspace::default();let a=image("reference");let b=image("target");
        multi.remember(a.clone(),json!({}),vec![]);multi.remember(b.clone(),json!({}),vec![]);multi.selection(0);
        multi.configure_layer(1,&json!({"enabled":true,"opacity":0.25,"scale":2.})).unwrap();
        assert_eq!(multi.image_layers(&a)[0].opacity,0.25);assert!(Arc::ptr_eq(&multi.image_layers(&a)[0].image,&b));
        multi.reorder_layer(0,true);assert_eq!(multi.slots[0].path,"reference");assert_eq!(multi.slots[1].path,"target");assert_eq!(multi.selected,Some(0));
        assert!(multi.source_layer_order(&a)>multi.image_layers(&a)[0].order);
        multi.configure_layer(0,&json!({"hidden":true,"opacity":0.4})).unwrap();assert_eq!(multi.source_layer_style(&a),(false,0.4));
        multi.configure_layer(1,&json!({"hidden":true})).unwrap();assert!(multi.image_layers(&a).is_empty());assert_eq!(b.physical(0),Some(1.));
    }
    #[test]fn field_star_result_requires_current_revision_source_reference_and_valid_geometry(){
        let mut multi=Workspace::default();let a=image("/data/reference.fits");let b=image("/data/target.fits");
        multi.remember(a.clone(),json!({}),vec![]);multi.remember(b.clone(),json!({}),vec![]);multi.selection(0);
        let request=multi.image_flux_request(1,&a,&json!({"mode":"pixel","assume_aligned":true})).unwrap();
        let value=json!({"operation":"image_flux_match","source":request["source"],"revision":request["revision"],"path":b.info.path,"hdu":0,
            "reference_path":a.info.path,"reference_hdu":0,"status":"ok","factor":2.,"used":8,"mapping":{"status":"ok","matrix":[[1.,0.,3.],[0.,1.,-2.]]}});
        let mut wrong=value.clone();wrong["reference_hdu"]=json!(1);assert!(multi.apply_flux_match(&wrong,Some(&a)).is_err());assert_eq!(multi.slots[1].overlay.scale,1.);
        let mut wrong=value.clone();wrong["reference_path"]=json!("/elsewhere/reference.fits");assert!(multi.apply_flux_match(&wrong,Some(&a)).is_err());
        let mut invalid=value.clone();invalid["mapping"]["matrix"]=json!([[0.,0.,0.],[0.,0.,0.]]);assert!(multi.apply_flux_match(&invalid,Some(&a)).is_err());assert_eq!(multi.slots[1].overlay.scale,1.);
        multi.apply_flux_match(&value,Some(&a)).unwrap();let layers=multi.image_layers(&a);assert_eq!(layers[0].scale,2.);assert_eq!(layers[0].transform,[[1.,0.,3.],[0.,1.,-2.]]);assert_eq!(b.physical(0),Some(1.));
        assert!(multi.apply_flux_match(&value,Some(&a)).is_err()); // same reply cannot apply twice
        let request=multi.image_flux_request(1,&a,&json!({})).unwrap();let failed=json!({"source":request["source"],"revision":request["revision"],"path":b.info.path,"hdu":0,"reference_path":a.info.path,"reference_hdu":0,"status":"insufficient_matches","factor":null});
        multi.apply_flux_match(&failed,Some(&a)).unwrap();assert_eq!(multi.slots[1].overlay.scale,2.);
        multi.configure_layer(1,&json!({"opacity":0.5})).unwrap();let mut stale=value.clone();stale["revision"]=request["revision"].clone();assert!(multi.apply_flux_match(&stale,Some(&a)).is_err());
        let other=image("/data/other.fits");assert_eq!(multi.image_layers(&other)[0].transform,[[1.,0.,0.],[0.,1.,0.]]);
    }
    #[test]fn changed_wcs_drops_cached_alignment_and_rejects_pending_flux_match(){
        let mut multi=Workspace::default();let a=image("reference");let b=image("target");
        multi.remember(a.clone(),json!({}),vec![]);multi.remember(b.clone(),json!({}),vec![]);multi.selection(0);
        let request=multi.image_flux_request(1,&a,&json!({})).unwrap();
        let mut result=json!({"operation":"image_flux_match","source":request["source"],"revision":request["revision"],"path":"target","hdu":0,
            "reference_path":"reference","reference_hdu":0,"status":"ok","factor":2.,"used":8,"mapping":{"status":"ok","matrix":[[1.,0.,3.],[0.,1.,-2.]]}});
        multi.apply_flux_match(&result,Some(&a)).unwrap();assert_eq!(multi.image_layers(&a)[0].transform[0][2],3.);
        let pending=multi.image_flux_request(1,&a,&json!({})).unwrap();result["revision"]=pending["revision"].clone();
        multi.invalidate_wcs();assert!(multi.apply_flux_match(&result,Some(&a)).is_err());
        let layer=&multi.image_layers(&a)[0];assert_eq!(layer.transform,[[1.,0.,0.],[0.,1.,0.]]);assert_eq!(layer.scale,2.);
        assert!(multi.slots[1].calibration.is_null());assert_eq!(b.physical(0),Some(1.));
    }
}


