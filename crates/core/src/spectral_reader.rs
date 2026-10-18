use super::{card, number};
use crate::*;
use fitsrs::hdu::data::bintable::{ColumnId, DataValue};
use std::path::Path;

const MAX_SAMPLES: usize = 1_000_000;
fn normalized(value: &str) -> String { value.trim().to_ascii_uppercase().replace([' ', '-'], "_") }
fn wave(names: &[String]) -> Option<usize> {
    names.iter().position(|s| ["WAVE","WAVELENGTH","LAMBDA","LOGLAM","LOG_LAMBDA"].contains(&normalized(s).as_str()))
}
fn flux(names: &[String]) -> Option<usize> {
    names.iter().position(|s| ["FLUX","FLUXCAL","FLUX_CAL","FLUX_NORM","SCI","SPEC"].contains(&normalized(s).as_str()))
}
fn error(names: &[String]) -> Option<usize> {
    names.iter().position(|s| ["ERR","ERROR","SIGMA","STDDEV","STD_DEV","FLUX_ERR","FLUX_ERROR","STATERROR","STAT_ERROR","IVAR","INVVAR","INVERSE_VARIANCE","VAR","VARIANCE"].contains(&normalized(s).as_str()))
}
pub(super) fn columns_match(names: &[String]) -> bool { wave(names).is_some() && flux(names).is_some() }

pub(super) fn validate_error_options(options: LoadOptions) -> Result<(), String> {
    options.validate()?;
    if options.ignore_errors && (options.error_column.is_some() || options.error_hdu.is_some()) {
        return Err("Choose either an uncertainty source or no errors".into());
    }
    if options.error_column.is_some() && options.error_hdu.is_some() {
        return Err("Choose either an uncertainty column or an uncertainty HDU".into());
    }
    Ok(())
}

pub(super) fn resolve_error_kind(requested: ErrorKind, name: &str) -> Result<ErrorKind, String> {
    if requested != ErrorKind::Auto { return Ok(requested); }
    match normalized(name).as_str() {
        "ERR"|"ERROR"|"SIGMA"|"STDDEV"|"STD_DEV"|"FLUX_ERR"|"FLUX_ERROR"|"STATERROR"|"STAT_ERROR"|"SYSERROR"|"SYS_ERROR" => Ok(ErrorKind::Sigma),
        "VAR"|"VARIANCE" => Ok(ErrorKind::Variance),
        "IVAR"|"INVVAR"|"INVERSE_VARIANCE" => Ok(ErrorKind::Ivar),
        _ => Err("Uncertainty type is unspecified: choose sigma, variance or ivar explicitly for this column/HDU".into()),
    }
}

pub(super) fn error_label(kind: ErrorKind) -> &'static str {
    match kind { ErrorKind::Sigma => "standard deviation", ErrorKind::Variance => "variance", ErrorKind::Ivar => "inverse variance", ErrorKind::Auto => "" }
}

pub(super) fn to_sigma(value: f64, kind: ErrorKind) -> f64 {
    if !value.is_finite() { return f64::NAN; }
    match kind {
        ErrorKind::Sigma if value >= 0. => value,
        ErrorKind::Variance if value >= 0. => value.sqrt(),
        ErrorKind::Ivar if value > 0. => 1. / value.sqrt(),
        _ => f64::NAN,
    }
}

fn from_columns(mut info: ImageInfo, names: Vec<String>, units: Vec<String>, columns: Vec<Vec<f64>>, options: LoadOptions) -> Result<FitsImage,String> {
    validate_error_options(options)?;
    if options.error_hdu.is_some() { return Err("An uncertainty HDU can only accompany a one-dimensional FITS image spectrum".into()); }
    let xcol=options.x_column.or_else(||wave(&names)).unwrap_or(0);
    let ycol=options.y_column.or_else(||flux(&names)).unwrap_or(1);
    if xcol>=columns.len() || ycol>=columns.len() { return Err("Spectrum column index is out of bounds".into()); }
    let ecol=if options.ignore_errors{None}else{match options.error_column {Some(i) if i>=columns.len()=>return Err("Spectrum error-column index is out of bounds".into()),Some(i)=>Some(i),None=>error(&names)}};
    let x=columns[xcol].clone(); let y=columns[ycol].clone();
    if x.len()!=y.len() || x.len()<2 || x.len()>MAX_SAMPLES { return Err("Spectrum requires 2 to 1,000,000 paired samples".into()); }
    let kind=ecol.map(|c|resolve_error_kind(options.error_kind,&names[c])).transpose()?;
    let errors=if let Some(c)=ecol {
        if columns[c].len()!=x.len() { return Err("Uncertainty length differs from spectrum".into()); }
        Some(columns[c].iter().map(|&e|to_sigma(e,kind.unwrap())).collect())
    } else { None };
    let logx=["LOGLAM","LOG_LAMBDA"].contains(&normalized(&names[xcol]).as_str());
    let x=if logx { x.into_iter().map(|v|10_f64.powf(v)).collect() } else { x };
    info.width=y.len(); info.height=1; info.bitpix=-64; info.bscale=1.; info.bzero=0.; info.blank=None;
    info.spectrum=Some(SpectrumMeta { x_label:if logx { format!("10^{}",names[xcol]) } else { names[xcol].clone() },x_unit:units.get(xcol).cloned().unwrap_or_default(),
        y_label:names[ycol].clone(),y_unit:units.get(ycol).cloned().unwrap_or_default(),columns:names,x_column:xcol,y_column:ycol,error_column:ecol,error_hdu:None,
        uncertainty_kind:kind.map(error_label).unwrap_or("").into(),note:if logx { "Explicit log10 coordinate column; source units retained" } else if units.get(xcol).is_some_and(|u|!u.is_empty()) && units.get(ycol).is_some_and(|u|!u.is_empty()) { "Source units retained; original samples are shown" } else { "Coordinate or flux unit unspecified; no unit is assumed" }.into() });
    Ok(FitsImage {info,pixels:RawPixels::F64(y),spectrum:Some(SpectrumData{x,error:errors})})
}

