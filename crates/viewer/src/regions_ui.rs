use eframe::egui::{self,Color32,Pos2,Rect,Stroke};
use fitsviewer_core::{ErrorKind,FitsImage,regions::*};
use crate::image_transform::ImageTransform;

#[derive(Clone,Copy,PartialEq,Debug)]
pub enum Tool {Pan,Circle,Move,Ellipse,Rectangle,Polygon,Line,Polyline,Curve}
#[derive(Clone,Copy,Debug,PartialEq)]
enum DragKind {Move,Radius,SkyInner,SkyOuter}
#[derive(Clone)]
struct ApertureDrag {index:usize,origin:[f64;2],original:Aperture,kind:DragKind,changed:bool}
const MIN_RADIUS:f64=0.1;
const MAX_RADIUS:f64=9_999.8;
const MAX_SKY_RADIUS:f64=10_000.;
#[derive(Clone)]
pub struct Regions {
    pub open:bool,pub tool:Tool,pub apertures:Vec<Aperture>,pub results:Vec<Option<Measurement>>,pub selected:Option<usize>,
    pub options:MeasureOptions,pub message:String,pub template:String,pub draft:Vec<[f64;2]>,pub radius:f64,pub inner:f64,pub outer:f64,pub width:f64,pub gap:f64,pub background_width:f64,
    pub paste_mode:String,pub need_sky_paste:bool,pub needs_measure:bool,pub source:String,
    pub wcs_override:Option<String>,pub need_growth:Option<Vec<f64>>,
    pub preview_template:Option<RegionTemplate>,pub auto_confirm:bool,pub profile_request:Option<serde_json::Value>,geometry_editor:String,template_io_path:String,
    history:Vec<Vec<Aperture>>,redo:Vec<Vec<Aperture>>,use_noise:bool,gain:f64,read_noise:f64,
    pub need_batch_measure:bool,pub error_path:String,pub growth:Vec<(f64,Measurement)>,pub growth_open:bool,copy_background:bool,copy_exclusions:bool,copy_options:bool,
    pub error_hdu:Option<usize>,pub error_kind:ErrorKind,pub error_same_file:bool,pub revision:u64,pub measurement_pending:bool,
    pub uncertainty_source:serde_json::Value,
    drag:Option<ApertureDrag>,
}
impl Default for Regions {
    fn default()->Self{Self{open:false,tool:Tool::Pan,apertures:vec![],results:vec![],selected:None,options:MeasureOptions::default(),message:String::new(),template:crate::text_io::saved_regions().unwrap_or_default(),draft:vec![],radius:8.,inner:12.,outer:18.,width:5.,gap:2.,background_width:4.,paste_mode:"Keep pixels".into(),need_sky_paste:false,needs_measure:false,source:String::new(),wcs_override:None,need_growth:None,preview_template:None,auto_confirm:false,profile_request:None,geometry_editor:String::new(),template_io_path:"fitsviewer-apertures.json".into(),history:vec![],redo:vec![],use_noise:false,gain:1.,read_noise:0.,need_batch_measure:false,error_path:String::new(),growth:vec![],growth_open:false,copy_background:true,copy_exclusions:true,copy_options:true,error_hdu:None,error_kind:ErrorKind::Sigma,error_same_file:false,revision:0,measurement_pending:false,uncertainty_source:serde_json::Value::Null,drag:None}}
}
impl Regions {
    pub fn replace_apertures(&mut self,apertures:Vec<Aperture>){self.drag=None;self.apertures.clear();self.selected=None;self.changed();for aperture in apertures{self.add_aperture(aperture);}self.history.clear();self.redo.clear();}
    pub fn add_aperture(&mut self,aperture:Aperture){
        if self.apertures.len()>=64{self.message="At most 64 apertures per image".into();return;}
        if let Err(error)=aperture.shape.validate(){self.message=error;return;}
        self.remember();self.apertures.push(aperture);self.selected=Some(self.apertures.len()-1);self.changed();self.open=true;
    }
    pub fn accept_sky_template(&mut self,value:serde_json::Value){
        match RegionTemplate::parse(&value.to_string()){Ok(template)=>{self.preview_template=Some(template);self.message="Sky boundaries transformed. Review the cyan preview before applying.".into();if self.auto_confirm{self.confirm_paste();}},Err(error)=>self.message=error,}
    }
    fn accept_options(&mut self,options:MeasureOptions){self.use_noise=options.noise.is_some();if let Some(noise)=options.noise{self.gain=noise.electrons_per_unit;self.read_noise=noise.read_noise_electrons;}self.options=options;}
    pub fn accept_growth(&mut self,value:serde_json::Value){match serde_json::from_value(value["growth_curve"].clone()){Ok(curve)=>{self.growth=curve;self.growth_open=true;self.message.clear();},Err(error)=>self.message=error.to_string(),}}
    pub fn on_image(&mut self,image:&FitsImage){
        self.uncertainty_source=serde_json::Value::Null;
        let source=format!("{}#{}",image.info.path,image.info.hdu);
        if self.source!=source {self.drag=None;self.apertures.clear();self.results.clear();self.growth.clear();self.need_growth=None;self.need_batch_measure=false;self.preview_template=None;self.selected=None;self.history.clear();self.redo.clear();self.message.clear();self.source=source;
            self.error_path.clear();self.error_hdu=None;self.error_same_file=false;self.error_kind=ErrorKind::Sigma;self.measurement_pending=false;self.needs_measure=false;self.revision=self.revision.saturating_add(1);}
        else {self.changed();}
    }
    pub fn editing(&self)->bool {self.tool!=Tool::Pan}
    fn remember(&mut self){self.history.push(self.apertures.clone());if self.history.len()>16{self.history.remove(0);}self.redo.clear();}
    fn changed(&mut self){self.revision=self.revision.saturating_add(1);self.results.resize(self.apertures.len(),None);self.results.fill(None);self.growth.clear();self.need_growth=None;self.need_batch_measure=false;self.needs_measure=true;self.uncertainty_source=serde_json::Value::Null;}
    // Geometry edits invalidate only their own result. The shared revision still
    // rejects every in-flight reply produced before the edit.
    // 几何编辑仅使自身结果失效；统一 revision 拒绝编辑前已发出的异步结果。
    fn edited(&mut self,index:usize){
        self.revision=self.revision.saturating_add(1);self.results.resize(self.apertures.len(),None);
        if let Some(result)=self.results.get_mut(index){*result=None;}
        self.growth.clear();self.need_growth=None;self.need_batch_measure=false;self.needs_measure=true;
        self.uncertainty_source=serde_json::Value::Null;
        self.message="Aperture changed; measurement will be recalculated.".into();
    }
    pub fn select_aperture(&mut self,index:usize)->Result<(),String>{
        if index>=self.apertures.len(){return Err("Unknown aperture index".into());}
        if self.selected!=Some(index){
            self.selected=Some(index);self.drag=None;self.geometry_editor.clear();self.growth.clear();self.need_growth=None;
            self.revision=self.revision.saturating_add(1);self.need_batch_measure=false;
            self.needs_measure=self.results.get(index).is_none_or(Option::is_none);
        }
        self.open=true;Ok(())
    }
    pub fn delete_selected(&mut self)->bool{
        let Some(index)=self.selected.filter(|&i|i<self.apertures.len())else{return false;};
        self.remember();self.apertures.remove(index);if index<self.results.len(){self.results.remove(index);}
        self.results.resize(self.apertures.len(),None);self.selected=(!self.apertures.is_empty()).then(||index.min(self.apertures.len()-1));
        self.drag=None;self.geometry_editor.clear();self.revision=self.revision.saturating_add(1);
        self.growth.clear();self.need_growth=None;self.need_batch_measure=false;
        self.needs_measure=self.selected.is_some_and(|i|self.results.get(i).is_none_or(Option::is_none));
        self.uncertainty_source=serde_json::Value::Null;self.message="Selected aperture deleted".into();true
    }
    /// Clear only this image's geometry; every queued measurement becomes stale.
    /// 只清当前图像孔径；递增 revision，迟到测光不会恢复已清除结果。
    pub fn clear_all(&mut self){
        if !self.apertures.is_empty(){self.remember();}
        self.apertures.clear();self.results.clear();self.selected=None;self.draft.clear();
        self.drag=None;self.geometry_editor.clear();self.preview_template=None;self.profile_request=None;
        self.growth.clear();self.need_growth=None;self.need_batch_measure=false;self.needs_measure=false;
        self.need_sky_paste=false;self.uncertainty_source=serde_json::Value::Null;
        self.revision=self.revision.saturating_add(1);self.message="All apertures cleared".into();
    }
    fn replace_selected(&mut self,aperture:Aperture)->Result<(),String>{
        validate_aperture(&aperture)?;
        let index=self.selected.filter(|&i|i<self.apertures.len()).ok_or("Select an aperture first")?;
        if same_aperture(&self.apertures[index],&aperture){return Ok(());}
        self.remember();self.apertures[index]=aperture;self.drag=None;self.edited(index);Ok(())
    }
    /// Centers are zero-based and clamped to the image. Source/background masks
    /// translate together; a source resize never discards custom exclusions.
    /// 中心使用零基像素并限制在图像内；源区、背景和掩码同步平移。
    pub fn update_selected_circle(&mut self,center:[f64;2],radius:f64,sky:Option<[f64;2]>,image:&FitsImage)->Result<(),String>{
        let index=self.selected.filter(|&i|i<self.apertures.len()).ok_or("Select an aperture first")?;
        let aperture=edited_circle(&self.apertures[index],center,radius,sky,image)?;
        self.replace_selected(aperture)
    }
    pub fn resize_selected(&mut self,radius:f64,image:&FitsImage)->Result<(),String>{
        let index=self.selected.filter(|&i|i<self.apertures.len()).ok_or("Select an aperture first")?;
        let Shape::Circle{center,..}=&self.apertures[index].shape else{return Err("Select a circular aperture".into());};
        self.update_selected_circle(*center,radius,None,image)
    }
    pub fn move_selected(&mut self,center:[f64;2],image:&FitsImage)->Result<(),String>{
        let index=self.selected.filter(|&i|i<self.apertures.len()).ok_or("Select an aperture first")?;
        let center=bounded_center(center,image)?;let original=shape_center(&self.apertures[index].shape);
        let aperture=translated_aperture(&self.apertures[index],center[0]-original[0],center[1]-original[1]);
        self.replace_selected(aperture)
    }
    fn undo(&mut self){if let Some(state)=self.history.pop(){self.redo.push(std::mem::replace(&mut self.apertures,state));self.selected=(!self.apertures.is_empty()).then_some(0);self.drag=None;self.geometry_editor.clear();self.changed();}}
    fn redo(&mut self){if let Some(state)=self.redo.pop(){self.history.push(std::mem::replace(&mut self.apertures,state));self.selected=(!self.apertures.is_empty()).then_some(0);self.drag=None;self.geometry_editor.clear();self.changed();}}
    fn delete_shortcut(&mut self,ctx:&egui::Context)->bool{
        if ctx.wants_keyboard_input() || self.drag.is_some() || self.selected.is_none(){return false;}
        let delete=ctx.input_mut(|input|input.consume_key(egui::Modifiers::NONE,egui::Key::Delete) || input.consume_key(egui::Modifiers::NONE,egui::Key::Backspace));
        delete && self.delete_selected()
    }
    pub fn has_error_map(&self)->bool{self.error_same_file || !self.error_path.trim().is_empty()}
    pub fn configure_uncertainty(&mut self,params:&serde_json::Value)->Result<(),String>{
        let kind=ErrorKind::parse(params["kind"].as_str().unwrap_or("sigma")).filter(|k|*k!=ErrorKind::Auto).ok_or("Choose sigma, variance or ivar")?;
        let path=params["path"].as_str().unwrap_or("");if path.len()>32768{return Err("Error-map path is too long".into());}
        let hdu=params["hdu"].as_u64().map(|v|v as usize);if hdu.is_some_and(|h|h>=4096){return Err("Invalid uncertainty HDU".into());}
        self.error_path=fitsviewer_core::pasted_path(path).into();self.error_same_file=params["same_file"].as_bool().unwrap_or(false);self.error_hdu=hdu;self.error_kind=kind;self.changed();Ok(())
    }
    fn error_source_ready(&self,image:&FitsImage)->bool{
        if !self.has_error_map(){return true;}
        if self.error_kind==ErrorKind::Auto{return false;}
        if !self.error_same_file{return true;}
        self.error_hdu.is_some_and(|index|image.info.hdus.iter().any(|h|h.index==index && h.viewable && h.axes.len()==2 && h.index!=image.info.hdu && !h.name.trim().eq_ignore_ascii_case("SCI")))
    }
    /// Invalid/unselected same-file uncertainty sources return null, never an
    /// implicit first SCI HDU. The caller must reject non-object parameters.
    /// 同文件误差 HDU 未明确选定时返回 null，调用方不得退回首个 SCI 图像。
    pub fn measurement_params(&self,image:&FitsImage,all:bool)->serde_json::Value{
        if !self.error_source_ready(image){return serde_json::Value::Null;}
        let indices:Vec<usize>=if all{(0..self.apertures.len()).collect()}else{self.selected.filter(|&i|i<self.apertures.len()).into_iter().collect()};
        if indices.is_empty(){return serde_json::Value::Null;}
        let apertures=indices.iter().map(|&index|&self.apertures[index]).collect::<Vec<_>>();
        let mut value=serde_json::json!({"schema":1,"revision":self.revision,"path":image.info.path,"hdu":image.info.hdu,"coordinate_origin":0,"indices":indices,"apertures":apertures,"options":self.options});
        if self.has_error_map(){value["error_path"]=serde_json::json!(if self.error_same_file{&image.info.path}else{&self.error_path});
            if let Some(hdu)=self.error_hdu{value["error_hdu"]=serde_json::json!(hdu);}
            value["error_kind"]=serde_json::json!(self.error_kind.as_str());}
        value
    }
    fn add(&mut self,shape:Shape,background:Option<Shape>){
        if self.apertures.len()>=64 {self.message="At most 64 editable apertures per image".into();return;}
        self.remember();self.apertures.push(Aperture{name:format!("A{}",self.apertures.len()+1),shape,background,exclude:vec![]});self.selected=Some(self.apertures.len()-1);self.changed();
    }
    pub fn measure_selected(&mut self,image:&FitsImage){
        if !self.needs_measure || self.measurement_pending{return;}
        if self.selected.is_none_or(|index|index>=self.apertures.len()){self.needs_measure=false;return;}
        if self.has_error_map() || matches!(image.pixels,fitsviewer_core::RawPixels::Unavailable){
            if !self.error_source_ready(image){self.message="Choose an uncertainty HDU and sigma, variance or ivar explicitly.".into();return;}
            if !self.need_batch_measure{self.need_batch_measure=true;self.needs_measure=false;}return;
        }
        self.needs_measure=false;
        self.uncertainty_source=serde_json::Value::Null;
        if let Some(index)=self.selected.filter(|&i|i<self.apertures.len()){
            match measure(image,&self.apertures[index],&self.options,None){Ok(result)=>{self.results.resize(self.apertures.len(),None);self.results[index]=Some(result);self.message.clear();},Err(error)=>self.message=error}
        }
    }
    pub fn panel(&mut self,ctx:&egui::Context,image:Option<&FitsImage>,pro:bool,windows:&mut crate::window_layout::Runtime){
        if self.open {let mut open=self.open;
            windows.show(ctx,crate::window_layout::Key::Photometry,crate::i18n::t("Aperture photometry"),&mut open,egui::vec2(430.,300.),|ui|self.ui(ui,ctx,image,pro));self.open=open;
        }
        if self.growth_open {let mut open=self.growth_open;windows.show(ctx,crate::window_layout::Key::Growth,crate::i18n::t("Aperture growth curve"),&mut open,egui::vec2(440.,300.),|ui|self.growth_ui(ui,ctx));self.growth_open=open;}
    }
    pub fn ui(&mut self,ui:&mut egui::Ui,ctx:&egui::Context,image:Option<&FitsImage>,pro:bool){
        crate::ribbon::align_controls(ui);
        ui.horizontal(|ui|{ui.strong(crate::i18n::t("Aperture tools"));crate::controls::information(ui,"aperture-help",|ui|{
            ui.label(crate::i18n::t("Click an aperture to select it. Drag inside to move; drag a circular edge to resize. Displayed coordinates are one-based."));
            ui.label(crate::i18n::t("Drag the yellow circle edge to resize; green edges edit the sky ring."));
            if pro{ui.label(crate::i18n::t("Line: two clicks. Polygon/polyline/curve: double-click the last point."));}
            ui.label(crate::i18n::t("Weighted sums use original physical pixel units; no photometric or surface-brightness calibration is inferred."));
            ui.label(crate::i18n::t("Enable only when these units apply to the image. Without a noise model/error map, total error and SNR stay unspecified."));
        });});
            if image.is_none_or(|i|i.spectrum.is_some()){ui.label(crate::i18n::t("Open a two-dimensional image for aperture photometry."));return;}
            ui.horizontal_wrapped(|ui|{for (tool,label) in [(Tool::Pan,"Pan"),(Tool::Circle,"Circle")]{if ui.selectable_value(&mut self.tool,tool,crate::i18n::t(&label)).clicked(){self.draft.clear();self.drag=None;}}});
            #[cfg(feature="pro")]
if pro{ui.horizontal_wrapped(|ui|{for (tool,label) in [(Tool::Ellipse,"Ellipse"),(Tool::Rectangle,"Rectangle"),(Tool::Polygon,"Polygon"),(Tool::Line,"Line"),(Tool::Polyline,"Polyline"),(Tool::Curve,"Curve")]{if ui.selectable_value(&mut self.tool,tool,crate::i18n::t(&label)).clicked(){self.draft.clear();}}});}
            self.selection_ui(ui,image.unwrap());
            let current=image.unwrap();
            ui.horizontal_wrapped(|ui|{
                let ready=if pro{!self.apertures.is_empty()}else{self.selected.is_some_and(|index|index<self.apertures.len())};
                if ui.add_enabled(ready && !self.measurement_pending && self.error_source_ready(current),egui::Button::new(crate::i18n::t(if pro{"Measure all apertures"}else{"Measure selected aperture"}))).clicked(){if pro || self.has_error_map(){self.need_batch_measure=true;self.needs_measure=false;}else{self.needs_measure=true;}}
                #[cfg(feature="pro")]
                if pro{let circle=self.selected.and_then(|index|self.apertures.get(index)).is_some_and(|aperture|matches!(aperture.shape,Shape::Circle{..}));
                    if ui.add_enabled(circle && !self.measurement_pending && self.error_source_ready(current),egui::Button::new(crate::i18n::t("Growth curve of selected circle"))).clicked(){self.request_growth(current);}
                }
            });
            if self.measurement_pending{ui.horizontal(|ui|{ui.spinner();ui.label(crate::i18n::t("Measuring…"));});}
            if !self.message.is_empty(){ui.add(egui::Label::new(crate::i18n::t(&self.message)).wrap());}
            if self.has_error_map() && !self.error_source_ready(current){ui.label(crate::i18n::t("Choose an uncertainty HDU and sigma, variance or ivar explicitly."));}

            egui::CollapsingHeader::new(crate::i18n::t("Defaults for new apertures")).id_salt("aperture-defaults").default_open(self.apertures.is_empty()).show(ui,|ui|{
                ui.horizontal_wrapped(|ui|{ui.label(crate::i18n::t("Radius"));ui.add(egui::DragValue::new(&mut self.radius).range(0.1..=512.).speed(0.1));ui.label(crate::i18n::t("Sky ring"));ui.add(egui::DragValue::new(&mut self.inner).range(0.2..=1024.).speed(0.1));ui.add(egui::DragValue::new(&mut self.outer).range(0.3..=2048.).speed(0.1));});
                #[cfg(feature="pro")]
                if pro{ui.horizontal_wrapped(|ui|{ui.label(crate::i18n::t("Path width / background gap / width"));ui.add(egui::DragValue::new(&mut self.width).range(0.1..=256.));ui.add(egui::DragValue::new(&mut self.gap).range(0.0..=256.));ui.add(egui::DragValue::new(&mut self.background_width).range(0.1..=256.));});}
            });
            egui::CollapsingHeader::new(crate::i18n::t("Explicit uncertainty model")).id_salt("Explicit uncertainty model").show(ui,|ui|{
                let mut changed=ui.checkbox(&mut self.use_noise,crate::i18n::t("Poisson + read noise in electron units")).changed();
                changed|=ui.add(egui::DragValue::new(&mut self.gain).range(0.001..=1e7).prefix(crate::i18n::t("electrons per physical unit: "))).changed();
                changed|=ui.add(egui::DragValue::new(&mut self.read_noise).range(0.0..=1e5).prefix(crate::i18n::t("read noise [electrons]: "))).changed();
                if changed{self.options.noise=self.use_noise.then_some(NoiseModel{electrons_per_unit:self.gain,read_noise_electrons:self.read_noise});self.changed();}
                let mut changed=ui.add(egui::DragValue::new(&mut self.options.subpixels).range(1..=32).prefix(crate::i18n::t("Subpixel sampling: "))).changed();
                changed|=ui.add(egui::DragValue::new(&mut self.options.clip_sigma).range(0.5..=10.).prefix(crate::i18n::t("Background MAD clip sigma: "))).changed();
                if changed{self.changed();}
                let current=image.unwrap();
                let mut error_changed=ui.checkbox(&mut self.error_same_file,crate::i18n::t("Use an error HDU from this FITS")).changed();
                if error_changed{self.error_hdu=None;}
                if self.error_same_file{
                    ui.label(crate::i18n::t("Error HDU (zero-based)"));
                    let selected=self.error_hdu.and_then(|index|current.info.hdus.iter().find(|h|h.index==index)).map(|h|format!("HDU {} · {}",h.index,h.name)).unwrap_or_else(||crate::i18n::t("Choose an uncertainty HDU").into());
                    egui::ComboBox::from_id_salt("aperture-error-hdu").selected_text(selected).show_ui(ui,|ui|{
                        error_changed|=ui.selectable_value(&mut self.error_hdu,None,crate::i18n::t("Choose an uncertainty HDU")).changed();
                        for h in current.info.hdus.iter().filter(|h|h.viewable && h.axes.len()==2 && h.index!=current.info.hdu && !h.name.trim().eq_ignore_ascii_case("SCI")){
                            error_changed|=ui.selectable_value(&mut self.error_hdu,Some(h.index),format!("HDU {} · {} · {}×{}",h.index,h.name,h.axes[0],h.axes[1])).changed();
                        }
                    });
                    if self.error_hdu.is_none(){ui.label(crate::i18n::t("Select the uncertainty HDU explicitly; the science HDU is not an error map."));}
                }else{
                    ui.label(crate::i18n::t("Other FITS error map path"));error_changed|=ui.text_edit_singleline(&mut self.error_path).changed();
                    let mut specific=self.error_hdu.is_some();
                    if ui.checkbox(&mut specific,crate::i18n::t("Use a specific error HDU")).changed(){self.error_hdu=specific.then_some(0);error_changed=true;}
                    if let Some(hdu)=&mut self.error_hdu{error_changed|=ui.add(egui::DragValue::new(hdu).range(0..=4095).prefix(crate::i18n::t("Error HDU (zero-based): "))).changed();}
                }
                ui.label(crate::i18n::t("Error values"));
                let label=match self.error_kind{ErrorKind::Sigma=>"Standard deviation (sigma)",ErrorKind::Variance=>"Variance",ErrorKind::Ivar=>"Inverse variance (ivar)",ErrorKind::Auto=>"Choose uncertainty type"};
                egui::ComboBox::from_id_salt("aperture-error-kind").selected_text(crate::i18n::t(label)).show_ui(ui,|ui|{
                    for (kind,label) in [(ErrorKind::Sigma,"Standard deviation (sigma)"),(ErrorKind::Variance,"Variance"),(ErrorKind::Ivar,"Inverse variance (ivar)")]{error_changed|=ui.selectable_value(&mut self.error_kind,kind,crate::i18n::t(label)).changed();}
                });
                if error_changed{self.changed();}
            });
            if let Some(result)=self.selected.and_then(|i|self.results.get(i)).and_then(|r|r.as_ref()){
                if let Some(image)=image{let unit=header_unit(&image.info.header);ui.label(format!("{}: {}",crate::i18n::t("Flux unit"),if unit.is_empty(){crate::i18n::t("unit unknown")}else{unit.as_str()}));}
                if let Some(hdu)=self.uncertainty_source["hdu"].as_u64(){ui.label(format!("{}: {}",crate::i18n::t("Uncertainty HDU"),hdu));}
                ui.label(format!("{} {:0.4} px² · {} {:0.8e}",crate::i18n::t("Area"),result.effective_area,crate::i18n::t("Weighted sum"),result.aperture_sum));
                ui.label(format!("{} {} · {} {:0.8e}",crate::i18n::t("Background"),result.background_per_pixel.map(|v|format!("{v:0.6}")).unwrap_or_else(||crate::i18n::t("not subtracted").into()),crate::i18n::t("Net weighted sum"),result.net_flux));
                ui.label(format!("{} {} · SNR {}",crate::i18n::t("Error"),result.total_error.map(|v|format!("{v:0.6e}")).unwrap_or_else(||crate::i18n::t("not computed").into()),result.snr.map(|v|format!("{v:0.3}")).unwrap_or_else(||crate::i18n::t("not computed").into())));
                for flag in &result.flags{ui.weak(crate::i18n::t(flag));}
                if ui.button(crate::i18n::t("Export current result / Copy JSON")).clicked(){if let Some(image)=image{let params=self.measurement_params(image,false);
                    let text=serde_json::to_string_pretty(&serde_json::json!({"schema":1,"coordinates":"zero_based_pixels","revision":self.revision,"path":image.info.path,"hdu":image.info.hdu,"BUNIT":header_unit(&image.info.header),"error_path":params["error_path"],"error_hdu":params["error_hdu"],"error_kind":params["error_kind"],"error_same_file":self.error_same_file,"resolved_uncertainty_source":self.uncertainty_source,"aperture":self.apertures[self.selected.unwrap()],"measurement":result,"options":self.options})).unwrap_or_default();ctx.copy_text(text);}}
            }
            #[cfg(feature="pro")]
if pro{
                if let Some(index)=self.selected.filter(|&i|i<self.apertures.len()){
                    egui::CollapsingHeader::new(crate::i18n::t("Edit geometry / background / exclusions")).id_salt("aperture-geometry").show(ui,|ui|{
                        if ui.button(crate::i18n::t("Load selected aperture for editing")).clicked(){self.geometry_editor=serde_json::to_string_pretty(&self.apertures[index]).unwrap_or_default();}
                        ui.add(egui::TextEdit::multiline(&mut self.geometry_editor).code_editor().desired_rows(6));
                        if ui.button(crate::i18n::t("Apply edited geometry")).clicked(){match serde_json::from_str::<Aperture>(&self.geometry_editor).map_err(|e|e.to_string()).and_then(|aperture|self.replace_selected(aperture)){Ok(())=>self.message.clear(),Err(error)=>self.message=error,}}
                    });
                    if let Shape::Path{points,width,..}=self.apertures[index].shape.flattened(){if ui.button(crate::i18n::t("Inspect selected line / curve profile")).clicked(){let (gap,background_width)=match &self.apertures[index].background{Some(Shape::Path{width:outer,inner_width,..})=>((inner_width-width).max(0.)/2.,(outer-inner_width).max(0.2)/2.),_=>(self.gap,self.background_width)};self.profile_request=Some(serde_json::json!({"points":points,"width":width,"background_gap":gap,"background_width":background_width}));}}
                }
                egui::CollapsingHeader::new(crate::i18n::t("Aperture templates")).id_salt("aperture-templates").show(ui,|ui|{
                ui.horizontal(|ui|{
                    if ui.button(crate::i18n::t("Copy aperture")).clicked(){if let Some(image)=image {self.copy(ctx,image);}}
                });
                egui::CollapsingHeader::new(crate::i18n::t("Template content")).id_salt("Template content").show(ui,|ui|{ui.checkbox(&mut self.copy_background,crate::i18n::t("Copy background region"));ui.checkbox(&mut self.copy_exclusions,crate::i18n::t("Copy exclusion regions"));ui.checkbox(&mut self.copy_options,crate::i18n::t("Copy measurement parameters"));if ui.button(crate::i18n::t("Copy all apertures")).clicked(){if let Some(image)=image{self.copy_all(ctx,image);}}});
                egui::ComboBox::new("Paste method",crate::i18n::t("Paste method")).selected_text(crate::i18n::t(&self.paste_mode)).show_ui(ui,|ui|{for mode in ["Keep pixels","At cursor","Keep sky position"]{ui.selectable_value(&mut self.paste_mode,mode.into(),crate::i18n::t(mode));}});
                ui.add(egui::TextEdit::multiline(&mut self.template).desired_rows(4).hint_text(crate::i18n::t("Region template JSON; Ctrl+V on the image also works")));
                if ui.button(crate::i18n::t("Paste template")).clicked(){self.auto_confirm=false;self.paste(None,image);}
                if let Some(preview)=&self.preview_template{ui.label(format!("{}: {}",crate::i18n::t("Staged apertures (cyan)"),preview.apertures.len()));ui.horizontal(|ui|{if ui.button(crate::i18n::t("Apply preview")).clicked(){self.confirm_paste();}if ui.button(crate::i18n::t("Cancel preview")).clicked(){self.preview_template=None;}});}
                ui.text_edit_singleline(&mut self.template_io_path);
                ui.horizontal(|ui|{
                    if ui.button(crate::i18n::t("Save template JSON")).clicked(){if let Err(error)=crate::text_io::save(&self.template_io_path,&self.template){self.message=error;}}
                    if ui.button(crate::i18n::t("Load template JSON")).clicked(){
                        #[cfg(not(target_arch="wasm32"))] {match crate::text_io::read(&self.template_io_path).and_then(|text|{RegionTemplate::parse(&text)?;Ok(text)}){Ok(text)=>{self.template=text;self.auto_confirm=false;self.paste(None,image);},Err(error)=>self.message=error,}}
                        #[cfg(target_arch="wasm32")] {if let Err(error)=crate::text_io::browser("importRegions",&[]){self.message=error;}}
                    }
                });
                });
            }
    }
    #[cfg(feature="pro")]
    fn request_growth(&mut self,image:&FitsImage){
        if let Some(index)=self.selected{if let Shape::Circle{radius,..}=&self.apertures[index].shape{
            let maximum=match &self.apertures[index].background{Some(Shape::Ring{inner,..})=>inner*0.9,_=>radius*2.};
            let radii=(1..=16).map(|i|maximum*i as f64/16.).collect::<Vec<_>>();
            if self.has_error_map(){self.need_growth=Some(radii);}else{match growth_curve(image,&self.apertures[index],&self.options,&radii){Ok(curve)=>{self.growth=curve;self.growth_open=true;},Err(error)=>self.message=error,}}
        }}
    }
    fn selection_ui(&mut self,ui:&mut egui::Ui,image:&FitsImage){
        ui.group(|ui|{
            ui.horizontal_wrapped(|ui|{
                ui.strong(crate::i18n::t("Selected aperture"));
                if ui.add_enabled(self.selected.is_some_and(|i|i<self.apertures.len()),egui::Button::new(crate::i18n::t("Delete selected aperture"))).on_hover_text(crate::i18n::t("Delete / Backspace; text fields keep their normal editing keys")).clicked(){self.delete_selected();}
            });
            if ui.add_enabled(!self.apertures.is_empty() || !self.draft.is_empty(),egui::Button::new(crate::i18n::t("Clear all apertures"))).clicked(){self.clear_all();}
            if self.apertures.is_empty(){ui.weak(crate::i18n::t("Choose Circle, then click the image to add an aperture."));}
            else{
                let mut selected=self.selected;
                let label=selected.and_then(|i|self.apertures.get(i)).map_or(crate::i18n::t("Select an aperture"),|a|a.name.as_str());
                egui::ComboBox::from_id_salt("selected-aperture").selected_text(label).show_ui(ui,|ui|{for (index,aperture) in self.apertures.iter().enumerate(){ui.selectable_value(&mut selected,Some(index),&aperture.name);}});
                if let Some(index)=selected.filter(|&i|i<self.apertures.len()){let _=self.select_aperture(index);}
                if let Some(index)=self.selected.filter(|&i|i<self.apertures.len()){
                    let mut aperture=self.apertures[index].clone();
                    if ui.add(egui::TextEdit::singleline(&mut aperture.name).char_limit(128).hint_text(crate::i18n::t("Aperture name"))).changed(){if let Err(error)=self.replace_selected(aperture){self.message=error;}}
                    if let Shape::Circle{center,radius}=&self.apertures[index].shape{
                        let mut center=[center[0]+1.,center[1]+1.];let mut radius=*radius;let mut changed=false;
                        ui.horizontal_wrapped(|ui|{
                            ui.label(crate::i18n::t("Center x / y [one-based pixels]"));
                            changed|=ui.add(egui::DragValue::new(&mut center[0]).range(1. ..=image.info.width.max(1) as f64).speed(0.1)).changed();
                            changed|=ui.add(egui::DragValue::new(&mut center[1]).range(1. ..=image.info.height.max(1) as f64).speed(0.1)).changed();
                        });
                        ui.horizontal_wrapped(|ui|{ui.label(crate::i18n::t("Radius [pixels]"));changed|=ui.add(egui::DragValue::new(&mut radius).range(MIN_RADIUS..=MAX_RADIUS).speed(0.1)).changed();});
                        let mut sky=None;
                        if let Some(Shape::Ring{inner,outer,..})=&self.apertures[index].background{
                            let [mut inner,mut outer]=fit_sky_ring(radius,*inner,*outer).unwrap_or([*inner,*outer]);let mut sky_changed=false;
                            ui.horizontal_wrapped(|ui|{
                                ui.label(crate::i18n::t("Sky inner / outer [pixels]"));
                                sky_changed|=ui.add(egui::DragValue::new(&mut inner).range((radius+0.1)..=(outer-0.1).max(radius+0.1)).speed(0.1)).changed();
                                sky_changed|=ui.add(egui::DragValue::new(&mut outer).range((inner+0.1)..=MAX_SKY_RADIUS).speed(0.1)).changed();
                            });
                            if sky_changed{sky=Some([inner,outer]);changed=true;}
                        }
                        if changed{if let Err(error)=self.update_selected_circle([center[0]-1.,center[1]-1.],radius,sky,image){self.message=error;}}
                    }
                }else{ui.weak(crate::i18n::t("Click an aperture on the image or choose it from the list."));}
            }
            ui.horizontal(|ui|{
                if ui.add_enabled(!self.history.is_empty(),egui::Button::new(crate::i18n::t("Undo"))).clicked(){self.undo();}
                if ui.add_enabled(!self.redo.is_empty(),egui::Button::new(crate::i18n::t("Redo"))).clicked(){self.redo();}
            });
        });
    }
    pub fn growth_ui(&mut self,ui:&mut egui::Ui,ctx:&egui::Context){
        ui.label(&self.source);
        if self.growth.is_empty(){ui.label(crate::i18n::t("No growth curve for this image"));return;}
            if ui.button(crate::i18n::t("Copy curve JSON")).clicked(){ctx.copy_text(serde_json::to_string_pretty(&self.growth).unwrap_or_default());}
            let (response,painter)=ui.allocate_painter(egui::vec2(ui.available_width().max(120.),(ui.clip_rect().bottom()-ui.cursor().min.y).clamp(120.,600.)),egui::Sense::hover());let area=response.rect.shrink(24.);
            if !self.growth.is_empty(){let xmax=self.growth.last().unwrap().0;let ymin=self.growth.iter().map(|p|p.1.net_flux).fold(0.,f64::min);let ymax=self.growth.iter().map(|p|p.1.net_flux).fold(0.,f64::max).max(ymin+1e-10);let points=self.growth.iter().map(|(r,m)|egui::pos2(area.left()+(*r/xmax) as f32*area.width(),area.bottom()-((m.net_flux-ymin)/(ymax-ymin)) as f32*area.height())).collect();painter.add(egui::Shape::line(points,Stroke::new(1.5_f32,Color32::LIGHT_BLUE)));painter.text(area.left_top(),egui::Align2::LEFT_TOP,crate::i18n::t("Net weighted sum vs radius [pixels]"),egui::FontId::proportional(12.),Color32::GRAY);}
    }
    pub fn copy(&mut self,ctx:&egui::Context,image:&FitsImage){let apertures=self.selected.and_then(|i|self.apertures.get(i)).cloned().into_iter().collect();self.copy_regions(ctx,image,apertures);}
    fn copy_all(&mut self,ctx:&egui::Context,image:&FitsImage){self.copy_regions(ctx,image,self.apertures.clone());}
    fn copy_regions(&mut self,ctx:&egui::Context,image:&FitsImage,mut apertures:Vec<Aperture>){
        for aperture in &mut apertures{if !self.copy_background{aperture.background=None;}if !self.copy_exclusions{aperture.exclude.clear();}}
        let effective_header=self.wcs_override.as_deref().unwrap_or(&image.info.header);
        let mut header=effective_header.lines().filter(|line|!line.starts_with("NAXIS1 ") && !line.starts_with("NAXIS2 ") && ["WCSAXES","WCSNAME","CTYPE","CUNIT","CRVAL","CRPIX","CDELT","CROTA","RADESYS","RADECSYS","EQUINOX","LONPOLE","LATPOLE","NAXIS","CD1_","CD2_","PC1_","PC2_","PV1_","PV2_","PS1_","PS2_","A_","B_","AP_","BP_","PLTRA","PLTDEC","PLTSCALE","PLTEQUIN","PPO","AMDX","AMDY","CNPIX","XPIXELSZ","YPIXELSZ","CQDIS","DQ1.","DQ2.","HIERARCH DQ1.","HIERARCH DQ2."].iter().any(|key|line.starts_with(key))).collect::<Vec<_>>().join("\n");
        header.push_str(&format!("\nNAXIS1  = {}\nNAXIS2  = {}",image.info.width,image.info.height));
        let template=RegionTemplate{schema:1,source_dimensions:[image.info.width,image.info.height],source_hdu:image.info.hdu,source_header:header,apertures,options:self.copy_options.then(||self.options.clone()),source_units:header_unit(&image.info.header),source_wcs_complete:!["CPDIS","D2IM","DET2IM"].iter().any(|key|effective_header.contains(key))};
        self.template=serde_json::to_string_pretty(&template).unwrap_or_default();ctx.copy_text(self.template.clone());self.message="Template copied; target measurements are recalculated.".into();
        if let Err(error)=crate::text_io::remember_regions(&self.template){self.message=error;}
    }
    pub fn paste(&mut self,cursor:Option<[f64;2]>,image:Option<&FitsImage>){
        self.preview_template=None;
        let mut template=match RegionTemplate::parse(&self.template){Ok(t)=>t,Err(e)=>{self.message=e;return;}};
        if self.paste_mode=="Keep sky position"{self.need_sky_paste=true;return;}
        let shift=if self.paste_mode=="At cursor" {
            let Some(cursor)=cursor else{self.message="Move the pointer over the target image, then press Ctrl+V.".into();return;};
            let Some(first)=template.apertures.first() else{return;};let b=first.shape.bounds();[cursor[0]-(b[0]+b[2])/2.,cursor[1]-(b[1]+b[3])/2.]
        }else{[0.,0.]};
        for aperture in &mut template.apertures{aperture.shape=aperture.shape.translated(shift[0],shift[1]);aperture.background=aperture.background.as_ref().map(|s|s.translated(shift[0],shift[1]));aperture.exclude=aperture.exclude.iter().map(|s|s.translated(shift[0],shift[1])).collect();}
        self.message=if image.is_some_and(|i|[i.info.width,i.info.height]!=template.source_dimensions){"Target grid differs. Review the pixel preview; no sky alignment is implied.".into()}else{"Review the cyan preview before applying.".into()};
        self.preview_template=Some(template);self.open=true;
        if self.auto_confirm{self.confirm_paste();}
    }
    pub fn confirm_paste(&mut self){
        let Some(template)=self.preview_template.take()else{return;};
        if self.apertures.len()+template.apertures.len()>64{self.message="Pasting would exceed 64 apertures".into();self.preview_template=Some(template);return;}
        self.remember();if let Some(options)=template.options{self.accept_options(options);}
        self.apertures.extend(template.apertures);self.selected=self.apertures.len().checked_sub(1);self.changed();
        self.message="Target aperture measurements recalculated; copied flux is never reused.".into();self.auto_confirm=false;
        let _=crate::text_io::remember_regions(&self.template);
    }
    fn hit_test(&self,p:[f64;2],zoom:f32)->Option<(usize,DragKind)>{
        if !self.draft.is_empty() || !zoom.is_finite() || zoom<=0.{return None;}
        // Selected edge handles win; otherwise choose the last drawn aperture.
        // 优先命中选中项边缘，否则按绘制顺序从后向前选取。
        if let Some(index)=self.selected.filter(|&i|i<self.apertures.len()){if let Some(kind)=edge_hit(&self.apertures[index],p,zoom){return Some((index,kind));}}
        for (index,aperture) in self.apertures.iter().enumerate().rev(){if let Some(kind)=edge_hit(aperture,p,zoom){return Some((index,kind));}}
        self.apertures.iter().enumerate().rev().find(|(_,a)|a.shape.contains(p) || a.background.as_ref().is_some_and(|s|s.contains(p))).map(|(index,_)|(index,DragKind::Move))
    }
    /// Call before applying primary-button image pan. Hit testing uses the press
    /// origin, so a pan crossing an aperture cannot turn into a geometry edit.
    /// 在主键平移图像前调用；按下点决定归属，经过孔径不会意外接管平移。
    pub fn claims_primary_drag(&self,ctx:&egui::Context,rect:Rect,zoom:f32)->bool{
        if self.drag.is_some(){return true;}
        self.claims_primary_drag_transformed(ctx,ImageTransform::from_rect(rect,zoom))
    }
    pub fn claims_primary_drag_transformed(&self,ctx:&egui::Context,transform:ImageTransform)->bool{
        if self.drag.is_some(){return true;}
        ctx.input(|input|input.pointer.primary_pressed().then(||input.pointer.press_origin()).flatten()).filter(|p|transform.contains(*p)).is_some_and(|p|self.hit_test(transform.screen_to_pixel(p),transform.zoom).is_some())
    }
    fn begin_drag(&mut self,index:usize,kind:DragKind,origin:[f64;2]){
        if self.select_aperture(index).is_ok(){self.drag=Some(ApertureDrag{index,origin,original:self.apertures[index].clone(),kind,changed:false});self.needs_measure=false;}
    }
    fn update_drag(&mut self,p:[f64;2],image:&FitsImage)->Result<(),String>{
        let Some(mut drag)=self.drag.clone()else{return Ok(());};
        if self.selected!=Some(drag.index) || drag.index>=self.apertures.len(){self.drag=None;return Ok(());}
        let aperture=match drag.kind{
            DragKind::Move=>{
                let center=shape_center(&drag.original.shape);let target=bounded_center([center[0]+p[0]-drag.origin[0],center[1]+p[1]-drag.origin[1]],image)?;
                translated_aperture(&drag.original,target[0]-center[0],target[1]-center[1])
            },
            DragKind::Radius=>{
                let Shape::Circle{center,radius}=&drag.original.shape else{return Ok(());};
                let delta=distance(p,*center)-distance(drag.origin,*center);
                edited_circle(&drag.original,*center,(radius+delta).clamp(MIN_RADIUS,MAX_RADIUS),None,image)?
            },
            DragKind::SkyInner|DragKind::SkyOuter=>{
                let Shape::Circle{radius,..}=&drag.original.shape else{return Ok(());};
                let Some(Shape::Ring{center,inner,outer})=&drag.original.background else{return Ok(());};
                let [inner,outer]=fit_sky_ring(*radius,*inner,*outer)?;let delta=distance(p,*center)-distance(drag.origin,*center);
                let (inner,outer)=if drag.kind==DragKind::SkyInner{((inner+delta).clamp(radius+0.1,(outer-0.1).max(radius+0.1)),outer)}else{(inner,(outer+delta).clamp(inner+0.1,MAX_SKY_RADIUS))};
                let mut aperture=drag.original.clone();aperture.background=Some(Shape::Ring{center:*center,inner,outer});aperture
            },
        };
        validate_aperture(&aperture)?;
        if !same_aperture(&self.apertures[drag.index],&aperture){
            if !drag.changed{self.remember();}
            self.apertures[drag.index]=aperture;self.edited(drag.index);self.needs_measure=false;drag.changed=true;
        }
        self.drag=Some(drag);Ok(())
    }
    fn finish_drag(&mut self){if let Some(drag)=self.drag.take(){self.needs_measure=drag.changed || self.results.get(drag.index).is_none_or(Option::is_none);}}
    pub fn canvas(&mut self,ctx:&egui::Context,response:&egui::Response,painter:&egui::Painter,rect:Rect,zoom:f32,image:&FitsImage,pro:bool){
        self.canvas_transformed(ctx,response,painter,ImageTransform::from_rect(rect,zoom),image,pro,true);
    }
    pub fn canvas_transformed(&mut self,ctx:&egui::Context,response:&egui::Response,painter:&egui::Painter,transform:ImageTransform,image:&FitsImage,pro:bool,interactive:bool){
        let zoom=transform.zoom;
        if image.spectrum.is_some() || !zoom.is_finite() || zoom<=0.{self.drag=None;return;}
        let pixel=|pos:Pos2|transform.screen_to_pixel(pos);
        let current=response.hover_pos().filter(|p|transform.contains(*p)).map(pixel);
        if interactive{
        self.delete_shortcut(ctx);
        #[cfg(feature="pro")]
        if pro && !ctx.wants_keyboard_input(){
            if ctx.input_mut(|i|i.consume_key(egui::Modifiers::CTRL,egui::Key::C)){self.copy(ctx,image);}
            let pasted=ctx.input(|i|i.events.iter().find_map(|event|if let egui::Event::Paste(text)=event{Some(text.clone())}else{None}));
            if let Some(text)=pasted.filter(|s|s.contains("apertures") && s.contains("source_dimensions")){self.template=text;self.auto_confirm=false;self.paste(current,Some(image));}
        }
        if response.hovered() && ctx.input(|input|input.pointer.primary_pressed()){
            if let Some(origin)=ctx.input(|input|input.pointer.press_origin()).filter(|p|transform.contains(*p)).map(pixel){
                if let Some((index,kind))=self.hit_test(origin,zoom){self.begin_drag(index,kind,origin);}
            }
        }
        if response.dragged_by(egui::PointerButton::Primary){if let Some(p)=response.interact_pointer_pos().map(pixel){if let Err(error)=self.update_drag(p,image){self.message=error;}}}
        if response.clicked_by(egui::PointerButton::Primary){if let Some(p)=current {
            if let Some((index,_))=self.hit_test(p,zoom){let _=self.select_aperture(index);}
            else{match self.tool {
                Tool::Circle=>{let center=bounded_center(p,image).unwrap_or(p);let radius=self.radius;self.add(Shape::Circle{center,radius},Some(Shape::Ring{center,inner:self.inner.max(radius+0.1),outer:self.outer.max(self.inner+0.2).max(radius+0.3)}));},
                Tool::Pan|Tool::Move=>{if self.selected.take().is_some(){self.geometry_editor.clear();self.growth.clear();self.need_growth=None;self.need_batch_measure=false;self.needs_measure=false;self.revision=self.revision.saturating_add(1);}},
                #[cfg(feature="pro")]
                Tool::Line|Tool::Polyline|Tool::Curve|Tool::Polygon|Tool::Ellipse|Tool::Rectangle if pro=>{
                    self.draft.push(p);
                    if (self.tool==Tool::Line || self.tool==Tool::Ellipse || self.tool==Tool::Rectangle) && self.draft.len()==2 || response.double_clicked() && self.draft.len()>=if self.tool==Tool::Polygon{3}else{2} {
                        let points=std::mem::take(&mut self.draft);
                        let shape=match self.tool {
                            Tool::Polygon=>Shape::Polygon{points:points.clone(),holes:vec![]},
                            Tool::Ellipse=>Shape::Ellipse{center:points[0],radii:[(points[1][0]-points[0][0]).abs().max(0.1),(points[1][1]-points[0][1]).abs().max(0.1)],angle_deg:0.},
                            Tool::Rectangle=>Shape::Rectangle{center:points[0],size:[2.*(points[1][0]-points[0][0]).abs().max(0.1),2.*(points[1][1]-points[0][1]).abs().max(0.1)],angle_deg:0.},
                            _=>Shape::Path{points:points.clone(),width:self.width,inner_width:0.,smooth:self.tool==Tool::Curve},
                        };
                        let background=matches!(self.tool,Tool::Line|Tool::Polyline|Tool::Curve).then_some(Shape::Path{points,width:self.width+2.*self.gap+2.*self.background_width,inner_width:self.width+2.*self.gap,smooth:self.tool==Tool::Curve});self.add(shape,background);
                    }
                },_=>{},
            }}
        }}
        if response.drag_stopped_by(egui::PointerButton::Primary) || !ctx.input(|input|input.pointer.primary_down()){self.finish_drag();}
        if response.hovered(){if let Some((_,kind))=current.and_then(|p|self.hit_test(p,zoom)){
            ctx.set_cursor_icon(if kind==DragKind::Move{if self.drag.is_some(){egui::CursorIcon::Grabbing}else{egui::CursorIcon::Grab}}else{egui::CursorIcon::ResizeHorizontal});
            response.clone().on_hover_text(crate::i18n::t("Drag inside to move; drag a circular edge to resize. Delete removes the selection."));
        }}
        }
        for (index,aperture) in self.apertures.iter().enumerate(){draw_shape_transformed(painter,transform,&aperture.shape,if self.selected==Some(index){Color32::YELLOW}else{Color32::LIGHT_BLUE});if let Some(background)=&aperture.background{draw_shape_transformed(painter,transform,background,Color32::from_rgb(90,190,110));}}
        if let Some(aperture)=self.selected.and_then(|i|self.apertures.get(i)){if let Shape::Circle{center,radius}=&aperture.shape{draw_circle_handles(painter,transform,*center,*radius,Color32::YELLOW);}}
        #[cfg(feature="pro")]
if pro{if let Some(template)=&self.preview_template{for aperture in &template.apertures{draw_shape_transformed(painter,transform,&aperture.shape,Color32::LIGHT_BLUE);if let Some(background)=&aperture.background{draw_shape_transformed(painter,transform,background,Color32::LIGHT_BLUE);}}}}
        #[cfg(feature="pro")]
        if self.draft.len()>1{draw_shape_transformed(painter,transform,&Shape::Path{points:self.draft.clone(),width:self.width,inner_width:0.,smooth:false},Color32::LIGHT_RED);}
    }
}
fn same_aperture(a:&Aperture,b:&Aperture)->bool{a.name==b.name && a.shape==b.shape && a.background==b.background && a.exclude==b.exclude}
fn validate_aperture(aperture:&Aperture)->Result<(),String>{
    aperture.shape.validate()?;if let Some(shape)=&aperture.background{shape.validate()?;}
    if aperture.exclude.len()>64{return Err("Too many exclusion regions".into());}for shape in &aperture.exclude{shape.validate()?;}Ok(())
}
fn bounded_center(center:[f64;2],image:&FitsImage)->Result<[f64;2],String>{
    if image.spectrum.is_some() || image.info.width==0 || image.info.height==0{return Err("Open a two-dimensional image for aperture photometry.".into());}
    if center.iter().any(|v|!v.is_finite()){return Err("Aperture center must be finite".into());}
    Ok([center[0].clamp(0.,(image.info.width-1) as f64),center[1].clamp(0.,(image.info.height-1) as f64)])
}
fn shape_center(shape:&Shape)->[f64;2]{let bounds=shape.bounds();[(bounds[0]+bounds[2])/2.,(bounds[1]+bounds[3])/2.]}
fn translated_aperture(aperture:&Aperture,dx:f64,dy:f64)->Aperture{
    Aperture{name:aperture.name.clone(),shape:aperture.shape.translated(dx,dy),background:aperture.background.as_ref().map(|s|s.translated(dx,dy)),exclude:aperture.exclude.iter().map(|s|s.translated(dx,dy)).collect()}
}
fn fit_sky_ring(radius:f64,inner:f64,outer:f64)->Result<[f64;2],String>{
    if !inner.is_finite() || !outer.is_finite() || inner<=0. || outer<=inner || outer>MAX_SKY_RADIUS{return Err("Sky radii must satisfy 0 < inner < outer <= 10000 pixels".into());}
    let shift=(radius+0.1-inner).max(0.);let inner=inner+shift;let outer=(outer+shift).min(MAX_SKY_RADIUS);
    if outer-inner<0.099999{return Err("Leave at least 0.1 pixel between the source and sky boundaries".into());}
    Ok([inner,outer])
}
fn edited_circle(aperture:&Aperture,center:[f64;2],radius:f64,sky:Option<[f64;2]>,image:&FitsImage)->Result<Aperture,String>{
    let Shape::Circle{center:original,..}=&aperture.shape else{return Err("Select a circular aperture".into());};
    if !radius.is_finite() || !(MIN_RADIUS..=MAX_RADIUS).contains(&radius){return Err("Aperture radius must be between 0.1 and 9999.8 pixels".into());}
    let center=bounded_center(center,image)?;let mut aperture=translated_aperture(aperture,center[0]-original[0],center[1]-original[1]);
    aperture.shape=Shape::Circle{center,radius};
    if let Some([inner,outer])=sky{let [inner,outer]=fit_sky_ring(radius,inner,outer)?;aperture.background=Some(Shape::Ring{center,inner,outer});}
    else if let Some(Shape::Ring{center,inner,outer})=&aperture.background{let [inner,outer]=fit_sky_ring(radius,*inner,*outer)?;aperture.background=Some(Shape::Ring{center:*center,inner,outer});}
    validate_aperture(&aperture)?;Ok(aperture)
}
fn distance(a:[f64;2],b:[f64;2])->f64{(a[0]-b[0]).hypot(a[1]-b[1])}
fn edge_hit(aperture:&Aperture,p:[f64;2],zoom:f32)->Option<DragKind>{
    let Shape::Circle{center,radius}=&aperture.shape else{return None;};
    let mut edges=vec![((distance(p,*center)-radius).abs(),*radius,DragKind::Radius)];
    if let Some(Shape::Ring{center,inner,outer})=&aperture.background{let d=distance(p,*center);edges.push(((d-inner).abs(),*inner,DragKind::SkyInner));edges.push(((d-outer).abs(),*outer,DragKind::SkyOuter));}
    edges.into_iter().filter(|(delta,radius,_)|*delta<=(6./f64::from(zoom)).min(radius*0.3)).min_by(|a,b|a.0.total_cmp(&b.0)).map(|(_,_,kind)|kind)
}
fn pixel_position(rect:Rect,zoom:f32,pos:Pos2)->[f64;2]{[f64::from((pos.x-rect.left())/zoom)-0.5,f64::from((rect.bottom()-pos.y)/zoom)-0.5]}
fn draw_circle_handles(painter:&egui::Painter,transform:ImageTransform,center:[f64;2],radius:f64,color:Color32){
    if radius*f64::from(transform.zoom)<8.{return;}
    for offset in [[radius,0.],[-radius,0.],[0.,radius],[0.,-radius]]{let p=transform.pixel_to_screen([center[0]+offset[0],center[1]+offset[1]]);painter.rect_filled(Rect::from_center_size(p,egui::vec2(6.,6.)),1.,color);}
}
fn position(rect:Rect,zoom:f32,p:[f64;2])->Pos2{Pos2::new(rect.left()+(p[0] as f32+0.5)*zoom,rect.bottom()-(p[1] as f32+0.5)*zoom)}
pub fn draw_shape(painter:&egui::Painter,rect:Rect,zoom:f32,shape:&Shape,color:Color32){
    draw_shape_transformed(painter,ImageTransform::from_rect(rect,zoom),shape,color);
}
pub fn draw_shape_transformed(painter:&egui::Painter,transform:ImageTransform,shape:&Shape,color:Color32){
    let zoom=transform.zoom;
    let stroke=Stroke::new(1.3_f32,color);
    match shape {
        Shape::Circle{center,radius}=>{painter.circle_stroke(transform.pixel_to_screen(*center),*radius as f32*zoom,stroke);},
        Shape::Ring{center,inner,outer}=>for radius in [inner,outer]{painter.circle_stroke(transform.pixel_to_screen(*center),*radius as f32*zoom,stroke);},
        Shape::Polygon{points,holes}=>{let mut points=points.iter().map(|p|transform.pixel_to_screen(*p)).collect::<Vec<_>>();if let Some(first)=points.first().copied(){points.push(first);}painter.add(egui::Shape::line(points,stroke));for hole in holes{draw_shape_transformed(painter,transform,&Shape::Polygon{points:hole.clone(),holes:vec![]},color);}},
        Shape::Union{shapes}=>for shape in shapes{draw_shape_transformed(painter,transform,shape,color);},
        Shape::Path{..}=>{let flattened=shape.flattened();if let Shape::Path{points,width,..}=flattened{painter.add(egui::Shape::line(points.iter().map(|p|transform.pixel_to_screen(*p)).collect(),stroke));if let Some(p)=points.first(){painter.text(transform.pixel_to_screen(*p),egui::Align2::LEFT_BOTTOM,format!("width {width:0.2} px"),egui::FontId::proportional(10.),color);}}},
        Shape::Ellipse{center,radii,angle_deg}|Shape::Rectangle{center,size:radii,angle_deg}=>{
            let ellipse=matches!(shape,Shape::Ellipse{..});let count=if ellipse{64}else{4};let angle=angle_deg.to_radians();let mut points=Vec::new();
            for i in 0..count {let p=if ellipse{let t=i as f64/count as f64*std::f64::consts::TAU;[radii[0]*t.cos(),radii[1]*t.sin()]}else{let signs=[[-0.5,-0.5],[0.5,-0.5],[0.5,0.5],[-0.5,0.5]][i];[radii[0]*signs[0],radii[1]*signs[1]]};points.push([center[0]+p[0]*angle.cos()-p[1]*angle.sin(),center[1]+p[0]*angle.sin()+p[1]*angle.cos()]);}
            draw_shape_transformed(painter,transform,&Shape::Polygon{points,holes:vec![]},color);
        },
    }
}

