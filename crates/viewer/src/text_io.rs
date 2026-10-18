//! Small JSON import/export helpers; no additional UI runtime.
//! 小型 JSON 导入导出接口，不添加额外 UI 运行时。
pub fn save(name:&str,text:&str)->Result<(),String>{
    #[cfg(not(target_arch="wasm32"))]
    {std::fs::write(name,text).map_err(|e|e.to_string())}
    #[cfg(target_arch="wasm32")]
    {browser("download",&[name,text])}
}
pub fn saved_regions()->Option<String>{
    #[cfg(not(target_arch="wasm32"))] let text=std::fs::read_to_string(crate::controls::settings_path().with_file_name("regions-v1.json")).ok();
    #[cfg(target_arch="wasm32")] let text=web_sys::window().and_then(|w|w.local_storage().ok().flatten()).and_then(|s|s.get_item("fitsviewer-regions-v1").ok().flatten());
    text.filter(|s|fitsviewer_core::regions::RegionTemplate::parse(s).is_ok())
}
pub fn remember_regions(text:&str)->Result<(),String>{
    fitsviewer_core::regions::RegionTemplate::parse(text)?;
    #[cfg(not(target_arch="wasm32"))]{let path=crate::controls::settings_path().with_file_name("regions-v1.json");std::fs::create_dir_all(path.parent().ok_or("No template directory")?).map_err(|e|e.to_string())?;std::fs::write(path,text).map_err(|e|e.to_string())?;}
    #[cfg(target_arch="wasm32")]{web_sys::window().and_then(|w|w.local_storage().ok().flatten()).ok_or("Template storage unavailable")?.set_item("fitsviewer-regions-v1",text).map_err(|_|"Could not save template")?;}
    Ok(())
}
#[cfg(not(target_arch="wasm32"))]
pub fn read(name:&str)->Result<String,String>{let path=std::path::Path::new(name);if path.metadata().map_err(|e|e.to_string())?.len()>256*1024{return Err("JSON file exceeds 256 KiB".into());}std::fs::read_to_string(path).map_err(|e|e.to_string())}
#[cfg(target_arch="wasm32")]
pub fn browser(method:&str,arguments:&[&str])->Result<(),String>{
    use wasm_bindgen::JsCast;
    let window=web_sys::window().ok_or("No browser window")?;
    let api=js_sys::Reflect::get(&window,&"FitsViewer".into()).map_err(|_|"Viewer API unavailable")?;
    let function=js_sys::Reflect::get(&api,&method.into()).map_err(|_|"Viewer operation unavailable")?.dyn_into::<js_sys::Function>().map_err(|_|"Viewer operation unavailable")?;
    let args=js_sys::Array::new();for argument in arguments{args.push(&(*argument).into());}
    function.apply(&api,&args).map_err(|_|"Browser operation failed")?;Ok(())
}
