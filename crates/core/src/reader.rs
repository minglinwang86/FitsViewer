use super::*;
use fitsrs::{Fits, Pixels, HDU};
use std::fmt::Debug;
use std::io::{BufReader, Cursor, Read, Seek, SeekFrom};
use std::path::Path;

/// Read a bounded ordinary 2D FITS image using fitsrs; malformed input is an error.
/// 使用 fitsrs 读取受容量限制的二维 FITS，损坏的输入返回错误。
pub fn load_file(path: &Path, hdu: Option<usize>) -> Result<FitsImage, String> {
    let result = std::panic::catch_unwind(|| load_file_inner(path, hdu));
    result.unwrap_or_else(|_| Err("FITS parser rejected malformed or unsupported input".into()))
}
fn load_file_inner(path: &Path, hdu: Option<usize>) -> Result<FitsImage, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_FILE_BYTES {
        return Err("File exceeds the MVP limit of 512 MiB".into());
    }
    let mut reader = BufReader::new(file);
    let mut magic = [0; 2];
    reader.read_exact(&mut magic).map_err(|e| e.to_string())?;
    reader.rewind().map_err(|e| e.to_string())?;
    if magic == [0x1f, 0x8b] {
        let mut bytes = Vec::new();
        flate2::read::MultiGzDecoder::new(reader)
            .take(MAX_FILE_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_FILE_BYTES {
            return Err("Decompressed FITS exceeds the MVP limit of 512 MiB".into());
        }
        load_reader(Cursor::new(bytes), path, hdu)
    } else {
        load_reader(reader, path, hdu)
    }
}
fn load_reader<R: Read + Seek + Debug>(
    mut reader: R,
    path: &Path,
    requested: Option<usize>,
) -> Result<FitsImage, String> {
    let mut summaries = Vec::new();
    let mut starts = Vec::new();
    let mut previous_end = 0;
    {
        let mut fits = Fits::from_reader(&mut reader);
        for (index, hdu) in (&mut fits).enumerate() {
            let hdu = hdu.map_err(|e| e.to_string())?;
            let (name, axes, bitpix, kind, data_start, data_size) = match &hdu {
                HDU::Primary(h) | HDU::XImage(h) => {
                    let header = h.get_header();
                    let image = header.get_xtension();
                    let bp = match image.get_bitpix() {
                        fitsrs::hdu::header::Bitpix::U8 => 8,
                        fitsrs::hdu::header::Bitpix::I16 => 16,
                        fitsrs::hdu::header::Bitpix::I32 => 32,
                        fitsrs::hdu::header::Bitpix::I64 => 64,
                        fitsrs::hdu::header::Bitpix::F32 => -32,
                        fitsrs::hdu::header::Bitpix::F64 => -64,
                    };
                    (
                        header.get_parsed::<String>("EXTNAME").unwrap_or_else(|_| {
                            if index == 0 {
                                "PRIMARY".into()
                            } else {
                                format!("IMAGE {index}")
                            }
                        }),
                        image.get_naxis().to_vec(),
                        bp,
                        "Image".to_string(),
                        h.get_data_unit_byte_offset(),
                        h.get_data_unit_byte_size(),
                    )
                }
                HDU::XBinaryTable(h) => (
                    h.get_header()
                        .get_parsed::<String>("EXTNAME")
                        .unwrap_or_else(|_| format!("TABLE {index}")),
                    vec![],
                    8,
                    "Table / compressed image (not in MVP)".into(),
                    h.get_data_unit_byte_offset(),
                    h.get_data_unit_byte_size(),
                ),
                HDU::XASCIITable(h) => (
                    format!("ASCII TABLE {index}"),
                    vec![],
                    8,
                    "ASCII table".into(),
                    h.get_data_unit_byte_offset(),
                    h.get_data_unit_byte_size(),
                ),
            };
            let viewable = axes.len() == 2 && axes.iter().all(|&n| n > 0);
            starts.push((previous_end, data_start));
            previous_end = data_start
                .checked_add(data_size.div_ceil(2880) * 2880)
                .ok_or("FITS offsets overflow")?;
            summaries.push(HduInfo {
                index,
                name,
                axes,
                bitpix,
                viewable,
                kind,
            });
            if summaries.len() > 4096 {
                return Err("Too many HDUs".into());
            }
        }
    }
    let selected=match requested { Some(i)=>i,None=>summaries.iter().position(|h|h.viewable).ok_or("No ordinary 2D image HDU. Cubes, tables and tile-compressed .fz images are not supported in this MVP.")? };
    let summary = summaries
        .get(selected)
        .ok_or("HDU index is out of bounds")?;
    if !summary.viewable {
        return Err("Selected HDU is not an ordinary 2D image".into());
    }
    let width = usize::try_from(summary.axes[0]).map_err(|_| "Image width overflow")?;
    let height = usize::try_from(summary.axes[1]).map_err(|_| "Image height overflow")?;
    let expected = checked_pixels(width, height)?;
    reader.rewind().map_err(|e| e.to_string())?;
    let (pixels, bscale, bzero, blank) = {
        let mut fits = Fits::from_reader(&mut reader);
        let hdu = fits
            .nth(selected)
            .ok_or("HDU missing")?
            .map_err(|e| e.to_string())?;
        match hdu {
            HDU::Primary(h) | HDU::XImage(h) => {
                let header = h.get_header();
                let bscale = if header.get("BSCALE").is_some() {
                    header
                        .get_parsed::<f64>("BSCALE")
                        .map_err(|e| e.to_string())?
                } else {
                    1.0
                };
                let bzero = if header.get("BZERO").is_some() {
                    header
                        .get_parsed::<f64>("BZERO")
                        .map_err(|e| e.to_string())?
                } else {
                    0.0
                };
                let blank = if header.get("BLANK").is_some() {
                    Some(
                        header
                            .get_parsed::<i64>("BLANK")
                            .map_err(|e| e.to_string())?,
                    )
                } else {
                    None
                };
                if !bscale.is_finite() || !bzero.is_finite() {
                    return Err("Non-finite BSCALE/BZERO".into());
                }
                let raw = match fits.get_data(&h).pixels() {
                    Pixels::U8(it) => RawPixels::U8(it.collect()),
                    Pixels::I16(it) => RawPixels::I16(it.collect()),
                    Pixels::I32(it) => RawPixels::I32(it.collect()),
                    Pixels::I64(it) => RawPixels::I64(it.collect()),
                    Pixels::F32(it) => RawPixels::F32(it.collect()),
                    Pixels::F64(it) => RawPixels::F64(it.collect()),
                };
                (raw, bscale, bzero, blank)
            }
            _ => return Err("Unsupported image HDU".into()),
        }
    };
    if pixels.len() != expected {
        return Err(format!(
            "Truncated pixel data: expected {expected}, read {}",
            pixels.len()
        ));
    }
    let (start, end) = starts[selected];
    let header_size = end.checked_sub(start).ok_or("Invalid header offset")?;
    if header_size > 1024 * 1024 {
        return Err("Header exceeds 1 MiB".into());
    }
    reader
        .seek(SeekFrom::Start(start))
        .map_err(|e| e.to_string())?;
    let mut header = vec![0; header_size as usize];
    reader.read_exact(&mut header).map_err(|e| e.to_string())?;
    let header = header
        .as_chunks::<80>()
        .0
        .iter()
        .map(|c| String::from_utf8_lossy(c).trim_end().to_string())
        .take_while(|s| s != "END")
        .collect::<Vec<_>>()
        .join("\n");
    let info = ImageInfo {
        path: path.to_string_lossy().into_owned(),
        hdu: selected,
        width,
        height,
        bitpix: summary.bitpix,
        bscale,
        bzero,
        blank,
        hdus: summaries,
        header,
    };
    Ok(FitsImage { info, pixels })
}
