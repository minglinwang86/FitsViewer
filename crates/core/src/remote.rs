//! Shared, bounded remote display protocol. Original samples stay explicitly separate.
//! 共享远程显示协议；原始科学像素与显示图块保持明确区分。
use crate::{FitsImage, ImageInfo, LoadOptions, RawPixels, Statistics};
use serde::{Deserialize, Serialize};

pub const FULL_TRANSFER_BYTES: u64 = 20_000_000;
pub const TILE_SIZE: usize = 256;

#[derive(Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub schema: u32,
    pub info: ImageInfo,
    pub source_version: String,
    pub full_packet_bytes: Option<u64>,
    pub threshold_bytes: u64,
    pub transport: String,
    pub compression: String,
    pub hdus_complete: bool,
    pub statistics: Statistics,
    pub statistics_sampled: bool,
    pub preview_level: u8,
}

/// Checked full-packet size, including metadata and spectral coordinate/error arrays.
/// 完整封包大小包括头和光谱附加数组，不使用压缩文件体积作为阈值。
pub fn packet_bytes(info: &ImageInfo) -> Result<u64, String> {
    if ![8, 16, 32, 64, -32, -64].contains(&info.bitpix) {
        return Err("Unsupported image storage type".into());
    }
    let count = (info.width as u64).checked_mul(info.height as u64).ok_or("Image size overflow")?;
    let element = u64::from(info.bitpix.unsigned_abs() / 8);
    let extra = info.spectrum.as_ref().map_or(0, |s| if s.has_error() {16} else {8});
    let metadata = serde_json::to_vec(info).map_err(|e| e.to_string())?;
    if metadata.len() > 1024 * 1024 { return Err("Image metadata exceeds 1 MiB".into()); }
    count.checked_mul(element + extra).and_then(|n| n.checked_add(4 + metadata.len() as u64)).ok_or("Packet size overflow".into())
}

pub fn preview_level(width: usize, height: usize) -> u8 {
    let longest = width.max(height);
    let mut level = 0;
    while longest.div_ceil(1usize << level) > TILE_SIZE && level < 30 { level += 1; }
    level
}

#[cfg(any(not(target_arch = "wasm32"), feature = "offline"))]
mod access {
    use super::*;
    use std::io::{BufReader, Read, Seek, SeekFrom};
    use std::path::{Path, PathBuf};

    #[derive(Clone)]
    pub struct Plane {
        pub info: ImageInfo,
        pub data_start: u64,
        pub data_size: u64,
        pub compression: String,
        pub hdus_complete: bool,
    }

