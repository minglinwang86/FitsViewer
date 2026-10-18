//! Minimal-mode tools and versioned preferences. PRO is initialized separately.
//! 极简模式的轻量工具与版本化偏好；PRO 单独按需初始化。
use eframe::egui::{self,Color32,Pos2,Rect,Stroke,Vec2};
use fitsviewer_core::{display::*,DisplayTone,FitsImage,Statistics,Stretch};
use serde::{Deserialize,Serialize};

pub const ACTIONS:[&str;10]=["Auto display","Fit","1:1","Zoom +","Zoom -","ZScale","Min/Max","Reset B/C","Invert","Header"];

pub struct AutoDisplay{pub low:f64,pub high:f64,pub stretch:Stretch}
/// A viewing starting point from the existing bounded sample, not a scientific calibration.
/// 复用已缓存的有限像素抽样选择查看参数，不进行科学定标或修改数据。
pub fn auto_display(stats:&Statistics)->Option<AutoDisplay>{
    if stats.finite==0 || stats.sample.is_empty(){return None;}
    let scale=stats.sample.iter().map(|value|value.abs()).fold(0_f64,f64::max);
    let constant=|value:f64|{
        let pad=if value==0.{1.}else{(value.abs()*1e-6).max(f64::MIN_POSITIVE)};
        let low=if (value-pad).is_finite(){value-pad}else{value};
        let high=if (value+pad).is_finite(){value+pad}else{value};
        (low<high && (high-low).is_finite()).then_some(AutoDisplay{low,high,stretch:Stretch::Linear})
    };
    if scale==0. || stats.sample.first()==stats.sample.last(){return constant(stats.sample[0]);}
    // Normalize before differences so physical units, tiny fluxes and bright outliers stay safe.
    // 先归一化后求差，避免小流量单位和亮离群值破坏阈值计算。
    let sample=stats.sample.iter().map(|value|value/scale).collect::<Vec<_>>();
    let quantile=|values:&[f64],p:f64|{let position=(values.len()-1) as f64*p;let index=position.floor() as usize;let weight=position-index as f64;values[index]*(1.-weight)+values[(index+1).min(values.len()-1)]*weight};
    let median=quantile(&sample,0.5);let sigma=(quantile(&sample,0.84)-quantile(&sample,0.16))/2.;
    let positive=(quantile(&sample,0.995)-median).max(0.);let negative=(median-quantile(&sample,0.005)).max(0.);
    let (low,high,stretch)=if sigma<=f64::EPSILON*(sample.last().unwrap()-sample[0]).abs(){
        let tail=sample.iter().copied().filter(|value|*value!=median).collect::<Vec<_>>();
        if tail.is_empty(){return constant(median*scale);}
        let low=quantile(&tail,0.02).min(median);let high=quantile(&tail,0.98).max(median);
        (low,high,if low<median && high>median{Stretch::Linear}else{Stretch::Asinh})
    }else if sample[0]<0. && *sample.last().unwrap()>0. && median.abs()<=1.5*sigma && negative>=0.5*positive{
        let half=(4.*sigma).max(positive).max(negative);(median-half,median+half,Stretch::Linear)
    }else if positive>20.*sigma{
        (median-sigma,median+(20.*sigma).max(positive),Stretch::Asinh)
    }else if positive>6.*sigma{
        (median-sigma,median+(16.*sigma).max(positive),Stretch::Sqrt)
    }else{(median-1.5*sigma,median+(6.*sigma).max(positive),Stretch::Linear)};
    let low=low.max(sample[0])*scale;let high=high.min(*sample.last().unwrap())*scale;
    if low.is_finite() && high.is_finite() && low<high && (high-low).is_finite(){Some(AutoDisplay{low,high,stretch})}else{constant(median*scale)}
}
#[derive(Clone,Serialize,Deserialize)]
#[serde(default)]
pub struct Preferences {
    pub schema:u32,
    pub mode:String,
    pub default_mode:String,
    pub theme:String,
    pub language:String,
    pub render:RenderOptions,
    pub colorbar:bool,
    pub scale_bar:bool,
    pub scale_pixels:f64,
    pub compass:bool,
    pub colorbar_position:String,
    pub toolbar:Vec<String>,
    pub ribbon_tab:String,
    pub ribbon_collapsed:bool,
    pub image_pro_width:f32,
    pub spectrum_pro_width:f32,
    pub image_pro_collapsed:bool,
    pub spectrum_pro_collapsed:bool,
    pub image_pro_layout:serde_json::Value,
    pub spectrum_pro_layout:serde_json::Value,
    pub window_layouts:crate::window_layout::Layouts,
    pub shortcuts:Vec<(String,String)>,
}
impl Default for Preferences {
    fn default()->Self {Self{schema:1,mode:"Minimal".into(),default_mode:"Minimal".into(),theme:"Dark".into(),language:"en".into(),render:RenderOptions::default(),colorbar:false,scale_bar:true,scale_pixels:100.,compass:false,colorbar_position:"Right".into(),toolbar:vec!["Fit".into(),"ZScale".into(),"Reset B/C".into()],ribbon_tab:"View".into(),ribbon_collapsed:false,image_pro_width:330.,spectrum_pro_width:330.,image_pro_collapsed:false,spectrum_pro_collapsed:false,image_pro_layout:serde_json::Value::Null,spectrum_pro_layout:serde_json::Value::Null,window_layouts:Default::default(),shortcuts:vec![]}}
}
impl Preferences {
    pub fn validate(&self)->Result<(),String>{
        #[cfg(not(feature="pro"))] if self.mode!="Minimal" || self.default_mode!="Minimal" || !["View","Measure","Coordinates"].contains(&self.ribbon_tab.as_str()){return Err("PRO is not included in this Mini build".into());}
        if self.schema!=1 || !["en","zh-CN"].contains(&self.language.as_str()) || !["Minimal","PRO"].contains(&self.mode.as_str()) || !["Dark","Light","Night"].contains(&self.theme.as_str()) || !["Left","Right","Top","Bottom"].contains(&self.colorbar_position.as_str()) {return Err("Unsupported preferences schema, mode, theme or colorbar position".into());}
        self.render.validate()?;
        if !self.scale_pixels.is_finite() || !(0.001..=1e7).contains(&self.scale_pixels){return Err("Invalid scale bar length".into());}
        self.window_layouts.validate()?;
        if [&self.image_pro_layout,&self.spectrum_pro_layout].iter().any(|layout|!layout.is_null() && (!layout.is_object() || layout.to_string().len()>24*1024)){return Err("Invalid or oversized PRO workspace layout".into());}
        if [self.image_pro_width,self.spectrum_pro_width].iter().any(|width|!width.is_finite() || !(220. ..=1200.).contains(width)){return Err("PRO panel width must be finite and between 220 and 1200".into());}
        if !["Minimal","PRO"].contains(&self.default_mode.as_str()){return Err("Unknown default startup mode".into());}
        if !["View","Measure","Coordinates","Analysis","Catalogues","Export"].contains(&self.ribbon_tab.as_str()){return Err("Unknown ribbon tab".into());}
        if self.toolbar.len()>ACTIONS.len() || self.toolbar.iter().any(|a|!ACTIONS.contains(&a.as_str())) {return Err("Unknown or oversized quick toolbar".into());}
        let mut unique=self.toolbar.clone();unique.sort();unique.dedup();if unique.len()!=self.toolbar.len(){return Err("Duplicate toolbar action".into());}
        if self.shortcuts.len()>ACTIONS.len() || self.shortcuts.iter().any(|(key,action)|!valid_shortcut(key) || !ACTIONS.contains(&action.as_str())) {return Err("Shortcuts must be Ctrl+letter and target a built-in action".into());}
        let mut keys=self.shortcuts.iter().map(|(key,_)|key).collect::<Vec<_>>();keys.sort();keys.dedup();if keys.len()!=self.shortcuts.len(){return Err("Duplicate shortcut key".into());}
        Ok(())
    }
    pub fn parse(text:&str)->Result<Self,String>{
        if text.len()>64*1024{return Err("Preferences exceed 64 KiB".into());}
        let prefs:Self=serde_json::from_str(text).map_err(|e|e.to_string())?;prefs.validate()?;Ok(prefs)
    }
    // Only a newly constructed window adopts the startup preference.
    // 仅新建窗口采用启动偏好；修改启动选项不切换当前模式。
    fn for_startup(mut self)->Self {self.mode=if cfg!(feature="pro"){self.default_mode.clone()}else{"Minimal".into()};self}
    pub fn pro_width(&self,spectrum:bool)->f32{if spectrum{self.spectrum_pro_width}else{self.image_pro_width}}
    pub fn set_pro_width(&mut self,spectrum:bool,width:f32){
        if width.is_finite() && (220. ..=1200.).contains(&width){
            let current=if spectrum{&mut self.spectrum_pro_width}else{&mut self.image_pro_width};
            if (*current-width).abs()>0.5{*current=width;}
        }
    }
    pub fn pro_collapsed(&self,spectrum:bool)->bool{if spectrum{self.spectrum_pro_collapsed}else{self.image_pro_collapsed}}
    pub fn set_pro_collapsed(&mut self,spectrum:bool,collapsed:bool){if spectrum{self.spectrum_pro_collapsed=collapsed;}else{self.image_pro_collapsed=collapsed;}}
    pub fn pro_layout(&self,spectrum:bool)->&serde_json::Value{if spectrum{&self.spectrum_pro_layout}else{&self.image_pro_layout}}
    pub fn set_pro_layout(&mut self,spectrum:bool,layout:serde_json::Value){if spectrum{self.spectrum_pro_layout=layout;}else{self.image_pro_layout=layout;}}
}

