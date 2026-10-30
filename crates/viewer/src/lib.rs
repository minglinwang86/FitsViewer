use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use fitsviewer_core::{DisplayTone, ErrorKind, FitsImage, Limits, LoadOptions, Statistics, Stretch};
use std::sync::{Arc, Mutex};
#[cfg_attr(not(feature="pro"),path="spectrum_mini.rs")]
mod spectrum;
mod plot_geometry;
mod image_transform;
mod selection_ui;
mod image_display;
mod remote_image;
mod environment_ui;
#[cfg(feature="pro")]
mod spectral_lines;
#[cfg(feature="pro")]
mod spectral_analysis;
#[cfg(feature="pro")]
mod image_layers;
mod controls;
mod ui_style;
mod window_layout;
mod ribbon;
mod science_client;
mod regions_ui;
#[cfg(feature="pro")]
mod pro;
#[cfg(feature="pro")]
mod pro_forms;
#[cfg(feature="pro")]
mod pro_points;
#[cfg(feature="pro")]
mod multiview;
#[cfg(feature="pro")]
mod workspace_ui;
mod text_io;
mod i18n;
mod coordinates;
mod wcs_cache;
mod document_lifecycle;
mod document_tabs;
mod open_documents;
#[cfg(feature="offline")] mod offline_sources;
mod header_ui;

pub fn install_cjk_font(ctx: &egui::Context, bytes: Vec<u8>) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert("cjk".into(), egui::FontData::from_owned(bytes).into());
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts.families.entry(family).or_default().push("cjk".into());
    }
    ctx.set_fonts(fonts);
}

enum ImageLoad { Full(FitsImage), Remote(fitsviewer_core::remote::Manifest) }
pub type ImageBridge=Arc<dyn Fn(String)->Result<Vec<u8>,String>+Send+Sync>;
#[derive(Clone,serde::Serialize,serde::Deserialize)]
pub enum WindowEvent{Open{path:String,options:LoadOptions},Exit}
#[derive(Clone)]
pub struct WindowControl{
    pub poll:Arc<dyn Fn(bool)->Vec<WindowEvent>+Send+Sync>,
    pub exit_all:Arc<dyn Fn()->Result<(),String>+Send+Sync>,
}

