//! Bounded tile framing around fitsrs' Rice codec; no additional runtime library.
//! 使用 fitsrs 的 Rice 解码器，修正位宽、分块边缘及浮点回退的兼容问题。
use super::{card, number};
use crate::{FitsImage, ImageInfo, RawPixels, MAX_FILE_BYTES, checked_pixels};
use fitsrs::hdu::data::bintable::tile_compressed::{rice::RICEDecoder, dithering::RAND_VALUES};
use std::io::{Cursor, Read, Seek, SeekFrom};

struct Column { name:String, offset:u64, kind:char, width:usize }
fn unsigned(bytes:&[u8])->Result<u64,String> {
    let value=match bytes.len(){4=>i32::from_be_bytes(bytes.try_into().unwrap()) as i64,8=>i64::from_be_bytes(bytes.try_into().unwrap()),_=>return Err("Invalid descriptor width".into())};
    u64::try_from(value).map_err(|_|"Negative compressed-data descriptor".into())
}
fn field<R:Read+Seek>(r:&mut R,c:&Column,row:u64,start:u64,row_size:u64)->Result<Vec<u8>,String>{
    r.seek(SeekFrom::Start(start+row*row_size+c.offset)).map_err(|e|e.to_string())?;
    let mut b=vec![0;c.width];r.read_exact(&mut b).map_err(|e|e.to_string())?;Ok(b)
}
fn scalar<R:Read+Seek>(r:&mut R,columns:&[Column],name:&str,row:u64,start:u64,row_size:u64,default:f64)->Result<f64,String>{
    let Some(c)=columns.iter().find(|c|c.name==name) else{return Ok(default);};
    let b=field(r,c,row,start,row_size)?;
    match (c.kind,b.len()) {('D',8)=>Ok(f64::from_be_bytes(b.try_into().unwrap())),('E',4)=>Ok(f32::from_be_bytes(b.try_into().unwrap()) as f64),('J',4)=>Ok(i32::from_be_bytes(b.try_into().unwrap()) as f64),_=>Err(format!("Unsupported {name} column type"))}
}
fn descriptor<R:Read+Seek>(r:&mut R,c:&Column,row:u64,start:u64,row_size:u64)->Result<(u64,u64),String>{
    let b=field(r,c,row,start,row_size)?;
    match c.kind {'P'=>Ok((unsigned(&b[..4])?,unsigned(&b[4..8])?)),'Q'=>Ok((unsigned(&b[..8])?,unsigned(&b[8..16])?)),_=>Err("Compressed tiles require P/Q byte-array descriptors".into())}
}