#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn preferences_reject_conflicting_keys_and_keep_custom_names(){
        let old=Preferences::parse("{\"schema\":1}").unwrap();assert_eq!(old.language,"en");
        let mut prefs=old;prefs.language="zh-CN".into();prefs.render.palette.name="Open".into();
        let restored=Preferences::parse(&serde_json::to_string(&prefs).unwrap()).unwrap();assert_eq!(restored.render.palette.name,"Open");
        prefs.shortcuts=vec![("Ctrl+F".into(),"Fit".into()),("Ctrl+F".into(),"Header".into())];assert!(prefs.validate().is_err());
        assert!(Preferences::parse("{\"language\":\"invalid\"}").is_err());
    }
    #[cfg(feature="pro")]
    #[test]fn ribbon_preferences_restore_old_defaults_and_round_trip(){
        let mut prefs=Preferences::parse("{\"schema\":1}").unwrap();
        assert_eq!(prefs.ribbon_tab,"View");assert!(!prefs.ribbon_collapsed);
        prefs.ribbon_tab="Catalogues".into();prefs.ribbon_collapsed=true;
        let restored=Preferences::parse(&serde_json::to_string(&prefs).unwrap()).unwrap();
        assert_eq!(restored.ribbon_tab,"Catalogues");assert!(restored.ribbon_collapsed);
        assert!(Preferences::parse("{\"ribbon_tab\":\"invalid\"}").is_err());
    }
    #[cfg(feature="pro")]
    #[test]fn default_mode_is_separate_from_current_mode(){
        let old=Preferences::parse("{\"schema\":1,\"mode\":\"PRO\"}").unwrap();
        assert_eq!(old.default_mode,"Minimal");assert_eq!(old.for_startup().mode,"Minimal");
        let mut prefs=Preferences::default();prefs.default_mode="PRO".into();
        let restored=Preferences::parse(&serde_json::to_string(&prefs).unwrap()).unwrap();
        assert_eq!(restored.mode,"Minimal");assert_eq!(restored.default_mode,"PRO");
        assert_eq!(restored.for_startup().mode,"PRO");
        assert!(Preferences::parse("{\"default_mode\":\"invalid\"}").is_err());
    }
    #[test]fn pro_panels_restore_old_defaults_and_remember_contexts_independently(){
        let mut prefs=Preferences::parse("{\"schema\":1}").unwrap();
        assert_eq!(prefs.pro_width(false),330.);assert_eq!(prefs.pro_width(true),330.);assert!(!prefs.pro_collapsed(false));assert!(!prefs.pro_collapsed(true));
        prefs.set_pro_width(false,500.);prefs.set_pro_collapsed(false,true);prefs.set_pro_width(true,710.);
        let restored=Preferences::parse(&serde_json::to_string(&prefs).unwrap()).unwrap();
        assert_eq!(restored.pro_width(false),500.);assert_eq!(restored.pro_width(true),710.);assert!(restored.pro_collapsed(false));assert!(!restored.pro_collapsed(true));
        assert!(Preferences::parse("{\"image_pro_width\":100}").is_err());assert!(Preferences::parse("{\"spectrum_pro_width\":1201}").is_err());
        prefs.image_pro_width=f32::NAN;assert!(prefs.validate().is_err());
    }
}
fn valid_shortcut(key:&str)->bool {key.len()==6 && key.starts_with("Ctrl+") && key.as_bytes()[5].is_ascii_uppercase()}