    /// A bounded seekable prefix supports common gzip headers without expanding a plane.
    /// 常见 gzip 头仅缓存至多 1 MiB；远处 HDU 明确交给有界磁盘准备路径。
    #[derive(Debug)]
    struct Prefix<R> { source: R, bytes: Vec<u8>, position: usize }
    impl<R: Read> Prefix<R> {
        fn ensure(&mut self, end: usize) -> std::io::Result<()> {
            if end > 1024 * 1024 { return Err(std::io::Error::other("Compressed metadata needs bounded preparation")); }
            while self.bytes.len() < end {
                let take = (end - self.bytes.len()).min(8192);
                let mut buffer = [0u8; 8192];
                let n = self.source.read(&mut buffer[..take])?;
                if n == 0 { break; }
                self.bytes.extend_from_slice(&buffer[..n]);
            }
            Ok(())
        }
    }
    impl<R: Read> Read for Prefix<R> {
        fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
            let end = self.position.checked_add(out.len()).ok_or_else(|| std::io::Error::other("Metadata offset overflow"))?;
            self.ensure(end)?;
            let n = out.len().min(self.bytes.len().saturating_sub(self.position));
            out[..n].copy_from_slice(&self.bytes[self.position..self.position+n]);
            self.position += n;
            Ok(n)
        }
    }
    impl<R: Read> Seek for Prefix<R> {
        fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
            let position = match position {
                SeekFrom::Start(n) => usize::try_from(n).ok(),
                SeekFrom::Current(n) => (self.position as i128 + i128::from(n)).try_into().ok(),
                SeekFrom::End(_) => None,
            }.ok_or_else(|| std::io::Error::other("Unsupported compressed metadata seek"))?;
            self.ensure(position)?;
            if position > self.bytes.len() { return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof,"Truncated compressed header")); }
            self.position = position;
            Ok(position as u64)
        }
    }

    pub fn inspect(path: &Path, options: LoadOptions) -> Result<Plane, String> {
        options.validate()?;
        std::panic::catch_unwind(|| {
            let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
            let size = file.metadata().map_err(|e| e.to_string())?.len();
            let mut reader = BufReader::new(file);
            let mut magic = [0u8; 2];
            reader.read_exact(&mut magic).map_err(|e| e.to_string())?;
            reader.rewind().map_err(|e| e.to_string())?;
            let gzip = magic == [0x1f, 0x8b];
            let metadata = if gzip {
                if size > crate::MAX_FILE_BYTES { return Err("Compressed input exceeds the file limit".into()); }
                let prefix = Prefix { source: flate2::read::MultiGzDecoder::new(reader), bytes:Vec::new(), position:0 };
                crate::reader::metadata_from_reader(prefix, path, options, true)?
            } else {
                crate::reader::metadata_from_reader(reader, path, options, false)?
            };
            if metadata.summary.axes.len() != 2 { return Err("Header-only tiles require a two-dimensional image".into()); }
            let info = metadata.info;
            if info.width == 0 || info.height == 0 || info.width > u32::MAX as usize || info.height > u32::MAX as usize {
                return Err("Unsupported remote image dimensions".into());
            }
            if !gzip && metadata.data_start.checked_add(metadata.data_size).is_none_or(|end| end > size) {
                return Err("Truncated image data".into());
            }
            let compressed_tiles = metadata.summary.kind == "Compressed image";
            Ok(Plane { info, data_start:metadata.data_start, data_size:metadata.data_size,
                compression:if gzip {"gzip"} else if compressed_tiles {"tile"} else {"none"}.into(), hdus_complete:!gzip })
        }).unwrap_or_else(|_|Err("Malformed remote FITS metadata".into()))
    }

    pub fn source_version(path: &Path) -> Result<String, String> {
        let metadata = path.metadata().map_err(|e|e.to_string())?;
        let modified = metadata.modified().ok().and_then(|t|t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d|d.as_nanos()).unwrap_or(0);
        #[cfg(unix)] {
            use std::os::unix::fs::MetadataExt;
            Ok(format!("{}-{modified}-{}-{}-{}-{}",metadata.len(),metadata.dev(),metadata.ino(),metadata.ctime(),metadata.ctime_nsec()))
        }
        #[cfg(not(unix))] { Ok(format!("{}-{modified}",metadata.len())) }
    }

    fn empty_pixels(bitpix: i32, capacity: usize) -> Result<RawPixels, String> {
        Ok(match bitpix {
            8 => RawPixels::U8(Vec::with_capacity(capacity)),
            16 => RawPixels::I16(Vec::with_capacity(capacity)),
            32 => RawPixels::I32(Vec::with_capacity(capacity)),
            64 => RawPixels::I64(Vec::with_capacity(capacity)),
            -32 => RawPixels::F32(Vec::with_capacity(capacity)),
            -64 => RawPixels::F64(Vec::with_capacity(capacity)),
            _ => return Err("Unsupported remote BITPIX".into()),
        })
    }
    fn push(pixels: &mut RawPixels, b: &[u8]) {
        match pixels {
            RawPixels::U8(v) => v.push(b[0]),
            RawPixels::I16(v) => v.push(i16::from_be_bytes(b.try_into().unwrap())),
            RawPixels::I32(v) => v.push(i32::from_be_bytes(b.try_into().unwrap())),
            RawPixels::I64(v) => v.push(i64::from_be_bytes(b.try_into().unwrap())),
            RawPixels::F32(v) => v.push(f32::from_be_bytes(b.try_into().unwrap())),
            RawPixels::F64(v) => v.push(f64::from_be_bytes(b.try_into().unwrap())),
            RawPixels::Unavailable => unreachable!(),
        }
    }

    pub fn tile_bounds(info: &ImageInfo, level: u8, tx: usize, ty: usize) -> Result<(usize,usize,usize,usize,usize),String> {
        if level > 30 { return Err("Invalid image pyramid level".into()); }
        let step = 1usize.checked_shl(u32::from(level)).ok_or("Pyramid scale overflow")?;
        let x = tx.checked_mul(TILE_SIZE).and_then(|n|n.checked_mul(step)).ok_or("Tile offset overflow")?;
        let y = ty.checked_mul(TILE_SIZE).and_then(|n|n.checked_mul(step)).ok_or("Tile offset overflow")?;
        if x >= info.width || y >= info.height { return Err("Tile is outside the image".into()); }
        let width = (info.width-x).div_ceil(step).min(TILE_SIZE);
        let height = (info.height-y).div_ceil(step).min(TILE_SIZE);
        Ok((x,y,width,height,step))
    }

    /// Read only sampled rows/columns for a display tile; no original full-plane allocation.
    /// 显示采用明确的最近邻采样；测量从原分辨率单独读取，不能使用这些样本替代。
    pub fn read_tile(path: &Path, plane: &Plane, level: u8, tx: usize, ty: usize) -> Result<FitsImage,String> {
        if plane.compression=="tile"{
            let mut reader=BufReader::new(std::fs::File::open(path).map_err(|e|e.to_string())?);
            return crate::reader::compressed_tile(&mut reader,plane.info.clone(),plane.data_start,plane.data_size,level,tx,ty);
        }
        if plane.compression != "none" { return Err("Compressed data needs the prepared reader".into()); }
        let (x,y,width,height,step) = tile_bounds(&plane.info,level,tx,ty)?;
        let unit = (plane.info.bitpix.unsigned_abs()/8) as usize;
        let mut pixels = empty_pixels(plane.info.bitpix,width*height)?;
        let mut reader = BufReader::new(std::fs::File::open(path).map_err(|e|e.to_string())?);
        let first_x = x.saturating_add(step/2).min(plane.info.width-1);
        let last_x = x.saturating_add((width-1)*step).saturating_add(step/2).min(plane.info.width-1);
        let row_bytes = (last_x-first_x+1).checked_mul(unit).ok_or("Tile row overflow")?;
        let mut row = vec![0u8;row_bytes.min(4*1024*1024)];
        for output_y in 0..height {
            let sy = y.saturating_add(output_y*step).saturating_add(step/2).min(plane.info.height-1);
            let offset = (sy as u64).checked_mul(plane.info.width as u64).and_then(|n|n.checked_add(first_x as u64)).and_then(|n|n.checked_mul(unit as u64)).and_then(|n|n.checked_add(plane.data_start)).ok_or("FITS row offset overflow")?;
            if row_bytes <= row.len() {
                reader.seek(SeekFrom::Start(offset)).map_err(|e|e.to_string())?;
                reader.read_exact(&mut row[..row_bytes]).map_err(|e|e.to_string())?;
                for output_x in 0..width {
                    let sx = x.saturating_add(output_x*step).saturating_add(step/2).min(plane.info.width-1);
                    let start = (sx-first_x)*unit;
                    push(&mut pixels,&row[start..start+unit]);
                }
            } else {
                let mut one = [0u8;8];
                for output_x in 0..width {
                    let sx = x.saturating_add(output_x*step).saturating_add(step/2).min(plane.info.width-1);
                    reader.seek(SeekFrom::Start(offset+(sx-first_x) as u64*unit as u64)).map_err(|e|e.to_string())?;
                    reader.read_exact(&mut one[..unit]).map_err(|e|e.to_string())?;
                    push(&mut pixels,&one[..unit]);
                }
            }
        }
        let mut info = plane.info.clone();
        info.width = width; info.height = height;
        info.header.clear();info.hdus.clear();info.path.clear();
        Ok(FitsImage {info,pixels,spectrum:None})
    }

    pub fn sample_decoded(image: &FitsImage, level:u8, tx:usize, ty:usize) -> Result<FitsImage,String> {
        let (x,y,width,height,step)=tile_bounds(&image.info,level,tx,ty)?;
        let mut values=Vec::with_capacity(width*height);
        for oy in 0..height { for ox in 0..width {
            let sx=(x+ox*step+step/2).min(image.info.width-1);
            let sy=(y+oy*step+step/2).min(image.info.height-1);
            values.push(image.physical(sy*image.info.width+sx).unwrap_or(f64::NAN));
        }}
        let mut info=image.info.clone();info.width=width;info.height=height;info.bitpix=-64;info.bscale=1.;info.bzero=0.;info.blank=None;info.header.clear();info.hdus.clear();info.path.clear();
        Ok(FitsImage {info,pixels:RawPixels::F64(values),spectrum:None})
    }

    /// Exact original physical/raw value; remote UI must label pending values separately.
    pub fn read_pixel(path:&Path,plane:&Plane,x:usize,y:usize)->Result<serde_json::Value,String>{
        if x>=plane.info.width || y>=plane.info.height{return Err("Pixel is outside the image".into());}
        if plane.compression=="tile"{
            let tile=read_tile(path,plane,0,x/TILE_SIZE,y/TILE_SIZE)?;
            let index=(y%TILE_SIZE)*tile.info.width+x%TILE_SIZE;
            return Ok(serde_json::json!({"x":x,"y":y,"raw":tile.pixels.raw_string(index),"value":tile.physical(index),"original_samples":true}));
        }
        if plane.compression!="none"{return Err("Compressed pixel requires a prepared reader".into());}
        let unit=(plane.info.bitpix.unsigned_abs()/8) as usize;
        let offset=(y as u64).checked_mul(plane.info.width as u64).and_then(|n|n.checked_add(x as u64)).and_then(|n|n.checked_mul(unit as u64)).and_then(|n|n.checked_add(plane.data_start)).ok_or("Pixel offset overflow")?;
        let mut file=std::fs::File::open(path).map_err(|e|e.to_string())?;
        file.seek(SeekFrom::Start(offset)).map_err(|e|e.to_string())?;
        let mut bytes=[0u8;8];file.read_exact(&mut bytes[..unit]).map_err(|e|e.to_string())?;
        let mut pixels=empty_pixels(plane.info.bitpix,1)?;push(&mut pixels,&bytes[..unit]);
        let raw=pixels.raw_string(0);let mut info=plane.info.clone();info.width=1;info.height=1;
        let image=FitsImage{info,pixels,spectrum:None};
        Ok(serde_json::json!({"x":x,"y":y,"raw":raw,"value":image.physical(0),"original_samples":true}))
    }

    pub fn prepared_path(directory:&Path,identity:&str)->PathBuf { directory.join(format!("{identity}.fits")) }
}