#[cfg(all(test,not(target_arch="wasm32")))]mod native_spectrum_loading_tests{
    use super::*;
    #[test]fn native_spectra_keep_the_existing_full_dataset_reader(){
        let path=std::env::temp_dir().join(format!("fv-native-spectrum-{}-{}.dat",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::write(&path,b"4000 1\n5000 2\n6000 3\n").unwrap();
        let mut app=FitsApp::new(path.to_string_lossy().into_owned(),None,String::new(),None).with_image_bridge(Arc::new(|_|Err("Native full dataset reader".into())));
        app.request(&egui::Context::default(),None);
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(5);
        let result=loop{if let Some(result)=app.pending.lock().unwrap().1.take(){break result;}assert!(std::time::Instant::now()<deadline);std::thread::sleep(std::time::Duration::from_millis(5));};
        assert!(matches!(result,Ok(ImageLoad::Full(image)) if image.spectrum.is_some() && image.pixels.len()==3));std::fs::remove_file(path).unwrap();
    }
}
type Pending = Arc<Mutex<(u64,Option<Result<ImageLoad, String>>)>>;
/// The egui viewer shared by desktop and browser builds.
pub struct FitsApp {
    image: Option<Arc<FitsImage>>,
    selection:selection_ui::Selection,
    stats: Statistics,
    texture: Option<egui::TextureHandle>,
    remote: Option<remote_image::RemoteImage>,
    path: String,
    api: Option<String>,
    local_images:Option<ImageBridge>,
    pending_load_options:Option<LoadOptions>,
    window_control:Option<WindowControl>,
    token: String,
    pending: Pending,
    pending_group:Option<String>,
    loading: bool,
    first_frame:bool,
    error: String,
    limits: Limits,
    stretch: Stretch,
    scale_mapping:Option<fitsviewer_core::display::ScaleMapping>,
    stretch_histogram_builds:u64,
    low: f64,
    high: f64,
    invert: bool,
    tone: DisplayTone,
    dirty: bool,
    zoom: f32,
    pan: Vec2,
    rotation_deg:f32,
    auto_fit: bool,
    header_open: bool,
    cursor: String,
    requested_hdu: Option<usize>,
    load_options: LoadOptions,
    plot: spectrum::Plot,
    status_height: f32,
    ribbon_height: f32,
    ribbon_rect: [f32;4],
    ribbon_tabs: serde_json::Value,
    ribbon_task_rect: [f32;4],
    ribbon_layout: serde_json::Value,
    view_rect: [f32; 4],
    tools: controls::Tools,
    environment:environment_ui::Environment,
    popup_windows:window_layout::Runtime,
    science: science_client::Client,
    generation:u64,
    lifecycle:document_lifecycle::State,
    header:header_ui::Headers,
    cursor_pixel:Option<[f64;2]>,
    wcs_pixel:Option<[f64;2]>,
    wcs_text:String,
    wcs_pending:bool,
    wcs_disabled:bool,
    wcs_requested_at:f64,wcs_cache:wcs_cache::PixelCache,
    regions:regions_ui::Regions,
    last_export:std::cell::Cell<f64>,
    #[cfg(feature="pro")]
    pro:Option<pro::Workspace>,
    #[cfg(feature="pro")]
    pro_dock:Option<(bool,workspace_ui::Dock)>,
    wcs_result:Option<serde_json::Value>,
    coordinates:coordinates::Coordinates,
    wcs_override:Option<String>,
    #[cfg(feature="pro")]
    multi:Option<multiview::Workspace>,
    saved_multi:serde_json::Value,pending_view:Option<serde_json::Value>,
    documents:document_tabs::Tabs,pending_document:Option<document_tabs::Snapshot>,document_switching:bool,
    single_source:bool,
    pending_open:Option<(document_tabs::Destination,String)>,replace_rollback:Option<open_documents::Rollback>,
    #[cfg(feature="pro")] pending_layer:Option<(String,LoadOptions)>,
    #[cfg(feature="pro")] layer_remote:Option<remote_image::RemoteImage>,
    #[cfg(not(target_arch = "wasm32"))]
    file_picker: Option<fn() -> Option<std::path::PathBuf>>,
    #[cfg(not(target_arch = "wasm32"))]
    started: std::time::Instant,
}
impl FitsApp {
    /// A completed visible dataset frame, rather than a started process or metadata response.
    /// 确认已绘制数据首帧，而不是仅有进程或元数据。
    pub fn first_frame_ready(&self)->bool{self.first_frame && !self.loading}
    pub fn first_frame_error(&self)->Option<&str>{
        if self.loading || self.first_frame{return None;}
        if !self.error.is_empty(){return Some(&self.error);}
        self.remote.as_ref().filter(|remote|!remote.error.is_empty()).map(|remote|remote.error.as_str())
    }
    pub fn new(path: String, api: Option<String>, token: String, hdu: Option<usize>) -> Self {
        Self {
            image: None,
            selection:Default::default(),
            stats: Statistics::default(),
            texture: None,
            remote: None,
            path,
            science:science_client::Client::new(api.clone(),token.clone()),
            api,
            token,
            pending: Arc::new(Mutex::new((0,None))),
            pending_group:None,
            local_images:None,
            pending_load_options:None,
            window_control:None,
            loading: false,
            first_frame:false,
            error: String::new(),
            limits: Limits::ZScale,
            stretch: Stretch::Linear,
            scale_mapping:None,
            stretch_histogram_builds:0,
            low: 0.0,
            high: 1.0,
            invert: false,
            tone: DisplayTone::default(),
            dirty: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            rotation_deg:0.,
            auto_fit: true,
            header_open: false,
            cursor: String::new(),
            requested_hdu: hdu,
            load_options: LoadOptions::default(),
            plot: spectrum::Plot::default(),
            status_height: 0.,
            ribbon_height: 0.,
            ribbon_rect: [0.;4],
            ribbon_tabs: serde_json::Value::Null,
            ribbon_task_rect: [0.;4],
            ribbon_layout: serde_json::Value::Null,
            view_rect: [0.; 4],
            tools: controls::Tools::default(),
            environment:Default::default(),
            popup_windows:Default::default(),
            generation:0,lifecycle:Default::default(),header:Default::default(),cursor_pixel:None,wcs_pixel:None,wcs_text:String::new(),wcs_pending:false,wcs_disabled:false,wcs_requested_at:-100.,wcs_cache:Default::default(),
            regions:regions_ui::Regions::default(),last_export:std::cell::Cell::new(-100.),
            #[cfg(feature="pro")]
            pro:None,
            #[cfg(feature="pro")]
            pro_dock:None,
            wcs_result:None,
            coordinates:coordinates::Coordinates::default(),wcs_override:None,
            #[cfg(feature="pro")]
            multi:None,
            saved_multi:serde_json::Value::Null,pending_view:None,
            documents:Default::default(),pending_document:None,document_switching:false,
            single_source:false,
            pending_open:None,replace_rollback:None,
            #[cfg(feature="pro")] pending_layer:None,
            #[cfg(feature="pro")] layer_remote:None,
            #[cfg(not(target_arch = "wasm32"))]
            file_picker: None,
            #[cfg(not(target_arch = "wasm32"))]
            started: std::time::Instant::now(),
        }
    }
    pub fn from_image(image: FitsImage) -> Self {
        let mut app = Self::new(
            image.info.path.clone(),
            None,
            String::new(),
            Some(image.info.hdu),
        );
        app.accept(image);
        app
    }
    pub fn with_science(mut self,callback:science_client::LocalScience)->Self{self.science.local=Some(callback);self}
    pub fn with_image_bridge(mut self,callback:ImageBridge)->Self{self.local_images=Some(callback);self}
    pub fn with_load_options(mut self,options:LoadOptions)->Self{self.pending_load_options=Some(options);self}
    pub fn reuse_existing_window()->bool{controls::load().open_in_same_window}
    pub fn with_window_control(mut self,control:WindowControl)->Self{self.window_control=Some(control);self}
    pub fn with_single_source_scope(mut self)->Self{self.single_source=true;self}
    #[cfg(not(target_arch="wasm32"))]
    pub fn window_registry_path()->std::path::PathBuf{controls::settings_path().parent().unwrap_or(std::path::Path::new(".")).join("windows-v1")}
    fn exit_all_windows(&mut self,ctx:&egui::Context){
        #[cfg(not(target_arch="wasm32"))]{
            if let Some(control)=&self.window_control{if let Err(error)=(control.exit_all)(){self.error=error;return;}}
            self.close_document(ctx);ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        #[cfg(target_arch="wasm32")]{let _=crate::text_io::browser("exitAll",&[]);}
    }
    pub fn with_session(mut self,value:serde_json::Value)->Result<Self,String>{
        self.pending_load_options=None;
        let views=value["views"].as_array().filter(|v|!v.is_empty() && v.len()<=32).ok_or("Session requires one to 32 views")?;
        if self.single_source && views.len()>1{return Err("Remote viewing is limited to its current authorized source.".into());}
        if value["schema"].as_u64().unwrap_or(1)!=1 || value.to_string().len()>1024*1024{return Err("Unsupported or oversized session".into());}
        for entry in views{
            entry["path"].as_str().ok_or("Session view requires path")?;
            fitsviewer_core::view::View::parse(&entry["view"].to_string())?;
            let apertures:Vec<fitsviewer_core::regions::Aperture>=serde_json::from_value(entry.get("apertures").cloned().unwrap_or(serde_json::json!([]))).map_err(|e|e.to_string())?;
            if apertures.len()>64{return Err("Session exceeds 64 apertures per view".into());}for aperture in apertures{aperture.shape.validate()?;}
        }
        let selected=match value.get("selected").filter(|selected|!selected.is_null()) {
            None=>0,
            Some(selected)=>selected.as_u64().and_then(|selected|usize::try_from(selected).ok()).filter(|&selected|selected<views.len()).ok_or("Session selected view is out of range")?,
        };
        let mode=value["mode"].as_str().unwrap_or(if views.len()>1{"PRO"}else{"Minimal"});
        if !["Minimal","PRO"].contains(&mode){return Err("Unknown session mode".into());}
        #[cfg(not(feature="pro"))] if mode!="Minimal" || views.len()!=1 || views[0]["view"]["annotations"].as_array().is_some_and(|v|!v.is_empty()){return Err("PRO is not included in this Mini build".into());}
        #[cfg(not(feature="pro"))] for entry in views {if let Some(apertures)=entry["apertures"].as_array(){for aperture in apertures{mini_aperture(aperture)?;}}}
        self.tools.prefs.mode=mode.into();self.path=views[selected]["path"].as_str().unwrap().into();self.requested_hdu=views[selected]["hdu"].as_u64().map(|v|v as usize);
        self.pending_view=Some(serde_json::json!({"view":views[selected]["view"],"apertures":views[selected].get("apertures").cloned().unwrap_or(serde_json::json!([]))}));self.saved_multi=value;
        if let Some(image)=self.image.take().filter(|image|image.info.path==self.path && self.requested_hdu==Some(image.info.hdu)){self.accept_shared(image);}
        Ok(self)
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_file_picker(mut self, picker: fn() -> Option<std::path::PathBuf>) -> Self {
        self.file_picker = Some(picker);
        self
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn browse(&mut self, ctx: &egui::Context) {
        if self.loading {
            return;
        }
        if let Some(path) = self.file_picker.and_then(|picker| picker()) {
            if let Err(error)=self.open_path(ctx,path.to_string_lossy().into_owned(),LoadOptions::default(),None){self.error=error;}
        }
    }
    fn popup_context(&self)->Option<window_layout::Context>{
        if self.image.is_none(){Some(window_layout::Context::Empty)}
        else if cfg!(feature="pro") && self.tools.prefs.mode=="PRO" {Some(if self.image.as_ref().is_some_and(|image|image.spectrum.is_some()){window_layout::Context::ProSpectrum}else{window_layout::Context::ProImage})}
        else if self.image.as_ref().is_some_and(|image|image.spectrum.is_some()){Some(window_layout::Context::Spectrum)}
        else{Some(window_layout::Context::Image)}
    }
    fn popup_open_flags(&self)->[bool;6]{[self.tools.settings_open,self.tools.histogram_open,self.coordinates.open,self.regions.open,self.regions.growth_open,self.header_open]}
    fn prepare_popup_windows(&mut self){
        let context=self.popup_context();
        if let Some(open)=self.popup_windows.begin(&self.tools.prefs.window_layouts,context){
            [self.tools.settings_open,self.tools.histogram_open,self.coordinates.open,self.regions.open,self.regions.growth_open,self.header_open]=open;
        }
    }
    fn remember_popup_windows(&mut self){
        let context=self.popup_context();let open=self.popup_open_flags();self.popup_windows.finish(&mut self.tools.prefs.window_layouts,context,open);
    }
    fn accept(&mut self, image: FitsImage) {
        self.accept_shared(Arc::new(image));
    }
    fn cancel_image_load(&mut self){if let Some(group)=self.pending_group.take(){remote_image::cancel_group(&group);}
        #[cfg(feature="pro")] {self.pending_layer=None;self.layer_remote=None;}
    }
    fn accept_remote(&mut self,manifest:fitsviewer_core::remote::Manifest){
        let saved_view=self.pending_view.is_some();
        let query=remote_image::query(&self.path,LoadOptions{hdu:Some(manifest.info.hdu),..self.load_options});
        let image=FitsImage{info:manifest.info.clone(),pixels:fitsviewer_core::RawPixels::Unavailable,spectrum:None};
        self.accept_shared(Arc::new(image));
        self.stats=manifest.statistics.clone();
        if !saved_view{(self.low,self.high)=self.stats.bounds(self.limits);}
        let group=format!("{}-{}",self.science.session,self.generation);
        self.remote=Some(remote_image::RemoteImage::new(manifest,self.api.clone().unwrap_or_default(),self.token.clone(),query,group,self.local_images.clone(),self.tools.prefs.compact_large_images && self.tools.prefs.mode!="PRO"));
        self.dirty=true;
    }
    fn cache_current(&mut self){
        if self.document_switching{return;}
        self.remember_document();
        #[cfg(feature="pro")] {let view=self.view_configuration();
        if let (Some(multi),Some(image))=(&mut self.multi,&self.image){multi.remember(image.clone(),view,self.regions.apertures.clone());if let Some(remote)=&self.remote{multi.remember_preview(remote.preview_shared());}self.documents.trim_to(document_tabs::CACHE_BUDGET.saturating_sub(multi.bytes()));}}
    }
    fn accept_shared(&mut self, image: Arc<FitsImage>) {
        self.cache_current();
        self.remote=None;
        self.pending_group=None;
        self.first_frame=false;
        let was_spectrum=self.image.as_ref().is_some_and(|image|image.spectrum.is_some());
        if was_spectrum!=image.spectrum.is_some(){
            self.tools.prefs.ribbon_tab=if image.spectrum.is_some() && self.tools.prefs.mode=="PRO"{"Analysis"}else{"View"}.into();
            self.tools.prefs.ribbon_collapsed=false;
        }
        self.regions.on_image(&image);
        self.stats = Statistics::new(&image);
        (self.low, self.high) = self.stats.bounds(self.limits);
        self.path = image.info.path.clone();
        self.dirty = image.spectrum.is_none();
        if let Some(meta)=&image.info.spectrum{
            self.load_options.x_column=(!meta.columns.is_empty()).then_some(meta.x_column);self.load_options.y_column=(!meta.columns.is_empty()).then_some(meta.y_column);
            self.load_options.error_column=meta.error_column;self.load_options.error_hdu=meta.error_hdu;self.load_options.ignore_errors=!meta.has_error();
            if meta.has_error(){self.load_options.error_kind=meta.source_error_kind();}
        }
        self.image = Some(image);
        self.texture = None;
        self.scale_mapping=None;
        self.plot = spectrum::Plot::default();
        self.auto_fit = true;
        self.pan = Vec2::ZERO;
        self.rotation_deg=0.;
        self.error.clear();
        self.cursor.clear();
        self.tools.histogram=None;
        self.generation+=1;self.science.bind_document(self.generation);self.header=Default::default();self.cursor_pixel=None;self.wcs_pixel=None;self.wcs_text.clear();self.wcs_pending=false;self.wcs_disabled=false;self.wcs_result=None;
        self.wcs_override=None;self.regions.wcs_override=None;
        #[cfg(feature="pro")]
if let Some(pro)=&mut self.pro{pro.clear_document_results();}
        if let Some(snapshot)=self.pending_view.take(){
            self.rotation_deg=snapshot["view"]["rotation_deg"].as_f64().filter(|v|v.is_finite()).unwrap_or(0.) as f32;
            if let Ok(view)=fitsviewer_core::view::View::parse(&snapshot["view"].to_string()){
                self.low=view.low;self.high=view.high;self.stretch=Stretch::parse(&view.stretch).unwrap_or(Stretch::Linear);self.tone=DisplayTone{bias:view.bias,contrast:view.contrast};self.invert=view.invert;self.zoom=view.zoom as f32;self.pan=Vec2::new(view.pan[0] as f32,view.pan[1] as f32);self.tools.prefs.render=view.render;self.auto_fit=false;
            }
            if let Ok(apertures)=serde_json::from_value(snapshot["apertures"].clone()){self.regions.replace_apertures(apertures);}
            if let Ok(bounds)=serde_json::from_value::<[f64;4]>(snapshot["view"]["plot_bounds"].clone()){if bounds.iter().all(|n|n.is_finite()) && bounds[0]<bounds[1] && bounds[2]<bounds[3]{self.plot.bounds=bounds;self.plot.fit=false;}}
            #[cfg(feature="pro")]
if let Some(pro)=&mut self.pro{pro.annotations=snapshot["view"]["annotations"].as_array().cloned().unwrap_or_default();}
            self.wcs_override=snapshot["view"]["wcs_override"].as_str().filter(|s|s.len()<=65536).map(str::to_string);
        }
        self.restore_document();
        self.cache_current();
    }
    fn request(&mut self, ctx: &egui::Context, hdu: Option<usize>) {
        if self.path.trim().is_empty() {
            return;
        }
        self.selection.cancel();
        self.cancel_image_load();
        let preserve=self.path.clone();self.abandon_document(ctx,Some(&preserve));
        self.loading = true;
        self.first_frame=false;
        self.error.clear();
        self.requested_hdu = hdu;
        if self.image.as_ref().is_none_or(|image| image.info.path != self.path || hdu != Some(image.info.hdu)) {
            self.load_options = LoadOptions::default();
        }
        if let Some(value)=self.pending_view.as_ref().map(|v|&v["view"]["load_options"]).filter(|v|v.is_object()){
            self.load_options=LoadOptions{hdu,x_column:value["x_column"].as_u64().map(|v|v as usize),y_column:value["y_column"].as_u64().map(|v|v as usize),error_column:value["error_column"].as_u64().map(|v|v as usize),error_hdu:value["error_hdu"].as_u64().map(|v|v as usize),error_kind:ErrorKind::parse(value["error_kind"].as_str().unwrap_or("auto")).unwrap_or_default(),ignore_errors:value["no_error"].as_bool().unwrap_or(false)};
        }
        if let Some(options)=self.pending_load_options.take(){self.load_options=options;}
        let options=LoadOptions{hdu,..self.load_options};self.load_dataset(ctx,self.path.clone(),options);
    }
    fn load_dataset(&mut self,ctx:&egui::Context,path:String,options:LoadOptions){
        let pending = self.pending.clone();
        let serial={let mut slot=pending.lock().unwrap();slot.0+=1;slot.1=None;slot.0};
        let group=format!("load-{}-{serial}",self.science.session);self.pending_group=Some(group.clone());
        let ctx = ctx.clone();
        if let Some(base) = self.api.as_deref().or(self.local_images.as_ref().map(|_|"")) {
            let query=format!("{}&session={}&generation={}&fv_image={}",remote_image::query(&path,options),urlencoding::encode(&self.science.session),self.generation,urlencoding::encode(&group));
            let url=format!("{base}/api/image/metadata?{query}");let base=base.to_owned();let token=self.token.clone();let local=self.local_images.clone();let native_path=path.clone();
            let busy=pending.clone();let busy_ctx=ctx.clone();
            if !remote_image::get(url,&self.token,2*1024*1024,local.clone(),move|response|{
                if pending.lock().unwrap().0!=serial{return;}
                #[cfg(not(target_arch="wasm32"))]
                if local.is_some() && response.as_ref().is_err_and(|error|error=="Native full dataset reader"){
                    let literal=std::path::Path::new(&native_path);let path=if literal.exists(){literal}else{std::path::Path::new(fitsviewer_core::pasted_path(&native_path))};
                    let result=fitsviewer_core::load_file_with_options(path,options).map(ImageLoad::Full);
                    let mut slot=pending.lock().unwrap();if slot.0==serial{slot.1=Some(result);}drop(slot);ctx.request_repaint();return;
                }
                #[cfg(target_arch="wasm32")]let _=&native_path;
                let manifest=response.and_then(|bytes|serde_json::from_slice::<fitsviewer_core::remote::Manifest>(&bytes).map_err(|e|e.to_string()));
                match manifest{
                    Ok(manifest) if manifest.transport=="full"=>{
                        let url=format!("{base}/api/image/full?{query}&version={}",urlencoding::encode(&manifest.source_version));
                        let busy=pending.clone();let busy_ctx=ctx.clone();
                        if !remote_image::get(url,&token,fitsviewer_core::remote::FULL_TRANSFER_BYTES as usize,local,move|response|{
                            let result=response.and_then(|bytes|FitsImage::from_packet(&bytes)).map(ImageLoad::Full);
                            let mut slot=pending.lock().unwrap();if slot.0==serial{slot.1=Some(result);}ctx.request_repaint();
                        }){let mut slot=busy.lock().unwrap();if slot.0==serial{slot.1=Some(Err("Too many image requests; retry opening the file".into()));}busy_ctx.request_repaint();}
                    },
                    result=>{let result=result.and_then(|manifest|if manifest.schema==1 && manifest.transport=="tiles" && manifest.info.spectrum.is_none(){Ok(ImageLoad::Remote(manifest))}else{Err("Unsupported remote image protocol".into())});let mut slot=pending.lock().unwrap();if slot.0==serial{slot.1=Some(result);}ctx.request_repaint();},
                }
            }){let mut slot=busy.lock().unwrap();if slot.0==serial{slot.1=Some(Err("Too many image requests; retry opening the file".into()));}busy_ctx.request_repaint();}
        } else {
            #[cfg(all(target_arch="wasm32",feature="offline"))]
            {let result=WEB_OFFLINE_SOURCE.with(|slot|slot.borrow_mut().load(fitsviewer_core::pasted_path(&path),options)).map(ImageLoad::Full);let mut slot=pending.lock().unwrap();if slot.0==serial{slot.1=Some(result);}ctx.request_repaint();}
            #[cfg(not(target_arch = "wasm32"))]
            {
                let path = path.clone();
                std::thread::spawn(move || {
                    let literal = std::path::Path::new(&path);
                    let path = if literal.exists() {literal} else {std::path::Path::new(fitsviewer_core::pasted_path(&path))};
                    let result = fitsviewer_core::load_file_with_options(path, options).map(ImageLoad::Full);
                    let mut slot=pending.lock().unwrap();if slot.0==serial{slot.1=Some(result);}
                    ctx.request_repaint();
                });
            }
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        if self.image.as_ref().is_some_and(|image| image.spectrum.is_some()) {
            ctx.input(|i| {if i.key_pressed(egui::Key::F) {self.plot.fit=true;}});
            return;
        }
        ctx.input(|i| {
            for (key, stretch) in [
                (egui::Key::Num1, Stretch::Linear),
                (egui::Key::Num2, Stretch::Log),
                (egui::Key::Num3, Stretch::Sqrt),
                (egui::Key::Num4, Stretch::Asinh),
                (egui::Key::Num5, Stretch::Power),
                (egui::Key::Num6, Stretch::Squared),
                (egui::Key::Num7, Stretch::Sinh),
                (egui::Key::Num8, Stretch::HistogramEqualization),
            ] {
                if i.key_pressed(key) {
                    self.stretch = stretch;
                    self.dirty = true;
                }
            }
            if i.key_pressed(egui::Key::F) {
                self.auto_fit = true;
                self.pan = Vec2::ZERO;
            }
            if i.key_pressed(egui::Key::Z) {
                self.limits = Limits::ZScale;
                (self.low, self.high) = self.stats.bounds(self.limits);
                self.dirty = true;
            }
            if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                self.zoom *= 1.25;
                self.pan *= 1.25;
                self.auto_fit = false;
            }
            if i.key_pressed(egui::Key::Minus) {
                self.zoom /= 1.25;
                self.pan /= 1.25;
                self.auto_fit = false;
            }
            if i.key_pressed(egui::Key::I) {
                self.invert = !self.invert;
                self.dirty = true;
            }
            if i.key_pressed(egui::Key::R) {
                self.tone = DisplayTone::default();
                self.dirty = true;
            }
            if i.key_pressed(egui::Key::PageDown) || i.key_pressed(egui::Key::PageUp) {
                let chosen = self.image.as_ref().and_then(|image| {
                    let valid: Vec<usize> = image
                        .info
                        .hdus
                        .iter()
                        .filter(|h| h.viewable)
                        .map(|h| h.index)
                        .collect();
                    let position = valid.iter().position(|&h| h == image.info.hdu)?;
                    let next = if i.key_pressed(egui::Key::PageDown) {
                        (position + 1) % valid.len()
                    } else {
                        (position + valid.len() - 1) % valid.len()
                    };
                    Some(valid[next])
                });
                if let Some(hdu) = chosen {
                    self.request(ctx, Some(hdu));
                }
            }
        });
    }
    #[cfg(feature="pro")]
    fn sync_pro(&mut self,ctx:&egui::Context){
        let is_pro=self.tools.prefs.mode=="PRO";
        // An empty PRO window retains its mode without recreating a released owner.
        // 空白 PRO 窗口保留模式；关闭文件后不重新创建已释放的后台会话。
        if is_pro && self.image.is_some() && self.pro.is_none(){
            let mut pro=pro::Workspace::default();if let (Some(views),Some(image))=(self.saved_multi["views"].as_array(),self.image.as_ref()){if let Some(view)=views.iter().find(|v|v["path"]==image.info.path && v["hdu"].as_u64()==Some(image.info.hdu as u64)){pro.annotations=view["view"]["annotations"].as_array().cloned().unwrap_or_default();}}
            self.pro=Some(pro);self.multi=Some(multiview::Workspace::from_json(&self.saved_multi));self.cache_current();self.science.request(ctx,self.generation,"pro_acquire",serde_json::json!({}));
        }
        if (!is_pro || self.image.is_none() && !self.loading) && self.pro.is_some(){self.cache_current();self.pro=None;self.pro_dock=None;if let Some(multi)=self.multi.take(){self.saved_multi=multi.configuration();}self.science.request(ctx,self.generation,"release_pro",serde_json::json!({}));}
    }
    #[cfg(feature="pro")]
    fn prepare_pro_dock(&mut self,ctx:&egui::Context,force:bool){
        use workspace_ui::{Dock,Page};
        self.sync_pro(ctx);if self.pro.is_none(){return;}
        let image=self.image.clone();let spectrum=image.as_ref().is_some_and(|image|image.spectrum.is_some());
        if !force && self.pro_dock.as_ref().is_some_and(|(context,_)|*context==spectrum){return;}
        let keep_open=!force && self.pro_dock.is_none() && self.tools.prefs.pro_layout(spectrum).is_null();
        let dock=Dock::restore(self.tools.prefs.pro_layout(spectrum));
        let pro=self.pro.as_mut().unwrap();pro.sync_spectrum(image.as_deref());
        if !keep_open {
            self.coordinates.open=!spectrum && dock.was_open(Page::Coordinates);
            self.regions.open=!spectrum && dock.was_open(Page::Photometry);
            self.regions.growth_open=!spectrum && dock.was_open(Page::Growth);
            pro.preview_open=dock.was_open(Page::Preview);
            self.multi.as_mut().unwrap().set_files_open(dock.was_open(Page::Files));
            self.tools.histogram_open=!spectrum && dock.was_open(Page::Histogram);
            // Settings and Header are independent base windows, never dock pages.
        }
        let (operation,task)=dock.operation().split_once(':').unwrap_or((dock.operation(),""));
        if !pro.busy && pro::OPERATIONS.iter().any(|(candidate,_)|*candidate==operation) && (if spectrum{pro::spectrum_operation(operation)}else{!["spectral_candidates","spectral_analysis"].contains(&operation)}){
            pro.select(operation);if let Some(analysis)=&mut pro.spectral_analysis{if ["Display","Measure","Process","Fit","Compare"].contains(&task){analysis.task=task.into();}}
        }
        self.pro_dock=Some((spectrum,dock));
    }
    #[cfg(feature="pro")]
    fn pro_workspace(&mut self,ctx:&egui::Context,view:&serde_json::Value,available:bool){
        use workspace_ui::Page;
        self.prepare_pro_dock(ctx,false);if self.pro.is_none(){return;}
        let image=self.image.clone();let spectrum=image.as_ref().is_some_and(|image|image.spectrum.is_some());
        let pro=self.pro.as_mut().unwrap();let multi=self.multi.get_or_insert_with(multiview::Workspace::default);
        pro.tick(ctx,&self.science,self.generation,image.as_deref(),view,multi);
        let (_,mut dock)=self.pro_dock.take().unwrap();let mut requested=vec![Page::Tool];
        if self.coordinates.open && !spectrum{requested.push(Page::Coordinates);}
        if self.regions.open && !spectrum{requested.push(Page::Photometry);}
        if self.regions.growth_open && !spectrum{requested.push(Page::Growth);}
        if self.pro.as_ref().is_some_and(|pro|pro.preview_open){requested.push(Page::Preview);}
        if self.multi.as_ref().is_some_and(|multi|multi.files_open()){requested.push(Page::Files);}
        if !spectrum && (self.multi.as_ref().is_some_and(|multi|multi.secondary_open()) || dock.was_open(Page::Secondary)){requested.push(Page::Secondary);}
        if self.tools.histogram_open && !spectrum{requested.push(Page::Histogram);}
        let pro=self.pro.as_ref().unwrap();let operation=format!("{}:{}",pro.operation,if pro.operation=="spectral_analysis"{pro.spectral_analysis.as_ref().map_or("",|analysis|analysis.task.as_str())}else{""});
        let width=self.tools.prefs.pro_width(spectrum);let collapsed=self.tools.prefs.pro_collapsed(spectrum);let saved_layout=self.tools.prefs.pro_layout(spectrum).clone();
        let result=dock.show(ctx,spectrum,&requested,&operation,width,collapsed,|ui,page|match page{
            Page::Tool=>{let pro=self.pro.as_mut().unwrap();let multi=self.multi.as_mut().unwrap();pro.ui(ui,ctx,&self.science,self.generation,image.as_deref(),view,multi);},
            Page::Coordinates=>self.coordinates.ui(ui,ctx,&self.science,self.generation,image.as_deref(),self.wcs_result.as_ref(),self.cursor_pixel,available),
            Page::Photometry=>self.regions.ui(ui,ctx,image.as_deref(),true),
            Page::Growth=>self.regions.growth_ui(ui,ctx),
            Page::Preview=>self.pro.as_mut().unwrap().preview_ui(ui),
            Page::Files=>self.multi.as_mut().unwrap().files_ui(ui),
            Page::Secondary=>{if let Some(image)=image.as_deref(){let center=image_transform::ImageTransform::new(Pos2::ZERO,egui::vec2(image.info.width as f32,image.info.height as f32),self.zoom,self.rotation_deg).screen_to_pixel(Pos2::ZERO-self.pan);self.multi.as_mut().unwrap().secondary_ui(ui,ctx,&self.science,self.generation,image,center,self.zoom,self.wcs_override.as_deref());}},
            Page::Histogram=>self.tools.histogram_ui(ui,image.as_deref(),self.low,self.high),
            Page::Settings=>self.tools.settings_ui(ui,ctx,self.stats.max),
            Page::Header=>self.header_ui(ui,ctx),
        });
        if self.tools.prefs.pro_layout(spectrum)!=&saved_layout{self.pro_dock=None;self.prepare_pro_dock(ctx,true);return;}
        for page in result.closed{match page {
            Page::Coordinates=>self.coordinates.open=false,Page::Photometry=>self.regions.open=false,Page::Growth=>self.regions.growth_open=false,Page::Preview=>self.pro.as_mut().unwrap().preview_open=false,Page::Files=>self.multi.as_mut().unwrap().set_files_open(false),
            Page::Secondary=>self.multi.as_mut().unwrap().close_secondary(),
            Page::Histogram=>self.tools.histogram_open=false,Page::Settings=>self.tools.settings_open=false,Page::Header=>self.header_open=false,Page::Tool=>{},
        }}
        if !result.collapsed{self.tools.prefs.set_pro_width(spectrum,result.width);}
        self.tools.prefs.set_pro_collapsed(spectrum,result.collapsed);self.pro.as_mut().unwrap().panel_width=result.width;
        // Persist after a completed gesture, not once per pointer-motion frame.
        // 手势完成后保存，避免拖动每一帧写入偏好；科学任务本身不序列化。
        let pro=self.pro.as_ref().unwrap();dock.remember_operation(format!("{}:{}",pro.operation,if pro.operation=="spectral_analysis"{pro.spectral_analysis.as_ref().map_or("",|analysis|analysis.task.as_str())}else{""}));
        if !ctx.input(|input|input.pointer.primary_down()){self.tools.prefs.set_pro_layout(spectrum,dock.summary());}
        self.pro_dock=Some((spectrum,dock));
    }
    fn action(&mut self, action:&str, ctx:&egui::Context){
        match action {
            "Auto display"=>{if self.image.as_ref().is_some_and(|image|image.spectrum.is_none()){if let Some(preset)=controls::auto_display(&self.stats){self.limits=Limits::Manual;self.low=preset.low;self.high=preset.high;self.stretch=preset.stretch;self.tone=DisplayTone::default();self.dirty=true;}}},
            "Fit"=>{self.auto_fit=true;self.pan=Vec2::ZERO;self.plot.fit=true;},
            "1:1"=>{self.auto_fit=false;self.zoom=1./ctx.pixels_per_point();self.pan=Vec2::ZERO;},
            "Zoom +"=>{self.auto_fit=false;self.zoom*=1.25;self.pan*=1.25;},
            "Zoom -"=>{self.auto_fit=false;self.zoom/=1.25;self.pan/=1.25;},
            "ZScale"=>{self.limits=Limits::ZScale;(self.low,self.high)=self.stats.bounds(self.limits);self.dirty=true;},
            "Min/Max"=>{self.limits=Limits::MinMax;(self.low,self.high)=self.stats.bounds(self.limits);self.dirty=true;},
            "Reset B/C"=>{self.tone=DisplayTone::default();self.dirty=true;},
            "Invert"=>{self.invert=!self.invert;self.dirty=true;},
            "Header"=>self.header_open=!self.header_open,
            _=>{},
        }
    }
    fn science_updates(&mut self,ctx:&egui::Context){
        for reply in self.science.drain(){
            if reply.operation.starts_with("environment."){self.environment.reply(reply);continue;}
            if reply.generation!=self.generation {continue;}
            if ["view_heartbeat","artifact_refs","release_document","release_view"].contains(&reply.operation.as_str()){continue;}
            if let Some(hdu)=reply.operation.strip_prefix("header.").and_then(|h|h.parse::<usize>().ok()){self.header.accept(hdu,reply.result);continue;}
            if reply.operation.starts_with("gaia.") {
                self.coordinates.reply(ctx,&self.science,self.generation,reply);
            }else if reply.operation.starts_with("wcs.") {
                let Some(requested_pixel)=self.wcs_cache.pending_pixel(&reply.operation) else{continue;};
                match reply.result {
                    Ok(value)=>{
                        if let Err(error)=self.wcs_cache.accept(&reply.operation,value){self.wcs_text=format!("J2000 unavailable: {error}");self.wcs_result=None;self.wcs_pixel=Some(requested_pixel);}
                        // Read the current cursor, never flash the position of a late reply.
                        // 迟到的像素结果只进缓存，不覆盖新光标读数。
                        if let Some(pixel)=self.wcs_target(){self.show_cached_wcs(pixel);}
                    },
                    Err(error)=>{
                        self.wcs_cache.fail(&reply.operation);self.wcs_text=format!("J2000 unavailable: {error}");self.wcs_result=None;self.wcs_pixel=Some(requested_pixel);
                        self.wcs_disabled=error.contains("No usable celestial WCS") || error.contains("No module named") || error.contains("Python environment is unavailable");
                    },
                }
                self.wcs_pending=self.wcs_cache.is_pending();ctx.request_repaint();
            }else if reply.operation=="region_transform" {
                match reply.result {Ok(value)=>self.regions.accept_sky_template(value),Err(error)=>self.regions.message=error,}
            }else if reply.operation=="view_link" {
                #[cfg(feature="pro")]
                if let Some(multi)=&mut self.multi {match reply.result {Ok(value)=>{multi.linked_center=value["center"].as_array().and_then(|a|Some([a.first()?.as_f64()?,a.get(1)?.as_f64()?]));multi.linked_ratio=value["ratio"].as_f64().unwrap_or(1.);},Err(error)=>multi.message=error,}}
            }else if reply.operation=="measure.growth" {
                self.regions.measurement_pending=false;
                match reply.result{Ok(value)=>{if value["revision"].as_u64()==Some(self.regions.revision){self.regions.accept_growth(value);}},Err(error)=>self.regions.message=error,}
            }else if reply.operation=="measure" {
                self.regions.measurement_pending=false;
                match reply.result{Ok(value)=>{
                    if value["revision"].as_u64()!=Some(self.regions.revision){continue;}
                    if let (Ok(results),Ok(indices))=(serde_json::from_value::<Vec<fitsviewer_core::regions::Measurement>>(value["measurements"].clone()),serde_json::from_value::<Vec<usize>>(value["indices"].clone())){
                        self.regions.uncertainty_source=value["uncertainty_source"].clone();
                        self.regions.results.resize(self.regions.apertures.len(),None);for (index,result) in indices.into_iter().zip(results){if let Some(slot)=self.regions.results.get_mut(index){*slot=Some(result);}}
                        self.regions.message.clear();
                    }
                },Err(error)=>self.regions.message=error,}
            }else {
                #[cfg(feature="pro")] #[cfg(feature="pro")]
if let Some(pro)=&mut self.pro {pro.reply(reply);}
            }
        }
    }
    fn settings_window(&mut self,ctx:&egui::Context){
        self.environment.tick(ctx,&self.science,self.generation);
        let mut open=self.tools.settings_open;
        let mut windows=std::mem::take(&mut self.popup_windows);
        windows.show(ctx,window_layout::Key::Settings,crate::i18n::t("Settings"),&mut open,egui::vec2(480.,580.),|ui|{
            #[cfg(not(all(target_arch="wasm32",feature="offline")))]
            egui::CollapsingHeader::new(crate::i18n::t("Scientific Python environment")).id_salt("settings-environment").default_open(true).show(ui,|ui|self.environment.ui(ui,ctx,&self.science,self.generation));
            #[cfg(all(target_arch="wasm32",feature="offline"))]
            egui::CollapsingHeader::new(crate::i18n::t("Scientific Python environment")).id_salt("settings-environment").default_open(true).show(ui,|ui|{ui.weak("Offline viewer: external Python is unavailable.");});
            ui.separator();ui.strong(crate::i18n::t("Image display / interface"));self.tools.settings_ui(ui,ctx,self.stats.max);
        });
        self.popup_windows=windows;self.tools.settings_open=open;
    }
    fn view_configuration(&self)->serde_json::Value{
        #[cfg(feature="pro")] let annotations=self.pro.as_ref().map(|p|p.annotations.clone()).unwrap_or_default();
        #[cfg(not(feature="pro"))] let annotations:Vec<serde_json::Value>=vec![];
        let crop=self.image.as_ref().filter(|i|i.spectrum.is_none()).and_then(|image|{
            let viewport=Rect::from_min_max(egui::pos2(self.view_rect[0],self.view_rect[1]),egui::pos2(self.view_rect[2],self.view_rect[3]));
            let transform=image_transform::ImageTransform::new(viewport.center()+self.pan,egui::vec2(image.info.width as f32,image.info.height as f32),self.zoom,self.rotation_deg);
            let bounds=transform.visible_roi(viewport);
            (viewport.is_positive() && bounds[0]<bounds[2] && bounds[1]<bounds[3]).then_some(bounds)
        });
        let viewport_polygon=self.image.as_ref().filter(|image|image.spectrum.is_none()).map(|image|{
            let viewport=Rect::from_min_max(egui::pos2(self.view_rect[0],self.view_rect[1]),egui::pos2(self.view_rect[2],self.view_rect[3]));
            let transform=image_transform::ImageTransform::new(viewport.center()+self.pan,egui::vec2(image.info.width as f32,image.info.height as f32),self.zoom,self.rotation_deg);
            [viewport.left_top(),viewport.right_top(),viewport.right_bottom(),viewport.left_bottom()].map(|point|transform.screen_to_pixel(point))
        });
        serde_json::json!({"schema":1,"path":self.image.as_ref().map_or(self.path.as_str(),|i|i.info.path.as_str()),"hdu":self.image.as_ref().map(|i|i.info.hdu),"low":self.low,"high":self.high,"stretch":self.stretch.name(),"invert":self.invert,"bias":self.tone.bias,"contrast":self.tone.contrast,"zoom":self.zoom,"rotation_deg":self.rotation_deg,"pan":[self.pan.x,self.pan.y],"crop":crop,"viewport_polygon":viewport_polygon,"viewport_size":[self.view_rect[2]-self.view_rect[0],self.view_rect[3]-self.view_rect[1]],"render":self.tools.prefs.render,"plot_bounds":self.plot.bounds,"apertures":self.regions.apertures,"selected_aperture":self.regions.selected,"arcsec_per_pixel":image_display::angular_scale(self.wcs_result.as_ref()),"load_options":{"x_column":self.load_options.x_column,"y_column":self.load_options.y_column,"error_column":self.load_options.error_column,"error_hdu":self.load_options.error_hdu,"error_kind":self.load_options.error_kind.as_str(),"no_error":self.load_options.ignore_errors},"annotations":annotations,"sky_position":self.wcs_result.as_ref().and_then(|v|v["points"].as_array()).and_then(|v|v.first()),"wcs_override":self.wcs_override})
    }
    fn state(&self)->serde_json::Value{
        let image=self.image.as_deref();
        let mut state=serde_json::json!({"schema":1,"loaded":image.is_some(),"loading":self.loading,"error":self.error,"path":self.path,"hdu":image.map(|m|m.info.hdu),"width":image.map(|m|m.info.width),"height":image.map(|m|m.info.height),"spectrum":image.and_then(|m|m.info.spectrum.as_ref()),"plot_bounds":self.plot.bounds,"status_height":self.status_height,"view_rect":self.view_rect,"stretch":self.stretch.name(),"limits":self.limits.name(),"low":self.low,"high":self.high,"bias":self.tone.bias,"contrast":self.tone.contrast,"zoom":self.zoom,"rotation_deg":self.rotation_deg,"pan":[self.pan.x,self.pan.y],"cursor":self.cursor,"texture":self.texture.is_some(),"mode":self.tools.prefs.mode,"theme":self.tools.prefs.theme,"preferences":self.tools.prefs,"histogram_builds":self.tools.histogram_builds,"histogram_loaded":self.tools.histogram.is_some(),"science_session":self.science.session,"generation":self.generation,"j2000":self.wcs_text,"wcs":self.wcs_result,"apertures":self.regions.apertures,"measurements":self.regions.results,"region_message":self.regions.message,"measurement_pending":self.regions.measurement_pending,"measurement_revision":self.regions.revision});
        state["documents"]=self.documents.summary();
        state["documents"]["single_source_scope"]=serde_json::json!(self.single_source);
        #[cfg(feature="pro")] {state["documents"]["combined_cache_bytes_upper_bound"]=serde_json::json!(self.documents.inactive_bytes()+self.multi.as_ref().map_or(0,|m|m.bytes()));}
        #[cfg(not(feature="pro"))] {state["documents"]["combined_cache_bytes_upper_bound"]=serde_json::json!(self.documents.inactive_bytes());}
        state["documents"]["active_dataset_pinned_bytes"]=serde_json::json!(self.image.as_ref().map_or(0,|i|document_tabs::dataset_bytes(i)));
        #[cfg(all(target_arch="wasm32",feature="offline"))] {state["documents"]["offline_source_bytes"]=WEB_OFFLINE_SOURCE.with(|slot|serde_json::json!(slot.borrow().bytes()));state["documents"]["offline_source_budget"]=serde_json::json!(offline_sources::BYTE_BUDGET);}
        state["selection"]=self.selection.state();
        state["histogram_controls"]=self.tools.histogram_controls.clone();
        state["ribbon_height"]=serde_json::json!(self.ribbon_height);
        state["ribbon_rect"]=serde_json::json!(self.ribbon_rect);
        state["ribbon_tabs"]=self.ribbon_tabs.clone();
        state["ribbon_task_rect"]=serde_json::json!(self.ribbon_task_rect);
        state["ribbon_layout"]=self.ribbon_layout.clone();
        state["first_frame_ready"]=serde_json::json!(self.first_frame_ready());
        state["remote_image"]=self.remote.as_ref().map_or(serde_json::Value::Null,|remote|remote.state());
        state["wcs_cache"]=self.wcs_cache.summary();
        state["wcs_pixel"]=serde_json::json!(self.wcs_pixel);
        state["cursor_pixel"]=serde_json::json!(self.cursor_pixel);
        state["selected_aperture"]=serde_json::json!(self.regions.selected);
        state["aperture_tool"]=serde_json::json!(format!("{:?}",self.regions.tool));
        state["ribbon_tab"]=serde_json::json!(self.tools.prefs.ribbon_tab);
        state["ribbon_collapsed"]=serde_json::json!(self.tools.prefs.ribbon_collapsed);
        state["stretch_histogram_builds"]=serde_json::json!(self.stretch_histogram_builds);
        let spectrum=image.is_some_and(|image|image.spectrum.is_some());
        #[cfg(feature="pro")] {
            state["pro_initialized"]=serde_json::json!(self.pro.is_some());
            state["pro_result"]=serde_json::json!(self.pro.as_ref().map(|p|p.summary()));
            state["pro_message"]=serde_json::json!(self.pro.as_ref().map(|p|&p.message));
            state["pro_busy"]=serde_json::json!(self.pro.as_ref().map(|p|p.busy));
            state["multi"]=serde_json::json!(self.multi.as_ref().map(|m|m.configuration()));
            state["multi_toolbar"]=self.multi.as_ref().map_or(serde_json::Value::Null,|m|m.toolbar_geometry());
            state["cache_bytes"]=serde_json::json!(self.multi.as_ref().map(|m|m.bytes()));
        state["image_layer_renderer"]=self.pro.as_ref().and_then(|pro|pro.image_layers.as_ref()).map(|renderer|renderer.summary()).unwrap_or(serde_json::Value::Null);
        state["spectral_lines"]=self.pro.as_ref().and_then(|pro|pro.spectral_lines.as_ref()).map(|lines|lines.summary()).unwrap_or(serde_json::Value::Null);
        state["spectral_analysis"]=self.pro.as_ref().and_then(|pro|pro.spectral_analysis.as_ref()).map(|analysis|analysis.summary()).unwrap_or(serde_json::Value::Null);
        state["pro_operation"]=self.pro.as_ref().map(|pro|serde_json::json!(pro.operation)).unwrap_or(serde_json::Value::Null);
        state["pro_panel"]=serde_json::json!({"width":self.pro.as_ref().map(|pro|pro.panel_width),"collapsed":self.tools.prefs.pro_collapsed(spectrum),"context":if spectrum{"spectrum"}else{"image"},"layout":self.pro_dock.as_ref().map(|(_,dock)|dock.summary())});
        }
        #[cfg(not(feature="pro"))] {
            state["pro_initialized"]=serde_json::json!(false);
            for key in ["pro_result","pro_message","pro_busy","multi","cache_bytes","image_layer_renderer","spectral_lines","spectral_analysis","pro_operation"]{state[key]=serde_json::Value::Null;}
            state["pro_panel"]=serde_json::json!({"width":null,"collapsed":self.tools.prefs.pro_collapsed(spectrum),"context":if spectrum{"spectrum"}else{"image"},"layout":null});
        }
        state["stretch_histogram_cached"]=serde_json::json!(self.scale_mapping.is_some() && self.stretch==Stretch::HistogramEqualization);
        state.as_object_mut().unwrap().extend(serde_json::json!({"coordinates_open":self.coordinates.open,"gaia_busy":self.coordinates.busy,"gaia_message":self.coordinates.message,"gaia_result":self.coordinates.result,"gaia_wcs_active":self.coordinates.header().is_some(),"region_preview_count":self.regions.preview_template.as_ref().map_or(0,|t|t.apertures.len())}).as_object().unwrap().clone());
        state["build_features"]=serde_json::json!({"pro":cfg!(feature="pro")});
        state
    }
    fn command(&mut self,ctx:&egui::Context,input:serde_json::Value)->Result<serde_json::Value,String>{
        use serde_json::json;
        #[cfg(feature="pro")] self.prepare_pro_dock(ctx,false);
        let operation=input["operation"].as_str().ok_or("Command requires operation")?;
        let params=&input["params"];
        match operation {
            "state"=>return Ok(self.state()),
            "selection.begin"=>{
                if self.loading{return Err("Wait for the selection image to load".into());}
                let image=self.image.as_ref().filter(|i|i.spectrum.is_none()).ok_or("Selection requires a two-dimensional image")?;
                let identity=self.remote.as_ref().map(|r|r.manifest.source_version.as_str()).or_else(||params["identity"].as_str()).ok_or("Selection requires a source identity")?;
                let source=json!({"path":image.info.path,"hdu":image.info.hdu,"width":image.info.width,"height":image.info.height,"identity":identity});
                self.selection.begin(params,source,self.generation)?;
            },
            "selection.cancel"=>self.selection.cancel(),
            "selection.fail"=>self.selection.fail(),
            "selection.finish"=>self.selection.finish(),
            "selection.undo"=>self.selection.undo(),
            "selection.clear"=>self.selection.clear(),
            "selection.configure"=>{
                let cmap=params["cmap"].as_str().map(|name|match name.to_ascii_lowercase().as_str(){"grey"|"gray"=>Ok("Gray"),"heat"=>Ok("Heat"),"cool"=>Ok("Cool"),"red"=>Ok("Red"),"blue-orange"=>Ok("Blue-orange"),_=>Err("Unsupported selection color map")}).transpose()?;
                self.selection.configure(params)?;if let Some(name)=cmap{self.tools.prefs.render.palette=fitsviewer_core::display::Palette::builtin(name);self.dirty=true;}
            },
            "exit_all"=>self.exit_all_windows(ctx),
            #[cfg(not(feature="pro"))]
            "session.export"=>return Ok(json!({"schema":1,"mode":"Minimal","views":[{"path":self.path,"hdu":self.image.as_ref().map(|i|i.info.hdu),"view":self.view_configuration(),"apertures":self.regions.apertures}]})),
            #[cfg(feature="pro")]
            "session.export"=>{self.cache_current();return Ok(if let Some(multi)=&self.multi{let mut value=multi.configuration();value["mode"]=json!(self.tools.prefs.mode);value}else{json!({"schema":1,"mode":self.tools.prefs.mode,"views":[{"path":self.path,"hdu":self.image.as_ref().map(|i|i.info.hdu),"view":self.view_configuration(),"apertures":self.regions.apertures}]})});},
            "session.apply"=>{
                if self.loading{return Err("Wait for the current file load before applying a session".into());}
                let replacement=Self::new(String::new(),self.api.clone(),self.token.clone(),None).with_session(params.clone())?;
                self.coordinates.cancel(ctx,&self.science,self.generation);self.science.request(ctx,self.generation,"release_pro",json!({}));
                self.path=replacement.path;self.requested_hdu=replacement.requested_hdu;self.pending_view=replacement.pending_view;self.saved_multi=replacement.saved_multi;
                self.tools.prefs.mode=replacement.tools.prefs.mode;#[cfg(feature="pro")] {self.pro=None;self.multi=None;self.pro_dock=None;}self.image=None;self.texture=None;self.remote=None;self.first_frame=false;self.generation+=1;
                self.request(ctx,self.requested_hdu);
            },
            #[cfg(feature="pro")]
            "result"=>return Ok(self.pro.as_ref().ok_or("Select PRO to read a scientific result")?.result.clone()),
            "open"=>{let destination=params["destination"].as_str().map(|s|document_tabs::Destination::parse(s).ok_or("Unknown open destination")).transpose()?;self.open_path(ctx,params["path"].as_str().ok_or("Open requires path")?.into(),LoadOptions{hdu:params["hdu"].as_u64().map(|v|v as usize),..Default::default()},destination)?;},
            "tab.select"=>self.select_document(ctx,params["index"].as_u64().ok_or("Missing document tab")? as usize)?,
            "tab.close"=>self.close_document_tab(ctx,params["index"].as_u64().map(|i|i as usize).or(self.documents.selected).ok_or("No document tab")?)?,
            #[cfg(feature="pro")] "layer.close"=>{let index=params["index"].as_u64().ok_or("Missing layer index")? as usize;self.multi.as_mut().ok_or("Select PRO for layers")?.remove_layer(index)?;self.refresh_document_refs(ctx);},
            "action"=>{let action=params["name"].as_str().ok_or("Action requires name")?;if !controls::ACTIONS.contains(&action){return Err("Unknown viewer action".into());}self.action(action,ctx);},
            "preferences"=>{let text=serde_json::to_string(params).map_err(|e|e.to_string())?;let prefs=controls::Preferences::parse(&text)?;controls::save(&prefs)?;self.tools.prefs=prefs;self.dirty=true;#[cfg(feature="pro")] {self.pro_dock=None;self.prepare_pro_dock(ctx,true);}self.prepare_popup_windows();},
            "coordinates"=>self.coordinates.open=params["open"].as_bool().unwrap_or(true),
            "coordinates.gaia"=>{let image=self.image.as_deref().ok_or("Open an image first")?;self.coordinates.start(ctx,&self.science,self.generation,image,params.clone())?;},
            "coordinates.cancel"=>{self.coordinates.cancel(ctx,&self.science,self.generation);self.coordinates.message="Gaia calibration cancelled".into();},
            "coordinates.use_gaia"=>{if self.coordinates.solution.is_none(){return Err("No validated Gaia solution".into());}self.coordinates.use_solution=params["enabled"].as_bool().unwrap_or(true);self.coordinates.changed=true;},
            "histogram"=>self.tools.histogram_open=params["open"].as_bool().unwrap_or(true),
            "header.select"=>{let hdu=params["hdu"].as_u64().ok_or("Missing header HDU")? as usize;if !self.image.as_ref().is_some_and(|image|image.info.hdus.iter().any(|header|header.index==hdu)){return Err("Unknown header HDU".into());}self.header.select(hdu);self.header_open=true;},
            #[cfg(feature="pro")]
            "workspace.add"=>{let multi=self.multi.as_mut().ok_or("Select PRO for multiple views")?;let paths:Vec<String>=serde_json::from_value(params["paths"].clone()).map_err(|e|e.to_string())?;if paths.len()>32{return Err("At most 32 files".into());}multi.add_paths(&paths);},
            #[cfg(feature="pro")]
            "workspace.select"=>self.select_workspace_document(ctx,params["index"].as_u64().ok_or("Missing view index")? as usize)?,
            #[cfg(feature="pro")]
            "workspace.close"=>self.close_workspace_document(ctx,params["index"].as_u64().ok_or("Missing view index")? as usize)?,
            "view"=>{
                let mut merged=self.view_configuration();if let (Some(target),Some(source))=(merged.as_object_mut(),params.as_object()){target.extend(source.clone());}
                fitsviewer_core::view::View::parse(&merged.to_string())?;
                let low=params["low"].as_f64().unwrap_or(self.low);let high=params["high"].as_f64().unwrap_or(self.high);
                if !low.is_finite() || !high.is_finite() || high<=low{return Err("View requires finite Low < High".into());}
                let stretch=if let Some(name)=params["stretch"].as_str(){Stretch::parse(name).ok_or("Unknown stretch")?}else{self.stretch};
                let bias=params["bias"].as_f64().unwrap_or(self.tone.bias);let contrast=params["contrast"].as_f64().unwrap_or(self.tone.contrast);
                if !(0. ..=1.).contains(&bias) || !(0.05..=20.).contains(&contrast){return Err("Invalid brightness/contrast".into());}
                let render=if params["render"].is_object(){let r:fitsviewer_core::display::RenderOptions=serde_json::from_value(params["render"].clone()).map_err(|e|e.to_string())?;r.validate()?;r}else{self.tools.prefs.render.clone()};
                self.low=low;self.high=high;self.stretch=stretch;self.tone=DisplayTone{bias,contrast};self.tools.prefs.render=render;self.limits=Limits::Manual;self.invert=params["invert"].as_bool().unwrap_or(self.invert);self.dirty=true;
                if let Some(zoom)=params["zoom"].as_f64().filter(|v|v.is_finite() && *v>=0.005 && *v<=128.){self.zoom=zoom as f32;self.auto_fit=false;}
                if let Some(angle)=params["rotation_deg"].as_f64().filter(|v|v.is_finite()){self.rotation_deg=((angle+180.).rem_euclid(360.)-180.) as f32;}
                if let (Some(x),Some(y))=(params["pan"][0].as_f64(),params["pan"][1].as_f64()){if x.abs()<1e8 && y.abs()<1e8{self.pan=egui::vec2(x as f32,y as f32);self.auto_fit=false;}}
                if let Ok(bounds)=serde_json::from_value::<[f64;4]>(params["plot_bounds"].clone()){self.plot.bounds=bounds;self.plot.fit=false;}
            },
            "aperture.uncertainty"=>self.regions.configure_uncertainty(params)?,
            "aperture.add"=>{#[cfg(not(feature="pro"))] mini_aperture(params)?;let aperture:fitsviewer_core::regions::Aperture=serde_json::from_value(params.clone()).map_err(|e|e.to_string())?;aperture.shape.validate()?;if self.tools.prefs.mode!="PRO" && !matches!(aperture.shape,fitsviewer_core::regions::Shape::Circle{..}){return Err("Select PRO for advanced geometry".into());}self.regions.add_aperture(aperture);},
            "aperture.select"|"aperture.clear"|"aperture.delete"|"aperture.resize"|"aperture.move"|"aperture.update"=>{
                let image=self.image.as_deref().ok_or("Open an image first")?;
                if image.spectrum.is_some(){return Err("Aperture editing needs a two-dimensional image".into());}
                if let Some(index)=params.get("index"){
                    self.regions.select_aperture(index.as_u64().ok_or("Invalid aperture index")? as usize)?;
                }else if operation=="aperture.select"{return Err("Missing aperture index".into());}
                match operation{
                    "aperture.clear"=>self.regions.clear_all(),
                    "aperture.delete"=>{if !self.regions.delete_selected(){return Err("Select an aperture first".into());}},
                    "aperture.resize"=>self.regions.resize_selected(params["radius"].as_f64().ok_or("Missing aperture radius")?,image)?,
                    "aperture.move"|"aperture.update"=>{
                        let center:[f64;2]=serde_json::from_value(params["center"].clone()).map_err(|_|"Expected two zero-based pixel coordinates")?;
                        if operation=="aperture.move"{self.regions.move_selected(center,image)?;}else{
                            let sky=if params["sky"].is_null(){None}else{Some(serde_json::from_value::<[f64;2]>(params["sky"].clone()).map_err(|_|"Expected inner and outer sky radii")?)};
                            self.regions.update_selected_circle(center,params["radius"].as_f64().ok_or("Missing aperture radius")?,sky,image)?;
                        }
                    },_=>{},
                }
            },
            #[cfg(feature="pro")]
            "aperture.template"=>{if self.tools.prefs.mode!="PRO"{return Err("Select PRO for template paste".into());}self.regions.template=serde_json::to_string(&params["template"]).map_err(|e|e.to_string())?;self.regions.paste_mode=params["method"].as_str().unwrap_or("Keep pixels").into();self.regions.auto_confirm=params["preview"]!=true;self.regions.paste(self.cursor_pixel,self.image.as_deref());},
            #[cfg(feature="pro")]
            "aperture.preview.apply"=>self.regions.confirm_paste(),
            #[cfg(feature="pro")]
            "aperture.preview.cancel"=>self.regions.preview_template=None,
            #[cfg(feature="pro")]
            "aperture.template_load"=>{if self.tools.prefs.mode!="PRO"{return Err("Select PRO for region templates".into());}fitsviewer_core::regions::RegionTemplate::parse(&params["template"].to_string())?;self.regions.template=params["template"].to_string();self.regions.auto_confirm=false;self.regions.paste(self.cursor_pixel,self.image.as_deref());},
            #[cfg(feature="pro")]
            "annotation.set"=>{let annotations:Vec<serde_json::Value>=serde_json::from_value(params["annotations"].clone()).map_err(|e|e.to_string())?;if annotations.len()>256 || params.to_string().len()>65536{return Err("Annotations exceed supported limits".into());}let pro=self.pro.as_mut().ok_or("Select PRO for annotations")?;pro.annotations=annotations;pro.annotations_dirty=true;},
            #[cfg(feature="pro")]
            "pro.select_row"=>self.pro.as_mut().ok_or("Select PRO for table rows")?.select_row(ctx,&self.science,self.generation,params["row"].as_u64().ok_or("Missing row")? as usize,params["notify"].as_bool().unwrap_or(false)),
            #[cfg(feature="pro")]
            "spectral.lines"=>{
                if self.tools.prefs.mode!="PRO"{return Err("Select PRO for spectral reference lines".into());}
                let pro=self.pro.as_mut().ok_or("Select PRO for spectral reference lines")?;
                pro.sync_spectrum(self.image.as_deref());
                pro.spectral_lines.as_mut().ok_or("Open a spectrum first")?.configure(params)?;
            },
            #[cfg(feature="pro")]
            "spectral.analysis"=>{
                if self.tools.prefs.mode!="PRO"{return Err("Select PRO for spectral analysis".into());}
                let pro=self.pro.as_mut().ok_or("Select PRO for spectral analysis")?;pro.sync_spectrum(self.image.as_deref());
                if params["action"].is_string() && pro.busy{return Err("Another PRO job is running".into());}
                let image=self.image.as_deref().ok_or("Open a spectrum first")?;
                let mut next_lines=pro.spectral_lines.as_ref().ok_or("Open a spectrum first")?.as_ref().clone();
                next_lines.configure(params)?;
                if params["action"].is_string(){next_lines.request(image)?;}
                let analysis=pro.spectral_analysis.as_mut().ok_or("Open a spectrum first")?;
                analysis.configure(params)?;analysis.sync_context(&next_lines);
                pro.spectral_lines=Some(Box::new(next_lines));pro.operation="spectral_analysis".into();self.tools.prefs.set_pro_collapsed(true,false);
                if let Some(action)=params["action"].as_str(){
                    let request=analysis.request(action,image,pro.spectral_lines.as_ref().unwrap())?;
                    pro.send(ctx,&self.science,self.generation,"spectral_analysis",request);
                }self.plot.fit=true;
            },
            #[cfg(feature="pro")]
            "layer.configure"=>{
                if self.tools.prefs.mode!="PRO"{return Err("Select PRO for layers".into());}
                self.multi.as_mut().ok_or("Select PRO for layers")?.configure_layer(params["index"].as_u64().ok_or("Missing layer index")? as usize,&params["settings"])?;self.plot.fit=true;
            },
            #[cfg(feature="pro")]
            "spectral.overlay"=>{
                if self.tools.prefs.mode!="PRO"{return Err("Select PRO for spectral comparisons".into());}
                self.multi.as_mut().ok_or("Select PRO for spectral comparisons")?.configure_spectrum_overlay(params["index"].as_u64().ok_or("Missing spectrum index")? as usize,&params["settings"])?;
                self.plot.fit=true;
            },
            #[cfg(feature="pro")]
            "image.match_flux"=>{
                if self.tools.prefs.mode!="PRO"{return Err("Select PRO for image layers".into());}
                let image=self.image.as_deref().filter(|image|image.spectrum.is_none()).ok_or("Open a reference image first")?;
                let pro=self.pro.as_mut().ok_or("Select PRO for image layers")?;if pro.busy{return Err("Another PRO job is running".into());}
                let index=params["index"].as_u64().ok_or("Missing target layer index")? as usize;
                let mut request=self.multi.as_ref().ok_or("Select PRO for image layers")?.image_flux_request(index,image,&params["params"])?;
                if let Some(wcs)=&self.wcs_override{request["reference_wcs_override"]=json!(wcs);}
                pro.select("image_layers");pro.send(ctx,&self.science,self.generation,"image_flux_match",request);self.tools.prefs.set_pro_collapsed(false,false);
            },
            #[cfg(feature="pro")]
            "pro.run"=>{if self.tools.prefs.mode!="PRO"{return Err("Select PRO before using scientific tools".into());}let operation=params["operation"].as_str().ok_or("Missing PRO operation")?;if !pro::OPERATIONS.iter().any(|(o,_)|*o==operation){return Err("Unknown PRO tool".into());}let view=self.view_configuration();let pro=self.pro.get_or_insert_with(pro::Workspace::default);if pro.busy{return Err("Another PRO job is running".into());}pro.operation=operation.into();pro.parameters=serde_json::to_string(&params["params"]).map_err(|e|e.to_string())?;pro.submit(ctx,&self.science,self.generation,self.image.as_deref(),&view);},
            "close"=>{self.selection.cancel();if let Some(index)=self.documents.selected{self.close_document_tab(ctx,index)?;}else{self.close_document(ctx);}
                self.refresh_document_refs(ctx);
            },
            _=>return Err("Unknown viewer command".into()),
        }if operation=="close"{self.scale_mapping=None;}Ok(json!({"accepted":true,"operation":operation,"asynchronous":(matches!(operation,"open"|"pro.run"|"coordinates.gaia"|"image.match_flux") || (operation=="spectral.analysis" && params["action"].is_string()))}))
    }
    fn commands(&mut self,ctx:&egui::Context){
        #[cfg(target_arch="wasm32")]
        {let commands=WEB_COMMANDS.with(|slot|std::mem::take(&mut *slot.borrow_mut()));for input in commands {let id=input.get("id").cloned().unwrap_or_default();let response=match self.command(ctx,input){Ok(result)=>serde_json::json!({"id":id,"ok":true,"result":result}),Err(error)=>serde_json::json!({"id":id,"ok":false,"error":error})};if let (Some(window),Ok(value))=(web_sys::window(),js_sys::JSON::parse(&response.to_string())){let _=js_sys::Reflect::set(&window,&"__fitsviewerCommandResult".into(),&value);}}}
        #[cfg(not(target_arch="wasm32"))]
        if let Some(path)=std::env::var_os("FITSVIEWER_COMMAND_FILE") {if let Ok(bytes)=std::fs::read(&path){let _=std::fs::remove_file(&path);if bytes.len()<=1024*1024{if let Ok(inputs)=serde_json::from_slice::<Vec<serde_json::Value>>(&bytes){for input in inputs{if let Err(error)=self.command(ctx,input){self.error=error;}}}}}ctx.request_repaint_after(std::time::Duration::from_millis(100));}
    }
    fn wcs_target(&self)->Option<[f64;2]>{
        let image=self.image.as_ref()?;
        if image.spectrum.is_some(){return None;}
        self.cursor_pixel.or_else(||self.coordinates.open.then_some([(image.info.width as f64-1.)/2.,(image.info.height as f64-1.)/2.]))
    }
    fn show_cached_wcs(&mut self,pixel:[f64;2])->bool{
        let Some(value)=self.wcs_cache.get(pixel) else{return false;};
        let Some(point)=wcs_cache::point_at(&value,pixel) else{return false;};
        self.wcs_text=format!("FK5 J2000 {}",point["sexagesimal"].as_str().unwrap_or_default());
        self.wcs_pixel=Some(pixel);self.wcs_result=Some(value);true
    }
    fn request_wcs(&mut self,ctx:&egui::Context){
        let Some(image)=self.image.as_ref() else{return;};
        if image.spectrum.is_some(){return;}
        if self.wcs_cache.bind(self.generation,&image.info.path,image.info.hdu,self.wcs_override.as_deref()){
            self.wcs_pixel=None;self.wcs_pending=false;self.wcs_disabled=false;self.wcs_result=None;self.wcs_text.clear();self.wcs_requested_at=-100.;
        }
        let Some(pixel)=self.wcs_target() else{return;};
        let changed_pixel=self.wcs_pixel!=Some(pixel);
        if self.show_cached_wcs(pixel){if changed_pixel{ctx.request_repaint();}return;}
        // Retry another position after a point failure, without hammering the same failed point.
        // 单点失败后仍可查看别处，但不在原点重复发起失败请求。
        if self.wcs_pixel==Some(pixel) && self.wcs_result.is_none() && !self.wcs_text.is_empty(){return;}
        if self.wcs_disabled || self.wcs_cache.is_pending(){return;}
        // Astropy validates the complete model, including DSS plate WCS without CTYPE.
        // 由外部 Astropy 判定完整 WCS；DSS 底片坐标不一定带有 CTYPE 关键字。
        let now=ctx.input(|i|i.time);
        if now-self.wcs_requested_at<0.12 {ctx.request_repaint_after(std::time::Duration::from_millis(120));return;}
        let Some(label)=self.wcs_cache.begin(pixel) else{return;};
        self.wcs_requested_at=now;self.wcs_pending=true;
        let image=self.image.as_ref().unwrap();
        self.science.request_labeled(ctx,self.generation,"wcs",&label,serde_json::json!({"path":image.info.path,"hdu":image.info.hdu,"points":[pixel],"wcs_override":self.wcs_override}));
    }
    fn sync_coordinates(&mut self,ctx:&egui::Context){
        let key=self.image.as_ref().map(|i|format!("{}:{}\n{}",self.generation,i.info.path,i.info.hdu)).unwrap_or_default();
        self.coordinates.bind(ctx,&self.science,self.generation,key,self.wcs_override.clone());
        if self.coordinates.changed{
            self.coordinates.changed=false;self.wcs_cache.clear();self.wcs_override=self.coordinates.header().map(str::to_string);
            self.wcs_pixel=None;self.wcs_pending=false;self.wcs_disabled=false;self.wcs_result=None;self.wcs_text.clear();
            self.regions.wcs_override=self.wcs_override.clone();
            // Markers computed with an old WCS must be recalculated, not silently reused.
            // WCS 更新后清除旧坐标叠加，避免沿用原解的标记。
            #[cfg(feature="pro")]
if let Some(pro)=&mut self.pro{pro.markers.clear();}
            #[cfg(feature="pro")]
if let Some(multi)=&mut self.multi{multi.invalidate_wcs();}
            self.cache_current();
            ctx.request_repaint();
        }
    }
    #[cfg(target_arch = "wasm32")]
    fn export_state(&self,ctx:&egui::Context) {
        let now=js_sys::Date::now();let elapsed=now-self.last_export.get();
        if elapsed<80.{ctx.request_repaint_after(std::time::Duration::from_millis((80.-elapsed).ceil().max(1.) as u64));return;}
        self.last_export.set(now);
        let state = self.state();
        if let (Some(window), Ok(value)) =
            (web_sys::window(), js_sys::JSON::parse(&state.to_string()))
        {
            let _ = js_sys::Reflect::set(
                &window,
                &wasm_bindgen::JsValue::from_str("__fitsviewerState"),
                &value,
            );
        }
    }
    fn spectrum_toolbar(&mut self,ui:&mut egui::Ui,ctx:&egui::Context){
        let Some(meta)=self.image.as_ref().and_then(|i|i.info.spectrum.clone())else{return;};
        ui.set_max_width(440.);let mut next=self.load_options;let mut reload=false;
        ui.horizontal(|ui|{if ui.button(crate::i18n::t("Fit spectrum (F)")).clicked(){self.plot.fit=true;}ui.checkbox(&mut self.plot.errors,crate::i18n::t("Show uncertainty"));});
        if !meta.columns.is_empty(){
            let mut x=meta.x_column;let mut y=meta.y_column;let mut error=meta.error_column.unwrap_or(meta.columns.len());
            for (id,label,chosen) in [("spectral-x","X",&mut x),("spectral-y","Flux",&mut y),("spectral-e","Error",&mut error)]{
                ui.horizontal(|ui|{ui.label(crate::i18n::t(label));egui::ComboBox::from_id_salt(id).selected_text(meta.columns.get(*chosen).map(String::as_str).unwrap_or(crate::i18n::t("None"))).width(180.).show_ui(ui,|ui|{if id=="spectral-e"{ui.selectable_value(chosen,meta.columns.len(),crate::i18n::t("None"));}for (i,name) in meta.columns.iter().enumerate(){ui.selectable_value(chosen,i,name);}});});
            }
            if x!=meta.x_column || y!=meta.y_column || error!=meta.error_column.unwrap_or(meta.columns.len()){
                next.x_column=Some(x);next.y_column=Some(y);next.error_column=(error<meta.columns.len()).then_some(error);next.error_hdu=None;next.ignore_errors=error==meta.columns.len();reload=true;
            }
        }else if let Some(image)=&self.image{
            let none=usize::MAX;let mut chosen=meta.error_hdu.unwrap_or(none);
            ui.horizontal(|ui|{ui.label(crate::i18n::t("Uncertainty HDU"));egui::ComboBox::from_id_salt("spectrum-error-hdu").selected_text(meta.error_hdu.map(|h|format!("HDU {h}")).unwrap_or_else(||crate::i18n::t("None").into())).show_ui(ui,|ui|{ui.selectable_value(&mut chosen,none,crate::i18n::t("None"));for h in image.info.hdus.iter().filter(|h|h.axes.len()==1 && h.viewable && h.index!=image.info.hdu){ui.selectable_value(&mut chosen,h.index,format!("{} · {}",h.index,h.name));}});});
            if chosen!=meta.error_hdu.unwrap_or(none){next.error_hdu=(chosen!=none).then_some(chosen);next.error_column=None;next.ignore_errors=chosen==none;reload=true;}
        }
        let previous=next.error_kind;
        ui.horizontal(|ui|{ui.label(crate::i18n::t("Uncertainty type"));egui::ComboBox::from_id_salt("spectrum-error-kind").selected_text(next.error_kind.as_str()).show_ui(ui,|ui|{for kind in [ErrorKind::Auto,ErrorKind::Sigma,ErrorKind::Variance,ErrorKind::Ivar]{ui.selectable_value(&mut next.error_kind,kind,kind.as_str());}});});
        if previous!=next.error_kind && (next.error_column.is_some() || next.error_hdu.is_some()){reload=true;}
        self.load_options=next;
        if reload{self.request(ctx,self.image.as_ref().map(|image|image.info.hdu));}
        ui.add(egui::Label::new(&meta.note).wrap());
    }
    #[cfg(not(target_arch = "wasm32"))]
    fn native_diagnostics(&self, ctx: &egui::Context) {
        let now=self.started.elapsed().as_secs_f64()*1000.;
        // Opt-in functional diagnostics for local testing; never sent over the network.
        // 可选的本地状态输出用于功能测试，不发送到网络。
        if let Some(path) = std::env::var_os("FITSVIEWER_DIAGNOSTICS").filter(|_|now-self.last_export.get()>80.) {
            self.last_export.set(now);
            let state = self.state();
            let _ = std::fs::write(path, state.to_string());
        }
        if let Ok(seconds) = std::env::var("FITSVIEWER_EXIT_AFTER")
            .unwrap_or_default()
            .parse::<f64>()
        {
            if seconds > 0.0 && self.started.elapsed().as_secs_f64() >= seconds {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}
impl eframe::App for FitsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let previous_document_refs=self.scientific_paths();
        if let Some(control)=self.window_control.clone(){
            for event in (control.poll)(ctx.input(|input|input.focused)){
                match event{
                    WindowEvent::Open{path,options}=>{if let Err(error)=self.open_path(ctx,path,options,None){self.error=error;}ctx.send_viewport_cmd(egui::ViewportCommand::Focus);},
                    WindowEvent::Exit=>{self.close_document(ctx);ctx.send_viewport_cmd(egui::ViewportCommand::Close);return;},
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
        // Inactive Jupyter iframes can have a zero-sized canvas. Never clamp or
        // persist layouts against that transient viewport. Native IPC is polled
        // above, so a minimized app still receives a graceful Exit all request.
        // 隐藏画布不更新布局；原生通信仍先处理，最小化时也能正常退出。
        if !window_layout::usable_viewport(ctx.screen_rect()){
            self.popup_windows.suspend();
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }
        self.sync_coordinates(ctx);
        if i18n::set(&self.tools.prefs.language){
            #[cfg(target_arch="wasm32")] let _=text_io::browser("setLanguage",&[&self.tools.prefs.language]);
            ctx.request_repaint();
        }
        #[cfg(all(target_arch="wasm32",feature="offline"))]
        {if let Some((key,bytes,image))=WEB_OFFLINE_IMAGE.with(|slot|slot.borrow_mut().take()){
            match self.accept_offline_image(ctx,image){Ok(())=>{WEB_OFFLINE_SOURCE.with(|slot|slot.borrow_mut().insert(key,bytes));self.refresh_document_refs(ctx);},Err(error)=>self.error=error}
        }}
        self.tools.apply_theme(ctx);
        self.science_updates(ctx);
        let previous_preferences=serde_json::to_string(&self.tools.prefs).unwrap_or_default();
        let previous_render=self.tools.prefs.render.clone();
        let ready = self.pending.lock().unwrap().1.take();
        if let Some(result) = ready {
            self.loading = false;
            #[cfg(feature="pro")] let layer=self.pending_layer.is_some();
            #[cfg(not(feature="pro"))] let layer=false;
            if layer{#[cfg(feature="pro")]self.accept_layer_load(ctx,result);}else{match result {
                Ok(ImageLoad::Full(image)) => self.accept(image),
                Ok(ImageLoad::Remote(manifest)) => self.accept_remote(manifest),
                Err(error) => {
                    self.fail_open(ctx,error);
                    // Keep the last valid view so a failed column/path choice can be corrected.
                    // 保留上一份有效数据，允许修正选列或路径错误。
                }
            }
        }
        }
        #[cfg(feature="pro")] self.poll_layer_preview(ctx);
        if self.image.is_none() && !self.loading && self.error.is_empty() && !self.path.is_empty() {
            self.request(ctx, self.requested_hdu);
        }
        let mut preview_changed=false;
        if let Some(remote)=&mut self.remote{
            remote.set_compact(self.tools.prefs.compact_large_images && self.tools.prefs.mode!="PRO");
            if remote.poll(){preview_changed=true;self.tools.histogram=None;self.scale_mapping=None;self.dirty=true;}
            if self.tools.histogram.is_none(){if let Some(preview)=remote.preview(){let mut histogram=fitsviewer_core::display::Histogram::new(preview,128);histogram.sampled=true;self.tools.histogram=Some(histogram);self.tools.histogram_builds+=1;}}
        }
        if preview_changed{self.cache_current();}
        self.prepare_popup_windows();
        #[cfg(feature="pro")] self.prepare_pro_dock(ctx,false);
        self.commands(ctx);
        self.selection.validate(self.generation,self.image.as_ref().map(|i|i.info.path.as_str()),self.image.as_ref().map(|i|i.info.hdu),self.remote.as_ref().map(|r|r.manifest.source_version.as_str()),self.loading);
        self.selection.toolbar(ctx);
        self.tools.apply_theme(ctx);
        self.sync_coordinates(ctx);
        self.shortcuts(ctx);
        if !self.selection.active(){if let Some(action)=self.tools.shortcut(ctx){self.action(&action,ctx);}}
        #[cfg(not(target_arch = "wasm32"))]
        {
            let dropped = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .find_map(|file| file.path.clone())
            });
            if let Some(path) = dropped.filter(|_| !self.loading) {
                if let Err(error)=self.open_path(ctx,path.to_string_lossy().into_owned(),LoadOptions::default(),None){self.error=error;}
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
                self.browse(ctx);
            }
        }
        let ribbon_multi_action=self.ribbon(ctx);
        self.prepare_popup_windows();
        self.settings_window(ctx);
        if i18n::set(&self.tools.prefs.language){
            #[cfg(target_arch="wasm32")] let _=text_io::browser("setLanguage",&[&self.tools.prefs.language]);
            ctx.request_repaint();
        }
        if !cfg!(feature="pro") || self.tools.prefs.mode!="PRO" || self.image.is_none(){self.tools.histogram(ctx,self.image.as_deref(),self.low,self.high,&mut self.popup_windows);}
        let spectrum=self.image.as_ref().is_some_and(|image|image.spectrum.is_some());
        let is_pro=cfg!(feature="pro") && self.tools.prefs.mode=="PRO";
        #[allow(unused_mut)] let mut document_action=None;
        #[allow(unused_mut)] let mut documents_shown=false;
        #[cfg(feature="pro")]
        {
        self.sync_pro(ctx);
        let multi_action=if spectrum{ribbon_multi_action}else if let Some(multi)=&mut self.multi{
            documents_shown=true;let (action,selected)=multi.toolbar_with_documents(ctx,&mut self.documents);document_action=selected;action.or(ribbon_multi_action)
        }else{ribbon_multi_action};
        if let Some(action)=multi_action {
            self.cache_current();
            match action {
                multiview::Action::Select(index)=>{let _=self.command(ctx,serde_json::json!({"operation":"workspace.select","params":{"index":index}}));},
                multiview::Action::Export(panels)=>{#[cfg(feature="pro")]
if let Some(pro)=&mut self.pro{pro.send(ctx,&self.science,self.generation,"export",serde_json::json!({"panels":panels,"width":1800,"height":1200}));}},
                multiview::Action::CopySession=>{if let Some(multi)=&self.multi{let mut session=multi.configuration();session["mode"]=serde_json::json!("PRO");ctx.copy_text(serde_json::to_string_pretty(&session).unwrap_or_default());}},
                multiview::Action::Close(index)=>{let _=self.command(ctx,serde_json::json!({"operation":"workspace.close","params":{"index":index}}));},
            }
        }
        }
        // Documents sit below every top toolbar, directly before the image workspace.
        if !documents_shown{document_action=self.documents.toolbar(ctx);}
        if let Some(action)=document_action{let result=match action{document_tabs::Action::Select(i)=>self.select_document(ctx,i),document_tabs::Action::Close(i)=>self.close_document_tab(ctx,i)};if let Err(error)=result{self.error=error;}}
        self.sync_coordinates(ctx);
        let available=self.api.is_some() || self.science.local.is_some();
        if !spectrum{if is_pro && self.image.is_some(){self.coordinates.tick(ctx,&self.science,self.generation);}else{self.coordinates.panel(ctx,&self.science,self.generation,self.image.as_deref(),self.wcs_result.as_ref(),self.cursor_pixel,available,&mut self.popup_windows);}}
        self.sync_coordinates(ctx);
        let view=self.view_configuration();
        #[cfg(feature="pro")] self.pro_workspace(ctx,&view,available);
        #[cfg(feature="pro")] if let Some(pro)=&mut self.pro {
            for aperture in std::mem::take(&mut pro.new_apertures){self.regions.add_aperture(aperture);}
            if let (Some(pixel),Some(image))=(pro.focus_pixel.take(),self.image.as_ref()){self.auto_fit=false;self.pan=image_transform::ImageTransform::new(Pos2::ZERO,Vec2::ZERO,1.,self.rotation_deg).rotate_vector(Vec2::new(((image.info.width as f64-1.)/2.-pixel[0]) as f32*self.zoom,(pixel[1]-(image.info.height as f64-1.)/2.) as f32*self.zoom));}
            #[cfg(not(target_arch="wasm32"))]
            if let Some(path)=pro.open_artifact.take(){let mut command=std::process::Command::new(if cfg!(windows){"explorer.exe"}else if cfg!(target_os="macos"){"open"}else{"xdg-open"});command.arg(path);let _=command.spawn();}
            if let Some(path)=pro.open_path.take(){let notice=pro.take_open_notice();if let Err(error)=self.open_generated_file(ctx,path){self.error=error;}if let Some(notice)=notice{if let Some(pro)=&mut self.pro{pro.message=notice;}}}
        }
        #[cfg(feature="pro")]
        if let Some(index)=self.multi.as_mut().and_then(|multi|multi.pending_selection.take()){
            if let Err(error)=self.command(ctx,serde_json::json!({"operation":"workspace.select","params":{"index":index}})){if let Some(multi)=&mut self.multi{multi.message=error;}}
        }
        if !spectrum && !is_pro{self.regions.panel(ctx,self.image.as_deref(),false,&mut self.popup_windows);}
        #[cfg(feature="pro")]
        if let Some(params)=self.regions.profile_request.take(){#[cfg(feature="pro")]
if let Some(pro)=&mut self.pro{if !pro.busy{pro.select("line_profile");pro.parameters=params.to_string();pro.submit(ctx,&self.science,self.generation,self.image.as_deref(),&view);}else{self.regions.message="Wait for the running PRO task".into();}}}
        if self.regions.need_batch_measure && !self.regions.measurement_pending{
            self.regions.need_batch_measure=false;
            if let Some(image)=&self.image{let params=self.regions.measurement_params(image,is_pro);if params.is_object(){self.regions.measurement_pending=true;self.science.request(ctx,self.generation,"measure",params);self.regions.message="Measuring…".into();}else{self.regions.message="Choose an aperture, uncertainty HDU and uncertainty type".into();}}
        }
        if !self.regions.measurement_pending{if let Some(radii)=self.regions.need_growth.take(){if let Some(image)=&self.image{let mut params=self.regions.measurement_params(image,false);if params.is_object(){params["growth_radii"]=serde_json::json!(radii);self.regions.measurement_pending=true;self.science.request_labeled(ctx,self.generation,"measure","measure.growth",params);self.regions.message="Measuring…".into();}else{self.regions.message="Choose an aperture, uncertainty HDU and uncertainty type".into();}}}}
        #[cfg(feature="pro")]
        if self.regions.need_sky_paste {
            self.regions.need_sky_paste=false;
            if let Some(image)=&self.image {match serde_json::from_str::<serde_json::Value>(&self.regions.template){Ok(template)=>self.science.request(ctx,self.generation,"region_transform",serde_json::json!({"path":image.info.path,"hdu":image.info.hdu,"template":template,"wcs_override":self.wcs_override})),Err(error)=>self.regions.message=error.to_string(),}}
        }
        if let Some(image)=&self.image{self.regions.measure_selected(image);}
        if previous_render!=self.tools.prefs.render {self.dirty=true;}
        let status=egui::TopBottomPanel::bottom("status").exact_height(40.).resizable(false).show(ctx, |ui| {
                let error=if !self.error.is_empty(){self.error.as_str()}else{self.remote.as_ref().map_or("",|remote|remote.error.as_str())};
                if !error.is_empty() {
                    ui.add(egui::Label::new(egui::RichText::new(error).color(Color32::from_rgb(255,150,130))).truncate()).on_hover_text(error);
                } else if let Some(image) = &self.image {
                    let dimensions=if image.spectrum.is_some(){format!("{} · {} {}",crate::i18n::t("Spectrum"),image.info.width,crate::i18n::t("samples"))}else{format!("{} × {} · BITPIX {}",image.info.width,image.info.height,image.info.bitpix)};
                    let details=format!("{dimensions} · {} {} · HDU {}",self.stats.finite,crate::i18n::t(if self.remote.is_some(){"valid display samples"}else{"valid values"}),image.info.hdu);
                    ui.add(egui::Label::new(&details).truncate()).on_hover_text(&details);
                } else {
                    ui.add(egui::Label::new(crate::i18n::t("Choose a FITS image or spectrum.")).truncate());
                }
                // Always reserve exactly two rows; long readouts never wrap the image viewport.
                // 始终固定两行，过长的读数截断并保留悬停详情，不挤动图像区域。
                let readout=if self.cursor.is_empty(){" ".into()}else if self.wcs_text.is_empty(){self.cursor.clone()}else if !self.wcs_disabled && self.cursor_pixel!=self.wcs_pixel{format!("{} · J2000 …",self.cursor)}else{format!("{} · {}",self.cursor,self.wcs_text)};
                ui.add(egui::Label::new(&readout).truncate()).on_hover_text(&readout);
        });
        self.status_height=status.response.rect.height();
        if self.header_open {
            let mut open=self.header_open;
            let mut windows=std::mem::take(&mut self.popup_windows);
            windows.show(ctx,window_layout::Key::Header,crate::i18n::t("FITS header"),&mut open,egui::vec2(680.,500.),|ui|self.header_ui(ui,ctx));
            self.popup_windows=windows;
            self.header_open=open;
        }
        if let Some((low,high))=self.tools.histogram_limits.take(){
            if low.is_finite() && high.is_finite() && low<high{self.low=low;self.high=high;self.limits=Limits::Manual;self.dirty=true;}
        }
        if self.dirty && self.low.is_finite() && self.high.is_finite() && self.low < self.high {
            if let Some(remote)=&mut self.remote{
                if self.scale_mapping.as_ref().is_none_or(|m|!m.matches(self.low,self.high,self.stretch,self.tools.prefs.render.stretch_exponent)){
                    self.scale_mapping=Some(fitsviewer_core::display::ScaleMapping::new(remote.preview(),self.low,self.high,self.stretch,self.tools.prefs.render.stretch_exponent));
                    if self.stretch==Stretch::HistogramEqualization{self.stretch_histogram_builds+=1;}
                }
                remote.dirty();
            }else if let Some(image) = &self.image {
                if self.scale_mapping.as_ref().is_none_or(|m|!m.matches(self.low,self.high,self.stretch,self.tools.prefs.render.stretch_exponent)){
                    self.scale_mapping=Some(fitsviewer_core::display::ScaleMapping::new(Some(image),self.low,self.high,self.stretch,self.tools.prefs.render.stretch_exponent));
                    if self.stretch==Stretch::HistogramEqualization{self.stretch_histogram_builds+=1;}
                }
                let rgba = fitsviewer_core::display::render_with_mapping(
                    image,
                    self.invert,
                    self.tone,
                    &self.tools.prefs.render,
                    self.scale_mapping.as_ref().unwrap(),
                );
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [image.info.width, image.info.height],
                    &rgba,
                );
                if let Some(texture) = &mut self.texture {
                    texture.set(color, egui::TextureOptions::NEAREST);
                } else {
                    self.texture =
                        Some(ctx.load_texture("fits", color, egui::TextureOptions::NEAREST));
                }
            }
            self.dirty = false;
        }
        self.selection.observe_view(self.low,self.high);
        self.cursor_pixel=None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(13, 17, 23)).inner_margin(egui::Margin{right:if is_pro && !self.tools.prefs.pro_collapsed(spectrum){6}else{0},..Default::default()}))
            .show(ctx, |ui| {
                if let Some(image)=&self.image {
                    if image.spectrum.is_some() {
                        let rect=ui.available_rect_before_wrap();
                        self.view_rect=[rect.min.x,rect.min.y,rect.max.x,rect.max.y];
                        #[cfg(feature="pro")]
                        {
                        let layers=if self.pro.is_some(){self.multi.as_ref().map(|multi|multi.spectrum_layers(image)).unwrap_or_default()}else{vec![]};
                        let source_style=self.multi.as_ref().filter(|_|self.pro.is_some()).map(|multi|multi.source_layer_style(image)).unwrap_or((true,1.));
                        let source_order=self.multi.as_ref().filter(|_|self.pro.is_some()).map(|multi|multi.source_layer_order(image)).unwrap_or(0);
                        let (lines,analysis)=self.pro.as_mut().map(|pro|(pro.spectral_lines.as_deref(),pro.spectral_analysis.as_deref_mut())).unwrap_or((None,None));
                        self.cursor=self.plot.show(ui,image,lines,analysis,&layers,source_style,source_order);
                        }
                        #[cfg(not(feature="pro"))]
                        {self.cursor=self.plot.show(ui,image);}
                        self.first_frame=true;
                        return;
                    }
                }
                let (response, painter) =
                    ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
                let unit=self.image.as_ref().and_then(|image|image.info.header.lines().find(|line|line.starts_with("BUNIT   ="))).and_then(|line|line.split('\'').nth(1)).unwrap_or("").trim();
                let area = self.tools.image_area(&painter,response.rect,self.low,self.high,self.stretch,self.invert,self.tone,unit,self.scale_mapping.as_ref());
                self.view_rect=[area.min.x,area.min.y,area.max.x,area.max.y];
                let center = area.center();
                if let Some(image) = &self.image {
                    let dimensions = Vec2::new(image.info.width as f32, image.info.height as f32);
                    if self.auto_fit {
                        let rotated=image_transform::ImageTransform::new(center,dimensions,1.,self.rotation_deg).bounding_size();
                        self.zoom = ((area.width() - 24.0) / rotated.x)
                            .min((area.height() - 24.0) / rotated.y)
                            .max(0.005);
                    }
                    let controls_hover=ctx.input(|input|input.pointer.interact_pos()).is_some_and(|p|p.x>area.right()-210. && p.y>area.bottom()-38.);
                    let transform=image_transform::ImageTransform::new(center+self.pan,dimensions,self.zoom,self.rotation_deg);
                    #[cfg(feature="pro")] let picking=self.pro.as_ref().is_some_and(|pro|pro.picking());
                    #[cfg(not(feature="pro"))] let picking=false;
                    if !controls_hover && (response.dragged_by(egui::PointerButton::Middle) || (response.dragged_by(egui::PointerButton::Primary) && !picking && !self.selection.active() && !self.regions.editing() && !self.regions.claims_primary_drag_transformed(ctx,transform))) {
                        self.pan += response.drag_delta();
                        self.auto_fit = false;
                    }
                    if !controls_hover && response.dragged_by(egui::PointerButton::Secondary) {
                        let delta = response.drag_delta();
                        // Relative motion avoids a jump at mouse-down. / 相对位移避免按下右键时跳变。
                        self.tone.bias = (self.tone.bias
                            - f64::from(delta.x / area.width().max(1.0)))
                        .clamp(0.0, 1.0);
                        self.tone.contrast = (self.tone.contrast
                            * f64::from(-4.0 * delta.y / area.height().max(1.0)).exp())
                        .clamp(0.05, 20.0);
                        self.dirty = true;
                        ctx.request_repaint();
                    }
                    if response.hovered() && !controls_hover {
                        let delta = ctx.input(|i| i.smooth_scroll_delta.y);
                        if delta != 0.0 {
                            if let Some(pointer) = response.hover_pos() {
                                let next = (self.zoom * (delta / 200.0).exp()).clamp(0.005, 128.0);
                                let local = (pointer - center - self.pan) / self.zoom;
                                self.pan = pointer - center - local * next;
                                self.zoom = next;
                                self.auto_fit = false;
                            }
                        }
                    }
                    if self.remote.is_some() || self.texture.is_some() {
                        let transform=image_transform::ImageTransform::new(center+self.pan,dimensions,self.zoom,self.rotation_deg);
                        let clipped=painter.with_clip_rect(area);
                        if let (Some(remote),Some(mapping))=(&mut self.remote,&self.scale_mapping){
                            #[cfg(feature="pro")]{
                                if let (Some(pro),Some(multi))=(&mut self.pro,&self.multi){
                                    let layers=multi.image_layers(image);let style=multi.source_layer_style(image);let order=multi.source_layer_order(image);
                                    let transfer=image_layers::Transfer{mapping,invert:self.invert,tone:self.tone,options:&self.tools.prefs.render,key:format!("remote:{}:{}:{}:{}:{}:{}:{}:{}:{}",image.info.path,image.info.hdu,self.low,self.high,self.stretch.name(),self.invert,self.tone.bias,self.tone.contrast,serde_json::to_string(&self.tools.prefs.render).unwrap_or_default())};
                                    let color=Color32::from_white_alpha(if style.0{(style.1.clamp(0.,1.)*255.).round() as u8}else{0});
                                    pro.image_layers.get_or_insert_with(||Box::new(image_layers::Renderer::default())).paint_progressive(ctx,&clipped,transform,order,&layers,transfer,||remote.paint(ctx,&clipped,transform,area,mapping,self.invert,self.tone,&self.tools.prefs.render,color));
                                }else{remote.paint(ctx,&clipped,transform,area,mapping,self.invert,self.tone,&self.tools.prefs.render,Color32::WHITE);}
                            }
                            #[cfg(not(feature="pro"))]remote.paint(ctx,&clipped,transform,area,mapping,self.invert,self.tone,&self.tools.prefs.render,Color32::WHITE);
                        }else if let Some(texture)=&self.texture{
                        #[cfg(feature="pro")]
                        {
                        if let (Some(pro),Some(multi),Some(mapping))=(&mut self.pro,&self.multi,&self.scale_mapping){
                            let layers=multi.image_layers(image);let source_style=multi.source_layer_style(image);let source_order=multi.source_layer_order(image);
                            let transfer=image_layers::Transfer{mapping,invert:self.invert,tone:self.tone,options:&self.tools.prefs.render,
                                key:format!("{}:{}:{:p}:{}:{}:{}:{}:{}:{}:{}",image.info.path,image.info.hdu,Arc::as_ptr(image),self.low,self.high,self.stretch.name(),self.invert,self.tone.bias,self.tone.contrast,serde_json::to_string(&self.tools.prefs.render).unwrap_or_default())};
                            pro.image_layers.get_or_insert_with(||Box::new(image_layers::Renderer::default())).paint_transformed(ctx,&clipped,texture.id(),transform,source_style,source_order,&layers,transfer);
                        }else{transform.paint_texture(&clipped,texture.id(),Color32::WHITE);}
                        }
                        #[cfg(not(feature="pro"))]
                        {transform.paint_texture(&clipped,texture.id(),Color32::WHITE);}
                        }
                        self.first_frame=self.texture.is_some() || self.remote.as_ref().is_some_and(|remote|remote.preview().is_some());
                        self.regions.canvas_transformed(ctx,&response,&clipped,transform,image,self.tools.prefs.mode=="PRO",!picking && !self.selection.active() && !controls_hover);
                        #[cfg(feature="pro")] if let Some(pro)=&mut self.pro{pro.image_interaction(ctx,&response,&clipped,transform,!controls_hover && !self.selection.active());}
                        self.selection.canvas(ctx,&response,&clipped,transform,!controls_hover);
                        image_display::paint(&clipped,area,transform,&self.tools.prefs,self.wcs_result.as_ref());
                        #[cfg(feature="pro")]
if let Some(pro)=&mut self.pro{pro.overlay(&clipped,transform,self.wcs_result.as_ref(),response.hover_pos());}
                        if let Some(pos) = response.hover_pos() {
                            let xy=transform.screen_to_pixel(pos);
                            let x=(xy[0]+0.5).floor() as isize;
                            let source_y=(xy[1]+0.5).floor() as isize;
                            if x >= 0 && source_y >= 0 && x < image.info.width as isize && source_y < image.info.height as isize {
                                let y=source_y as usize;
                                let i = y * image.info.width + x as usize;
                                self.cursor_pixel=Some([x as f64,y as f64]);
                                self.cursor = if let Some(remote)=&mut self.remote{remote.cursor(ctx,x as usize,y)}else{format!(
                                    "x={}  y={}  ·  value={}  ·  raw={}",
                                    x + 1,
                                    y + 1,
                                    image
                                        .physical(i)
                                        .map(|v| format!("{v:.8}"))
                                        .unwrap_or_else(|| "invalid".into()),
                                    image.pixels.raw_string(i)
                                )};
                            } else {
                                self.cursor.clear();
                            }
                        }
                    }
                    if image_display::rotation_controls(ui,area,&mut self.rotation_deg){ctx.request_repaint();}
                } else {
                    painter.text(
                        center,
                        egui::Align2::CENTER_CENTER,
                        if self.loading {
                            crate::i18n::t("Loading FITS…")
                        } else {
                            crate::i18n::t("Open a FITS image")
                        },
                        egui::FontId::proportional(23.0),
                        Color32::from_gray(155),
                    );
                }
            });
        // Capture after every popup (including Header), without restoring scientific
        // state. A settings import from this frame must win over old observations.
        // 所有弹窗（包括文件头）绘制后采样；当前帧导入优先，不恢复科学任务。
        self.remember_popup_windows();
        if previous_preferences!=serde_json::to_string(&self.tools.prefs).unwrap_or_default(){
            if let Err(error)=controls::save(&self.tools.prefs){self.tools.message=error;}
        }
        self.request_wcs(ctx);
        if self.scientific_paths()!=previous_document_refs{self.refresh_document_refs(ctx);}else{self.scientific_heartbeat(ctx);}

        #[cfg(target_arch = "wasm32")]
        self.export_state(ctx);
        #[cfg(not(target_arch = "wasm32"))]
        self.native_diagnostics(ctx);
    }
}