#[cfg(not(target_arch="wasm32"))]
pub(crate) fn settings_path()->std::path::PathBuf{
    if let Some(path)=std::env::var_os("FITSVIEWER_UI_CONFIG"){return path.into();}
    let base=std::env::var_os("XDG_CONFIG_HOME").map(std::path::PathBuf::from).unwrap_or_else(||{
        if cfg!(windows){std::env::var_os("APPDATA").map(std::path::PathBuf::from).unwrap_or_default()}
        else {std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default().join(".config")}
    });base.join("fitsviewer/ui-v1.json")
}
pub fn load()->Preferences{
    #[cfg(not(target_arch="wasm32"))]
    let text=std::fs::read_to_string(settings_path()).ok();
    #[cfg(target_arch="wasm32")]
    let text=web_sys::window().and_then(|w|w.local_storage().ok().flatten()).and_then(|s|s.get_item("fitsviewer-ui-v1").ok().flatten());
    text.as_deref().and_then(|s|Preferences::parse(s).ok()).unwrap_or_default()
}
pub fn save(prefs:&Preferences)->Result<(),String>{
    prefs.validate()?;let text=serde_json::to_string_pretty(prefs).map_err(|e|e.to_string())?;
    #[cfg(not(target_arch="wasm32"))] {
        let path=settings_path();let parent=path.parent().ok_or("No settings parent")?;
        std::fs::create_dir_all(parent).map_err(|e|e.to_string())?;
        let temporary=path.with_extension("json.new");std::fs::write(&temporary,text).map_err(|e|e.to_string())?;
        std::fs::rename(temporary,path).map_err(|e|e.to_string())?;
    }
    #[cfg(target_arch="wasm32")] {
        let storage=web_sys::window().and_then(|w|w.local_storage().ok().flatten()).ok_or("Browser preference storage is unavailable")?;
        storage.set_item("fitsviewer-ui-v1",&text).map_err(|_|"Could not save browser preferences")?;
    }
    Ok(())
}