fn header_unit(header:&str)->String{header.lines().find(|line|line.starts_with("BUNIT   =")).and_then(|line|line.split('\'').nth(1)).unwrap_or("").trim().into()}

#[cfg(test)]
mod measurement_input_tests{
    use super::*;
    use fitsviewer_core::{HduInfo,ImageInfo,RawPixels};

    // State/parameter numerical fixture only; no fabricated scientific image.
    // 仅检查状态及请求映射，不冒充真实科学图像验收。
    fn image(path:&str)->FitsImage{
        FitsImage{info:ImageInfo{path:path.into(),hdu:1,width:4,height:4,bitpix:-64,bscale:1.,bzero:0.,blank:None,
            hdus:vec![HduInfo{index:0,name:"PRIMARY".into(),axes:vec![],bitpix:8,viewable:false,kind:"Image".into()},
                HduInfo{index:1,name:"SCI".into(),axes:vec![4,4],bitpix:-64,viewable:true,kind:"Image".into()},
                HduInfo{index:2,name:"ERR".into(),axes:vec![4,4],bitpix:-64,viewable:true,kind:"Image".into()},
                HduInfo{index:3,name:"SCI".into(),axes:vec![4,4],bitpix:-64,viewable:true,kind:"Image".into()}],
            header:"EXTNAME = 'SCI'\nBUNIT   = 'COUNTS/S'".into(),spectrum:None},pixels:RawPixels::F64(vec![1.;16]),spectrum:None}
    }
    fn aperture(name:&str)->Aperture{Aperture{name:name.into(),shape:Shape::Circle{center:[1.,1.],radius:0.49},background:None,exclude:vec![]}}

