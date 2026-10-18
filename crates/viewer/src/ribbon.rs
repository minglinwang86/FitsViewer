//! Compact tabbed ribbon; actions reuse the existing viewer and scientific tools.
//! 紧凑标签工具栏；复用查看器与科学工具，不在切换标签时执行任务。
use eframe::egui::{self, Color32, Vec2};
use fitsviewer_core::{DisplayTone, Limits, Stretch};
use crate::{FitsApp, i18n::t, regions_ui::Tool};

const BASIC_TABS: [&str; 3] = ["View", "Measure", "Coordinates"];
#[cfg(feature="pro")]
type RibbonAction=crate::multiview::Action;
#[cfg(not(feature="pro"))]
type RibbonAction=();
const PRO_TABS: [&str; 3] = ["Analysis", "Catalogues", "Export"];

// Task buttons share the normal control font; compact padding keeps them together.
// 任务按钮与其余控件共用普通字号；通过紧凑边距保持同行，不缩小文字。
fn image_task_tabs(ui:&mut egui::Ui,active:&mut String,pro:bool)->(bool,bool){
    let tabs=BASIC_TABS.into_iter().chain(PRO_TABS.into_iter().filter(|_|pro));
    let mut changed=false;let mut collapsed=false;
    ui.horizontal(|ui|{
        ui.spacing_mut().button_padding.x=4.;ui.spacing_mut().item_spacing.x=3.;
        for tab in tabs{
            let response=ui.add(egui::Button::selectable(active.as_str()==tab,t(tab))
                .wrap_mode(egui::TextWrapMode::Extend)
                .min_size(Vec2::new(0.,ui.spacing().interact_size.y)));
            if response.clicked(){*active=tab.into();changed=true;}
            if response.double_clicked(){collapsed=true;}
        }
    });
    (changed,collapsed)
}

