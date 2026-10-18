//! A fixed, stdin-only remote window launcher. No shell or local-file command is accepted.
//! 固定原生查看入口；连接能力仅经有界 stdin 传入，不进入命令行或本机文件选择器。
use eframe::{egui, App};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    io::Read,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    time::Duration,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Connection {
    api: String,
    token: String,
    path: String,
    hdu: Option<usize>,
    request_id: String,
    attempt_epoch: u64,
    instance: String,
}
fn valid_api(api: &str) -> bool {
    ["http://127.0.0.1:", "http://localhost:", "http://[::1]:"]
        .iter()
        .any(|prefix| {
            api.strip_prefix(prefix)
                .is_some_and(|port| !port.is_empty() && port.parse::<u16>().is_ok_and(|p| p > 0))
        })
}
struct RemoteApp {
    inner: fitsviewer_web::FitsApp,
    acks: Option<mpsc::SyncSender<&'static str>>,
    visible: bool,
    first_frame: bool,
    failed: bool,
    api: String,
    token: String,
    message: Arc<Mutex<Option<String>>>,
    download_running: Arc<AtomicBool>,
}
impl App for RemoteApp {
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        self.inner.update(ctx, frame);
        let prefix = format!("{}/api/artifact?", self.api);
        let download=ctx.output_mut(|output|output.commands.iter().position(|command|matches!(command,egui::OutputCommand::OpenUrl(url) if url.url.starts_with(&prefix))).and_then(|index|match output.commands.remove(index){egui::OutputCommand::OpenUrl(url)=>Some(url),_=>None}));
        if let Some(download) =
            download.filter(|_| !self.download_running.swap(true, Ordering::AcqRel))
        {
            let api = self.api.clone();
            let token = self.token.clone();
            let message = self.message.clone();
            let ctx = ctx.clone();
            let running = self.download_running.clone();
            std::thread::spawn(move || {
                let result = (|| -> Result<(), String> {
                    let id = download
                        .url
                        .split('?')
                        .nth(1)
                        .and_then(|q| q.split('&').find_map(|part| part.strip_prefix("id=")))
                        .filter(|id| id.len() == 32 && id.bytes().all(|b| b.is_ascii_hexdigit()))
                        .ok_or("Invalid artifact link")?;
                    let response: Value = ureq::post(&format!("{api}/api/fver/artifact-link"))
                        .timeout(Duration::from_secs(5))
                        .set("X-Fitsviewer-Token", &token)
                        .send_json(json!({"id":id}))
                        .map_err(|e| e.to_string())?
                        .into_json()
                        .map_err(|e| e.to_string())?;
                    let ticket = response["ticket"]
                        .as_str()
                        .ok_or("Missing one-time download ticket")?;
                    let url = format!("{api}/#open={ticket}");
                    #[cfg(windows)]
                    let mut command = {
                        let mut c = std::process::Command::new("rundll32.exe");
                        c.arg("url.dll,FileProtocolHandler");
                        use std::os::windows::process::CommandExt;
                        c.creation_flags(0x08000000);
                        c
                    };
                    #[cfg(target_os = "macos")]
                    let mut command = std::process::Command::new("open");
                    #[cfg(all(not(windows), not(target_os = "macos")))]
                    let mut command = std::process::Command::new("xdg-open");
                    let mut child = command
                        .arg(url)
                        .stdin(std::process::Stdio::null())
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .spawn()
                        .map_err(|e| e.to_string())?;
                    let deadline = std::time::Instant::now() + Duration::from_secs(15);
                    loop {
                        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                            return if status.success() {
                                Ok(())
                            } else {
                                Err(format!("Browser launcher exited: {status}"))
                            };
                        }
                        if std::time::Instant::now() >= deadline {
                            let _ = child.kill();
                            let _ = child.wait();
                            return Err(
                                "Browser launcher exceeded 15 seconds and its own child was reaped"
                                    .into(),
                            );
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                })();
                running.store(false, Ordering::Release);
                *message.lock().unwrap() = Some(match result {
                    Ok(()) => {
                        "Export download opened in your browser. / 已在浏览器打开导出下载。".into()
                    }
                    Err(error) => format!("Export download: {error}"),
                });
                ctx.request_repaint();
            });
        }
        let mut message = self.message.lock().unwrap();
        if let Some(text) = message.as_ref() {
            let mut open = true;
            egui::Window::new("Remote download / 远程下载")
                .open(&mut open)
                .show(ctx, |ui| {
                    ui.label(text);
                });
            if !open {
                *message = None;
            }
        }
        if !self.visible {
            self.visible = true;
            if let Some(acks) = &self.acks {
                let _ = acks.try_send("visible");
            }
        }
        if !self.first_frame && self.inner.first_frame_ready() {
            self.first_frame = true;
            if let Some(acks) = &self.acks {
                let _ = acks.try_send("first_frame");
            }
        }
        if !self.failed && !self.first_frame && self.inner.first_frame_error().is_some() {
            self.failed = true;
            if let Some(acks) = &self.acks {
                let _ = acks.try_send("failed");
            }
        }
    }
}
impl Drop for RemoteApp {
    fn drop(&mut self) {
        if let Some(acks) = self.acks.take() {
            let _ = acks.try_send("close");
        }
    }
}
pub fn run(web: Option<&Path>) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    std::io::stdin().take(16_385).read_to_end(&mut bytes)?;
    if bytes.len() > 16_384 {
        return Err("Remote connection input exceeds 16 KiB".into());
    }
    let connection: Connection = serde_json::from_slice(&bytes)?;
    if !valid_api(&connection.api)
        || !connection.token.starts_with("fv1_")
        || connection.token.len() > 128
        || connection.path.len() > 8192
        || connection.path.is_empty()
        || connection.request_id.len() != 32
        || !connection.request_id.bytes().all(|b| b.is_ascii_hexdigit())
        || connection.attempt_epoch == 0
        || connection.instance.len() != 32
    {
        return Err("Invalid paired remote connection".into());
    }
    #[cfg(all(not(windows), not(target_os = "macos")))]
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        return Err("No local display is available for the native helper".into());
    }
    let app = fitsviewer_web::FitsApp::new(
        connection.path.clone(),
        Some(connection.api.clone()),
        connection.token.clone(),
        connection.hdu,
    );
    let api = connection.api.clone();
    let token = connection.token.clone();
    let (sender, receiver) = mpsc::sync_channel(8);
    std::thread::spawn(move || {
        for action in receiver {
            let result=ureq::post(&format!("{}/api/fver/ack",connection.api)).timeout(Duration::from_secs(3)).set("X-Fitsviewer-Token",&connection.token).send_json(json!({"request_id":connection.request_id,"attempt_epoch":connection.attempt_epoch,"action":action}));
            if let Ok(response) = result {
                if response
                    .into_json::<Value>()
                    .ok()
                    .is_some_and(|v| v["instance"] != connection.instance)
                {
                    break;
                }
            }
        }
    });
    let font = std::fs::read(
        web.map(Path::to_path_buf)
            .unwrap_or_else(|| super::project().join("web"))
            .join("fonts/cjk.ttf"),
    )?;
    let viewport = egui::ViewportBuilder::default()
        .with_inner_size([1150.0, 820.0])
        .with_title("FitsViewer — remote")
        .with_app_id("fitsviewer")
        .with_icon(eframe::icon_data::from_png_bytes(include_bytes!(
            "../../../windows/app.png"
        ))?);
    eframe::run_native(
        if cfg!(target_os = "linux") {
            "fitsviewer"
        } else {
            "FitsViewer"
        },
        eframe::NativeOptions {
            viewport,
            ..Default::default()
        },
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            fitsviewer_web::install_cjk_font(&cc.egui_ctx, font);
            Ok(Box::new(RemoteApp {
                inner: app,
                acks: Some(sender),
                visible: false,
                first_frame: false,
                failed: false,
                api,
                token,
                message: Arc::new(Mutex::new(None)),
                download_running: Arc::new(AtomicBool::new(false)),
            }))
        }),
    )?;
    Ok(())
}
#[cfg(test)]
mod tests {
    #[test]
    fn only_explicit_loopback_endpoints() {
        for api in [
            "http://127.0.0.1:8877",
            "http://[::1]:1234",
            "http://localhost:42",
        ] {
            assert!(super::valid_api(api));
        }
        for api in [
            "https://example.org",
            "http://127.0.0.1:80@evil",
            "http://127.0.0.1:80/path",
            "http://localhost:0",
            "http://localhost:22?x",
        ] {
            assert!(!super::valid_api(api));
        }
    }
}
