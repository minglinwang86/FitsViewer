//! Progressive numeric tiles reuse the ordinary display mapping and original-pixel geometry.
//! 渐进数值图块复用本地拉伸，拖动不等待网络，精确数值单独从原始像素读取。
use crate::image_transform::ImageTransform;
use eframe::egui::{self,Color32,Rect};
use fitsviewer_core::{remote::{Manifest,TILE_SIZE},FitsImage,LoadOptions,DisplayTone,display::{RenderOptions,ScaleMapping,MaskStyle}};
use std::{collections::{HashMap,HashSet,VecDeque},sync::{Arc,Mutex,atomic::{AtomicBool,AtomicUsize,Ordering}}};

const CACHE_BUDGET:usize=40*1024*1024;
static IN_FLIGHT:AtomicUsize=AtomicUsize::new(0);

#[derive(Clone,Copy,Debug,Hash,PartialEq,Eq)]
pub(super) struct Key {pub level:u8,pub x:usize,pub y:usize}
struct Tile {image:Arc<FitsImage>,texture:Option<egui::TextureHandle>,revision:u64,used:u64,bytes:usize}
enum Reply {Tile(Key,Result<FitsImage,String>),Pixel([usize;2],Result<serde_json::Value,String>)}
pub(super) struct RemoteImage {
    pub manifest:Manifest,
    api:String,token:String,query:String,group:String,request_epoch:u64,
    local:Option<crate::ImageBridge>,compact:bool,
    tiles:HashMap<Key,Tile>,pending:HashSet<Key>,replies:Arc<Mutex<VecDeque<Reply>>>,active:Arc<AtomicBool>,
    frame:u64,revision:u64,desired:Vec<Key>,pixel_pending:bool,last_pixel_time:f64,
    now:f64,retry:HashMap<Key,(f64,u8)>,
    pixel:Option<([usize;2],serde_json::Value)>,pub error:String,network_bytes:usize,
}

pub(super) fn query(path:&str,options:LoadOptions)->String{
    let mut value=format!("path={}&ekind={}",urlencoding::encode(path),options.error_kind.as_str());
    for (name,index) in [("hdu",options.hdu),("xcol",options.x_column),("ycol",options.y_column),("ecol",options.error_column),("ehdu",options.error_hdu)]{if let Some(index)=index{value.push_str(&format!("&{name}={index}"));}}
    if options.ignore_errors{value.push_str("&no_error=true");}value
}
pub(super) fn cancel_group(group:&str){
    #[cfg(target_arch="wasm32")]let _=js_sys::eval(&format!("window.FitsViewer?.keepImageRequests?.({},[])",serde_json::to_string(group).unwrap_or_default()));
    #[cfg(not(target_arch="wasm32"))]let _=group;
}
pub(super) fn get(url:String,token:&str,max_bytes:usize,local:Option<crate::ImageBridge>,done:impl FnOnce(Result<Vec<u8>,String>)+Send+'static)->bool{
    if IN_FLIGHT.fetch_update(Ordering::AcqRel,Ordering::Acquire,|n|(n<12).then_some(n+1)).is_err(){return false;}
    #[cfg(not(target_arch="wasm32"))]{
        let token=token.to_owned();
        let result=std::thread::Builder::new().name("fitsviewer-image".into()).spawn(move||{
            let response=(||{
                if let Some(local)=local{let bytes=local(url)?;if bytes.len()>max_bytes{return Err("Local image response exceeds its negotiated size".into());}return Ok(bytes);}
                use std::io::Read;
                let seconds=if url.contains("/tile?") || url.contains("/pixel?"){20}else{60};
                let mut request=ureq::get(&url).timeout(std::time::Duration::from_secs(seconds));
                if !token.is_empty(){request=request.set("X-Fitsviewer-Token",&token);}
                let response=match request.call(){Ok(response)=>response,Err(ureq::Error::Status(_,response))=>{let mut message=String::new();let _=response.into_reader().take(4096).read_to_string(&mut message);return Err(message);},Err(error)=>return Err(error.to_string())};
                if response.header("Content-Length").and_then(|s|s.parse::<usize>().ok()).is_some_and(|n|n>max_bytes){return Err("Remote response exceeds its negotiated size".into());}
                let mut bytes=Vec::new();response.into_reader().take(max_bytes as u64+1).read_to_end(&mut bytes).map_err(|e|e.to_string())?;
                if bytes.len()>max_bytes{return Err("Remote response exceeds its negotiated size".into());}Ok(bytes)
            })();
            IN_FLIGHT.fetch_sub(1,Ordering::AcqRel);done(response);
        });
        if result.is_err(){IN_FLIGHT.fetch_sub(1,Ordering::AcqRel);return false;}return true;
    }
    #[cfg(target_arch="wasm32")]{
    let _=local;
    let mut request=ehttp::Request::get(url);if !token.is_empty(){request.headers.insert("X-Fitsviewer-Token",token);}
    ehttp::fetch(request,move|response|{
        IN_FLIGHT.fetch_sub(1,Ordering::AcqRel);
        done(response.map_err(|e|e.to_string()).and_then(|response|{
            if !response.ok{return Err(String::from_utf8_lossy(&response.bytes[..response.bytes.len().min(4096)]).into_owned());}
            if response.bytes.len()>max_bytes{return Err("Remote response exceeds its negotiated size".into());}
            Ok(response.bytes)
        }));
    });true}
}

