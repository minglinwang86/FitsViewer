//! PRO spectral analysis configuration, source-coordinate selections and display references.
//! PRO 光谱分析配置、源坐标选区和显示参考；不修改原始采样。
use eframe::egui::{self,Color32,Pos2,Rect};
use fitsviewer_core::FitsImage;
use serde::{Deserialize,Serialize};
use serde_json::{json,Value};
use std::collections::BTreeMap;
use crate::{i18n::t,spectral_lines::{Controls,unit_factor,observed}};

const C_KMS:f64=299792.458;
#[derive(Clone,Copy,Debug,PartialEq)]
pub struct AxisTransform{pub mode:u8,pub factor:f64,pub redshift:f64,pub air:bool,pub reference:f64}
impl AxisTransform{
    pub fn observed()->Self{Self{mode:0,factor:1.,redshift:0.,air:false,reference:6356.}}
    pub fn vacuum(&self,source:f64)->Option<f64>{
        let wave=source*self.factor;if !wave.is_finite() || wave<=0.{return None;}
        if !self.air{return Some(wave);}
        let mut vacuum=wave*1.00028;
        for _ in 0..8{vacuum*=wave/observed(vacuum,0.,true)?;}Some(vacuum)
    }
    pub fn forward(&self,source:f64)->Option<f64>{
        if !source.is_finite(){return None;}if self.mode==0{return Some(source);}
        let rest=self.vacuum(source)?/(1.+self.redshift);
        let value=if self.mode==1{rest/self.factor}else{let ratio=(rest/self.reference).powi(2);C_KMS*(ratio-1.)/(ratio+1.)};
        value.is_finite().then_some(value)
    }
    pub fn inverse(&self,display:f64)->Option<f64>{
        if !display.is_finite(){return None;}if self.mode==0{return Some(display);}
        let rest=if self.mode==1{display*self.factor}else{
            let beta=display/C_KMS;if beta.abs()>=1.{return None;}self.reference*((1.+beta)/(1.-beta)).sqrt()
        };
        let vacuum=rest*(1.+self.redshift);let wave=if self.air{observed(vacuum,0.,true)?}else{vacuum};
        (wave.is_finite() && wave>0.).then_some(wave/self.factor)
    }
}

#[derive(Clone,Serialize,Deserialize)]
#[serde(default)]
pub struct Component{pub amplitude:f64,pub center:f64,pub sigma:f64,pub fwhm_g:f64,pub fwhm_l:f64,pub bounds:Bounds}
#[derive(Clone,Serialize,Deserialize)]
#[serde(default)]
pub struct Bounds{pub amplitude:[f64;2],pub center:[f64;2],pub sigma:[f64;2],pub fwhm_g:[f64;2],pub fwhm_l:[f64;2]}
impl Default for Bounds{fn default()->Self{Self{amplitude:[-10.,10.],center:[0.,1.],sigma:[0.001,1.],fwhm_g:[0.001,1.],fwhm_l:[0.001,1.]}}}
impl Default for Component{fn default()->Self{Self{amplitude:1.,center:0.5,sigma:0.1,fwhm_g:0.2,fwhm_l:0.2,bounds:Bounds::default()}}}
#[derive(Clone,Serialize,Deserialize)]
#[serde(default)]
pub struct Parameters{
    pub region:[f64;2],pub continuum_windows:Vec<[f64;2]>,pub masks:Vec<[f64;2]>,
    pub continuum_method:String,pub continuum_degree:usize,pub continuum_output:String,pub mc_samples:usize,pub seed:u32,
    pub smooth_method:String,pub smooth_window:usize,pub smooth_sigma:f64,pub smooth_polyorder:usize,
    pub fit_profile:String,pub fit_include_continuum:bool,pub components:Vec<Component>,
}
impl Default for Parameters{fn default()->Self{Self{region:[0.,1.],continuum_windows:vec![],masks:vec![],continuum_method:"linear".into(),continuum_degree:1,continuum_output:"all".into(),mc_samples:128,seed:20261017,
    smooth_method:"boxcar".into(),smooth_window:7,smooth_sigma:2.,smooth_polyorder:2,fit_profile:"gaussian".into(),fit_include_continuum:true,components:vec![Component::default()]}}}
#[derive(Clone,Serialize,Deserialize)]
#[serde(default)]
pub struct CustomLine{pub name:String,pub rest:f64,pub medium:String,pub show:bool}
impl Default for CustomLine{fn default()->Self{Self{name:"Custom".into(),rest:6355.,medium:"vacuum".into(),show:true}}}
#[derive(Clone,Serialize,Deserialize)]
#[serde(default)]
pub struct Display{
    pub axis:String,pub reference:f64,pub preview:String,pub show_original:bool,pub show_continuum:bool,pub show_model:bool,pub show_residual:bool,
    pub blackbody:bool,pub temperature:f64,pub blackbody_scale:f64,pub blackbody_pivot:f64,pub custom_lines:Vec<CustomLine>,
}
impl Default for Display{fn default()->Self{Self{axis:"Observed".into(),reference:6356.,preview:"Original".into(),show_original:true,show_continuum:true,show_model:true,show_residual:true,
    blackbody:false,temperature:10000.,blackbody_scale:1.,blackbody_pivot:5500.,custom_lines:vec![]}}}
