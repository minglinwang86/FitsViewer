//! Opening routes share the ordinary dataset loader and generation/release boundary.
use crate::{FitsApp,document_tabs::{Destination,Snapshot},ImageLoad};
use eframe::egui;
use fitsviewer_core::{FitsImage,LoadOptions,RawPixels};
use std::sync::Arc;
pub(super) struct Rollback {image:Arc<FitsImage>,manifest:Option<fitsviewer_core::remote::Manifest>,snapshot:Snapshot}

impl FitsApp {
    pub(super) fn open_destination(&self)->Destination{
        if self.single_source{return Destination::Replace;}
        if cfg!(feature="pro") && self.tools.prefs.mode=="PRO"{self.tools.prefs.pro_open_destination}else{self.tools.prefs.minimal_open_destination}
    }
    pub(super) fn open_generated_file(&mut self,ctx:&egui::Context,path:String)->Result<(),String>{self.open_path(ctx,path,LoadOptions::default(),None)}
    #[cfg(feature="pro")]
    pub(super) fn select_workspace_document(&mut self,ctx:&egui::Context,index:usize)->Result<(),String>{
        if self.loading{return Err("Wait for the current load".into());}self.cache_current();
        let selection=self.multi.as_mut().ok_or("Select PRO for multiple views")?.selection(index).ok_or("Unknown view index")?;
        if let Some(index)=self.documents.find(&selection.path,selection.hdu){return self.select_document(ctx,index);}
        let mut options:LoadOptions=serde_json::from_value(selection.view["load_options"].clone()).unwrap_or_default();options.hdu=selection.hdu;
        self.open_path(ctx,selection.path,options,Some(Destination::Tab))?;
        self.pending_view=Some(serde_json::json!({"view":selection.view,"apertures":selection.apertures}));
        if let Some(image)=selection.image{self.invalidate_document_load();self.accept_shared(image);}
        Ok(())
    }
    #[cfg(feature="pro")]
    pub(super) fn close_workspace_document(&mut self,ctx:&egui::Context,index:usize)->Result<(),String>{
        let config=self.multi.as_ref().ok_or("Select PRO for multiple views")?.configuration();let entry=config["views"].as_array().and_then(|views|views.get(index)).ok_or("Unknown view index")?;
        let path=entry["path"].as_str().ok_or("View requires a path")?;let hdu=entry["hdu"].as_u64().map(|h|h as usize);
        if let Some(document)=self.documents.find(path,hdu){self.close_document_tab(ctx,document)?;if self.documents.find(path,hdu).is_none(){if let Some(multi)=&mut self.multi{multi.forget_source(path,hdu);}}}else{self.multi.as_mut().unwrap().remove_layer(index)?;}
        self.refresh_document_refs(ctx);Ok(())
    }
    pub(super) fn remember_document(&mut self){
        if self.document_switching{return;}
        let Some(image)=self.image.clone()else{return;};
        let mut regions=self.regions.clone();regions.suspend_document();
        let snapshot=Snapshot{image:(!matches!(image.pixels,RawPixels::Unavailable)).then_some(image.clone()),view:self.view_configuration(),regions,options:self.load_options,limits:self.limits,auto_fit:self.auto_fit};
        self.documents.remember(image.info.path.clone(),image.info.hdu,snapshot);
    }
    fn invalidate_document_load(&mut self){
        self.selection.cancel();self.cancel_image_load();let mut pending=self.pending.lock().unwrap();pending.0+=1;pending.1=None;drop(pending);self.loading=false;
    }
    pub(super) fn open_path(&mut self,ctx:&egui::Context,path:String,options:LoadOptions,destination:Option<Destination>)->Result<(),String>{
        if path.trim().is_empty(){return Err("Choose a file to open".into());}options.validate()?;
        if self.loading{return Err("Wait for the current load".into());}
        let destination=destination.unwrap_or_else(||self.open_destination());
        if self.single_source && destination!=Destination::Replace{return Err("Remote viewing is limited to its current authorized source.".into());}
        if destination==Destination::Layer{
            #[cfg(feature="pro")] {
                if self.tools.prefs.mode!="PRO"{return Err("New layers require PRO mode".into());}
                if self.image.is_none(){return Err("Open a reference image before adding a layer".into());}
                if self.loading{return Err("Wait for the current load".into());}
                // The editable path field may already contain the layer filename.
                self.path=self.image.as_ref().unwrap().info.path.clone();
                self.sync_pro(ctx);self.pending_layer=Some((path.clone(),options));self.loading=true;self.error.clear();self.load_dataset(ctx,path,options);self.refresh_document_refs(ctx);return Ok(());
            }
            #[cfg(not(feature="pro"))] return Err("New layers require PRO mode".into());
        }
        self.cache_current();
        let rollback=if destination==Destination::Replace{self.image.as_ref().map(|image|{let mut regions=self.regions.clone();regions.suspend_document();Rollback{image:image.clone(),manifest:self.remote.as_ref().map(|r|r.manifest.clone()),snapshot:Snapshot{image:None,view:self.view_configuration(),regions,options:self.load_options,limits:self.limits,auto_fit:self.auto_fit}}})}else{None};
        self.invalidate_document_load();self.pending_open=None;self.replace_rollback=rollback;
        let previous=self.image.as_ref().map(|image|image.info.path.clone());
        if destination==Destination::Tab{self.documents.add(path.clone(),options.hdu)?;}
        let _=previous;
        self.pending_open=Some((destination,path.clone()));self.document_switching=true;self.pending_document=None;
        self.image=None;self.remote=None;self.texture=None;self.stats=Default::default();self.regions=Default::default();self.tools.histogram=None;
        self.path=path;self.pending_load_options=Some(options);self.request(ctx,options.hdu);self.refresh_document_refs(ctx);Ok(())
    }
    pub(super) fn select_document(&mut self,ctx:&egui::Context,index:usize)->Result<(),String>{
        if self.documents.selected==Some(index) && !self.document_switching{return Ok(());}
        self.cache_current();let(path,hdu,mut snapshot)=self.documents.select(index)?;self.invalidate_document_load();self.pending_open=None;self.replace_rollback=None;
        #[cfg(feature="pro")]if let Some(snapshot)=&mut snapshot{if snapshot.image.is_none(){snapshot.image=self.multi.as_ref().and_then(|multi|multi.cached_source(&path,hdu));}}
        self.document_switching=true;self.pending_document=snapshot.clone();self.pending_view=snapshot.as_ref().map(|s|serde_json::json!({"view":s.view,"apertures":s.regions.apertures}));
        self.path=path;self.pending_load_options=snapshot.as_ref().map(|s|s.options);
        if let Some(image)=snapshot.and_then(|s|s.image){self.pending_load_options=None;self.replace_shared(ctx,image);}
        else{self.image=None;self.remote=None;self.texture=None;self.regions=Default::default();self.stats=Default::default();self.tools.histogram=None;self.pending_open=Some((Destination::Tab,self.path.clone()));self.request(ctx,hdu);}self.refresh_document_refs(ctx);Ok(())
    }
    pub(super) fn close_document_tab(&mut self,ctx:&egui::Context,index:usize)->Result<(),String>{
        self.cache_current();let(active,path,next)=self.documents.remove(index)?;
        #[cfg(feature="pro")]self.forget_unused_multi(&path);
        #[cfg(not(feature="pro"))]let _=path;
        if active{
            self.invalidate_document_load();self.pending_open=None;self.replace_rollback=None;self.image=None;self.remote=None;self.texture=None;self.pending_document=None;self.document_switching=true;
            if let Some(next)=next{self.select_document(ctx,next)?;}else{self.close_document(ctx);}
        }
        self.refresh_document_refs(ctx);Ok(())
    }
    #[cfg(feature="pro")]
    fn forget_unused_multi(&mut self,path:&str){
        if !self.documents.paths().iter().any(|p|p==path){if let Some(multi)=&mut self.multi{multi.forget_reference(path);}}
    }
    pub(super) fn restore_document(&mut self){
        if let Some((destination,path))=self.pending_open.take(){if destination==Destination::Replace{
            let previous=self.replace_rollback.as_ref().map(|r|r.image.info.path.clone());let _=self.documents.replace(path,self.image.as_ref().map(|i|i.info.hdu));
            #[cfg(feature="pro")]if let Some(previous)=previous.as_deref(){self.forget_unused_multi(previous);}
            #[cfg(not(feature="pro"))]let _=previous;
        }}self.replace_rollback=None;
        if let Some(mut snapshot)=self.pending_document.take(){
            snapshot.regions.suspend_document();self.regions=snapshot.regions;self.load_options=snapshot.options;self.limits=snapshot.limits;self.auto_fit=snapshot.auto_fit;
        }
        self.document_switching=false;
    }
    #[cfg(feature="offline")]
    pub(super) fn accept_offline_image(&mut self,ctx:&egui::Context,image:FitsImage)->Result<(),String>{
        if self.loading{return Err("Wait for the current load".into());}
        let destination=self.open_destination();let path=image.info.path.clone();
        #[cfg(feature="pro")]if destination==Destination::Layer{self.sync_pro(ctx);self.multi.as_mut().ok_or("Open a reference image before adding a layer")?.insert_layer(Arc::new(image),None)?;return Ok(());}
        self.cache_current();if destination==Destination::Tab{self.documents.add(path.clone(),Some(image.info.hdu))?;}
        self.invalidate_document_load();self.document_switching=true;self.pending_open=Some((destination,path.clone()));self.pending_document=None;self.pending_view=None;
        self.replace_rollback=self.image.as_ref().map(|old|{let mut regions=self.regions.clone();regions.suspend_document();Rollback{image:old.clone(),manifest:None,snapshot:Snapshot{image:None,view:self.view_configuration(),regions,options:self.load_options,limits:self.limits,auto_fit:self.auto_fit}}});
        self.abandon_document(ctx,Some(&path));self.load_options=LoadOptions{hdu:Some(image.info.hdu),..Default::default()};self.accept(image);Ok(())
    }
    pub(super) fn fail_open(&mut self,ctx:&egui::Context,error:String){
        let opening=self.pending_open.take();self.loading=false;
        if opening.is_some(){
            if let Some(rollback)=self.replace_rollback.take(){self.path=rollback.image.info.path.clone();self.pending_view=Some(serde_json::json!({"view":rollback.snapshot.view,"apertures":rollback.snapshot.regions.apertures}));self.load_options=rollback.snapshot.options;self.pending_document=Some(rollback.snapshot);
                self.abandon_document(ctx,Some(&self.path.clone()));if let Some(manifest)=rollback.manifest{self.accept_remote(manifest);}else{self.accept_shared(rollback.image);}
            }else{self.document_switching=false;self.pending_document=None;self.image=None;self.remote=None;self.texture=None;self.regions=Default::default();}
        }self.error=error;self.refresh_document_refs(ctx);
    }
    #[cfg(feature="pro")]
    pub(super) fn accept_layer_load(&mut self,ctx:&egui::Context,result:Result<ImageLoad,String>){
        let Some((path,options))=self.pending_layer.clone()else{return;};
        match result{
            Ok(ImageLoad::Full(image))=>{let result=if self.image.as_ref().is_some_and(|reference|reference.spectrum.is_some()!=image.spectrum.is_some()){Err("Layer and reference data types must match".into())}else{self.multi.as_mut().ok_or_else(||"PRO workspace is no longer open".to_string()).and_then(|multi|multi.insert_layer(Arc::new(image),None))};if let Err(error)=result{self.error=error;}self.pending_layer=None;self.pending_group=None;self.loading=false;self.refresh_document_refs(ctx);},
            Ok(ImageLoad::Remote(manifest))=>{
                if self.image.as_ref().is_some_and(|reference|reference.spectrum.is_some()){
                    self.error="Layer and reference data types must match".into();self.pending_layer=None;self.pending_group=None;self.loading=false;self.refresh_document_refs(ctx);return;
                }
                let query=format!("{}&session={}&generation={}",crate::remote_image::query(&path,options),urlencoding::encode(&self.science.session),self.generation);
                let group=self.pending_group.take().unwrap_or_else(||format!("layer-{}",self.generation));
                self.layer_remote=Some(crate::remote_image::RemoteImage::new(manifest,self.api.clone().unwrap_or_default(),self.token.clone(),query,group,self.local_images.clone(),false));
                self.loading=true;self.poll_layer_preview(ctx);
            },
            Err(error)=>{self.error=error;self.pending_layer=None;self.pending_group=None;self.loading=false;},
        }
    }
    #[cfg(feature="pro")]
    pub(super) fn poll_layer_preview(&mut self,ctx:&egui::Context){
        let Some(remote)=&mut self.layer_remote else{return;};remote.load_preview(ctx);remote.poll();
        if let Some(preview)=remote.preview_shared(){
            let image=Arc::new(FitsImage{info:remote.manifest.info.clone(),pixels:RawPixels::Unavailable,spectrum:None});
            let result=self.multi.as_mut().ok_or_else(||"PRO workspace is no longer open".to_string()).and_then(|multi|multi.insert_layer(image,Some(preview)));
            if let Err(error)=result{self.error=error;}self.layer_remote=None;self.pending_layer=None;self.loading=false;self.refresh_document_refs(ctx);
        }else if !remote.error.is_empty(){self.error=remote.error.clone();self.layer_remote=None;self.pending_layer=None;self.loading=false;}
    }
}

