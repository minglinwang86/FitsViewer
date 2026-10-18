//! Shared headless/native/HTTP read-only measurement and original-header commands.
//! 原生、HTTP、CLI 共用测光与头读取；误差类型和 HDU 来源始终显式记录。
use fitsviewer_core::{ErrorKind,LoadOptions,regions::{Aperture,MeasureOptions,measure}};
use serde_json::{json,Value};
use std::path::Path;

pub fn header_json(params:&Value)->Result<Value,String>{
    let path=Path::new(params["path"].as_str().ok_or("Header requires path")?);let hdu=params["hdu"].as_u64().unwrap_or(0) as usize;
    Ok(json!({"path":path,"hdu":hdu,"header":fitsviewer_core::load_header(path,hdu)?,"merged":false}))
}
fn unit(header:&str)->String{header.lines().find(|line|line.starts_with("BUNIT   =")).and_then(|line|line.split('\'').nth(1)).unwrap_or("").trim().into()}
pub fn measure_json(params:&Value)->Result<Value,String>{
    let path=Path::new(params["path"].as_str().ok_or("Measurement requires path")?);
    let image=fitsviewer_core::load_file_with_options(path,LoadOptions{hdu:params["hdu"].as_u64().map(|v|v as usize),..Default::default()})?;
    if image.spectrum.is_some(){return Err("Aperture photometry requires a two-dimensional image".into());}
    #[cfg(not(feature="pro"))] crate::mini_measure(params)?;
    let apertures:Vec<Aperture>=serde_json::from_value(params["apertures"].clone()).map_err(|e|e.to_string())?;
    if apertures.is_empty() || apertures.len()>64{return Err("Expected one to 64 apertures".into());}
    let options:MeasureOptions=if params["options"].is_object(){serde_json::from_value(params["options"].clone()).map_err(|e|e.to_string())?}else{Default::default()};
    let error_path=params["error_path"].as_str().filter(|p|!p.trim().is_empty()).map(Path::new).or_else(||params["error_hdu"].as_u64().map(|_|path));
    let mut error_source=Value::Null;
    let errors=if let Some(error_path)=error_path{
        let kind=ErrorKind::parse(params["error_kind"].as_str().unwrap_or("sigma")).filter(|kind|*kind!=ErrorKind::Auto).ok_or("Error map requires sigma, variance or ivar explicitly")?;
        let hdu=params["error_hdu"].as_u64().map(|v|v as usize);
        if error_path.canonicalize().map_err(|e|e.to_string())?==path.canonicalize().map_err(|e|e.to_string())? && hdu.is_none(){return Err("Choose an explicit uncertainty HDU for an error map in the same FITS file".into());}
        let map=fitsviewer_core::load_file(error_path,hdu)?;
        if error_path.canonicalize().map_err(|e|e.to_string())?==path.canonicalize().map_err(|e|e.to_string())? && map.info.hdu==image.info.hdu{return Err("Science and uncertainty HDUs must differ".into());}
        if map.spectrum.is_some() || map.info.width!=image.info.width || map.info.height!=image.info.height{return Err("Error image dimensions differ".into());}
        error_source=json!({"path":error_path,"hdu":map.info.hdu,"kind":kind.as_str(),"bunit":unit(&map.info.header),"output_kind":"sigma","alignment":"same pixel grid required; no reprojection"});
        Some((0..map.pixels.len()).map(|i|{
            let value=map.physical(i).unwrap_or(f64::NAN);
            if !value.is_finite(){f64::NAN}else{match kind{ErrorKind::Sigma if value>=0.=>value,ErrorKind::Variance if value>=0.=>value.sqrt(),ErrorKind::Ivar if value>0.=>1./value.sqrt(),_=>f64::NAN}}
        }).collect::<Vec<_>>())
        // map's original pixels are released here, before the aperture calculation.
    }else{None};
    let mut results=Vec::new();for aperture in &apertures{results.push(measure(&image,aperture,&options,errors.as_deref())?);}
    let growth=if let Some(radii)=params.get("growth_radii"){let radii:Vec<f64>=serde_json::from_value(radii.clone()).map_err(|e|e.to_string())?;Some(fitsviewer_core::regions::growth_curve_with_errors(&image,&apertures[0],&options,&radii,errors.as_deref())?)}else{None};
    Ok(json!({"schema":1,"path":path,"hdu":image.info.hdu,"bunit":unit(&image.info.header),"coordinate_origin":0,"apertures":apertures,"options":options,"uncertainty_source":error_source,"measurements":results,"growth_curve":growth,"original_samples":true,"indices":params["indices"],"revision":params["revision"]}))
}