    #[test]
    fn explicit_same_file_hdu_and_selected_indices_are_preserved(){
        let image=image("state-fixture.fits");let mut regions=Regions::default();regions.on_image(&image);
        regions.add_aperture(aperture("first"));regions.add_aperture(aperture("second"));
        assert_eq!(regions.error_kind,ErrorKind::Sigma);assert!(!regions.has_error_map());
        let selected=regions.measurement_params(&image,false);
        assert_eq!(selected["indices"],serde_json::json!([1]));assert_eq!(selected["apertures"][0]["name"],"second");
        assert_eq!(regions.measurement_params(&image,true)["indices"],serde_json::json!([0,1]));
        regions.error_same_file=true;assert!(regions.has_error_map());assert!(regions.measurement_params(&image,false).is_null());
        for hdu in [0,1,3,99]{regions.error_hdu=Some(hdu);assert!(regions.measurement_params(&image,false).is_null());}
        regions.error_hdu=Some(2);let params=regions.measurement_params(&image,false);
        assert_eq!(params["error_path"],image.info.path);assert_eq!(params["error_hdu"],2);assert_eq!(params["error_kind"],"sigma");
        assert_eq!(params["hdu"],1);assert_eq!(params["revision"],regions.revision);
        regions.error_kind=ErrorKind::Auto;assert!(regions.measurement_params(&image,false).is_null());
    }