pub(super) fn read<R:Read+Seek>(r:&mut R,info:ImageInfo,start:u64,data_size:u64)->Result<FitsImage,String>{
    read_inner(r,info,start,data_size,None)
}
pub(super) fn read_sampled<R:Read+Seek>(r:&mut R,info:ImageInfo,start:u64,data_size:u64,level:u8,x:usize,y:usize)->Result<FitsImage,String>{
    let grid=crate::remote::tile_bounds(&info,level,x,y)?;
    read_inner(r,info,start,data_size,Some(grid))
}
fn read_inner<R:Read+Seek>(r:&mut R,mut info:ImageInfo,start:u64,data_size:u64,grid:Option<(usize,usize,usize,usize,usize)>)->Result<FitsImage,String>{
    let h=&info.header;
    let algorithm=card(h,"ZCMPTYPE").unwrap_or_default();
    if !["RICE_1","RICE_ONE","GZIP_1","GZIP_2"].contains(&algorithm.as_str()){return Err(format!("Unsupported tile compression {algorithm}; supported: RICE_1, GZIP_1, GZIP_2"));}
    if ![8,16,32,-32,-64].contains(&info.bitpix){return Err(format!("Unsupported compressed BITPIX {}",info.bitpix));}
    if card(h,"ZMASKCMP").is_some(){return Err("Compressed NULL_PIXEL_MASK is not supported".into());}
    let count=if let Some((_,_,width,height,_))=grid{checked_pixels(width,height)?}else{checked_pixels(info.width,info.height)?};
    let tw=number(h,"ZTILE1",info.width as f64)? as usize;
    let th=number(h,"ZTILE2",1.)? as usize;
    if tw==0 || th==0 || tw>info.width || th>info.height{return Err("Invalid tile dimensions".into());}
    let tiles_x=info.width.div_ceil(tw);let tiles_y=info.height.div_ceil(th);
    let rows=number(h,"NAXIS2",0.)? as u64;let row_size=number(h,"NAXIS1",0.)? as u64;
    let fields=number(h,"TFIELDS",0.)? as usize;
    if fields==0 || fields>256 || rows!=tiles_x.checked_mul(tiles_y).ok_or("Tile count overflow")? as u64 || row_size>1024*1024{return Err("Invalid compressed table dimensions".into());}
    let heap=number(h,"THEAP",(rows*row_size) as f64)? as u64;
    if heap<rows*row_size || heap>data_size{return Err("Invalid compressed heap offset".into());}
    let mut offset=0_u64;let mut columns=Vec::new();
    for i in 1..=fields{
        let form=card(h,&format!("TFORM{i}")).ok_or("Missing compressed table format")?;
        let digits=form.chars().take_while(char::is_ascii_digit).collect::<String>();
        let repeat=if digits.is_empty(){1}else{digits.parse::<usize>().map_err(|_|"Invalid repeat count")?};
        let kind=form.chars().nth(digits.len()).ok_or("Invalid table format")?;
        let unit=match kind{'P'=>8,'Q'=>16,'D'|'K'=>8,'J'|'E'=>4,'I'=>2,'B'|'A'|'L'=>1,_=>return Err("Unsupported compressed table column format".into())};
        let width=repeat.checked_mul(unit).ok_or("Column size overflow")?;
        if width>1024*1024 || offset+width as u64>row_size{return Err("Compressed column exceeds row".into());}
        if (kind=='P' && width!=8) || (kind=='Q' && width!=16) {return Err("Compressed tiles require a single P/Q descriptor".into());}
        columns.push(Column{name:card(h,&format!("TTYPE{i}")).unwrap_or_default(),offset,kind,width});offset+=width as u64;
    }
    let compressed=columns.iter().find(|c|c.name=="COMPRESSED_DATA").ok_or("Missing COMPRESSED_DATA")?;
    let mut block_size=32;let mut bytepix=if info.bitpix<0{4}else{info.bitpix as usize/8};
    for i in 1..=32{match card(h,&format!("ZNAME{i}")).as_deref(){Some("BLOCKSIZE")=>block_size=number(h,&format!("ZVAL{i}"),32.)? as usize,Some("BYTEPIX")=>bytepix=number(h,&format!("ZVAL{i}"),bytepix as f64)? as usize,_=>{}}}
    if block_size==0 || block_size>1024 || ![1,2,4].contains(&bytepix){return Err("Unsupported Rice coding parameters".into());}
    if algorithm.starts_with("RICE") && info.bitpix>0 && bytepix!=info.bitpix as usize/8 {return Err("Rice BYTEPIX does not match the image storage type".into());}
    let selected=grid.map(|(x,y,width,height,step)|{
        let mut tiles=std::collections::BTreeMap::<u64,Vec<(usize,usize)>>::new();
        for oy in 0..height{for ox in 0..width{
            let sx=(x+ox*step+step/2).min(info.width-1);let sy=(y+oy*step+step/2).min(info.height-1);
            let row=(sy/th*tiles_x+sx/tw) as u64;
            let actual_width=tw.min(info.width-sx/tw*tw);
            tiles.entry(row).or_default().push(((sy%th)*actual_width+sx%tw,oy*width+ox));
        }}for samples in tiles.values_mut(){samples.sort_unstable_by_key(|&(source,_)|source);}tiles
    });
    let mut output=match info.bitpix{8=>RawPixels::U8(vec![0;count]),16=>RawPixels::I16(vec![0;count]),32=>RawPixels::I32(vec![0;count]),-32=>RawPixels::F32(vec![f32::NAN;count]),-64=>RawPixels::F64(vec![f64::NAN;count]),_=>unreachable!()};
    let row_indices:Box<dyn Iterator<Item=u64>+'_>=if let Some(selected)=&selected{Box::new(selected.keys().copied())}else{Box::new(0..rows)};
    for row in row_indices{
        let samples=selected.as_ref().and_then(|tiles|tiles.get(&row));
        if selected.is_some() && samples.is_none(){continue;}
        let tile_x=row as usize%tiles_x*tw;let tile_y=row as usize/tiles_x*th;
        let width=tw.min(info.width-tile_x);let height=th.min(info.height-tile_y);let n=width*height;
        let (mut length,mut position)=descriptor(r,compressed,row,start,row_size)?;
        let mut codec=algorithm.as_str();let mut fallback=false;
        if length==0 {
            if let Some(c)=columns.iter().find(|c|c.name=="GZIP_COMPRESSED_DATA"){
                (length,position)=descriptor(r,c,row,start,row_size)?;codec="GZIP_1";fallback=true;
            }
        }
        if length==0 || length>MAX_FILE_BYTES || heap.checked_add(position).and_then(|v|v.checked_add(length)).is_none_or(|end|end>data_size){return Err("Compressed tile points outside the FITS heap".into());}
        if grid.is_some() && length>16*1024*1024{return Err("One compressed FITS tile exceeds the 16 MiB display budget".into());}
        r.seek(SeekFrom::Start(start+heap+position)).map_err(|e|e.to_string())?;
        let mut input=vec![0;length as usize];r.read_exact(&mut input).map_err(|e|e.to_string())?;
        let float=info.bitpix<0;
        let quantized=float && !fallback && columns.iter().any(|c|c.name=="ZSCALE");
        if codec.starts_with("RICE") && float && (!quantized || bytepix!=4) {
            return Err("Rice floating images require 32-bit quantized tiles".into());
        }
        let size=if codec.starts_with("RICE"){bytepix}else if quantized{4}else{info.bitpix.unsigned_abs() as usize/8};
        let expected=n.checked_mul(size).ok_or("Decoded tile overflow")?;
        if expected as u64>MAX_FILE_BYTES{return Err("Decoded tile exceeds limit".into());}
        if grid.is_some() && expected>16*1024*1024{return Err("One decoded FITS tile exceeds the 16 MiB display budget".into());}
        let mut decoded=vec![0;expected];let little=codec.starts_with("RICE");
        if little {
            let cursor=Cursor::new(&input);
            let result=match bytepix{1=>RICEDecoder::<_,u8>::new(cursor,block_size as i32,n as i32).read_exact(&mut decoded),2=>RICEDecoder::<_,i16>::new(cursor,block_size as i32,n as i32).read_exact(&mut decoded),4=>RICEDecoder::<_,i32>::new(cursor,block_size as i32,n as i32).read_exact(&mut decoded),_=>unreachable!()};
            result.map_err(|e|format!("Rice tile {row}: {e}"))?;
        } else {
            let mut bytes=Vec::new();flate2::read::GzDecoder::new(Cursor::new(input)).take(expected as u64+1).read_to_end(&mut bytes).map_err(|e|format!("Gzip tile {row}: {e}"))?;
            if bytes.len()!=expected{return Err(format!("Decoded tile {row} length mismatch"));}
            if codec=="GZIP_2" {for i in 0..n{for b in 0..size{decoded[i*size+b]=bytes[b*n+i];}}}else{decoded=bytes;}
        }
        let scale=scalar(r,&columns,"ZSCALE",row,start,row_size,1.)?;
        let zero=scalar(r,&columns,"ZZERO",row,start,row_size,0.)?;
        let blank=scalar(r,&columns,"ZBLANK",row,start,row_size,number(h,"ZBLANK",i32::MIN as f64)?)? as i32;
        let quantization=card(h,"ZQUANTIZ").unwrap_or_else(||"NO_DITHER".into());
        if quantized && !["NO_DITHER","SUBTRACTIVE_DITHER_1","SUBTRACTIVE_DITHER_2"].contains(&quantization.as_str()) {
            return Err(format!("Unsupported floating quantization {quantization}"));
        }
        let dither=quantization.starts_with("SUBTRACTIVE_DITHER");
        let mut seed=((row as i64+number(h,"ZDITHER0",1.)? as i64-1).rem_euclid(10000)) as usize;
        let mut random=(RAND_VALUES[seed]*500.) as usize;
        let mut sample_index=0;
        for i in 0..n{
            let b=&decoded[i*size..(i+1)*size];
            let integer=match size{1=>b[0] as i64,2=>(if little{i16::from_le_bytes(b.try_into().unwrap())}else{i16::from_be_bytes(b.try_into().unwrap())}) as i64,4=>(if little{i32::from_le_bytes(b.try_into().unwrap())}else{i32::from_be_bytes(b.try_into().unwrap())}) as i64,_=>0};
            let value=if quantized{
                let rand=if dither{let v=RAND_VALUES[random] as f64;random+=1;if random==10000{seed=(seed+1)%10000;random=(RAND_VALUES[seed]*500.) as usize;}v}else{0.5};
                if integer as i32==blank{f64::NAN}else if quantization=="SUBTRACTIVE_DITHER_2" && integer==-2147483646{0.}else{(integer as f64-rand+0.5)*scale+zero}
            }else if float {match info.bitpix{-32=>f32::from_be_bytes(b.try_into().unwrap()) as f64,-64=>f64::from_be_bytes(b.try_into().unwrap()),_=>unreachable!()}}else{integer as f64};
            let target=if let Some(samples)=samples{if samples.get(sample_index).is_none_or(|&(source,_)|source!=i){continue;}let target=samples[sample_index].1;sample_index+=1;target}else{(tile_y+i/width)*info.width+tile_x+i%width};
            match &mut output{RawPixels::U8(v)=>v[target]=integer as u8,RawPixels::I16(v)=>v[target]=integer as i16,RawPixels::I32(v)=>v[target]=integer as i32,RawPixels::F32(v)=>v[target]=value as f32,RawPixels::F64(v)=>v[target]=value,_=>unreachable!()}
        }
    }
    if let Some((_,_,width,height,_))=grid{info.width=width;info.height=height;info.header.clear();info.hdus.clear();info.path.clear();}
    Ok(FitsImage{info,pixels:output,spectrum:None})
}
