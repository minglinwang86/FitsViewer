//! Explicit-file capabilities for the standalone fver control plane.
//! 控制凭据、一次性引导票据、查看凭据与助手凭据分离，不授权文件所在目录。
use super::AppState;
use axum::{
    extract::{Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs::File,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};
type Error = (StatusCode, String);
fn forbidden(message: &str) -> Error {
    (StatusCode::FORBIDDEN, message.into())
}
fn secret(prefix: &str) -> String {
    format!(
        "{prefix}{}{}",
        uuid::Uuid::new_v4().simple(),
        uuid::Uuid::new_v4().simple()
    )
}
fn id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}
const SESSION_IDLE: Duration = Duration::from_secs(1800);
const SESSION_MAX: Duration = Duration::from_secs(8 * 3600);
const REQUEST_TTL: Duration = Duration::from_secs(600);
pub fn lifecycle_only(operation: &str) -> bool {
    [
        "job_cancel",
        "release_view",
        "release_document",
        "release_pro",
        "gaia_release",
        "artifact_refs",
        "view_heartbeat",
        "pro_heartbeat",
        "gaia_heartbeat",
        "pro_acquire",
        "gaia_acquire",
    ]
    .contains(&operation)
}

struct Grant {
    path: PathBuf,
    file: File,
    length: u64,
    modified: Option<SystemTime>,
    created: Instant,
    source: Mutex<Option<Arc<super::remote_sources::Source>>>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(windows)]
    identity: (u32, u32, u32),
}
#[cfg(windows)]
pub(super) fn file_identity(file: &File) -> Result<(u32, u32, u32), String> {
    use std::os::windows::io::AsRawHandle;
    #[repr(C)]
    struct Information {
        attributes: u32,
        created: [u32; 2],
        accessed: [u32; 2],
        written: [u32; 2],
        volume: u32,
        size_high: u32,
        size_low: u32,
        links: u32,
        index_high: u32,
        index_low: u32,
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(
            handle: *mut std::ffi::c_void,
            information: *mut Information,
        ) -> i32;
    }
    let mut information = std::mem::MaybeUninit::<Information>::uninit();
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), information.as_mut_ptr()) } == 0 {
        return Err("Cannot inspect the selected Windows file identity".into());
    }
    let information = unsafe { information.assume_init() };
    Ok((
        information.volume,
        information.index_high,
        information.index_low,
    ))
}
impl Grant {
    fn new(path: &str) -> Result<Self, String> {
        let path = Path::new(path)
            .canonicalize()
            .map_err(|_| "The explicitly selected file is unavailable")?;
        let file = File::open(&path).map_err(|_| "Cannot open the explicitly selected file")?;
        let meta = file
            .metadata()
            .map_err(|_| "Cannot inspect selected file")?;
        if !meta.is_file() {
            return Err("Select a regular file".into());
        }
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        #[cfg(windows)]
        let identity = file_identity(&file)?;
        Ok(Self {
            path,
            file,
            length: meta.len(),
            modified: meta.modified().ok(),
            created: Instant::now(),
            source: Mutex::new(None),
            #[cfg(unix)]
            device: meta.dev(),
            #[cfg(unix)]
            inode: meta.ino(),
            #[cfg(windows)]
            identity,
        })
    }
    fn check(&self) -> Result<PathBuf, Error> {
        let meta = std::fs::metadata(&self.path)
            .map_err(|_| forbidden("Authorized source is unavailable; run fver again"))?;
        let held = self
            .file
            .metadata()
            .map_err(|_| forbidden("Authorized source handle is unavailable"))?;
        if !meta.is_file()
            || meta.len() != self.length
            || meta.modified().ok() != self.modified
            || held.len() != self.length
            || held.modified().ok() != self.modified
        {
            return Err((
                StatusCode::CONFLICT,
                "source_changed: run fver again to authorize the new file".into(),
            ));
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if meta.dev() != self.device || meta.ino() != self.inode {
                return Err((
                    StatusCode::CONFLICT,
                    "source_changed: file identity changed".into(),
                ));
            }
        }
        #[cfg(windows)]
        {
            let current =
                File::open(&self.path).map_err(|_| forbidden("Selected file is unavailable"))?;
            if file_identity(&current).ok() != Some(self.identity) {
                return Err((
                    StatusCode::CONFLICT,
                    "source_changed: Windows file identity changed".into(),
                ));
            }
        }
        Ok(self.path.clone())
    }
}
struct Ticket {
    channel: String,
    request: String,
    expires: Instant,
    pair: bool,
    fixed: bool,
    artifact: Option<String>,
}
#[derive(Clone)]
pub struct ReadLease {
    grant: Arc<Grant>,
    store: Arc<super::remote_sources::Store>,
}
impl ReadLease {
    pub fn prepare(&self) -> Result<Arc<super::remote_sources::Source>, String> {
        self.grant.check().map_err(|e| e.1)?;
        let mut slot = self
            .grant
            .source
            .lock()
            .map_err(|_| "Private source lease lock failed")?;
        if slot.is_none() {
            *slot = Some(self.store.capture(&self.grant.path, &self.grant.file)?);
        }
        let source = slot.as_ref().unwrap().clone();
        self.grant.check().map_err(|e| e.1)?;
        source.verify()?;
        Ok(source)
    }
}
struct Scope {
    channel: String,
    request: Option<String>,
    attempt: Option<u64>,
    only_artifact: Option<String>,
    created: Instant,
    touched: Instant,
    owners: HashMap<String, String>,
    documents: HashMap<(String, String), u64>,
    jobs: HashSet<String>,
    artifacts: HashSet<String>,
    job_sources: HashMap<String, Vec<ReadLease>>,
    job_lifecycle: HashMap<String, Value>,
}
pub struct Release {
    pub owner: String,
    pub generation: Option<u64>,
}
struct Helper {
    token: String,
    touched: Instant,
    mode: String,
}
struct Channel {
    grants: VecDeque<Arc<Grant>>,
    requests: VecDeque<Intent>,
    browser: Option<String>,
    helper: Option<Helper>,
    touched: Instant,
    sequence: u64,
}
struct Intent {
    id: String,
    epoch: u64,
    seq: u64,
    path: String,
    hdu: Option<u64>,
    route: String,
    state: String,
    created: Instant,
    updated: Instant,
}
impl Intent {
    fn value(&self) -> Value {
        json!({"request_id":self.id,"attempt_epoch":self.epoch,"seq":self.seq,"path":self.path,"hdu":self.hdu,"route":self.route,"state":self.state})
    }
}
pub struct Broker {
    pub enabled: bool,
    instance: String,
    channels: HashMap<String, Channel>,
    tickets: HashMap<String, Ticket>,
    scopes: HashMap<String, Scope>,
    released: Vec<Release>,
    authorities: HashMap<String, Instant>,
    sources: Arc<super::remote_sources::Store>,
}
impl Broker {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled,
            instance: id(),
            channels: HashMap::new(),
            tickets: HashMap::new(),
            scopes: HashMap::new(),
            released: Vec::new(),
            authorities: HashMap::new(),
            sources: Arc::new(super::remote_sources::Store::new(
                super::runtime_path()
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join("file-capabilities"),
            )),
        }
    }
    fn clean(&mut self) {
        let now = Instant::now();
        self.tickets.retain(|_, t| t.expires > now);
        self.authorities.retain(|_, expires| *expires > now);
        self.channels
            .retain(|_, c| now.duration_since(c.touched) < SESSION_MAX);
        for channel in self.channels.values_mut() {
            for request in &mut channel.requests {
                if now.duration_since(request.created) > REQUEST_TTL
                    && ["queued", "accepted", "visible", "unknown"]
                        .contains(&request.state.as_str())
                {
                    request.state = "expired".into();
                    request.epoch += 1;
                }
                if request.route == "helper"
                    && request.state == "queued"
                    && now.duration_since(request.created) > Duration::from_secs(3)
                {
                    request.route = "browser".into();
                    request.epoch += 1;
                }
                if ["accepted", "visible"].contains(&request.state.as_str())
                    && now.duration_since(request.updated) > Duration::from_secs(30)
                {
                    request.state = "unknown".into();
                }
            }
            channel.grants.retain(|g| {
                g.created.elapsed() < SESSION_MAX
                    && channel.requests.iter().any(|r| {
                        Path::new(&r.path) == g.path
                            && !["cancelled", "closed", "expired", "superseded", "failed"]
                                .contains(&r.state.as_str())
                    })
            });
        }
        self.scopes.retain(|_, s| {
            let live = self.channels.get(&s.channel).is_some_and(|c| {
                s.request.as_ref().is_none_or(|id| {
                    c.requests.iter().any(|r| {
                        &r.id == id
                            && r.created.elapsed() < SESSION_MAX
                            && s.attempt == Some(r.epoch)
                            && !["cancelled", "closed", "expired", "superseded", "failed"]
                                .contains(&r.state.as_str())
                    })
                })
            });
            let keep = live
                && now.duration_since(s.touched) < SESSION_IDLE
                && now.duration_since(s.created) < SESSION_MAX;
            if !keep {
                self.released
                    .extend(s.owners.values().cloned().map(|owner| Release {
                        owner,
                        generation: None,
                    }));
            }
            keep
        });
        for scope in self.scopes.values_mut() {
            scope.documents.retain(|(_, request), _| {
                self.channels[&scope.channel]
                    .requests
                    .iter()
                    .any(|r| &r.id == request)
            });
        }
    }
    fn channel(&mut self, name: &str) -> Result<&mut Channel, Error> {
        if name.is_empty()
            || name.len() > 64
            || !name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(forbidden(
                "Target must be a short explicit alphanumeric name",
            ));
        }
        if !self.channels.contains_key(name) && self.channels.len() >= 16 {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                "At most 16 active fver targets".into(),
            ));
        }
        Ok(self.channels.entry(name.into()).or_insert_with(|| Channel {
            grants: VecDeque::new(),
            requests: VecDeque::new(),
            browser: None,
            helper: None,
            touched: Instant::now(),
            sequence: 0,
        }))
    }
    pub fn scope(&mut self, token: &str) -> Option<String> {
        self.clean();
        let scope = self.scopes.get_mut(token)?;
        if let Some(request) = &scope.request {
            let intent = self
                .channels
                .get(&scope.channel)?
                .requests
                .iter()
                .find(|r| &r.id == request)?;
            if scope.attempt != Some(intent.epoch)
                || ["cancelled", "expired", "superseded", "closed"].contains(&intent.state.as_str())
            {
                return None;
            }
        }
        scope.touched = Instant::now();
        self.channels.get_mut(&scope.channel)?.touched = Instant::now();
        Some(token.into())
    }
    pub fn resolve(&mut self, token: &str, path: &str) -> Result<PathBuf, Error> {
        self.resolve_permission(token, path, true)
    }
    fn resolve_permission(
        &mut self,
        token: &str,
        path: &str,
        read: bool,
    ) -> Result<PathBuf, Error> {
        self.scope(token)
            .ok_or_else(|| forbidden("Viewing capability expired; run fver again"))?;
        let scope = &self.scopes[token];
        if let Some(artifact) = path.strip_prefix("artifact:") {
            if scope.artifacts.contains(artifact) {
                return Ok(PathBuf::from(path));
            }
            return Err(forbidden("Artifact is outside this viewing capability"));
        }
        if scope.only_artifact.is_some() {
            return Err(forbidden(
                "This download ticket authorizes only its one exported artifact",
            ));
        }
        let channel = &self.channels[&scope.channel];
        let requested = Path::new(path);
        let grant = channel.grants.iter().find(|g| g.path == requested);
        if let Some(request) = &scope.request {
            if channel
                .requests
                .iter()
                .find(|r| &r.id == request)
                .is_none_or(|r| Path::new(&r.path) != requested)
            {
                return Err(forbidden(
                    "Native capability is limited to its selected file",
                ));
            }
        }
        if !read
            && channel
                .requests
                .iter()
                .any(|r| Path::new(&r.path) == requested)
        {
            return Ok(requested.into());
        }
        let grant = grant
            .ok_or_else(|| forbidden("This file was not explicitly authorized for this target"))?;
        grant.check()
    }
    pub fn image_lease(&mut self, token: &str, path: &str) -> Result<Option<ReadLease>, Error> {
        self.resolve(token, path)?;
        let scope = &self.scopes[token];
        Ok(self.channels[&scope.channel]
            .grants
            .iter()
            .find(|g| g.path == Path::new(path))
            .map(|grant| ReadLease {
                grant: grant.clone(),
                store: self.sources.clone(),
            }))
    }
    pub fn leases(&mut self, token: &str, params: &Value) -> Vec<ReadLease> {
        let scope = &self.scopes[token];
        let grants = &self.channels[&scope.channel].grants;
        let mut leases = scope
            .job_sources
            .get(params["job_id"].as_str().unwrap_or(""))
            .cloned()
            .unwrap_or_default();
        fn collect(
            value: &Value,
            grants: &VecDeque<Arc<Grant>>,
            store: &Arc<super::remote_sources::Store>,
            leases: &mut Vec<ReadLease>,
        ) {
            match value {
                Value::Object(map) => {
                    for value in map.values() {
                        collect(value, grants, store, leases)
                    }
                }
                Value::Array(values) => {
                    for value in values {
                        collect(value, grants, store, leases)
                    }
                }
                Value::String(path) => {
                    if let Some(grant) = grants.iter().find(|g| g.path == Path::new(path)) {
                        if !leases.iter().any(|l| l.grant.path == grant.path) {
                            leases.push(ReadLease {
                                grant: grant.clone(),
                                store: store.clone(),
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        collect(params, grants, &self.sources, &mut leases);
        leases
    }
    pub fn lifecycle(&self, token: &str, params: &Value) -> Value {
        self.scopes
            .get(token)
            .and_then(|scope| {
                scope
                    .job_lifecycle
                    .get(params["job_id"].as_str().unwrap_or(""))
            })
            .cloned()
            .unwrap_or_else(|| params.clone())
    }
    pub fn logicalize(&self, token: &str, value: &mut Value) {
        if let Some(scope) = self.scopes.get(token) {
            let names = self.channels[&scope.channel]
                .grants
                .iter()
                .filter_map(|g| {
                    g.source.try_lock().ok().and_then(|source| {
                        source.as_ref().map(|s| {
                            (
                                s.path().to_string_lossy().into_owned(),
                                g.path.to_string_lossy().into_owned(),
                            )
                        })
                    })
                })
                .collect::<HashMap<_, _>>();
            fn walk(value: &mut Value, names: &HashMap<String, String>) {
                match value {
                    Value::String(path) => {
                        if let Some(logical) = names.get(path) {
                            *path = logical.clone();
                        }
                    }
                    Value::Object(map) => {
                        for value in map.values_mut() {
                            walk(value, names)
                        }
                    }
                    Value::Array(values) => {
                        for value in values {
                            walk(value, names)
                        }
                    }
                    _ => {}
                }
            }
            walk(value, &names);
        }
    }
    pub fn source_status(&self) -> Value {
        self.sources.status()
    }
    pub fn refresh(&mut self, token: &str, bridge: &super::science::Bridge) {
        if let Some(scope) = self.scopes.get_mut(token) {
            scope.artifacts.retain(|id| {
                scope
                    .owners
                    .values()
                    .any(|owner| bridge.artifact_owned(id, owner))
            });
        }
    }
    pub fn science(
        &mut self,
        token: &str,
        operation: &str,
        params: &mut Value,
    ) -> Result<(), Error> {
        self.scope(token)
            .ok_or_else(|| forbidden("Viewing capability expired"))?;
        if self.scopes[token].only_artifact.is_some() {
            return Err(forbidden(
                "An artifact download capability cannot run scientific operations",
            ));
        }
        let target = if operation == "job_start" {
            params["operation"].as_str().unwrap_or("")
        } else {
            operation
        };
        if ![
            "measure",
            "header",
            "capabilities",
            "environment_info",
            "dependency_probe",
            "wcs",
            "view_link",
            "sky_point",
            "view_heartbeat",
            "release_view",
            "release_document",
            "artifact_refs",
            "pro_acquire",
            "pro_heartbeat",
            "release_pro",
            "gaia_acquire",
            "gaia_heartbeat",
            "gaia_release",
            "job_start",
            "job_status",
            "job_cancel",
            "detect_stars",
            "detect_lines",
            "trace_curve",
            "line_profile",
            "catalogue",
            "xmatch",
            "reproject",
            "rgb",
            "export",
            "math_preview",
            "region_transform",
            "image_flux_match",
            "spectral_analysis",
            "spectral_candidates",
            "gaia_solve",
            "service_presets",
            "archive_search",
            "identify_stars",
            "xmatch_cds",
        ]
        .contains(&target)
        {
            // Scientific/network jobs can disclose data or change global state. A
            // file-only capability does not silently authorize those extra powers.
            // 仅文件能力不隐式授予外发坐标、下载新文件或修改全局环境的权限。
            return Err(forbidden("This file-only remote session does not authorize that operation; use an explicitly trusted full session"));
        }
        if ["job_status", "job_cancel"].contains(&operation)
            && !self.scopes[token]
                .jobs
                .contains(params["job_id"].as_str().unwrap_or(""))
        {
            return Err(forbidden("Task does not belong to this viewing capability"));
        }
        fn paths(
            broker: &mut Broker,
            token: &str,
            value: &Value,
            key: &str,
            read: bool,
        ) -> Result<(), Error> {
            match value {
                Value::Object(map) => {
                    for (key, value) in map {
                        if key == "output" {
                            return Err(forbidden(
                                "Remote exports stay in the private artifact cache",
                            ));
                        }
                        paths(broker, token, value, key, read)?;
                    }
                }
                Value::Array(values) => {
                    for value in values {
                        paths(broker, token, value, key, read)?;
                    }
                }
                Value::String(path)
                    if !path.is_empty()
                        && (key.ends_with("path")
                            || ["catalogue", "paths", "preserve_paths"].contains(&key)
                            || path.starts_with("artifact:")) =>
                {
                    broker.resolve_permission(token, path, read)?;
                }
                _ => {}
            }
            Ok(())
        }
        paths(self, token, params, "", !lifecycle_only(operation))?;
        if operation == "job_start" && self.scopes[token].jobs.len() >= 64 {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                "This capability has 64 tracked tasks; open a fresh fver link".into(),
            ));
        }
        if ["export", "rgb", "math_preview", "reproject", "gaia_solve"].contains(&target)
            && self.scopes[token].artifacts.len() >= 64
        {
            return Err((StatusCode::TOO_MANY_REQUESTS,"This capability has 64 live artifacts; release unused results before creating more".into()));
        }
        if params.get("uploads").is_some() {
            return Err(forbidden(
                "Remote table uploads require a separate explicit grant",
            ));
        }
        let input = params["_viewer_session"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 256)
            .ok_or_else(|| forbidden("Remote science requests require a viewer identity"))?
            .to_owned();
        let scope = &self.scopes[token];
        let path = params
            .get("path")
            .or_else(|| params.get("params").and_then(|p| p.get("path")))
            .and_then(Value::as_str);
        let request = scope.request.clone().or_else(|| {
            self.channels[&scope.channel]
                .requests
                .iter()
                .rev()
                .find(|r| {
                    path.map_or(
                        ["queued", "accepted", "visible", "first_frame", "unknown"]
                            .contains(&r.state.as_str()),
                        |p| r.path == p,
                    )
                })
                .map(|r| r.id.clone())
        });
        if request.is_none() {
            return Err(forbidden(
                "Remote science must belong to an explicit open request",
            ));
        }
        let scope = self.scopes.get_mut(token).unwrap();
        if !scope.owners.contains_key(&input) && scope.owners.len() >= 16 {
            return Err(forbidden("Too many viewer identities in this capability"));
        }
        let owner = scope
            .owners
            .entry(input)
            .or_insert_with(|| format!("fver-{}", id()))
            .clone();
        let generation = params["_document_generation"]
            .as_u64()
            .ok_or_else(|| forbidden("Remote science requires an explicit document generation"))?;
        if let Some(request) = request {
            let key = (owner.clone(), request);
            if !lifecycle_only(operation)
                && !scope.documents.contains_key(&key)
                && scope.documents.len() >= 128
            {
                return Err((StatusCode::TOO_MANY_REQUESTS,"Remote document tracking is full; close unused requests or open a fresh fver link".into()));
            }
            if scope.documents.contains_key(&key) || scope.documents.len() < 128 {
                scope
                    .documents
                    .entry(key)
                    .and_modify(|old| *old = (*old).max(generation))
                    .or_insert(generation);
            }
        }
        fn owners(value: &mut Value, owner: &str) {
            match value {
                Value::Object(map) => {
                    if map.contains_key("_viewer_session") {
                        map.insert("_viewer_session".into(), json!(owner));
                    }
                    for child in map.values_mut() {
                        owners(child, owner);
                    }
                }
                Value::Array(values) => {
                    for value in values {
                        owners(value, owner);
                    }
                }
                _ => {}
            }
        }
        owners(params, &owner);
        let tags = [
            ("_document_generation", json!(generation)),
            (
                "_role_epoch",
                params.get("_role_epoch").cloned().unwrap_or(json!(0)),
            ),
            (
                "_science_role",
                params
                    .get("_science_role")
                    .cloned()
                    .unwrap_or(json!("view")),
            ),
            (
                "_gaia_generation",
                params
                    .get("_gaia_generation")
                    .cloned()
                    .unwrap_or(Value::Null),
            ),
            ("_file_capability", json!(true)),
        ];
        fn tags_match(value: &mut Value, tags: &[(&str, Value)]) {
            match value {
                Value::Object(map) => {
                    for (key, tag) in tags {
                        if map.contains_key(*key) {
                            map.insert((*key).into(), tag.clone());
                        }
                    }
                    for value in map.values_mut() {
                        tags_match(value, tags);
                    }
                }
                Value::Array(values) => {
                    for value in values {
                        tags_match(value, tags);
                    }
                }
                _ => {}
            }
        }
        tags_match(params, &tags);
        params["_file_capability"] = json!(true);
        if operation == "job_start" {
            let nested = params["params"]
                .as_object_mut()
                .ok_or_else(|| forbidden("Job params must be an object"))?;
            nested.insert("_viewer_session".into(), json!(owner));
            for (key, value) in tags {
                nested.insert(key.into(), value);
            }
        }
        Ok(())
    }
    pub fn record(
        &mut self,
        token: &str,
        value: &Value,
        bridge: &super::science::Bridge,
        starting: bool,
        leases: &[ReadLease],
        lifecycle: &Value,
    ) -> Result<(), String> {
        if let Some(scope) = self.scopes.get_mut(token) {
            scope.artifacts.retain(|id| {
                scope
                    .owners
                    .values()
                    .any(|owner| bridge.artifact_owned(id, owner))
            });
            let job = if starting {
                value["job_id"].as_str()
            } else {
                None
            };
            if job.is_some_and(|id| !scope.jobs.contains(id) && scope.jobs.len() >= 64) {
                return Err("Remote task tracking filled while a job was starting; that new task was cancelled".into());
            }
            let mut found = HashSet::new();
            fn collect(
                value: &Value,
                scope: &Scope,
                bridge: &super::science::Bridge,
                found: &mut HashSet<String>,
            ) {
                match value {
                    Value::Object(map) => {
                        if let Some(id) = map.get("artifact_id").and_then(Value::as_str) {
                            if scope
                                .owners
                                .values()
                                .any(|owner| bridge.artifact_owned(id, owner))
                            {
                                found.insert(id.into());
                            }
                        }
                        for child in map.values() {
                            collect(child, scope, bridge, found);
                        }
                    }
                    Value::Array(values) => {
                        for value in values {
                            collect(value, scope, bridge, found);
                        }
                    }
                    _ => {}
                }
            }
            collect(value, scope, bridge, &mut found);
            if scope.artifacts.union(&found).count() > 64 {
                return Err(
                    "Remote artifact tracking is full; this document's results were released"
                        .into(),
                );
            }
            scope.artifacts.extend(found);
            if let Some(job) = job {
                scope.jobs.insert(job.into());
                scope.job_sources.insert(job.into(), leases.to_vec());
                scope.job_lifecycle.insert(job.into(), lifecycle.clone());
            }
            Ok(())
        } else {
            Err("Remote capability ended before its task completed".into())
        }
    }
    fn scope_for(&mut self, channel: &str, request: Option<String>) -> String {
        let token = secret("fv1_");
        let attempt = request
            .as_ref()
            .and_then(|id| self.channels[channel].requests.iter().find(|r| &r.id == id))
            .map(|r| r.epoch);
        self.scopes.insert(
            token.clone(),
            Scope {
                channel: channel.into(),
                request,
                attempt,
                only_artifact: None,
                created: Instant::now(),
                touched: Instant::now(),
                owners: HashMap::new(),
                documents: HashMap::new(),
                jobs: HashSet::new(),
                artifacts: HashSet::new(),
                job_sources: HashMap::new(),
                job_lifecycle: HashMap::new(),
            },
        );
        token
    }
    fn helper_channel(&mut self, token: &str) -> Option<String> {
        self.clean();
        let (name, channel) = self
            .channels
            .iter_mut()
            .find(|(_, c)| c.helper.as_ref().is_some_and(|h| h.token == token))?;
        channel.touched = Instant::now();
        channel.helper.as_mut()?.touched = Instant::now();
        Some(name.clone())
    }
    fn route_channel(&mut self, token: &str) -> Option<(String, bool)> {
        if let Some(name) = self.helper_channel(token) {
            Some((name, true))
        } else {
            self.scope(token)?;
            let s = &self.scopes[token];
            if s.only_artifact.is_some() {
                return None;
            }
            let helper = s
                .request
                .as_ref()
                .and_then(|id| {
                    self.channels[&s.channel]
                        .requests
                        .iter()
                        .find(|r| &r.id == id)
                })
                .is_some_and(|r| r.route == "helper");
            Some((s.channel.clone(), helper))
        }
    }
    pub fn owner(
        &mut self,
        token: &str,
        input: &str,
        generation: Option<u64>,
    ) -> Result<String, Error> {
        let mut p = json!({"_viewer_session":input,"_document_generation":generation.unwrap_or(0)});
        self.science(token, "artifact_refs", &mut p)?;
        Ok(p["_viewer_session"].as_str().unwrap().into())
    }
    pub fn take_releases(&mut self) -> Vec<Release> {
        self.clean();
        std::mem::take(&mut self.released)
    }
    fn revoke(&mut self, token: &str) {
        if let Some(scope) = self.scopes.remove(token) {
            self.released
                .extend(scope.owners.into_values().map(|owner| Release {
                    owner,
                    generation: None,
                }));
        }
    }
    pub fn instance(&self) -> &str {
        &self.instance
    }
}
pub fn credential(headers: &HeaderMap) -> Option<&str> {
    headers
        .get("x-fitsviewer-token")
        .and_then(|v| v.to_str().ok())
}
fn transport(state: &AppState, headers: &HeaderMap) -> Result<(), Error> {
    if !state.broker.lock().unwrap().enabled {
        return Err(forbidden("Standalone fver broker is not enabled"));
    }
    let host = headers
        .get("host")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| forbidden("Missing Host"))?;
    let public = state
        .runtime
        .public_url
        .split("://")
        .nth(1)
        .and_then(|v| v.split('/').next())
        .unwrap_or("");
    if ![
        format!("127.0.0.1:{}", state.runtime.port),
        format!("localhost:{}", state.runtime.port),
        public.to_owned(),
    ]
    .contains(&host.to_owned())
        && !state
            .broker
            .lock()
            .unwrap()
            .authorities
            .get(host)
            .is_some_and(|expires| *expires > Instant::now())
    {
        return Err(forbidden("Unexpected standalone viewer Host"));
    }
    if let Some(origin) = headers.get("origin").and_then(|v| v.to_str().ok()) {
        if origin != format!("http://{host}") && origin != format!("https://{host}") {
            return Err(forbidden("Cross-origin broker requests are forbidden"));
        }
    }
    if headers
        .get("sec-fetch-site")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| !["same-origin", "none"].contains(&v))
    {
        return Err(forbidden("Cross-site broker request rejected"));
    }
    Ok(())
}
fn loopback_authority(host: &str) -> bool {
    ["127.0.0.1:", "localhost:", "[::1]:"].iter().any(|prefix| {
        host.strip_prefix(prefix)
            .is_some_and(|port| port.parse::<u16>().is_ok_and(|p| p > 0))
    })
}
fn bootstrap_transport(state: &AppState, headers: &HeaderMap, input: &Value) -> Result<(), Error> {
    if let Err(error) = transport(state, headers) {
        // A fresh helper may own a different local -L port. Only its live one-use
        // pairing ticket may register that exact loopback authority; browser
        // Origin/Fetch-Metadata requests cannot expand the Host allowlist.
        // 助手可使用不同本机转发端口；仅有效配对票据能登记该精确 Host。
        let host = headers
            .get("host")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        let broker = state.broker.lock().unwrap();
        let pair = input["ticket"]
            .as_str()
            .and_then(|key| broker.tickets.get(key))
            .is_some_and(|t| t.pair && t.expires > Instant::now());
        if !broker.enabled
            || headers.contains_key("origin")
            || headers.contains_key("sec-fetch-site")
            || !loopback_authority(host)
            || !pair
        {
            return Err(error);
        }
    }
    Ok(())
}
fn control(state: &AppState, headers: &HeaderMap) -> Result<(), Error> {
    transport(state, headers)?;
    if headers.contains_key("origin")
        || credential(headers) != Some(state.runtime.token.as_str())
        || state.runtime.token.is_empty()
    {
        return Err(forbidden("Private local control credential required"));
    }
    Ok(())
}
fn reply(value: Result<Value, Error>) -> Response {
    let mut response = match value {
        Ok(value) => Json(value).into_response(),
        Err(error) => error.into_response(),
    };
    response
        .headers_mut()
        .insert("cache-control", "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("referrer-policy", "no-referrer".parse().unwrap());
    response
}
pub async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    reply((|| {
        control(&state, &headers)?;
        let grant = Grant::new(
            input["path"]
                .as_str()
                .ok_or_else(|| forbidden("Select a file"))?,
        )
        .map_err(|e| (StatusCode::BAD_REQUEST, e))?;
        let name = input["target"].as_str().unwrap_or("default");
        let route = input["route"].as_str().unwrap_or("auto");
        if !["auto", "browser", "native"].contains(&route) {
            return Err(forbidden("Unknown route"));
        }
        let mut broker = state.broker.lock().unwrap();
        broker.clean();
        if broker.tickets.len() >= 256 {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                "Too many bootstrap tickets".into(),
            ));
        }
        let channel = broker.channel(name)?;
        channel.touched = Instant::now();
        channel.grants.retain(|g| g.path != grant.path);
        let path = grant.path.to_string_lossy().into_owned();
        channel.grants.push_back(Arc::new(grant));
        while channel.grants.len() > 32 {
            channel.grants.pop_front();
        }
        for request in &mut channel.requests {
            if request.state == "queued" {
                request.state = "superseded".into();
                request.epoch += 1;
            }
        }
        let helper = channel
            .helper
            .as_ref()
            .is_some_and(|h| h.touched.elapsed() < Duration::from_secs(10));
        let routed = if route != "browser" && helper {
            "helper"
        } else {
            "browser"
        };
        channel.sequence += 1;
        let request = Intent {
            id: id(),
            epoch: 1,
            seq: channel.sequence,
            path,
            hdu: input["hdu"].as_u64(),
            route: routed.into(),
            state: "queued".into(),
            created: Instant::now(),
            updated: Instant::now(),
        };
        let intent = request.value();
        let request_id = request.id.clone();
        channel.requests.push_back(request);
        while channel.requests.len() > 64 {
            channel.requests.pop_front();
        }
        let ticket = secret("ot_");
        broker.tickets.insert(
            ticket.clone(),
            Ticket {
                channel: name.into(),
                request: request_id,
                expires: Instant::now() + Duration::from_secs(60),
                pair: false,
                fixed: false,
                artifact: None,
            },
        );
        Ok(
            json!({"instance":broker.instance,"request":intent,"target":name,"helper_online":helper,"url":format!("{}#open={ticket}",state.runtime.public_url),"ticket_expires_in":60}),
        )
    })())
}
pub async fn pair_ticket(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    reply((|| {
        control(&state, &headers)?;
        let mut broker = state.broker.lock().unwrap();
        broker.clean();
        let name = input["target"].as_str().unwrap_or("default");
        broker.channel(name)?;
        let ticket = secret("pair_");
        if broker.tickets.len() >= 256 {
            return Err(forbidden("Too many outstanding tickets"));
        }
        broker.tickets.insert(
            ticket.clone(),
            Ticket {
                channel: name.into(),
                request: String::new(),
                expires: Instant::now() + Duration::from_secs(120),
                pair: true,
                fixed: false,
                artifact: None,
            },
        );
        Ok(json!({"instance":broker.instance,"target":name,"pair_code":ticket,"expires_in":120}))
    })())
}
pub async fn bootstrap(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    let result = (|| {
        bootstrap_transport(&state, &headers, &input)?;
        let mut broker = state.broker.lock().unwrap();
        broker.clean();
        let key = input["ticket"]
            .as_str()
            .ok_or_else(|| forbidden("One-time ticket required"))?;
        let ticket = broker
            .tickets
            .remove(key)
            .ok_or_else(|| forbidden("Ticket expired or already used; run fver again"))?;
        if ticket.pair {
            if broker.authorities.len() >= 32 {
                return Err(forbidden("Too many paired local forwarding authorities"));
            }
            let mode = input["mode"].as_str().unwrap_or("native");
            if !["native", "browser"].contains(&mode) {
                return Err(forbidden("Helper mode must be native or browser"));
            }
            let token = secret("helper_");
            if let Some(host) = headers.get("host").and_then(|v| v.to_str().ok()) {
                broker
                    .authorities
                    .insert(host.into(), Instant::now() + SESSION_MAX);
            }
            broker
                .channels
                .get_mut(&ticket.channel)
                .ok_or_else(|| forbidden("Target expired"))?
                .helper = Some(Helper {
                token: token.clone(),
                mode: mode.into(),
                touched: Instant::now(),
            });
            return Ok(
                json!({"helper_token":token,"instance":broker.instance,"target":ticket.channel,"mode":mode}),
            );
        }
        let intent = broker
            .channels
            .get(&ticket.channel)
            .and_then(|c| c.requests.iter().find(|r| r.id == ticket.request))
            .ok_or_else(|| forbidden("Open request expired"))?
            .value();
        if ["expired", "cancelled", "superseded", "closed", "failed"]
            .contains(&intent["state"].as_str().unwrap_or(""))
        {
            return Err(forbidden("Open request is no longer active"));
        }
        if broker.scopes.len() >= 128 {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                "Too many active viewing capabilities".into(),
            ));
        }
        if !ticket.fixed {
            if let Some(old) = broker
                .channels
                .get_mut(&ticket.channel)
                .unwrap()
                .browser
                .take()
            {
                broker.revoke(&old);
            }
        }
        let token = broker.scope_for(&ticket.channel, ticket.fixed.then_some(ticket.request));
        if !ticket.fixed {
            broker.channels.get_mut(&ticket.channel).unwrap().browser = Some(token.clone());
        }
        if let Some(artifact) = &ticket.artifact {
            let owner = format!("fver-download-{}", id());
            let scope = broker.scopes.get_mut(&token).unwrap();
            scope.artifacts.insert(artifact.clone());
            scope.owners.insert("download".into(), owner);
            scope.only_artifact = Some(artifact.clone());
        }
        Ok(
            json!({"token":token,"instance":broker.instance,"target":ticket.channel,"request":intent,"fixed":ticket.fixed,"download_artifact":ticket.artifact,"expires_idle_seconds":1800}),
        )
    })();
    let pin = result.as_ref().ok().and_then(|value| {
        let artifact = value["download_artifact"].as_str()?.to_owned();
        let token = value["token"].as_str()?.to_owned();
        let owner = state
            .broker
            .lock()
            .unwrap()
            .scopes
            .get(&token)?
            .owners
            .get("download")?
            .clone();
        Some((artifact, token, owner))
    });
    if let Some((artifact, token, owner)) = pin {
        let bridge = state.science.clone();
        let claim_owner = owner.clone();
        let claim =
            tokio::task::spawn_blocking(move || bridge.claim(&artifact, &claim_owner, None)).await;
        let live = state.broker.lock().unwrap().scope(&token).is_some();
        if !matches!(claim, Ok(Ok(()))) || !live {
            state.broker.lock().unwrap().revoke(&token);
            let bridge = state.science.clone();
            let _ = tokio::task::spawn_blocking(move || {
                bridge.call("release_view", json!({"_viewer_session":owner}))
            })
            .await;
            return reply(Err((
                StatusCode::CONFLICT,
                "Export lease ended while preparing the download; export it again".into(),
            )));
        }
    }
    reply(result)
}
pub async fn events(State(state): State<AppState>, headers: HeaderMap) -> Response {
    reply((|| {
        transport(&state, &headers)?;
        let token =
            credential(&headers).ok_or_else(|| forbidden("Viewer/helper credential required"))?;
        let mut broker = state.broker.lock().unwrap();
        let (channel, helper) = broker
            .route_channel(token)
            .ok_or_else(|| forbidden("Connection expired; pair/open again"))?;
        let fixed = broker.scopes.get(token).and_then(|s| s.request.as_deref());
        let c = &broker.channels[&channel];
        let requests = c
            .requests
            .iter()
            .filter(|r| {
                fixed.is_none_or(|id| r.id == id)
                    && (helper
                        || r.route == "browser"
                        || ["accepted", "visible", "unknown", "first_frame"]
                            .contains(&r.state.as_str()))
            })
            .map(Intent::value)
            .collect::<Vec<_>>();
        Ok(
            json!({"instance":broker.instance,"target":channel,"latest":c.sequence,"requests":requests,"helper_mode":c.helper.as_ref().map(|h|h.mode.clone())}),
        )
    })())
}
pub async fn acknowledge(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    reply((|| {
        transport(&state, &headers)?;
        let token =
            credential(&headers).ok_or_else(|| forbidden("Connection credential required"))?;
        let mut broker = state.broker.lock().unwrap();
        let (channel, helper) = broker
            .route_channel(token)
            .ok_or_else(|| forbidden("Connection expired"))?;
        let request_id = input["request_id"].as_str().unwrap_or("");
        let action = input["action"].as_str().unwrap_or("");
        if broker
            .scopes
            .get(token)
            .and_then(|s| s.request.as_deref())
            .is_some_and(|id| id != request_id || ["claim", "fallback"].contains(&action))
        {
            return Err(forbidden(
                "Native acknowledgement is limited to its own attempt",
            ));
        }
        if helper && action == "claim" && broker.scopes.len() >= 128 {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                "Too many native windows".into(),
            ));
        }
        if helper
            && action == "claim"
            && broker.channels[&channel]
                .requests
                .iter()
                .filter(|r| {
                    r.route == "helper"
                        && ["accepted", "visible", "unknown", "first_frame"]
                            .contains(&r.state.as_str())
                })
                .count()
                >= 8
        {
            return Err((
                StatusCode::TOO_MANY_REQUESTS,
                "This helper already has eight active windows".into(),
            ));
        }
        let request = broker
            .channels
            .get_mut(&channel)
            .unwrap()
            .requests
            .iter_mut()
            .find(|r| r.id == request_id)
            .ok_or_else(|| forbidden("Unknown target request"))?;
        if input["attempt_epoch"].as_u64() != Some(request.epoch) {
            return Err((StatusCode::CONFLICT, "Stale launch attempt".into()));
        }
        let route = if helper { "helper" } else { "browser" };
        if action == "fallback" {
            if helper
                || ["cancelled", "closed", "expired", "superseded", "failed"]
                    .contains(&request.state.as_str())
            {
                return Err(forbidden(
                    "Only an active request can be explicitly taken over in the browser",
                ));
            }
            request.route = "browser".into();
            request.epoch += 1;
            request.state = "queued".into();
        } else {
            if request.route != route {
                return Err(forbidden("Request belongs to another delivery route"));
            }
            match action {
                "claim" if request.state == "queued" => request.state = "accepted".into(),
                "visible"
                    if ["accepted", "visible", "first_frame", "unknown"]
                        .contains(&request.state.as_str()) =>
                {
                    if request.state == "accepted" {
                        request.state = "visible".into();
                    }
                }
                "first_frame"
                    if ["accepted", "visible", "unknown", "first_frame"]
                        .contains(&request.state.as_str()) =>
                {
                    request.state = "first_frame".into()
                }
                "failed"
                    if ["accepted", "visible", "unknown"].contains(&request.state.as_str()) =>
                {
                    request.state = if helper { "queued" } else { "failed" }.into();
                    request.route = "browser".into();
                    request.epoch += 1;
                }
                "close" => request.state = "closed".into(),
                _ => {
                    return Err((
                        StatusCode::CONFLICT,
                        "Request was already claimed or changed state".into(),
                    ))
                }
            }
        }
        request.updated = Instant::now();
        let value = request.value();
        if helper && action == "claim" {
            let browser = broker.channels[&channel]
                .helper
                .as_ref()
                .is_some_and(|h| h.mode == "browser");
            let capability = if browser {
                None
            } else {
                Some(broker.scope_for(&channel, Some(request_id.into())))
            };
            return Ok(
                json!({"request":value,"token":capability,"api":format!("http://127.0.0.1:{}",state.runtime.port),"instance":broker.instance}),
            );
        }
        Ok(json!({"request":value,"instance":broker.instance}))
    })())
}
pub async fn browser_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    reply((|| {
        transport(&state, &headers)?;
        let token =
            credential(&headers).ok_or_else(|| forbidden("Connection credential required"))?;
        let mut broker = state.broker.lock().unwrap();
        let (channel, helper) = broker
            .route_channel(token)
            .ok_or_else(|| forbidden("Connection expired"))?;
        if broker.tickets.len() >= 256 {
            return Err(forbidden("Too many outstanding tickets"));
        }
        let request_id = input["request_id"].as_str().unwrap_or("");
        let fixed = broker.scopes.get(token).and_then(|s| s.request.as_deref());
        if fixed.is_some_and(|id| id != request_id) {
            return Err(forbidden("Window is limited to its own file"));
        }
        let c = broker.channels.get_mut(&channel).unwrap();
        let original = c
            .requests
            .iter()
            .find(|r| r.id == request_id)
            .ok_or_else(|| forbidden("Unknown request"))?;
        if input["attempt_epoch"].as_u64() != Some(original.epoch)
            || !["accepted", "visible", "unknown", "first_frame"].contains(&original.state.as_str())
        {
            return Err((
                StatusCode::CONFLICT,
                "Request changed or has no active window".into(),
            ));
        }
        let id = if helper {
            request_id.into()
        } else {
            let path = original.path.clone();
            let hdu = original.hdu;
            c.sequence += 1;
            let intent = Intent {
                id: id(),
                epoch: 1,
                seq: c.sequence,
                path,
                hdu,
                route: "browser".into(),
                state: "accepted".into(),
                created: Instant::now(),
                updated: Instant::now(),
            };
            let id = intent.id.clone();
            c.requests.push_back(intent);
            while c.requests.len() > 64 {
                c.requests.pop_front();
            }
            id
        };
        let key = secret("ot_");
        broker.tickets.insert(
            key.clone(),
            Ticket {
                channel,
                request: id,
                expires: Instant::now() + Duration::from_secs(60),
                pair: false,
                fixed: true,
                artifact: None,
            },
        );
        Ok(json!({"ticket":key,"instance":broker.instance}))
    })())
}
pub async fn artifact_link(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    reply((|| {
        transport(&state, &headers)?;
        let token = credential(&headers).ok_or_else(|| forbidden("Viewing credential required"))?;
        let mut broker = state.broker.lock().unwrap();
        let artifact = input["id"]
            .as_str()
            .ok_or_else(|| forbidden("Artifact ID required"))?;
        broker.resolve(token, &format!("artifact:{artifact}"))?;
        if broker.tickets.len() >= 256 {
            return Err(forbidden("Too many outstanding tickets"));
        }
        let scope = &broker.scopes[token];
        let channel = scope.channel.clone();
        let request = scope
            .request
            .clone()
            .or_else(|| {
                broker.channels[&channel]
                    .requests
                    .iter()
                    .rev()
                    .find(|r| {
                        ["accepted", "visible", "first_frame", "unknown"]
                            .contains(&r.state.as_str())
                    })
                    .map(|r| r.id.clone())
            })
            .ok_or_else(|| forbidden("No active source request"))?;
        let key = secret("ot_");
        broker.tickets.insert(
            key.clone(),
            Ticket {
                channel,
                request,
                expires: Instant::now() + Duration::from_secs(60),
                pair: false,
                fixed: true,
                artifact: Some(artifact.into()),
            },
        );
        Ok(json!({"ticket":key,"instance":broker.instance}))
    })())
}
pub async fn release_download(State(state): State<AppState>, headers: HeaderMap) -> Response {
    reply((|| {
        transport(&state, &headers)?;
        let token =
            credential(&headers).ok_or_else(|| forbidden("Download credential required"))?;
        let mut broker = state.broker.lock().unwrap();
        broker
            .scope(token)
            .ok_or_else(|| forbidden("Download expired"))?;
        if broker.scopes[token].only_artifact.is_none() {
            return Err(forbidden(
                "Only a download capability can release itself here",
            ));
        }
        broker.revoke(token);
        Ok(json!({"released":true,"instance":broker.instance}))
    })())
}
pub async fn status(
    State(state): State<AppState>,
    headers: HeaderMap,
    Query(input): Query<HashMap<String, String>>,
) -> Response {
    reply((|| {
        control(&state, &headers)?;
        let mut broker = state.broker.lock().unwrap();
        broker.clean();
        let channel = input.get("target").map(String::as_str).unwrap_or("default");
        let request = broker
            .channels
            .get(channel)
            .and_then(|c| {
                c.requests
                    .iter()
                    .find(|r| Some(&r.id) == input.get("request_id"))
            })
            .ok_or_else(|| forbidden("Unknown request"))?;
        Ok(json!({"instance":broker.instance,"request":request.value()}))
    })())
}
pub async fn cancel(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Value>,
) -> Response {
    reply((|| {
        control(&state, &headers)?;
        let mut broker = state.broker.lock().unwrap();
        let channel = input["target"].as_str().unwrap_or("default");
        let request_id = input["request_id"].as_str().unwrap_or("");
        let c = broker
            .channels
            .get_mut(channel)
            .ok_or_else(|| forbidden("Unknown target"))?;
        let request = c
            .requests
            .iter_mut()
            .find(|r| r.id == request_id)
            .ok_or_else(|| forbidden("Unknown request"))?;
        request.state = "cancelled".into();
        request.epoch += 1;
        let path = request.path.clone();
        let value = request.value();
        if !c.requests.iter().any(|r| {
            r.path == path
                && !["cancelled", "closed", "expired", "superseded", "failed"]
                    .contains(&r.state.as_str())
        }) {
            c.grants.retain(|g| g.path != Path::new(&path));
        }
        let mut releases = Vec::new();
        for s in broker.scopes.values_mut().filter(|s| s.channel == channel) {
            if s.request.as_deref() == Some(request_id) {
                releases.extend(s.owners.drain().map(|(_, owner)| Release {
                    owner,
                    generation: None,
                }));
            } else if s.request.is_none() {
                releases.extend(
                    s.documents
                        .iter()
                        .filter(|((_, id), _)| id == request_id)
                        .map(|((owner, _), generation)| Release {
                            owner: owner.clone(),
                            generation: Some(*generation),
                        }),
                );
            }
        }
        broker.released.extend(releases);
        Ok(json!({"request":value}))
    })())
}
pub fn verify_transport(state: &AppState, headers: &HeaderMap) -> Result<(), Error> {
    transport(state, headers)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn capability_cannot_escape_its_file_or_read_another_job() {
        let directory = std::env::temp_dir().join(format!("fver-scope-test-{}", id()));
        std::fs::create_dir(&directory).unwrap();
        let a = directory.join("one.fits");
        let b = directory.join("two.fits");
        std::fs::write(&a, b"test fixture only").unwrap();
        std::fs::write(&b, b"separate").unwrap();
        // register() publishes Grant's canonical spelling, including Windows'
        // extended path prefix; model that same protocol rather than a raw temp path.
        let grant = Arc::new(Grant::new(a.to_str().unwrap()).unwrap());
        let a = grant.path.clone();
        let b = b.canonicalize().unwrap();
        let mut broker = Broker::new(true);
        let channel = broker.channel("one").unwrap();
        channel.grants.push_back(grant);
        channel.requests.push_back(Intent {
            id: id(),
            epoch: 1,
            seq: 1,
            path: a.to_string_lossy().into_owned(),
            hdu: None,
            route: "browser".into(),
            state: "queued".into(),
            created: Instant::now(),
            updated: Instant::now(),
        });
        let token = broker.scope_for("one", None);
        assert!(broker.resolve(&token, a.to_str().unwrap()).is_ok());
        assert!(broker.resolve(&token, b.to_str().unwrap()).is_err());
        assert!(broker
            .science(
                &token,
                "job_status",
                &mut json!({"job_id":"another","_viewer_session":"fake"})
            )
            .is_err());
        let mut params = json!({"_viewer_session":"client","_document_generation":1,"path":a});
        broker.science(&token, "wcs", &mut params).unwrap();
        assert_ne!(params["_viewer_session"], "client");
        std::fs::write(&a, b"replacement changed length").unwrap();
        assert!(broker.resolve(&token, a.to_str().unwrap()).is_err());
        drop(broker);
        std::fs::remove_dir_all(directory).unwrap();
    }
    fn intent(path: &Path, state: &str, route: &str) -> Intent {
        Intent {
            id: id(),
            epoch: 1,
            seq: 1,
            path: path.to_string_lossy().into_owned(),
            hdu: None,
            route: route.into(),
            state: state.into(),
            created: Instant::now(),
            updated: Instant::now(),
        }
    }
    #[test]
    fn nested_file_fields_and_owner_names_cannot_cross_capabilities() {
        let mut broker = Broker::new(true);
        broker.channel("a").unwrap();
        broker.channel("b").unwrap();
        broker
            .channels
            .get_mut("a")
            .unwrap()
            .requests
            .push_back(intent(
                Path::new("/synthetic.fits"),
                "first_frame",
                "browser",
            ));
        broker
            .channels
            .get_mut("b")
            .unwrap()
            .requests
            .push_back(intent(
                Path::new("/synthetic.fits"),
                "first_frame",
                "browser",
            ));
        let a = broker.scope_for("a", None);
        let b = broker.scope_for("b", None);
        let mut pa = json!({"_viewer_session":"client-chosen","_document_generation":1});
        let mut pb = pa.clone();
        broker.science(&a, "wcs", &mut pa).unwrap();
        broker.science(&b, "wcs", &mut pb).unwrap();
        assert_ne!(pa["_viewer_session"], pb["_viewer_session"]);
        for params in [
            json!({"panels":[{"path":"/not-authorized.fits"}]}),
            json!({"path":"","error_path":"/not-authorized.fits"}),
            json!({"params":{"channels":[{"path":"/not-authorized.fits"}]}}),
        ] {
            let mut params = params;
            params["_viewer_session"] = "client-chosen".into();
            assert!(broker.science(&a, "export", &mut params).is_err());
        }
        let mut global = json!({"_viewer_session":"client-chosen","python":"/arbitrary/python"});
        assert!(broker
            .science(&a, "environment_select", &mut global)
            .is_err());
        assert!(broker.resolve(&a, "artifact:other-owner").is_err());
    }
    #[test]
    fn old_native_attempt_is_revoked_and_unaccepted_helper_falls_back() {
        let mut broker = Broker::new(true);
        let request = intent(Path::new("/synthetic.fits"), "accepted", "helper");
        let request_id = request.id.clone();
        broker.channel("a").unwrap().requests.push_back(request);
        let token = broker.scope_for("a", Some(request_id));
        assert!(broker.scope(&token).is_some());
        broker.channels.get_mut("a").unwrap().requests[0].epoch += 1;
        assert!(broker.scope(&token).is_none());
        let request = &mut broker.channels.get_mut("a").unwrap().requests[0];
        request.state = "queued".into();
        request.created = Instant::now() - Duration::from_secs(4);
        broker.clean();
        assert_eq!(broker.channels["a"].requests[0].route, "browser");
    }
    #[test]
    fn accepted_receipt_becomes_unknown_without_blind_relaunch() {
        let mut broker = Broker::new(true);
        let mut request = intent(Path::new("/synthetic.fits"), "accepted", "helper");
        request.updated = Instant::now() - Duration::from_secs(31);
        broker.channel("a").unwrap().requests.push_back(request);
        broker.clean();
        let request = &broker.channels["a"].requests[0];
        assert_eq!(request.state, "unknown");
        assert_eq!(request.route, "helper");
        assert_eq!(request.epoch, 1);
    }
    #[test]
    fn completed_frame_does_not_expire_at_the_delivery_timeout() {
        let mut broker = Broker::new(true);
        let mut request = intent(Path::new("/synthetic.fits"), "first_frame", "helper");
        request.created = Instant::now() - Duration::from_secs(601);
        let id = request.id.clone();
        broker.channel("a").unwrap().requests.push_back(request);
        let token = broker.scope_for("a", Some(id));
        broker.clean();
        assert_eq!(broker.channels["a"].requests[0].state, "first_frame");
        assert!(broker.scope(&token).is_some());
        broker.scopes.get_mut(&token).unwrap().touched =
            Instant::now() - SESSION_IDLE - Duration::from_secs(1);
        assert!(broker.scope(&token).is_none());
    }
    #[test]
    fn tickets_and_target_count_are_bounded() {
        let mut broker = Broker::new(true);
        broker.channel("a").unwrap();
        broker.tickets.insert(
            "expired".into(),
            Ticket {
                channel: "a".into(),
                request: String::new(),
                expires: Instant::now() - Duration::from_secs(1),
                pair: false,
                fixed: false,
                artifact: None,
            },
        );
        broker.clean();
        assert!(broker.tickets.is_empty());
        for number in 0..15 {
            broker.channel(&format!("target-{number}")).unwrap();
        }
        assert!(broker.channel("one-too-many").is_err());
        assert!(broker.channel("../unsafe").is_err());
    }
    #[test]
    fn forwarded_helper_host_is_explicit_loopback_only() {
        for host in ["127.0.0.1:44217", "localhost:8877", "[::1]:22"] {
            assert!(loopback_authority(host));
        }
        for host in [
            "evil.example:8877",
            "127.0.0.1:0",
            "127.0.0.1:80@evil.example",
            "127.0.0.1:80/path",
            "127.0.0.1:65536",
        ] {
            assert!(!loopback_authority(host));
        }
    }
    #[test]
    fn download_capability_cannot_become_a_file_or_execution_capability() {
        let mut broker = Broker::new(true);
        broker.channel("a").unwrap();
        let token = broker.scope_for("a", None);
        let scope = broker.scopes.get_mut(&token).unwrap();
        scope.only_artifact = Some("owned".into());
        scope.artifacts.insert("owned".into());
        assert!(broker.resolve(&token, "artifact:owned").is_ok());
        assert!(broker.resolve(&token, "/not-a-download.fits").is_err());
        assert!(broker.route_channel(&token).is_none());
        assert!(broker
            .science(&token, "wcs", &mut json!({"_viewer_session":"fake"}))
            .is_err());
    }
    #[test]
    fn lifecycle_tags_are_mandatory_and_job_tags_cannot_forge_a_new_epoch() {
        let mut broker = Broker::new(true);
        broker.channel("a").unwrap().requests.push_back(intent(
            Path::new("/synthetic.fits"),
            "first_frame",
            "browser",
        ));
        let token = broker.scope_for("a", None);
        assert!(broker
            .science(&token, "wcs", &mut json!({"_viewer_session":"a"}))
            .is_err());
        let mut params = json!({"_viewer_session":"a","_document_generation":1,"_role_epoch":2,"_science_role":"pro","_file_capability":false,"operation":"export","params":{"_viewer_session":"forged","_document_generation":999,"_role_epoch":999,"_science_role":"gaia","_file_capability":false}});
        broker.science(&token, "job_start", &mut params).unwrap();
        for key in [
            "_viewer_session",
            "_document_generation",
            "_role_epoch",
            "_science_role",
            "_file_capability",
        ] {
            assert_eq!(params["params"][key], params[key]);
        }
        assert_eq!(params["_file_capability"], true);
        for target in ["vo_download", "vo_datalink", "persist_download", "persist_cancel"] {
            assert!(broker.science(&token,target,&mut json!({"_viewer_session":"a","_document_generation":1,"url":"http://127.0.0.1/private"})).is_err());
            assert!(broker.science(&token,"job_start",&mut json!({"_viewer_session":"a","_document_generation":1,"operation":target,"params":{"source_path":"/not-authorized/sibling.fits"}})).is_err());
        }
    }
    #[test]
    fn capacity_never_starts_an_untracked_job_and_release_does_not_need_the_source() {
        let mut broker = Broker::new(true);
        let request = intent(
            Path::new("/already-deleted-source.fits"),
            "first_frame",
            "browser",
        );
        let request_id = request.id.clone();
        broker.channel("a").unwrap().requests.push_back(request);
        let token = broker.scope_for("a", None);
        broker
            .scopes
            .get_mut(&token)
            .unwrap()
            .jobs
            .extend((0..64).map(|n| format!("job-{n}")));
        let mut params = json!({"_viewer_session":"a","_document_generation":1,"operation":"export","params":{}});
        assert_eq!(
            broker
                .science(&token, "job_start", &mut params)
                .unwrap_err()
                .0,
            StatusCode::TOO_MANY_REQUESTS
        );
        broker.scopes.get_mut(&token).unwrap().jobs.clear();
        broker
            .scopes
            .get_mut(&token)
            .unwrap()
            .artifacts
            .extend((0..64).map(|n| format!("artifact-{n}")));
        assert_eq!(
            broker.science(&token, "export", &mut params).unwrap_err().0,
            StatusCode::TOO_MANY_REQUESTS
        );
        broker.scopes.get_mut(&token).unwrap().artifacts.clear();
        broker
            .scopes
            .get_mut(&token)
            .unwrap()
            .documents
            .extend((0..128).map(|n| ((format!("owner-{n}"), request_id.clone()), 1)));
        assert_eq!(
            broker.science(&token, "wcs", &mut params).unwrap_err().0,
            StatusCode::TOO_MANY_REQUESTS
        );
        let mut release = json!({"_viewer_session":"a","_document_generation":1,"preserve_paths":["/already-deleted-source.fits"]});
        assert!(broker
            .science(&token, "release_document", &mut release)
            .is_ok());
        broker
            .scopes
            .get_mut(&token)
            .unwrap()
            .jobs
            .insert("owned".into());
        let mut cancel = json!({"_viewer_session":"a","_document_generation":1,"job_id":"owned","path":"/already-deleted-source.fits"});
        assert!(broker.science(&token, "job_cancel", &mut cancel).is_ok());
    }
}
