use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use fitsviewer_core::{DisplayTone, FitsImage, Limits, Statistics, Stretch};
use std::sync::{Arc, Mutex};

type Pending = Arc<Mutex<Option<Result<FitsImage, String>>>>;
/// The egui viewer shared by desktop and browser builds.
pub struct FitsApp {
    image: Option<FitsImage>,
    stats: Statistics,
    texture: Option<egui::TextureHandle>,
    path: String,
    api: Option<String>,
    token: String,
    pending: Pending,
    loading: bool,
    error: String,
    limits: Limits,
    stretch: Stretch,
    low: f64,
    high: f64,
    invert: bool,
    tone: DisplayTone,
    dirty: bool,
    zoom: f32,
    pan: Vec2,
    auto_fit: bool,
    header_open: bool,
    cursor: String,
    requested_hdu: Option<usize>,
    #[cfg(windows)]
    file_picker: Option<fn() -> Option<std::path::PathBuf>>,
    #[cfg(windows)]
    started: std::time::Instant,
}
impl FitsApp {
    pub fn new(path: String, api: Option<String>, token: String, hdu: Option<usize>) -> Self {
        Self {
            image: None,
            stats: Statistics::default(),
            texture: None,
            path,
            api,
            token,
            pending: Arc::new(Mutex::new(None)),
            loading: false,
            error: String::new(),
            limits: Limits::ZScale,
            stretch: Stretch::Linear,
            low: 0.0,
            high: 1.0,
            invert: false,
            tone: DisplayTone::default(),
            dirty: false,
            zoom: 1.0,
            pan: Vec2::ZERO,
            auto_fit: true,
            header_open: false,
            cursor: String::new(),
            requested_hdu: hdu,
            #[cfg(windows)]
            file_picker: None,
            #[cfg(windows)]
            started: std::time::Instant::now(),
        }
    }
    pub fn from_image(image: FitsImage) -> Self {
        let mut app = Self::new(
            image.info.path.clone(),
            None,
            String::new(),
            Some(image.info.hdu),
        );
        app.accept(image);
        app
    }
    #[cfg(windows)]
    pub fn with_file_picker(mut self, picker: fn() -> Option<std::path::PathBuf>) -> Self {
        self.file_picker = Some(picker);
        self
    }
    #[cfg(windows)]
    fn browse(&mut self, ctx: &egui::Context) {
        if self.loading {
            return;
        }
        if let Some(path) = self.file_picker.and_then(|picker| picker()) {
            self.path = path.to_string_lossy().into_owned();
            self.request(ctx, None);
        }
    }
    fn accept(&mut self, image: FitsImage) {
        self.stats = Statistics::new(&image);
        (self.low, self.high) = self.stats.bounds(self.limits);
        self.path = image.info.path.clone();
        self.image = Some(image);
        self.dirty = true;
        self.auto_fit = true;
        self.pan = Vec2::ZERO;
        self.error.clear();
        self.cursor.clear();
    }
    fn request(&mut self, ctx: &egui::Context, hdu: Option<usize>) {
        if self.loading || self.path.trim().is_empty() {
            return;
        }
        self.loading = true;
        self.error.clear();
        self.requested_hdu = hdu;
        let pending = self.pending.clone();
        let ctx = ctx.clone();
        if let Some(base) = &self.api {
            let mut url = format!("{base}/api/image?path={}", urlencoding::encode(&self.path));
            if let Some(hdu) = hdu {
                url.push_str(&format!("&hdu={hdu}"));
            }
            let mut request = ehttp::Request::get(url);
            if !self.token.is_empty() {
                request.headers.insert("X-Fitsviewer-Token", &self.token);
            }
            ehttp::fetch(request, move |response| {
                let result = response.map_err(|e| e.to_string()).and_then(|r| {
                    if r.ok {
                        FitsImage::from_packet(&r.bytes)
                    } else {
                        Err(String::from_utf8_lossy(&r.bytes).into_owned())
                    }
                });
                *pending.lock().unwrap() = Some(result);
                ctx.request_repaint();
            });
        } else {
            #[cfg(not(target_arch = "wasm32"))]
            {
                let path = self.path.clone();
                std::thread::spawn(move || {
                    let result = fitsviewer_core::load_file(std::path::Path::new(&path), hdu);
                    *pending.lock().unwrap() = Some(result);
                    ctx.request_repaint();
                });
            }
        }
    }
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        ctx.input(|i| {
            for (key, stretch) in [
                (egui::Key::Num1, Stretch::Linear),
                (egui::Key::Num2, Stretch::Log),
                (egui::Key::Num3, Stretch::Sqrt),
                (egui::Key::Num4, Stretch::Asinh),
            ] {
                if i.key_pressed(key) {
                    self.stretch = stretch;
                    self.dirty = true;
                }
            }
            if i.key_pressed(egui::Key::F) {
                self.auto_fit = true;
                self.pan = Vec2::ZERO;
            }
            if i.key_pressed(egui::Key::Z) {
                self.limits = Limits::ZScale;
                (self.low, self.high) = self.stats.bounds(self.limits);
                self.dirty = true;
            }
            if i.key_pressed(egui::Key::Plus) || i.key_pressed(egui::Key::Equals) {
                self.zoom *= 1.25;
                self.pan *= 1.25;
                self.auto_fit = false;
            }
            if i.key_pressed(egui::Key::Minus) {
                self.zoom /= 1.25;
                self.pan /= 1.25;
                self.auto_fit = false;
            }
            if i.key_pressed(egui::Key::I) {
                self.invert = !self.invert;
                self.dirty = true;
            }
            if i.key_pressed(egui::Key::R) {
                self.tone = DisplayTone::default();
                self.dirty = true;
            }
            if i.key_pressed(egui::Key::PageDown) || i.key_pressed(egui::Key::PageUp) {
                let chosen = self.image.as_ref().and_then(|image| {
                    let valid: Vec<usize> = image
                        .info
                        .hdus
                        .iter()
                        .filter(|h| h.viewable)
                        .map(|h| h.index)
                        .collect();
                    let position = valid.iter().position(|&h| h == image.info.hdu)?;
                    let next = if i.key_pressed(egui::Key::PageDown) {
                        (position + 1) % valid.len()
                    } else {
                        (position + valid.len() - 1) % valid.len()
                    };
                    Some(valid[next])
                });
                if let Some(hdu) = chosen {
                    self.request(ctx, Some(hdu));
                }
            }
        });
    }
    #[cfg(target_arch = "wasm32")]
    fn export_state(&self) {
        let image = self.image.as_ref();
        let state = serde_json::json!({"loaded":image.is_some(),"loading":self.loading,"error":self.error,"path":self.path,"hdu":image.map(|m|m.info.hdu),"width":image.map(|m|m.info.width),"height":image.map(|m|m.info.height),"stretch":self.stretch.name(),"limits":self.limits.name(),"low":self.low,"high":self.high,"bias":self.tone.bias,"contrast":self.tone.contrast,"zoom":self.zoom,"pan":[self.pan.x,self.pan.y],"cursor":self.cursor});
        if let (Some(window), Ok(value)) =
            (web_sys::window(), js_sys::JSON::parse(&state.to_string()))
        {
            let _ = js_sys::Reflect::set(
                &window,
                &wasm_bindgen::JsValue::from_str("__fitsviewerState"),
                &value,
            );
        }
    }
    #[cfg(windows)]
    fn native_diagnostics(&self, ctx: &egui::Context) {
        // Opt-in functional diagnostics for local testing; never sent over the network.
        // 可选的本地状态输出用于功能测试，不发送到网络。
        if let Some(path) = std::env::var_os("FITSVIEWER_DIAGNOSTICS") {
            let state = serde_json::json!({"loaded":self.image.is_some(),"loading":self.loading,
                "texture":self.texture.is_some(),"error":self.error,"path":self.path,
                "width":self.image.as_ref().map(|m|m.info.width),"height":self.image.as_ref().map(|m|m.info.height),
                "stretch":self.stretch.name(),"low":self.low,"high":self.high,"zoom":self.zoom,
                "bias":self.tone.bias,"contrast":self.tone.contrast});
            let _ = std::fs::write(path, state.to_string());
        }
        if let Ok(seconds) = std::env::var("FITSVIEWER_EXIT_AFTER")
            .unwrap_or_default()
            .parse::<f64>()
        {
            if seconds > 0.0 && self.started.elapsed().as_secs_f64() >= seconds {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }
    }
}
impl eframe::App for FitsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !ctx.style().visuals.dark_mode {
            ctx.set_visuals(egui::Visuals::dark());
        }
        let ready = self.pending.lock().unwrap().take();
        if let Some(result) = ready {
            self.loading = false;
            match result {
                Ok(image) => self.accept(image),
                Err(error) => {
                    self.error = error;
                    self.image = None;
                    self.texture = None;
                }
            }
        }
        if self.image.is_none() && !self.loading && self.error.is_empty() && !self.path.is_empty() {
            self.request(ctx, self.requested_hdu);
        }
        self.shortcuts(ctx);
        #[cfg(windows)]
        {
            let dropped = ctx.input(|i| {
                i.raw
                    .dropped_files
                    .iter()
                    .find_map(|file| file.path.clone())
            });
            if let Some(path) = dropped.filter(|_| !self.loading) {
                self.path = path.to_string_lossy().into_owned();
                self.request(ctx, None);
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::O)) {
                self.browse(ctx);
            }
        }
        egui::TopBottomPanel::top("toolbar").show(ctx, |ui| {
            ui.add_space(5.0);
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new("FitsViewer")
                        .size(21.0)
                        .strong()
                        .color(Color32::from_rgb(100, 210, 225)),
                );
                ui.separator();
                #[cfg(windows)]
                if ui.add_enabled(!self.loading, egui::Button::new("Browse…")).clicked() {
                    self.browse(ctx);
                }
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.path)
                        .desired_width((ui.available_width() - if cfg!(windows) { 380.0 } else { 190.0 }).max(100.0))
                        .hint_text("FITS file path"),
                );
                if ui
                    .add_enabled(!self.loading, egui::Button::new("Open"))
                    .clicked()
                    || (response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
                {
                    self.request(ctx, None);
                }
                if ui.button("Header").clicked() {
                    self.header_open = !self.header_open;
                }
                #[cfg(windows)]
                if ui.add_enabled(!self.loading && self.image.is_some(), egui::Button::new("Close image")).clicked() {
                    self.image = None;
                    self.texture = None;
                    self.path.clear();
                    self.stats = Statistics::default();
                    self.cursor.clear();
                    self.error.clear();
                }
                if self.loading {
                    ui.spinner();
                }
            });
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label("HDU");
                let mut chosen = None;
                if let Some(image) = &self.image {
                    egui::ComboBox::from_id_salt("hdu")
                        .selected_text(format!(
                            "{} · {}",
                            image.info.hdu, image.info.hdus[image.info.hdu].name
                        ))
                        .width(145.0)
                        .show_ui(ui, |ui| {
                            for hdu in &image.info.hdus {
                                if ui
                                    .add_enabled(
                                        hdu.viewable,
                                        egui::Button::new(format!("{} · {}", hdu.index, hdu.name)),
                                    )
                                    .clicked()
                                {
                                    chosen = Some(hdu.index);
                                }
                            }
                        });
                }
                if let Some(hdu) = chosen {
                    self.request(ctx, Some(hdu));
                }
                ui.separator();
                ui.label("Limits");
                let old = self.limits;
                egui::ComboBox::from_id_salt("limits")
                    .selected_text(self.limits.name())
                    .width(92.0)
                    .show_ui(ui, |ui| {
                        for v in Limits::ALL {
                            ui.selectable_value(&mut self.limits, v, v.name());
                        }
                    });
                if old != self.limits {
                    (self.low, self.high) = self.stats.bounds(self.limits);
                    self.dirty = true;
                }
                ui.label("Stretch");
                let old = self.stretch;
                egui::ComboBox::from_id_salt("stretch")
                    .selected_text(self.stretch.name())
                    .width(82.0)
                    .show_ui(ui, |ui| {
                        for v in Stretch::ALL {
                            ui.selectable_value(&mut self.stretch, v, v.name());
                        }
                    });
                if old != self.stretch {
                    self.dirty = true;
                }
                let speed = (self.high - self.low).abs().max(0.001) / 300.0;
                ui.label("Low");
                let a = ui
                    .add(
                        egui::DragValue::new(&mut self.low)
                            .speed(speed)
                            .max_decimals(5),
                    )
                    .changed();
                ui.label("High");
                let b = ui
                    .add(
                        egui::DragValue::new(&mut self.high)
                            .speed(speed)
                            .max_decimals(5),
                    )
                    .changed();
                if a || b {
                    self.limits = Limits::Manual;
                    self.dirty = true;
                }
                if ui.checkbox(&mut self.invert, "Invert").changed() {
                    self.dirty = true;
                }
            });
            ui.horizontal_wrapped(|ui| {
                if ui.button("−").clicked() {
                    self.zoom /= 1.25;
                    self.pan /= 1.25;
                    self.auto_fit = false;
                }
                if ui.button("+").clicked() {
                    self.zoom *= 1.25;
                    self.pan *= 1.25;
                    self.auto_fit = false;
                }
                if ui.button("Fit").clicked() {
                    self.auto_fit = true;
                    self.pan = Vec2::ZERO;
                }
                if ui.button("1:1").clicked() {
                    self.zoom = 1.0 / ctx.pixels_per_point();
                    self.pan = Vec2::ZERO;
                    self.auto_fit = false;
                }
                ui.label(format!(
                    "{:.0}%",
                    self.zoom * ctx.pixels_per_point() * 100.0
                ));
                ui.separator();
                ui.label("Bias");
                let bias_changed = ui
                    .add(egui::DragValue::new(&mut self.tone.bias).speed(0.005).range(0.0..=1.0).fixed_decimals(3))
                    .on_hover_text("Brightness offset. Right-drag horizontally: right brighter, left darker.")
                    .changed();
                ui.label("Contrast");
                let contrast_changed = ui
                    .add(egui::DragValue::new(&mut self.tone.contrast).speed(0.01).range(0.05..=20.0).fixed_decimals(3))
                    .on_hover_text("Right-drag vertically: up increases contrast, down decreases it.")
                    .changed();
                if bias_changed || contrast_changed {
                    self.dirty = true;
                }
                if ui.button("Reset B/C").on_hover_text("Reset brightness and contrast (R)").clicked() {
                    self.tone = DisplayTone::default();
                    self.dirty = true;
                }
                ui.separator();
                ui.weak("Wheel: zoom · Left drag: pan · Right drag: B/C · R: reset · 1–4: stretch · Z: zscale · F: fit");
            });
            ui.add_space(4.0);
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                if !self.error.is_empty() {
                    ui.colored_label(Color32::from_rgb(255, 150, 130), &self.error);
                } else if let Some(image) = &self.image {
                    ui.label(format!(
                        "{} × {}  ·  BITPIX {}  ·  {} valid pixels",
                        image.info.width, image.info.height, image.info.bitpix, self.stats.finite
                    ));
                    ui.separator();
                    ui.label(&self.cursor);
                } else {
                    ui.weak("Choose a FITS image to begin.");
                }
            });
        });
        if self.header_open {
            egui::Window::new("FITS header")
                .open(&mut self.header_open)
                .default_size([680.0, 500.0])
                .show(ctx, |ui| {
                    if let Some(image) = &self.image {
                        egui::ScrollArea::both().show(ui, |ui| {
                            ui.monospace(&image.info.header);
                        });
                    }
                });
        }
        if self.dirty && self.low.is_finite() && self.high.is_finite() && self.low < self.high {
            if let Some(image) = &self.image {
                let rgba = fitsviewer_core::render_rgba_with_tone(
                    image,
                    self.low,
                    self.high,
                    self.stretch,
                    self.invert,
                    self.tone,
                );
                let color = egui::ColorImage::from_rgba_unmultiplied(
                    [image.info.width, image.info.height],
                    &rgba,
                );
                if let Some(texture) = &mut self.texture {
                    texture.set(color, egui::TextureOptions::NEAREST);
                } else {
                    self.texture =
                        Some(ctx.load_texture("fits", color, egui::TextureOptions::NEAREST));
                }
            }
            self.dirty = false;
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::from_rgb(13, 17, 23)))
            .show(ctx, |ui| {
                let (response, painter) =
                    ui.allocate_painter(ui.available_size(), egui::Sense::click_and_drag());
                let area = response.rect;
                let center = area.center();
                if let Some(image) = &self.image {
                    let dimensions = Vec2::new(image.info.width as f32, image.info.height as f32);
                    if self.auto_fit {
                        self.zoom = ((area.width() - 24.0) / dimensions.x)
                            .min((area.height() - 24.0) / dimensions.y)
                            .max(0.005);
                    }
                    if response.dragged_by(egui::PointerButton::Primary) {
                        self.pan += response.drag_delta();
                        self.auto_fit = false;
                    }
                    if response.dragged_by(egui::PointerButton::Secondary) {
                        let delta = response.drag_delta();
                        // Relative motion avoids a jump at mouse-down. / 相对位移避免按下右键时跳变。
                        self.tone.bias = (self.tone.bias
                            - f64::from(delta.x / area.width().max(1.0)))
                        .clamp(0.0, 1.0);
                        self.tone.contrast = (self.tone.contrast
                            * f64::from(-4.0 * delta.y / area.height().max(1.0)).exp())
                        .clamp(0.05, 20.0);
                        self.dirty = true;
                        ctx.request_repaint();
                    }
                    if response.hovered() {
                        let delta = ctx.input(|i| i.smooth_scroll_delta.y);
                        if delta != 0.0 {
                            if let Some(pointer) = response.hover_pos() {
                                let next = (self.zoom * (delta / 200.0).exp()).clamp(0.005, 128.0);
                                let local = (pointer - center - self.pan) / self.zoom;
                                self.pan = pointer - center - local * next;
                                self.zoom = next;
                                self.auto_fit = false;
                            }
                        }
                    }
                    if let Some(texture) = &self.texture {
                        let size = dimensions * self.zoom;
                        let rect = Rect::from_center_size(center + self.pan, size);
                        painter.image(
                            texture.id(),
                            rect,
                            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                            Color32::WHITE,
                        );
                        if let Some(pos) = response.hover_pos() {
                            let xy = (pos - rect.min) / self.zoom;
                            let x = xy.x.floor() as isize;
                            let screen_y = xy.y.floor() as isize;
                            if x >= 0
                                && screen_y >= 0
                                && x < image.info.width as isize
                                && screen_y < image.info.height as isize
                            {
                                let y = image.info.height - 1 - screen_y as usize;
                                let i = y * image.info.width + x as usize;
                                self.cursor = format!(
                                    "x={}  y={}  ·  value={}  ·  raw={}",
                                    x + 1,
                                    y + 1,
                                    image
                                        .physical(i)
                                        .map(|v| format!("{v:.8}"))
                                        .unwrap_or_else(|| "invalid".into()),
                                    image.pixels.raw_string(i)
                                );
                            } else {
                                self.cursor.clear();
                            }
                        }
                    }
                } else {
                    painter.text(
                        center,
                        egui::Align2::CENTER_CENTER,
                        if self.loading {
                            "Loading FITS…"
                        } else {
                            "Open a FITS image"
                        },
                        egui::FontId::proportional(23.0),
                        Color32::from_gray(155),
                    );
                }
            });
        #[cfg(target_arch = "wasm32")]
        self.export_state();
        #[cfg(windows)]
        self.native_diagnostics(ctx);
    }
}

#[cfg(target_arch = "wasm32")]
thread_local! {
    // Own the renderer so a closing document can release it explicitly.
    // 保留渲染器所有权，关闭文件页时主动释放图像、纹理和事件回调。
    static WEB_RUNNER: std::cell::RefCell<Option<eframe::WebRunner>> = const { std::cell::RefCell::new(None) };
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub fn stop() {
    WEB_RUNNER.with(|slot| {
        if let Some(runner) = slot.borrow_mut().take() {
            runner.destroy();
        }
    });
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen]
pub async fn start(
    canvas: web_sys::HtmlCanvasElement,
    api: String,
    path: String,
    token: String,
    hdu: Option<usize>,
) -> Result<(), wasm_bindgen::JsValue> {
    stop();
    let runner = eframe::WebRunner::new();
    runner
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(move |cc| {
                cc.egui_ctx.set_visuals(egui::Visuals::dark());
                Ok(Box::new(FitsApp::new(path, Some(api), token, hdu)))
            }),
        )
        .await?;
    WEB_RUNNER.with(|slot| *slot.borrow_mut() = Some(runner));
    Ok(())
}
