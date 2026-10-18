//! Cross-check the seek reader against the established typed FITS reader.
//! 用已有完整读取器验证分块读取、缩放和原始值；这些小型数值夹具不用于界面展示。
use super::*;
use std::{fs,io::Write,path::Path};

fn card(name:&str,value:&str)->String{format!("{name:<8}= {value:>20}").chars().chain(std::iter::repeat(' ')).take(80).collect()}
fn fixture(path:&Path,bitpix:i32,width:usize,height:usize,empty_primary:bool){
    let mut bytes=Vec::new();
    if empty_primary{let header=[card("SIMPLE","T"),card("BITPIX","8"),card("NAXIS","0"),card("EXTEND","T"),format!("{:<80}","END")].concat();bytes.extend_from_slice(header.as_bytes());bytes.resize(2880,b' ');}
    let mut cards=if empty_primary{vec![card("XTENSION","'IMAGE   '")]}else{vec![card("SIMPLE","T")]};
    cards.extend([card("BITPIX",&bitpix.to_string()),card("NAXIS","2"),card("NAXIS1",&width.to_string()),card("NAXIS2",&height.to_string())]);
    if empty_primary{cards.extend([card("PCOUNT","0"),card("GCOUNT","1")]);}
    cards.extend([card("BSCALE","2.5"),card("BZERO","-10.0")]);
    if bitpix>0{cards.push(card("BLANK","17"));}cards.push(format!("{:<80}","END"));
    bytes.extend_from_slice(cards.concat().as_bytes());bytes.resize(bytes.len().div_ceil(2880)*2880,b' ');
    for i in 0..width*height{let n=(i%101) as i64;match bitpix{
        8=>bytes.push(n as u8),16=>bytes.extend((n as i16).to_be_bytes()),32=>bytes.extend((n as i32).to_be_bytes()),64=>bytes.extend(n.to_be_bytes()),
        -32=>bytes.extend((if n==17{f32::NAN}else{n as f32-30.25}).to_be_bytes()),
        -64=>bytes.extend((if n==17{f64::NAN}else{n as f64-30.25}).to_be_bytes()),_=>unreachable!(),
    }}
    bytes.resize(bytes.len().div_ceil(2880)*2880,0);fs::write(path,bytes).unwrap();
}

#[test]
fn typed_roi_and_original_pixels_match_complete_reader(){
    let directory=tempfile::tempdir().unwrap();
    for bitpix in [8,16,32,64,-32,-64]{
        let path=directory.path().join(format!("中文-typed-{bitpix}.fits"));fixture(&path,bitpix,600,270,true);
        let options=LoadOptions{hdu:Some(1),..Default::default()};
        let full=crate::load_file_with_options(&path,options).unwrap();let plane=inspect(&path,options).unwrap();
        assert_eq!(plane.info.hdu,1);assert_eq!(plane.compression,"none");
        for (level,tx,ty) in [(0,0,0),(0,1,1),(0,2,0),(2,0,0)]{
            let tile=read_tile(&path,&plane,level,tx,ty).unwrap();
            let restored=FitsImage::from_packet(&tile.to_packet().unwrap()).unwrap();
            let (x,y,width,height,step)=tile_bounds(&plane.info,level,tx,ty).unwrap();
            assert_eq!((restored.info.width,restored.info.height),(width,height));
            for oy in 0..height{for ox in 0..width{
                let sx=(x+ox*step+step/2).min(599);let sy=(y+oy*step+step/2).min(269);
                let original=sy*600+sx;let output=oy*width+ox;
                assert_eq!(restored.pixels.raw_string(output),full.pixels.raw_string(original));
                assert_eq!(restored.physical(output),full.physical(original));
            }}
        }
        for (x,y) in [(0,0),(17,0),(599,269),(257,256)]{let read=read_pixel(&path,&plane,x,y).unwrap();assert_eq!(read["raw"].as_str(),Some(full.pixels.raw_string(y*600+x).as_str()));assert_eq!(read["value"].as_f64(),full.physical(y*600+x));assert_eq!(read["original_samples"],true);}
        assert!(read_pixel(&path,&plane,600,1).is_err());assert!(read_tile(&path,&plane,0,3,0).is_err());
    }
}

#[test]
fn gzip_prefix_is_bounded_and_source_revision_changes(){
    let directory=tempfile::tempdir().unwrap();let path=directory.path().join("base.fits");fixture(&path,-32,17,19,false);
    let compressed=directory.path().join("base.fits.gz");let mut encoder=flate2::write::GzEncoder::new(fs::File::create(&compressed).unwrap(),flate2::Compression::fast());encoder.write_all(&fs::read(&path).unwrap()).unwrap();encoder.finish().unwrap();
    let plane=inspect(&compressed,LoadOptions::default()).unwrap();assert_eq!(plane.compression,"gzip");assert!(!plane.hdus_complete);assert_eq!((plane.info.width,plane.info.height),(17,19));assert!(read_tile(&compressed,&plane,0,0,0).is_err());
    let before=source_version(&path).unwrap();fs::OpenOptions::new().append(true).open(&path).unwrap().write_all(&[0]).unwrap();assert_ne!(before,source_version(&path).unwrap());
    let truncated=directory.path().join("truncated.fits");let bytes=fs::read(&path).unwrap();fs::write(&truncated,&bytes[..2881]).unwrap();assert!(inspect(&truncated,LoadOptions::default()).is_err());
}
