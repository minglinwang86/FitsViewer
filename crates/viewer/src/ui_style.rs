//! Shared compact controls for Mini/PRO, images/spectra and docked/floating tools.
//! Mini/PRO、图像/光谱及停靠/浮动工具共用紧凑控件；不修改科学绘图或数据配色。
use eframe::egui::{self, Color32, CornerRadius, Stroke};
use std::sync::Arc;

const CORNER: CornerRadius = CornerRadius::same(3);
const BUTTON_PADDING: egui::Vec2 = egui::vec2(7., 4.);
const ROW_GAP: f32 = 4.;
const TITLE_FONT_SIZE: f32 = 12.;

/// Titles reuse the existing font family, without reducing body or plot text.
/// 标题复用既有字体族；不缩小正文或绘图字号。
pub(crate) fn title_font(style: &egui::Style) -> egui::FontId {
    let mut font = egui::TextStyle::Button.resolve(style);
    font.size = TITLE_FONT_SIZE;
    font
}

pub(crate) fn window_caption(ctx: &egui::Context, title: &str) -> Arc<egui::Galley> {
    let style = ctx.style();
    ctx.fonts(|fonts| fonts.layout_no_wrap(title.into(), title_font(&style), style.visuals.strong_text_color()))
}

/// egui adds the frame's vertical margin to its title row. Keep that margin in
/// the body instead, so the title itself has the same height as a toolbar row.
/// egui 会把 frame 上下边距叠加到标题行；将它们留在正文中，使标题与工具栏等高。
pub(crate) fn compact_window_frame(ctx: &egui::Context) -> egui::Frame {
    let mut frame = egui::Frame::window(&ctx.style());
    frame.inner_margin.top = 0;
    frame.inner_margin.bottom = 0;
    frame
}

pub(crate) fn window_body_margin(ctx: &egui::Context) -> egui::Margin {
    let margin = ctx.style().spacing.window_margin;
    egui::Margin { left: 0, right: 0, top: margin.top, bottom: margin.bottom }
}

/// Resize memory stores the original content size, not the restored body padding.
/// resize 仍保存原内容尺寸；补回正文边距时只扩展 egui 的内部尺寸。
pub(crate) fn padded_window_size(ctx: &egui::Context, size: egui::Vec2) -> egui::Vec2 {
    size + egui::vec2(0., window_body_margin(ctx).sum().y)
}

// A frame derives the child bounds before layout. Adjusting a Resize Ui's cursor
// after its minimum size is set would accumulate padding on every repaint.
// Frame 先计算正文边界；在 Resize 的最小尺寸确定后手动追加边距会逐帧累积。
pub(crate) fn window_body<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    let margin = window_body_margin(ui.ctx());
    egui::Frame::NONE.inner_margin(margin).show(ui, body).inner
}

pub(crate) fn window_title_rect(ctx: &egui::Context, outer: egui::Rect) -> egui::Rect {
    let style = ctx.style();
    let mut rect = outer.shrink(style.visuals.window_stroke.width);
    rect.max.y = rect.min.y + style.spacing.interact_size.y;
    rect
}

fn title_weight_offset(ctx: &egui::Context) -> egui::Vec2 {
    egui::vec2(0.3 / ctx.pixels_per_point(), 0.)
}

pub(crate) fn paint_weighted_title(painter: &egui::Painter, position: egui::Pos2, galley: Arc<egui::Galley>, color: Color32) {
    let offset = title_weight_offset(painter.ctx());
    painter.galley(position - offset, galley.clone(), color);
    painter.galley(position + offset, galley, color);
}

/// Weight only the exact native title galley in this window's title rectangle.
/// Cloning its painted shape preserves native fade opacity, clipping and layer.
/// 只加重当前窗口标题矩形内的原始 galley；复制既有 shape 保留淡入淡出、裁剪与层级。
pub(crate) fn weight_window_title(ctx: &egui::Context, response: &egui::Response, caption: &Arc<egui::Galley>) {
    let row = window_title_rect(ctx, response.rect);
    let inset = row.height() + ctx.style().spacing.item_spacing.x;
    let clip = row.shrink2(egui::vec2(inset, 0.));
    let offset = title_weight_offset(ctx);
    ctx.graphics_mut(|graphics| {
        let Some(list) = graphics.get_mut(response.layer_id) else { return; };
        let source = {
            let mut matches = list.all_entries().enumerate().filter_map(|(index, shape)| {
                let egui::epaint::Shape::Text(text) = &shape.shape else { return None; };
                (Arc::ptr_eq(&text.galley, caption) && row.contains(text.pos + text.galley.rect.center().to_vec2()))
                    .then(|| (index, shape.clip_rect.intersect(clip), text.clone()))
            });
            let first = matches.next();
            if matches.next().is_some() { return; }
            first
        };
        if let Some((index, clip, text)) = source {
            let mut left = text.clone();let mut right = text;
            left.pos -= offset;right.pos += offset;
            list.set(egui::layers::ShapeIdx(index), clip, egui::epaint::Shape::Text(left));
            list.add(clip, egui::epaint::Shape::Text(right));
        }
    });
}

