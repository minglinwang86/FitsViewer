//! Explicit interpreter selection; scientific packages remain in external environments.
//! 只选择真实外部解释器；不修改基础环境、不嵌入科研运行时。
use serde_json::{json,Value};
use std::{io::Read,path::{Path,PathBuf},process::{Command,Stdio},time::{Duration,Instant}};

pub fn probe(python:&Path)->Result<Value,String>{
    let script="import json,sys,pathlib; p=pathlib.Path(sys.prefix); print(json.dumps({'python':sys.executable,'executable':sys.executable,'prefix':sys.prefix,'environment':p.name,'python_version':sys.version.split()[0]},ensure_ascii=False))";
    let mut command=Command::new(python);
    command.args(["-I","-c",script]).env("PYTHONIOENCODING","utf-8").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let (mut child,mut tree,_owner)=crate::science_process::spawn(command).map_err(|e|format!("Cannot inspect Python interpreter: {e}"))?;
    let stdout=child.stdout.take().ok_or("Python probe stdout unavailable")?;
    let stderr=child.stderr.take().ok_or("Python probe stderr unavailable")?;
    let output=std::thread::spawn(move||{let mut b=Vec::new();let _=stdout.take(16384).read_to_end(&mut b);b});
    let errors=std::thread::spawn(move||{let mut b=Vec::new();let _=stderr.take(4096).read_to_end(&mut b);b});
    let deadline=Instant::now()+Duration::from_secs(8);
    let status=loop{
        match child.try_wait(){Ok(Some(status))=>break Ok(status),Ok(None) if Instant::now()<deadline=>std::thread::sleep(Duration::from_millis(25)),Ok(None)=>break Err("Python inspection timed out".to_string()),Err(e)=>break Err(e.to_string())}
    };
    tree.stop();let _=child.kill();let _=child.wait();
    let bytes=output.join().unwrap_or_default();let error=String::from_utf8_lossy(&errors.join().unwrap_or_default()).to_string();
    let status=status?;if !status.success(){return Err(format!("Selected executable is not a working Python interpreter: {}",error.trim()));}
    let value:Value=serde_json::from_slice(&bytes).map_err(|_|"Python interpreter returned invalid metadata")?;
    if value["executable"].as_str().is_none() || value["prefix"].as_str().is_none(){return Err("Incomplete Python interpreter metadata".into());}
    Ok(value)
}

pub fn candidates(current:&Path)->Value{
    let mut paths=vec![current.to_path_buf()];
    for key in ["FITSVIEWER_PYTHON","CONDA_PREFIX","VIRTUAL_ENV"]{
        if let Some(path)=std::env::var_os(key){let path=PathBuf::from(path);paths.push(if key=="FITSVIEWER_PYTHON"{path}else{interpreter(&path)});}
    }
    if let Some(path)=std::env::var_os("PATH"){
        for root in std::env::split_paths(&path).take(128){for name in if cfg!(windows){vec!["python.exe","python3.exe"]}else{vec!["python3","python"]}{paths.push(root.join(name));}}
    }
    let mut prefixes=vec![PathBuf::from("/opt/conda"),PathBuf::from("/usr"),PathBuf::from("/usr/local")];
    if let Some(home)=std::env::var_os(if cfg!(windows){"USERPROFILE"}else{"HOME"}){
        let user=PathBuf::from(home);
        prefixes.extend([user.join("miniconda3"),user.join("anaconda3"),user.join("miniforge3"),user.join("mambaforge")]);
        let environments=user.join(".conda/environments.txt");
        if environments.metadata().is_ok_and(|m|m.len()<128*1024){if let Ok(text)=std::fs::read_to_string(environments){for line in text.lines().filter(|line|!line.trim().is_empty()).take(64){prefixes.push(PathBuf::from(line.trim()));}}}
    }
    for root in prefixes.clone(){if let Ok(entries)=std::fs::read_dir(root.join("envs")){prefixes.extend(entries.flatten().take(64).map(|e|e.path()));}}
    for prefix in prefixes{paths.push(interpreter(&prefix));}
    let mut seen=std::collections::HashSet::new();let mut environments=vec![];
    for path in paths{
        // Preserve the executable symlink: resolving venv/bin/python to its base
        // binary changes sys.prefix and silently selects a different environment.
        // 保留解释器符号链接，避免把虚拟环境错误切换成基础环境。
        if !path.is_file(){continue;}let absolute=std::path::absolute(&path).unwrap_or(path);
        if !seen.insert(absolute.clone()){continue;}
        let parent=absolute.parent().unwrap_or(Path::new("."));let prefix=if parent.file_name().is_some_and(|n|n=="bin" || n=="Scripts"){parent.parent().unwrap_or(parent)}else{parent};
        let kind=if prefix.join("conda-meta").is_dir(){"conda"}else if prefix.join("pyvenv.cfg").is_file(){"venv"}else{"python"};
        environments.push(json!({"name":prefix.file_name().unwrap_or_default().to_string_lossy(),"python":absolute,"kind":kind}));
        if environments.len()>=64{break;}
    }
    json!({"environments":environments})
}
fn interpreter(prefix:&Path)->PathBuf{prefix.join(if cfg!(windows){"python.exe"}else{"bin/python"})}

pub fn selected(config:&Path)->Option<PathBuf>{
    let metadata=config.metadata().ok()?;if metadata.len()>16384{return None;}
    let value:Value=serde_json::from_slice(&std::fs::read(config).ok()?).ok()?;
    value["python"].as_str().map(PathBuf::from).filter(|path|path.is_absolute() && path.is_file())
}
pub fn save(config:&Path,python:&Path)->Result<(),String>{
    let root=config.parent().ok_or("Missing environment configuration directory")?;
    std::fs::create_dir_all(root).map_err(|e|e.to_string())?;
    let temporary=root.join(format!("environment-{}.tmp",uuid::Uuid::new_v4().simple()));
    let bytes=serde_json::to_vec_pretty(&json!({"schema":1,"python":python})).map_err(|e|e.to_string())?;
    let mut options=std::fs::OpenOptions::new();options.write(true).create_new(true);
    #[cfg(unix)]{use std::os::unix::fs::OpenOptionsExt;options.mode(0o600);}
    let result=(||{use std::io::Write;let mut file=options.open(&temporary).map_err(|e|e.to_string())?;file.write_all(&bytes).and_then(|_|file.sync_all()).map_err(|e|e.to_string())?;drop(file);
        #[cfg(windows)]if config.exists(){std::fs::remove_file(config).map_err(|e|e.to_string())?;}
        std::fs::rename(&temporary,config).map_err(|e|e.to_string())})();
    if result.is_err(){let _=std::fs::remove_file(&temporary);}result
}
