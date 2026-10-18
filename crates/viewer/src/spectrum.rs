use crate::plot_geometry::{screen,draw_curve,axis_padding};
use eframe::egui::{self,Color32,Pos2,Rect,Stroke,Vec2};
use fitsviewer_core::FitsImage;
use crate::spectral_analysis::{Analysis,AxisTransform};
use crate::spectral_lines::{Controls,unit_factor,observed};
use crate::multiview::SpectrumLayer;

pub struct Plot {
    pub bounds:[f64;4],pub fit:bool,pub errors:bool,
    axis:Option<AxisTransform>,preview:String,overlay_signature:String,display_generation:u64,selection_start:Option<f64>,selection_target:String,
}
impl Default for Plot{fn default()->Self{Self{bounds:[0.,1.,0.,1.],fit:true,errors:true,axis:None,preview:String::new(),overlay_signature:String::new(),display_generation:0,selection_start:None,selection_target:String::new()}}}


pub fn comparison_x(source:f64,layer:&SpectrumLayer,main:AxisTransform)->Option<f64>{
    let axis=AxisTransform{factor:unit_factor(&layer.unit)?,redshift:layer.redshift,air:layer.air,..main};
    match main.mode{0=>{let vacuum=axis.vacuum(source)?;Some((if main.air{observed(vacuum,0.,true)?}else{vacuum})/main.factor)},1=>Some(axis.forward(source)?*axis.factor/main.factor),_=>axis.forward(source)}
}
fn extend(bounds:&mut[f64;4],x:f64,y:f64){if x.is_finite() && y.is_finite(){bounds[0]=bounds[0].min(x);bounds[1]=bounds[1].max(x);bounds[2]=bounds[2].min(y);bounds[3]=bounds[3].max(y);}}
// Scale padding to the actual physical values; tiny calibrated flux is not a zero.

