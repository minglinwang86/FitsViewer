//! Manual original-pixel selection. No centroiding, apertures or science mutation.
use eframe::egui::{self,Color32};
use serde_json::{json,Value};
use crate::image_transform::ImageTransform;

#[derive(Default)]
pub(super) struct Selection{pub session:Option<Session>,message:String}
pub(super) struct Session{pub id:String,pub revision:u64,pub status:&'static str,pub points:Vec<[f64;2]>,pub source:Value,pub generation:u64,max:usize,prompt:String,markers:Vec<Value>,view:Option<[f64;2]>,config_revision:u64}
impl Selection{
    pub fn active(&self)->bool{self.session.as_ref().is_some_and(|s|s.status=="selecting")}
    pub fn begin(&mut self,params:&Value,source:Value,generation:u64)->Result<(),String>{
        let id=params["id"].as_str().filter(|s|!s.is_empty() && s.len()<=128).ok_or("Invalid selection ID")?;
        let max=params["max_points"].as_u64().filter(|n|(1..=4096).contains(n)).ok_or("Invalid selection point limit")? as usize;
        if params["path"]!=source["path"] || params["hdu"]!=source["hdu"]{return Err("Selection source does not match the loaded image".into());}
        if source["identity"].as_str().is_none_or(str::is_empty){return Err("Selection requires a source identity".into());}
        if let Some(expected)=params["identity"].as_str(){if Some(expected)!=source["identity"].as_str(){return Err("Selection source identity changed".into());}}
        if self.active(){return Err("Cancel the active selection before starting another".into());}
        self.message.clear();self.session=Some(Session{id:id.into(),revision:1,status:"selecting",points:vec![],source,generation,max,prompt:String::new(),markers:vec![],view:None,config_revision:0});Ok(())
    }
    pub fn observe_view(&mut self,low:f64,high:f64){if let Some(s)=self.session.as_mut().filter(|s|s.status=="selecting"){let view=[low,high];if low.is_finite() && high.is_finite() && s.view!=Some(view){s.view=Some(view);s.revision+=1;}}}
    pub fn configure(&mut self,params:&Value)->Result<(),String>{
        let s=self.session.as_mut().filter(|s|s.status=="selecting" && params["id"].as_str()==Some(s.id.as_str())).ok_or("No matching active selection")?;
        let prompt=params["prompt"].as_str().unwrap_or("");if prompt.chars().count()>256{return Err("Selection prompt exceeds 256 characters".into());}
        let markers=params["markers"].as_array().cloned().unwrap_or_default();if markers.len()>1024{return Err("Too many selection markers".into());}
        for marker in &markers{if marker["x"].as_f64().is_none_or(|v|!v.is_finite()) || marker["y"].as_f64().is_none_or(|v|!v.is_finite()) || marker["radius"].as_f64().is_none_or(|v|!v.is_finite() || v<=0. || v>4096.) || !["circle","box"].contains(&marker["shape"].as_str().unwrap_or("circle")) || marker["label"].as_str().is_some_and(|v|v.chars().count()>128){return Err("Invalid selection marker".into());}}
        s.prompt=prompt.into();s.markers=markers;s.config_revision=params["config_revision"].as_u64().unwrap_or(0);s.revision+=1;Ok(())
    }
    pub fn state(&self)->Value{self.session.as_ref().map_or(Value::Null,|s|json!({"schema":1,"id":s.id,"revision":s.revision,"status":s.status,"points":s.points,"source":s.source,"origin":0,"coordinate_system":"image","max_points":s.max,"applied_config_revision":s.config_revision}))}
    pub fn cancel(&mut self){self.terminal("cancelled");}
    pub fn fail(&mut self){self.terminal("failed");self.message="Selection session is unavailable or already used by another page".into();}
    fn terminal(&mut self,status:&'static str){if let Some(s)=self.session.as_mut().filter(|s|s.status=="selecting"){s.status=status;s.revision+=1;}}
    pub fn finish(&mut self){self.terminal("completed");}
    pub fn undo(&mut self){if let Some(s)=self.session.as_mut().filter(|s|s.status=="selecting" && !s.points.is_empty()){s.points.pop();s.revision+=1;}}
    pub fn clear(&mut self){if let Some(s)=self.session.as_mut().filter(|s|s.status=="selecting" && !s.points.is_empty()){s.points.clear();s.revision+=1;}}
    pub fn validate(&mut self,generation:u64,path:Option<&str>,hdu:Option<usize>,identity:Option<&str>,loading:bool){
        if self.session.as_ref().is_some_and(|s|s.status=="selecting" && (loading || generation!=s.generation || path!=s.source["path"].as_str() || hdu.map(|n|n as u64)!=s.source["hdu"].as_u64() || identity.is_some_and(|v|Some(v)!=s.source["identity"].as_str()))){self.cancel();}
    }
    fn add(&mut self,point:[f64;2]){if let Some(s)=self.session.as_mut().filter(|s|s.status=="selecting" && s.points.len()<s.max){
        let w=s.source["width"].as_f64().unwrap_or(0.);let h=s.source["height"].as_f64().unwrap_or(0.);
        if point.iter().all(|v|v.is_finite()) && point[0]>=0. && point[1]>=0. && point[0]<w && point[1]<h{s.points.push(point);s.revision+=1;}
    }}
    pub fn toolbar(&mut self,ctx:&egui::Context){
        if !self.message.is_empty(){egui::TopBottomPanel::top("manual-selection-error").show(ctx,|ui|{ui.colored_label(Color32::LIGHT_RED,crate::i18n::t(&self.message));});}
        if !self.active(){return;}
        egui::TopBottomPanel::top("manual-selection").show(ctx,|ui|{ui.horizontal_wrapped(|ui|{
            let s=self.session.as_ref().unwrap();ui.strong(crate::i18n::t("Select stars"));ui.label(format!("{} / {}",s.points.len(),s.max));
            if ui.button(crate::i18n::t("Finish selection (Q)")).clicked(){self.finish();}
            if ui.button(crate::i18n::t("Undo last point")).clicked(){self.undo();}
            if ui.button(crate::i18n::t("Clear selection")).clicked(){self.clear();}
            if ui.button(crate::i18n::t("Cancel selection (Esc)")).clicked(){self.cancel();}
        });ui.label(crate::i18n::t("Click or X: select · Middle drag: pan · Q: finish · Backspace: undo"));if let Some(s)=&self.session{if !s.prompt.is_empty(){ui.label(&s.prompt);}}});
        if !ctx.wants_keyboard_input(){ctx.input(|input|{if input.key_pressed(egui::Key::Escape){self.cancel();}else if input.key_pressed(egui::Key::Q){self.finish();}else if input.key_pressed(egui::Key::Backspace){self.undo();}});}
    }
    pub fn canvas(&mut self,ctx:&egui::Context,response:&egui::Response,painter:&egui::Painter,transform:ImageTransform,allow:bool){
        if !self.active(){return;}
        if allow && response.hovered() && !ctx.wants_keyboard_input() && (response.clicked_by(egui::PointerButton::Primary) || ctx.input(|i|i.key_pressed(egui::Key::X))){if let Some(pos)=response.hover_pos().filter(|p|painter.clip_rect().contains(*p)){self.add(transform.screen_to_pixel(pos));}}
        if let Some(s)=&self.session{
            for marker in &s.markers{
                let point=[marker["x"].as_f64().unwrap(),marker["y"].as_f64().unwrap()];let p=transform.pixel_to_screen(point);let radius=marker["radius"].as_f64().unwrap();
                let color=match marker["color"].as_str().unwrap_or("green"){"red"=>Color32::RED,"blue"=>Color32::LIGHT_BLUE,"yellow"=>Color32::YELLOW,"cyan"=>Color32::CYAN,"magenta"=>Color32::MAGENTA,"white"=>Color32::WHITE,_=>Color32::GREEN};let stroke=egui::Stroke::new(1.5,color);
                if marker["shape"]=="box"{let corners=[[-radius,-radius],[radius,-radius],[radius,radius],[-radius,radius]];let points:Vec<_>=corners.iter().map(|c|transform.pixel_to_screen([point[0]+c[0],point[1]+c[1]])).collect();painter.add(egui::Shape::closed_line(points,stroke));}else{painter.circle_stroke(p,(radius as f32*transform.zoom).min(1e7),stroke);}
                if let Some(label)=marker["label"].as_str(){painter.text(p+egui::vec2(8.,8.),egui::Align2::LEFT_TOP,label,egui::FontId::proportional(13.),color);}
            }
            for (i,point) in s.points.iter().enumerate(){let p=transform.pixel_to_screen(*point);let color=Color32::YELLOW;let stroke=egui::Stroke::new(1.5,color);painter.line_segment([p-egui::vec2(7.,0.),p+egui::vec2(7.,0.)],stroke);painter.line_segment([p-egui::vec2(0.,7.),p+egui::vec2(0.,7.)],stroke);painter.text(p+egui::vec2(8.,-8.),egui::Align2::LEFT_BOTTOM,(i+1).to_string(),egui::FontId::proportional(14.),color);}
        }
    }
}

#[cfg(test)]mod tests{
    use super::*;
    fn setup()->Selection{let mut s=Selection::default();s.begin(&json!({"id":"owned","path":"a.fits","hdu":0,"max_points":2,"identity":"v1"}),json!({"path":"a.fits","hdu":0,"width":100,"height":80,"identity":"v1"}),4).unwrap();s}
    #[test]fn original_float_coordinates_and_point_limit(){let mut s=setup();s.add([12.25,33.75]);s.add([0.,0.]);s.add([4.,5.]);assert_eq!(s.state()["points"],json!([[12.25,33.75],[0.,0.]]));assert_eq!(s.state()["origin"],0);s.finish();assert_eq!(s.state()["status"],"completed");s.cancel();assert_eq!(s.state()["status"],"completed");}
    #[test]fn bounds_undo_clear_and_revision(){let mut s=setup();for p in [[-0.1,0.],[100.,0.],[0.,80.],[f64::NAN,0.]]{s.add(p);}assert_eq!(s.state()["revision"],1);s.add([1.,2.]);s.undo();assert_eq!(s.state()["revision"],3);s.add([3.,4.]);s.clear();assert_eq!(s.state()["points"],json!([]));}
    #[test]fn document_generation_and_identity_cancel(){for change in 0..5{let mut s=setup();s.validate(if change==0{5}else{4},Some(if change==1{"b.fits"}else{"a.fits"}),Some(if change==2{1}else{0}),Some(if change==3{"v2"}else{"v1"}),change==4);assert_eq!(s.state()["status"],"cancelled");}}
    #[test]fn rotated_pick_preserves_original_coordinates(){let mut s=setup();let t=ImageTransform::new(egui::pos2(400.,300.),egui::vec2(100.,80.),3.2,37.);s.add(t.screen_to_pixel(t.pixel_to_screen([12.25,33.75])));let p=s.session.unwrap().points[0];assert!((p[0]-12.25).abs()<0.0001 && (p[1]-33.75).abs()<0.0001);}
}
