//! FITS numeric data and display transformations. / FITS 数值与显示变换。
use serde::{Deserialize, Serialize};
pub mod display;
pub mod regions;
pub mod view;
pub mod remote;

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spectrum: Option<SpectrumMeta>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SpectrumMeta {
    pub x_label: String,
    pub x_unit: String,
    pub y_label: String,
    pub y_unit: String,
    pub columns: Vec<String>,
    pub x_column: usize,
    pub y_column: usize,
    pub error_column: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_hdu: Option<usize>,
    pub uncertainty_kind: String,
    pub note: String,
}

impl SpectrumMeta {
    /// The packet stores sigma for either an uncertainty column or image HDU.
    /// 误差列或误差图像 HDU 均在封包末尾保存标准差，不伪造列索引。
    pub fn has_error(&self) -> bool { self.error_column.is_some() || self.error_hdu.is_some() }
    pub fn source_error_kind(&self)->ErrorKind{match self.uncertainty_kind.as_str(){"standard deviation"|"sigma"=>ErrorKind::Sigma,"variance"=>ErrorKind::Variance,"inverse variance"=>ErrorKind::Ivar,_=>ErrorKind::Auto}}
}

#[derive(Clone, Debug)]
pub struct SpectrumData {
    pub x: Vec<f64>,
    /// Standard deviations after interpreting the source uncertainty convention.
    /// 按源文件的误差约定转换后的标准差，缺失或无效值保留为 NaN。
    pub error: Option<Vec<f64>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all="lowercase")]
pub enum ErrorKind {
    #[default]
    Auto,
    Sigma,
    Variance,
    Ivar,
}