struct Palette {
    panel: Color32,
    window: Color32,
    input: Color32,
    faint: Color32,
    normal: Color32,
    hover: Color32,
    pressed: Color32,
    border: Color32,
    hover_border: Color32,
    accent: Color32,
    text: Color32,
    strong: Color32,
    weak: Color32,
    selection: Color32,
    selection_text: Color32,
    warning: Color32,
    error: Color32,
}

impl Palette {
    fn for_theme(name: &str) -> Self {
        let rgb = Color32::from_rgb;
        match name {
            "Light" => Self {
                panel: rgb(242, 245, 248), window: rgb(252, 253, 255),
                input: rgb(255, 255, 255), faint: rgb(232, 238, 244),
                normal: rgb(235, 240, 245), hover: rgb(219, 231, 240), pressed: rgb(200, 220, 232),
                border: rgb(169, 181, 194), hover_border: rgb(101, 136, 155), accent: rgb(16, 120, 147),
                text: rgb(43, 54, 67), strong: rgb(17, 34, 47), weak: rgb(105, 119, 134),
                selection: rgb(188, 226, 238), selection_text: rgb(13, 77, 100),
                warning: rgb(160, 86, 15), error: rgb(180, 42, 41),
            },
            "Night" => Self {
                // Red-only UI includes selection, cursor, links and status colors.
                // 夜间 UI 的选中、光标、链接及状态色全部使用红色，避免默认蓝色残留。
                panel: rgb(20, 0, 0), window: rgb(25, 0, 0),
                input: rgb(13, 0, 0), faint: rgb(30, 0, 0),
                normal: rgb(44, 0, 0), hover: rgb(67, 0, 0), pressed: rgb(80, 0, 0),
                border: rgb(83, 0, 0), hover_border: rgb(140, 0, 0), accent: rgb(225, 0, 0),
                text: rgb(215, 0, 0), strong: rgb(245, 0, 0), weak: rgb(145, 0, 0),
                selection: rgb(95, 0, 0), selection_text: rgb(255, 0, 0),
                warning: rgb(235, 0, 0), error: rgb(255, 0, 0),
            },
            _ => Self {
                panel: rgb(25, 28, 33), window: rgb(31, 35, 41),
                input: rgb(20, 23, 28), faint: rgb(34, 39, 46),
                normal: rgb(41, 47, 55), hover: rgb(54, 64, 75), pressed: rgb(31, 53, 65),
                border: rgb(65, 74, 86), hover_border: rgb(91, 120, 137), accent: rgb(71, 172, 195),
                text: rgb(208, 215, 224), strong: rgb(239, 245, 250), weak: rgb(145, 157, 172),
                selection: rgb(28, 88, 108), selection_text: rgb(215, 247, 255),
                warning: rgb(236, 175, 84), error: rgb(240, 113, 113),
            },
        }
    }
}

fn widget(fill: Color32, border: Color32, text: Color32) -> egui::style::WidgetVisuals {
    egui::style::WidgetVisuals {
        bg_fill: fill,
        weak_bg_fill: fill,
        bg_stroke: Stroke::new(1., border),
        corner_radius: CORNER,
        fg_stroke: Stroke::new(1., text),
        expansion: 0.,
    }
}

fn visuals(name: &str) -> egui::Visuals {
    let palette = Palette::for_theme(name);
    let mut visuals = if name == "Light" { egui::Visuals::light() } else { egui::Visuals::dark() };
    // Keep state-specific foregrounds: egui's `strong` changes color, not font weight.
    // 保留不同状态的前景色；egui 的 strong 只改变颜色，不改变字重。
    visuals.override_text_color = None;
    visuals.weak_text_color = Some(palette.weak);
    visuals.widgets.noninteractive = widget(palette.window, palette.border, palette.text);
    visuals.widgets.inactive = widget(palette.normal, palette.border, palette.text);
    visuals.widgets.hovered = widget(palette.hover, palette.hover_border, palette.strong);
    visuals.widgets.active = widget(palette.pressed, palette.accent, palette.strong);
    visuals.widgets.open = widget(palette.pressed, palette.accent, palette.strong);
    visuals.selection.bg_fill = palette.selection;
    visuals.selection.stroke = Stroke::new(1., palette.selection_text);
    visuals.hyperlink_color = palette.accent;
    visuals.faint_bg_color = palette.faint;
    visuals.extreme_bg_color = palette.input;
    visuals.text_edit_bg_color = Some(palette.input);
    visuals.code_bg_color = palette.normal;
    visuals.warn_fg_color = palette.warning;
    visuals.error_fg_color = palette.error;
    visuals.text_cursor.stroke = Stroke::new(1., palette.accent);
    visuals.panel_fill = palette.panel;
    visuals.window_fill = palette.window;
    visuals.window_stroke = Stroke::new(1., palette.border);
    visuals.window_corner_radius = CORNER;
    visuals.menu_corner_radius = CORNER;
    visuals.window_shadow = egui::epaint::Shadow {
        offset: [0, 3], blur: 10, spread: 0,
        color: Color32::from_black_alpha(if name == "Light" { 28 } else { 70 }),
    };
    visuals.popup_shadow = egui::epaint::Shadow {
        offset: [0, 2], blur: 7, spread: 0,
        color: Color32::from_black_alpha(if name == "Light" { 24 } else { 64 }),
    };
    visuals.button_frame = true;
    visuals.collapsing_header_frame = true;
    visuals
}