pub struct Tools {
    pub prefs:Preferences,
    pub settings_open:bool,
    pub histogram_open:bool,
    pub histogram:Option<Histogram>,
    pub histogram_builds:usize,
    pub logarithmic_counts:bool,
    pub json:String,
    pub message:String,
    applied_theme:String,
    shortcut_key:String,
    shortcut_action:String,
    io_path:String,
}
impl Default for Tools {
    fn default()->Self{Self{prefs:load().for_startup(),settings_open:false,histogram_open:false,histogram:None,histogram_builds:0,logarithmic_counts:false,json:String::new(),message:String::new(),applied_theme:String::new(),shortcut_key:"Ctrl+F".into(),shortcut_action:"Fit".into(),io_path:"fitsviewer-ui.json".into()}}
}
impl Tools {
    pub fn apply_theme(&mut self,ctx:&egui::Context){
        if self.applied_theme==self.prefs.theme{return;}
        crate::ui_style::apply_theme(ctx,&self.prefs.theme);self.applied_theme=self.prefs.theme.clone();
    }
    pub fn toolbar(&mut self,ui:&mut egui::Ui,allow_pro:bool){
        ui.horizontal(|ui|{
            if ui.selectable_label(self.prefs.mode=="Minimal",crate::i18n::t("Minimal")).clicked(){self.prefs.mode="Minimal".into();}
            #[cfg(feature="pro")]
            if ui.add_enabled(allow_pro,egui::Button::selectable(self.prefs.mode=="PRO","PRO")).on_disabled_hover_text(crate::i18n::t("Scientific PRO tools require a selected Python environment and service")).clicked(){self.prefs.mode="PRO".into();}
        });
    }
    pub fn shortcut(&self,ctx:&egui::Context)->Option<String>{
        if ctx.wants_keyboard_input(){return None;}
        ctx.input(|i|self.prefs.shortcuts.iter().find(|(key,_)|i.modifiers.ctrl && egui::Key::from_name(&key[5..]).is_some_and(|k|i.key_pressed(k))).map(|(_,a)|a.clone()))
    }
    pub fn settings(&mut self,ctx:&egui::Context,maximum:f64,windows:&mut crate::window_layout::Runtime){
        let mut open=self.settings_open;
        windows.show(ctx,crate::window_layout::Key::Settings,crate::i18n::t("Viewer tools"),&mut open,egui::vec2(430.,480.),|ui|self.settings_ui(ui,ctx,maximum));self.settings_open=open;
    }
    pub fn settings_ui(&mut self,ui:&mut egui::Ui,ctx:&egui::Context,maximum:f64){
        crate::ribbon::align_controls(ui);
        ui.checkbox(&mut self.prefs.compass,crate::i18n::t("N / E"));
        information(ui,"settings-help",|ui|{ui.label(crate::i18n::t("Applies to new windows only; does not switch the current mode."));ui.label(crate::i18n::t("Manual threshold, in scaled physical units; no detector saturation is inferred."));});
            egui::ComboBox::from_id_salt("interface-language").selected_text(if self.prefs.language=="zh-CN"{"界面语言：简体中文"}else{"Language: English"}).show_ui(ui,|ui|{ui.selectable_value(&mut self.prefs.language,"en".into(),"English");ui.selectable_value(&mut self.prefs.language,"zh-CN".into(),"简体中文");});
            egui::ComboBox::new("Interface theme",crate::i18n::t("Interface theme")).selected_text(crate::i18n::t(&self.prefs.theme)).show_ui(ui,|ui|{for name in ["Dark","Light","Night"]{ui.selectable_value(&mut self.prefs.theme,name.into(),crate::i18n::t(name));}});
            egui::ComboBox::new("default-startup-mode",crate::i18n::t("Default startup mode")).selected_text(crate::i18n::t(&self.prefs.default_mode)).show_ui(ui,|ui|{for &mode in if cfg!(feature="pro"){&["Minimal","PRO"][..]}else{&["Minimal"][..]}{ui.selectable_value(&mut self.prefs.default_mode,mode.into(),crate::i18n::t(mode));}});
            let mut palette=self.prefs.render.palette.name.clone();
            egui::ComboBox::new("Palette",crate::i18n::t("Palette")).selected_text(if Palette::NAMES.contains(&palette.as_str()){crate::i18n::t(&palette)}else{&palette}).show_ui(ui,|ui|{for name in Palette::NAMES{ui.selectable_value(&mut palette,name.into(),crate::i18n::t(name));}});
            if palette!=self.prefs.render.palette.name{self.prefs.render.palette=Palette::builtin(&palette);}
            ui.checkbox(&mut self.prefs.colorbar,crate::i18n::t("Colorbar"));
            egui::ComboBox::new("Colorbar position",crate::i18n::t("Colorbar position")).selected_text(crate::i18n::t(&self.prefs.colorbar_position)).show_ui(ui,|ui|{for name in ["Left","Right","Top","Bottom"]{ui.selectable_value(&mut self.prefs.colorbar_position,name.into(),crate::i18n::t(name));}});
            let mut masked=self.prefs.render.mask.is_some();
            if ui.checkbox(&mut masked,crate::i18n::t("Physical-value threshold overlay")).changed(){self.prefs.render.mask=masked.then_some(ThresholdMask{threshold:maximum,opacity:0.55,color:[255,70,30],style:MaskStyle::Fill});}
            if let Some(mask)=&mut self.prefs.render.mask {
                ui.horizontal(|ui|{ui.label(crate::i18n::t("Threshold"));ui.add(egui::DragValue::new(&mut mask.threshold).speed(maximum.abs().max(1.)*0.001));});
                ui.add(egui::Slider::new(&mut mask.opacity,0.0..=1.0).text(crate::i18n::t("Opacity")));
                ui.color_edit_button_srgb(&mut mask.color);
                egui::ComboBox::new("Mask style",crate::i18n::t("Mask style")).selected_text(crate::i18n::t(if mask.style==MaskStyle::Fill{"Fill"}else{"Outline"})).show_ui(ui,|ui|{ui.selectable_value(&mut mask.style,MaskStyle::Fill,crate::i18n::t("Fill"));ui.selectable_value(&mut mask.style,MaskStyle::Outline,crate::i18n::t("Outline"));});
                ui.weak(crate::i18n::t("Manual threshold, in scaled physical units; no detector saturation is inferred."));
            }
            egui::CollapsingHeader::new(crate::i18n::t("Quick toolbar")).id_salt("Quick toolbar").show(ui,|ui|{
                for name in ACTIONS {let mut enabled=self.prefs.toolbar.iter().any(|a|a==name);if ui.checkbox(&mut enabled,crate::i18n::t(name)).changed(){if enabled{self.prefs.toolbar.push(name.into());}else{self.prefs.toolbar.retain(|a|a!=name);}}}
                for index in 0..self.prefs.toolbar.len(){ui.horizontal(|ui|{ui.label(crate::i18n::t(&self.prefs.toolbar[index]));if index>0 && ui.small_button(crate::i18n::t("Up")).clicked(){self.prefs.toolbar.swap(index,index-1);}if index+1<self.prefs.toolbar.len() && ui.small_button(crate::i18n::t("Down")).clicked(){self.prefs.toolbar.swap(index,index+1);}});}
            });
            egui::CollapsingHeader::new(crate::i18n::t("Additional shortcuts")).id_salt("Additional shortcuts").show(ui,|ui|{
                ui.text_edit_singleline(&mut self.shortcut_key);
                egui::ComboBox::from_id_salt("shortcut-action").selected_text(crate::i18n::t(&self.shortcut_action)).show_ui(ui,|ui|{for name in ACTIONS{ui.selectable_value(&mut self.shortcut_action,name.into(),crate::i18n::t(name));}});
                if ui.button(crate::i18n::t("Assign Ctrl+letter")).clicked(){if valid_shortcut(&self.shortcut_key){self.prefs.shortcuts.retain(|(k,_)|k!=&self.shortcut_key);self.prefs.shortcuts.push((self.shortcut_key.clone(),self.shortcut_action.clone()));}else{self.message="Use Ctrl+ followed by one uppercase letter".into();}}
                for (key,action) in &self.prefs.shortcuts{ui.label(format!("{key}: {}",crate::i18n::t(action)));}
            });
            egui::CollapsingHeader::new(crate::i18n::t("Preferences and custom palette")).id_salt("Preferences and custom palette").show(ui,|ui|{
                ui.text_edit_singleline(&mut self.io_path);
                ui.horizontal(|ui|{
                    if ui.button(crate::i18n::t("Save JSON file")).clicked(){let text=serde_json::to_string_pretty(&self.prefs).unwrap_or_default();self.message=match crate::text_io::save(&self.io_path,&text){Ok(())=>"Preferences exported".into(),Err(error)=>error};}
                    if ui.button(crate::i18n::t("Load JSON file")).clicked(){
                        #[cfg(not(target_arch="wasm32"))]
                        {match crate::text_io::read(&self.io_path).and_then(|text|Preferences::parse(&text)){Ok(prefs)=>{self.prefs=prefs;self.message="Preferences imported".into();},Err(error)=>self.message=error,}}
                        #[cfg(target_arch="wasm32")]
                        {if let Err(error)=crate::text_io::browser("importPreferences",&[]){self.message=error;}}
                    }
                });
                ui.horizontal(|ui|{
                    if ui.button(crate::i18n::t("Export JSON / Copy")).clicked(){self.json=serde_json::to_string_pretty(&self.prefs).unwrap_or_default();ctx.copy_text(self.json.clone());self.message="Preferences copied. Save this JSON for backup; it contains no session tokens.".into();}
                    if ui.button(crate::i18n::t("Import JSON")).clicked(){match Preferences::parse(&self.json){Ok(p)=>{self.prefs=p;self.message="Validated preferences imported".into();},Err(e)=>self.message=e}}
                    if ui.button(crate::i18n::t("Restore defaults")).clicked(){self.prefs=Preferences::default();self.message="Defaults restored".into();}
                });
                ui.add(egui::TextEdit::multiline(&mut self.json).desired_rows(7).desired_width(f32::INFINITY));
                ui.weak(crate::i18n::t("Custom palette: set render.palette.name and 2–256 RGBA control points in JSON. Unknown fields are ignored; invalid data does not replace current preferences."));
            });
            if !self.message.is_empty(){ui.label(crate::i18n::t(&self.message));}
    }
    pub fn histogram(&mut self,ctx:&egui::Context,image:Option<&FitsImage>,low:f64,high:f64,windows:&mut crate::window_layout::Runtime){
        if !self.histogram_open{return;}let mut open=self.histogram_open;
        windows.show(ctx,crate::window_layout::Key::Histogram,crate::i18n::t("Physical-value histogram"),&mut open,egui::vec2(340.,180.),|ui|self.histogram_ui(ui,image,low,high));self.histogram_open=open;
    }
    pub fn histogram_ui(&mut self,ui:&mut egui::Ui,image:Option<&FitsImage>,low:f64,high:f64){
        if self.histogram.is_none(){if let Some(image)=image{self.histogram=Some(Histogram::new(image,128));self.histogram_builds+=1;}}
            if let Some(hist)=&self.histogram{
                // Keep counts beside their control; a dock must not stretch the plot to its full height.
                // 计数紧跟选项，直方图不随工作区高度无限扩张。
                ui.horizontal(|ui|{
                    ui.checkbox(&mut self.logarithmic_counts,egui::RichText::new(crate::i18n::t("Log counts")).size(11.));
                    let details=format!("{} {} · {} {} · {}",hist.finite,crate::i18n::t("finite values"),hist.counts.len(),crate::i18n::t("bins"),crate::i18n::t(if hist.sampled{"sampled"}else{"full image"}));
                    ui.add(egui::Label::new(egui::RichText::new(&details).size(11.)).truncate()).on_hover_text(details);
                });
                let plot_height=(ui.available_width()*0.33).clamp(84.,120.);
                let (response,painter)=ui.allocate_painter(Vec2::new(ui.available_width(),plot_height),egui::Sense::hover());let rect=response.rect;
                let height=|n:u64|if self.logarithmic_counts{(n as f64).ln_1p()}else{n as f64};let maximum=height(*hist.counts.iter().max().unwrap_or(&1)).max(1.);
                for (i,&n) in hist.counts.iter().enumerate(){let x0=rect.left()+rect.width()*i as f32/hist.counts.len() as f32;let x1=rect.left()+rect.width()*(i+1) as f32/hist.counts.len() as f32;painter.rect_filled(Rect::from_min_max(Pos2::new(x0,rect.bottom()-(height(n)/maximum) as f32*rect.height()),Pos2::new(x1,rect.bottom())),0.,Color32::from_rgb(75,155,185));}
                for bound in [low,high]{let x=rect.left()+((bound-hist.minimum)/(hist.maximum-hist.minimum)) as f32*rect.width();if x>=rect.left() && x<=rect.right(){painter.line_segment([Pos2::new(x,rect.top()),Pos2::new(x,rect.bottom())],Stroke::new(1.5_f32,Color32::YELLOW));}}
                let range=format!("{:.3e} … {:.3e} · {}",hist.minimum,hist.maximum,crate::i18n::t("yellow = Low / High"));
                ui.add(egui::Label::new(egui::RichText::new(&range).size(11.)).truncate()).on_hover_text(format!("{range}\nLow: {low:.7e}\nHigh: {high:.7e}"));
            }else{ui.label(crate::i18n::t("Open an image to compute its histogram."));}
    }
    pub fn image_area(&self,painter:&egui::Painter,mut area:Rect,low:f64,high:f64,stretch:Stretch,invert:bool,tone:DisplayTone,unit:&str,mapping:Option<&ScaleMapping>)->Rect{
        if !self.prefs.colorbar{return area;}
        let horizontal=["Top","Bottom"].contains(&self.prefs.colorbar_position.as_str());
        if area.width()<180. || area.height()<150. {return area;}
        let bar=match self.prefs.colorbar_position.as_str(){
            "Left"=>{let r=Rect::from_min_max(area.left_top()+Vec2::new(8.,28.),area.left_bottom()+Vec2::new(20.,-28.));area.min.x+=85.;r},
            "Top"=>{let r=Rect::from_min_max(area.left_top()+Vec2::new(45.,8.),area.right_top()+Vec2::new(-45.,20.));area.min.y+=53.;r},
            "Bottom"=>{let r=Rect::from_min_max(area.left_bottom()+Vec2::new(45.,-45.),area.right_bottom()+Vec2::new(-45.,-33.));area.max.y-=56.;r},
            _=>{let r=Rect::from_min_max(area.right_top()+Vec2::new(-78.,28.),area.right_bottom()+Vec2::new(-66.,-28.));area.max.x-=85.;r},
        };
        for i in 0..256 {let t=i as f64/255.;let value=low+(high-low)*t;let c=if let Some(mapping)=mapping{mapping.color(value,invert,tone,&self.prefs.render.palette)}else{display_color(value,low,high,stretch,invert,tone,&self.prefs.render.palette)};
            let segment=if horizontal{Rect::from_min_max(Pos2::new(bar.left()+bar.width()*i as f32/256.,bar.top()),Pos2::new(bar.left()+bar.width()*(i+1) as f32/256.,bar.bottom()))}else{Rect::from_min_max(Pos2::new(bar.left(),bar.bottom()-bar.height()*(i+1) as f32/256.),Pos2::new(bar.right(),bar.bottom()-bar.height()*i as f32/256.))};
            painter.rect_filled(segment,0.,Color32::from_rgba_unmultiplied(c[0],c[1],c[2],c[3]));
        }
        for t in [0.,0.5,1.] {let value=low+(high-low)*t;let (point,align)=if horizontal{(Pos2::new(bar.left()+bar.width()*t as f32,bar.bottom()+3.),egui::Align2::CENTER_TOP)}else{(Pos2::new(bar.right()+4.,bar.bottom()-bar.height()*t as f32),egui::Align2::LEFT_CENTER)};painter.text(point,align,format!("{value:.2e}"),egui::FontId::proportional(10.),Color32::LIGHT_GRAY);}
        if !unit.is_empty(){painter.text(Pos2::new(bar.center().x,bar.top()-3.),egui::Align2::CENTER_BOTTOM,unit,egui::FontId::proportional(10.),Color32::LIGHT_GRAY);}
        area
    }
}

