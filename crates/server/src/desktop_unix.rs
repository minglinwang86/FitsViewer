//! Native file selection using the desktop tools already installed by the user.
//! 调用系统已有文件选择器，不捆绑另一套图形运行时。
use std::path::PathBuf;
fn available(name:&str)->bool{std::env::var_os("PATH").is_some_and(|path|std::env::split_paths(&path).any(|p|p.join(name).is_file()))}
pub fn has_picker()->bool{cfg!(target_os="macos") || available("zenity") || available("kdialog")}
pub fn pick_file()->Option<PathBuf>{
    let mut command=if cfg!(target_os="macos"){
        let mut command=std::process::Command::new("osascript");command.args(["-e","POSIX path of (choose file with prompt \"Open FITS image or spectrum\")"]);command
    }else if available("zenity"){
        let mut command=std::process::Command::new("zenity");command.args(["--file-selection","--title=FitsViewer: open FITS or spectrum"]);command
    }else if available("kdialog"){
        let mut command=std::process::Command::new("kdialog");command.args(["--getopenfilename",".","*.fits *.fit *.fts *.fz *.gz *.txt *.dat *.csv"]);command
    }else{return None;};
    let output=command.output().ok()?;if !output.status.success(){return None;}
    let text=String::from_utf8(output.stdout).ok()?;let path=PathBuf::from(text.trim_end_matches(['\r','\n']));path.is_file().then_some(path)
}

