use super::*;
use fitsrs::{Fits, Pixels, HDU};
use std::{fmt::Debug, io::{BufReader, Cursor, Read, Seek, SeekFrom}, path::Path};

#[path = "spectral_reader.rs"]
mod spectral;
#[path = "compressed_reader.rs"]
mod compressed;

pub fn load_file(path: &Path, hdu: Option<usize>) -> Result<FitsImage, String> {
    load_file_with_options(path, LoadOptions { hdu, ..Default::default() })
}

pub fn load_file_with_options(path: &Path, options: LoadOptions) -> Result<FitsImage, String> {
    std::panic::catch_unwind(|| load_inner(path, options))
        .unwrap_or_else(|_| Err("FITS parser rejected malformed or unsupported input".into()))
}

/// Read one original FITS header independently of whether its HDU is viewable.
/// 独立读取指定 HDU 原始头，包括无图像的 primary；不合并或改写头卡。
pub fn load_header(path: &Path, hdu: usize) -> Result<String, String> {
    std::panic::catch_unwind(|| {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > MAX_FILE_BYTES {
            return Err("File exceeds the 512 MiB input limit".into());
        }
        let mut reader = BufReader::new(file);
        let mut magic = [0; 2];
        reader.read_exact(&mut magic).map_err(|e| e.to_string())?;
        reader.rewind().map_err(|e| e.to_string())?;
        if magic == [0x1f, 0x8b] {
            let mut bytes = Vec::new();
            flate2::read::MultiGzDecoder::new(reader).take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
            if bytes.len() as u64 > MAX_FILE_BYTES { return Err("Decoded input exceeds the size limit".into()); }
            header_from_reader(Cursor::new(bytes), hdu)
        } else { header_from_reader(reader, hdu) }
    }).unwrap_or_else(|_| Err("FITS parser rejected malformed or unsupported header input".into()))
}
/// Optional offline browser input, separately bounded to 64 MiB including decompression.
/// 离线浏览器入口：输入及解压后数据均限制为 64 MiB。
pub fn load_bytes_with_options(bytes:&[u8],name:&str,options:LoadOptions)->Result<FitsImage,String>{
    std::panic::catch_unwind(||{
        spectral::validate_error_options(options)?;
        let limit=64*1024*1024;if bytes.len()>limit{return Err("Offline input exceeds 64 MiB".into());}
        let path=Path::new(name);let suffix=name.to_lowercase();
        let text=[".txt",".dat",".csv",".ascii"].iter().any(|s|suffix.ends_with(s) || suffix.ends_with(&format!("{s}.gz")));
        if bytes.starts_with(&[0x1f,0x8b]){let mut decoded=Vec::new();flate2::read::MultiGzDecoder::new(Cursor::new(bytes)).take(limit as u64+1).read_to_end(&mut decoded).map_err(|e|e.to_string())?;if decoded.len()>limit{return Err("Offline decoded input exceeds 64 MiB".into());}if text{spectral::read_text(&decoded,path,options)}else{load_reader(Cursor::new(decoded),path,options)}}
        else if text{spectral::read_text(bytes,path,options)}else{load_reader(Cursor::new(bytes),path,options)}
    }).unwrap_or_else(|_|Err("FITS parser rejected malformed offline input".into()))
}

fn load_inner(path: &Path, options: LoadOptions) -> Result<FitsImage, String> {
    spectral::validate_error_options(options)?;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_FILE_BYTES {
        return Err("File exceeds the 512 MiB input limit".into());
    }
    let mut reader = BufReader::new(file);
    let mut magic = [0; 2];
    reader.read_exact(&mut magic).map_err(|e| e.to_string())?;
    reader.rewind().map_err(|e| e.to_string())?;
    let suffix = path.to_string_lossy().to_lowercase();
    let text = [".txt", ".dat", ".csv", ".ascii"].iter().any(|s| suffix.ends_with(s) || suffix.ends_with(&format!("{s}.gz")));
    if magic == [0x1f, 0x8b] || text {
        let mut bytes = Vec::new();
        let limit = if text { 32 * 1024 * 1024 } else { MAX_FILE_BYTES };
        let mut input: Box<dyn Read> = if magic == [0x1f, 0x8b] {
            Box::new(flate2::read::MultiGzDecoder::new(reader))
        } else { Box::new(reader) };
        input.by_ref().take(limit + 1).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() as u64 > limit { return Err("Decoded input exceeds the size limit".into()); }
        if text { spectral::read_text(&bytes, path, options) }
        else { load_reader(Cursor::new(bytes), path, options) }
    } else { load_reader(reader, path, options) }
}