#[cfg(test)]
mod auto_display_tests{
    use super::*;
    fn statistics(mut values:Vec<f64>)->Statistics{values.retain(|value|value.is_finite());values.sort_by(f64::total_cmp);Statistics{min:values.first().copied().unwrap_or(0.),max:values.last().copied().unwrap_or(1.),finite:values.len(),sample:values}}
    #[test]fn automatic_limits_reject_invalid_values_and_one_extreme_outlier(){
        let mut values=(0..999).map(|index|100.+(index%31) as f64*0.1).collect::<Vec<_>>();values.extend([1e12,f64::NAN,f64::INFINITY]);
        let stats=statistics(values);let preset=auto_display(&stats).unwrap();
        assert!(preset.low>90. && preset.high<200. && preset.low<preset.high);assert!(preset.high<stats.max*1e-6);
    }
    #[test]fn automatic_stretch_compresses_bright_tails_and_preserves_signed_signal(){
        let mut values=(0..970).map(|index|100.+(index%31) as f64*0.1).collect::<Vec<_>>();values.extend((1..=30).map(|index|100.+index as f64*1000.));
        assert_eq!(auto_display(&statistics(values)).unwrap().stretch,Stretch::Asinh);
        let signed=auto_display(&statistics((-500..=500).map(|value|value as f64).collect())).unwrap();
        assert_eq!(signed.stretch,Stretch::Linear);assert!(signed.low<0. && signed.high>0.);
    }
    #[test]fn automatic_display_handles_sparse_constant_and_empty_frames(){
        let mut values=vec![0.;990];values.extend((1..=10).map(|value|value as f64));
        let sparse=auto_display(&statistics(values)).unwrap();assert_eq!(sparse.low,0.);assert!(sparse.high>5.);assert_eq!(sparse.stretch,Stretch::Asinh);
        for value in [0.,3.,-3.,1e-17,1e308,-1e308]{let preset=auto_display(&statistics(vec![value;10])).unwrap();assert!(preset.low.is_finite() && preset.high.is_finite() && preset.low<preset.high);}
        assert!(auto_display(&statistics(vec![f64::NAN,f64::INFINITY])).is_none());
    }
    #[test]fn automatic_display_respects_small_and_large_flux_units(){
        let values=(0..1000).map(|index|100.+(index%101) as f64*0.1).collect::<Vec<_>>();let base=auto_display(&statistics(values.clone())).unwrap();
        for factor in [1e-17,1e17]{let scaled=auto_display(&statistics(values.iter().map(|value|value*factor).collect())).unwrap();assert_eq!(scaled.stretch,base.stretch);assert!((scaled.low/(base.low*factor)-1.).abs()<1e-10);assert!((scaled.high/(base.high*factor)-1.).abs()<1e-10);}
    }
    fn image(spectrum:bool)->FitsImage{
        let values=(0..100).map(|index|if index==10{f64::NAN}else{100.+(index%31) as f64*0.1}).collect();
        let meta=spectrum.then(||fitsviewer_core::SpectrumMeta{x_label:"Wavelength".into(),x_unit:"Angstrom".into(),y_label:"Flux".into(),y_unit:String::new(),columns:vec![],x_column:0,y_column:1,error_column:None,error_hdu:None,uncertainty_kind:String::new(),note:String::new()});
        FitsImage{info:fitsviewer_core::ImageInfo{path:"display-parameters.fits".into(),hdu:0,width:if spectrum{100}else{10},height:if spectrum{1}else{10},bitpix:-64,bscale:2.,bzero:10.,blank:None,hdus:vec![],header:String::new(),spectrum:meta},pixels:fitsviewer_core::RawPixels::F64(values),spectrum:spectrum.then(||fitsviewer_core::SpectrumData{x:(0..100).map(|index|4000.+index as f64).collect(),error:None})}
    }
    #[test]fn automatic_action_preserves_pixels_camera_and_minimal_lifecycle(){
        let mut app=crate::FitsApp::from_image(image(false));app.tools.prefs=Preferences::default();
        let before=app.image.as_ref().unwrap().clone();let packet=before.to_packet().unwrap();app.low=1000.;app.high=1001.;app.tone=DisplayTone{bias:0.1,contrast:10.};app.zoom=2.;app.pan=Vec2::new(12.,-9.);
        app.action("Auto display",&egui::Context::default());
        assert!(app.low<app.high && app.high<1000.);assert_eq!(app.tone.bias,0.5);assert_eq!(app.tone.contrast,1.);assert_eq!(app.zoom,2.);assert_eq!(app.pan,Vec2::new(12.,-9.));
        #[cfg(feature="pro")] assert!(app.pro.is_none() && app.multi.is_none());assert!(std::sync::Arc::ptr_eq(&before,app.image.as_ref().unwrap()));assert_eq!(packet,app.image.as_ref().unwrap().to_packet().unwrap());
    }
    #[test]fn automatic_image_action_does_not_change_a_spectrum(){
        let mut app=crate::FitsApp::from_image(image(true));app.low=1000.;app.high=1001.;app.tone=DisplayTone{bias:0.1,contrast:10.};
        app.action("Auto display",&egui::Context::default());assert_eq!((app.low,app.high,app.tone.bias,app.tone.contrast),(1000.,1001.,0.1,10.));
    }
}