#[cfg(all(target_arch="wasm32",feature="offline"))]
thread_local!{
    static WEB_OFFLINE_SOURCE:std::cell::RefCell<offline_sources::Store>=std::cell::RefCell::new(offline_sources::Store::default());
    static WEB_OFFLINE_IMAGE:std::cell::RefCell<Option<(String,Vec<u8>,FitsImage)>>=const{std::cell::RefCell::new(None)};
}
#[cfg(all(target_arch="wasm32",feature="offline"))]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn open_file(name:String,bytes:Vec<u8>)->Result<(),wasm_bindgen::JsValue>{
    let key=WEB_OFFLINE_SOURCE.with(|slot|slot.borrow_mut().key(&name));
    let image=fitsviewer_core::load_bytes_with_options(&bytes,&key,LoadOptions::default()).map_err(|e|wasm_bindgen::JsValue::from_str(&e))?;
    WEB_OFFLINE_IMAGE.with(|slot|*slot.borrow_mut()=Some((key,bytes,image)));
    WEB_CONTEXT.with(|slot|{if let Some(ctx)=slot.borrow().as_ref(){ctx.request_repaint();}});Ok(())
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    // Own the renderer so a closing document can release it explicitly.
    // 保留渲染器所有权，关闭文件页时主动释放图像、纹理和事件回调。
    static WEB_RUNNER: std::cell::RefCell<Option<eframe::WebRunner>> = const { std::cell::RefCell::new(None) };
    static WEB_COMMANDS: std::cell::RefCell<Vec<serde_json::Value>> = const {std::cell::RefCell::new(Vec::new())};
    static WEB_CONTEXT:std::cell::RefCell<Option<egui::Context>>=const{std::cell::RefCell::new(None)};
}

