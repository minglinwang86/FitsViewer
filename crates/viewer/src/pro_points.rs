//! Original-pixel seed/path editing; mouse modes are exclusive with aperture edits.
//! 原始像素上的种子/路径控制点；不将旋转后的屏幕数值写入科研请求。
use eframe::egui::{self,Color32,Stroke};
use serde_json::{json,Value};
use crate::{image_transform::ImageTransform,i18n::t};

#[derive(Default)]
pub struct Points {pub active:bool,selected:Option<usize>,drag:Option<usize>}
fn pair(v:&Value)->Option<[f64;2]>{Some([v[0].as_f64()?,v[1].as_f64()?])}
impl Points {
    pub fn cancel(&mut self){self.active=false;self.drag=None;}
    pub fn ui(&mut self,ui:&mut egui::Ui,operation:&str,params:&mut Value,lines:&[Vec<[f64;2]>]){
        if !["trace_curve","line_profile"].contains(&operation){return;}
        let seed=operation=="trace_curve";
        ui.horizontal_wrapped(|ui|{
            if ui.selectable_label(self.active,t(if seed{"Pick seed on image"}else{"Draw / edit path on image"})).clicked(){self.active=!self.active;self.drag=None;}
            if self.active && ui.button(t("Finish picking")).clicked(){self.cancel();}
            if !seed && ui.add_enabled(!lines.is_empty(),egui::Button::new(t("Use detected trace"))).clicked(){params["points"]=json!(lines[0].iter().step_by((lines[0].len()/120).max(1)).take(128).collect::<Vec<_>>());}
        });
        if self.active{ui.label(t(if seed{"Click the image to place the seed; drag it to adjust. Esc finishes picking."}else{"Click to add points; drag a point to move it. Double-click or Esc finishes picking."}));}
        let key=if seed{"seed"}else{"points"};
        let mut points=if seed{pair(&params[key]).into_iter().collect::<Vec<_>>()}else{params[key].as_array().into_iter().flatten().filter_map(pair).collect::<Vec<_>>()};
        let mut changed=false;
        let mut remove=None;
        egui::CollapsingHeader::new(t("Control points [one-based pixels]")).id_salt("trace-control-points").default_open(seed).show(ui,|ui|{
            for (index,point) in points.iter_mut().enumerate(){ui.horizontal_wrapped(|ui|{
                if ui.selectable_label(self.selected==Some(index),format!("{}",index+1)).clicked(){self.selected=Some(index);}
                for value in point.iter_mut(){let mut displayed=*value+1.;if ui.add(egui::DragValue::new(&mut displayed).speed(0.1)).changed(){*value=displayed-1.;changed=true;}}
                if ui.small_button("×").clicked(){remove=Some(index);}
            });}
        });
        if let Some(index)=remove{points.remove(index);self.selected=None;changed=true;}
        if changed{params[key]=if seed{points.first().map_or(Value::Null,|p|json!(p))}else{json!(points)};}
    }
    pub fn canvas(&mut self,ctx:&egui::Context,response:&egui::Response,painter:&egui::Painter,transform:ImageTransform,operation:&str,params:&mut Value,interactive:bool)->bool{
        if !["trace_curve","line_profile"].contains(&operation){self.cancel();return false;}
        let seed=operation=="trace_curve";let key=if seed{"seed"}else{"points"};
        let mut points=if seed{pair(&params[key]).into_iter().collect::<Vec<_>>()}else{params[key].as_array().into_iter().flatten().filter_map(pair).collect::<Vec<_>>()};
        let mut changed=false;
        if self.active && interactive{
            if !ctx.wants_keyboard_input(){
                if ctx.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Escape)){self.cancel();}
                if ctx.input_mut(|i|i.consume_key(egui::Modifiers::NONE,egui::Key::Delete) || i.consume_key(egui::Modifiers::NONE,egui::Key::Backspace)){if let Some(index)=self.selected.take().filter(|&i|i<points.len()){points.remove(index);changed=true;}}
            }
            let pointer=response.interact_pointer_pos().or_else(||response.hover_pos());
            let nearest=pointer.and_then(|pos|points.iter().enumerate().map(|(i,p)|(i,transform.pixel_to_screen(*p).distance(pos))).filter(|(_,d)|*d<9.).min_by(|a,b|a.1.total_cmp(&b.1)).map(|(i,_)|i));
            if response.hovered() && ctx.input(|i|i.pointer.primary_pressed()){self.drag=nearest;self.selected=nearest;}
            if response.dragged_by(egui::PointerButton::Primary){if let (Some(index),Some(pos))=(self.drag,pointer){let p=transform.screen_to_pixel(pos);points[index]=[p[0].clamp(0.,f64::from(transform.dimensions.x)-1.),p[1].clamp(0.,f64::from(transform.dimensions.y)-1.)];changed=true;}}
            if response.clicked() && nearest.is_none(){if let Some(pos)=pointer.filter(|p|transform.contains(*p)){
                let p=transform.screen_to_pixel(pos);if seed{points=vec![p];self.selected=Some(0);}else if points.len()<128{points.push(p);self.selected=Some(points.len()-1);}changed=true;
            }}
            if response.double_clicked(){self.cancel();}
            if !ctx.input(|i|i.pointer.primary_down()){self.drag=None;}
            if response.hovered(){ctx.set_cursor_icon(if nearest.is_some(){egui::CursorIcon::Grab}else{egui::CursorIcon::Crosshair});}
        }
        if seed{if let Some(point)=points.first(){
            let axis=params["axis"].as_str().unwrap_or("x");let direction=params["direction"].as_f64().unwrap_or(1.);let half=params["search_half_width"].as_f64().unwrap_or(8.);
            let a=if axis=="x"{[point[0],point[1]-half]}else{[point[0]-half,point[1]]};let b=if axis=="x"{[point[0],point[1]+half]}else{[point[0]+half,point[1]]};
            painter.line_segment([transform.pixel_to_screen(a),transform.pixel_to_screen(b)],Stroke::new(2.0_f32,Color32::LIGHT_GREEN));
            let end=if axis=="x"{[point[0]+direction*35.,point[1]]}else{[point[0],point[1]+direction*35.]};
            let start=transform.pixel_to_screen(*point);painter.arrow(start,transform.pixel_to_screen(end)-start,Stroke::new(1.5_f32,Color32::YELLOW));
        }}else if points.len()>1{
            let width=params["width"].as_f64().unwrap_or(5.);let gap=params["background_gap"].as_f64().unwrap_or(2.);let background=params["background_width"].as_f64().unwrap_or(4.);
            let line=points.iter().map(|p|transform.pixel_to_screen(*p)).collect::<Vec<_>>();
            for (pixels,color) in [(width+2.*gap+2.*background,Color32::from_rgba_unmultiplied(70,180,100,35)),(width,Color32::from_rgba_unmultiplied(90,190,250,55))]{painter.add(egui::Shape::line(line.clone(),Stroke::new((pixels as f32*transform.zoom).max(1.),color)));}
            painter.add(egui::Shape::line(line,Stroke::new(1.3_f32,Color32::LIGHT_BLUE)));
        }
        for (index,point) in points.iter().enumerate(){let screen=transform.pixel_to_screen(*point);painter.circle_stroke(screen,if self.selected==Some(index){7.}else{4.},Stroke::new(1.5_f32,Color32::YELLOW));painter.text(screen+egui::vec2(5.,-5.),egui::Align2::LEFT_BOTTOM,(index+1).to_string(),egui::FontId::proportional(11.),Color32::YELLOW);}
        if changed{params[key]=if seed{points.first().map_or(Value::Null,|p|json!(p))}else{json!(points)};}
        changed
    }
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn clicking_a_rotated_image_records_the_original_pixel(){
        let ctx=egui::Context::default();let transform=ImageTransform::new(egui::pos2(180.,160.),egui::vec2(64.,64.),3.,37.);
        let mut points=Points{active:true,..Default::default()};let mut params=json!({"seed":null});let point=transform.pixel_to_screen([10.,20.]);
        let input=|events|egui::RawInput{screen_rect:Some(egui::Rect::from_min_size(egui::Pos2::ZERO,egui::vec2(500.,400.))),events,..Default::default()};
        let frame=|ctx:&egui::Context,points:&mut Points,params:&mut Value|{egui::CentralPanel::default().show(ctx,|ui|{let (response,painter)=ui.allocate_painter(egui::vec2(450.,350.),egui::Sense::click_and_drag());points.canvas(ctx,&response,&painter,transform,"trace_curve",params,true);});};
        let _=ctx.run(input(vec![]),|ctx|frame(ctx,&mut points,&mut params));
        for pressed in [true,false]{let _=ctx.run(input(vec![egui::Event::PointerMoved(point),egui::Event::PointerButton{pos:point,button:egui::PointerButton::Primary,pressed,modifiers:egui::Modifiers::NONE}]),|ctx|frame(ctx,&mut points,&mut params));}
        let actual=pair(&params["seed"]).expect("image click sets a seed");assert!((actual[0]-10.).abs()<0.0001 && (actual[1]-20.).abs()<0.0001);
    }
}
