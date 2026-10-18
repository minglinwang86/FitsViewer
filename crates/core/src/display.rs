//! Small shared display settings; no plotting or scientific runtime is bundled.
//! 轻量共享显示参数，不捆绑科研运行环境或额外绘图库。
use crate::{DisplayTone, FitsImage, Stretch};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<[u8; 4]>,
}
impl Default for Palette {
    fn default() -> Self { Self::builtin("Gray") }
}
impl Palette {
    pub const NAMES: [&'static str; 5] = ["Gray", "Heat", "Cool", "Blue-orange", "Red"];
    pub fn builtin(name: &str) -> Self {
        let rgb: &[[u8; 3]] = match name {
            "Heat" => &[[0,0,0],[150,0,0],[255,140,0],[255,255,220]],
            "Cool" => &[[0,15,35],[0,120,180],[80,220,220],[240,255,255]],
            "Blue-orange" => &[[20,45,130],[90,160,220],[235,235,235],[240,150,45],[140,45,10]],
            "Red" => &[[0,0,0],[255,70,50]],
            _ => &[[0,0,0],[255,255,255]],
        };
        Self { name: if Self::NAMES.contains(&name) {name.into()} else {"Gray".into()}, colors: rgb.iter().map(|p|[p[0],p[1],p[2],255]).collect() }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.name.len()>80 || !(2..=256).contains(&self.colors.len()) {return Err("A palette requires 2 to 256 RGBA control points and a short name".into());}
        Ok(())
    }
    pub fn sample(&self, level: f64) -> [u8; 4] {
        if self.colors.len()<2 {return [0,0,0,255];}
        let t=level.clamp(0.,1.)*(self.colors.len()-1) as f64;
        let lo=(t.floor() as usize).min(self.colors.len()-1);let hi=(lo+1).min(self.colors.len()-1);
        let mut out=[0;4];
        for (c,value) in out.iter_mut().enumerate(){*value=(f64::from(self.colors[lo][c])+(f64::from(self.colors[hi][c])-f64::from(self.colors[lo][c]))*(t-lo as f64)).round() as u8;}
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum MaskStyle { Fill, Outline }
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ThresholdMask {
    pub threshold: f64,
    pub opacity: f32,
    pub color: [u8;3],
    pub style: MaskStyle,
}
impl ThresholdMask {
    pub fn validate(&self)->Result<(),String>{
        if !self.threshold.is_finite() || !self.opacity.is_finite() || !(0.0..=1.0).contains(&self.opacity){return Err("Invalid physical threshold or mask opacity".into());}Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RenderOptions { pub palette: Palette, pub mask: Option<ThresholdMask>, pub stretch_exponent:f64 }
impl Default for RenderOptions {fn default()->Self{Self{palette:Palette::default(),mask:None,stretch_exponent:1000.}}}
impl RenderOptions {
    pub fn validate(&self)->Result<(),String>{self.palette.validate()?;if !self.stretch_exponent.is_finite() || self.stretch_exponent<=1. || self.stretch_exponent>1e6{return Err("Stretch exponent must be finite and greater than 1, at most 1,000,000".into());}if let Some(mask)=&self.mask {mask.validate()?;}Ok(())}
}

/// Bounded transfer table shared by the image and colorbar. Reuse it for tone changes.
/// 固定大小的映射表供图像与色条共用；亮度/对比度变化无需重新统计图像。
pub struct ScaleMapping {low:f64,high:f64,stretch:Stretch,exponent:f64,cdf:Option<Vec<f64>>}
impl ScaleMapping {
    pub const BINS:usize=4096;
    pub fn new(image:Option<&FitsImage>,low:f64,high:f64,stretch:Stretch,exponent:f64)->Self{
        let mut out=Self{low,high,stretch,exponent,cdf:None};
        if stretch!=Stretch::HistogramEqualization || high<=low{return out;}
        let Some(image)=image else{return out;};
        let mut counts=vec![0u64;Self::BINS];
        for i in 0..image.pixels.len(){if let Some(v)=image.physical(i){if v>=low && v<=high{let bin=(((v-low)/(high-low)*(Self::BINS-1) as f64).floor() as usize).min(Self::BINS-1);counts[bin]+=1;}}}
        let total=counts.iter().sum::<u64>();let first=counts.iter().copied().find(|v|*v>0).unwrap_or(0);
        if total>first{let mut sum=0u64;out.cdf=Some(counts.into_iter().map(|n|{sum+=n;sum.saturating_sub(first) as f64/(total-first) as f64}).collect());}
        out
    }
    pub fn matches(&self,low:f64,high:f64,stretch:Stretch,exponent:f64)->bool{self.low==low && self.high==high && self.stretch==stretch && self.exponent==exponent}
    pub fn apply(&self,x:f64)->f64{
        let x=x.clamp(0.,1.);
        if let Some(cdf)=&self.cdf{
            if x<=0. || x>=1.{return x;}
            let t=x*(Self::BINS-1) as f64;let i=(t.floor() as usize).min(Self::BINS-1);let next=(i+1).min(Self::BINS-1);
            cdf[i]+(cdf[next]-cdf[i])*(t-i as f64)
        }else{self.stretch.apply_with_exponent(x,self.exponent)}
    }
    pub fn color(&self,value:f64,invert:bool,tone:DisplayTone,palette:&Palette)->[u8;4]{
        let mut level=tone.apply(self.apply((value-self.low)/(self.high-self.low).max(f64::MIN_POSITIVE)));
        if invert{level=1.-level;}palette.sample(level)
    }
}

/// The same physical-value mapping feeds images, colorbars and exports.
/// 图像、色条和导出共用物理值到颜色的映射。
pub fn display_color(value:f64,low:f64,high:f64,stretch:Stretch,invert:bool,tone:DisplayTone,palette:&Palette)->[u8;4]{
    ScaleMapping::new(None,low,high,stretch,1000.).color(value,invert,tone,palette)
}

pub fn render(image:&FitsImage,low:f64,high:f64,stretch:Stretch,invert:bool,tone:DisplayTone,options:&RenderOptions)->Vec<u8>{
    let mapping=ScaleMapping::new(Some(image),low,high,stretch,options.stretch_exponent);
    render_with_mapping(image,invert,tone,options,&mapping)
}
pub fn render_with_mapping(image:&FitsImage,invert:bool,tone:DisplayTone,options:&RenderOptions,mapping:&ScaleMapping)->Vec<u8>{
    let mut out=Vec::with_capacity(image.pixels.len()*4);
    for screen_y in 0..image.info.height {
        let y=image.info.height-1-screen_y;
        for x in 0..image.info.width {
            let i=y*image.info.width+x;
            let Some(value)=image.physical(i) else {out.extend_from_slice(&[95,45,110,255]);continue;};
            let mut color=mapping.color(value,invert,tone,&options.palette);
            if let Some(mask)=&options.mask {
                let boundary=mask.style==MaskStyle::Fill || [(x.wrapping_sub(1),y),(x+1,y),(x,y.wrapping_sub(1)),(x,y+1)].iter().any(|&(nx,ny)|nx>=image.info.width || ny>=image.info.height || image.physical(ny*image.info.width+nx).is_none_or(|v|v<mask.threshold));
                if value>=mask.threshold && boundary {
                    for c in 0..3 {color[c]=(f32::from(color[c])*(1.-mask.opacity)+f32::from(mask.color[c])*mask.opacity).round() as u8;}
                }
            }
            out.extend_from_slice(&color);
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct Histogram {pub minimum:f64,pub maximum:f64,pub counts:Vec<u64>,pub finite:usize,pub sampled:bool}
impl Histogram {
    /// Cached by the UI until another image/range is selected; pointer motion does not rescan.
    /// 由界面缓存，只有图像或统计范围变化时重新计算。
    pub fn new(image:&FitsImage,bins:usize)->Self{
        let bins=bins.clamp(8,1024);let mut lo=f64::INFINITY;let mut hi=f64::NEG_INFINITY;let mut finite=0;
        for i in 0..image.pixels.len(){if let Some(v)=image.physical(i){lo=lo.min(v);hi=hi.max(v);finite+=1;}}
        if finite==0 {lo=0.;hi=1.;}else if lo==hi {hi=lo+lo.abs().max(1.)*1e-6;}
        let mut counts=vec![0;bins];
        for i in 0..image.pixels.len(){if let Some(v)=image.physical(i){let bin=(((v-lo)/(hi-lo)*bins as f64).floor() as usize).min(bins-1);counts[bin]+=1;}}
        Self{minimum:lo,maximum:hi,counts,finite,sampled:false}
    }
}