impl RemoteImage {
    pub fn new(manifest:Manifest,api:String,token:String,query:String,group:String,local:Option<crate::ImageBridge>,compact:bool)->Self{
        Self{manifest,api,token,query,group,request_epoch:0,local,compact,tiles:HashMap::new(),pending:HashSet::new(),replies:Arc::new(Mutex::new(VecDeque::new())),active:Arc::new(AtomicBool::new(true)),frame:0,revision:1,desired:vec![],pixel_pending:false,last_pixel_time:-100.,now:0.,retry:HashMap::new(),pixel:None,error:String::new(),network_bytes:0}
    }
    fn request_group(&self)->String{format!("{}-display-{}",self.group,self.request_epoch)}
    fn url(&self,operation:&str)->String{format!("{}/api/image/{operation}?{}&version={}&fv_image={}",self.api,self.query,urlencoding::encode(&self.manifest.source_version),urlencoding::encode(&self.request_group()))}
    pub fn poll(&mut self)->bool{
        let replies:Vec<_>=self.replies.lock().unwrap().drain(..).collect();let mut preview_changed=false;
        for reply in replies{match reply{
            Reply::Tile(key,result)=>{self.pending.remove(&key);match result{
                Ok(image)=>{
                    self.retry.remove(&key);
                    if self.retry.is_empty(){self.error.clear();}
                    if image.info.hdu!=self.manifest.info.hdu || image.info.width>TILE_SIZE || image.info.height>TILE_SIZE{self.error="Remote tile identity or dimensions are invalid".into();continue;}
                    let bytes=image.pixels.len()*((image.info.bitpix.unsigned_abs()/8) as usize+12);
                    self.network_bytes=self.network_bytes.saturating_add(image.pixels.len()*(image.info.bitpix.unsigned_abs()/8) as usize);
                    preview_changed|=key.level==self.manifest.preview_level && key.x==0 && key.y==0;
                    self.tiles.insert(key,Tile{image:Arc::new(image),texture:None,revision:0,used:self.frame,bytes});
                    for neighbor in [Key{level:key.level,x:key.x.saturating_sub(1),y:key.y},Key{level:key.level,x:key.x+1,y:key.y},Key{level:key.level,x:key.x,y:key.y.saturating_sub(1)},Key{level:key.level,x:key.x,y:key.y+1}]{if let Some(tile)=self.tiles.get_mut(&neighbor){tile.revision=0;}}
                },Err(error)=>{if !error.contains("AbortError") && !error.contains("aborted"){let tries=self.retry.get(&key).map_or(1,|(_,tries)|tries.saturating_add(1).min(6));self.retry.insert(key,(self.now+0.5*2f64.powi(i32::from(tries)),tries));self.error=error;}},
            }},
            Reply::Pixel(position,result)=>{self.pixel_pending=false;match result{Ok(value) if value["source_version"]==self.manifest.source_version=>self.pixel=Some((position,value)),Ok(_)=>{},Err(error)=>{if error.contains("changed") || error.contains("expired"){self.error=error;}}}},
        }}preview_changed
    }
    pub fn preview(&self)->Option<&FitsImage>{self.tiles.get(&Key{level:self.manifest.preview_level,x:0,y:0}).map(|tile|tile.image.as_ref())}
    pub fn preview_shared(&self)->Option<Arc<FitsImage>>{if self.compact{return None;}self.tiles.get(&Key{level:self.manifest.preview_level,x:0,y:0}).map(|tile|tile.image.clone())}
    pub fn dirty(&mut self){self.revision=self.revision.wrapping_add(1);}
    pub fn set_compact(&mut self,compact:bool){
        if self.compact==compact{return;}
        // Closed browser request groups stay closed. A new precision starts a new
        // group, so late old requests remain cancelled while new tiles can load.
        // 浏览器取消组不可复用；精度切换创建新组并重新声明当前视野。
        self.active.store(false,Ordering::Release);cancel_group(&self.request_group());
        self.request_epoch=self.request_epoch.wrapping_add(1);
        self.active=Arc::new(AtomicBool::new(true));self.replies=Arc::new(Mutex::new(VecDeque::new()));
        self.tiles.clear();self.pending.clear();self.retry.clear();self.desired.clear();self.error.clear();self.pixel_pending=false;self.compact=compact;
    }
    fn rgba(&self,key:Key,mapping:&ScaleMapping,invert:bool,tone:DisplayTone,options:&RenderOptions)->Vec<u8>{
        let image=&self.tiles[&key].image;
        let mut rgba=fitsviewer_core::display::render_with_mapping(image,invert,tone,options,mapping);
        let Some(mask)=options.mask.as_ref().filter(|mask|mask.style==MaskStyle::Outline) else{return rgba;};
        // A tile boundary is not an image boundary. Unknown neighbor samples do
        // not create a false contour; arrival invalidates both touching textures.
        // 图块边界不是图像边界；邻块未到时不绘制假轮廓，收到后刷新相邻纹理。
        let sample=|x:isize,y:isize|->Option<f64>{
            let grid_x=key.x as isize*TILE_SIZE as isize+x;let grid_y=key.y as isize*TILE_SIZE as isize+y;
            let step=1usize<<key.level;
            if grid_x<0 || grid_y<0 || grid_x as usize>=self.manifest.info.width.div_ceil(step) || grid_y as usize>=self.manifest.info.height.div_ceil(step){return None;}
            let neighbor=Key{level:key.level,x:grid_x as usize/TILE_SIZE,y:grid_y as usize/TILE_SIZE};
            let Some(tile)=self.tiles.get(&neighbor) else{return Some(mask.threshold);};
            let px=grid_x as usize%TILE_SIZE;let py=grid_y as usize%TILE_SIZE;
            if px>=tile.image.info.width || py>=tile.image.info.height{return None;}tile.image.physical(py*tile.image.info.width+px)
        };
        for y in 0..image.info.height{for x in 0..image.info.width{
            if x>0 && x+1<image.info.width && y>0 && y+1<image.info.height{continue;}
            let Some(value)=image.physical(y*image.info.width+x).filter(|value|*value>=mask.threshold) else{continue;};
            let boundary=[(x as isize-1,y as isize),(x as isize+1,y as isize),(x as isize,y as isize-1),(x as isize,y as isize+1)].iter().any(|&(x,y)|sample(x,y).is_none_or(|value|value<mask.threshold));
            let mut color=mapping.color(value,invert,tone,&options.palette);
            if boundary{for channel in 0..3{color[channel]=(f32::from(color[channel])*(1.-mask.opacity)+f32::from(mask.color[channel])*mask.opacity).round() as u8;}}
            let index=((image.info.height-1-y)*image.info.width+x)*4;rgba[index..index+4].copy_from_slice(&color);
        }}rgba
    }
    fn request(&mut self,ctx:&egui::Context,key:Key){
        if self.tiles.contains_key(&key) || self.pending.contains(&key) || self.pending.len()>=4{return;}
        if self.retry.get(&key).is_some_and(|(time,_)|*time>self.now){return;}
        let url=format!("{}&level={}&x={}&y={}&display_bits={}",self.url("tile"),key.level,key.x,key.y,if self.compact{16}else{0});
        let replies=self.replies.clone();let active=self.active.clone();let context=ctx.clone();
        if get(url,&self.token,2*1024*1024,self.local.clone(),move|bytes|{
            if !active.load(Ordering::Acquire){return;}
            let result=bytes.and_then(|bytes|FitsImage::from_packet(&bytes));
            let mut queue=replies.lock().unwrap();if queue.len()<8{queue.push_back(Reply::Tile(key,result));}context.request_repaint();
        }){self.pending.insert(key);}
    }
    fn choose(&self,transform:ImageTransform,viewport:Rect)->Vec<Key>{
        let info=&self.manifest.info;let roi=transform.visible_roi(viewport);
        let mut level=if transform.zoom<1.{(1./transform.zoom.max(0.00001)).log2().floor().max(0.) as u8}else{0};level=level.min(self.manifest.preview_level);
        loop{
            let span=(TILE_SIZE as f64)*(2f64.powi(i32::from(level)));
            let x0=(roi[0]/span).floor().max(0.) as usize;let y0=(roi[1]/span).floor().max(0.) as usize;
            let x1=(roi[2]/span).ceil().min((info.width as f64/span).ceil()) as usize;let y1=(roi[3]/span).ceil().min((info.height as f64/span).ceil()) as usize;
            if (x1.saturating_sub(x0)).saturating_mul(y1.saturating_sub(y0))>20 && level<self.manifest.preview_level{level+=1;continue;}
            let center=[(roi[0]+roi[2])/2./span,(roi[1]+roi[3])/2./span];let mut keys=vec![];
            for y in y0..y1{for x in x0..x1{keys.push(Key{level,x,y});}}
            keys.sort_by(|a,b|{let distance=|key:&Key|(key.x as f64+0.5-center[0]).powi(2)+(key.y as f64+0.5-center[1]).powi(2);distance(a).total_cmp(&distance(b))});
            return keys;
        }
    }
    pub fn paint(&mut self,ctx:&egui::Context,painter:&egui::Painter,transform:ImageTransform,viewport:Rect,mapping:&ScaleMapping,invert:bool,tone:DisplayTone,options:&RenderOptions,color:Color32){
        self.frame=self.frame.wrapping_add(1);
        self.now=ctx.input(|input|input.time);
        let preview=Key{level:self.manifest.preview_level,x:0,y:0};
        let desired=self.choose(transform,viewport);
        if desired!=self.desired{
            #[cfg(target_arch="wasm32")]{let keep=desired.iter().chain(std::iter::once(&preview)).map(|k|format!("{}:{}:{}",k.level,k.x,k.y)).collect::<Vec<_>>();let _=js_sys::eval(&format!("window.FitsViewer?.keepImageRequests?.({},{})",serde_json::to_string(&self.request_group()).unwrap_or_default(),serde_json::to_string(&keep).unwrap_or_default()));}
            self.desired=desired;
        }
        self.request(ctx,preview);
        for key in self.desired.clone(){self.request(ctx,key);if self.pending.len()>=4{break;}}
        // A small neighboring ring is speculative and never precedes visible tiles.
        if self.desired.iter().all(|key|self.tiles.contains_key(key)) && self.pending.is_empty(){
            if let Some(key)=self.desired.first().copied(){let max_x=self.manifest.info.width.div_ceil(TILE_SIZE<<key.level);let max_y=self.manifest.info.height.div_ceil(TILE_SIZE<<key.level);
                for (x,y) in [(key.x.saturating_sub(1),key.y),(key.x+1,key.y),(key.x,key.y.saturating_sub(1)),(key.x,key.y+1)]{if x<max_x && y<max_y{self.request(ctx,Key{level:key.level,x,y});}}}
        }
        let mut order=self.tiles.keys().copied().collect::<Vec<_>>();order.sort_by(|a,b|b.level.cmp(&a.level));
        for key in order{
            if key!=preview && !self.desired.contains(&key){continue;}
            let Some(tile)=self.tiles.get(&key) else{continue;};
            let rebuild=tile.revision!=self.revision || tile.texture.is_none();
            if rebuild{
                let rgba=self.rgba(key,mapping,invert,tone,options);
                let tile=self.tiles.get_mut(&key).unwrap();
                let color=egui::ColorImage::from_rgba_unmultiplied([tile.image.info.width,tile.image.info.height],&rgba);
                if let Some(texture)=&mut tile.texture{texture.set(color,egui::TextureOptions::NEAREST);}else{tile.texture=Some(ctx.load_texture(format!("remote-{}-{}-{}",key.level,key.x,key.y),color,egui::TextureOptions::NEAREST));}
                tile.revision=self.revision;
            }
            let tile=self.tiles.get_mut(&key).unwrap();tile.used=self.frame;
            let span=(TILE_SIZE as f64)*2f64.powi(i32::from(key.level));
            let bounds=[key.x as f64*span,key.y as f64*span,((key.x+1) as f64*span).min(self.manifest.info.width as f64),((key.y+1) as f64*span).min(self.manifest.info.height as f64)];
            if let Some(texture)=&tile.texture{transform.paint_texture_rect(painter,texture.id(),bounds,color);}
        }
        while self.tiles.values().map(|tile|tile.bytes).sum::<usize>()>CACHE_BUDGET{
            let candidate=self.tiles.iter().filter(|(key,_)|**key!=preview && !self.desired.contains(key)).min_by_key(|(_,tile)|tile.used).map(|(key,_)|*key);
            if let Some(key)=candidate{self.tiles.remove(&key);}else{break;}
        }
        if !self.pending.is_empty(){ctx.request_repaint_after(std::time::Duration::from_millis(100));}
        else if !self.retry.is_empty(){ctx.request_repaint_after(std::time::Duration::from_secs(1));}
    }
    pub fn cursor(&mut self,ctx:&egui::Context,x:usize,y:usize)->String{
        let now=ctx.input(|input|input.time);
        if !self.pixel_pending && now-self.last_pixel_time>=0.12 && self.pixel.as_ref().is_none_or(|(position,_)|*position!=[x,y]){
            let url=format!("{}&x={x}&y={y}",self.url("pixel"));let replies=self.replies.clone();let active=self.active.clone();let context=ctx.clone();
            if get(url,&self.token,16384,self.local.clone(),move|response|{if active.load(Ordering::Acquire){let result=response.and_then(|bytes|serde_json::from_slice(&bytes).map_err(|e|e.to_string()));replies.lock().unwrap().push_back(Reply::Pixel([x,y],result));context.request_repaint();}}){self.pixel_pending=true;self.last_pixel_time=now;}
        }
        if let Some((position,value))=&self.pixel{if *position==[x,y]{return format!("x={}  y={} · value={} · raw={}",x+1,y+1,value["value"].as_f64().map(|n|format!("{n:.8}")).unwrap_or_else(||"invalid".into()),value["raw"].as_str().unwrap_or("unavailable"));}}
        format!("x={}  y={} · {}",x+1,y+1,crate::i18n::t("Original pixel pending…"))
    }
    pub fn state(&self)->serde_json::Value{serde_json::json!({"transport":"tiles","display_encoding":if self.compact{"quantized-int16"}else{"source"},"source_bitpix":self.manifest.info.bitpix,"source_version":self.manifest.source_version,"cached_tiles":self.tiles.len(),"cache_bytes":self.tiles.values().map(|tile|tile.bytes).sum::<usize>(),"cache_budget":CACHE_BUDGET,"in_flight":self.pending.len(),"numeric_bytes_received":self.network_bytes,"sampled_statistics":true,"error":self.error})}
}
impl Drop for RemoteImage{
    fn drop(&mut self){self.active.store(false,Ordering::Release);self.replies.lock().unwrap().clear();
        cancel_group(&self.request_group());
    }
}