#[cfg(target_arch="wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn command(text:String)->Result<(),wasm_bindgen::JsValue>{
    if text.len()>1024*1024{return Err("Command exceeds 1 MiB".into());}
    let value:serde_json::Value=serde_json::from_str(&text).map_err(|e|wasm_bindgen::JsValue::from_str(&e.to_string()))?;
    WEB_COMMANDS.with(|slot|{let mut q=slot.borrow_mut();if q.len()<32{q.push(value);}});
    WEB_CONTEXT.with(|slot|{if let Some(ctx)=slot.borrow().as_ref(){ctx.request_repaint();}});Ok(())
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn stop() {
    #[cfg(feature="offline")]
    {WEB_OFFLINE_SOURCE.with(|slot|slot.borrow_mut().clear());WEB_OFFLINE_IMAGE.with(|slot|*slot.borrow_mut()=None);}
    WEB_COMMANDS.with(|slot|slot.borrow_mut().clear());WEB_CONTEXT.with(|slot|*slot.borrow_mut()=None);
    WEB_RUNNER.with(|slot| {
        if let Some(runner) = slot.borrow_mut().take() {
            runner.destroy();
        }
    });
}

// Resolve explicit embedding/session mode before the first frame can acquire PRO.
// 在第一帧之前解析嵌入页面或会话模式，避免短暂创建不需要的 PRO 工作区。
#[cfg(any(target_arch = "wasm32", test))]
fn resolve_startup_mode(explicit:Option<&str>,query:&str)->Result<Option<String>,String>{
    let mode=if let Some(mode)=explicit {Some(mode.to_owned())}else{
        query.trim_start_matches('?').split('&').find_map(|part|{
            let (key,value)=part.split_once('=').unwrap_or((part,""));
            (key=="mode").then_some(value)
        }).map(|value|urlencoding::decode(value).map(|mode|mode.into_owned()).map_err(|_|"Unknown UI mode".to_owned())).transpose()?
    };
    if mode.as_deref().is_some_and(|mode|!["Minimal","PRO"].contains(&mode)){return Err("Unknown UI mode".into());}
    #[cfg(not(feature="pro"))] if mode.as_deref()==Some("PRO"){return Err("PRO is not included in this Mini build".into());}
    Ok(mode)
}