/// Shared help placement also works in a compiled Mini viewer.
pub(crate) fn information(ui:&mut egui::Ui,id:impl std::hash::Hash,body:impl FnOnce(&mut egui::Ui)){
    #[cfg(feature="pro")] crate::workspace_ui::information(ui,id,body);
    #[cfg(not(feature="pro"))] ui.push_id(id,|ui|{ui.menu_button("…",|ui|{crate::ui_style::menu_style(ui.style_mut());ui.set_max_width(360.);body(ui);}).response.on_hover_text(crate::i18n::t("Info"));});
}

/// Keep a useful failure visible without filling a tool with a transport traceback.
/// 错误摘要保留在主面板；完整诊断与复制操作放在同一行的省略号菜单。
pub(crate) fn status_message(ui:&mut egui::Ui,message:&str){
    if message.trim().is_empty(){return;}
    let summary=status_summary(message);
    if summary==message{ui.add(egui::Label::new(crate::i18n::t(message)).wrap());return;}
    ui.horizontal_wrapped(|ui|{
        ui.add(egui::Label::new(crate::i18n::t(&summary)).wrap());
        information(ui,"status-message-details",|ui|{
            ui.strong(crate::i18n::t("Error details"));
            if ui.button(crate::i18n::t("Copy details")).clicked(){ui.ctx().copy_text(message.into());}
            egui::ScrollArea::vertical().max_height(200.).show(ui,|ui|{ui.add(egui::Label::new(message).wrap());});
        });
    });
}