    #[test]
    fn pending_measurements_defer_updates_without_duplicate_queueing(){
        let image=image("state-fixture.fits");let mut regions=Regions::default();regions.on_image(&image);regions.add_aperture(aperture("source"));
        regions.uncertainty_source=serde_json::json!({"hdu":3});regions.measure_selected(&image);assert!(regions.results[0].is_some());assert!(regions.uncertainty_source.is_null());
        regions.uncertainty_source=serde_json::json!({"hdu":3});regions.error_same_file=true;regions.error_hdu=Some(2);regions.changed();let revision=regions.revision;assert!(regions.uncertainty_source.is_null());
        assert!(regions.results[0].is_none());regions.measurement_pending=true;
        regions.measure_selected(&image);assert!(regions.needs_measure);assert!(!regions.need_batch_measure);
        regions.measurement_pending=false;regions.measure_selected(&image);assert!(regions.need_batch_measure);assert!(!regions.needs_measure);
        regions.measure_selected(&image);assert!(regions.need_batch_measure);assert_eq!(regions.revision,revision);
        regions.measurement_pending=true;regions.changed();assert!(regions.measurement_pending);assert!(regions.needs_measure);assert!(!regions.need_batch_measure);assert!(regions.revision>revision);
        regions.measurement_pending=false;regions.selected=None;regions.measure_selected(&image);assert!(!regions.need_batch_measure);assert!(!regions.needs_measure);
    }

