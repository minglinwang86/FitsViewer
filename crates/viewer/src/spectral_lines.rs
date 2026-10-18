//! PRO-only wavelength markers and provisional feature matches, never flux edits.
//! PRO 专用波长标线与初步候选匹配，不修改流量或原始波长。
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use fitsviewer_core::{FitsImage, SpectrumMeta};
use serde_json::{json, Value};
use crate::i18n::t;

pub struct Line {pub name:&'static str,pub vacuum:f64}
// Reference wavelengths in vacuum Angstrom, SDSS SPECTRO1D (not an exhaustive atomic list).
// 真空埃参考波长；部分为 SDSS 采用的复合特征参考值，不冒充精密原子分量。
// https://classic.sdss.org/dr7/algorithms/linestable.php
pub const LINES:&[Line]=&[
    Line{name:"Ly alpha",vacuum:1215.24},Line{name:"C IV blend",vacuum:1549.48},
    Line{name:"C III] blend",vacuum:1908.734},Line{name:"Mg II blend",vacuum:2799.117},
    Line{name:"[O II] 3727",vacuum:3727.092},Line{name:"[O II] 3730",vacuum:3729.875},
    Line{name:"Ca II K",vacuum:3934.777},Line{name:"Ca II H",vacuum:3969.588},
    Line{name:"H delta",vacuum:4102.89},Line{name:"H gamma",vacuum:4341.68},
    Line{name:"He II",vacuum:1640.4},Line{name:"H beta",vacuum:4862.68},
    Line{name:"[O III] 4960",vacuum:4960.295},Line{name:"[O III] 5008",vacuum:5008.240},
    Line{name:"[N II] 6550",vacuum:6549.86},Line{name:"H alpha",vacuum:6564.61},
    Line{name:"[N II] 6585",vacuum:6585.27},Line{name:"[S II] 6718",vacuum:6718.29},
    Line{name:"[S II] 6733",vacuum:6732.67},Line{name:"Ca II 8500",vacuum:8500.36},
    Line{name:"Ca II 8544",vacuum:8544.44},Line{name:"Ca II 8665",vacuum:8664.52},
];
const GROUPS:&[(&str,&[usize])]=&[
    ("Balmer group",&[8,9,11,15]),("[O II] doublet",&[4,5]),("[O III] doublet",&[12,13]),
    ("[N II] doublet",&[14,16]),("[S II] doublet",&[17,18]),("Ca II H/K",&[6,7]),("Ca II triplet",&[19,20,21]),
];

fn reserve_label(clip:Rect,x:f32,size:egui::Vec2,top:bool,occupied:&mut Vec<Rect>)->Option<Pos2>{
    if size.x>clip.width() || size.y>clip.height(){return None;}
    let left=(x+3.).clamp(clip.left(),clip.right()-size.x);
    for lane in 0..3{
        let y=if top{clip.top()+4.+lane as f32*(size.y+3.)}else{clip.bottom()-25.-size.y-lane as f32*(size.y+3.)};
        let area=Rect::from_min_size(Pos2::new(left,y),size);
        if clip.contains_rect(area) && !occupied.iter().any(|used|used.expand(2.).intersects(area)){
            occupied.push(area);return Some(area.min);
        }
    }None
}
fn paint_label(painter:&egui::Painter,clip:Rect,x:f32,text:&str,top:bool,color:Color32,occupied:&mut Vec<Rect>){
    let galley=painter.layout_no_wrap(text.to_owned(),egui::FontId::proportional(11.),color);
    if let Some(position)=reserve_label(clip,x,galley.size(),top,occupied){painter.galley(position,galley,color);}
}

pub fn unit_factor(unit:&str)->Option<f64>{match unit.trim().to_lowercase().as_str(){
    "angstrom"|"angstroms"|"å"|"aa"|"a"=>Some(1.),"nm"|"nanometer"|"nanometers"=>Some(10.),
    "um"|"µm"|"μm"|"micron"|"microns"=>Some(10000.),_=>None,
}}
pub fn sample_axis(meta:&SpectrumMeta)->bool{
    let label=meta.x_label.trim().to_lowercase();let unit=meta.x_unit.trim().to_lowercase();
    ["sample","pixel","index","frequency","velocity","energy"].iter().any(|name|label.contains(name))
        || unit.ends_with("hz") || ["pixel","pix","index","s","sec","km/s","m/s","ev","kev"].contains(&unit.as_str())
}
pub fn observed(rest:f64,z:f64,air:bool)->Option<f64>{
    if !rest.is_finite() || rest<=0. || !z.is_finite() || z<=-1. || z>20. {return None;}
    let vacuum=rest*(1.+z);
    if !vacuum.is_finite() || vacuum<=0. {return None;}
    if !air{return Some(vacuum);}
    // SDSS DR9+ Ciddor conversion; keep air overlays inside the optical/NIR range.
    // SDSS Ciddor 真空转空气公式；空气标线限定在光学/近红外范围。
    // https://sdss.org/dr18/tutorials/conversions/
    if !(2000. ..=20000.).contains(&vacuum){return None;}
    let s2=(10000./vacuum).powi(2);
    Some(vacuum/(1.+0.05792105/(238.0185-s2)+0.00167917/(57.362-s2)))
}

#[derive(Clone)]
pub struct Controls {
    pub source:String,pub redshift:f64,pub width:f32,pub unit:String,pub air:bool,pub visible:Vec<bool>,
    pub manual_unit:bool,
    pub tolerance:f64,pub candidates:Vec<Value>,pub candidates_source:String,
    pub show_candidates:bool,
    blocked:bool,method:String,sigma:f64,window:usize,message:String,
}
impl Controls {
    pub fn source(image:&FitsImage)->String{
        let meta=image.info.spectrum.as_ref().unwrap();
        format!("{}#{}:{}:{}:{:?}:{:?}:{}",image.info.path,image.info.hdu,meta.x_column,meta.y_column,meta.error_column,meta.error_hdu,meta.source_error_kind().as_str())
    }
    pub fn new(image:&FitsImage)->Self{
        let meta=image.info.spectrum.as_ref().unwrap();
        let unit=match unit_factor(&meta.x_unit){Some(1.)=>"Angstrom",Some(10.)=>"nm",Some(_)=>"um",None=>"Select unit"}.into();
        Self{source:Self::source(image),redshift:0.,width:1.,unit,air:false,visible:vec![false;LINES.len()],manual_unit:false,tolerance:5.,
            candidates:vec![],candidates_source:String::new(),show_candidates:true,blocked:sample_axis(meta),method:"derivative".into(),sigma:5.,window:101,message:String::new()}
    }
    pub fn configure(&mut self,params:&Value)->Result<(),String>{
        let mut next=self.clone();
        if let Some(value)=params.get("redshift"){next.redshift=value.as_f64().ok_or("Redshift must be finite and -1 < z <= 20")?;}
        if let Some(value)=params.get("line_width"){next.width=value.as_f64().ok_or("Invalid marker width")? as f32;}
        if let Some(value)=params.get("unit"){next.unit=value.as_str().ok_or("Choose Angstrom, nm or um")?.into();next.manual_unit=true;}
        if let Some(value)=params.get("air"){next.air=value.as_bool().ok_or("Air must be boolean")?;}
        if let Some(value)=params.get("tolerance"){next.tolerance=value.as_f64().ok_or("Invalid matching tolerance")?;}
        if let Some(value)=params.get("show_candidates"){next.show_candidates=value.as_bool().ok_or("Show candidates must be boolean")?;}
        if let Some(value)=params.get("visible"){
            let entries=value.as_array().ok_or("Visible lines must be an array of names")?;next.visible.fill(false);
            for entry in entries{
                let name=entry.as_str().ok_or("Unknown reference line")?;
                let index=LINES.iter().position(|line|line.name==name).ok_or("Unknown reference line")?;next.visible[index]=true;
            }
        }
        next.validate()?;
        if next.unit!=self.unit || next.air!=self.air{next.candidates.clear();next.candidates_source.clear();}
        *self=next;Ok(())
    }
    fn validate(&self)->Result<(),String>{
        if !self.redshift.is_finite() || self.redshift<=-1. || self.redshift>20.{return Err("Redshift must be finite and -1 < z <= 20".into());}
        if !self.width.is_finite() || !(0.5..=5.).contains(&self.width){return Err("Invalid marker width".into());}
        if !self.tolerance.is_finite() || !(0.01..=1000.).contains(&self.tolerance){return Err("Invalid matching tolerance".into());}
        if self.unit!="Select unit" && unit_factor(&self.unit).is_none(){return Err("Choose Angstrom, nm or um".into());}Ok(())
    }
    pub fn ready(&self)->bool{!self.blocked && unit_factor(&self.unit).is_some() && self.validate().is_ok()}
    pub fn matches(&self,center:f64)->Vec<(&'static str,f64)>{
        if !self.ready() || !center.is_finite(){return vec![];}
        let mut found=LINES.iter().filter_map(|line|{
            let delta=center-observed(line.vacuum,self.redshift,self.air)?;
            (delta.abs()<=self.tolerance).then_some((line.name,delta))
        }).collect::<Vec<_>>();found.sort_by(|a,b|a.1.abs().total_cmp(&b.1.abs()));found
    }
    pub fn summary(&self)->Value{
        json!({"redshift":self.redshift,"line_width":self.width,"unit":self.unit,"manual_unit":self.manual_unit,"air":self.air,"ready":self.ready(),
            "visible":LINES.iter().zip(&self.visible).filter(|(_,show)|**show).map(|(line,_)|line.name).collect::<Vec<_>>(),
            "tolerance":self.tolerance,"candidates_count":self.candidates.len(),"candidate_ticks":self.plotted_candidates().len(),"show_candidates":self.show_candidates,"candidates_source":self.candidates_source,
            "candidates":self.candidates.iter().take(32).map(|candidate|json!({"center_angstrom":candidate["center_angstrom"],"kind":candidate["kind"],
                "possible_lines":self.matches(candidate["center_angstrom"].as_f64().unwrap_or(f64::NAN)).iter().map(|(name,delta)|json!({"name":name,"delta_angstrom":delta})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
    }
    pub fn accept(&mut self,value:&Value){
        if value["source"].as_str()!=Some(self.source.as_str()) || value["unit"].as_str()!=Some(self.unit.as_str()) || value["air"].as_bool()!=Some(self.air){return;}
        self.candidates=value["candidates"].as_array().map(|v|v.iter().take(256).cloned().collect()).unwrap_or_default();
        self.candidates_source=self.source.clone();
    }
    pub fn request(&self,image:&FitsImage)->Result<Value,String>{
        self.validate()?;if !self.ready(){return Err("Choose a wavelength unit; sample indices cannot be matched".into());}
        let meta=image.info.spectrum.as_ref().ok_or("Open a spectrum first")?;
        Ok(json!({"path":image.info.path,"hdu":image.info.hdu,"x_column":meta.x_column,"y_column":meta.y_column,
            "error_column":meta.error_column,"error_hdu":meta.error_hdu,"error_kind":meta.source_error_kind().as_str(),"no_error":!meta.has_error(),"unit":self.unit,"air":self.air,
            "method":self.method,"sigma":self.sigma,"continuum_window":self.window,"source":self.source}))
    }
    pub fn ui(&mut self,ui:&mut egui::Ui,busy:bool)->bool{
        let previous_unit=self.unit.clone();let previous_air=self.air;
        if self.blocked{ui.label(t("Sample indices or non-wavelength axes cannot be matched."));return false;}
        ui.horizontal(|ui|{
            ui.label(t("Wavelength unit"));egui::ComboBox::from_id_salt("spectral-unit").selected_text(t(&self.unit)).show_ui(ui,|ui|{
                for name in ["Select unit","Angstrom","nm","um"]{ui.selectable_value(&mut self.unit,name.into(),t(name));}
            });
            crate::pro::information(ui,"spectral-line-info",|ui|{
                ui.label(t("SDSS reference wavelengths: vacuum Angstrom; blends are reference features."));
                ui.hyperlink_to(t("Reference table"),"https://classic.sdss.org/dr7/algorithms/linestable.php");
                ui.label(t("Air conversion applies only at observed 2000–20000 Angstrom."));
                ui.label(t("Candidates only: continuum subtraction and wavelength proximity do not identify species or solve redshift."));
                ui.label(t("Matching tolerance is independent of marker width; broad features need specialist fitting."));
            });
        });
        ui.checkbox(&mut self.air,t("Observed axis is air wavelength"));
        if previous_unit!=self.unit{self.manual_unit=true;}
        if previous_unit!=self.unit || previous_air!=self.air{self.candidates.clear();self.candidates_source.clear();}
        let previous=self.redshift;
        ui.add(egui::Slider::new(&mut self.redshift,-0.01..=5.).clamping(egui::SliderClamping::Never).show_value(false).text(t("Redshift z")));
        ui.horizontal(|ui|{ui.label(t("Fine z"));ui.add(egui::DragValue::new(&mut self.redshift).speed(0.00001).max_decimals(7));});
        if self.validate().is_err(){self.redshift=previous;self.message="Redshift must be finite and -1 < z <= 20".into();}
        ui.add(egui::Slider::new(&mut self.width,0.5..=5.).text(t("Marker width (px)")));
        ui.horizontal(|ui|{
            if ui.small_button(t("Show all")).clicked(){self.visible.fill(true);}
            if ui.small_button(t("Hide all")).clicked(){self.visible.fill(false);}
        });
        ui.horizontal_wrapped(|ui|{for (label,indices) in GROUPS{
            let all=indices.iter().all(|&i|self.visible[i]);
            if ui.selectable_label(all,t(label)).clicked(){for &index in *indices{self.visible[index]=!all;}}
        }});
        egui::ScrollArea::vertical().id_salt("reference-line-list").max_height(160.).show(ui,|ui|{
            for (index,line) in LINES.iter().enumerate(){ui.checkbox(&mut self.visible[index],format!("{}  {:.3}",line.name,line.vacuum));}
        });
        ui.separator();
        ui.label(t("Candidate features (Specutils)"));
        ui.checkbox(&mut self.show_candidates,t("Show candidate ticks (green)"));
        ui.horizontal(|ui|{
            egui::ComboBox::from_id_salt("spectral-method").selected_text(t(if self.method=="threshold"{"Threshold method"}else{"Derivative method"})).show_ui(ui,|ui|{
                for (method,label) in [("derivative","Derivative method"),("threshold","Threshold method")]{ui.selectable_value(&mut self.method,method.into(),t(label));}
            });
            ui.label(t("Noise factor"));ui.add(egui::DragValue::new(&mut self.sigma).range(1. ..=30.).speed(0.1));
        });
        ui.horizontal(|ui|{ui.label(t("Continuum window (samples)"));ui.add(egui::DragValue::new(&mut self.window).range(5..=1001).speed(2));});
        let run=ui.add_enabled(self.ready() && !busy,egui::Button::new(t("Find candidate emission / absorption"))).clicked();
        ui.horizontal(|ui|{ui.label(t("Match tolerance (Angstrom)"));ui.add(egui::DragValue::new(&mut self.tolerance).range(0.01..=1000.).speed(0.1));});
        if !self.message.is_empty(){ui.label(t(&self.message));}
        for candidate in self.candidates.iter().take(64){
            let Some(center)=candidate["center_angstrom"].as_f64() else{continue;};
            let matches=self.matches(center).into_iter().take(4).map(|(name,delta)|format!("{name} ({delta:+.2})")).collect::<Vec<_>>().join(", ");
            ui.label(format!("{center:.3} A · {} · {}",t(candidate["kind"].as_str().unwrap_or("feature")),if matches.is_empty(){t("No reference within tolerance")}else{&matches}));
        }
        run
    }
    pub fn plotted_candidates(&self)->Vec<&Value>{
        if !self.ready() || !self.show_candidates{return vec![];}
        let strength=|candidate:&Value|candidate["contrast_over_noise"].as_f64().filter(|value|value.is_finite()).unwrap_or(0.);
        let mut candidates=self.candidates.iter().collect::<Vec<_>>();
        candidates.sort_by(|a,b|strength(b).total_cmp(&strength(a)));candidates.truncate(256);candidates
    }
    pub fn hover_candidate(&self,pointer:Option<Pos2>,rect:Rect,bounds:[f64;4],axis:crate::spectral_analysis::AxisTransform)->Option<String>{
        let pointer=pointer?;if pointer.y<rect.bottom()-26. || pointer.y>rect.bottom()+3. || !self.ready(){return None;}
        let factor=unit_factor(&self.unit)?;
        let nearest=self.plotted_candidates().into_iter().filter_map(|candidate|{
            let center=candidate["center_angstrom"].as_f64()?;let x=axis.forward(center/factor)?;
            if x<bounds[0] || x>bounds[1]{return None;}
            let pixel=rect.left()+((x-bounds[0])/(bounds[1]-bounds[0])) as f32*rect.width();
            ((pixel-pointer.x).abs()<=6.).then_some((candidate,center,(pixel-pointer.x).abs()))
        }).min_by(|a,b|a.2.total_cmp(&b.2))?;
        let names=self.matches(nearest.1).into_iter().map(|(name,delta)|format!("{name} ({delta:+.2} A)")).collect::<Vec<_>>().join(", ");
        Some(format!("? {:.3} A · {}\n{}",nearest.1,t(nearest.0["kind"].as_str().unwrap_or("Candidate")),if names.is_empty(){t("No reference within tolerance")}else{&names}))
    }
    pub fn paint(&self,painter:&egui::Painter,rect:Rect,bounds:[f64;4],axis:crate::spectral_analysis::AxisTransform){
        if !self.ready(){return;}
        let factor=unit_factor(&self.unit).unwrap();let clipped=painter.with_clip_rect(rect);
        let mut occupied=Vec::new();
        for (line,show) in LINES.iter().zip(&self.visible){
            if !show{continue;}let Some(x)=observed(line.vacuum,self.redshift,self.air).and_then(|wave|axis.forward(wave/factor))else{continue;};
            if x<bounds[0] || x>bounds[1]{continue;}
            let px=rect.left()+((x-bounds[0])/(bounds[1]-bounds[0])) as f32*rect.width();
            clipped.line_segment([Pos2::new(px,rect.top()),Pos2::new(px,rect.bottom())],Stroke::new(self.width,Color32::from_rgba_unmultiplied(255,190,85,150)));
            paint_label(&clipped,rect,px,line.name,true,Color32::from_rgb(255,205,125),&mut occupied);
        }
        if self.show_candidates{
            for (index,candidate) in self.plotted_candidates().into_iter().enumerate(){
                let Some(center)=candidate["center_angstrom"].as_f64()else{continue;};let Some(x)=axis.forward(center/factor)else{continue;};
                if !x.is_finite() || x<bounds[0] || x>bounds[1]{continue;}
                let px=rect.left()+((x-bounds[0])/(bounds[1]-bounds[0])) as f32*rect.width();
                let bottom=rect.bottom()-3.;let color=Color32::from_rgb(100,235,155);
                clipped.line_segment([Pos2::new(px,bottom-18.),Pos2::new(px,bottom)],Stroke::new(1.5_f32,color));
                if index<64{
                    let matched=self.matches(center);let label=matched.first().map(|(name,_)|*name).unwrap_or_else(||t(candidate["kind"].as_str().unwrap_or("Candidate")));
                    paint_label(&clipped,rect,px,&format!("? {label}"),false,color,&mut occupied);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn redshift_and_medium_conversion(){
        assert!((observed(5008.240,0.1,false).unwrap()-5509.064).abs()<1e-9);
        assert!((observed(5008.240,0.,true).unwrap()-5006.843).abs()<0.01);
        for z in [f64::NAN,f64::INFINITY,-1.,-2.,21.]{assert!(observed(5008.,z,false).is_none());}
        assert!(observed(1215.24,0.,true).is_none());assert_eq!(unit_factor("nm"),Some(10.));assert_eq!(unit_factor("um"),Some(10000.));assert_eq!(unit_factor("pixel"),None);
    }
    #[test]fn groups_include_both_components_and_triplet(){
        for (_,indices) in GROUPS{for &index in *indices{assert!(index<LINES.len());}}
        assert_eq!(GROUPS[1].1.len(),2);assert_eq!(GROUPS.last().unwrap().1.len(),3);
    }
    fn controls()->Controls{Controls{source:"test".into(),redshift:0.,width:1.,unit:"Angstrom".into(),air:false,visible:vec![false;LINES.len()],manual_unit:false,tolerance:1.,
        candidates:vec![],candidates_source:String::new(),show_candidates:true,blocked:false,method:"derivative".into(),sigma:5.,window:101,message:String::new()}}
    #[test]fn candidate_matching_uses_redshift_and_not_marker_width(){
        let mut lines=controls();lines.configure(&json!({"redshift":0.1,"line_width":5.,"tolerance":0.2})).unwrap();
        let center=5008.240*1.1;assert_eq!(lines.matches(center)[0].0,"[O III] 5008");assert!(lines.matches(center+0.3).is_empty());
        lines.configure(&json!({"line_width":0.5})).unwrap();assert_eq!(lines.matches(center)[0].0,"[O III] 5008");
        lines.configure(&json!({"redshift":0.})).unwrap();assert!(lines.matches(center).is_empty());
        assert!(lines.configure(&json!({"redshift":-1.,"visible":["H alpha"]})).is_err());assert!(!lines.visible.iter().any(|show|*show));
    }
    #[test]fn unit_changes_clear_candidates_and_stale_results_are_ignored(){
        let mut lines=controls();let result=json!({"source":"test","unit":"Angstrom","air":false,"candidates":[{"center_angstrom":5008.24}]});
        lines.accept(&result);assert_eq!(lines.candidates.len(),1);lines.configure(&json!({"unit":"nm"})).unwrap();assert!(lines.candidates.is_empty());
        lines.accept(&result);assert!(lines.candidates.is_empty());lines.blocked=true;assert!(!lines.ready());assert!(lines.matches(5008.24).is_empty());
    }
    #[test]fn candidate_ticks_are_independent_and_keep_the_strongest(){
        let mut lines=controls();lines.candidates=(0..80).map(|i|json!({"center_angstrom":4000.+i as f64,"contrast_over_noise":i})).collect();
        assert_eq!(lines.plotted_candidates().len(),80);assert_eq!(lines.plotted_candidates()[0]["center_angstrom"],json!(4079.));
        assert!(!lines.visible.iter().any(|show|*show));lines.show_candidates=false;assert!(lines.plotted_candidates().is_empty());
    }
    #[test]fn dense_labels_reserve_nonoverlapping_lanes_then_omit_text(){
        let clip=Rect::from_min_size(Pos2::ZERO,egui::vec2(100.,120.));let mut occupied=Vec::new();
        for _ in 0..3{assert!(reserve_label(clip,30.,egui::vec2(60.,12.),false,&mut occupied).is_some());}
        assert!(reserve_label(clip,31.,egui::vec2(60.,12.),false,&mut occupied).is_none());
        assert_eq!(occupied.len(),3);for (index,area) in occupied.iter().enumerate(){assert!(clip.contains_rect(*area));for other in occupied.iter().skip(index+1){assert!(!area.intersects(*other));}}
    }
}