pub(super) fn image_axis(image: &mut FitsImage) -> Result<(),String> {
    if image.info.width>MAX_SAMPLES { return Err("Spectrum exceeds 1,000,000 samples".into()); }
    let h=&image.info.header;
    let ctype=card(h,"CTYPE1").unwrap_or_default();
    let known=card(h,"CRVAL1").is_some() && (card(h,"CDELT1").is_some() || card(h,"CD1_1").is_some());
    // Algorithm suffixes need a full spectral WCS implementation; use sample indices.
    // 带算法后缀的光谱坐标需要完整 WCS 支持，当前明确回退为采样序号。
    let linear=known && !ctype.contains('-') && !ctype.to_uppercase().contains("MULTISPEC");
    let reference=number(h,"CRVAL1",0.)?; let pixel=number(h,"CRPIX1",1.)?;
    let increment=if card(h,"CD1_1").is_some() { number(h,"CD1_1",1.)? } else { number(h,"CDELT1",1.)?*number(h,"PC1_1",1.)? };
    let log=linear && number(h,"DC-FLAG",0.)?==1.;
    let x=(0..image.info.width).map(|i| if linear { let v=reference+(i as f64+1.-pixel)*increment; if log {10_f64.powf(v)} else {v} } else {i as f64+1.}).collect();
    image.info.spectrum=Some(SpectrumMeta {x_label:if linear {if ctype.is_empty(){"Coordinate".into()}else{ctype}}else{"Sample".into()},
        x_unit:if linear {card(h,"CUNIT1").unwrap_or_default()}else{"pixel".into()},y_label:"Flux".into(),y_unit:card(h,"BUNIT").filter(|value|!value.trim().is_empty()).or_else(||card(h,"FLXUNITS")).unwrap_or_default(),
        columns:vec![],x_column:0,y_column:1,error_column:None,error_hdu:None,uncertainty_kind:String::new(),
        note:if !linear {"No supported wavelength solution; showing sample indices"} else if log {"Legacy DC-FLAG=1 log10 dispersion"} else {"Linear spectral coordinates; original flux scaling preserved"}.into()});
    image.spectrum=Some(SpectrumData{x,error:None});
    Ok(())
}