    #[test]
    fn external_paths_are_literal_and_document_changes_clear_error_ownership(){
        let first=image("state-fixture.fits");let mut regions=Regions::default();regions.on_image(&first);regions.add_aperture(aperture("source"));
        regions.error_path="folder with spaces/观测 误差.fits".into();regions.error_kind=ErrorKind::Variance;
        let params=regions.measurement_params(&first,false);assert_eq!(params["error_path"],regions.error_path);assert!(params.get("error_hdu").is_none());assert_eq!(params["error_kind"],"variance");
        regions.error_same_file=true;regions.error_hdu=Some(2);regions.measurement_pending=true;regions.uncertainty_source=serde_json::json!({"hdu":3});let previous=regions.revision;
        regions.on_image(&image("another-source.fits"));assert!(regions.error_path.is_empty());assert!(regions.error_hdu.is_none());assert!(!regions.error_same_file);assert!(!regions.measurement_pending);assert!(!regions.need_batch_measure);assert!(regions.apertures.is_empty());assert!(regions.revision>previous);assert!(regions.uncertainty_source.is_null());
    }

    fn marker(flux:f64)->Measurement{Measurement{aperture_sum:flux,effective_area:1.,background_per_pixel:None,background_area:0.,net_flux:flux,total_error:None,snr:None,flags:vec![],method:"state marker only".into()}}
    fn drag_image()->FitsImage{let mut image=image("gesture-state-fixture.fits");image.info.width=64;image.info.height=64;image.pixels=RawPixels::F64(vec![1.;64*64]);image}
    fn circle(name:&str,center:[f64;2])->Aperture{Aperture{name:name.into(),shape:Shape::Circle{center,radius:5.},background:Some(Shape::Ring{center,inner:8.,outer:12.}),exclude:vec![Shape::Circle{center:[center[0]+1.,center[1]],radius:0.2}]}}