fn residual_extent(values:&[f64],flux_scale:f64)->f64{
    let maximum=values.iter().filter(|value|value.is_finite()).map(|value|value.abs()).fold(0_f64,f64::max);
    if maximum>0.{maximum}else if flux_scale.is_finite() && flux_scale>0.{flux_scale*0.01}else{1.}
}
impl Plot{
    pub fn show(&mut self,ui:&mut egui::Ui,image:&FitsImage,lines:Option<&Controls>,mut analysis:Option<&mut Analysis>,layers:&[SpectrumLayer],source_style:(bool,f32),source_order:i32)->String{
        let Some(data)=&image.spectrum else{return String::new();};let meta=image.info.spectrum.as_ref().unwrap();
        let axis=analysis.as_ref().zip(lines).map(|(analysis,lines)|analysis.axis(lines)).unwrap_or_else(AxisTransform::observed);
        let preview=analysis.as_ref().map(|analysis|analysis.display.preview.clone()).unwrap_or_else(||"Original".into());
        let layers=if lines.is_some_and(|lines|lines.ready()) && preview!="Normalized"{layers}else{&[]};
        let generation=analysis.as_ref().map(|analysis|analysis.display_generation).unwrap_or(0);
        let signature=layers.iter().map(|layer|format!("{}:{}:{}:{}:{}:{}:{}",layer.image.info.path,layer.scale,layer.offset,layer.unit,layer.redshift,layer.air,layer.image.info.hdu)).collect::<Vec<_>>().join("|");
        if self.axis!=Some(axis) || self.preview!=preview || self.overlay_signature!=signature || self.display_generation!=generation{self.fit=true;self.axis=Some(axis);self.preview=preview.clone();self.overlay_signature=signature;self.display_generation=generation;}
        let (response,painter)=ui.allocate_painter(ui.available_size(),egui::Sense::click_and_drag());let outer=response.rect.shrink2(Vec2::new(56.,36.));
        if outer.width()<8. || outer.height()<8.{return String::new();}
        let residual=analysis.as_ref().filter(|analysis|analysis.display.show_residual && outer.height()>180.).and_then(|analysis|analysis.series.iter().find(|series|series.kind=="residual"));
        let mut rect=outer;if residual.is_some(){rect.max.y-=outer.height()*0.25+16.;}
        let target=analysis.as_ref().map(|analysis|if ui.input(|input|input.modifiers.shift) && analysis.drag_target=="Pan"{"Region".into()}else{analysis.drag_target.clone()}).unwrap_or_else(||"Pan".into());
        let selecting=analysis.is_some() && target!="Pan";
        if response.drag_started_by(egui::PointerButton::Primary) && selecting{
            self.selection_start=response.interact_pointer_pos().and_then(|position|axis.inverse(self.bounds[0]+f64::from(((position.x-rect.left())/rect.width()).clamp(0.,1.))*(self.bounds[1]-self.bounds[0])));self.selection_target=target;
        }
        if response.drag_stopped_by(egui::PointerButton::Primary){if let (Some(start),Some(position),Some(analysis))=(self.selection_start.take(),response.interact_pointer_pos(),analysis.as_deref_mut()){
            if let Some(end)=axis.inverse(self.bounds[0]+f64::from(((position.x-rect.left())/rect.width()).clamp(0.,1.))*(self.bounds[1]-self.bounds[0])){analysis.selection(start,end,&self.selection_target);}
        }}
        if response.dragged_by(egui::PointerButton::Middle) || (response.dragged_by(egui::PointerButton::Primary) && !selecting && self.selection_start.is_none()){
            let delta=response.drag_delta();let dx=f64::from(delta.x/rect.width())*(self.bounds[1]-self.bounds[0]);let dy=f64::from(delta.y/rect.height())*(self.bounds[3]-self.bounds[2]);self.bounds[0]-=dx;self.bounds[1]-=dx;self.bounds[2]+=dy;self.bounds[3]+=dy;
        }
        let primary_kind=match preview.as_str(){"Normalized"=>"normalized","Subtracted"=>"subtracted","Smoothed"=>"smooth",_=>""};
        let primary=analysis.as_ref().and_then(|analysis|analysis.series.iter().rev().find(|series|series.kind==primary_kind));
        let original=primary.is_none() || (preview!="Normalized" && analysis.as_ref().is_some_and(|analysis|analysis.display.show_original));
        if self.fit{
            let mut bounds=[f64::INFINITY,f64::NEG_INFINITY,f64::INFINITY,f64::NEG_INFINITY];
            if original{for (index,&x) in data.x.iter().enumerate(){if let (Some(x),Some(y))=(axis.forward(x),image.physical(index)){extend(&mut bounds,x,y);}}}
            if let Some(series)=primary{for (&x,&y) in series.x.iter().zip(&series.y){if let Some(x)=axis.forward(x){extend(&mut bounds,x,y);}}}
            for layer in layers{if let Some(data)=&layer.image.spectrum{for (index,&x) in data.x.iter().enumerate(){if let (Some(x),Some(y))=(comparison_x(x,layer,axis),layer.image.physical(index)){extend(&mut bounds,x,y*layer.scale+layer.offset);}}}}
            if bounds.iter().all(|value|value.is_finite()){for index in [0,2]{let padding=axis_padding(bounds[index],bounds[index+1]);bounds[index]-=padding;bounds[index+1]+=padding;}self.bounds=bounds;}self.fit=false;
        }
        if response.hovered(){let scroll=ui.ctx().input(|input|input.smooth_scroll_delta.y);if scroll!=0.{if let Some(position)=response.hover_pos(){let factor=f64::from((-scroll/200.).exp()).clamp(0.5,2.);
            for (index,t) in [(0,f64::from((position.x-rect.left())/rect.width())),(2,f64::from((rect.bottom()-position.y)/rect.height()))]{let anchor=self.bounds[index]+t*(self.bounds[index+1]-self.bounds[index]);let next=[anchor+(self.bounds[index]-anchor)*factor,anchor+(self.bounds[index+1]-anchor)*factor];if next[0].is_finite() && next[1].is_finite() && next[1]>next[0]{self.bounds[index]=next[0];self.bounds[index+1]=next[1];}}
        }}}
        let stroke=Stroke::new(1_f32,Color32::GRAY);painter.line_segment([rect.left_top(),rect.left_bottom()],stroke);painter.line_segment([rect.left_bottom(),rect.right_bottom()],stroke);
        for tick in 0..=4{let fraction=f64::from(tick)/4.;let x=self.bounds[0]+fraction*(self.bounds[1]-self.bounds[0]);let y=self.bounds[2]+fraction*(self.bounds[3]-self.bounds[2]);
            painter.text(Pos2::new(screen(rect,self.bounds,x,self.bounds[2]).x,rect.bottom()+3.),egui::Align2::CENTER_TOP,format!("{x:.3}"),egui::FontId::proportional(11.),Color32::GRAY);
            painter.text(Pos2::new(rect.left()-4.,screen(rect,self.bounds,self.bounds[0],y).y),egui::Align2::RIGHT_CENTER,format!("{y:.2e}"),egui::FontId::proportional(11.),Color32::GRAY);
        }
        let unit=lines.filter(|lines|lines.ready()).map(|lines|if lines.manual_unit{format!("{} ({})",lines.unit,crate::i18n::t("manual interpretation"))}else{lines.unit.clone()}).unwrap_or_else(||if meta.x_unit.is_empty(){"unit unknown".into()}else{meta.x_unit.clone()});
        let x_label=match axis.mode{1=>format!("{} [{}] · vacuum",crate::i18n::t("Rest wavelength"),unit),2=>format!("{} [km/s] · lambda0={:.3} A vacuum",crate::i18n::t("Velocity"),axis.reference),_=>format!("{} [{}]",meta.x_label,unit)};
        painter.text(Pos2::new(rect.center().x,response.rect.bottom()-2.),egui::Align2::CENTER_BOTTOM,x_label,egui::FontId::proportional(13.),Color32::LIGHT_GRAY);
        let y_label=if preview=="Normalized" && primary.is_some(){crate::i18n::t("Normalized flux (relative)").to_owned()}else{format!("{} [{}]",meta.y_label,if meta.y_unit.is_empty(){"unit unknown"}else{&meta.y_unit})};
        painter.text(Pos2::new(rect.left(),response.rect.top()+3.),egui::Align2::LEFT_TOP,y_label,egui::FontId::proportional(13.),Color32::LIGHT_GRAY);
        if let Some(analysis)=analysis.as_ref(){analysis.paint_regions(&painter,rect,self.bounds,axis);}
        if source_style.0 && self.errors && original && primary.is_none(){if let Some(errors)=&data.error{let clipped=painter.with_clip_rect(rect);for index in (0..data.x.len()).step_by((data.x.len()/1500).max(1)){
            if let (Some(x),Some(y))=(axis.forward(data.x[index]),image.physical(index).filter(|_|errors[index].is_finite())){clipped.line_segment([screen(rect,self.bounds,x,y-errors[index]),screen(rect,self.bounds,x,y+errors[index])],Stroke::new(1_f32,Color32::from_rgba_unmultiplied(130,145,160,85).gamma_multiply(source_style.1.clamp(0.,1.))));}
        }}}
        let paint_source=||{if source_style.0 && source_style.1>0.{
            if original{draw_curve(&painter,rect,self.bounds,&data.x,|x|axis.forward(x),|index|image.physical(index),Color32::from_rgb(110,220,235).gamma_multiply(source_style.1.clamp(0.,1.)));}
            if let Some(series)=primary{draw_curve(&painter,rect,self.bounds,&series.x,|x|axis.forward(x),|index|series.y[index].is_finite().then_some(series.y[index]),Color32::from_rgb(210,140,245).gamma_multiply(source_style.1.clamp(0.,1.)));}
        }};
        let colors=[Color32::from_rgb(245,170,115),Color32::from_rgb(170,185,255),Color32::from_rgb(225,150,205),Color32::from_rgb(140,225,180)];
        let mut base_painted=false;
        for (index,layer) in layers.iter().enumerate(){if !base_painted && layer.order>=source_order{paint_source();base_painted=true;}
            if let Some(data)=&layer.image.spectrum{
                draw_curve(&painter,rect,self.bounds,&data.x,|x|comparison_x(x,layer,axis),|sample|layer.image.physical(sample).map(|y|y*layer.scale+layer.offset),colors[index%colors.len()].gamma_multiply(layer.opacity.clamp(0.,1.)));
                painter.text(Pos2::new(rect.left()+6.,rect.top()+55.+index as f32*14.),egui::Align2::LEFT_TOP,format!("{} {} · x{:.3} {}",layer.label,layer.epoch,layer.scale,crate::spectral_analysis::format_number(layer.offset)),egui::FontId::proportional(11.),colors[index%colors.len()].gamma_multiply(layer.opacity.clamp(0.,1.)));
            }
        }if !base_painted{paint_source();}
        if let Some(analysis)=analysis.as_ref(){
            if preview!="Normalized"{for series in &analysis.series{
                let color=match series.kind.as_str(){"continuum" if analysis.display.show_continuum=>Some(Color32::from_rgb(115,220,125)),"smooth" if preview=="Original"=>Some(Color32::from_rgb(215,135,245)),"model" if analysis.display.show_model=>Some(Color32::from_rgb(255,150,85)),"component" if analysis.display.show_model=>Some(Color32::from_rgb(160,135,85)),_=>None};
                if let Some(color)=color{draw_curve(&painter,rect,self.bounds,&series.x,|x|axis.forward(x),|index|series.y[index].is_finite().then_some(series.y[index]),color);}
            }}
            if analysis.display.blackbody && lines.is_some_and(|lines|lines.ready()){
                let mut points=Vec::new();for index in 0..=500{let x=self.bounds[0]+(self.bounds[1]-self.bounds[0])*index as f64/500.;if let Some(source)=axis.inverse(x){if let Some(y)=analysis.blackbody(source,axis){let point=screen(rect,self.bounds,x,y);if point.is_finite(){points.push(point);}}}}
                if points.len()>1{painter.with_clip_rect(rect).add(egui::Shape::line(points,Stroke::new(1.5_f32,Color32::from_rgb(250,110,125))));}
            }
            for line in analysis.display.custom_lines.iter().filter(|line|line.show){
                let rest=if line.medium=="air"{AxisTransform{air:true,..AxisTransform::observed()}.vacuum(line.rest)}else{Some(line.rest)};
                if let Some(x)=rest.and_then(|rest|observed(rest,axis.redshift,axis.air)).and_then(|wave|axis.forward(wave/axis.factor)){
                    let px=screen(rect,self.bounds,x,self.bounds[2]).x;if px>=rect.left() && px<=rect.right(){let clipped=painter.with_clip_rect(rect);clipped.line_segment([Pos2::new(px,rect.top()),Pos2::new(px,rect.bottom())],Stroke::new(1_f32,Color32::LIGHT_RED));clipped.text(Pos2::new(px+3.,rect.center().y),egui::Align2::LEFT_TOP,&line.name,egui::FontId::proportional(10.),Color32::LIGHT_RED);}
                }
            }
        }
        if let Some(lines)=lines{lines.paint(&painter,rect,self.bounds,axis);if let Some(text)=lines.hover_candidate(response.hover_pos(),rect,self.bounds,axis){response.clone().on_hover_text(text);}}
        if let Some(residual)=analysis.as_ref().filter(|analysis|analysis.display.show_residual && outer.height()>180.).and_then(|analysis|analysis.series.iter().find(|series|series.kind=="residual")){
            let residual_rect=Rect::from_min_max(Pos2::new(rect.left(),rect.bottom()+28.),outer.right_bottom());let maximum=residual_extent(&residual.y,self.bounds[2].abs().max(self.bounds[3].abs()));
            let bounds=[self.bounds[0],self.bounds[1],-maximum*1.1,maximum*1.1];painter.line_segment([screen(residual_rect,bounds,bounds[0],0.),screen(residual_rect,bounds,bounds[1],0.)],Stroke::new(1_f32,Color32::DARK_GRAY));
            draw_curve(&painter,residual_rect,bounds,&residual.x,|x|axis.forward(x),|index|residual.y[index].is_finite().then_some(residual.y[index]),Color32::LIGHT_GREEN);
            painter.text(residual_rect.left_top(),egui::Align2::LEFT_TOP,crate::i18n::t("Residual (source flux units)"),egui::FontId::proportional(10.),Color32::GRAY);
        }
        if let (Some(start),Some(position))=(self.selection_start,response.interact_pointer_pos()){if let Some(x)=axis.forward(start){let left=screen(rect,self.bounds,x,self.bounds[2]).x;painter.with_clip_rect(rect).rect_filled(Rect::from_min_max(Pos2::new(left.min(position.x),rect.top()),Pos2::new(left.max(position.x),rect.bottom())),0.,Color32::from_rgba_unmultiplied(140,180,255,45));}}
        if let Some(position)=response.hover_pos().filter(|position|rect.contains(*position)){
            let shown=self.bounds[0]+f64::from((position.x-rect.left())/rect.width())*(self.bounds[1]-self.bounds[0]);if let Some(target)=axis.inverse(shown){
                if let Some(index)=(0..data.x.len()).filter(|&index|data.x[index].is_finite()).min_by(|&a,&b|(data.x[a]-target).abs().total_cmp(&(data.x[b]-target).abs())){
                    return format!("sample={} · source x={:.8} · display x={:.5} · flux={}",index+1,data.x[index],shown,image.physical(index).map(|value|format!("{value:.8e}")).unwrap_or_else(||"invalid".into()));
                }
            }
        }String::new()
    }
}
#[cfg(test)]
mod tests{
    use super::*;
    #[test]fn tiny_physical_flux_keeps_its_plot_scale(){
        let padding=axis_padding(1e-17,2e-17);assert!((padding/5e-19-1.).abs()<1e-12);
        assert!(axis_padding(1e-17,1e-17)<1e-17);assert_eq!(axis_padding(0.,0.),1.);
        assert_eq!(residual_extent(&[f64::NAN,1e-19,-2e-19],1e-17),2e-19);
        assert!(residual_extent(&[0.,0.],1e-17)>0.);
    }
    #[test]fn arriving_normalized_preview_refits_from_physical_flux(){
        let meta=fitsviewer_core::SpectrumMeta{x_label:"Wavelength".into(),x_unit:"Angstrom".into(),y_label:"Flux".into(),y_unit:String::new(),columns:vec!["wave".into(),"flux".into()],x_column:0,y_column:1,error_column:None,error_hdu:None,uncertainty_kind:String::new(),note:String::new()};
        let image=FitsImage{info:fitsviewer_core::ImageInfo{path:"numeric-fixture".into(),hdu:0,width:2,height:1,bitpix:-64,bscale:1.,bzero:0.,blank:None,hdus:vec![],header:String::new(),spectrum:Some(meta)},pixels:fitsviewer_core::RawPixels::F64(vec![1e-17,2e-17]),spectrum:Some(fitsviewer_core::SpectrumData{x:vec![4000.,5000.],error:None})};
        let lines=Controls::new(&image);let mut analysis=Analysis::new(&image,None);analysis.sync_context(&lines);analysis.display.preview="Normalized".into();let mut plot=Plot::default();let ctx=egui::Context::default();
        let input=||egui::RawInput{screen_rect:Some(Rect::from_min_size(Pos2::ZERO,egui::vec2(800.,600.))),..Default::default()};
        let _=ctx.run(input(),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{plot.show(ui,&image,Some(&lines),Some(&mut analysis),&[],(true,1.),0);});});
        assert!(plot.bounds[3]<1e-16);
        let value=serde_json::json!({"operation":"spectral_analysis","action":"continuum","source":analysis.source,"revision":analysis.revision,"series":[{"name":"normalized","kind":"normalized","x":[4000.,5000.],"y":[0.8,1.2]}]});assert!(analysis.accept(&value));
        let _=ctx.run(input(),|ctx|{egui::CentralPanel::default().show(ctx,|ui|{plot.show(ui,&image,Some(&lines),Some(&mut analysis),&[],(true,1.),0);});});
        assert!(plot.bounds[2]<0.8 && plot.bounds[2]>0.5);assert!(plot.bounds[3]>1.2 && plot.bounds[3]<1.5);
    }

}