fn status_summary(message:&str)->String{
    let lower=message.to_ascii_lowercase();
    let summary=if lower.contains("nameresolutionerror") || lower.contains("getaddrinfo failed") || lower.contains("failed to resolve"){
        Some("Cannot resolve the remote service address. Check the network or proxy.")
    }else if lower.contains("timed out") || lower.contains("readtimeout") || lower.contains("connecttimeout") || (lower.contains("timeout") && (lower.contains("connectionpool") || lower.contains("network request"))){
        Some("The remote request timed out. Try again or select another service.")
    }else if lower.contains("503") && (lower.contains("server error") || lower.contains("service unavailable")){
        Some("The remote service is temporarily unavailable. Try another service.")
    }else if lower.contains("sslerror") || lower.contains("certificate_verify_failed"){
        Some("The secure connection failed. Open the details to check the certificate error.")
    }else if lower.contains("httpsconnectionpool") || lower.contains("network request failed"){
        Some("The remote service request failed. Open the details for the cause.")
    }else if lower.contains("input values did not match any of the formats"){
        Some("The observation date could not be read. Open the details.")
    }else{None};
    if let Some(summary)=summary{return summary.into();}
    if message.chars().count()<=200 && !message.contains('\n'){return message.into();}
    let first=message.lines().next().unwrap_or(message);
    let mut summary=first.chars().take(160).collect::<String>();
    if first.chars().count()>160{summary.push('…');}
    summary
}

#[cfg(test)]
mod status_message_tests{
    use super::status_summary;
    #[test]fn transport_diagnostics_keep_their_cause_visible(){
        assert!(status_summary("Gaia services failed: NameResolutionError: Failed to resolve 'example'").starts_with("Cannot resolve"));
        assert!(status_summary("HTTPSConnectionPool timed out (ReadTimeout)").contains("timed out"));
        assert!(status_summary("503 Server Error: Service Unavailable for url: example").contains("temporarily unavailable"));
        assert!(status_summary("SSLError: CERTIFICATE_VERIFY_FAILED").contains("secure connection"));
        assert!(status_summary("HTTP 500: Network request failed").contains("request failed"));
    }
    #[test]fn short_scientific_rejections_are_not_hidden_or_relabelled(){
        for message in ["Completed","Gaia calibration rejected: fit RMS 0.685, holdout RMS 2.877 arcsec","No usable uncertainty array","Observation date is invalid","Timeout must be a finite positive number"]{assert_eq!(status_summary(message),message);}
        let long=format!("{}\nprivate diagnostic", "测".repeat(250));let short=status_summary(&long);
        assert_eq!(short.chars().count(),161);assert!(!short.contains("private"));
    }
}