// Preserve quoted units containing '/', rather than treating them as comments.
// 保留单位中的斜杠，避免将其错误地解释为 FITS 注释。
pub(super) fn card(header: &str, key: &str) -> Option<String> {
    let line = header.lines().find(|s| s.get(..8).is_some_and(|v| v.trim() == key))?;
    if line.as_bytes().get(8) != Some(&b'=') { return None; }
    let value = line.get(10..)?.trim();
    if let Some(value) = value.strip_prefix('\'') {
        let mut out = String::new();
        let mut chars = value.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\'' { if chars.peek() == Some(&'\'') { chars.next(); } else { break; } }
            out.push(c);
        }
        Some(out.trim_end().to_string())
    } else { Some(value.split('/').next()?.trim().to_string()) }
}
pub(super) fn number(header: &str, key: &str, default: f64) -> Result<f64, String> {
    match card(header, key) {
        Some(v) => v.replace(['D','d'], "E").parse().map_err(|_| format!("Invalid {key}")),
        None => Ok(default),
    }
}

fn read_header_range<R: Read + Seek>(reader: &mut R, start: u64, stop: u64) -> Result<String, String> {
    let length = stop.checked_sub(start).ok_or("Invalid header offset")?;
    if length > 1024 * 1024 { return Err("Header exceeds 1 MiB".into()); }
    reader.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
    let mut header = vec![0; length as usize];
    reader.read_exact(&mut header).map_err(|e| e.to_string())?;
    Ok(header.chunks_exact(80).map(|b| String::from_utf8_lossy(b).trim_end().to_string())
        .take_while(|s| s != "END").collect::<Vec<_>>().join("\n"))
}

fn header_from_reader<R: Read + Seek + Debug>(mut reader: R, selected: usize) -> Result<String, String> {
    if selected >= 4096 { return Err("HDU index exceeds the 4096 HDU limit".into()); }
    let mut end = 0_u64;
    let mut bounds = None;
    {
        let mut fits = Fits::from_reader(&mut reader);
        for (index, hdu) in (&mut fits).enumerate() {
            let hdu = hdu.map_err(|e| e.to_string())?;
            let (start, size) = match &hdu {
                HDU::Primary(h) | HDU::XImage(h) => (h.get_data_unit_byte_offset(), h.get_data_unit_byte_size()),
                HDU::XBinaryTable(h) => (h.get_data_unit_byte_offset(), h.get_data_unit_byte_size()),
                HDU::XASCIITable(h) => (h.get_data_unit_byte_offset(), h.get_data_unit_byte_size()),
            };
            if start.checked_sub(end).ok_or("Invalid header offset")? > 1024 * 1024 { return Err("Header exceeds 1 MiB".into()); }
            if index == selected { bounds = Some((end, start)); break; }
            end = start.checked_add(size.div_ceil(2880).checked_mul(2880).ok_or("FITS offsets overflow")?).ok_or("FITS offsets overflow")?;
        }
    }
    let (start, stop) = bounds.ok_or("HDU index is out of bounds")?;
    read_header_range(&mut reader, start, stop)
}

