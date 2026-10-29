//! Native views use the same bounded metadata/preview/tile store as HTTP views.
//! 原生窗口不再先完整展开图像；保持相同的原像素与科学计算边界。
use std::{path::{Path,PathBuf},sync::Arc};
use axum::extract::Query;
use crate::{ImageQuery,TileQuery,remote_data::Store};

pub fn bridge(cache:PathBuf)->fitsviewer_web::ImageBridge{
    let store=Arc::new(Store::new(cache));
    Arc::new(move|url|{
        let uri=url.parse::<axum::http::Uri>().map_err(|e|e.to_string())?;
        let Query(image)=Query::<ImageQuery>::try_from_uri(&uri).map_err(|e|e.to_string())?;
        let literal=Path::new(&image.path);
        let source=if literal.exists(){literal}else{Path::new(fitsviewer_core::pasted_path(&image.path))};
        let logical=source.to_string_lossy();let options=image.options()?;
        if uri.path()=="/api/image/metadata"{
            // Spectra and existing non-2D formats retain the native full reader;
            // the HTTP display packet limit must not become a new native limit.
            // 光谱等非二维数据沿用原生完整读取，不套用 HTTP 的 20 MB 显示门槛。
            if fitsviewer_core::remote::inspect(source,options).is_err(){return Err("Native full dataset reader".into());}
            return serde_json::to_vec(&store.manifest(source,&logical,options)?).map_err(|e|e.to_string());
        }
        let Query(query)=Query::<TileQuery>::try_from_uri(&uri).map_err(|e|e.to_string())?;
        match uri.path(){
            "/api/image/full"=>store.full(source,&logical,options,&query.version),
            "/api/image/tile" if [0,16].contains(&query.display_bits)=>store.display_tile(source,&logical,options,&query.version,query.level,query.x,query.y,query.display_bits==16),
            "/api/image/pixel"=>serde_json::to_vec(&store.pixel(source,&logical,options,&query.version,query.x,query.y)?).map_err(|e|e.to_string()),
            _=>Err("Unknown native image operation".into()),
        }
    })
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn non_image_native_inputs_use_the_full_reader_contract(){
        let root=std::env::temp_dir().join(format!("fv-native-data-{}",uuid::Uuid::new_v4()));std::fs::create_dir(&root).unwrap();
        let path=root.join("spectrum.dat");std::fs::write(&path,b"4000 1\n5000 2\n").unwrap();
        let image=bridge(root.join("cache"));
        let result=image(format!("/api/image/metadata?path={}",urlencoding::encode(&path.to_string_lossy())));
        assert_eq!(result.unwrap_err(),"Native full dataset reader");drop(image);std::fs::remove_dir_all(root).unwrap();
    }
}