impl ErrorKind {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Some(Self::Auto), "sigma" => Some(Self::Sigma),
            "variance" => Some(Self::Variance), "ivar" => Some(Self::Ivar), _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self { Self::Auto => "auto", Self::Sigma => "sigma", Self::Variance => "variance", Self::Ivar => "ivar" }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct LoadOptions {
    pub hdu: Option<usize>,
    pub x_column: Option<usize>,
    pub y_column: Option<usize>,
    pub error_column: Option<usize>,
    pub error_hdu: Option<usize>,
    pub error_kind: ErrorKind,
    #[serde(rename="no_error",alias="ignore_errors")]
    pub ignore_errors: bool,
}
impl LoadOptions{
    pub fn validate(&self)->Result<(),String>{
        if [self.hdu,self.error_hdu].iter().flatten().any(|index|*index>=4096) || [self.x_column,self.y_column,self.error_column].iter().flatten().any(|index|*index>=256){return Err("Input HDU/column mapping exceeds supported limits".into());}
        if self.ignore_errors && (self.error_column.is_some() || self.error_hdu.is_some()) || self.error_column.is_some() && self.error_hdu.is_some(){return Err("Choose one uncertainty source or no errors".into());}
        Ok(())
    }
}

/// Keep storage types intact; only the display uses floating-point values.
/// 保留存储类型；只在显示计算时转换为浮点值。
#[derive(Clone, Debug)]
pub enum RawPixels {
    /// Metadata-only remote view: no scientific samples are present on this client.
    /// 远程视图的显式空数据源；显示图块不能冒充原始科学像素。
    Unavailable,
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
            Self::Unavailable => 0,
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
        if i >= self.len() { return "unavailable".into(); }
        match self {
            Self::Unavailable => "unavailable".into(),
            Self::U8(v) => v[i].to_string(),
            Self::I16(v) => v[i].to_string(),
            Self::I32(v) => v[i].to_string(),
            Self::I64(v) => v[i].to_string(),
            Self::F32(v) => v[i].to_string(),
            Self::F64(v) => v[i].to_string(),
        }
    }
    fn numeric(&self, i: usize, blank: Option<i64>) -> Option<f64> {
        if i >= self.len() { return None; }
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
            Self::Unavailable => return None,
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
    pub spectrum: Option<SpectrumData>,
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
        if matches!(self.pixels, RawPixels::Unavailable) {
            return Err("Remote metadata has no original pixels to serialize".into());
        }
        if self.pixels.len() != checked_pixels(self.info.width, self.info.height)? {
            return Err("Image shape does not match the stored pixel count".into());
        }
        match (&self.spectrum, &self.info.spectrum) {
            (Some(data), Some(meta)) if data.x.len() == self.pixels.len()
                && data.error.as_ref().map_or(true, |e| e.len() == data.x.len())
                && data.error.is_some() == meta.has_error() => {},
            (None, None) => {},
            _ => return Err("Inconsistent spectrum arrays or uncertainty source".into()),
        }
        let header = serde_json::to_vec(&self.info).map_err(|e| e.to_string())?;
        if header.len() > 1024 * 1024 { return Err("Image metadata exceeds 1 MiB".into()); }
        let pixel_bytes = self.pixels.len().checked_mul((self.info.bitpix.unsigned_abs() / 8) as usize).ok_or("Packet size overflow")?;
        let spectrum_bytes = self.spectrum.as_ref().map_or(0, |s| s.x.len() * 8 * if s.error.is_some() { 2 } else { 1 });
        let capacity = 4usize.checked_add(header.len()).and_then(|n| n.checked_add(pixel_bytes)).and_then(|n| n.checked_add(spectrum_bytes)).ok_or("Packet size overflow")?;
        let mut packet = Vec::with_capacity(capacity);
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
            RawPixels::Unavailable => return Err("Original pixels are unavailable".into()),
            RawPixels::U8(v) => packet.extend_from_slice(v),
            RawPixels::I16(v) => write!(v),
            RawPixels::I32(v) => write!(v),
            RawPixels::I64(v) => write!(v),
            RawPixels::F32(v) => write!(v),
            RawPixels::F64(v) => write!(v),
        }
        if let Some(spectrum) = &self.spectrum {
            write!(&spectrum.x);
            if let Some(error) = &spectrum.error {
                write!(error);
            }
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
        let payload = &packet[4 + n..];
        let size = (info.bitpix.unsigned_abs() / 8) as usize;
        let spectral_bytes = info.spectrum.as_ref().map_or(0, |s| count * 8 * if s.has_error() { 2 } else { 1 });
        if size == 0 || payload.len() != count * size + spectral_bytes {
            return Err("Incomplete pixel data".into());
        }
        let (bytes, spectral) = payload.split_at(count * size);
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
        let spectrum = info.spectrum.as_ref().map(|meta| {
            let values = |bytes: &[u8]| bytes.chunks_exact(8).map(|b| f64::from_le_bytes(b.try_into().unwrap())).collect();
            SpectrumData {
                x: values(&spectral[..count * 8]),
                error: meta.has_error().then(|| values(&spectral[count * 8..])),
            }
        });
        Ok(Self { info, pixels, spectrum })
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

#[cfg(any(not(target_arch = "wasm32"),feature="offline"))]
mod reader;
#[cfg(not(target_arch = "wasm32"))]
pub use reader::{load_file, load_file_with_options, load_header};
#[cfg(any(not(target_arch = "wasm32"),feature="offline"))]
pub use reader::load_bytes_with_options;

/// Remove only a paired outer quote; preserve meaningful spaces and Unicode.
/// 仅移除粘贴路径的一对外层引号，保留有效空格和 Unicode 字符。
pub fn pasted_path(value: &str) -> &str {
    if value.len() >= 2 && ((value.starts_with('"') && value.ends_with('"'))
        || (value.starts_with('\'') && value.ends_with('\''))) {
        &value[1..value.len() - 1]
    } else { value }
}

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
    Power,
    Sqrt,
    Squared,
    Asinh,
    Sinh,
    HistogramEqualization,
}
impl Stretch {
    pub const ALL: [Self; 8] = [Self::Linear, Self::Log, Self::Power, Self::Sqrt, Self::Squared, Self::Asinh, Self::Sinh, Self::HistogramEqualization];
    pub fn name(self) -> &'static str {
        match self {
            Self::Linear => "Linear",
            Self::Log => "Log",
            Self::Power => "Power",
            Self::Sqrt => "Sqrt",
            Self::Squared => "Squared",
            Self::Asinh => "Asinh",
            Self::Sinh => "Sinh",
            Self::HistogramEqualization => "Histogram Equalization",
        }
    }
    pub fn parse(name: &str) -> Option<Self> {
        match name.to_ascii_lowercase().as_str() {
            "pow" => Some(Self::Power),
            "histequ" => Some(Self::HistogramEqualization),
            _ => Self::ALL.into_iter().find(|s| s.name().eq_ignore_ascii_case(name)),
        }
    }
    pub fn apply(self, x: f64) -> f64 {
        self.apply_with_exponent(x, 1000.)
    }
    /// Histogram equalization needs image context; without it the fallback is linear.
    /// 直方图均衡由 display::ScaleMapping 提供图像统计；无统计时回退线性。
    pub fn apply_with_exponent(self, x: f64, exponent: f64) -> f64 {
        let x = x.clamp(0.0, 1.0);
        match self {
            Self::Linear | Self::HistogramEqualization => x,
            Self::Log => (exponent * x).ln_1p() / exponent.ln_1p(),
            Self::Power => (exponent.ln() * x).exp_m1() / (exponent - 1.),
            Self::Sqrt => x.sqrt(),
            Self::Squared => x*x,
            Self::Asinh => (10.0 * x).asinh() / 10.0_f64.asinh(),
            Self::Sinh => (3.0*x).sinh() / 3.0_f64.sinh(),
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
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
    display::render(image, low, high, stretch, invert, tone, &display::RenderOptions::default())
}
