//! Tool-specific inputs and prerequisites. Protocol JSON stays in the overflow menu.
//! 每页声明自己的常用输入；内部协议字段不再自动堆到所有页面。
use eframe::egui;
use serde_json::{json,Value};
use crate::i18n::t;

pub fn action(operation:&str)->&'static str{match operation{
    "identify_stars"=>"▶ Identify catalogue objects","detect_stars"=>"▶ Detect local candidates",
    "detect_lines"=>"▶ Detect traces","trace_curve"=>"▶ Track curve","line_profile"=>"▶ Extract profile",
    "catalogue"=>"▶ Overlay catalogue","xmatch"|"xmatch_cds"=>"▶ Match coordinates",
    "archive_search"|"vo_query"=>"▶ Search observations","reproject"=>"▶ Reproject preview",
    "rgb"=>"▶ Compose RGB","export"=>"▶ Export figure","math_preview"=>"▶ Preview MathText",
    "vo_download"=>"▶ Download resource","vo_datalink"=>"▶ Resolve product links",_=>"Run",
}}
pub fn validate(operation:&str,params:&Value)->Option<&'static str>{
    let path=|key:&str|params[key].as_str().is_some_and(|s|!s.trim().is_empty());
    match operation{
        "catalogue" if !path("catalogue")=>Some("Choose a catalogue file first."),
        "xmatch" if ["first","second"].iter().any(|key|params[*key]["catalogue"].as_str().is_none_or(|p|p.trim().is_empty()))=>Some("Choose both coordinate tables first."),
        "xmatch_cds" if !path("catalogue") && !params["coordinates"]["ra_deg"].as_array().is_some_and(|a|!a.is_empty())=>Some("Choose a coordinate table or reuse identified objects first."),
        "rgb" if params["channels"].as_array().is_none_or(|channels|channels.len()!=3 || channels.iter().any(|channel|channel["path"].as_str().is_none_or(|p|p.trim().is_empty())))=>Some("Choose all three R / G / B image files first."),
        "trace_curve" if !params["seed"].as_array().is_some_and(|a|a.len()==2 && a.iter().all(|v|v.as_f64().is_some_and(f64::is_finite)))=>Some("Select a seed before tracing."),
        "line_profile" if !params["points"].as_array().is_some_and(|a|a.len()>=2)=>Some("Draw at least two path points before extracting."),
        "reproject"|"archive_search" if params["use_image_center"]!=true && (params["ra_deg"].as_f64().is_none_or(|n|!n.is_finite()) || params["dec_deg"].as_f64().is_none_or(|n|!n.is_finite() || n.abs()>90.))=>Some("Choose a WCS position or enter RA / Dec before searching."),
        "vo_download"|"vo_datalink" if !path("url")=>Some("Choose a product or enter its URL first."),
        _=>None,
    }
}
pub fn fields(ui:&mut egui::Ui,params:&mut Value,keys:&[&str]){
    for key in keys{if let Some(value)=params.get_mut(*key){
        ui.push_id(key,|ui|{ui.horizontal_wrapped(|ui|{
            ui.label(t(&key.replace('_'," ")));
            match value{
                Value::String(text)=>{ui.add(egui::TextEdit::singleline(text).desired_width(180.));},
                Value::Bool(value)=>{ui.checkbox(value,"");},
                Value::Number(number)=>{let integer=number.is_i64() || number.is_u64();let mut n=number.as_f64().unwrap_or(0.);if ui.add(egui::DragValue::new(&mut n).speed(if integer{1.}else{0.1})).changed(){*value=if integer{json!(n.round() as i64)}else{json!(n)};}},
                Value::Null if ["ra_deg","dec_deg"].contains(key)=>{let mut number=0.;if ui.add(egui::DragValue::new(&mut number).speed(0.001)).changed(){*value=json!(number);}if ui.small_button(t("Set coordinate")).clicked(){*value=json!(number);}},
                _=>{},
            }
        });});
    }}
}
fn choice(ui:&mut egui::Ui,params:&mut Value,key:&str,label:&str,choices:&[(&str,&str)]){
    let mut selected=params[key].as_str().unwrap_or(choices[0].0).to_owned();
    ui.label(t(label));egui::ComboBox::from_id_salt(("tool-choice",key)).selected_text(choices.iter().find(|(id,_)|*id==selected).map(|(_,label)|t(label)).unwrap_or(&selected)).show_ui(ui,|ui|{
        for (id,label) in choices{ui.selectable_value(&mut selected,(*id).into(),t(label));}
    });params[key]=json!(selected);
}
pub fn common(ui:&mut egui::Ui,operation:&str,params:&mut Value,paths:&[String],services:&Value,sky:&Value){
    match operation{
        "identify_stars"=>{choice(ui,params,"catalogue_source","Catalogue",&[("gaia_dr3","Gaia DR3"),("sdss_dr17","SDSS DR17")]);fields(ui,params,&["magnitude_limit"]);ui.weak(t("Catalogue positions projected through the active FITS / Gaia WCS; proximity alone is not a verified image detection."));},
        "detect_stars"=>{fields(ui,params,&["fwhm","threshold_sigma"]);ui.weak(t("Local image peaks are candidates; use catalogue identification to identify known objects."));},
        "detect_lines"=>fields(ui,params,&["threshold_sigma","min_length","width"]),
        "trace_curve"=>{choice(ui,params,"axis","Tracking axis",&[("x","x"),("y","y")]);ui.horizontal(|ui|{ui.label(t("Direction"));let mut direction=params["direction"].as_i64().unwrap_or(1);ui.selectable_value(&mut direction,1,"+ →");ui.selectable_value(&mut direction,-1,"− ←");params["direction"]=json!(direction);});fields(ui,params,&["search_half_width","centroid_half_width"]);},
        "line_profile"=>{fields(ui,params,&["width","background_gap","background_width"]);ui.weak(t("Interpolated profile in original pixel / image units; no wavelength calibration is applied."));},
        "catalogue"|"xmatch_cds"=>{
            fields(ui,params,&["catalogue","ra_column","dec_column"]);
            if params["coordinates"]["ra_deg"].as_array().is_some_and(|a|!a.is_empty()){ui.label(format!("{}: {}",t("Reused coordinates"),params["coordinates"]["ra_deg"].as_array().unwrap().len()));}
            if operation=="catalogue"{choice(ui,params,"frame","Coordinate frame",&[("icrs","ICRS"),("fk5","FK5 J2000")]);fields(ui,params,&["ra_unit","dec_unit"]);}
            else{choice(ui,params,"remote_catalogue","Remote catalogue",&[("vizier:I/355/gaiadr3","Gaia DR3"),("vizier:II/246/out","2MASS"),("vizier:II/349/ps1","Pan-STARRS1"),("vizier:V/154/sdss16","SDSS DR16"),("vizier:I/345/gaia2","Gaia DR2")]);fields(ui,params,&["radius_arcsec"]);choice(ui,params,"strategy","Match strategy",&[("all","All candidates"),("nearest","Nearest candidate")]);}
        },
        "xmatch"=>{for key in ["first","second"]{ui.group(|ui|{ui.strong(t(if key=="first"{"First coordinate table"}else{"Second coordinate table"}));fields(ui,&mut params[key],&["catalogue","ra_column","dec_column","frame"]);});}fields(ui,params,&["radius_arcsec"]);choice(ui,params,"strategy","Match strategy",&[("nearest","Nearest candidate"),("all","All candidates")]);},
        "rgb"=>{if let Some(channels)=params["channels"].as_array_mut(){for (index,channel) in channels.iter_mut().take(3).enumerate(){ui.group(|ui|{ui.strong(["R","G","B"][index]);if !paths.is_empty(){let mut path=channel["path"].as_str().unwrap_or("").to_owned();egui::ComboBox::from_id_salt(("rgb-layer",index)).selected_text(t("Use loaded file")).show_ui(ui,|ui|{for candidate in paths{if ui.selectable_label(path==*candidate,candidate.rsplit(['/', '\\']).next().unwrap_or(candidate)).clicked(){path=candidate.clone();}}});channel["path"]=json!(path);}fields(ui,channel,&["path","weight","stretch"]);});}}fields(ui,params,&["assume_aligned","reproject"]);},
        "archive_search"=>{
            ui.label(t("Archive / survey"));let previous=params["service_id"].as_str().unwrap_or("aladin_hips").to_owned();let mut selected=previous.clone();
            egui::ComboBox::from_id_salt("archive-service").selected_text(services["archives"].as_array().and_then(|a|a.iter().find(|v|v["id"]==selected)).and_then(|v|v["label"].as_str()).unwrap_or(&selected)).show_ui(ui,|ui|{
                if let Some(archives)=services["archives"].as_array(){for archive in archives{if let Some(id)=archive["id"].as_str(){ui.selectable_value(&mut selected,id.into(),archive["label"].as_str().unwrap_or(id));}}}
                else{for (id,label) in [("aladin_hips","CDS / Aladin HiPS"),("sdss_dr17","SDSS DR17"),("des_dr2","DES DR2"),("des_dr2_se","DES single exposure")]{ui.selectable_value(&mut selected,id.into(),label);}}
            });
            if selected!=previous{if let Some(values)=params.as_object_mut(){values.remove("collection");values.remove("observation_id");}}
            params["service_id"]=json!(selected);
            if params["service_id"]=="mast"{choice(ui,params,"collection","Mission / collection",&[("","All missions"),("HST","HST"),("JWST","JWST"),("GALEX","GALEX"),("TESS","TESS")]);}
            if params["service_id"]=="irsa"{
                if params["collection"].is_null(){params["collection"]=json!("wise_allwise");}
                choice(ui,params,"collection","Mission / collection",&[("","All collections"),("wise_allwise","WISE AllWISE"),("twomass_allsky","2MASS All-Sky"),("neowiser","NEOWISE"),("euclid_DpdMerBksMosaic","Euclid MER mosaics"),("euclid_DpdNirCalibratedFrame","Euclid NIR calibrated frames"),("euclid_DpdVisCalibratedQuadFrame","Euclid VIS calibrated frames")]);
            }
            let mut center=params["use_image_center"].as_bool().unwrap_or(true);ui.checkbox(&mut center,t("Use current image footprint"));params["use_image_center"]=json!(center);
            if !center{
                if ui.add_enabled(sky["ra_deg"].is_number() && sky["dec_deg"].is_number(),egui::Button::new(t("Use current WCS position"))).clicked(){params["ra_deg"]=sky["ra_deg"].clone();params["dec_deg"]=sky["dec_deg"].clone();params["frame"]=json!("fk5");}
                fields(ui,params,&["ra_deg","dec_deg"]);choice(ui,params,"frame","Coordinate frame",&[("icrs","ICRS"),("fk5","FK5 J2000")]);
                fields(ui,params,&["radius_deg"]);
            }
            fields(ui,params,&["max_rows"]);
        },
        "reproject"=>{
            let mut center=params["use_image_center"].as_bool().unwrap_or(false);ui.checkbox(&mut center,t("Use image center from WCS"));params["use_image_center"]=json!(center);
            ui.add_enabled_ui(!center,|ui|{if ui.add_enabled(sky["ra_deg"].is_number(),egui::Button::new(t("Use current WCS position"))).clicked(){params["ra_deg"]=sky["ra_deg"].clone();params["dec_deg"]=sky["dec_deg"].clone();}fields(ui,params,&["ra_deg","dec_deg"]);});
            ui.weak(t("Target coordinates: FK5 J2000"));let mut native=params["scale_arcsec"].is_null();if ui.checkbox(&mut native,t("Use native pixel scale")).changed(){params["scale_arcsec"]=if native{Value::Null}else{json!(1.)};}
            fields(ui,params,&["projection","scale_arcsec","width","height"]);ui.weak(t("Display interpolation; no flux-conserving scientific reprojection is inferred."));
        },
        "export"=>{choice(ui,params,"format","Format",&[("png","PNG"),("jpeg","JPEG"),("svg","SVG"),("pdf","PDF")]);fields(ui,params,&["width","height","dpi","wcs_axes","colorbar"]);},
        "math_preview"=>{fields(ui,params,&["expression"]);ui.weak(t("Matplotlib MathText preview; this is not a TeX installation or scientific fit."));},
        "vo_download"|"vo_datalink"=>fields(ui,params,&["url","maximum_bytes"]),
        "vo_query"=>fields(ui,params,&["protocol","service","adql","max_rows"]),_=>{},
    }
}
pub fn advanced(ui:&mut egui::Ui,operation:&str,params:&mut Value){
    let keys:&[&str]=match operation{
        "identify_stars"=>&["max_rows","timeout"],"detect_stars"=>&["min_separation","limit","mask_path"],
        "detect_lines"=>&["gap","votes","seed"],"trace_curve"=>&["order"],"catalogue"=>&["marker_limit"],
        "reproject"=>&["rotation_deg"],"export"=>&["compass"],"archive_search"=>&["timeout"],_=>&[],
    };fields(ui,params,keys);
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn empty_scientific_inputs_are_rejected_before_submission(){
        for operation in ["catalogue","xmatch","xmatch_cds","rgb","trace_curve","line_profile","archive_search","vo_download"]{assert!(validate(operation,&json!({})).is_some(),"{operation}");}
        assert!(validate("line_profile",&json!({"points":[[3.,4.],[9.,7.]]})).is_none());
        assert!(validate("xmatch_cds",&json!({"coordinates":{"ra_deg":[12.],"dec_deg":[-32.]}})).is_none());
        assert!(validate("archive_search",&json!({"ra_deg":0.,"dec_deg":-90.})).is_none());
    }
}