#[cfg(test)]
mod display_interaction_tests{
    use super::*;
    use fitsviewer_core::regions::{Aperture,Shape,measure,MeasureOptions};
    use serde_json::json;
    // Controlled state fixture; scientific acceptance uses real FITS separately.
    // 此处仅验证交互与异步状态，真实科学数据另行验收。
    fn app()->FitsApp{
        FitsApp::from_image(FitsImage{info:fitsviewer_core::ImageInfo{path:"display-state-fixture".into(),hdu:0,width:64,height:64,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:None},pixels:fitsviewer_core::RawPixels::F64(vec![1.;4096]),spectrum:None})
    }
    #[test]
    fn clearing_apertures_rejects_a_measurement_already_in_flight(){
        let ctx=egui::Context::default();let mut app=app();
        let aperture=Aperture{name:"source".into(),shape:Shape::Circle{center:[25.,25.],radius:3.},background:None,exclude:vec![]};
        app.regions.add_aperture(aperture.clone());
        let measurement=measure(app.image.as_ref().unwrap(),&aperture,&MeasureOptions::default(),None).unwrap();
        let old_revision=app.regions.revision;let defaults=(app.regions.radius,app.regions.inner,app.regions.outer);
        app.regions.measurement_pending=true;app.regions.clear_all();
        app.science.replies.lock().unwrap().push(science_client::Reply{generation:app.generation,operation:"measure".into(),result:Ok(json!({"revision":old_revision,"indices":[0],"measurements":[measurement]}))});
        app.science_updates(&ctx);
        assert!(app.regions.apertures.is_empty());assert!(app.regions.results.is_empty());assert!(app.regions.growth.is_empty());
        assert_eq!(app.regions.selected,None);assert!(!app.regions.measurement_pending);assert!(!app.regions.needs_measure);
        assert_eq!((app.regions.radius,app.regions.inner,app.regions.outer),defaults);
    }
    #[test]
    fn rotated_view_exports_inverse_polygon_without_replacing_samples(){
        let mut app=app();let image=app.image.as_ref().unwrap().clone();app.view_rect=[20.,40.,420.,340.];app.zoom=5.;app.pan=egui::vec2(13.,-7.);app.rotation_deg=37.;
        let view=app.view_configuration();let transform=image_transform::ImageTransform::new(egui::pos2(220.,190.)+app.pan,egui::vec2(64.,64.),5.,37.);
        let polygon:Vec<[f64;2]>=serde_json::from_value(view["viewport_polygon"].clone()).unwrap();
        for (pixel,screen) in polygon.iter().zip([egui::pos2(20.,40.),egui::pos2(420.,40.),egui::pos2(420.,340.),egui::pos2(20.,340.)]){assert!(transform.pixel_to_screen(*pixel).distance(screen)<0.001);}
        assert_eq!(view["rotation_deg"],37.);assert!(Arc::ptr_eq(&image,app.image.as_ref().unwrap()));assert_eq!(image.physical(0),Some(1.));
    }
}