pub struct Series{pub name:String,pub kind:String,pub x:Vec<f64>,pub y:Vec<f64>}
pub struct Analysis{
    pub source:String,pub revision:u64,pub display_generation:u64,pub task:String,pub parameters:Parameters,pub display:Display,pub series:Vec<Series>,pub results:BTreeMap<String,Value>,
    pub drag_target:String,pub keep_regions:bool,pub message:String,pub unit:String,pub air:bool,pub redshift:f64,
    region_json:String,flux_scale:f64,
}
fn valid_interval(value:&[f64;2])->bool{value.iter().all(|n|n.is_finite()) && value[0]<value[1]}
pub fn format_number(value:f64)->String{if value!=0. && (value.abs()<1e-4 || value.abs()>=1e7){format!("{value:.6e}")}else{format!("{value:.6}")}}
fn scientific_drag(ui:&mut egui::Ui,value:&mut f64,speed:f64)->egui::Response{
    ui.add(egui::DragValue::new(value).speed(speed.max(f64::MIN_POSITIVE)).custom_formatter(|value,_|format_number(value))
        .custom_parser(|text|text.trim().parse::<f64>().ok().filter(|value|value.is_finite())))
}
fn interval_ui(ui:&mut egui::Ui,label:&str,range:&mut[f64;2],speed:f64){ui.horizontal(|ui|{ui.label(t(label));for value in range{scientific_drag(ui,value,speed);}});}
impl Analysis{
    pub fn new(image:&FitsImage,previous:Option<&Self>)->Self{
        let data=image.spectrum.as_ref().unwrap();let mut low=f64::INFINITY;let mut high=f64::NEG_INFINITY;let mut scale=0_f64;
        for (index,&x) in data.x.iter().enumerate(){if let Some(y)=image.physical(index).filter(|_|x.is_finite()){low=low.min(x);high=high.max(x);scale=scale.max(y.abs());}}
        if !low.is_finite() || high<=low{low=0.;high=1.;}let span=high-low;
        let mut parameters=Parameters{region:[low+0.4*span,low+0.6*span],continuum_windows:vec![[low+0.2*span,low+0.35*span],[low+0.65*span,low+0.8*span]],..Default::default()};
        let keep=previous.is_some_and(|old|old.keep_regions);
        if let Some(old)=previous.filter(|old|old.keep_regions){parameters.region=old.parameters.region;parameters.continuum_windows=old.parameters.continuum_windows.clone();parameters.masks=old.parameters.masks.clone();}
        let scale=if scale.is_finite() && scale>0.{scale}else{1.};
        let mut result=Self{source:Controls::source(image),revision:1,display_generation:1,task:"Measure".into(),parameters,display:Display{blackbody_scale:scale*0.5,..Default::default()},series:vec![],results:BTreeMap::new(),drag_target:"Pan".into(),keep_regions:keep,message:String::new(),unit:"Select unit".into(),air:false,redshift:0.,region_json:String::new(),flux_scale:scale};
        result.reset_components(1);result
    }
    pub fn invalidate(&mut self){self.revision=self.revision.saturating_add(1);self.display_generation=self.display_generation.saturating_add(1);self.series.clear();self.results.clear();self.message="Settings changed; run the analysis again".into();}
    pub fn sync_context(&mut self,lines:&Controls){if self.unit!=lines.unit || self.air!=lines.air || self.redshift!=lines.redshift{self.unit=lines.unit.clone();self.air=lines.air;self.redshift=lines.redshift;self.invalidate();}}
    pub fn axis(&self,lines:&Controls)->AxisTransform{
        if !lines.ready() || !self.display.reference.is_finite() || self.display.reference<=0.{return AxisTransform::observed();}
        AxisTransform{mode:match self.display.axis.as_str(){"Rest"=>1,"Velocity"=>2,_=>0},factor:unit_factor(&lines.unit).unwrap(),redshift:lines.redshift,air:lines.air,reference:self.display.reference}
    }
    pub fn reset_components(&mut self,count:usize){
        let region=self.parameters.region;let span=if valid_interval(&region){region[1]-region[0]}else{1.};let count=count.clamp(1,4);
        self.parameters.components=(0..count).map(|index|Component{amplitude:self.flux_scale*0.2,center:region[0]+span*(index+1) as f64/(count+1) as f64,sigma:span/20.,fwhm_g:span/10.,fwhm_l:span/20.,
            bounds:Bounds{amplitude:[-10.*self.flux_scale,10.*self.flux_scale],center:region,sigma:[span*1e-5,span],fwhm_g:[span*1e-5,span],fwhm_l:[span*1e-5,span]}}).collect();
    }
    pub fn validate(&self,action:&str)->Result<(),String>{
        let p=&self.parameters;
        if !valid_interval(&p.region) || p.continuum_windows.iter().chain(&p.masks).any(|range|!valid_interval(range)) || p.masks.len()>32 || p.continuum_windows.len()>8{return Err("Intervals must be finite and ordered; at most 8 continuum windows and 32 masks".into());}
        if !["linear","poly"].contains(&p.continuum_method.as_str()) || p.continuum_degree>5 || p.mc_samples>512{return Err("Invalid continuum degree or uncertainty sample count".into());}
        if action=="smooth" && (!["boxcar","gaussian","savgol"].contains(&p.smooth_method.as_str()) || p.smooth_window<3 || p.smooth_window>1001 || p.smooth_window%2!=1 || !p.smooth_sigma.is_finite() || !(0.1..=333.).contains(&p.smooth_sigma) || p.smooth_polyorder>5 || p.smooth_polyorder>=p.smooth_window){return Err("Smoothing requires an odd 3–1001 sample window and valid parameters".into());}
        if !["Observed","Rest","Velocity"].contains(&self.display.axis.as_str()) || !self.display.reference.is_finite() || self.display.reference<=0.{return Err("Invalid wavelength axis or reference".into());}
        if !self.display.temperature.is_finite() || !(1000. ..=50000.).contains(&self.display.temperature) || !self.display.blackbody_scale.is_finite() || self.display.blackbody_scale<0. || !self.display.blackbody_pivot.is_finite() || self.display.blackbody_pivot<=0.{return Err("Invalid relative blackbody reference".into());}
        if self.display.custom_lines.len()>32 || self.display.custom_lines.iter().any(|line|!line.rest.is_finite() || line.rest<=0. || !["vacuum","air"].contains(&line.medium.as_str())){return Err("Custom rest wavelengths must be positive with explicit air/vacuum convention".into());}
        if action=="fit"{
            if !["gaussian","voigt"].contains(&p.fit_profile.as_str()) || !(1..=4).contains(&p.components.len()){return Err("Choose 1–4 Gaussian or Voigt components".into());}
            for component in &p.components{
                let mut fields=vec![(component.amplitude,component.bounds.amplitude,false),(component.center,component.bounds.center,false)];
                if p.fit_profile=="gaussian"{fields.push((component.sigma,component.bounds.sigma,true));}else{fields.push((component.fwhm_g,component.bounds.fwhm_g,true));fields.push((component.fwhm_l,component.bounds.fwhm_l,true));}
                if fields.iter().any(|(value,bound,positive)|!valid_interval(bound) || !value.is_finite() || *value<=bound[0] || *value>=bound[1] || (*positive && bound[0]<=0.)) || component.center<p.region[0] || component.center>p.region[1]{return Err("Each fit initial value must lie strictly within finite bounds; centers must lie in the region".into());}
            }
        }Ok(())
    }
    pub fn configure(&mut self,params:&Value)->Result<(),String>{
        if params["action"].as_str().is_some_and(|action|!["continuum","measure","smooth","fit"].contains(&action)){return Err("Unknown spectral analysis action".into());}
        let mut next_display=self.display.clone();
        let value=params.get("parameters").unwrap_or(params);let mut merged=serde_json::to_value(&self.parameters).unwrap();for (key,value) in value.as_object().ok_or("Parameters must be an object")?{merged[key]=value.clone();}let next_parameters=serde_json::from_value(merged).map_err(|e|e.to_string())?;
        if let Some(value)=params.get("display"){let mut merged=serde_json::to_value(&next_display).unwrap();for (key,value) in value.as_object().ok_or("Display must be an object")?{merged[key]=value.clone();}next_display=serde_json::from_value(merged).map_err(|e|e.to_string())?;}
        if params["task"].as_str().is_some_and(|task|!["Display","Measure","Process","Fit","Compare"].contains(&task)){return Err("Unknown spectrum analysis task".into());}
        let old_parameters=std::mem::replace(&mut self.parameters,next_parameters);let old_display=std::mem::replace(&mut self.display,next_display);
        if let Err(error)=self.validate(params["action"].as_str().unwrap_or("configure")){self.parameters=old_parameters;self.display=old_display;return Err(error);}
        if serde_json::to_value(&old_parameters).unwrap()!=serde_json::to_value(&self.parameters).unwrap(){self.invalidate();}
        if let Some(task)=params["task"].as_str(){if !["Display","Measure","Process","Fit","Compare"].contains(&task){return Err("Unknown spectrum analysis task".into());}self.task=task.into();}
        if let Some(keep)=params["keep_regions"].as_bool(){self.keep_regions=keep;}
        Ok(())
    }
    pub fn request(&mut self,action:&str,image:&FitsImage,lines:&Controls)->Result<Value,String>{
        if !["continuum","measure","smooth","fit"].contains(&action){return Err("Unknown spectral analysis action".into());}
        self.sync_context(lines);self.validate(action)?;if !lines.ready(){return Err("Choose a wavelength unit; sample indices cannot be matched".into());}
        let mut params=serde_json::to_value(&self.parameters).unwrap();let original=lines.request(image)?;
        if action=="fit"{
            // Keep both profiles' editable guesses locally; send only the selected model's fields.
            // 界面保留两类模型的初值；请求仅发送当前模型使用的参数和边界。
            let keys=if self.parameters.fit_profile=="gaussian"{&["amplitude","center","sigma"][..]}else{&["amplitude","center","fwhm_g","fwhm_l"][..]};
            for component in params["components"].as_array_mut().unwrap(){
                component.as_object_mut().unwrap().retain(|key,_|key=="bounds" || keys.contains(&key.as_str()));
                component["bounds"].as_object_mut().unwrap().retain(|key,_|keys.contains(&key.as_str()));
            }
        }
        for key in ["path","hdu","x_column","y_column","error_column","error_hdu","error_kind","no_error","unit","air","source"]{params[key]=original[key].clone();}
        params["action"]=json!(action);params["redshift"]=json!(lines.redshift);params["revision"]=json!(self.revision);Ok(params)
    }
    pub fn accept(&mut self,value:&Value)->bool{
        if value["operation"]!="spectral_analysis" || value["source"].as_str()!=Some(&self.source) || value["revision"].as_u64()!=Some(self.revision){return false;}
        if value["action"]=="fit"{self.series.retain(|series|!["model","component","residual"].contains(&series.kind.as_str()));}
        if let Some(series)=value["series"].as_array(){for entry in series.iter().take(12){
            let (Some(x),Some(y))=(entry["x"].as_array(),entry["y"].as_array())else{continue;};if x.len()!=y.len() || x.len()>8192{continue;}
            let name=entry["name"].as_str().unwrap_or("preview").to_owned();let kind=entry["kind"].as_str().unwrap_or("preview").to_owned();
            self.series.retain(|old|old.name!=name || old.kind!=kind);
            self.series.push(Series{name,kind,x:x.iter().map(|v|v.as_f64().unwrap_or(f64::NAN)).collect(),y:y.iter().map(|v|v.as_f64().unwrap_or(f64::NAN)).collect()});
        }}
        let action=value["action"].as_str().unwrap_or("analysis").to_owned();self.results.insert(action,value["measurements"].clone());
        self.message=value["warnings"].as_array().map(|v|v.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n")).unwrap_or_default();
        self.display_generation=self.display_generation.saturating_add(1);true
    }
    pub fn summary(&self)->Value{json!({"source":self.source,"revision":self.revision,"display_generation":self.display_generation,"task":self.task,"parameters":self.parameters,"display":self.display,"keep_regions":self.keep_regions,"selection_target":self.drag_target,
        "series":self.series.iter().map(|series|json!({"name":series.name,"kind":series.kind,"points":series.x.len()})).collect::<Vec<_>>(),"results":self.results,"message":self.message})}
    pub fn selection(&mut self,start:f64,end:f64,target:&str){
        let range=[start.min(end),start.max(end)];if !valid_interval(&range){return;}
        match target{"Left continuum"=>{if self.parameters.continuum_windows.len()<2{self.parameters.continuum_windows.resize(2,range);}self.parameters.continuum_windows[0]=range;},
            "Right continuum"=>{if self.parameters.continuum_windows.len()<2{self.parameters.continuum_windows.resize(2,range);}self.parameters.continuum_windows[1]=range;},
            "Mask"=>{if self.parameters.masks.len()<32{self.parameters.masks.push(range);}},_=>self.parameters.region=range}
        self.invalidate();
    }
    pub fn blackbody(&self,source:f64,axis:AxisTransform)->Option<f64>{
        if !self.display.blackbody || self.display.blackbody_scale<0.{return None;}let rest=axis.vacuum(source)?/(1.+axis.redshift);
        planck_relative(rest,self.display.temperature,self.display.blackbody_pivot).map(|shape|shape*self.display.blackbody_scale/if self.display.preview=="Normalized"{self.flux_scale}else{1.}).filter(|value|value.is_finite())
    }
    pub fn ui(&mut self,ui:&mut egui::Ui,image:&FitsImage,lines:&mut Controls,busy:bool,multi:&mut crate::multiview::Workspace)->Option<String>{
        self.sync_context(lines);let before=serde_json::to_value(&self.parameters).unwrap();let mut action=None;
        ui.horizontal_wrapped(|ui|{for task in ["Display","Measure","Process","Fit","Compare"]{ui.selectable_value(&mut self.task,task.into(),t(if task=="Fit"{"Model fit"}else{task}));}
            crate::pro::information(ui,"analysis-info",|ui|{
                let hints:&[&str]=match self.task.as_str(){
                    "Display"=>&["Rest/velocity transform x only; flux is unchanged. Velocity uses the relativistic wavelength ratio.","SN preset wavelengths are approximate blended feature references, not velocity measurements.","Relative B_lambda shape normalized at the pivot; no absolute temperature fit, extinction or flux calibration is inferred."],
                    "Compare"=>&["Layers: eye, order and opacity affect display only.","Display scale and offset only; flux units and resolution are not converted. No coadd."],
                    "Process"=>&["Shift-drag selects a region. All windows and masks use original source coordinates.","Smoothing is a display preview; measurements use original samples."],
                    "Fit"=>&["Shift-drag selects a region. All windows and masks use original source coordinates.","Baseline is fixed from continuum windows; parameters are not jointly fitted.","Initial value / lower bound / upper bound; widths and centers use source x units."],
                    _=>&["Shift-drag selects a region. All windows and masks use original source coordinates."],
                };for text in hints{ui.label(t(text));}
            });
        });
        match self.task.as_str(){
            "Display"=>self.display_ui(ui,lines),
            "Compare"=>multi.spectrum_comparison_ui(ui),
            _=>{
                self.regions_ui(ui,image);
                self.continuum_ui(ui);
                match self.task.as_str(){
                    "Measure"=>{ui.horizontal(|ui|{ui.label(t("Uncertainty draws"));ui.add(egui::DragValue::new(&mut self.parameters.mc_samples).range(0..=512));});
                        if ui.add_enabled(!busy,egui::Button::new(t("Measure EW / flux / profile"))).clicked(){action=Some("measure".into());}self.results_ui(ui,"measure");},
                    "Process"=>{
                        ui.horizontal(|ui|{ui.label(t("Smoothing"));egui::ComboBox::from_id_salt("smoothing-method").selected_text(t(&self.parameters.smooth_method)).show_ui(ui,|ui|{for name in ["boxcar","gaussian","savgol"]{ui.selectable_value(&mut self.parameters.smooth_method,name.into(),t(name));}});});
                        ui.horizontal(|ui|{ui.label(t("Window (odd samples)"));ui.add(egui::DragValue::new(&mut self.parameters.smooth_window).range(3..=1001).speed(2));});
                        if self.parameters.smooth_method=="gaussian"{ui.add(egui::DragValue::new(&mut self.parameters.smooth_sigma).range(0.1..=333.).speed(0.1).prefix("sigma: "));}
                        if self.parameters.smooth_method=="savgol"{ui.add(egui::DragValue::new(&mut self.parameters.smooth_polyorder).range(0..=5).prefix(t("Polynomial degree: ")));}
                        if ui.add_enabled(!busy,egui::Button::new(t("Preview smoothing"))).clicked(){action=Some("smooth".into());}
                    },
                    "Fit"=>{self.fit_ui(ui);if ui.add_enabled(!busy,egui::Button::new(t("Fit bounded components"))).clicked(){action=Some("fit".into());}self.results_ui(ui,"fit");},_=>{}
                }
                if ui.add_enabled(!busy,egui::Button::new(t("Preview continuum"))).clicked(){action=Some("continuum".into());}
                self.preview_ui(ui);
            }
        }
        if before!=serde_json::to_value(&self.parameters).unwrap(){self.invalidate();}
        self.sync_context(lines);if !self.message.is_empty(){ui.label(t(&self.message));}
        action
    }
    fn regions_ui(&mut self,ui:&mut egui::Ui,image:&FitsImage){
        let speed=((self.parameters.region[1]-self.parameters.region[0]).abs()/300.).max(f64::MIN_POSITIVE);
        ui.horizontal(|ui|{ui.label(t("Drag selection"));egui::ComboBox::from_id_salt("spectral-drag-target").selected_text(t(&self.drag_target)).show_ui(ui,|ui|{for name in ["Pan","Region","Left continuum","Right continuum","Mask"]{ui.selectable_value(&mut self.drag_target,name.into(),t(name));}});});
        interval_ui(ui,"Region",&mut self.parameters.region,speed);
        if self.parameters.continuum_windows.is_empty() && ui.small_button(t("Add continuum windows")).clicked(){let r=self.parameters.region;let d=(r[1]-r[0])*0.2;self.parameters.continuum_windows=vec![[r[0]-d,r[0]],[r[1],r[1]+d]];}
        for (index,range) in self.parameters.continuum_windows.iter_mut().enumerate(){interval_ui(ui,if index==0{"Left continuum"}else if index==1{"Right continuum"}else{"Continuum window"},range,speed);}
        egui::CollapsingHeader::new(t("Masks / copy regions")).id_salt("spectral-mask-editor").show(ui,|ui|{
            let mut remove=None;for (index,range) in self.parameters.masks.iter_mut().enumerate(){ui.horizontal(|ui|{interval_ui(ui,"Mask",range,speed);if ui.small_button("x").clicked(){remove=Some(index);}});}if let Some(index)=remove{self.parameters.masks.remove(index);}
            if self.parameters.masks.len()<32 && ui.small_button(t("Add mask")).clicked(){self.parameters.masks.push(self.parameters.region);}
            ui.checkbox(&mut self.keep_regions,t("Keep source-coordinate regions on file change"));
            if ui.button(t("Copy regions JSON")).clicked(){ui.ctx().copy_text(json!({"region":self.parameters.region,"continuum_windows":self.parameters.continuum_windows,"masks":self.parameters.masks}).to_string());}
            ui.add(egui::TextEdit::multiline(&mut self.region_json).desired_rows(2).hint_text(t("Region JSON")));
            if ui.button(t("Apply regions JSON")).clicked(){match serde_json::from_str::<Value>(&self.region_json){Ok(value)=>{if let Err(error)=self.configure(&json!({"parameters":value})){self.message=error;}},Err(error)=>self.message=error.to_string()}}
        });
        if let Some(data)=&image.spectrum{let mut indices=data.x.iter().enumerate().filter(|(_,x)|**x>=self.parameters.region[0] && **x<=self.parameters.region[1]).map(|(i,_)|i);let first=indices.next();let mut count=usize::from(first.is_some());let mut last=first;for index in indices{last=Some(index);count+=1;}
            let range=first.zip(last).map(|(first,last)|format!("{first}–{last}")).unwrap_or_else(||"—".into());
            ui.weak(format!("{}: {count} · {} {range}",t("Source samples"),t("zero-based indices")));}
    }
    fn continuum_ui(&mut self,ui:&mut egui::Ui){
        ui.horizontal(|ui|{ui.label(t("Continuum"));egui::ComboBox::from_id_salt("continuum-method").selected_text(t(&self.parameters.continuum_method)).show_ui(ui,|ui|{for name in ["linear","poly"]{ui.selectable_value(&mut self.parameters.continuum_method,name.into(),t(name));}});
            if self.parameters.continuum_method=="poly"{ui.add(egui::DragValue::new(&mut self.parameters.continuum_degree).range(0..=5));}else{self.parameters.continuum_degree=1;}});
    }
    fn preview_ui(&mut self,ui:&mut egui::Ui){
        ui.horizontal(|ui|{ui.label(t("Plot preview"));egui::ComboBox::from_id_salt("spectral-preview").selected_text(t(&self.display.preview)).show_ui(ui,|ui|{for name in ["Original","Normalized","Subtracted","Smoothed"]{ui.selectable_value(&mut self.display.preview,name.into(),t(name));}});});
        ui.checkbox(&mut self.display.show_original,t("Overlay original flux"));ui.checkbox(&mut self.display.show_continuum,t("Continuum preview"));ui.checkbox(&mut self.display.show_model,t("Model / components"));ui.checkbox(&mut self.display.show_residual,t("Residual panel"));
    }
    fn fit_ui(&mut self,ui:&mut egui::Ui){
        let coordinate_speed=((self.parameters.region[1]-self.parameters.region[0]).abs()/1000.).max(f64::MIN_POSITIVE);let flux_scale=self.flux_scale;
        ui.horizontal(|ui|{egui::ComboBox::from_id_salt("line-fit-profile").selected_text(t(&self.parameters.fit_profile)).show_ui(ui,|ui|{for name in ["gaussian","voigt"]{ui.selectable_value(&mut self.parameters.fit_profile,name.into(),t(name));}});
            let mut count=self.parameters.components.len();if ui.add(egui::DragValue::new(&mut count).range(1..=4).prefix(t("Components: "))).changed(){self.reset_components(count);}});
        ui.checkbox(&mut self.parameters.fit_include_continuum,t("Include fixed fitted continuum"));
        if ui.small_button(t("Reset component guesses / bounds")).clicked(){self.reset_components(self.parameters.components.len());}
        for (index,component) in self.parameters.components.iter_mut().enumerate(){egui::CollapsingHeader::new(format!("{} {}",t("Component"),index+1)).id_salt(("fit-component",index)).default_open(index==0).show(ui,|ui|{
            let mut fields=vec![("Amplitude",&mut component.amplitude,&mut component.bounds.amplitude),("Center",&mut component.center,&mut component.bounds.center)];
            if self.parameters.fit_profile=="gaussian"{fields.push(("Sigma",&mut component.sigma,&mut component.bounds.sigma));}else{fields.push(("Gaussian FWHM",&mut component.fwhm_g,&mut component.bounds.fwhm_g));fields.push(("Lorentz FWHM",&mut component.fwhm_l,&mut component.bounds.fwhm_l));}
            for (name,value,bounds) in fields{ui.label(t(name));ui.horizontal(|ui|{let speed=if name=="Amplitude"{value.abs().max(flux_scale)*0.005}else{coordinate_speed};scientific_drag(ui,value,speed);for bound in bounds{let speed=if name=="Amplitude"{bound.abs().max(flux_scale)*0.005}else{coordinate_speed};scientific_drag(ui,bound,speed);}});}
        });}
    }
    fn results_ui(&self,ui:&mut egui::Ui,action:&str){
        if let Some(result)=self.results.get(action){
            if action=="fit" && result["fit"].is_object(){let fit=&result["fit"];
                ui.strong(format!("{} · {}",t("Fit status"),fit["success"].as_bool().map(|ok|t(if ok{"Converged"}else{"Failed"})).unwrap_or(t("Unknown"))));
                let chi=fit["reduced_chi2"].as_f64().map(|v|format!("{v:.5}")).unwrap_or_else(||t("not computed").into());
                ui.label(format!("χ² / dof: {chi} · dof {} · N {}",fit["dof"],fit["samples"]));
                ui.label(t(if fit["weighted"]==true{"Weighted by source sigma; inspect residuals and model assumptions."}else{"Unweighted fit; formal parameter errors are unavailable."}));
                if let Some(components)=result["components"].as_array(){for (index,component) in components.iter().enumerate(){let center=component["center"].as_f64().map(format_number).unwrap_or_else(||"—".into());let error=component["uncertainty"]["center"].as_f64().map(|v|format!(" ± {}",format_number(v))).unwrap_or_default();ui.label(format!("{} {} · {} {center}{error} {}",t("Component"),index+1,t("Center"),self.unit));}}
                if fit["boundary_parameters"].as_array().is_some_and(|v|!v.is_empty()) || fit["covariance_status"]=="rank_deficient"{ui.label(t("Bounds or covariance warning: inspect fit details."));}
            }
            if let Some(metrics)=result["metrics"].as_array(){for metric in metrics{let value=metric["value"].as_f64().map(|value|format!("{value:.6e}")).unwrap_or_else(||"—".into());
                let uncertainty=metric["uncertainty"].as_f64().map(|error|format!(" ± {error:.3e}")).unwrap_or_default();
                ui.label(format!("{}: {value}{uncertainty} {}",metric["name"].as_str().unwrap_or("Result"),metric["unit"].as_str().unwrap_or("")));
                ui.label(format!("{} · {}",metric["status"].as_str().unwrap_or("unknown"),metric["uncertainty_status"].as_str().unwrap_or("unspecified")));}}
            egui::CollapsingHeader::new(t("Measurement / fit details")).id_salt(("analysis-details",action)).show(ui,|ui|{ui.add(egui::Label::new(serde_json::to_string_pretty(result).unwrap_or_default()).wrap());});
        }
    }
    fn display_ui(&mut self,ui:&mut egui::Ui,lines:&mut Controls){
        let old_context=(lines.unit.clone(),lines.air);
        ui.horizontal(|ui|{ui.label(t("Wavelength unit"));egui::ComboBox::from_id_salt("analysis-unit").selected_text(t(&lines.unit)).show_ui(ui,|ui|{for unit in ["Select unit","Angstrom","nm","um"]{if ui.selectable_value(&mut lines.unit,unit.into(),t(unit)).changed(){lines.manual_unit=true;lines.candidates.clear();}}});});
        ui.horizontal(|ui|{ui.label(t("Redshift z"));ui.add(egui::DragValue::new(&mut lines.redshift).range(-0.999..=20.).speed(0.00001).max_decimals(7));});
        ui.checkbox(&mut lines.air,t("Observed axis is air wavelength"));
        ui.horizontal(|ui|{ui.label(t("Display axis"));egui::ComboBox::from_id_salt("spectral-axis").selected_text(t(&self.display.axis)).show_ui(ui,|ui|{for name in ["Observed","Rest","Velocity"]{ui.selectable_value(&mut self.display.axis,name.into(),t(name));}});});
        ui.add(egui::DragValue::new(&mut self.display.reference).range(1. ..=1e7).speed(0.1).prefix(t("Vacuum reference (A): ")));
        egui::CollapsingHeader::new(t("SN / custom reference lines")).id_salt("sn-reference-lines").show(ui,|ui|{
            for (name,rest,medium) in [("Si II 6355 blend",6355.,"air"),("C II 6580 blend",6580.,"air"),("Si II 5972 blend",5972.,"air"),("O I 7774 blend",7774.,"air"),("Ca II NIR blend",8579.,"air")]{
                if ui.small_button(format!("{name} ({medium})")).clicked() && self.display.custom_lines.len()<32{self.display.custom_lines.push(CustomLine{name:name.into(),rest,medium:medium.into(),show:true});}
            }
            if ui.small_button(t("Add custom line")).clicked() && self.display.custom_lines.len()<32{self.display.custom_lines.push(CustomLine::default());}
            let mut remove=None;for (index,line) in self.display.custom_lines.iter_mut().enumerate(){ui.horizontal(|ui|{ui.checkbox(&mut line.show,"");ui.add(egui::TextEdit::singleline(&mut line.name).desired_width(95.));if ui.small_button("x").clicked(){remove=Some(index);}});
                ui.horizontal(|ui|{ui.add(egui::DragValue::new(&mut line.rest).range(1. ..=1e7).speed(0.1));egui::ComboBox::from_id_salt(("custom-medium",index)).selected_text(t(&line.medium)).show_ui(ui,|ui|{for medium in ["air","vacuum"]{ui.selectable_value(&mut line.medium,medium.into(),t(medium));}});});}if let Some(index)=remove{self.display.custom_lines.remove(index);}
        });
        ui.separator();ui.checkbox(&mut self.display.blackbody,t("Relative blackbody reference"));
        if self.display.blackbody{
            ui.add(egui::Slider::new(&mut self.display.temperature,1000. ..=50000.).logarithmic(true).text(t("Temperature (K)")));
            ui.horizontal(|ui|{ui.label(t("Fine temperature"));ui.add(egui::DragValue::new(&mut self.display.temperature).range(1000. ..=50000.).speed(10.));});
            let mut relative=self.display.blackbody_scale/self.flux_scale;let upper=relative.max(5.);
            if ui.add(egui::Slider::new(&mut relative,0. ..=upper).text(t("Scale / source amplitude"))).changed(){self.display.blackbody_scale=relative*self.flux_scale;}
            ui.horizontal(|ui|{ui.label(t("Relative scale"));if scientific_drag(ui,&mut self.display.blackbody_scale,self.flux_scale*0.005).changed(){self.display.blackbody_scale=self.display.blackbody_scale.max(0.);}});
            ui.add(egui::DragValue::new(&mut self.display.blackbody_pivot).range(100. ..=1e7).speed(10.).prefix(t("Rest vacuum pivot (A): ")));
        }
        if old_context!=(lines.unit.clone(),lines.air){lines.candidates.clear();lines.candidates_source.clear();}
        self.preview_ui(ui);
    }
    pub fn paint_regions(&self,painter:&egui::Painter,rect:Rect,bounds:[f64;4],axis:AxisTransform){
        let map=|value|axis.forward(value).map(|value|rect.left()+((value-bounds[0])/(bounds[1]-bounds[0])) as f32*rect.width());let clipped=painter.with_clip_rect(rect);
        for (range,color) in std::iter::once((&self.parameters.region,Color32::from_rgba_unmultiplied(100,150,255,22)))
            .chain(self.parameters.continuum_windows.iter().map(|range|(range,Color32::from_rgba_unmultiplied(80,220,130,26))))
            .chain(self.parameters.masks.iter().map(|range|(range,Color32::from_rgba_unmultiplied(255,70,70,45)))){
            if let (Some(left),Some(right))=(map(range[0]),map(range[1])){clipped.rect_filled(Rect::from_min_max(Pos2::new(left.min(right),rect.top()),Pos2::new(left.max(right),rect.bottom())),0.,color);}
        }
    }
}
pub fn planck_relative(rest_angstrom:f64,temperature:f64,pivot:f64)->Option<f64>{
    if !rest_angstrom.is_finite() || rest_angstrom<=0. || !temperature.is_finite() || !(1000. ..=50000.).contains(&temperature) || !pivot.is_finite() || pivot<=0.{return None;}
    let c2=6.62607015e-34*299792458./1.380649e-23*1e10;
    let log_expm1=|value:f64|if value>700.{value}else{value.exp_m1().ln()};
    let log_ratio=5.*(pivot/rest_angstrom).ln()+log_expm1(c2/(pivot*temperature))-log_expm1(c2/(rest_angstrom*temperature));
    (log_ratio.is_finite() && log_ratio<700.).then(||log_ratio.exp())
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn axis_round_trips_and_keeps_flux_out_of_transform(){for air in [false,true]{for mode in [0,1,2]{let axis=AxisTransform{mode,factor:10.,redshift:0.1,air,reference:5008.24};let source=550.9;assert!((axis.inverse(axis.forward(source).unwrap()).unwrap()-source).abs()<1e-6);}}}
    #[test]fn relative_blackbody_is_normalized_and_finite(){for temperature in [1000.,10000.,50000.]{assert!((planck_relative(5500.,temperature,5500.).unwrap()-1.).abs()<1e-12);assert!(planck_relative(4000.,temperature,5500.).unwrap().is_finite());}assert!(planck_relative(-1.,10000.,5500.).is_none());assert!(planck_relative(5000.,f64::NAN,5500.).is_none());}
    #[test]fn invalid_selection_intervals_are_rejected(){assert!(!valid_interval(&[1.,1.]));assert!(!valid_interval(&[f64::NAN,2.]));assert!(valid_interval(&[-2.,3.]));}
    #[test]fn scientific_flux_formatter_preserves_small_physical_values(){let value=1.234567e-17;let text=format_number(value);assert!(text.contains('e'));assert!((text.parse::<f64>().unwrap()/value-1.).abs()<1e-6);assert_eq!(format_number(0.),"0.000000");}
    fn analysis()->Analysis{
        Analysis{source:"test-spectrum".into(),revision:1,display_generation:1,task:"Measure".into(),parameters:Parameters::default(),display:Display::default(),series:vec![],results:BTreeMap::new(),drag_target:"Pan".into(),keep_regions:false,message:String::new(),unit:"Angstrom".into(),air:false,redshift:0.,region_json:String::new(),flux_scale:1e-17}
    }
    #[test]fn blackbody_matches_independent_astropy_blambda_reference(){
        // Astropy 7.2 BlackBody -> B_lambda via spectral_density, normalized at 5500 A.
        let waves=[3500.,4000.,5500.,7000.,9000.];
        for (temperature,expected) in [(10000.,[2.0252963496779812,1.756226135728623,1.,0.5575847427429924,0.27386732975112615]),(5000.,[0.4795872699513565,0.687766443451847,1.,0.9291246549809281,0.676127153385333])]{
            for (wave,expected) in waves.into_iter().zip(expected){assert!((planck_relative(wave,temperature,5500.).unwrap()/expected-1.).abs()<1e-10);}
        }
    }
    #[test]fn stale_replies_cannot_replace_results_or_trigger_refit(){
        let mut analysis=analysis();let value=json!({"operation":"spectral_analysis","action":"continuum","source":"test-spectrum","revision":1,"measurements":{"status":"ok"},"series":[{"name":"normalized","kind":"normalized","x":[1.,2.],"y":[1.,null]}]});
        assert!(analysis.accept(&value));assert_eq!(analysis.display_generation,2);assert_eq!(analysis.series.len(),1);assert!(analysis.series[0].y[1].is_nan());
        analysis.invalidate();let generation=analysis.display_generation;assert!(!analysis.accept(&value));assert_eq!(analysis.display_generation,generation);assert!(analysis.series.is_empty());assert!(analysis.results.is_empty());
        let wrong=json!({"operation":"spectral_analysis","source":"another-spectrum","revision":analysis.revision});assert!(!analysis.accept(&wrong));
    }
    #[test]fn invalid_configuration_is_atomic_and_display_changes_keep_science(){
        let mut analysis=analysis();let before=analysis.summary();
        assert!(analysis.configure(&json!({"parameters":{"region":[2.,1.]},"display":{"temperature":5000.}})).is_err());assert_eq!(analysis.summary(),before);
        assert!(analysis.configure(&json!({"action":"unknown","parameters":{"region":[2.,3.]}})).is_err());assert_eq!(analysis.summary(),before);
        analysis.results.insert("measure".into(),json!({"status":"ok"}));let revision=analysis.revision;
        analysis.configure(&json!({"display":{"blackbody":true,"temperature":5000.,"blackbody_scale":2e-17}})).unwrap();assert_eq!(analysis.revision,revision);assert!(!analysis.results.is_empty());
        assert!(analysis.configure(&json!({"display":{"blackbody_scale":-1.}})).is_err());assert_eq!(analysis.display.blackbody_scale,2e-17);
        analysis.configure(&json!({"parameters":{"region":[2.,3.]}})).unwrap();assert!(analysis.revision>revision);assert!(analysis.results.is_empty());
    }
    #[test]fn fit_requests_send_only_selected_profile_fields_and_keep_editable_guesses(){
        let meta=fitsviewer_core::SpectrumMeta{x_label:"Wavelength".into(),x_unit:"Angstrom".into(),y_label:"Flux".into(),y_unit:String::new(),columns:vec![],x_column:0,y_column:1,error_column:None,error_hdu:None,uncertainty_kind:String::new(),note:String::new()};
        let image=FitsImage{info:fitsviewer_core::ImageInfo{path:"real-protocol-test.fits".into(),hdu:1,width:3,height:1,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:Some(meta)},pixels:fitsviewer_core::RawPixels::F64(vec![1.,2.,1.]),spectrum:Some(fitsviewer_core::SpectrumData{x:vec![4000.,5000.,6000.],error:None})};
        let lines=Controls::new(&image);let mut analysis=Analysis::new(&image,None);
        for (profile,expected) in [("gaussian",vec!["amplitude","center","sigma"]),("voigt",vec!["amplitude","center","fwhm_g","fwhm_l"]) ]{
            analysis.parameters.fit_profile=profile.into();let before=serde_json::to_value(&analysis.parameters).unwrap();
            let request=analysis.request("fit",&image,&lines).unwrap();let component=&request["components"][0];
            assert_eq!(component["bounds"].as_object().unwrap().len(),expected.len());
            assert_eq!(component.as_object().unwrap().len(),expected.len()+1);
            for key in expected{assert!(component.get(key).is_some());assert!(component["bounds"].get(key).is_some());}
            assert_eq!(serde_json::to_value(&analysis.parameters).unwrap(),before);
        }
    }

}
