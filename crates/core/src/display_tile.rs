//! Compact display samples only. Scientific data and exact readouts never use these tiles.
//! 仅压缩显示图块；原像素读数和科学测量始终访问原文件。
use crate::{FitsImage,RawPixels};

/// Quantize a bounded display tile to signed 16-bit, with one reserved invalid code.
/// Per-tile BSCALE/BZERO retains the physical-value mapping for interactive stretches.
pub fn compact(mut tile:FitsImage)->FitsImage{
    if tile.info.bitpix.unsigned_abs()<=16{return tile;}
    let mut minimum=f64::INFINITY;let mut maximum=f64::NEG_INFINITY;
    for i in 0..tile.pixels.len(){if let Some(value)=tile.physical(i).filter(|v|v.is_finite()){minimum=minimum.min(value);maximum=maximum.max(value);}}
    let center=if minimum.is_finite(){minimum/2.+maximum/2.}else{0.};
    let scale=if maximum>minimum{maximum/65534.-minimum/65534.}else{1.};
    // If a subnormal interval cannot be represented, keep the original bounded tile.
    // 极小动态范围不能安全量化时保留原始图块，不能把有效像素改成无效值。
    if !center.is_finite() || !scale.is_finite() || scale<=0.{return tile;}
    let values=(0..tile.pixels.len()).map(|i|match tile.physical(i).filter(|v|v.is_finite()){
        Some(value)=>((value-center)/scale).round().clamp(-32767.,32767.) as i16,
        None=>i16::MIN,
    }).collect();
    tile.pixels=RawPixels::I16(values);tile.info.bitpix=16;tile.info.bscale=scale;
    tile.info.bzero=center;tile.info.blank=Some(i64::from(i16::MIN));tile
}

#[cfg(test)]mod tests{
    use super::*;
    fn tile(values:Vec<f64>)->FitsImage{FitsImage{info:crate::ImageInfo{path:"display-only".into(),hdu:1,width:values.len(),height:1,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:None},pixels:RawPixels::F64(values),spectrum:None}}
    #[test]fn compact_error_is_bounded_and_invalid_samples_stay_invalid(){
        let source=tile(vec![-1000.,-21.,0.,18.,1000.,f64::NAN,f64::INFINITY]);let result=compact(source);
        assert_eq!(result.info.bitpix,16);
        for (i,value) in [-1000.,-21.,0.,18.,1000.].iter().enumerate(){assert!((result.physical(i).unwrap()-value).abs()<=result.info.bscale*0.51);}
        assert!(!result.physical(5).is_some_and(f64::is_finite));assert!(!result.physical(6).is_some_and(f64::is_finite));
    }
    #[test]fn offset_and_constant_images_keep_their_physical_scale(){
        for values in [vec![1e15,1e15+1.,1e15+2.],vec![123.5;8],vec![f64::NAN;8]]{
            let result=compact(tile(values.clone()));
            for (i,value) in values.iter().enumerate(){if value.is_finite(){assert!((result.physical(i).unwrap()-value).abs()<=0.25);}else{assert!(!result.physical(i).is_some_and(f64::is_finite));}}
        }
    }
}
