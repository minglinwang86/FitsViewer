//! Bounded PRO display textures. Original FITS arrays remain shared and unchanged.
//! PRO 有界图层预览；共享原始数组，不重采样科研数据或修改测量。
use eframe::egui::{self,Color32,Pos2,Rect,TextureId};
use fitsviewer_core::{DisplayTone,display::{ScaleMapping,RenderOptions,MaskStyle}};
use crate::multiview::ImageLayer;

pub const MAX_PIXELS:usize=1024*1024;
pub const MAX_LAYERS:usize=8;
pub struct Transfer<'a>{pub mapping:&'a ScaleMapping,pub invert:bool,pub tone:DisplayTone,pub options:&'a RenderOptions,pub key:String}
struct Cached{index:usize,key:String,texture:egui::TextureHandle,pixels:usize}
#[derive(Default)]
pub struct Renderer{textures:Vec<Cached>,transfer_key:String,pub builds:u64}

pub fn preview_size(width:usize,height:usize)->[usize;2]{
    if width==0 || height==0{return [0,0];}
    let ratio=(MAX_PIXELS as f64/(width as f64*height as f64)).sqrt().min(1.);
    let mut size=[((width as f64*ratio).floor() as usize).max(1),((height as f64*ratio).floor() as usize).max(1)];
    // Extremely thin images can round the short dimension up and exceed the budget.
    if size[0].saturating_mul(size[1])>MAX_PIXELS{if size[0]>=size[1]{size[0]=(MAX_PIXELS/size[1]).max(1);}else{size[1]=(MAX_PIXELS/size[0]).max(1);}}
    size
}
fn rgba(layer:&ImageLayer,size:[usize;2],transfer:&Transfer<'_>)->Vec<u8>{
    let image=&layer.image;let mut rgba=Vec::with_capacity(size[0]*size[1]*4);
    for screen_y in 0..size[1]{
        let y=image.info.height-1-(((screen_y as f64+0.5)*image.info.height as f64/size[1] as f64).floor() as usize).min(image.info.height-1);
        for preview_x in 0..size[0]{
            let x=(((preview_x as f64+0.5)*image.info.width as f64/size[0] as f64).floor() as usize).min(image.info.width-1);
            let Some(value)=image.physical(y*image.info.width+x).map(|value|value*layer.scale+layer.offset).filter(|value|value.is_finite())else{rgba.extend_from_slice(&[0,0,0,0]);continue;};
            let mut color=transfer.mapping.color(value,transfer.invert,transfer.tone,&transfer.options.palette);
            if let Some(mask)=&transfer.options.mask{
                let boundary=mask.style==MaskStyle::Fill || [(x.wrapping_sub(1),y),(x+1,y),(x,y.wrapping_sub(1)),(x,y+1)].iter().any(|&(nx,ny)|nx>=image.info.width || ny>=image.info.height || image.physical(ny*image.info.width+nx).map(|value|value*layer.scale+layer.offset).is_none_or(|value|value<mask.threshold));
                if value>=mask.threshold && boundary{for channel in 0..3{color[channel]=(f32::from(color[channel])*(1.-mask.opacity)+f32::from(mask.color[channel])*mask.opacity).round() as u8;}}
            }rgba.extend_from_slice(&color);
        }
    }rgba
}
pub fn screen_point(rect:Rect,zoom:f32,transform:[[f64;3];2],point:[f64;2])->Option<Pos2>{
    let x=transform[0][0]*point[0]+transform[0][1]*point[1]+transform[0][2];
    let y=transform[1][0]*point[0]+transform[1][1]*point[1]+transform[1][2];
    let point=Pos2::new(rect.left()+(x+0.5) as f32*zoom,rect.bottom()-(y+0.5) as f32*zoom);
    point.is_finite().then_some(point)
}
fn draw_layer(painter:&egui::Painter,texture:TextureId,layer:&ImageLayer,rect:Rect,zoom:f32){
    if !layer.opacity.is_finite() || layer.opacity<=0.{return;}
    let width=layer.image.info.width as f64;let height=layer.image.info.height as f64;
    let coordinates=[[-0.5,height-0.5],[width-0.5,height-0.5],[width-0.5,-0.5],[-0.5,-0.5]];
    let points=coordinates.map(|point|screen_point(rect,zoom,layer.transform,point));
    let [Some(a),Some(b),Some(c),Some(d)]=points else{return;};
    let color=Color32::from_white_alpha((layer.opacity.clamp(0.,1.)*255.).round() as u8);
    let mut mesh=egui::Mesh::with_texture(texture);
    for (pos,uv) in [a,b,c,d].into_iter().zip([Pos2::new(0.,0.),Pos2::new(1.,0.),Pos2::new(1.,1.),Pos2::new(0.,1.)]){mesh.vertices.push(egui::epaint::Vertex{pos,uv,color});}
    mesh.indices=vec![0,1,2,0,2,3];painter.add(egui::Shape::mesh(mesh));
}
impl Renderer{
    pub fn clear(&mut self){self.textures.clear();self.transfer_key.clear();}
    pub fn summary(&self)->serde_json::Value{serde_json::json!({"textures":self.textures.len(),"display_pixels":self.textures.iter().map(|entry|entry.pixels).sum::<usize>(),"rgba_bytes":self.textures.iter().map(|entry|entry.pixels*4).sum::<usize>(),"builds":self.builds,"per_layer_pixel_budget":MAX_PIXELS})}
    pub fn paint(&mut self,ctx:&egui::Context,painter:&egui::Painter,base:TextureId,rect:Rect,zoom:f32,source_style:(bool,f32),source_order:i32,layers:&[ImageLayer],transfer:Transfer<'_>){
        if self.transfer_key!=transfer.key{self.textures.clear();self.transfer_key=transfer.key.clone();}
        let mut layers=layers.iter().filter(|layer|layer.image.spectrum.is_none() && layer.image.info.width>0 && layer.image.info.height>0 && layer.opacity.is_finite() && layer.opacity>0. && layer.scale.is_finite() && layer.offset.is_finite() && layer.transform.iter().flatten().all(|value|value.is_finite())).take(MAX_LAYERS).collect::<Vec<_>>();
        layers.sort_by_key(|layer|(layer.order,layer.index));
        self.textures.retain(|cached|layers.iter().any(|layer|layer.index==cached.index));
        for layer in &layers{
            let key=format!("{}:{}:{:p}:{}:{}",layer.image.info.path,layer.image.info.hdu,std::sync::Arc::as_ptr(&layer.image),layer.scale,layer.offset);
            let existing=self.textures.iter().position(|entry|entry.index==layer.index);
            if existing.is_some_and(|index|self.textures[index].key==key){continue;}
            let size=preview_size(layer.image.info.width,layer.image.info.height);let pixels=rgba(layer,size,&transfer);
            let color=egui::ColorImage::from_rgba_unmultiplied(size,&pixels);
            if let Some(index)=existing{let entry=&mut self.textures[index];entry.texture.set(color,egui::TextureOptions::LINEAR);entry.key=key;entry.pixels=size[0]*size[1];}
            else{self.textures.push(Cached{index:layer.index,key,texture:ctx.load_texture(format!("fits-layer-{}",layer.index),color,egui::TextureOptions::LINEAR),pixels:size[0]*size[1]});}self.builds=self.builds.saturating_add(1);
        }
        let paint_base=||{if source_style.0 && source_style.1.is_finite() && source_style.1>0.{painter.image(base,rect,Rect::from_min_max(Pos2::ZERO,Pos2::new(1.,1.)),Color32::from_white_alpha((source_style.1.clamp(0.,1.)*255.).round() as u8));}};
        let mut base_painted=false;
        for layer in layers{if !base_painted && layer.order>=source_order{paint_base();base_painted=true;}
            if let Some(entry)=self.textures.iter().find(|entry|entry.index==layer.index){draw_layer(painter,entry.texture.id(),layer,rect,zoom);}
        }if !base_painted{paint_base();}
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    use fitsviewer_core::FitsImage;
    #[test]fn preview_budget_is_bounded_without_changing_aspect_unnecessarily(){for size in [[4096,4096],[32000,32000],[1,32000000],[32000000,1],[1024,1024],[3,5]]{let preview=preview_size(size[0],size[1]);assert!(preview[0]>0 && preview[1]>0);assert!(preview[0]*preview[1]<=MAX_PIXELS);}assert_eq!(preview_size(3,5),[3,5]);}
    #[test]fn fits_zero_origin_affine_maps_pixel_edges_without_flip(){let rect=Rect::from_min_max(Pos2::new(10.,20.),Pos2::new(110.,220.));let identity=[[1.,0.,0.],[0.,1.,0.]];assert_eq!(screen_point(rect,2.,identity,[-0.5,-0.5]),Some(rect.left_bottom()));assert_eq!(screen_point(rect,2.,identity,[49.5,99.5]),Some(rect.right_top()));let translated=[[1.,0.,2.],[0.,1.,3.]];assert_eq!(screen_point(rect,2.,translated,[0.,0.]),Some(Pos2::new(15.,213.)));}
    #[test]fn scaling_uses_reference_transfer_and_keeps_source_pixels(){
        let image=std::sync::Arc::new(FitsImage{info:fitsviewer_core::ImageInfo{path:"numeric-image".into(),hdu:0,width:2,height:2,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:None},pixels:fitsviewer_core::RawPixels::F64(vec![1.,2.,f64::NAN,4.]),spectrum:None});
        let layer=ImageLayer{index:0,image:image.clone(),label:"fixture".into(),opacity:1.,scale:2.,offset:1.,transform:[[1.,0.,0.],[0.,1.,0.]],revision:0,order:0};
        let options=RenderOptions::default();let mapping=ScaleMapping::new(None,0.,10.,fitsviewer_core::Stretch::Linear,1000.);let transfer=Transfer{mapping:&mapping,invert:false,tone:DisplayTone::default(),options:&options,key:String::new()};
        let pixels=rgba(&layer,[2,2],&transfer);assert_eq!(&pixels[0..4],&[0,0,0,0]);assert_eq!(pixels[4],230);assert_eq!(pixels[8],77);assert_eq!(pixels[12],128);assert_eq!(image.physical(0),Some(1.));assert_eq!(std::sync::Arc::strong_count(&image),2);
    }
    #[test]fn texture_cache_is_bounded_reused_and_released_when_hidden(){
        let image=std::sync::Arc::new(FitsImage{info:fitsviewer_core::ImageInfo{path:"cache-fixture".into(),hdu:0,width:1,height:1,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:None},pixels:fitsviewer_core::RawPixels::F64(vec![1.]),spectrum:None});
        let mut layers=(0..9).map(|index|ImageLayer{index,image:image.clone(),label:String::new(),opacity:1.,scale:1.,offset:0.,transform:[[1.,0.,0.],[0.,1.,0.]],revision:0,order:index as i32}).collect::<Vec<_>>();
        let ctx=egui::Context::default();let mut renderer=Renderer::default();let options=RenderOptions::default();let mapping=ScaleMapping::new(None,0.,2.,fitsviewer_core::Stretch::Linear,1000.);
        let _=ctx.run(Default::default(),|ctx|{
            let base=ctx.load_texture("base",egui::ColorImage::from_rgba_unmultiplied([1,1],&[255,255,255,255]),Default::default());let painter=ctx.layer_painter(egui::LayerId::new(egui::Order::Background,egui::Id::new("layer-cache-test")));let rect=Rect::from_min_max(Pos2::ZERO,Pos2::new(100.,100.));
            let transfer=||Transfer{mapping:&mapping,invert:false,tone:DisplayTone::default(),options:&options,key:"unchanged".into()};
            renderer.paint(ctx,&painter,base.id(),rect,100.,(true,1.),4,&layers,transfer());assert_eq!(renderer.textures.len(),8);assert_eq!(renderer.builds,8);
            layers[0].opacity=0.5;layers[0].order=7;renderer.paint(ctx,&painter,base.id(),rect,100.,(true,0.5),1,&layers,transfer());assert_eq!(renderer.builds,8);
            renderer.paint(ctx,&painter,base.id(),rect,100.,(false,1.),1,&[],transfer());assert_eq!(renderer.textures.len(),0);
        });assert_eq!(std::sync::Arc::strong_count(&image),10);
    }

}