#[cfg(any(not(target_arch = "wasm32"), feature = "offline"))]
pub use access::{inspect,prepared_path,read_pixel,read_tile,sample_decoded,source_version,tile_bounds,Plane};

#[cfg(test)]
#[path="remote_tests.rs"]
mod reader_tests;

#[cfg(test)]
mod tests {
    use super::*;
    fn info()->ImageInfo {ImageInfo{path:"metadata.fits".into(),hdu:0,width:1024,height:1024,bitpix:-32,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:None}}
    #[test] fn packet_threshold_uses_decoded_arrays(){let mut value=info();assert!(packet_bytes(&value).unwrap()<FULL_TRANSFER_BYTES);value.width=4096;value.height=4096;assert!(packet_bytes(&value).unwrap()>FULL_TRANSFER_BYTES);}
    #[test] fn remote_placeholder_never_fakes_original_pixels(){let image=FitsImage{info:info(),pixels:RawPixels::Unavailable,spectrum:None};assert!(image.physical(0).is_none());assert_eq!(image.pixels.raw_string(0),"unavailable");assert!(image.to_packet().is_err());}
    #[test] fn pyramid_bounds_and_overflow(){assert_eq!(preview_level(4800,4800),5);assert!(tile_bounds(&info(),31,0,0).is_err());assert!(tile_bounds(&info(),0,usize::MAX,0).is_err());assert_eq!(tile_bounds(&info(),2,0,0).unwrap(),(0,0,256,256,4));}
}