fn load_reader<R: Read + Seek + Debug>(mut reader: R, path: &Path, options: LoadOptions) -> Result<FitsImage, String> {
    spectral::validate_error_options(options)?;
    // Two calls with the same concrete reader type avoid recursive generic
    // instantiation while retaining one source stream for same-file errors.
    // 两次调用保持相同 reader 类型，避免递归泛型单态化；误差仍来自同一文件。
    let mut image = read_selected(&mut reader, path, LoadOptions { error_hdu: None, ..options })?;
    if let Some(error_hdu) = options.error_hdu {
        let source_one_dimensional = image.info.hdus[image.info.hdu].axes.len() == 1;
        if !source_one_dimensional || image.spectrum.is_none() {
            return Err("An uncertainty HDU can only accompany a one-dimensional FITS image spectrum".into());
        }
        if error_hdu == image.info.hdu { return Err("The uncertainty HDU must differ from the flux HDU".into()); }
        reader.rewind().map_err(|e| e.to_string())?;
        let uncertainty = read_selected(&mut reader, path, LoadOptions { hdu: Some(error_hdu), ..Default::default() })?;
        if uncertainty.info.hdus[error_hdu].axes.len() != 1 || uncertainty.info.width != image.info.width || uncertainty.info.height != 1 {
            return Err("Uncertainty HDU must be a one-dimensional vector matching the source spectrum".into());
        }
        let name = card(&uncertainty.info.header, "EXTNAME").unwrap_or_default();
        let kind = spectral::resolve_error_kind(options.error_kind, &name)?;
        let errors = (0..uncertainty.pixels.len()).map(|i| spectral::to_sigma(uncertainty.physical(i).unwrap_or(f64::NAN), kind)).collect();
        image.spectrum.as_mut().unwrap().error = Some(errors);
        let metadata = image.info.spectrum.as_mut().unwrap();
        metadata.error_hdu = Some(error_hdu);
        metadata.uncertainty_kind = spectral::error_label(kind).into();
        metadata.note.push_str(&format!("; uncertainty HDU {error_hdu} ({name}) interpreted as {}, stored as sigma", spectral::error_label(kind)));
    }
    Ok(image)
}

pub(crate) struct Metadata {
    pub info: ImageInfo,
    pub summary: HduInfo,
    pub data_start: u64,
    pub data_size: u64,
}

/// Header-only indexing never asks fitsrs to materialize an image array.
/// 只索引头与数据偏移；gzip 快速路径找到目标即停止，避免为估计大小解完整图。
pub(crate) fn metadata_from_reader<R: Read + Seek + Debug>(mut reader: R, path: &Path, options: LoadOptions, early: bool) -> Result<Metadata, String> {
    let mut summaries = Vec::new();
    let mut starts = Vec::new();
    let mut end = 0;
    {
        let mut fits = Fits::from_reader(&mut reader);
        for (index, hdu) in (&mut fits).enumerate() {
            let hdu = hdu.map_err(|e| e.to_string())?;
            let (name, axes, bitpix, kind, start, size) = match &hdu {
                HDU::Primary(h) | HDU::XImage(h) => {
                    let head = h.get_header();
                    let image = head.get_xtension();
                    let bp = match image.get_bitpix() {
                        fitsrs::hdu::header::Bitpix::U8 => 8, fitsrs::hdu::header::Bitpix::I16 => 16,
                        fitsrs::hdu::header::Bitpix::I32 => 32, fitsrs::hdu::header::Bitpix::I64 => 64,
                        fitsrs::hdu::header::Bitpix::F32 => -32, fitsrs::hdu::header::Bitpix::F64 => -64,
                    };
                    (head.get_parsed::<String>("EXTNAME").unwrap_or_else(|_| if index == 0 { "PRIMARY".into() } else { format!("IMAGE {index}") }),
                     image.get_naxis().to_vec(), bp, if image.get_naxis().len() == 1 { "Spectrum" } else { "Image" }.to_string(), h.get_data_unit_byte_offset(), h.get_data_unit_byte_size())
                }
                HDU::XBinaryTable(h) => {
                    let head = h.get_header();
                    let compressed = head.get_parsed::<bool>("ZIMAGE").unwrap_or(false);
                    let axes = if compressed {
                        let n = head.get_parsed::<usize>("ZNAXIS").unwrap_or(0);
                        (1..=n.min(9)).map(|i| head.get_parsed::<u64>(&format!("ZNAXIS{i}")).unwrap_or(0)).collect()
                    } else { vec![] };
                    let fields = head.get_parsed::<usize>("TFIELDS").unwrap_or(0).min(256);
                    let names = (1..=fields).map(|i| head.get_parsed::<String>(&format!("TTYPE{i}")).unwrap_or_default()).collect::<Vec<_>>();
                    let spectrum = spectral::columns_match(&names);
                    (head.get_parsed::<String>("EXTNAME").unwrap_or_else(|_| format!("TABLE {index}")), axes,
                     if compressed { head.get_parsed::<i32>("ZBITPIX").unwrap_or(0) } else { -64 },
                     if compressed { "Compressed image" } else if spectrum { "Table spectrum" } else { "Table (not a spectrum)" }.into(), h.get_data_unit_byte_offset(), h.get_data_unit_byte_size())
                }
                HDU::XASCIITable(h) => (format!("ASCII TABLE {index}"), vec![], 8, "ASCII table (unsupported)".into(), h.get_data_unit_byte_offset(), h.get_data_unit_byte_size()),
            };
            let viewable = ((axes.len() == 1 || axes.len() == 2) && axes.iter().all(|&n| n > 0)) || kind == "Table spectrum";
            starts.push((end, start, size));
            end = start.checked_add(size.div_ceil(2880).checked_mul(2880).ok_or("FITS offsets overflow")?).ok_or("FITS offsets overflow")?;
            summaries.push(HduInfo { index, name, axes, bitpix, viewable, kind });
            if summaries.len() > 4096 { return Err("Too many HDUs".into()); }
            if early && (options.hdu == Some(index) || options.hdu.is_none() && viewable) { break; }
        }
    }
    let selected = options.hdu.or_else(|| summaries.iter().position(|h| h.viewable)).ok_or("No supported 2D image or basic spectrum HDU")?;
    let summary = summaries.get(selected).ok_or("HDU index is out of bounds")?.clone();
    if !summary.viewable { return Err("Selected HDU is not a supported image or basic spectrum".into()); }
    if options.error_column.is_some() && summary.kind != "Table spectrum" {
        return Err("Uncertainty column selection requires a table or text spectrum; choose an uncertainty HDU for a one-dimensional image".into());
    }
    let (start, stop, data_size) = starts[selected];
    let header = read_header_range(&mut reader, start, stop)?;
    let info = ImageInfo { path: path.to_string_lossy().into_owned(), hdu: selected,
        width: usize::try_from(summary.axes.first().copied().unwrap_or(1)).map_err(|_|"Image dimension exceeds this platform")?, height: usize::try_from(summary.axes.get(1).copied().unwrap_or(1)).map_err(|_|"Image dimension exceeds this platform")?,
        bitpix: summary.bitpix, bscale: number(&header,"BSCALE",1.)?, bzero: number(&header,"BZERO",0.)?,
        blank: card(&header,"BLANK").or_else(|| card(&header,"ZBLANK")).and_then(|s| s.parse().ok()), hdus: summaries, header, spectrum: None };
    if !info.bscale.is_finite() || !info.bzero.is_finite() { return Err("Non-finite BSCALE/BZERO".into()); }
    Ok(Metadata { info, summary, data_start: stop, data_size })
}