#[cfg(all(test,feature="pro"))]
mod compact_spectrum_tests{
    use super::*;
    fn app(pro:bool)->FitsApp{
        let meta=fitsviewer_core::SpectrumMeta{x_label:"Wavelength".into(),x_unit:"Angstrom".into(),y_label:"Flux".into(),y_unit:String::new(),columns:vec!["wave".into(),"flux".into()],x_column:0,y_column:1,error_column:None,error_hdu:None,uncertainty_kind:String::new(),note:"Source units retained".into()};
        let image=fitsviewer_core::FitsImage{info:fitsviewer_core::ImageInfo{path:"spec-0751-52251-0160.fits".into(),hdu:1,width:2,height:1,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:Some(meta)},pixels:fitsviewer_core::RawPixels::F64(vec![1.,2.]),spectrum:Some(fitsviewer_core::SpectrumData{x:vec![4000.,5000.],error:None})};
        let mut app=FitsApp::from_image(image);app.tools.prefs=crate::controls::Preferences::default();
        if pro{
            app.tools.prefs.mode="PRO".into();app.api=Some("http://127.0.0.1".into());app.pro=Some(crate::pro::Workspace::default());
            app.multi=Some(crate::multiview::Workspace::default());app.multi.as_mut().unwrap().add_paths(&[app.path.clone()]);
        }app
    }
    #[test]fn spectrum_header_wraps_only_used_controls_on_narrow_windows(){
        for language in ["en","zh-CN"]{crate::i18n::set(language);
            for width in [650.,850.,1150.,1600.]{let mut heights=vec![];
                for pro in [false,true]{let mut app=app(pro);let ctx=egui::Context::default();
                    let _=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(width,850.))),..Default::default()},|ctx|{
                        let _=app.ribbon(ctx);
                        assert!(app.ribbon_height<=if pro && width<850.{140.}else if pro{108.}else{80.},"width={width}, language={language}, pro={pro}, height={}",app.ribbon_height);
                        assert!((ctx.available_rect().top()-app.ribbon_height).abs()<1.,"A second top panel consumed space");
                    });heights.push(app.ribbon_height);
                }assert!(heights[1]-heights[0]>=18. && heights[1]-heights[0]<=if width<850.{68.}else{36.},"Expected only the used PRO command rows: {heights:?}");
            }
        }crate::i18n::set("en");
    }
    #[test]fn rejected_analysis_command_leaves_units_and_parameters_unchanged(){
        let mut app=app(true);let ctx=egui::Context::default();app.pro.as_mut().unwrap().sync_spectrum(app.image.as_deref());
        let before=app.pro.as_ref().unwrap().spectral_analysis.as_ref().unwrap().summary();
        let invalid=serde_json::json!({"operation":"spectral.analysis","params":{"unit":"nm","parameters":{"region":[2.,1.]}}});
        assert!(app.command(&ctx,invalid).is_err());
        assert_eq!(app.pro.as_ref().unwrap().spectral_lines.as_ref().unwrap().unit,"Angstrom");
        assert_eq!(app.pro.as_ref().unwrap().spectral_analysis.as_ref().unwrap().summary(),before);
        app.pro.as_mut().unwrap().busy=true;
        assert!(app.command(&ctx,serde_json::json!({"operation":"spectral.analysis","params":{"unit":"nm","action":"continuum"}})).is_err());
        assert_eq!(app.pro.as_ref().unwrap().spectral_lines.as_ref().unwrap().unit,"Angstrom");
    }

    #[test]fn wrapped_buttons_and_dropdowns_share_height_and_center(){
        for language in ["en","zh-CN"]{crate::i18n::set(language);
            let ctx=egui::Context::default();let mut rects=vec![];
            crate::install_cjk_font(&ctx,include_bytes!("../../../web/fonts/cjk.ttf").to_vec());
            let _=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(950.,300.))),..Default::default()},|ctx|{
                egui::CentralPanel::default().show(ctx,|ui|{align_controls(ui);ui.horizontal_wrapped(|ui|{
                    rects.push(ui.button(t("Auto display")).rect);
                    rects.push(egui::ComboBox::from_id_salt("height-limits").selected_text(t("Manual")).show_ui(ui,|ui|{ui.label("item");}).response.rect);
                    rects.push(egui::ComboBox::from_id_salt("height-stretch").selected_text(t("Asinh")).show_ui(ui,|ui|{ui.label("item");}).response.rect);
                    rects.push(ui.menu_button(t("Display parameters"),|ui|{ui.label("item");}).response.rect);
                });});
            });
            for rect in &rects[1..]{assert!((rect.height()-rects[0].height()).abs()<0.1,"{language}: {rects:?}");assert!((rect.center().y-rects[0].center().y).abs()<0.1,"{language}: {rects:?}");}
        }crate::i18n::set("en");
    }

    #[test]fn image_hdu_uses_command_row_and_display_groups_have_no_fixed_gap(){
        for language in ["en","zh-CN"]{crate::i18n::set(language);
            for pro in [false,true]{
                let mut app=FitsApp::new(String::new(),Some("http://127.0.0.1".into()),String::new(),None);
                app.tools.prefs=crate::controls::Preferences::default();app.tools.prefs.language=language.into();
                if pro{app.tools.prefs.mode="PRO".into();}
                let ctx=egui::Context::default();let mut labels=vec![];
                crate::install_cjk_font(&ctx,include_bytes!("../../../web/fonts/cjk.ttf").to_vec());
                fn collect(shape:&egui::epaint::Shape,labels:&mut Vec<(String,egui::Rect)>){match shape{
                    egui::epaint::Shape::Text(text)=>labels.push((text.galley.job.text.clone(),egui::Rect::from_min_size(text.pos,text.galley.size()))),
                    egui::epaint::Shape::Vec(shapes)=>for shape in shapes{collect(shape,labels);},_=>{},
                }}
                for _ in 0..3{
                    let output=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1600.,900.))),..Default::default()},|ctx|{app.ribbon(ctx);});
                    labels.clear();for shape in &output.shapes{collect(&shape.shape,&mut labels);}
                }
                let text_rect=|label:&str|{labels.iter().find(|(text,_)|text==label).map(|(_,rect)|*rect)
                    .unwrap_or_else(||panic!("Missing {label} in {language}, PRO={pro}: {labels:?}"))};
                let hdu=text_rect("HDU");let auto=text_rect(t("Auto display"));let inspect=text_rect(t("Histogram"));
                assert!(hdu.center().y+10.<auto.center().y,"HDU must move above display controls");
                if pro{assert!((inspect.center().y-auto.center().y).abs()<2.);}else{assert!((inspect.center().y-hdu.center().y).abs()<2.);}
                let parameters=text_rect(t("Display parameters"));let fit=text_rect(t("Fit"));
                assert!(fit.left()-parameters.right()<120.,"Fixed display/zoom padding remains: {language}, PRO={pro}");
            }
        }crate::i18n::set("en");
    }

    #[test]fn file_actions_share_header_row_while_unicode_path_shrinks(){
        for language in ["en","zh-CN"]{crate::i18n::set(language);
            for width in [650.,850.,1600.]{for pro in [false,true]{
                let ctx=egui::Context::default();crate::install_cjk_font(&ctx,include_bytes!("../../../web/fonts/cjk.ttf").to_vec());
                let full=format!("/workspace/{}/科学图像.fits","中文目录/".repeat(40));
                let mut app=FitsApp::new(full.clone(),None,String::new(),None);
                app.tools.prefs=crate::controls::Preferences::default();if pro{app.tools.prefs.mode="PRO".into();}
                let output=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(width,300.))),..Default::default()},|ctx|{
                    egui::TopBottomPanel::top("file-row-test").show(ctx,|ui|{align_controls(ui);app.ribbon_file(ui,ctx);});
                });
                let text_rect=|label:&str|output.shapes.iter().find_map(|shape|if let egui::epaint::Shape::Text(text)=&shape.shape{
                    (text.galley.job.text==t(label)).then_some(egui::Rect::from_min_size(text.pos,text.galley.size()))
                }else{None}).unwrap_or_else(||panic!("Missing {label}: {language}, width={width}, PRO={pro}"));
                let header=text_rect("Header");let settings=text_rect("Settings");let open=text_rect("Open");
                assert!(open.right()<header.left() && header.right()<settings.left());
                for label in ["Open","Header","Settings"].into_iter().chain(pro.then_some("Work area")){
                    let rect=text_rect(label);
                    assert!((rect.center().y-header.center().y).abs()<1. && rect.right()<width-3.,"{label}: {language}, width={width}, PRO={pro}, {rect:?}");
                }
                assert_eq!(app.path,full,"A shortened caption must not replace the path");
            }}
        }
        let ctx=egui::Context::default();crate::install_cjk_font(&ctx,include_bytes!("../../../web/fonts/cjk.ttf").to_vec());
        let full=format!("/workspace/{}/科学图像.fits", "中文目录/".repeat(40));let mut path=full.clone();let mut id=None;
        let _=ctx.run(egui::RawInput::default(),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{
            let caption=path_caption(ui,&path,220.);assert!(caption.starts_with('…'));assert!(caption.ends_with("科学图像.fits"));
            let response=path_field(ui,&mut path,220.);id=Some(response.id);
        });});assert_eq!(path,full);
        ctx.memory_mut(|memory|memory.request_focus(id.unwrap()));
        let pasted="C:\\科学数据\\新图像.fits";let modifiers=egui::Modifiers{ctrl:true,command:true,..Default::default()};
        let _=ctx.run(egui::RawInput{events:vec![egui::Event::Key{key:egui::Key::A,physical_key:None,pressed:true,repeat:false,modifiers},egui::Event::Paste(pasted.into())],..Default::default()},|ctx|{
            egui::CentralPanel::default().show(ctx,|ui|{path_field(ui,&mut path,220.);});
        });assert_eq!(path,pasted,"Editing must retain the complete Unicode clipboard path");crate::i18n::set("en");
    }

    #[test]fn image_and_multiview_rows_share_vertical_spacing(){
        for language in ["en","zh-CN"]{crate::i18n::set(language);
            let ctx=egui::Context::default();crate::install_cjk_font(&ctx,include_bytes!("../../../web/fonts/cjk.ttf").to_vec());
            let mut app=FitsApp::new(String::new(),Some("http://127.0.0.1".into()),String::new(),None);
            app.tools.prefs=crate::controls::Preferences::default();app.tools.prefs.mode="PRO".into();app.multi=Some(crate::multiview::Workspace::default());let mut labels=vec![];
            fn collect(shape:&egui::epaint::Shape,labels:&mut Vec<(String,f32)>){match shape{
                egui::epaint::Shape::Text(text)=>labels.push((text.galley.job.text.clone(),text.pos.y+text.galley.size().y/2.)),
                egui::epaint::Shape::Vec(shapes)=>for shape in shapes{collect(shape,labels);},_=>{},
            }}
            for _ in 0..3{let output=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(1600.,900.))),..Default::default()},|ctx|{app.ribbon(ctx);app.multi.as_mut().unwrap().toolbar(ctx);});
                labels.clear();for shape in &output.shapes{collect(&shape.shape,&mut labels);}
            }
            let centers=["Open","HDU","Auto display","Add files…"].map(|label|labels.iter().find(|(text,_)|text==t(label)).map(|(_,y)|*y).unwrap());
            let comparison=labels.iter().find(|(text,_)|text==t("Secondary view")).unwrap().1;
            assert!((comparison-centers[2]).abs()<1.,"Comparison controls should share the options row: {language}");
            let gaps=centers.windows(2).map(|v|v[1]-v[0]).collect::<Vec<_>>();
            assert!(gaps.iter().all(|gap|(*gap-gaps[0]).abs()<1.),"{language}: row gaps {gaps:?}");
        }crate::i18n::set("en");
    }

    #[test]fn narrow_image_ribbon_keeps_task_names_and_tools_reachable(){
        fn collect(shape:&egui::epaint::Shape,labels:&mut Vec<(String,egui::Rect)>){match shape{
            egui::epaint::Shape::Text(text)=>labels.push((text.galley.job.text.clone(),egui::Rect::from_min_size(text.pos,text.galley.size()))),
            egui::epaint::Shape::Vec(shapes)=>for shape in shapes{collect(shape,labels);},_=>{},
        }}
        for language in ["en","zh-CN"]{crate::i18n::set(language);
            for theme in ["Dark","Light"]{for width in [650.,850.]{for pro in [false,true]{
                for tab in BASIC_TABS.into_iter().chain(PRO_TABS.into_iter().filter(|_|pro)){
                    let ctx=egui::Context::default();crate::install_cjk_font(&ctx,include_bytes!("../../../web/fonts/cjk.ttf").to_vec());
                    let mut app=FitsApp::new(String::new(),Some("http://127.0.0.1".into()),String::new(),None);
                    app.tools.prefs=crate::controls::Preferences::default();app.tools.prefs.ribbon_tab=tab.into();
                    if pro{app.tools.prefs.mode="PRO".into();app.pro=Some(crate::pro::Workspace::default());}
                    let mut labels=vec![];
                    for frame in 0..3{let output=ctx.run(egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(width,850.))),..Default::default()},|ctx|{
                        if frame==0{crate::ui_style::apply_theme(ctx,theme);}app.ribbon(ctx);
                    });
                        labels.clear();for shape in &output.shapes{collect(&shape.shape,&mut labels);}
                    }
                    let rect=|key:&str|labels.iter().find(|(text,_)|text==t(key)).map(|(_,rect)|*rect)
                        .unwrap_or_else(||panic!("Missing {key}: {language}, {theme}, width={width}, PRO={pro}, tab={tab}"));
                    let task_y=rect("View").center().y;
                    for key in BASIC_TABS.into_iter().chain(PRO_TABS.into_iter().filter(|_|pro)){
                        let bounds=rect(key);
                        assert!((bounds.center().y-task_y).abs()<1. && bounds.left()>=0. && bounds.right()<width,
                            "Task hidden or wrapped: {key}, {language}, {theme}, width={width}, {bounds:?}");
                    }
                    if pro{
                        let layers=rect("Layers");
                        assert!(layers.left()>=0. && layers.right()<width,"Layers shortcut is clipped: {language}, width={width}, {layers:?}");
                        if width>=850.{assert!((layers.center().y-task_y).abs()<1.,"Layers should share the task/mode row at 850px: {language}, {layers:?}");}
                    }
                    let tools:&[&str]=match tab{
                        "Measure"=>&["Pan","Circle","Aperture photometry"],
                        "Coordinates"=>&["Gaia coordinate recognition / calibration"],
                        "Analysis"=>&["Layers","Find stars","Local source candidates","Detect straight traces","Track seeded curve","Line / trace profile"],
                        "Catalogues"=>&["Catalogue overlay","Local XMatch","CDS XMatch","Nearby observations","Download selected resource","SAMP"],
                        "Export"=>&["Figure export","MathText preview","Reprojection","RGB composition"],
                        _=>&["Auto display","Display parameters","Fit","Histogram"],
                    };
                    for key in tools{let bounds=rect(key);assert!(bounds.left()>=0. && bounds.right()<width && bounds.bottom()<=app.ribbon_height,
                        "Tool clipped: {key}, {language}, {theme}, width={width}, PRO={pro}, tab={tab}, {bounds:?}");}
                    assert_eq!(labels.iter().filter(|(text,_)|text==t("Settings")).count(),1,"Settings belongs only to the file row");
                    assert!(!labels.iter().any(|(text,_)|[t("Data"),t("Coordinate tools"),t("Move aperture"),t("Selected environment")].contains(&text.as_str())));
                }
            }}}
        }crate::i18n::set("en");
    }

}