    #[test]
    fn edits_keep_other_results_and_move_background_and_exclusions_together(){
        let image=drag_image();let mut regions=Regions::default();regions.on_image(&image);
        let first=circle("untouched",[10.,10.]);regions.add_aperture(first.clone());regions.add_aperture(circle("edited",[20.,20.]));
        regions.results=vec![Some(marker(11.)),Some(marker(22.))];regions.growth=vec![(1.,marker(22.))];regions.need_growth=Some(vec![1.]);
        regions.need_batch_measure=true;regions.measurement_pending=true;let revision=regions.revision;
        regions.update_selected_circle([-100.,100.],9.,None,&image).unwrap();
        assert!(same_aperture(&regions.apertures[0],&first));assert_eq!(regions.results[0].as_ref().unwrap().net_flux,11.);assert!(regions.results[1].is_none());
        assert_eq!(regions.apertures[1].shape,Shape::Circle{center:[0.,63.],radius:9.});
        match regions.apertures[1].background.as_ref().unwrap(){Shape::Ring{center,inner,outer}=>{assert_eq!(*center,[0.,63.]);assert!((*inner-9.1).abs()<1e-10);assert!((outer-inner-4.).abs()<1e-10);},_=>panic!("expected ring")}
        assert_eq!(regions.apertures[1].exclude[0],Shape::Circle{center:[1.,63.],radius:0.2});
        assert!(regions.revision>revision);assert!(regions.measurement_pending);assert!(regions.needs_measure);assert!(!regions.need_batch_measure);assert!(regions.growth.is_empty());assert!(regions.need_growth.is_none());
        regions.resize_selected(2.,&image).unwrap();match regions.apertures[1].background.as_ref().unwrap(){Shape::Ring{inner,outer,..}=>{assert!((*inner-9.1).abs()<1e-10);assert!((*outer-13.1).abs()<1e-10);},_=>panic!("expected ring")}
    }

