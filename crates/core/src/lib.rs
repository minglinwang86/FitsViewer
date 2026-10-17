//! FITS numeric data and display transformations. / FITS 数值与显示变换。
use serde::{Deserialize, Serialize};

/// Maximum image size accepted by this MVP.
pub const MAX_PIXELS: usize = 32 * 1024 * 1024;
/// Maximum compressed or uncompressed input size.
pub const MAX_FILE_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Metadata needed to select a viewable HDU.
pub struct HduInfo {
    pub index: usize,
    pub name: String,
    pub axes: Vec<u64>,
    pub bitpix: i32,
    pub viewable: bool,
    pub kind: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Image shape, FITS numeric conventions and original header.
pub struct ImageInfo {
    pub path: String,
    pub hdu: usize,
    pub width: usize,
    pub height: usize,
    pub bitpix: i32,
    pub bscale: f64,
    pub bzero: f64,
    pub blank: Option<i64>,
    pub hdus: Vec<HduInfo>,
    pub header: String,
}

/// Keep storage types intact; only the display uses floating-point values.
/// 保留存储类型；只在显示计算时转换为浮点值。
#[derive(Clone, Debug)]
pub enum RawPixels {
    U8(Vec<u8>),
    I16(Vec<i16>),
    I32(Vec<i32>),
    I64(Vec<i64>),
    F32(Vec<f32>),
    F64(Vec<f64>),
}

impl RawPixels {
    pub fn len(&self) -> usize {
        match self {
            Self::U8(v) => v.len(),
            Self::I16(v) => v.len(),
            Self::I32(v) => v.len(),
            Self::I64(v) => v.len(),
            Self::F32(v) => v.len(),
            Self::F64(v) => v.len(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn raw_string(&self, i: usize) -> String {
        match self {
            Self::U8(v) => v[i].to_string(),
            Self::I16(v) => v[i].to_string(),
            Self::I32(v) => v[i].to_string(),
            Self::I64(v) => v[i].to_string(),
            Self::F32(v) => v[i].to_string(),
            Self::F64(v) => v[i].to_string(),
        }
    }
    fn numeric(&self, i: usize, blank: Option<i64>) -> Option<f64> {
        let integer = match self {
            Self::U8(v) => Some(v[i] as i64),
            Self::I16(v) => Some(v[i] as i64),
            Self::I32(v) => Some(v[i] as i64),
            Self::I64(v) => Some(v[i]),
            _ => None,
        };
        if integer.is_some() && integer == blank {
            return None;
        }
        let value = match self {
            Self::U8(v) => v[i] as f64,
            Self::I16(v) => v[i] as f64,
            Self::I32(v) => v[i] as f64,
            Self::I64(v) => v[i] as f64,
            Self::F32(v) => v[i] as f64,
            Self::F64(v) => v[i],
        };
        value.is_finite().then_some(value)
    }
}

#[derive(Clone, Debug)]
/// Typed storage pixels and the metadata needed to interpret them.
pub struct FitsImage {
    pub info: ImageInfo,
    pub pixels: RawPixels,
}

impl FitsImage {
    pub fn physical(&self, i: usize) -> Option<f64> {
        let value = self.pixels.numeric(i, self.info.blank)?;
        let value = value.mul_add(self.info.bscale, self.info.bzero);
        value.is_finite().then_some(value)
    }
    /// Binary framing preserves i64/f64 and NaNs across HTTP, without JSON arrays.
    /// 二进制封包保留 i64/f64 和 NaN，避免把像素数组转换成 JSON。
    pub fn to_packet(&self) -> Result<Vec<u8>, String> {
        let header = serde_json::to_vec(&self.info).map_err(|e| e.to_string())?;
        let mut packet = Vec::with_capacity(4 + header.len() + self.pixels.len() * 8);
        packet.extend_from_slice(&(header.len() as u32).to_le_bytes());
        packet.extend_from_slice(&header);
        macro_rules! write {
            ($values:expr) => {
                for value in $values {
                    packet.extend_from_slice(&value.to_le_bytes());
                }
            };
        }
        match &self.pixels {
            RawPixels::U8(v) => packet.extend_from_slice(v),
            RawPixels::I16(v) => write!(v),
            RawPixels::I32(v) => write!(v),
            RawPixels::I64(v) => write!(v),
            RawPixels::F32(v) => write!(v),
            RawPixels::F64(v) => write!(v),
        }
        Ok(packet)
    }
    pub fn from_packet(packet: &[u8]) -> Result<Self, String> {
        if packet.len() < 4 {
            return Err("Incomplete image response".into());
        }
        let n = u32::from_le_bytes(packet[..4].try_into().unwrap()) as usize;
        if n > 1024 * 1024 || packet.len() < 4 + n {
            return Err("Invalid image metadata length".into());
        }
        let info: ImageInfo =
            serde_json::from_slice(&packet[4..4 + n]).map_err(|e| e.to_string())?;
        let count = checked_pixels(info.width, info.height)?;
        let bytes = &packet[4 + n..];
        let size = (info.bitpix.unsigned_abs() / 8) as usize;
        if size == 0 || bytes.len() != count * size {
            return Err("Incomplete pixel data".into());
        }
        macro_rules! read {
            ($ty:ty, $variant:ident, $n:literal) => {
                RawPixels::$variant(
                    bytes
                        .chunks_exact($n)
                        .map(|b| <$ty>::from_le_bytes(b.try_into().unwrap()))
                        .collect(),
                )
            };
        }
        let pixels = match info.bitpix {
            8 => RawPixels::U8(bytes.to_vec()),
            16 => read!(i16, I16, 2),
            32 => read!(i32, I32, 4),
            64 => read!(i64, I64, 8),
            -32 => read!(f32, F32, 4),
            -64 => read!(f64, F64, 8),
            _ => return Err("Unsupported BITPIX".into()),
        };
        Ok(Self { info, pixels })
    }
}

fn checked_pixels(width: usize, height: usize) -> Result<usize, String> {
    let count = width
        .checked_mul(height)
        .ok_or("Image dimensions overflow")?;
    if count == 0 || count > MAX_PIXELS {
        return Err(format!(
            "Image exceeds the MVP limit of {MAX_PIXELS} pixels, or is empty"
        ));
    }
    Ok(count)
}

#[cfg(not(target_arch = "wasm32"))]
mod reader;
#[cfg(not(target_arch = "wasm32"))]
pub use reader::load_file;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Algorithms for selecting the displayed physical-value interval.
pub enum Limits {
    ZScale,
    Percentile,
    MinMax,
    Manual,
}
impl Limits {
    pub const ALL: [Self; 4] = [Self::ZScale, Self::Percentile, Self::MinMax, Self::Manual];
    pub fn name(self) -> &'static str {
        match self {
            Self::ZScale => "ZScale",
            Self::Percentile => "99.5%",
            Self::MinMax => "Min / Max",
            Self::Manual => "Manual",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Brightness mappings applied after clipping to the display interval.
pub enum Stretch {
    Linear,
    Log,
    Sqrt,
    Asinh,
}
impl Stretch {
    pub const ALL: [Self; 4] = [Self::Linear, Self::Log, Self::Sqrt, Self::Asinh];
    pub fn name(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Log => "Log",
            Self::Sqrt => "Sqrt",
            Self::Asinh => "Asinh",
        }
    }
    pub fn apply(self, x: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Self::Linear => x,
            Self::Log => (1000.0 * x).ln_1p() / 1001.0_f64.ln(),
            Self::Sqrt => x.sqrt(),
            Self::Asinh => (10.0 * x).asinh() / 10.0_f64.asinh(),
        }
    }
}

#[derive(Default)]
/// Finite physical-value extrema and a sorted sample of up to 1000 values.
pub struct Statistics {
    pub min: f64,
    pub max: f64,
    pub finite: usize,
    pub sample: Vec<f64>,
}
impl Statistics {
    pub fn new(image: &FitsImage) -> Self {
        let mut out = Self {
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
            ..Self::default()
        };
        let step = (image.pixels.len() / 1000).max(1);
        for i in 0..image.pixels.len() {
            if let Some(v) = image.physical(i) {
                out.min = out.min.min(v);
                out.max = out.max.max(v);
                out.finite += 1;
                if i % step == 0 && out.sample.len() < 1000 {
                    out.sample.push(v);
                }
            }
        }
        // Sparse masks can miss the grid; fall back to the first valid values.
        // 稀疏掩膜可能避开采样网格，必要时补取有效像素。
        if out.sample.len() < 5 {
            out.sample = (0..image.pixels.len())
                .filter_map(|i| image.physical(i))
                .take(1000)
                .collect();
        }
        out.sample.sort_by(f64::total_cmp);
        if out.finite == 0 {
            out.min = 0.0;
            out.max = 1.0;
        }
        out
    }
    pub fn bounds(&self, mode: Limits) -> (f64, f64) {
        let bounds = match mode {
            Limits::Percentile if !self.sample.is_empty() => (
                quantile(&self.sample, 0.0025),
                quantile(&self.sample, 0.9975),
            ),
            Limits::ZScale if self.sample.len() >= 5 => zscale(&self.sample, 0.25),
            _ => (self.min, self.max),
        };
        if bounds.0 < bounds.1 {
            bounds
        } else {
            (bounds.0, bounds.0 + bounds.0.abs().max(1.0) * 1e-6)
        }
    }
}
fn quantile(v: &[f64], p: f64) -> f64 {
    let position = (v.len() - 1) as f64 * p;
    let low = position.floor() as usize;
    let high = (low + 1).min(v.len() - 1);
    v[low] + (v[high] - v[low]) * (position - low as f64)
}

/// IRAF-style sampled line fit with iterative rejection. / IRAF 式采样直线拟合与迭代剔除。
pub fn zscale(sorted: &[f64], contrast: f64) -> (f64, f64) {
    let n = sorted.len();
    if n < 5 {
        return (
            *sorted.first().unwrap_or(&0.0),
            *sorted.last().unwrap_or(&1.0),
        );
    }
    let mut good = vec![true; n];
    let mut slope = 0.0;
    let grow = (n / 100).max(1);
    for _ in 0..5 {
        let mut count = 0.0;
        let mut sx = 0.0;
        let mut sy = 0.0;
        let mut sxx = 0.0;
        let mut sxy = 0.0;
        for (i, &y) in sorted.iter().enumerate() {
            if good[i] {
                let x = i as f64;
                count += 1.0;
                sx += x;
                sy += y;
                sxx += x * x;
                sxy += x * y;
            }
        }
        if count < (n / 2).max(5) as f64 {
            return (sorted[0], sorted[n - 1]);
        }
        let denom = count * sxx - sx * sx;
        if denom == 0.0 {
            break;
        }
        slope = (count * sxy - sx * sy) / denom;
        let intercept = (sy - slope * sx) / count;
        let mean = sorted
            .iter()
            .enumerate()
            .filter(|(i, _)| good[*i])
            .map(|(i, y)| y - (intercept + slope * i as f64))
            .sum::<f64>()
            / count;
        let sigma = (sorted
            .iter()
            .enumerate()
            .filter(|(i, _)| good[*i])
            .map(|(i, y)| (y - intercept - slope * i as f64 - mean).powi(2))
            .sum::<f64>()
            / count)
            .sqrt();
        if sigma == 0.0 {
            break;
        }
        let mut next = good.clone();
        for (i, y) in sorted.iter().enumerate() {
            if good[i] && (y - intercept - slope * i as f64 - mean).abs() > 2.5 * sigma {
                for flag in &mut next[i.saturating_sub(grow)..(i + grow + 1).min(n)] {
                    *flag = false;
                }
            }
        }
        if next == good {
            break;
        }
        good = next;
    }
    let median = quantile(sorted, 0.5);
    let center = (n - 1) as f64 / 2.0;
    let slope = slope / contrast.max(1e-9);
    let bounds = (
        (median - center * slope).max(sorted[0]),
        (median + (n as f64 - 1.0 - center) * slope).min(sorted[n - 1]),
    );
    if bounds.0 < bounds.1 {
        bounds
    } else {
        (sorted[0], sorted[n - 1])
    }
}

/// Display-only colormap controls; defaults leave stretched values unchanged.
/// 仅影响显示的色阶参数，默认值保持 stretch 后的灰度不变。
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayTone {
    pub bias: f64,
    pub contrast: f64,
}
impl Default for DisplayTone {
    fn default() -> Self {
        Self {
            bias: 0.5,
            contrast: 1.0,
        }
    }
}
impl DisplayTone {
    /// Map a finite stretched level to grayscale, clipping to [0, 1].
    /// 将有限的 stretch 灰度映射到 [0, 1]，不改变 FITS 像素或 Low/High。
    pub fn apply(self, level: f64) -> f64 {
        ((level - self.bias) * self.contrast + 0.5).clamp(0.0, 1.0)
    }
}

pub fn render_rgba(
    image: &FitsImage,
    low: f64,
    high: f64,
    stretch: Stretch,
    invert: bool,
) -> Vec<u8> {
    render_rgba_with_tone(image, low, high, stretch, invert, DisplayTone::default())
}

pub fn render_rgba_with_tone(
    image: &FitsImage,
    low: f64,
    high: f64,
    stretch: Stretch,
    invert: bool,
    tone: DisplayTone,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(image.pixels.len() * 4);
    let span = (high - low).max(f64::MIN_POSITIVE);
    // FITS y increases upwards; screen y increases downwards. / FITS 与屏幕纵轴方向相反。
    for screen_y in 0..image.info.height {
        let y = image.info.height - 1 - screen_y;
        for x in 0..image.info.width {
            let i = y * image.info.width + x;
            if let Some(value) = image.physical(i) {
                let mut t = tone.apply(stretch.apply((value - low) / span));
                if invert {
                    t = 1.0 - t;
                }
                let c = (t * 255.0).round() as u8;
                out.extend_from_slice(&[c, c, c, 255]);
            } else {
                out.extend_from_slice(&[95, 45, 110, 255]);
            }
        }
    }
    out
}