#[cfg(test)]mod tests{
    use super::*;
    fn view()->RemoteImage{
        let info=fitsviewer_core::ImageInfo{path:"numeric-boundary.fits".into(),hdu:0,width:512,height:256,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:None};
        RemoteImage::new(Manifest{schema:1,info,source_version:"fixed".into(),full_packet_bytes:None,threshold_bytes:20_000_000,transport:"tiles".into(),compression:"none".into(),hdus_complete:true,statistics:Default::default(),statistics_sampled:true,preview_level:1},String::new(),String::new(),String::new(),"test".into(),None,true)
    }
    fn tile(view:&RemoteImage,value:f64)->Tile{let mut info=view.manifest.info.clone();info.width=256;info.height=256;Tile{image:Arc::new(FitsImage{info,pixels:fitsviewer_core::RawPixels::F64(vec![value;256*256]),spectrum:None}),texture:None,revision:0,used:0,bytes:256*256*20}}
    #[test]fn original_precision_switch_discards_compact_cache_and_old_replies(){
        let mut view=view();let preview=Key{level:view.manifest.preview_level,x:0,y:0};let compact=tile(&view,1.);view.tiles.insert(preview,compact);
        assert!(view.preview_shared().is_none());let active=view.active.clone();let replies=view.replies.clone();
        view.desired.push(preview);let old_group=view.request_group();let old_url=view.url("pixel");
        view.set_compact(false);assert!(!active.load(Ordering::Acquire));assert!(view.tiles.is_empty());
        assert!(view.desired.is_empty());assert_ne!(view.request_group(),old_group);assert_ne!(view.url("pixel"),old_url);
        replies.lock().unwrap().push_back(Reply::Tile(preview,Ok((*tile(&view,2.).image).clone())));
        assert!(!view.poll());assert!(view.tiles.is_empty());
        let original=tile(&view,3.);view.tiles.insert(preview,original);assert_eq!(view.preview_shared().unwrap().info.bitpix,-64);assert_eq!(view.state()["display_encoding"],"source");
        let pro_group=view.request_group();view.set_compact(false);assert_eq!(view.request_group(),pro_group);
        view.set_compact(true);assert_ne!(view.request_group(),old_group);assert_ne!(view.request_group(),pro_group);assert!(view.tiles.is_empty());
    }
    #[test]fn threshold_contours_do_not_outline_every_tile(){
        let mut view=view();let left=Key{level:0,x:0,y:0};let right=Key{level:0,x:1,y:0};view.tiles.insert(left,tile(&view,1.));view.tiles.insert(right,tile(&view,1.));
        let mut options=RenderOptions::default();options.mask=Some(fitsviewer_core::display::ThresholdMask{threshold:0.5,opacity:1.,color:[255,0,0],style:MaskStyle::Outline});
        let mapping=ScaleMapping::new(None,0.,2.,fitsviewer_core::Stretch::Linear,1000.);
        let rgba=view.rgba(left,&mapping,false,DisplayTone::default(),&options);let pixel=|x:usize,y:usize|((255-y)*256+x)*4;
        assert_eq!(&rgba[pixel(255,100)..pixel(255,100)+4],&[128,128,128,255]);
        assert_eq!(&rgba[pixel(0,100)..pixel(0,100)+4],&[255,0,0,255]);
        view.tiles.insert(right,tile(&view,0.));let rgba=view.rgba(left,&mapping,false,DisplayTone::default(),&options);
        assert_eq!(&rgba[pixel(255,100)..pixel(255,100)+4],&[255,0,0,255]);
    }
    #[test]fn visible_tile_requests_stay_bounded_under_rotation(){
        let mut view=view();view.manifest.info.width=100_000;view.manifest.info.height=100_000;view.manifest.preview_level=fitsviewer_core::remote::preview_level(100_000,100_000);
        let viewport=Rect::from_min_max(egui::Pos2::ZERO,egui::pos2(1600.,1000.));
        for zoom in [0.005,0.1,1.,128.]{let transform=ImageTransform::new(viewport.center(),egui::vec2(100_000.,100_000.),zoom,37.);let requests=view.choose(transform,viewport);assert!(!requests.is_empty());assert!(requests.len()<=20);}
    }
}