fn control_text_height(ctx: &egui::Context, style: &egui::Style) -> f32 {
    let font = egui::TextStyle::Body.resolve(style);
    ctx.fonts(|fonts| fonts.layout_no_wrap("Ag中文".into(), font.clone(), style.visuals.text_color()).size().y)
        .max(ctx.fonts(|fonts| fonts.row_height(&font))).ceil()
}

fn control_spacing(spacing: &mut egui::Spacing, text_height: f32) {
    spacing.button_padding = BUTTON_PADDING;
    spacing.interact_size.y = text_height + 2. * BUTTON_PADDING.y;
    spacing.icon_width = text_height;
    spacing.item_spacing.y = ROW_GAP;
}

/// Reuse measured glyph height so English and the existing CJK font align alike.
/// 按现有中英文字形测量高度，按钮、下拉箭头与数值框保持居中，不新增字体。
pub(crate) fn align_controls(ui: &mut egui::Ui) {
    let text_height = control_text_height(ui.ctx(), ui.style());
    control_spacing(ui.spacing_mut(), text_height);
}

/// Keep native menu rows frameless while sharing the regular control padding.
/// 保留原生菜单项的无框样式，同时采用普通控件的内边距与行间距。
pub(crate) fn menu_style(style: &mut egui::Style) {
    egui::containers::menu::menu_style(style);
    style.spacing.button_padding = BUTTON_PADDING;
    style.spacing.item_spacing = egui::vec2(6., ROW_GAP);
}

pub(crate) fn apply_theme(ctx: &egui::Context, name: &str) {
    let visuals = visuals(name);
    let text_height = control_text_height(ctx, &ctx.style());
    ctx.set_theme(if name == "Light" { egui::Theme::Light } else { egui::Theme::Dark });
    // Explicit preferences cover both host styles, including a system-theme change.
    // 同步宿主的深浅两套样式，系统主题变化不会带回默认控件样式。
    ctx.all_styles_mut(|style| {
        style.visuals = visuals.clone();
        style.spacing.item_spacing.x = 6.;
        style.spacing.window_margin = egui::Margin::same(6);
        style.spacing.menu_margin = egui::Margin::same(5);
        control_spacing(&mut style.spacing, text_height);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn themes_keep_the_same_control_geometry_and_distinct_states() {
        for name in ["Dark", "Light", "Night"] {
            let visuals = visuals(name);
            for widget in [visuals.widgets.noninteractive, visuals.widgets.inactive,
                visuals.widgets.hovered, visuals.widgets.active, visuals.widgets.open] {
                assert_eq!(widget.corner_radius, CORNER);
                assert_eq!(widget.bg_stroke.width, 1.);
                assert_eq!(widget.expansion, 0.);
            }
            assert!(visuals.button_frame && visuals.collapsing_header_frame);
            assert_ne!(visuals.widgets.inactive.bg_fill, visuals.widgets.hovered.bg_fill);
            assert_ne!(visuals.widgets.hovered.bg_fill, visuals.widgets.active.bg_fill);
            assert_ne!(visuals.selection.bg_fill, visuals.widgets.active.bg_fill);
            assert_eq!(visuals.strong_text_color(), visuals.widgets.active.fg_stroke.color);
        }
    }

    #[test]
    fn night_replaces_default_blue_in_all_shared_ui_states() {
        let visuals = visuals("Night");
        let mut colors = vec![visuals.text_color(), visuals.weak_text_color(),
            visuals.selection.bg_fill, visuals.selection.stroke.color, visuals.hyperlink_color,
            visuals.faint_bg_color, visuals.extreme_bg_color, visuals.text_edit_bg_color(),
            visuals.code_bg_color, visuals.warn_fg_color, visuals.error_fg_color,
            visuals.text_cursor.stroke.color, visuals.panel_fill, visuals.window_fill,
            visuals.window_stroke.color, visuals.window_shadow.color, visuals.popup_shadow.color];
        for widget in [visuals.widgets.noninteractive, visuals.widgets.inactive,
            visuals.widgets.hovered, visuals.widgets.active, visuals.widgets.open] {
            colors.extend([widget.bg_fill, widget.weak_bg_fill, widget.bg_stroke.color, widget.fg_stroke.color]);
        }
        assert!(colors.into_iter().all(|color| color.g() == 0 && color.b() == 0));
    }
}