#[cfg(test)]
mod startup_mode_tests{
    use super::resolve_startup_mode;
    #[test]fn explicit_mode_is_available_before_first_frame(){
        assert_eq!(resolve_startup_mode(Some("Minimal"),"?mode=PRO").unwrap().as_deref(),Some("Minimal"));
        assert_eq!(resolve_startup_mode(None,"?path=x&mode=Min%69mal").unwrap().as_deref(),Some("Minimal"));
        #[cfg(feature="pro")] assert_eq!(resolve_startup_mode(None,"?mode=PRO").unwrap().as_deref(),Some("PRO"));
        assert_eq!(resolve_startup_mode(None,"?path=x").unwrap(),None);
        assert!(resolve_startup_mode(Some("invalid"),"").is_err());
        assert!(resolve_startup_mode(None,"?mode=invalid").is_err());
    }
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn start(
    canvas: web_sys::HtmlCanvasElement,
    api: String,
    path: String,
    token: String,
    hdu: Option<usize>,
    cjk_font: Vec<u8>,
    initial_mode: Option<String>,
) -> Result<(), wasm_bindgen::JsValue> {
    stop();
    let query=web_sys::window().and_then(|window|window.location().search().ok()).unwrap_or_default();
    let initial_mode=resolve_startup_mode(initial_mode.as_deref(),&query).map_err(|error|wasm_bindgen::JsValue::from_str(&error))?;
    let runner = eframe::WebRunner::new();
    runner
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |cc| {
                WEB_CONTEXT.with(|slot|*slot.borrow_mut()=Some(cc.egui_ctx.clone()));
                cc.egui_ctx.set_visuals(egui::Visuals::dark());
                install_cjk_font(&cc.egui_ctx, cjk_font);
                let mut app=FitsApp::new(path, (!api.is_empty()).then_some(api), token, hdu);
                app.single_source=web_sys::window().and_then(|window|js_sys::Reflect::get(&window,&"__fitsviewerSingleSource".into()).ok()).and_then(|value|value.as_bool()).unwrap_or(false);
                if let Some(mode)=initial_mode {app.tools.prefs.mode=mode;}
                Ok(Box::new(app))
            }),
        )
        .await?;
    WEB_RUNNER.with(|slot| *slot.borrow_mut() = Some(runner));
    Ok(())
}