pub(super) fn read_table<I:Iterator<Item=DataValue>>(data:I, info:ImageInfo, options:LoadOptions) -> Result<FitsImage,String> {
    let fields=number(&info.header,"TFIELDS",0.)? as usize;
    if fields<2 || fields>256 || number(&info.header,"NAXIS1",0.)?>16.*1024.*1024. { return Err("Spectral table exceeds supported row/column limits".into()); }
    let names=(1..=fields).map(|i|card(&info.header,&format!("TTYPE{i}")).unwrap_or_else(||format!("Column {i}"))).collect::<Vec<_>>();
    let units=(1..=fields).map(|i|card(&info.header,&format!("TUNIT{i}")).unwrap_or_default()).collect::<Vec<_>>();
    let mut columns=vec![Vec::new();fields];
    let mut scales=Vec::new(); let mut offsets=Vec::new(); let mut nulls=Vec::new();
    for i in 1..=fields {
        let form=card(&info.header,&format!("TFORM{i}")).unwrap_or_default();
        if form.contains('P') || form.contains('Q') { return Err("Variable-length spectrum columns are not yet supported".into()); }
        if number(&info.header,"NAXIS2",0.)?>1. && form.chars().take_while(char::is_ascii_digit).collect::<String>().parse::<usize>().unwrap_or(1)>1 {
            return Err("Multiple vector rows require explicit order selection (not yet supported)".into());
        }
        scales.push(number(&info.header,&format!("TSCAL{i}"),1.)?);
        offsets.push(number(&info.header,&format!("TZERO{i}"),0.)?);
        nulls.push(card(&info.header,&format!("TNULL{i}")).and_then(|s|s.parse::<i64>().ok()));
    }
    let mut total=0_usize;
    for value in data {
        let numeric=match value {
            DataValue::UnsignedByte{value,column,..}=>Some((f64::from(value),Some(i64::from(value)),column)),
            DataValue::Short{value,column,..}=>Some((f64::from(value),Some(i64::from(value)),column)),
            DataValue::Integer{value,column,..}=>Some((f64::from(value),Some(i64::from(value)),column)),
            DataValue::Long{value,column,..}=>Some((value as f64,Some(value),column)),
            DataValue::Float{value,column,..}=>Some((f64::from(value),None,column)),
            DataValue::Double{value,column,..}=>Some((value,None,column)),_=>None };
        if let Some((value,integer,column))=numeric {
            let c=match column {ColumnId::Index(c)=>c,ColumnId::Name(n)=>names.iter().position(|s|s==n).unwrap_or(fields)};
            if c<fields {
                columns[c].push(if integer.is_some() && integer==nulls[c] {f64::NAN} else {value.mul_add(scales[c],offsets[c])});
                total+=1;
                if columns[c].len()>MAX_SAMPLES || total>MAX_SAMPLES*16 { return Err("Spectral table exceeds numeric sample limit".into()); }
            }
        }
    }
    if let Some(mask)=names.iter().position(|s|["MASK","DQ","QUALITY","AND_MASK"].contains(&normalized(s).as_str())) {
        if let Some(y)=options.y_column.or_else(||flux(&names)).filter(|&y|y<columns.len()) {
            if columns[mask].len()==columns[y].len() { for i in 0..columns[y].len() { if columns[mask][i]!=0. {columns[y][i]=f64::NAN;} } }
        }
    }
    from_columns(info,names,units,columns,options)
}

pub(super) fn read_text(bytes:&[u8], path:&Path, options:LoadOptions) -> Result<FitsImage,String> {
    let text=std::str::from_utf8(bytes).map_err(|_|"Text spectra must be UTF-8")?.trim_start_matches('\u{feff}');
    let split=|line:&str| {
        if let Some(separator)=if line.contains(','){Some(',')}else if line.contains(';'){Some(';')}else{None} {
            line.split(separator).map(|s|s.trim().to_string()).collect::<Vec<_>>()
        } else {line.split_whitespace().map(str::to_string).collect::<Vec<_>>()}
    };
    let mut names=Vec::new(); let mut columns:Vec<Vec<f64>>=Vec::new();
    for (line_index,line) in text.lines().enumerate() {
        let line=line.trim();
        if line.is_empty(){continue;}
        if line.starts_with('#') || line.starts_with('!') {
            if columns.is_empty(){let words=split(line.trim_start_matches(['#','!']).trim());if words.len()>=2 {names=words;}}
            continue;
        }
        let words=split(line.split('#').next().unwrap_or_default());
        let numbers=words.iter().map(|s|if s.is_empty(){Ok(f64::NAN)}else{s.replace(['D','d'],"E").parse::<f64>()}).collect::<Result<Vec<_>,_>>();
        let numbers=match numbers {Ok(v)=>v,Err(_) if columns.is_empty()=>{names=words;continue;},Err(_)=>return Err(format!("Invalid numeric spectrum row {}",line_index+1))};
        if numbers.len()<2 || numbers.len()>128 {return Err("Text spectrum needs 2 to 128 numeric columns".into());}
        if columns.is_empty(){columns=vec![Vec::new();numbers.len()];}
        if numbers.len()!=columns.len(){return Err(format!("Inconsistent column count at row {}",line_index+1));}
        for (column,value) in columns.iter_mut().zip(numbers){column.push(value);if column.len()>MAX_SAMPLES{return Err("Text spectrum exceeds 1,000,000 samples".into());}}
    }
    if columns.len()<2 {return Err("No two-column numerical spectrum found".into());}
    if names.len()!=columns.len(){names=(1..=columns.len()).map(|i|format!("Column {i}")).collect();}
    let info=ImageInfo {path:path.to_string_lossy().into_owned(),hdu:0,width:1,height:1,bitpix:-64,bscale:1.,bzero:0.,blank:None,
        hdus:vec![HduInfo {index:0,name:"Text spectrum".into(),axes:vec![],bitpix:-64,viewable:true,kind:"Spectrum".into()}],header:"Text spectrum; coordinate/flux units are unspecified unless supplied separately.".into(),spectrum:None};
    let units=vec![String::new();names.len()];
    from_columns(info,names,units,columns,options)
}