    #[test]
    fn deletion_preserves_result_indices_and_can_restore_geometry(){
        let image=drag_image();let mut regions=Regions::default();regions.on_image(&image);
        for name in ["first","middle","last"]{regions.add_aperture(aperture(name));}
        regions.results=vec![Some(marker(11.)),Some(marker(22.)),Some(marker(33.))];regions.select_aperture(1).unwrap();
        regions.measurement_pending=true;let revision=regions.revision;assert!(regions.delete_selected());
        assert_eq!(regions.apertures.iter().map(|a|a.name.as_str()).collect::<Vec<_>>(),vec!["first","last"]);
        assert_eq!(regions.results.iter().map(|r|r.as_ref().unwrap().net_flux).collect::<Vec<_>>(),vec![11.,33.]);
        assert_eq!(regions.selected,Some(1));assert!(!regions.needs_measure);assert!(regions.measurement_pending);assert!(regions.revision>revision);
        regions.undo();assert_eq!(regions.apertures[1].name,"middle");assert_eq!(regions.apertures.len(),3);assert!(regions.results.iter().all(Option::is_none));
        regions.redo();assert_eq!(regions.apertures.len(),2);regions.delete_selected();regions.delete_selected();assert!(regions.apertures.is_empty());assert!(regions.results.is_empty());assert_eq!(regions.selected,None);assert!(!regions.needs_measure);assert!(!regions.delete_selected());
    }