#[cfg(not(feature="pro"))]
fn mini_aperture(value:&serde_json::Value)->Result<(),String>{
    if value["shape"]["kind"]!="circle" || value["background"].as_object().is_some_and(|_|value["background"]["kind"]!="ring") || value["exclude"].as_array().is_some_and(|v|!v.is_empty()) {return Err("Mini supports circular apertures with an optional sky ring".into());}Ok(())
}
#[cfg(all(test,not(feature="pro")))]
mod mini_build_tests{
 use super::*;
 #[test]fn pro_cannot_enter_by_url_preferences_or_session(){
  assert!(resolve_startup_mode(Some("PRO"),"").is_err());
  assert!(resolve_startup_mode(None,"?mode=PRO").is_err());
  assert!(controls::Preferences::parse(r#"{"mode":"PRO"}"#).is_err());
  assert!(controls::Preferences::parse(r#"{"default_mode":"PRO"}"#).is_err());
  let app=FitsApp::new(String::new(),None,String::new(),None);
  assert!(app.with_session(serde_json::json!({"views":[{"path":"a","view":{}}],"mode":"PRO"})).is_err());
 }
 #[test]fn pro_commands_are_not_compiled(){
  let mut app=FitsApp::new(String::new(),None,String::new(),None);let ctx=egui::Context::default();
  for operation in ["pro.run","workspace.add","spectral.lines","spectral.analysis","layer.configure","image.match_flux","annotation.set","aperture.template_load"]{assert!(app.command(&ctx,serde_json::json!({"operation":operation,"params":{}})).is_err(),"{operation}");}
 }
}