pub(crate) fn align_controls(ui:&mut egui::Ui){
    crate::ui_style::align_controls(ui);
}
// Overflow hides directories before the filename; the editable path remains complete.
// 溢出时先缩略左侧目录；完整路径仍用于编辑和打开。
pub(crate) fn path_caption(ui:&egui::Ui,path:&str,width:f32)->String{
    let font=egui::TextStyle::Body.resolve(ui.style());let color=ui.visuals().text_color();
    let text_width=|text:&str|ui.fonts(|fonts|fonts.layout_no_wrap(text.into(),font.clone(),color).size().x);
    if text_width(path)<=width{return path.into();}
    let offsets=path.char_indices().map(|(index,_)|index).chain(std::iter::once(path.len())).collect::<Vec<_>>();
    let (mut low,mut high)=(0,offsets.len()-1);
    while low<high{let middle=(low+high)/2;if text_width(&format!("…{}",&path[offsets[middle]..]))<=width{high=middle;}else{low=middle+1;}}
    format!("…{}",&path[offsets[low]..])
}

fn path_field(ui:&mut egui::Ui,path:&mut String,width:f32)->egui::Response{
    let id=ui.make_persistent_id("file-path");let editing=ui.memory(|memory|memory.has_focus(id));
    let color=ui.visuals().widgets.inactive.text_color();let height=ui.spacing().interact_size.y;
    let response=ui.add(egui::TextEdit::singleline(path).id(id).font(egui::TextStyle::Body)
        .desired_width((width-8.).max(24.)).min_size(Vec2::new(0.,height)).vertical_align(egui::Align::Center)
        .text_color(if editing{color}else{Color32::TRANSPARENT}).hint_text(t("FITS or spectrum file path")));
    if !editing && !path.is_empty(){
        let rect=response.rect.shrink2(Vec2::new(4.,0.));let caption=path_caption(ui,path,rect.width());
        ui.painter().with_clip_rect(rect.intersect(ui.clip_rect())).text(rect.left_center(),egui::Align2::LEFT_CENTER,caption,egui::TextStyle::Body.resolve(ui.style()),color);
    }
    response.on_hover_text(path.as_str())
}