fn read_selected<R: Read + Seek + Debug>(mut reader: R, path: &Path, options: LoadOptions) -> Result<FitsImage, String> {
    let Metadata { info, summary, data_start:stop, data_size } = metadata_from_reader(&mut reader,path,options,false)?;
    let selected = info.hdu;
    let expected = checked_pixels(info.width, info.height)?;
    if summary.kind == "Compressed image" { return compressed::read(&mut reader,info,stop,data_size); }
    reader.rewind().map_err(|e| e.to_string())?;
    let mut fits = Fits::from_reader(&mut reader);
    let hdu = fits.nth(selected).ok_or("HDU missing")?.map_err(|e| e.to_string())?;
    let pixels = match hdu {
        HDU::Primary(h) | HDU::XImage(h) => match fits.get_data(&h).pixels() {
            Pixels::U8(it) => RawPixels::U8(it.take(expected+1).collect()), Pixels::I16(it) => RawPixels::I16(it.take(expected+1).collect()),
            Pixels::I32(it) => RawPixels::I32(it.take(expected+1).collect()), Pixels::I64(it) => RawPixels::I64(it.take(expected+1).collect()),
            Pixels::F32(it) => RawPixels::F32(it.take(expected+1).collect()), Pixels::F64(it) => RawPixels::F64(it.take(expected+1).collect()),
        },
        HDU::XBinaryTable(h) => {
            if summary.kind == "Table spectrum" { return spectral::read_table(fits.get_data(&h).table_data(), info, options); }
            return Err("Unsupported binary table".into());
        },
        _ => return Err("Unsupported HDU".into()),
    };
    if pixels.len() != expected { return Err(format!("Truncated pixel data: expected {expected}, read {}",pixels.len())); }
    let mut image = FitsImage { info, pixels, spectrum: None };
    if summary.axes.len() == 1 { spectral::image_axis(&mut image)?; }
    Ok(image)
}

