use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    path::{Component, Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, SystemTime},
};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "FITSView: fitsrs-powered FITS viewer for Linux, JupyterLab and notebooks"
)]
struct Args {
    file: Option<PathBuf>,
    #[arg(long)]
    native: bool,
    #[arg(long)]
    serve: bool,
    #[arg(long, default_value_t = 8765)]
    port: u16,
    #[arg(long)]
    root: Option<PathBuf>,
    #[arg(long)]
    web_dir: Option<PathBuf>,
    #[arg(long)]
    hdu: Option<usize>,
    #[arg(long)]
    jupyter: bool,
    #[arg(long)]
    no_browser: bool,
    #[arg(long)]
    public_url: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Runtime {
    port: u16,
    pid: u32,
    root: String,
    token: String,
    public_url: String,
    epoch: String,
}
#[derive(Clone, Serialize)]
struct OpenRequest {
    seq: u64,
    path: String,
    hdu: Option<usize>,
}
struct Queue {
    seq: u64,
    items: VecDeque<OpenRequest>,
}
#[derive(Clone)]
struct AppState {
    runtime: Runtime,
    roots: Vec<PathBuf>,
    queue: Arc<Mutex<Queue>>,
}
#[derive(Deserialize)]
struct ImageQuery {
    path: String,
    hdu: Option<usize>,
    token: Option<String>,
}
#[derive(Deserialize)]
struct RequestQuery {
    after: Option<u64>,
    token: Option<String>,
}
#[derive(Deserialize, Serialize)]
struct OpenInput {
    path: String,
    hdu: Option<usize>,
}
#[derive(Serialize)]
struct Requests {
    epoch: String,
    latest: u64,
    requests: Vec<OpenRequest>,
}

fn project() -> PathBuf {
    if let Some(home) = std::env::var_os("FITSVIEW_HOME") {
        return PathBuf::from(home);
    }
    // Discover installed or source resources without embedding the build machine's path.
    // 从运行目录查找资源，避免将构建机器的绝对路径写进发行程序。
    if let Ok(executable) = std::env::current_exe() {
        for candidate in executable.ancestors().skip(1) {
            if candidate.join("web/index.html").is_file() {
                return candidate.to_path_buf();
            }
        }
    }
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
fn runtime_path() -> PathBuf {
    if let Some(directory) = std::env::var_os("FITSVIEW_RUNTIME_DIR") {
        return PathBuf::from(directory).join("server.json");
    }
    project().join(".runtime/server.json")
}
fn authorize(
    state: &AppState,
    headers: &HeaderMap,
    token: Option<&str>,
) -> Result<(), (StatusCode, String)> {
    if state.runtime.token.is_empty()
        || token == Some(state.runtime.token.as_str())
        || headers
            .get("x-fitsview-token")
            .and_then(|v| v.to_str().ok())
            == Some(state.runtime.token.as_str())
    {
        Ok(())
    } else {
        Err((
            StatusCode::UNAUTHORIZED,
            "FITSView session token required".into(),
        ))
    }
}
fn resolve(state: &AppState, path: &str) -> Result<PathBuf, (StatusCode, String)> {
    let p = Path::new(path);
    if p.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err((
            StatusCode::FORBIDDEN,
            "Parent-directory traversal is not allowed".into(),
        ));
    }
    let candidate = if p.is_absolute() {
        p.to_path_buf()
    } else {
        Path::new(&state.runtime.root).join(p)
    };
    let resolved = candidate
        .canonicalize()
        .map_err(|e| (StatusCode::NOT_FOUND, e.to_string()))?;
    if !state.roots.iter().any(|root| resolved.starts_with(root)) || !resolved.is_file() {
        return Err((
            StatusCode::FORBIDDEN,
            "File is outside the configured viewer roots".into(),
        ));
    }
    Ok(resolved)
}
async fn image(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<ImageQuery>,
) -> Response {
    if let Err(error) = authorize(&state, &headers, q.token.as_deref()) {
        return error.into_response();
    }
    let path = match resolve(&state, &q.path) {
        Ok(path) => path,
        Err(error) => return error.into_response(),
    };
    let result = tokio::task::spawn_blocking(move || {
        let mut image = fitsview_core::load_file(&path, q.hdu)?;
        image.info.path = q.path;
        image.to_packet()
    })
    .await;
    match result {
        Ok(Ok(bytes)) => (
            [
                ("content-type", "application/octet-stream"),
                ("cache-control", "no-store"),
            ],
            bytes,
        )
            .into_response(),
        Ok(Err(error)) => (StatusCode::UNPROCESSABLE_ENTITY, error).into_response(),
        Err(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response(),
    }
}
async fn requests(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(q): Query<RequestQuery>,
) -> Response {
    if let Err(error) = authorize(&state, &headers, q.token.as_deref()) {
        return error.into_response();
    }
    let queue = state.queue.lock().unwrap();
    Json(Requests {
        epoch: state.runtime.epoch.clone(),
        latest: queue.seq,
        requests: queue
            .items
            .iter()
            .filter(|r| r.seq > q.after.unwrap_or(0))
            .cloned()
            .collect(),
    })
    .into_response()
}
async fn open(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(q): Json<OpenInput>,
) -> Response {
    if let Err(error) = authorize(&state, &headers, None) {
        return error.into_response();
    }
    let resolved = match resolve(&state, &q.path) {
        Ok(path) => path,
        Err(error) => return error.into_response(),
    };
    let virtual_path = if Path::new(&q.path).is_absolute() {
        let root = Path::new(&state.runtime.root);
        Path::new(&q.path)
            .strip_prefix(root)
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|_| {
                for entry in std::fs::read_dir(root).into_iter().flatten().flatten() {
                    if entry.file_type().is_ok_and(|t| t.is_symlink()) {
                        if let Ok(target) = entry.path().canonicalize() {
                            if let Ok(relative) = resolved.strip_prefix(target) {
                                return Path::new(&entry.file_name())
                                    .join(relative)
                                    .to_string_lossy()
                                    .into_owned();
                            }
                        }
                    }
                }
                resolved.to_string_lossy().into_owned()
            })
    } else {
        q.path
    };
    let mut queue = state.queue.lock().unwrap();
    queue.seq += 1;
    let request = OpenRequest {
        seq: queue.seq,
        path: virtual_path.clone(),
        hdu: q.hdu,
    };
    queue.items.push_back(request);
    if queue.items.len() > 32 {
        queue.items.pop_front();
    }
    let mut url = format!(
        "{}?path={}",
        state.runtime.public_url,
        urlencoding::encode(&virtual_path)
    );
    if let Some(hdu) = q.hdu {
        url.push_str(&format!("&hdu={hdu}"));
    }
    if !state.runtime.token.is_empty() {
        url.push_str(&format!("&token={}", state.runtime.token));
    }
    Json(serde_json::json!({"url":url,"path":virtual_path,"seq":queue.seq})).into_response()
}
async fn health() -> Json<serde_json::Value> {
    Json(
        serde_json::json!({"ok":true,"viewer":"fitsview","version":env!("CARGO_PKG_VERSION"),"fits_backend":"fitsrs 0.4.1"}),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.native {
        let path = args.file.ok_or("Pass a FITS file to --native")?;
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return Err("No display is available. Use the default SSH/browser mode, or start an X11/X2Go session before --native.".into());
        }
        let image = fitsview_core::load_file(&path, args.hdu).map_err(std::io::Error::other)?;
        let options = eframe::NativeOptions {
            viewport: eframe::egui::ViewportBuilder::default()
                .with_inner_size([1150.0, 820.0])
                .with_title("FITSView"),
            ..Default::default()
        };
        eframe::run_native(
            "FITSView",
            options,
            Box::new(move |cc| {
                cc.egui_ctx.set_visuals(eframe::egui::Visuals::dark());
                Ok(Box::new(fitsview_web::FitsApp::from_image(image)))
            }),
        )?;
        return Ok(());
    }
    if args.serve {
        return tokio::runtime::Runtime::new()?.block_on(serve(args));
    }
    let path = args
        .file
        .as_ref()
        .ok_or("Pass a FITS file, or use --serve")?
        .canonicalize()?;
    let mut active = std::fs::read(runtime_path())
        .ok()
        .and_then(|b| serde_json::from_slice::<Runtime>(&b).ok())
        .filter(|r| {
            ureq::get(&format!("http://127.0.0.1:{}/api/health", r.port))
                .timeout(Duration::from_secs(1))
                .call()
                .is_ok()
        });
    if active.is_none() {
        // Installed resources may be read-only. / 安装目录可为只读，运行记录放用户缓存。
        let logs = if std::env::var_os("FITSVIEW_RUNTIME_DIR").is_some() {
            runtime_path().parent().unwrap().join("logs")
        } else {
            project().join("logs")
        };
        std::fs::create_dir_all(&logs)?;
        let root = args
            .root
            .clone()
            .unwrap_or_else(|| path.parent().unwrap().to_path_buf());
        let mut cmd = std::process::Command::new(std::env::current_exe()?);
        cmd.arg("--serve")
            .arg("--port")
            .arg(args.port.to_string())
            .arg("--root")
            .arg(root);
        if let Some(url) = &args.public_url {
            cmd.arg("--public-url").arg(url);
        }
        if let Some(web) = &args.web_dir {
            cmd.arg("--web-dir").arg(web);
        }
        cmd.stdin(std::process::Stdio::null())
            .stdout(std::fs::File::create(logs.join("standalone.log"))?)
            .stderr(
                std::fs::OpenOptions::new()
                    .append(true)
                    .open(logs.join("standalone.log"))?,
            );
        let mut child = cmd.spawn()?;
        for _ in 0..100 {
            std::thread::sleep(Duration::from_millis(100));
            if let Some(status) = child.try_wait()? {
                return Err(
                    format!("Viewer server exited: {status}. See logs/standalone.log").into(),
                );
            }
            if let Ok(bytes) = std::fs::read(runtime_path()) {
                if let Ok(r) = serde_json::from_slice::<Runtime>(&bytes) {
                    if r.pid == child.id()
                        && ureq::get(&format!("http://127.0.0.1:{}/api/health", r.port))
                            .timeout(Duration::from_secs(1))
                            .call()
                            .is_ok()
                    {
                        active = Some(r);
                        break;
                    }
                }
            }
        }
    }
    let active = active.ok_or("Viewer server did not become ready")?;
    let reply: serde_json::Value =
        ureq::post(&format!("http://127.0.0.1:{}/api/open", active.port))
            .set("X-Fitsview-Token", &active.token)
            .send_json(serde_json::json!({"path":path,"hdu":args.hdu}))?
            .into_json()?;
    let url = reply["url"].as_str().ok_or("Invalid open response")?;
    println!("{url}");
    if active.token.is_empty() {
        println!("Queued for the connected JupyterLab window. Use its Open in window button for a separate browser window.");
    }
    if !args.no_browser && std::env::var_os("DISPLAY").is_some() {
        let _ = std::process::Command::new("xdg-open")
            .arg(url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    Ok(())
}
async fn serve(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let root = args
        .root
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")))
        .canonicalize()?;
    let mut roots = vec![root.clone()];
    // Top-level links visible in Jupyter are explicit additional read roots.
    // Jupyter 根目录中的顶层符号链接作为明确的附加只读根目录。
    for entry in std::fs::read_dir(&root)?.flatten() {
        if entry.file_type()?.is_symlink() {
            if let Ok(target) = entry.path().canonicalize() {
                roots.push(target);
            }
        }
    }
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", args.port)).await?;
    let port = listener.local_addr()?.port();
    let runtime = Runtime {
        port,
        pid: std::process::id(),
        root: root.to_string_lossy().into_owned(),
        token: if args.jupyter {
            String::new()
        } else {
            uuid::Uuid::new_v4().simple().to_string()
        },
        public_url: args
            .public_url
            .unwrap_or_else(|| format!("http://127.0.0.1:{port}/")),
        epoch: format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)?
                .as_secs()
        ),
    };
    std::fs::create_dir_all(runtime_path().parent().unwrap())?;
    std::fs::write(runtime_path(), serde_json::to_vec_pretty(&runtime)?)?;
    let state = AppState {
        runtime,
        roots,
        queue: Arc::new(Mutex::new(Queue {
            seq: 0,
            items: VecDeque::new(),
        })),
    };
    let web = args.web_dir.unwrap_or_else(|| project().join("web"));
    let app = Router::new()
        .route("/api/health", get(health))
        .route("/api/image", get(image))
        .route("/api/open", post(open))
        .route("/api/requests", get(requests))
        .fallback_service(tower_http::services::ServeDir::new(web))
        .with_state(state);
    println!("FITSView listening on loopback port {port}");
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