    #[test]
    fn drag_uses_press_snapshot_and_one_undo_step_in_pan_mode(){
        let image=drag_image();let mut regions=Regions::default();regions.on_image(&image);
        let first=circle("first",[20.,20.]);let second=circle("second",[45.,45.]);regions.add_aperture(first.clone());regions.add_aperture(second.clone());
        assert_eq!(regions.tool,Tool::Pan);assert_eq!(regions.hit_test([20.,20.],4.),Some((0,DragKind::Move)));
        let history=regions.history.len();regions.begin_drag(0,DragKind::Move,[20.,20.]);regions.update_drag([22.,21.],&image).unwrap();regions.update_drag([24.,22.],&image).unwrap();
        assert_eq!(shape_center(&regions.apertures[0].shape),[24.,22.]);assert!(same_aperture(&regions.apertures[1],&second));assert_eq!(regions.history.len(),history+1);assert!(!regions.needs_measure);
        assert_eq!(regions.apertures[0].background,first.background.as_ref().map(|s|s.translated(4.,2.)));assert_eq!(regions.apertures[0].exclude,first.exclude.iter().map(|s|s.translated(4.,2.)).collect::<Vec<_>>());
        regions.finish_drag();assert!(regions.needs_measure);regions.undo();assert!(same_aperture(&regions.apertures[0],&first));
    }

    #[test]
    fn circle_edge_resize_is_relative_and_sky_edges_stay_ordered(){
        let image=drag_image();let mut regions=Regions::default();regions.on_image(&image);regions.add_aperture(circle("source",[20.,20.]));
        assert_eq!(regions.hit_test([25.8,20.],4.),Some((0,DragKind::Radius)));
        assert_eq!(regions.hit_test([28.,20.],4.),Some((0,DragKind::SkyInner)));assert_eq!(regions.hit_test([32.,20.],4.),Some((0,DragKind::SkyOuter)));
        regions.begin_drag(0,DragKind::Radius,[25.8,20.]);regions.update_drag([26.8,20.],&image).unwrap();
        assert_eq!(regions.apertures[0].shape,Shape::Circle{center:[20.,20.],radius:6.});regions.finish_drag();
        regions.begin_drag(0,DragKind::SkyInner,[28.,20.]);regions.update_drag([100.,20.],&image).unwrap();regions.finish_drag();
        match regions.apertures[0].background.as_ref().unwrap(){Shape::Ring{inner,outer,..}=>{assert!(*inner>6.);assert!((*outer-*inner-0.1).abs()<1e-10);},_=>panic!("expected ring")}
        regions.draft.push([1.,1.]);assert_eq!(regions.hit_test([20.,20.],4.),None);
    }

    #[test]
    fn invalid_edit_is_atomic_and_empty_images_are_rejected(){
        let image=drag_image();let mut regions=Regions::default();regions.on_image(&image);let original=circle("source",[20.,20.]);regions.add_aperture(original.clone());let revision=regions.revision;let history=regions.history.len();
        assert!(regions.resize_selected(f64::NAN,&image).is_err());assert!(regions.resize_selected(-1.,&image).is_err());assert!(regions.move_selected([f64::INFINITY,0.],&image).is_err());
        assert!(regions.update_selected_circle([20.,20.],5.,Some([12.,8.]),&image).is_err());assert!(regions.select_aperture(99).is_err());
        let mut empty=image;empty.info.width=0;assert!(regions.move_selected([0.,0.],&empty).is_err());
        assert!(same_aperture(&regions.apertures[0],&original));assert_eq!(regions.revision,revision);assert_eq!(regions.history.len(),history);
    }

    #[test]
    fn primary_drag_claim_stays_with_the_initial_image_gesture(){
        let ctx=egui::Context::default();let image=drag_image();let mut regions=Regions::default();regions.on_image(&image);regions.add_aperture(circle("source",[20.,20.]));
        let rect=Rect::from_min_size(Pos2::ZERO,egui::vec2(256.,256.));let empty=position(rect,4.,[60.,60.]);
        let input=|events|egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(300.,300.))),events,..Default::default()};
        let press=|pos|egui::Event::PointerButton{pos,button:egui::PointerButton::Primary,pressed:true,modifiers:egui::Modifiers::NONE};
        let paint=|ctx:&egui::Context,regions:&mut Regions,rect|{egui::CentralPanel::default().show(ctx,|ui|{let (response,painter)=ui.allocate_painter(egui::vec2(256.,256.),egui::Sense::click_and_drag());regions.canvas(ctx,&response,&painter,rect,4.,&image,false);});};
        let _=ctx.run(input(vec![]),|ctx|paint(ctx,&mut regions,rect));
        let _=ctx.run(input(vec![egui::Event::PointerMoved(empty),press(empty)]),|ctx|{assert!(!regions.claims_primary_drag(ctx,rect,4.));paint(ctx,&mut regions,rect);});
        // A pan has moved the aperture under the original screen press point.
        // 图像平移后孔径经过原按下点，仍不得转为孔径拖动。
        let shifted=rect.translate(empty-position(rect,4.,[20.,20.]));
        let _=ctx.run(input(vec![egui::Event::PointerMoved(empty+egui::vec2(2.,2.))]),|ctx|{assert!(!regions.claims_primary_drag(ctx,shifted,4.));paint(ctx,&mut regions,shifted);});
        let _=ctx.run(input(vec![egui::Event::PointerButton{pos:empty,button:egui::PointerButton::Primary,pressed:false,modifiers:egui::Modifiers::NONE}]),|ctx|paint(ctx,&mut regions,rect));
        let center=position(rect,4.,[20.,20.]);
        let _=ctx.run(input(vec![egui::Event::PointerMoved(center),press(center)]),|ctx|{
            assert!(regions.claims_primary_drag(ctx,rect,4.));
            paint(ctx,&mut regions,rect);
        });
        assert!(regions.drag.is_some());
        let _=ctx.run(input(vec![egui::Event::PointerMoved(position(rect,4.,[24.,22.]))]),|ctx|{
            assert!(regions.claims_primary_drag(ctx,rect,4.));
            paint(ctx,&mut regions,rect);
        });
        assert_eq!(shape_center(&regions.apertures[0].shape),[24.,22.]);
    }

    #[test]
    fn delete_keys_do_not_remove_an_aperture_while_a_text_field_has_focus(){
        let ctx=egui::Context::default();let mut regions=Regions::default();regions.add_aperture(aperture("source"));
        let text_id=egui::Id::new("aperture-edit-test-text");let mut text=String::from("rename");
        let _=ctx.run(egui::RawInput::default(),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{ui.add(egui::TextEdit::singleline(&mut text).id(text_id)).request_focus();});});
        let key=|key|egui::RawInput{events:vec![egui::Event::Key{key,physical_key:None,pressed:true,repeat:false,modifiers:egui::Modifiers::NONE}],..Default::default()};
        for key_input in [egui::Key::Delete,egui::Key::Backspace]{let _=ctx.run(key(key_input),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{ui.add(egui::TextEdit::singleline(&mut text).id(text_id));});assert!(ctx.wants_keyboard_input());assert!(!regions.delete_shortcut(ctx));});assert_eq!(regions.apertures.len(),1);}
        ctx.memory_mut(|memory|memory.surrender_focus(text_id));
        let _=ctx.run(key(egui::Key::Delete),|ctx|{assert!(!ctx.wants_keyboard_input());assert!(regions.delete_shortcut(ctx));});assert!(regions.apertures.is_empty());
    }
}

