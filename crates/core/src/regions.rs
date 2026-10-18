//! Pixel-based apertures, shared by GUI/CLI/export; all coordinates are zero-based.
//! 图形界面、CLI 和导出共用的像素孔径；内部坐标从零开始，界面读数从一开始。
use crate::FitsImage;
use serde::{Deserialize,Serialize};

#[derive(Clone,Debug,Serialize,Deserialize,PartialEq)]
#[serde(tag="kind",rename_all="snake_case")]
pub enum Shape {
    Circle{center:[f64;2],radius:f64},
    Ring{center:[f64;2],inner:f64,outer:f64},
    Ellipse{center:[f64;2],radii:[f64;2],angle_deg:f64},
    Rectangle{center:[f64;2],size:[f64;2],angle_deg:f64},
    Polygon{points:Vec<[f64;2]>,#[serde(default)]holes:Vec<Vec<[f64;2]>>},
    Union{shapes:Vec<Shape>},
    Path{points:Vec<[f64;2]>,width:f64,#[serde(default)]inner_width:f64,#[serde(default)]smooth:bool},
}
impl Shape {
    pub fn validate(&self)->Result<(),String>{self.validate_depth(0)}
    fn validate_depth(&self,depth:usize)->Result<(),String>{
        if depth>8{return Err("Region nesting exceeds eight levels".into());}
        let valid=match self {
            Self::Circle{center,radius}=>point(*center) && positive(*radius),
            Self::Ring{center,inner,outer}=>point(*center) && positive(*inner) && positive(*outer) && outer>inner,
            Self::Ellipse{center,radii,angle_deg}=>point(*center) && radii.iter().all(|v|positive(*v)) && angle_deg.is_finite(),
            Self::Rectangle{center,size,angle_deg}=>point(*center) && size.iter().all(|v|positive(*v)) && angle_deg.is_finite(),
            Self::Polygon{points,holes}=>points.len()>=3 && points.len()<=2048 && points.iter().all(|p|point(*p)) && holes.len()<=16 && holes.iter().all(|h|h.len()>=3 && h.len()<=2048 && h.iter().all(|p|point(*p))),
            Self::Union{shapes}=>!shapes.is_empty() && shapes.len()<=32 && shapes.iter().all(|s|s.validate_depth(depth+1).is_ok()),
            Self::Path{points,width,inner_width,..}=>points.len()>=2 && points.len()<=128 && points.iter().all(|p|point(*p)) && positive(*width) && inner_width.is_finite() && *inner_width>=0. && inner_width<width,
        };
        if valid {Ok(())}else{Err("Invalid or oversized region geometry".into())}
    }
    pub fn translated(&self,dx:f64,dy:f64)->Self{
        let shift=|p:[f64;2]|[p[0]+dx,p[1]+dy];
        match self {
            Self::Circle{center,radius}=>Self::Circle{center:shift(*center),radius:*radius},
            Self::Ring{center,inner,outer}=>Self::Ring{center:shift(*center),inner:*inner,outer:*outer},
            Self::Ellipse{center,radii,angle_deg}=>Self::Ellipse{center:shift(*center),radii:*radii,angle_deg:*angle_deg},
            Self::Rectangle{center,size,angle_deg}=>Self::Rectangle{center:shift(*center),size:*size,angle_deg:*angle_deg},
            Self::Polygon{points,holes}=>Self::Polygon{points:points.iter().map(|p|shift(*p)).collect(),holes:holes.iter().map(|h|h.iter().map(|p|shift(*p)).collect()).collect()},
            Self::Union{shapes}=>Self::Union{shapes:shapes.iter().map(|s|s.translated(dx,dy)).collect()},
            Self::Path{points,width,inner_width,smooth}=>Self::Path{points:points.iter().map(|p|shift(*p)).collect(),width:*width,inner_width:*inner_width,smooth:*smooth},
        }
    }
    pub fn bounds(&self)->[f64;4]{
        match self {
            Self::Circle{center,radius}=>[center[0]-radius,center[1]-radius,center[0]+radius,center[1]+radius],
            Self::Ring{center,outer,..}=>[center[0]-outer,center[1]-outer,center[0]+outer,center[1]+outer],
            Self::Ellipse{center,radii,..}=>{let r=radii[0].max(radii[1]);[center[0]-r,center[1]-r,center[0]+r,center[1]+r]},
            Self::Rectangle{center,size,..}=>{let r=size[0].hypot(size[1])/2.;[center[0]-r,center[1]-r,center[0]+r,center[1]+r]},
            Self::Polygon{points,..}=>point_bounds(points,0.),
            Self::Union{shapes}=>{let mut bounds=[f64::INFINITY,f64::INFINITY,f64::NEG_INFINITY,f64::NEG_INFINITY];for shape in shapes {let b=shape.bounds();bounds=[bounds[0].min(b[0]),bounds[1].min(b[1]),bounds[2].max(b[2]),bounds[3].max(b[3])];}bounds},
            Self::Path{points,width,..}=>point_bounds(points,*width),
        }
    }
    pub fn flattened(&self)->Self {
        if let Self::Union{shapes}=self{return Self::Union{shapes:shapes.iter().map(Self::flattened).collect()};}
        if let Self::Path{points,width,inner_width,smooth:true}=self {
            let mut out=Vec::new();
            for i in 0..points.len()-1 {
                let p0=points[i.saturating_sub(1)];let p1=points[i];let p2=points[i+1];let p3=points[(i+2).min(points.len()-1)];
                let steps=((p1[0]-p2[0]).hypot(p1[1]-p2[1]).ceil() as usize).clamp(8,64);
                for n in 0..steps {let t=n as f64/steps as f64;let mut p=[0.;2];for c in 0..2 {p[c]=0.5*((2.*p1[c])+(-p0[c]+p2[c])*t+(2.*p0[c]-5.*p1[c]+4.*p2[c]-p3[c])*t*t+(-p0[c]+3.*p1[c]-3.*p2[c]+p3[c])*t*t*t);}out.push(p);}
            }
            out.push(*points.last().unwrap());Self::Path{points:out,width:*width,inner_width:*inner_width,smooth:false}
        }else{self.clone()}
    }
    pub fn contains(&self,p:[f64;2])->bool {
        match self {
            Self::Circle{center,radius}=>distance(p,*center)<=*radius,
            Self::Ring{center,inner,outer}=>{let d=distance(p,*center);d>=*inner && d<=*outer},
            Self::Ellipse{center,radii,angle_deg}=>{let q=rotated(p,*center,*angle_deg);(q[0]/radii[0]).powi(2)+(q[1]/radii[1]).powi(2)<=1.},
            Self::Rectangle{center,size,angle_deg}=>{let q=rotated(p,*center,*angle_deg);q[0].abs()<=size[0]/2. && q[1].abs()<=size[1]/2.},
            Self::Polygon{points,holes}=>in_polygon(points,p) && !holes.iter().any(|h|in_polygon(h,p)),
            Self::Union{shapes}=>shapes.iter().any(|s|s.contains(p)),
            Self::Path{points,width,inner_width,..}=>{let d=points.windows(2).map(|pair|segment_distance(p,pair[0],pair[1])).chain(points.iter().skip(1).take(points.len().saturating_sub(2)).map(|v|distance(p,*v))).fold(f64::INFINITY,f64::min);d<=width/2. && (*inner_width==0. || d>=inner_width/2.)},
        }
    }
    fn cost(&self)->usize{match self {Self::Polygon{points,holes}=>points.len()+holes.iter().map(Vec::len).sum::<usize>(),Self::Union{shapes}=>shapes.iter().map(Self::cost).sum(),Self::Path{points,..}=>points.len()*2,_=>1}}
}
fn in_polygon(points:&[[f64;2]],p:[f64;2])->bool{let mut inside=false;let mut previous=points.len()-1;for i in 0..points.len(){let a=points[i];let b=points[previous];if (a[1]>p[1])!=(b[1]>p[1]) && p[0]<(b[0]-a[0])*(p[1]-a[1])/(b[1]-a[1])+a[0] {inside=!inside;}previous=i;}inside}
fn point(p:[f64;2])->bool{p.iter().all(|v|v.is_finite() && v.abs()<=1e7)}
fn positive(v:f64)->bool{v.is_finite() && v>0. && v<=10000.}
fn distance(a:[f64;2],b:[f64;2])->f64{(a[0]-b[0]).hypot(a[1]-b[1])}
fn rotated(p:[f64;2],c:[f64;2],angle:f64)->[f64;2]{let(s,cosa)=angle.to_radians().sin_cos();let x=p[0]-c[0];let y=p[1]-c[1];[x*cosa+y*s,-x*s+y*cosa]}
fn point_bounds(points:&[[f64;2]],padding:f64)->[f64;4]{let mut b=[f64::INFINITY,f64::INFINITY,f64::NEG_INFINITY,f64::NEG_INFINITY];for p in points {b[0]=b[0].min(p[0]-padding);b[1]=b[1].min(p[1]-padding);b[2]=b[2].max(p[0]+padding);b[3]=b[3].max(p[1]+padding);}b}
fn segment_distance(p:[f64;2],a:[f64;2],b:[f64;2])->f64{let d=[b[0]-a[0],b[1]-a[1]];let n=d[0]*d[0]+d[1]*d[1];if n==0.{return distance(p,a);}let t=((p[0]-a[0])*d[0]+(p[1]-a[1])*d[1])/n;if !(0.0..=1.0).contains(&t){return f64::INFINITY;}distance(p,[a[0]+t*d[0],a[1]+t*d[1]])}

#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Aperture {pub name:String,pub shape:Shape,pub background:Option<Shape>,#[serde(default)]pub exclude:Vec<Shape>}
#[derive(Clone,Copy,Debug,Serialize,Deserialize)]
pub struct NoiseModel {pub electrons_per_unit:f64,pub read_noise_electrons:f64}
#[derive(Clone,Debug,Serialize,Deserialize)]
#[serde(default)]
pub struct MeasureOptions {pub subpixels:u8,pub clip_sigma:f64,pub saturation:Option<f64>,pub noise:Option<NoiseModel>}
impl Default for MeasureOptions {fn default()->Self{Self{subpixels:16,clip_sigma:3.,saturation:None,noise:None}}}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Measurement {
    pub aperture_sum:f64,pub effective_area:f64,pub background_per_pixel:Option<f64>,pub background_area:f64,
    pub net_flux:f64,pub total_error:Option<f64>,pub snr:Option<f64>,pub flags:Vec<String>,pub method:String,
}

pub fn measure(image:&FitsImage,aperture:&Aperture,options:&MeasureOptions,errors:Option<&[f64]>)->Result<Measurement,String>{
    aperture.shape.validate()?;if let Some(background)=&aperture.background{background.validate()?;}
    if aperture.exclude.len()>64{return Err("Too many exclusion regions".into());}for shape in &aperture.exclude{shape.validate()?;}
    if options.subpixels==0 || options.subpixels>32 || !options.clip_sigma.is_finite() || options.clip_sigma<=0. || options.saturation.is_some_and(|v|!v.is_finite()) {return Err("Invalid aperture sampling/noise options".into());}
    if errors.is_some_and(|e|e.len()!=image.pixels.len()){return Err("Error map shape differs from image".into());}
    if let Some(noise)=options.noise {if (!noise.electrons_per_unit.is_finite() || noise.electrons_per_unit<=0.) || !noise.read_noise_electrons.is_finite() || noise.read_noise_electrons<0. {return Err("Invalid explicit electron gain/read noise".into());}}
    let source=aperture.shape.flattened();let background=aperture.background.as_ref().map(Shape::flattened);let excluded=aperture.exclude.iter().map(Shape::flattened).collect::<Vec<_>>();
    let mut bounds=source.bounds();if let Some(b)=&background{let b=b.bounds();bounds=[bounds[0].min(b[0]),bounds[1].min(b[1]),bounds[2].max(b[2]),bounds[3].max(b[3])];}
    let xmin=(bounds[0]-0.5).floor().max(0.) as usize;let ymin=(bounds[1]-0.5).floor().max(0.) as usize;
    let xmax=(bounds[2]+0.5).ceil().clamp(0.,image.info.width as f64) as usize;let ymax=(bounds[3]+0.5).ceil().clamp(0.,image.info.height as f64) as usize;
    let pixels=xmax.saturating_sub(xmin).checked_mul(ymax.saturating_sub(ymin)).ok_or("Aperture dimensions overflow")?;
    let complexity=source.cost()+background.as_ref().map_or(0,Shape::cost)+excluded.iter().map(Shape::cost).sum::<usize>();
    if pixels>1_000_000 || pixels.saturating_mul(usize::from(options.subpixels).pow(2)).saturating_mul(complexity)>180_000_000 {return Err("Aperture exceeds the bounded geometry sampling budget; reduce the region, boundary vertices or subpixel count".into());}
    let samples=f64::from(options.subpixels).powi(2);let mut sum=0.;let mut area=0.;let mut variance=0.;let mut complete_errors=true;let mut bg=Vec::new();let mut flags=Vec::new();
    if bounds[0]< -0.5 || bounds[1]< -0.5 || bounds[2]>image.info.width as f64-0.5 || bounds[3]>image.info.height as f64-0.5{flags.push("Region clipped by image boundary".into());}
    for y in ymin..ymax {for x in xmin..xmax {
        let mut sw=0.;let mut bw=0.;
        for sy in 0..options.subpixels {for sx in 0..options.subpixels {
            let p=[x as f64-0.5+(f64::from(sx)+0.5)/f64::from(options.subpixels),y as f64-0.5+(f64::from(sy)+0.5)/f64::from(options.subpixels)];
            if excluded.iter().any(|e|e.contains(p)){continue;}
            if source.contains(p){sw+=1.;}else if background.as_ref().is_some_and(|b|b.contains(p)){bw+=1.;}
        }}
        sw/=samples;bw/=samples;if sw==0. && bw==0. {continue;}
        let i=y*image.info.width+x;
        let Some(value)=image.physical(i) else {if !flags.iter().any(|f|f=="Invalid source/background pixels excluded"){flags.push("Invalid source/background pixels excluded".into());}continue;};
        sum+=sw*value;area+=sw;
        let noise_variance=if let Some(errors)=errors {if errors[i].is_finite() && errors[i]>=0.{Some(errors[i]*errors[i])}else{complete_errors=false;None}}else{options.noise.map(|noise|value.max(0.)/noise.electrons_per_unit+(noise.read_noise_electrons/noise.electrons_per_unit).powi(2))};
        if bw>0.{bg.push((value,bw,noise_variance,sw));}
        if sw>0. {
            if options.saturation.is_some_and(|v|value>=v) && !flags.iter().any(|f|f=="Manual saturation threshold reached"){flags.push("Manual saturation threshold reached".into());}
            if let Some(value)=noise_variance{variance+=sw*sw*value;}
        }
    }}
    if area==0.{return Err("No valid pixels inside the aperture".into());}
    for _ in 0..4 {
        if bg.len()<3{break;}
        let mut values=bg.iter().map(|p|p.0).collect::<Vec<_>>();values.sort_by(f64::total_cmp);let median=values[values.len()/2];
        let mut deviations=values.iter().map(|v|(v-median).abs()).collect::<Vec<_>>();deviations.sort_by(f64::total_cmp);
        let sigma=deviations[deviations.len()/2]*1.482602218505602;let limit=(options.clip_sigma*sigma).max(median.abs().max(1.)*f64::EPSILON*8.);
        let n=bg.len();bg.retain(|(v,_,_,_)|(v-median).abs()<=limit);if bg.len()==n{break;}
    }
    let (mean,_bg_variance,bg_area,_bg_effective)=background_stats(&bg);
    let background_mean_variance=if bg_area>0.{bg.iter().map(|p|p.1*p.1*p.2.unwrap_or(0.)).sum::<f64>()/(bg_area*bg_area)}else{0.};
    let background_mean=(!bg.is_empty()).then_some(mean);
    if aperture.background.is_some() && bg.is_empty(){return Err("No valid pixels in the background region".into());}
    if background_mean.is_none(){flags.push("No background subtraction".into());}
    // Fractional source/background coverage can share the same noisy detector pixel.
    // 亚像素源区与背景区可能共享原像素，净通量误差需扣除协方差。
    let covariance=if bg_area>0.{area/bg_area*bg.iter().map(|p|p.1*p.3*p.2.unwrap_or(0.)).sum::<f64>()}else{0.};
    let total_error=if (errors.is_some() || options.noise.is_some()) && complete_errors {Some((variance+area*area*background_mean_variance-2.*covariance).max(0.).sqrt())}else{flags.push("Total uncertainty not computed: provide an error map or explicit electron gain/read noise".into());None};
    let net_flux=sum-mean*area;
    Ok(Measurement{aperture_sum:sum,effective_area:area,background_per_pixel:background_mean,background_area:bg_area,net_flux,total_error,snr:total_error.filter(|e|*e>0.).map(|e|net_flux/e),flags,method:format!("{}x{} subpixel union coverage; MAD-clipped weighted-mean background; original physical values",options.subpixels,options.subpixels)})
}
fn background_stats(values:&[(f64,f64,Option<f64>,f64)])->(f64,f64,f64,f64){
    let weight=values.iter().map(|p|p.1).sum::<f64>();if weight==0.{return (0.,0.,0.,0.);}
    let mean=values.iter().map(|p|p.0*p.1).sum::<f64>()/weight;let squared=values.iter().map(|p|p.1*p.1).sum::<f64>();let effective=weight*weight/squared;
    let variance=values.iter().map(|p|p.1*(p.0-mean).powi(2)).sum::<f64>()/weight * if effective>1.{effective/(effective-1.)}else{0.};(mean,variance,weight,effective)
}
pub fn growth_curve(image:&FitsImage,aperture:&Aperture,options:&MeasureOptions,radii:&[f64])->Result<Vec<(f64,Measurement)>,String>{
    growth_curve_with_errors(image,aperture,options,radii,None)
}
pub fn growth_curve_with_errors(image:&FitsImage,aperture:&Aperture,options:&MeasureOptions,radii:&[f64],errors:Option<&[f64]>)->Result<Vec<(f64,Measurement)>,String>{
    if radii.is_empty() || radii.len()>32 || radii.iter().any(|r|!positive(*r)){return Err("Growth curve requires one to 32 valid radii".into());}
    let Shape::Circle{center,..}=&aperture.shape else{return Err("Growth curves require a circular aperture".into());};
    let mut output=Vec::new();for &radius in radii{let mut region=aperture.clone();region.shape=Shape::Circle{center:*center,radius};output.push((radius,measure(image,&region,options,errors)?));}Ok(output)
}

#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct RegionTemplate {pub schema:u32,pub source_dimensions:[usize;2],pub source_hdu:usize,pub source_header:String,pub apertures:Vec<Aperture>,#[serde(default)]pub options:Option<MeasureOptions>,#[serde(default)]pub source_units:String,#[serde(default="complete_wcs")]pub source_wcs_complete:bool}
fn complete_wcs()->bool{true}
impl RegionTemplate {
    pub fn parse(text:&str)->Result<Self,String>{
        if text.len()>256*1024{return Err("Region template exceeds 256 KiB".into());}
        let value:Self=serde_json::from_str(text).map_err(|e|e.to_string())?;
        if value.schema!=1 || value.apertures.len()>64 || value.source_header.len()>128*1024 {return Err("Unsupported or oversized region template".into());}
        for region in &value.apertures {region.shape.validate()?;if let Some(background)=&region.background{background.validate()?;}for excluded in &region.exclude{excluded.validate()?;}}
        Ok(value)
    }
}
