use crate::plot_geometry::{screen,draw_curve,axis_padding};
use eframe::egui::{self,Color32,Pos2,Rect,Stroke,Vec2};
use fitsviewer_core::FitsImage;
pub struct Plot{pub bounds:[f64;4],pub fit:bool,pub errors:bool}
impl Default for Plot{fn default()->Self{Self{bounds:[0.,1.,0.,1.],fit:true,errors:true}}}




impl Plot{
 pub fn show(&mut self,ui:&mut egui::Ui,image:&FitsImage)->String{
  let Some(data)=&image.spectrum else{return String::new();};let meta=image.info.spectrum.as_ref().unwrap();
  let (response,painter)=ui.allocate_painter(ui.available_size(),egui::Sense::click_and_drag());let rect=response.rect.shrink2(Vec2::new(56.,36.));
  if rect.width()<8. || rect.height()<8.{return String::new();}
  if self.fit{
   let mut b=[f64::INFINITY,f64::NEG_INFINITY,f64::INFINITY,f64::NEG_INFINITY];
   for (index,&x) in data.x.iter().enumerate(){if let Some(y)=image.physical(index).filter(|_|x.is_finite()){b[0]=b[0].min(x);b[1]=b[1].max(x);b[2]=b[2].min(y);b[3]=b[3].max(y);}}
   if b.iter().all(|v|v.is_finite()){for i in [0,2]{let pad=axis_padding(b[i],b[i+1]);b[i]-=pad;b[i+1]+=pad;}self.bounds=b;}self.fit=false;
  }
  if response.dragged_by(egui::PointerButton::Middle)||response.dragged_by(egui::PointerButton::Primary){let delta=response.drag_delta();let dx=f64::from(delta.x/rect.width())*(self.bounds[1]-self.bounds[0]);let dy=f64::from(delta.y/rect.height())*(self.bounds[3]-self.bounds[2]);self.bounds[0]-=dx;self.bounds[1]-=dx;self.bounds[2]+=dy;self.bounds[3]+=dy;}
  if response.hovered(){let scroll=ui.ctx().input(|i|i.smooth_scroll_delta.y);if scroll!=0.{if let Some(p)=response.hover_pos(){let factor=f64::from((-scroll/200.).exp()).clamp(0.5,2.);for (i,t) in [(0,f64::from((p.x-rect.left())/rect.width())),(2,f64::from((rect.bottom()-p.y)/rect.height()))]{let anchor=self.bounds[i]+t*(self.bounds[i+1]-self.bounds[i]);let next=[anchor+(self.bounds[i]-anchor)*factor,anchor+(self.bounds[i+1]-anchor)*factor];if next[0].is_finite()&&next[1].is_finite()&&next[1]>next[0]{self.bounds[i]=next[0];self.bounds[i+1]=next[1];}}}}}
  let stroke=Stroke::new(1_f32,Color32::GRAY);painter.line_segment([rect.left_top(),rect.left_bottom()],stroke);painter.line_segment([rect.left_bottom(),rect.right_bottom()],stroke);
  for tick in 0..=4{let f=f64::from(tick)/4.;let x=self.bounds[0]+f*(self.bounds[1]-self.bounds[0]);let y=self.bounds[2]+f*(self.bounds[3]-self.bounds[2]);painter.text(Pos2::new(screen(rect,self.bounds,x,self.bounds[2]).x,rect.bottom()+3.),egui::Align2::CENTER_TOP,format!("{x:.3}"),egui::FontId::proportional(11.),Color32::GRAY);painter.text(Pos2::new(rect.left()-4.,screen(rect,self.bounds,self.bounds[0],y).y),egui::Align2::RIGHT_CENTER,format!("{y:.2e}"),egui::FontId::proportional(11.),Color32::GRAY);}
  painter.text(Pos2::new(rect.center().x,response.rect.bottom()-2.),egui::Align2::CENTER_BOTTOM,format!("{} [{}]",meta.x_label,if meta.x_unit.is_empty(){"unit unknown"}else{&meta.x_unit}),egui::FontId::proportional(13.),Color32::LIGHT_GRAY);
  painter.text(Pos2::new(rect.left(),response.rect.top()+3.),egui::Align2::LEFT_TOP,format!("{} [{}]",meta.y_label,if meta.y_unit.is_empty(){"unit unknown"}else{&meta.y_unit}),egui::FontId::proportional(13.),Color32::LIGHT_GRAY);
  if self.errors{if let Some(errors)=&data.error{let clipped=painter.with_clip_rect(rect);for i in (0..data.x.len()).step_by((data.x.len()/1500).max(1)){if let Some(y)=image.physical(i).filter(|_|data.x[i].is_finite()&&errors[i].is_finite()){clipped.line_segment([screen(rect,self.bounds,data.x[i],y-errors[i]),screen(rect,self.bounds,data.x[i],y+errors[i])],Stroke::new(1_f32,Color32::from_rgba_unmultiplied(130,145,160,85)));}}}}
  draw_curve(&painter,rect,self.bounds,&data.x,|x|Some(x),|i|image.physical(i),Color32::from_rgb(110,220,235));
  if let Some(p)=response.hover_pos().filter(|p|rect.contains(*p)){let x=self.bounds[0]+f64::from((p.x-rect.left())/rect.width())*(self.bounds[1]-self.bounds[0]);if let Some(i)=(0..data.x.len()).filter(|&i|data.x[i].is_finite()).min_by(|&a,&b|(data.x[a]-x).abs().total_cmp(&(data.x[b]-x).abs())){return format!("sample={} · source x={:.8} · flux={}",i+1,data.x[i],image.physical(i).map(|v|format!("{v:.8e}")).unwrap_or_else(||"invalid".into()));}}
  String::new()
 }
}
