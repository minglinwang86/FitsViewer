//! Versioned, bounded view configuration shared by automation and interactive views.
//! 版本化视图配置：GUI 与无界面脚本使用相同的数值约束。
use crate::{display::RenderOptions,Stretch};
use serde::{Deserialize,Serialize};
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(default)]
pub struct View {
    pub schema:u32,pub low:f64,pub high:f64,pub stretch:String,pub invert:bool,
    pub bias:f64,pub contrast:f64,pub zoom:f64,pub pan:[f64;2],pub crop:Option<[f64;4]>,pub render:RenderOptions,pub plot_bounds:Option<[f64;4]>,
    pub annotations:Vec<serde_json::Value>,pub wcs_override:Option<String>,
    pub load_options:crate::LoadOptions,
}
impl Default for View {fn default()->Self{Self{schema:1,low:0.,high:1.,stretch:"Linear".into(),invert:false,bias:0.5,contrast:1.,zoom:1.,pan:[0.,0.],crop:None,render:RenderOptions::default(),plot_bounds:None,annotations:vec![],wcs_override:None,load_options:Default::default()}}}
impl View {
    pub fn parse(text:&str)->Result<Self,String>{if text.len()>64*1024{return Err("View configuration exceeds 64 KiB".into());}let value:Self=serde_json::from_str(text).map_err(|e|e.to_string())?;value.validate()?;Ok(value)}
    pub fn validate(&self)->Result<(),String>{
        if self.schema!=1 || !self.low.is_finite() || !self.high.is_finite() || self.high<=self.low || Stretch::parse(&self.stretch).is_none() || !(0. ..=1.).contains(&self.bias) || !(0.05..=20.).contains(&self.contrast) || !(0.005..=128.).contains(&self.zoom) || self.pan.iter().any(|v|!v.is_finite() || v.abs()>1e8) {return Err("Invalid view schema, limits, stretch or viewport".into());}
        if self.crop.is_some_and(|v|v.iter().any(|v|!v.is_finite()) || v[0]>=v[2] || v[1]>=v[3]){return Err("Crop must contain increasing finite pixel bounds".into());}
        if self.plot_bounds.is_some_and(|v|v.iter().any(|v|!v.is_finite()) || v[0]>=v[1] || v[2]>=v[3]){return Err("Spectrum bounds must be finite and increasing".into());}
        if self.annotations.len()>256 || self.wcs_override.as_ref().is_some_and(|s|s.len()>65536){return Err("Oversized annotations or WCS override".into());}
        self.load_options.validate()?;self.render.validate()
    }
}
