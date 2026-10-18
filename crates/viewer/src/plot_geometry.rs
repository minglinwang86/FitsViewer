use eframe::egui::{self,Color32,Pos2,Rect,Stroke};

pub(crate) fn screen(rect:Rect,bounds:[f64;4],x:f64,y:f64)->Pos2{Pos2::new(rect.left()+((x-bounds[0])/(bounds[1]-bounds[0])) as f32*rect.width(),rect.bottom()-((y-bounds[2])/(bounds[3]-bounds[2])) as f32*rect.height())}

pub(crate) fn draw_curve<X:Fn(f64)->Option<f64>,Y:Fn(usize)->Option<f64>>(painter:&egui::Painter,rect:Rect,bounds:[f64;4],x:&[f64],map_x:X,y:Y,color:Color32){
    let clipped=painter.with_clip_rect(rect);let step=(x.len()/(rect.width().max(1.) as usize*2)).max(1);let mut run=Vec::new();
    for start in (0..x.len()).step_by(step){
        let end=(start+step).min(x.len());let valid=(start..end).filter_map(|index|Some((index,map_x(x[index])?,y(index)?))).filter(|(_,x,y)|x.is_finite() && y.is_finite()).collect::<Vec<_>>();
        if valid.len()!=end-start{
            if run.len()>1{clipped.add(egui::Shape::line(std::mem::take(&mut run),Stroke::new(1.2_f32,color)));}else{run.clear();}
            for (_,x,y) in valid{let point=screen(rect,bounds,x,y);if point.is_finite(){clipped.circle_filled(point,1.,color);}}continue;
        }
        let low=valid.iter().min_by(|a,b|a.2.total_cmp(&b.2)).unwrap().0;let high=valid.iter().max_by(|a,b|a.2.total_cmp(&b.2)).unwrap().0;
        let mut indices=vec![start,low,high,end-1];indices.sort_unstable();indices.dedup();
        for index in indices{if let (Some(x),Some(y))=(map_x(x[index]),y(index)){let point=screen(rect,bounds,x,y);if point.is_finite(){run.push(point);}}}
    }
    if run.len()>1{clipped.add(egui::Shape::line(run,Stroke::new(1.2_f32,color)));}
}

pub(crate) fn axis_padding(low:f64,high:f64)->f64{
    let span=(high-low).abs();let scale=low.abs().max(high.abs());
    if span>0.{(span*0.05).max(scale*f64::EPSILON*8.)}else if scale>0.{scale*0.05}else{1.}
}