#[cfg(all(test,not(target_arch="wasm32")))]mod tests{
    use super::*;
    use fitsviewer_core::regions::{Aperture,Shape,MeasureOptions,measure};
    use serde_json::json;
    fn files()->(std::path::PathBuf,String,String){
        let root=std::env::temp_dir().join(format!("viewer-document-tabs-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));std::fs::create_dir(&root).unwrap();
        let sample=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/starfield.fits");let a=root.join("first.fits");let b=root.join("second.fits");std::fs::copy(&sample,&a).unwrap();std::fs::copy(sample,&b).unwrap();(root,a.to_string_lossy().into_owned(),b.to_string_lossy().into_owned())
    }
    fn finish(app:&mut FitsApp,ctx:&egui::Context){
        let deadline=std::time::Instant::now()+std::time::Duration::from_secs(10);
        loop{let result=app.pending.lock().unwrap().1.take();if let Some(result)=result{
            app.loading=false;
            #[cfg(feature="pro")]if app.pending_layer.is_some(){app.accept_layer_load(ctx,result);return;}
            match result{Ok(ImageLoad::Full(image))=>app.accept(image),Ok(ImageLoad::Remote(_))=>panic!("Local fixture should use the full reader"),Err(error)=>app.fail_open(ctx,error)};return;
        }assert!(std::time::Instant::now()<deadline);std::thread::sleep(std::time::Duration::from_millis(2));}
    }
    #[test]fn document_tabs_preserve_view_measurements_and_reject_old_generation(){
        let(root,a,b)=files();let ctx=egui::Context::default();let mut app=FitsApp::from_image(fitsviewer_core::load_file_with_options(std::path::Path::new(&a),LoadOptions::default()).unwrap());
        app.command(&ctx,json!({"operation":"view","params":{"zoom":3.,"pan":[4.,5.],"stretch":"Sqrt","low":-2.,"high":120.}})).unwrap();
        let aperture=Aperture{name:"kept".into(),shape:Shape::Circle{center:[25.,25.],radius:3.},background:None,exclude:vec![]};let result=measure(app.image.as_ref().unwrap(),&aperture,&MeasureOptions::default(),None).unwrap();
        app.regions.add_aperture(aperture);app.regions.results[0]=Some(result.clone());app.regions.measurement_pending=true;
        let old_generation=app.generation;let old_revision=app.regions.revision;
        app.open_path(&ctx,b,LoadOptions::default(),Some(Destination::Tab)).unwrap();finish(&mut app,&ctx);assert_eq!(app.documents.len(),2);assert!(app.regions.apertures.is_empty());assert!(app.generation>old_generation);
        app.science.replies.lock().unwrap().push(crate::science_client::Reply{generation:old_generation,operation:"measure".into(),result:Ok(json!({"revision":old_revision,"indices":[0],"measurements":[result]}))});app.science_updates(&ctx);assert!(app.regions.apertures.is_empty());
        let generation=app.generation;app.select_document(&ctx,0).unwrap();assert_eq!(app.path,a);assert!(app.generation>generation);assert_eq!(app.zoom,3.);assert_eq!(app.pan,egui::vec2(4.,5.));assert_eq!(app.stretch,fitsviewer_core::Stretch::Sqrt);assert_eq!((app.low,app.high),(-2.,120.));assert_eq!(app.regions.apertures.len(),1);assert!(app.regions.results[0].is_some());assert!(!app.regions.measurement_pending);assert_eq!(app.image.as_ref().unwrap().info.hdu,0);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]fn replace_and_tab_close_release_their_cached_datasets(){
        let(root,a,b)=files();let ctx=egui::Context::default();let mut app=FitsApp::from_image(fitsviewer_core::load_file_with_options(std::path::Path::new(&a),LoadOptions::default()).unwrap());let old=Arc::downgrade(app.image.as_ref().unwrap());
        app.open_path(&ctx,b.clone(),LoadOptions::default(),Some(Destination::Replace)).unwrap();finish(&mut app,&ctx);assert_eq!(app.documents.len(),1);assert!(old.upgrade().is_none());
        let previous=Arc::downgrade(app.image.as_ref().unwrap());app.open_path(&ctx,a,LoadOptions::default(),Some(Destination::Tab)).unwrap();finish(&mut app,&ctx);assert!(previous.upgrade().is_some());app.close_document_tab(&ctx,0).unwrap();assert!(previous.upgrade().is_none());assert_eq!(app.documents.len(),1);app.close_document_tab(&ctx,0).unwrap();assert!(app.image.is_none());assert_eq!(app.documents.len(),0);std::fs::remove_dir_all(root).unwrap();
    }
    #[test]fn destinations_are_independent_and_mini_never_uses_a_layer(){
        let mut app=FitsApp::new(String::new(),None,String::new(),None);app.tools.prefs.minimal_open_destination=Destination::Tab;app.tools.prefs.pro_open_destination=Destination::Layer;assert_eq!(app.open_destination(),Destination::Tab);
        #[cfg(feature="pro")] {app.tools.prefs.mode="PRO".into();assert_eq!(app.open_destination(),Destination::Layer);app.tools.prefs.mode="Minimal".into();}
        assert!(app.open_path(&egui::Context::default(),"unused.fits".into(),LoadOptions::default(),Some(Destination::Layer)).is_err());
        let json=serde_json::to_string(&app.tools.prefs).unwrap();let restored=crate::controls::Preferences::parse(&json).unwrap();assert_eq!(restored.minimal_open_destination,Destination::Tab);assert_eq!(restored.pro_open_destination,Destination::Layer);
    }
    #[test]fn single_source_scope_never_retains_tabs_or_layers_from_preferences(){
        let mut app=FitsApp::new(String::new(),None,String::new(),None).with_single_source_scope();app.tools.prefs.minimal_open_destination=Destination::Tab;app.tools.prefs.pro_open_destination=Destination::Layer;assert_eq!(app.open_destination(),Destination::Replace);
        for target in [Destination::Tab,Destination::Layer]{assert!(app.open_path(&egui::Context::default(),"scope-source".into(),LoadOptions::default(),Some(target)).unwrap_err().contains("authorized source"));}
        assert_eq!(app.documents.len(),0);
    }
    #[test]fn failed_new_tab_is_empty_and_failed_replace_restores_valid_identity(){
        let(root,a,_)=files();let ctx=egui::Context::default();let mut app=FitsApp::from_image(fitsviewer_core::load_file_with_options(std::path::Path::new(&a),LoadOptions::default()).unwrap());
        let missing=root.join("missing.fits").to_string_lossy().into_owned();app.open_path(&ctx,missing.clone(),LoadOptions::default(),Some(Destination::Tab)).unwrap();assert!(app.image.is_none());finish(&mut app,&ctx);
        assert!(app.image.is_none());assert!(app.regions.apertures.is_empty());assert!(!app.document_switching);assert_eq!(app.path,missing);assert_eq!(app.documents.len(),2);assert!(!app.error.is_empty());
        app.select_document(&ctx,0).unwrap();let original=app.image.as_ref().unwrap().clone();app.zoom=4.;let generation=app.generation;
        app.open_path(&ctx,missing,LoadOptions::default(),Some(Destination::Replace)).unwrap();assert!(app.image.is_none());finish(&mut app,&ctx);assert_eq!(app.path,a);assert!(Arc::ptr_eq(&original,app.image.as_ref().unwrap()));assert_eq!(app.zoom,4.);assert!(app.generation>generation);assert!(!app.document_switching);assert!(!app.error.is_empty());std::fs::remove_dir_all(root).unwrap();
    }
    #[test]fn mixed_image_spectrum_tabs_restore_plot_and_uncertainty_mapping(){
        let ctx=egui::Context::default();
        let meta=fitsviewer_core::SpectrumMeta{x_label:"Wavelength".into(),x_unit:"Angstrom".into(),y_label:"Flux".into(),y_unit:String::new(),columns:vec!["wave".into(),"flux".into(),"sigma".into()],x_column:0,y_column:1,error_column:Some(2),error_hdu:None,uncertainty_kind:"sigma".into(),note:String::new()};
        let spectrum=FitsImage{info:fitsviewer_core::ImageInfo{path:"spectrum-fixture".into(),hdu:1,width:3,height:1,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:Some(meta)},pixels:RawPixels::F64(vec![1.,2.,3.]),spectrum:Some(fitsviewer_core::SpectrumData{x:vec![4000.,5000.,6000.],error:Some(vec![0.1,0.2,0.3])})};
        let mut app=FitsApp::from_image(spectrum);app.plot.bounds=[4200.,5700.,0.5,4.];app.plot.fit=false;app.cache_current();
        app.documents.add("image-fixture".into(),Some(0)).unwrap();app.document_switching=true;app.abandon_document(&ctx,Some("image-fixture"));
        app.accept(FitsImage{info:fitsviewer_core::ImageInfo{path:"image-fixture".into(),hdu:0,width:2,height:2,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:None},pixels:RawPixels::F64(vec![4.;4]),spectrum:None});
        app.select_document(&ctx,0).unwrap();assert_eq!(app.plot.bounds,[4200.,5700.,0.5,4.]);assert!(!app.plot.fit);assert_eq!(app.load_options.x_column,Some(0));assert_eq!(app.load_options.y_column,Some(1));assert_eq!(app.load_options.error_column,Some(2));assert_eq!(app.load_options.error_kind,fitsviewer_core::ErrorKind::Sigma);assert!(!app.load_options.ignore_errors);assert_eq!(app.image.as_ref().unwrap().info.hdu,1);
    }
    #[cfg(feature="pro")]
    #[test]fn new_layer_uses_existing_overlay_without_replacing_reference(){
        let(root,a,b)=files();let ctx=egui::Context::default();let mut app=FitsApp::from_image(fitsviewer_core::load_file_with_options(std::path::Path::new(&a),LoadOptions::default()).unwrap());app.tools.prefs.mode="PRO".into();app.sync_pro(&ctx);let reference=app.image.as_ref().unwrap().clone();
        app.tools.prefs.pro_open_destination=Destination::Layer;app.path=b.clone();app.open_generated_file(&ctx,b).unwrap();finish(&mut app,&ctx);assert_eq!(app.path,a);assert!(Arc::ptr_eq(&reference,app.image.as_ref().unwrap()));assert_eq!(app.documents.len(),1);
        let multi=app.multi.as_mut().unwrap();let layers=multi.image_layers(&reference);assert_eq!(layers.len(),1);let index=layers[0].index;let weak=Arc::downgrade(&layers[0].image);drop(layers);multi.remove_layer(index).unwrap();assert!(multi.image_layers(&reference).is_empty());assert!(weak.upgrade().is_none());std::fs::remove_dir_all(root).unwrap();
    }
    #[cfg(feature="pro")]
    #[test]fn generated_files_and_legacy_workspace_actions_preserve_other_documents(){
        let(root,a,b)=files();let ctx=egui::Context::default();let mut app=FitsApp::from_image(fitsviewer_core::load_file_with_options(std::path::Path::new(&a),LoadOptions::default()).unwrap());
        app.open_path(&ctx,b.clone(),LoadOptions::default(),Some(Destination::Tab)).unwrap();finish(&mut app,&ctx);app.tools.prefs.mode="PRO".into();app.sync_pro(&ctx);
        assert_eq!(app.multi.as_ref().unwrap().configuration()["views"].as_array().unwrap().len(),1);
        app.command(&ctx,json!({"operation":"workspace.close","params":{"index":0}})).unwrap();assert_eq!(app.documents.len(),1);assert_eq!(app.path,a);
        app.tools.prefs.pro_open_destination=Destination::Tab;app.open_generated_file(&ctx,b.clone()).unwrap();finish(&mut app,&ctx);assert_eq!(app.documents.len(),2);
        let config=app.multi.as_ref().unwrap().configuration();let views=config["views"].as_array().unwrap();let first=views.iter().position(|v|v["path"]==a).unwrap();let second=views.iter().position(|v|v["path"]==b).unwrap();
        app.command(&ctx,json!({"operation":"workspace.select","params":{"index":first}})).unwrap();assert_eq!(app.path,a);assert_eq!(app.documents.len(),2);
        app.command(&ctx,json!({"operation":"workspace.close","params":{"index":second}})).unwrap();assert_eq!(app.path,a);assert_eq!(app.documents.len(),1);std::fs::remove_dir_all(root).unwrap();
    }
}

