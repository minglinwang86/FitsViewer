//! One display transform for textures, overlays and original-pixel interaction.
//! 统一显示变换；旋转只作用于绘制，不复制、重采样或改变科研像素。
use eframe::egui::{self, Color32, Pos2, Rect, TextureId, Vec2};

#[derive(Clone, Copy, Debug)]
pub struct ImageTransform {
    pub center: Pos2,
    pub dimensions: Vec2,
    pub zoom: f32,
    /// Clockwise on screen, in degrees. / 屏幕顺时针角度。
    pub rotation_deg: f32,
}
impl ImageTransform {
    pub fn new(center: Pos2, dimensions: Vec2, zoom: f32, rotation_deg: f32) -> Self {
        Self { center, dimensions, zoom, rotation_deg }
    }
    pub fn from_rect(rect: Rect, zoom: f32) -> Self {
        Self::new(rect.center(), rect.size() / zoom, zoom, 0.)
    }
    pub fn rotate_vector(self, vector: Vec2) -> Vec2 {
        let (s, c) = self.rotation_deg.to_radians().sin_cos();
        egui::vec2(c * vector.x - s * vector.y, s * vector.x + c * vector.y)
    }
    pub fn pixel_to_screen(self, point: [f64; 2]) -> Pos2 {
        self.center + self.rotate_vector(egui::vec2(
            point[0] as f32 + 0.5 - self.dimensions.x / 2.,
            self.dimensions.y / 2. - point[1] as f32 - 0.5,
        ) * self.zoom)
    }
    pub fn screen_to_pixel(self, point: Pos2) -> [f64; 2] {
        let vector = Self { rotation_deg: -self.rotation_deg, ..self }.rotate_vector(point - self.center) / self.zoom;
        [f64::from(vector.x + self.dimensions.x / 2. - 0.5),
         f64::from(self.dimensions.y / 2. - vector.y - 0.5)]
    }
    pub fn contains(self, point: Pos2) -> bool {
        let [x, y] = self.screen_to_pixel(point);
        x >= -0.5 && y >= -0.5 && x < f64::from(self.dimensions.x) - 0.5 && y < f64::from(self.dimensions.y) - 0.5
    }
    pub fn bounding_size(self) -> Vec2 {
        let (s, c) = self.rotation_deg.to_radians().sin_cos();
        egui::vec2(c.abs() * self.dimensions.x + s.abs() * self.dimensions.y,
                   s.abs() * self.dimensions.x + c.abs() * self.dimensions.y) * self.zoom
    }
    /// Conservative original-pixel edge ROI, including every rotated view corner.
    /// 返回覆盖可见区的原始像素边界；仅供取块/范围，不冒充精确旋转裁剪。
    pub fn visible_roi(self, viewport: Rect) -> [f64; 4] {
        let points = [viewport.left_top(), viewport.right_top(), viewport.right_bottom(), viewport.left_bottom()].map(|p| self.screen_to_pixel(p));
        let min = |axis: usize| points.iter().map(|p| p[axis] + 0.5).fold(f64::INFINITY, f64::min).max(0.);
        let max = |axis: usize, size: f32| points.iter().map(|p| p[axis] + 0.5).fold(f64::NEG_INFINITY, f64::max).min(f64::from(size)).max(0.);
        [min(0).min(f64::from(self.dimensions.x)), min(1).min(f64::from(self.dimensions.y)), max(0, self.dimensions.x), max(1, self.dimensions.y)]
    }
    pub fn paint_texture(self, painter: &egui::Painter, texture: TextureId, color: Color32) {
        self.paint_texture_rect(painter, texture, [0., 0., f64::from(self.dimensions.x), f64::from(self.dimensions.y)], color);
    }
    /// Bounds are original FITS pixel edges, [x0,y0,x1,y1]; texture rows run top down.
    /// 图块边界按原图像素边缘给出，纹理第一行为显示顶部。
    pub fn paint_texture_rect(self, painter: &egui::Painter, texture: TextureId, bounds: [f64; 4], color: Color32) {
        let [x0,y0,x1,y1] = bounds;
        let points = [[x0-0.5,y1-0.5],[x1-0.5,y1-0.5],[x1-0.5,y0-0.5],[x0-0.5,y0-0.5]];
        let mut mesh = egui::Mesh::with_texture(texture);
        for (point, uv) in points.into_iter().zip([Pos2::new(0.,0.),Pos2::new(1.,0.),Pos2::new(1.,1.),Pos2::new(0.,1.)]) {
            mesh.vertices.push(egui::epaint::Vertex { pos: self.pixel_to_screen(point), uv, color });
        }
        mesh.indices = vec![0,1,2,0,2,3];
        painter.add(egui::Shape::mesh(mesh));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn arbitrary_rotation_round_trips_pixel_centers_and_edges() {
        for angle in [0.,90.,-90.,180.,37.,-143.] {
            let transform=ImageTransform::new(egui::pos2(427.,311.),egui::vec2(512.,300.),2.3,angle);
            for point in [[0.,0.],[511.,299.],[255.5,149.5],[-0.5,-0.5],[511.5,299.5]] {
                let actual=transform.screen_to_pixel(transform.pixel_to_screen(point));
                assert!((actual[0]-point[0]).abs()<0.0002 && (actual[1]-point[1]).abs()<0.0002, "{angle}: {point:?} -> {actual:?}");
            }
        }
    }
    #[test]
    fn quarter_turn_roi_uses_original_axes() {
        let transform=ImageTransform::new(egui::pos2(100.,100.),egui::vec2(200.,100.),1.,90.);
        assert_eq!(transform.bounding_size().round(),egui::vec2(100.,200.));
        let roi=transform.visible_roi(Rect::from_center_size(transform.center,egui::vec2(40.,80.)));
        for (actual,expected) in roi.into_iter().zip([60.,30.,140.,70.]) {assert!((actual-expected).abs()<0.0001);}
    }
}
