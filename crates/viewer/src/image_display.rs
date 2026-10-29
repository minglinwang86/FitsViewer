//! Basic display overlays; shared by Mini and PRO without creating a PRO owner.
//! 基础显示叠加独立于科研工作区，不为极简模式初始化 PRO。
use eframe::egui::{self, Color32, Pos2, Rect, Stroke};
use serde_json::Value;
use crate::{controls::Preferences,image_transform::ImageTransform,i18n::t};

pub fn angular_scale(wcs:Option<&Value>)->Option<f64>{
    wcs.and_then(|value|value["compass"]["arcsec_per_pixel_x"].as_f64()).filter(|v|v.is_finite() && *v>0.)
}
/// Invert the local east/north pixel vectors rather than assuming square pixels.
/// 用局部天球基矢计算屏幕水平方向的角尺度，旋转后不假设像素正方形。
pub fn horizontal_angular_scale(wcs:Option<&Value>,rotation_deg:f32)->Option<f64>{
    let compass=&wcs?["compass"];
    let vector=|key:&str|Some([compass[key][0].as_f64()?,compass[key][1].as_f64()?]);
    if let (Some([ex,ey]),Some([nx,ny]))=(vector("east"),vector("north")){
        let determinant=ex*ny-nx*ey;
        if determinant.is_finite() && determinant.abs()>1e-15{
            let (s,c)=f64::from(rotation_deg).to_radians().sin_cos();
            let scale=((ny*c-nx*s)/determinant).hypot((-ey*c+ex*s)/determinant);
            return (scale.is_finite() && scale>0.).then_some(scale);
        }
    }
    (rotation_deg.rem_euclid(180.).abs()<0.0001).then(||angular_scale(wcs)).flatten()
}
fn chevron_menu(ui:&mut egui::Ui,label:&str,contents:impl FnOnce(&mut egui::Ui)){
    // Draw the disclosure mark; the bundled fonts need not contain U+25BE.
    let response=ui.menu_button(format!("{label}   "),contents).response;
    let x=if label.is_empty(){response.rect.center().x}else{response.rect.right()-9.};
    let center=egui::pos2(x,response.rect.center().y);
    let stroke=ui.style().interact(&response).fg_stroke;
    ui.painter().line_segment([center+egui::vec2(-3.,-1.5),center+egui::vec2(0.,1.5)],stroke);
    ui.painter().line_segment([center+egui::vec2(0.,1.5),center+egui::vec2(3.,-1.5)],stroke);
}
pub fn scale_controls(ui:&mut egui::Ui,prefs:&mut Preferences,wcs:Option<&Value>,rotation_deg:f32){
    ui.horizontal(|ui|{
        ui.spacing_mut().item_spacing.x=1.;
        if ui.selectable_label(prefs.scale_bar,t("Scale bar")).clicked(){prefs.scale_bar=!prefs.scale_bar;}
        chevron_menu(ui,"",|ui|{
            let scale=horizontal_angular_scale(wcs,rotation_deg);let mut length=prefs.scale_pixels*scale.unwrap_or(1.);
            ui.label(t(if scale.is_some(){"Length [arcsec]"}else{"Length [pixels]"}));
            if ui.add(egui::Slider::new(&mut length,0.01..=100000.).logarithmic(true)).changed(){prefs.scale_pixels=(length/scale.unwrap_or(1.)).clamp(0.001,1e7);}
            ui.weak(t(if scale.is_some(){"Angular scale at the current WCS location"}else{"Pixel scale; no reliable angular WCS is available"}));
        });
    });
}
pub fn paint(painter:&egui::Painter,viewport:Rect,transform:ImageTransform,prefs:&Preferences,wcs:Option<&Value>){
    let color=Color32::YELLOW;
    if prefs.scale_bar{
        let length=(prefs.scale_pixels as f32*transform.zoom).min((viewport.width()-44.).max(0.));
        let pixels=f64::from(length/transform.zoom);
        if length.is_finite() && length>=2. && length<viewport.width()-40.{
            let base=Pos2::new(viewport.left()+22.,viewport.bottom()-25.);
            painter.line_segment([base,base+egui::vec2(length,0.)],Stroke::new(2.0_f32,color));
            for x in [0.,length]{painter.line_segment([base+egui::vec2(x,-3.),base+egui::vec2(x,3.)],Stroke::new(1.0_f32,color));}
            let label=if let Some(scale)=horizontal_angular_scale(wcs,transform.rotation_deg){format!("{:.3} arcsec (local)",pixels*scale)}else{format!("{pixels:.2} px")};
            painter.text(base-egui::vec2(0.,5.),egui::Align2::LEFT_BOTTOM,label,egui::FontId::proportional(11.),color);
        }
    }
    if prefs.compass{if let Some(wcs)=wcs{
        let base=Pos2::new(viewport.left()+52.,viewport.top()+57.);
        for (name,key) in [("N","north"),("E","east")]{
            let v=&wcs["compass"][key];if let (Some(x),Some(y))=(v[0].as_f64(),v[1].as_f64()){
                let vector=transform.rotate_vector(egui::vec2(x as f32,-y as f32)).normalized()*30.;
                if vector.is_finite(){painter.arrow(base,vector,Stroke::new(1.4_f32,color));painter.text(base+vector,egui::Align2::CENTER_BOTTOM,name,egui::FontId::proportional(12.),color);}
            }
        }
    }}
}

#[cfg(test)]mod tests{
    use super::*;
    #[test]fn rotated_scale_respects_anisotropic_wcs_pixels(){
        let wcs=serde_json::json!({"compass":{"east":[0.5,0.],"north":[0.,1.],"arcsec_per_pixel_x":2.,"arcsec_per_pixel_y":1.}});
        for (angle,expected) in [(0.,2.),(90.,1.),(45.,2.5_f64.sqrt())]{assert!((horizontal_angular_scale(Some(&wcs),angle).unwrap()-expected).abs()<1e-10);}
        assert!(horizontal_angular_scale(None,37.).is_none());
    }
}
pub fn rotation_controls(ui:&mut egui::Ui,viewport:Rect,angle:&mut f32)->bool{
    let before=*angle;
    let rect=Rect::from_min_size(egui::pos2((viewport.right()-262.).max(viewport.left()),viewport.bottom()-34.),egui::vec2(254.,28.));
    ui.scope_builder(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)),|ui|{
        egui::Frame::new().fill(ui.visuals().window_fill()).corner_radius(4.).inner_margin(3.).show(ui,|ui|{
            if ui.button("↶ 90°").on_hover_text(t("Rotate left 90°")).clicked(){*angle-=90.;}
            if ui.button("↷ 90°").on_hover_text(t("Rotate right 90°")).clicked(){*angle+=90.;}
            chevron_menu(ui,&format!("{:.0}°",*angle),|ui|{
                ui.label(t("Display rotation"));ui.add(egui::Slider::new(angle,-180. ..=180.).suffix("°"));
            });
            if ui.button(t("Reset")).on_hover_text(t("Reset rotation")).clicked(){*angle=0.;}
        });
    });
    *angle=(*angle+180.).rem_euclid(360.)-180.;before!=*angle
}