impl FitsApp {
    pub(crate) fn ribbon(&mut self, ctx: &egui::Context)->Option<RibbonAction> {
        let allow_pro = cfg!(feature="pro") && (self.api.is_some() || self.science.local.is_some());
        if !allow_pro { self.tools.prefs.mode = "Minimal".into(); }
        let spectrum=self.image.as_ref().is_some_and(|image|image.spectrum.is_some());
        if spectrum && ["Measure","Coordinates"].contains(&self.tools.prefs.ribbon_tab.as_str()){self.tools.prefs.ribbon_tab="View".into();}
        if self.tools.prefs.mode != "PRO" && !BASIC_TABS.contains(&self.tools.prefs.ribbon_tab.as_str()) {
            self.tools.prefs.ribbon_tab = "View".into();
        }
        if spectrum{return self.spectrum_ribbon(ctx,allow_pro);}
        #[allow(unused_mut)] let mut multi_action=None;
        let panel = egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6., 3.);
            ui.spacing_mut().button_padding = Vec2::new(7., 4.);
            align_controls(ui);
            self.ribbon_file(ui, ctx);
            ui.horizontal_wrapped(|ui| {
                let pro=self.tools.prefs.mode=="PRO";
                let (changed,collapsed)=image_task_tabs(ui,&mut self.tools.prefs.ribbon_tab,pro);
                if changed{self.tools.prefs.ribbon_collapsed=false;if pro && PRO_TABS.contains(&self.tools.prefs.ribbon_tab.as_str()){self.tools.prefs.set_pro_collapsed(false,false);}}
                if collapsed{self.tools.prefs.ribbon_collapsed=true;}
                ui.separator();
                self.tools.toolbar(ui, allow_pro);
                #[cfg(feature="pro")]
                if self.tools.prefs.mode=="PRO" && ui.add(egui::Button::new(t("Layers")).wrap_mode(egui::TextWrapMode::Extend)).clicked(){
                    self.tools.prefs.set_pro_collapsed(false,false);if let Some(pro)=&mut self.pro{pro.select("image_layers");}
                }
                ui.menu_button("…", |ui| {
                    ui.set_max_width(430.);ui.strong(t("Help"));
                    ui.label(t(if spectrum{"Wheel: zoom · Drag: pan"}else{"Wheel: zoom · Left/middle drag: pan · Right drag: B/C · R: reset · 1–8: stretch · Z: zscale · F: fit"}));
                    ui.label(t("Ctrl+O: browse · Paste a path and press Enter to open"));
                    ui.label(t("Choose a tab to expand; double-click a tab to collapse."));
                }).response.on_hover_text(t("Help"));
                let label = if self.tools.prefs.ribbon_collapsed { "+" } else { "−" };
                if ui.button(label).on_hover_text(t(if self.tools.prefs.ribbon_collapsed {"Expand ribbon"} else {"Collapse ribbon"})).clicked() {
                    self.tools.prefs.ribbon_collapsed = !self.tools.prefs.ribbon_collapsed;
                }
                ui.separator();self.ribbon_hdu(ui,ctx);
                if self.tools.prefs.mode!="PRO"{
                    ui.separator();self.ribbon_inspect(ui);
                }
            });
            // Switching away from PRO follows the existing acquire/release lifecycle below.
            // 切回极简时沿用主界面原有的工作区创建与释放流程。
            if self.tools.prefs.mode != "PRO" && !BASIC_TABS.contains(&self.tools.prefs.ribbon_tab.as_str()) {
                self.tools.prefs.ribbon_tab = "View".into();
            }
            let row_right=ui.max_rect().right();
            if self.tools.prefs.ribbon_collapsed {
                ui.horizontal_wrapped(|ui| {
                    let mut action = None;
                    if !self.tools.prefs.toolbar.iter().any(|name|name=="Auto display") && ui.add_enabled(self.image.as_ref().is_some_and(|image|image.spectrum.is_none()) && self.stats.finite>0,egui::Button::new(t("Auto display"))).on_hover_text(t("Automatic sampled limits and stretch; reset brightness/contrast. Display only.")).clicked(){action=Some("Auto display".into());}
                    for name in &self.tools.prefs.toolbar {
                        if spectrum && !["Fit","Header"].contains(&name.as_str()){continue;}
                        if ui.button(t(name)).clicked() { action = Some(name.clone()); }
                    }
                    if let Some(action) = action { self.action(&action, ctx); }
                    #[cfg(feature="pro")]
                    {multi_action=self.ribbon_comparison(ui,row_right);}
                });
            } else {
                let y=ui.cursor().top()-ui.spacing().item_spacing.y/2.;
                ui.painter().hline(ui.max_rect().x_range(),y,ui.visuals().widgets.noninteractive.bg_stroke);
                // Allocate only the controls being shown; wrap actual tools when needed.
                // 只分配实际控件的空间；宽度不足时让工具自然换行，不预留空行。
                ui.horizontal_wrapped(|ui| {
                    match self.tools.prefs.ribbon_tab.as_str() {
                        "Measure" => self.ribbon_measure(ui),
                        "Coordinates" => self.ribbon_coordinates(ui),
                        #[cfg(feature="pro")]
                        "Analysis" | "Catalogues" | "Export" => self.ribbon_pro(ui),
                        _ => self.ribbon_view(ui, ctx),
                    }
                    #[cfg(feature="pro")]
                    {multi_action=self.ribbon_comparison(ui,row_right);}
                });
            }
        });
        self.remember_ribbon_height(ctx,panel.response.rect.height());
        multi_action
    }

    #[cfg(feature="pro")]
    fn ribbon_comparison(&mut self,ui:&mut egui::Ui,row_right:f32)->Option<RibbonAction>{
        if self.tools.prefs.mode!="PRO"{return None;}
        let multi=self.multi.as_mut()?;
        let width=crate::multiview::Workspace::comparison_width(ui);
        let compact=width+3.*ui.spacing().item_spacing.x+1.>row_right-ui.cursor().left();
        ui.separator();
        if compact {
            // Never reserve an empty overflow row when the panel height/clip changes.
            // 窄窗将对照工具收进同行菜单，不再预留可能被旧面板裁剪的空白行。
            let mut action=None;
            ui.menu_button(t("Compare"),|ui|{action=multi.comparison_controls(ui,true);});
            action
        }else{
            multi.comparison_controls(ui,false)
        }
    }

    fn remember_ribbon_height(&mut self,ctx:&egui::Context,height:f32){
        // TopBottomPanel clips to its previous size; repaint once after a row change.
        // 顶部面板使用上一帧尺寸裁剪；行数变化后追加一帧，使新边界及时生效。
        if (self.ribbon_height-height).abs()>0.5{ctx.request_repaint();}
        self.ribbon_height=height;
    }

    fn spectrum_ribbon(&mut self,ctx:&egui::Context,allow_pro:bool)->Option<RibbonAction>{
        let mut action=None;
        let panel=egui::TopBottomPanel::top("toolbar").show(ctx,|ui|{
            ui.spacing_mut().item_spacing=Vec2::new(5.,3.);
            ui.spacing_mut().button_padding=Vec2::new(6.,3.);
            align_controls(ui);
            self.ribbon_file(ui,ctx);
            ui.horizontal_wrapped(|ui|{
                ui.menu_button(t("Columns"),|ui|{self.spectrum_toolbar(ui,ctx);}).response.on_hover_text(t("Columns / uncertainty"));
                #[cfg(feature="pro")]
                if self.tools.prefs.mode=="PRO"{
                    for (tab,operation,label) in [("Catalogues","samp","SAMP")]{
                        if ui.button(t(label)).clicked(){self.tools.prefs.ribbon_tab=tab.into();self.tools.prefs.set_pro_collapsed(true,false);if let Some(pro)=&mut self.pro{pro.select(operation);}}
                    }
                }
                ui.separator();self.tools.toolbar(ui,allow_pro);
                ui.menu_button("…",|ui|{
                    ui.set_max_width(430.);
                    ui.strong(t(if self.tools.prefs.mode=="PRO"{"Spectrum PRO workspace"}else{"Spectrum"}));
                    ui.label(t("SDSS reference wavelengths: vacuum Angstrom; blends are reference features."));
                    ui.hyperlink_to(t("Reference table"),"https://classic.sdss.org/dr7/algorithms/linestable.php");
                    ui.label(t("Air conversion applies only at observed 2000–20000 Angstrom."));
                    ui.label(t("Candidates only: continuum subtraction and wavelength proximity do not identify species or solve redshift."));
                    ui.separator();ui.label(t("Wheel: zoom · Drag: pan"));
                    ui.label(t("Ctrl+O: browse · Paste a path and press Enter to open"));
                    ui.label(t("Drag the left edge to resize; Work area restores this panel."));
                }).response.on_hover_text(t("SDSS reference wavelengths: vacuum Angstrom; blends are reference features."));
                #[cfg(feature="pro")]
                if self.tools.prefs.mode=="PRO"{if let Some(multi)=&mut self.multi{ui.separator();action=multi.inline_spectrum(ui);}}
            });
            #[cfg(feature="pro")]
            if self.tools.prefs.mode=="PRO"{
                ui.horizontal_wrapped(|ui|{
                    for (index,(task,label)) in [("Display","Display"),("Lines","Spectral lines"),("Measure","Measure"),("Process","Process"),("Fit","Model fit"),("Compare","Compare")].into_iter().enumerate(){
                        if index==2 || index==4{ui.separator();}
                        let selected=self.pro.as_ref().is_some_and(|pro|if task=="Lines"{pro.operation=="spectral_candidates"}else{pro.operation=="spectral_analysis" && pro.spectral_analysis.as_ref().is_some_and(|analysis|analysis.task==task)});
                        if ui.add_enabled(self.pro.as_ref().is_some_and(|pro|!pro.busy),egui::Button::selectable(selected,t(label))).clicked(){
                            self.tools.prefs.ribbon_tab="Analysis".into();self.tools.prefs.set_pro_collapsed(true,false);
                            if let Some(pro)=&mut self.pro{if task=="Lines"{pro.select("spectral_candidates");}else{pro.select_analysis(task);}}
                        }
                    }
                });
            }
        });
        self.remember_ribbon_height(ctx,panel.response.rect.height());action
    }

    fn ribbon_file(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("FitsViewer").size(16.).strong().color(Color32::from_rgb(100, 210, 225)));
            #[cfg(not(target_arch = "wasm32"))]
            if ui.add_enabled(!self.loading && self.file_picker.is_some(), egui::Button::new(t("Browse…"))).clicked() { self.browse(ctx); }
            let font=egui::TextStyle::Body.resolve(ui.style());let color=ui.visuals().text_color();
            let measure=|text:&str|ui.fonts(|fonts|fonts.layout_no_wrap(text.into(),font.clone(),color).size().x);
            let button_width=|label:&str|(measure(t(label))+ui.spacing().button_padding.x*2.).max(ui.spacing().interact_size.x);
            let gap=ui.spacing().item_spacing.x;let open_width=button_width("Open");
            #[cfg(target_arch="wasm32")]
            let window_width=button_width("↗");
            let mut reserved=open_width+button_width("Header")+button_width("Settings")+gap*3.;
            #[cfg(feature="pro")]
            if self.tools.prefs.mode=="PRO"{reserved+=button_width("Work area")+gap;}
            #[cfg(not(target_arch="wasm32"))]
            {reserved+=button_width("Close image")+gap;}
            #[cfg(target_arch="wasm32")]
            {reserved+=window_width+gap;}
            if self.loading{reserved+=ui.spacing().interact_size.y+gap;}
            // File actions keep their width; the path gives up space first.
            // 文件操作保持可达，宽度不足时优先压缩路径显示，不修改完整字符串。
            let maximum=(ui.available_width()-reserved).max(32.);
            let centered=(ui.max_rect().center().x-open_width/2.-ui.cursor().left()-gap).max(32.);
            let width=(measure(&self.path)+8.).max(centered).min(maximum);
            let response=path_field(ui,&mut self.path,width);
            if ui.add_enabled(!self.loading, egui::Button::new(t("Open")).min_size(Vec2::new(open_width,0.))).clicked()
                || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) { self.request(ctx, None); }
            if ui.button(t("Header")).clicked() { self.header_open = !self.header_open; }
            if ui.selectable_label(self.tools.settings_open,t("Settings")).clicked(){self.tools.settings_open=!self.tools.settings_open;}
            #[cfg(feature="pro")]
            if self.tools.prefs.mode=="PRO"{
                let spectrum=self.image.as_ref().is_some_and(|image|image.spectrum.is_some());
                let visible=!self.tools.prefs.pro_collapsed(spectrum);
                if ui.selectable_label(visible,t("Work area")).on_hover_text(t(if visible{"Hide work area"}else{"Show work area"})).clicked(){self.tools.prefs.set_pro_collapsed(spectrum,visible);}
            }
            #[cfg(not(target_arch = "wasm32"))]
            if ui.add_enabled(!self.loading && self.image.is_some(), egui::Button::new(t("Close image"))).clicked() {
                let _ = self.command(ctx, serde_json::json!({"operation":"close"}));
            }
            if self.loading { ui.spinner(); }
            #[cfg(target_arch="wasm32")]
            {
                ui.add_space((ui.available_width()-window_width).max(0.));
                if ui.add(egui::Button::new("↗").min_size(Vec2::new(window_width,0.))).on_hover_text(t("Open in window")).clicked(){let _=crate::text_io::browser("openWindow",&[]);}
            }
        });
    }

    fn ribbon_hdu(&mut self,ui:&mut egui::Ui,ctx:&egui::Context){
            // Keep the label and dropdown together before the row can expand.
            // 剩余宽度不足时先换行，避免 HDU 下拉框把整条工具栏撑出窗口。
            let label_width=ui.fonts(|fonts|fonts.layout_no_wrap("HDU".into(),egui::TextStyle::Body.resolve(ui.style()),ui.visuals().text_color()).size().x);
            if ui.available_size_before_wrap().x<label_width+ui.spacing().item_spacing.x+125.{ui.end_row();}
            ui.label("HDU");
            let mut chosen = None;
            egui::ComboBox::from_id_salt("hdu").width(125.)
                .selected_text(self.image.as_ref().map(|image| format!("{} · {}", image.info.hdu, image.info.hdus[image.info.hdu].name)).unwrap_or_else(|| "—".into()))
                .show_ui(ui, |ui| {
                    if let Some(image) = &self.image { for hdu in &image.info.hdus {
                        if ui.add_enabled(hdu.viewable && !self.loading, egui::Button::new(format!("{} · {}", hdu.index, hdu.name))).clicked() { chosen = Some(hdu.index); }
                    } }
                });
            if let Some(hdu) = chosen { self.request(ctx, Some(hdu)); }
            if self.image.as_ref().is_some_and(|image|image.spectrum.is_none()){crate::image_display::scale_controls(ui,&mut self.tools.prefs,self.wcs_result.as_ref(),self.rotation_deg);}
    }
    fn ribbon_inspect(&mut self,ui:&mut egui::Ui){
        if ui.selectable_label(self.tools.histogram_open,t("Histogram")).clicked(){self.tools.histogram_open=!self.tools.histogram_open;}
        ui.checkbox(&mut self.tools.prefs.colorbar,t("Colorbar"));
    }
    fn ribbon_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        if self.image.as_ref().is_some_and(|image| image.spectrum.is_some()) {
            self.spectrum_toolbar(ui, ctx);
            #[cfg(feature="pro")]
            if self.tools.prefs.mode=="PRO"{
                ui.separator();
                if ui.button(t("Spectral reference lines")).clicked(){self.tools.prefs.set_pro_collapsed(true,false);if let Some(pro)=&mut self.pro{pro.select("spectral_candidates");}}
            }
            return;
        }
        let exponent = matches!(self.stretch, Stretch::Log | Stretch::Power);
        if ui.add_enabled(self.image.is_some() && self.stats.finite>0,egui::Button::new(t("Auto display"))).on_hover_text(t("Automatic sampled limits and stretch; reset brightness/contrast. Display only.")).clicked(){self.action("Auto display",ctx);}
        let old = self.limits;
        egui::ComboBox::from_id_salt("limits").selected_text(t(self.limits.name())).width(90.).show_ui(ui, |ui| {
            for value in Limits::ALL { ui.selectable_value(&mut self.limits, value, t(value.name())); }
        }).response.on_hover_text(t("Limits"));
        if old != self.limits { (self.low, self.high) = self.stats.bounds(self.limits); self.dirty = true; }
        let old = self.stretch;
        egui::ComboBox::from_id_salt("stretch").selected_text(t(self.stretch.name())).width(105.).show_ui(ui, |ui| {
            for value in Stretch::ALL { ui.selectable_value(&mut self.stretch, value, t(value.name())); }
        }).response.on_hover_text(t("Stretch"));
        if old != self.stretch { self.dirty = true; }
        if exponent {
            ui.label(t("Exponent"));
            if ui.add(egui::DragValue::new(&mut self.tools.prefs.render.stretch_exponent).range(1.000001..=1e6).speed(10.).max_decimals(3)).changed() { self.dirty = true; }
        }
        ui.menu_button(t("Display parameters"), |ui| { self.ribbon_display_parameters(ui); });
        ui.separator();
        for (label, action) in [("−", "Zoom -"), ("+", "Zoom +"), ("Fit", "Fit"), ("1:1", "1:1")] {
            if ui.button(t(label)).clicked() { self.action(action, ctx); }
        }
        ui.label(format!("{:.0}%", self.zoom * ctx.pixels_per_point() * 100.));
        #[cfg(feature="pro")]
        if self.tools.prefs.mode=="PRO"{
            ui.separator();self.ribbon_inspect(ui);
        }
    }

    fn ribbon_display_parameters(&mut self, ui: &mut egui::Ui) {
        let speed = (self.high - self.low).abs().max(0.001) / 300.;
        egui::Grid::new("display-parameters").show(ui, |ui| {
            ui.label(t("Low"));
            let low = ui.add(egui::DragValue::new(&mut self.low).speed(speed).max_decimals(5)).changed(); ui.end_row();
            ui.label(t("High"));
            let high = ui.add(egui::DragValue::new(&mut self.high).speed(speed).max_decimals(5)).changed(); ui.end_row();
            if low || high { self.limits = Limits::Manual; self.dirty = true; }
            ui.label(t("Bias"));
            self.dirty |= ui.add(egui::DragValue::new(&mut self.tone.bias).speed(0.005).range(0.0..=1.0).fixed_decimals(3)).changed(); ui.end_row();
            ui.label(t("Contrast"));
            self.dirty |= ui.add(egui::DragValue::new(&mut self.tone.contrast).speed(0.01).range(0.05..=20.).fixed_decimals(3)).changed(); ui.end_row();
        });
        self.dirty |= ui.checkbox(&mut self.invert, t("Invert")).changed();
        if ui.button(t("Reset B/C")).clicked() { self.tone = DisplayTone::default(); self.dirty = true; }
    }

    fn ribbon_measure(&mut self, ui: &mut egui::Ui) {
        for (tool, label) in [(Tool::Pan, "Pan"), (Tool::Circle, "Circle")] {
            if ui.selectable_value(&mut self.regions.tool, tool, t(label)).clicked() { self.regions.draft.clear(); }
        }
        if ui.selectable_label(self.regions.open, t("Aperture photometry")).clicked() { self.regions.open = !self.regions.open; }
        crate::controls::information(ui,"ribbon-aperture-help",|ui|{
            ui.label(t("Circle: click a center. Drag an aperture to move it. Middle drag: pan. Displayed coordinates are one-based."));
        });
    }

    fn ribbon_coordinates(&mut self, ui: &mut egui::Ui) {
        if ui.add(egui::Button::selectable(self.coordinates.open,t("Gaia coordinate recognition / calibration")).wrap_mode(egui::TextWrapMode::Extend))
            .on_hover_text(t("Open coordinate settings to inspect WCS or configure Gaia calibration.")).clicked() { self.coordinates.open = !self.coordinates.open; }
        crate::controls::information(ui,"ribbon-coordinate-help",|ui|{
            ui.label(t("Open coordinate settings to inspect WCS or configure Gaia calibration."));
        });
    }

    #[cfg(feature="pro")]
    fn ribbon_pro(&mut self, ui: &mut egui::Ui) {
        let spectrum=self.image.as_ref().is_some_and(|image|image.spectrum.is_some());
        let groups: &[(&str, &[&str])] = match self.tools.prefs.ribbon_tab.as_str() {
            "Analysis" if spectrum => &[("Spectrum", &["spectral_candidates"])],
            "Catalogues" if spectrum => &[("SAMP", &["samp"])],
            "Export" if spectrum => &[],
            "Analysis" => &[("Layers", &["image_layers"]),("Detection", &["identify_stars", "detect_stars", "detect_lines", "trace_curve"]), ("Profiles", &["line_profile"])],
            "Catalogues" => &[("Catalogues", &["catalogue", "xmatch", "xmatch_cds"]), ("Virtual Observatory", &["archive_search", "vo_download", "samp"])],
            _ => &[("Figures", &["export", "math_preview"]), ("Image products", &["reproject", "rgb"])],
        };
        for (index, (title, operations)) in groups.iter().enumerate() {
            if index > 0 { ui.separator(); }
            for operation in *operations {
                let label = crate::pro::OPERATIONS.iter().find(|(name, _)| name == operation).map(|(_, label)| *label).unwrap_or(operation);
                let selected = self.pro.as_ref().is_some_and(|pro| pro.operation == *operation);
                if ui.add_enabled(self.pro.as_ref().is_some_and(|pro| !pro.busy), egui::Button::selectable(selected, t(label)).wrap_mode(egui::TextWrapMode::Extend))
                    .on_hover_text(format!("{}\n{}",t(title),t("Select parameters in the PRO workspace; run explicitly when ready."))).clicked() {
                    self.tools.prefs.set_pro_collapsed(spectrum,false);
                    if let Some(pro) = &mut self.pro { pro.select(operation); }
                }
            }
        }
    }
}
