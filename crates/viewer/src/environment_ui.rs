//! Explicit external Python selection and dependency tasks, available in Mini.
//! 环境切换以宿主成功回执为准；不把路径编辑当作已经切换环境。
use eframe::egui;
use serde_json::{json,Value};
use crate::{i18n::t,science_client::{Client,Reply}};

#[derive(Default)]
pub struct Environment {
    pub info:Value,
    probe:Value,candidates:Value,path:String,message:String,busy:bool,
    job:Option<String>,last_poll:f64,poll_pending:bool,installing:bool,
    packages:Vec<String>,recheck:bool,installation:Value,info_requested:bool,
}
impl Environment {
    pub fn active(&self)->bool{self.busy || self.job.is_some() || !self.installation.is_null()}
    pub fn abandon(&mut self){self.busy=false;self.job=None;self.poll_pending=false;self.installing=false;self.recheck=false;self.installation=Value::Null;}
    pub fn artifact_path(&self)->Option<String>{self.installation["log"]["artifact_id"].as_str().map(|id|format!("artifact:{id}"))}
    pub fn reply(&mut self,reply:Reply){
        self.poll_pending=false;
        match reply.result{
            Err(error)=>{self.busy=false;self.installing=false;self.message=error;self.job=None;},
            Ok(value)=>match reply.operation.as_str(){
                "environment.info"|"environment.select"=>{
                    if let Some(path)=value["python"].as_str().or_else(||value["executable"].as_str()){self.path=path.into();}
                    if reply.operation=="environment.select"{self.probe=Value::Null;self.packages.clear();self.recheck=true;}
                    self.info=value;self.busy=false;self.message="Environment ready".into();
                },
                "environment.candidates"=>{self.candidates=value;self.busy=false;},
                "environment.probe"=>{self.probe=value;self.busy=false;self.message="Dependency check complete".into();self.packages.retain(|name|self.probe["missing"].as_array().is_some_and(|a|a.iter().any(|v|v==name)));},
                "environment.install"=>{self.job=value["job_id"].as_str().map(str::to_owned);if self.job.is_none(){self.probe=value;self.busy=false;self.installing=false;}},
                "environment.status"=>{
                    match value["state"].as_str().unwrap_or(""){
                        "queued"|"running"=>{self.message=value["message"].as_str().unwrap_or(t("Installing dependencies…")).into();if !value["progress"]["log_tail"].is_null(){self.probe["installation_log"]=value["progress"]["log_tail"].clone();}},
                        "completed"=>{self.installation=value["result"].clone();self.job=None;self.busy=false;self.installing=false;self.recheck=true;self.message="Installation finished; checking dependencies…".into();},
                        _=>{self.message=value["error"].as_str().unwrap_or(t("Dependency installation cancelled")).into();self.job=None;self.busy=false;self.installing=false;},
                    }
                },_=>{},
            }
        }
    }
    pub fn tick(&mut self,ctx:&egui::Context,client:&Client,generation:u64){
        if self.recheck && !self.busy{self.recheck=false;self.busy=true;client.request_labeled(ctx,generation,"dependency_probe","environment.probe",json!({}));}
        if let Some(job)=&self.job{let now=ctx.input(|i|i.time);if !self.poll_pending && now-self.last_poll>0.5{self.last_poll=now;self.poll_pending=true;client.request_labeled(ctx,generation,"job_status","environment.status",json!({"job_id":job}));}ctx.request_repaint_after(std::time::Duration::from_millis(300));}
    }
    pub fn ui(&mut self,ui:&mut egui::Ui,ctx:&egui::Context,client:&Client,generation:u64){
        if !self.info_requested{self.info_requested=true;self.busy=true;client.request_labeled(ctx,generation,"environment_info","environment.info",json!({}));client.request_labeled(ctx,generation,"environment_candidates","environment.candidates",json!({}));}
        ui.strong(t("Scientific Python environment"));
        if self.info.is_null(){ui.label(t("Inspect the active environment before selecting or installing packages."));}
        else{
            let python=self.info["python"].as_str().or_else(||self.info["executable"].as_str()).unwrap_or("");
            ui.label(format!("{}: {}",t("Active Python"),python));
            if let Some(name)=self.info["environment"].as_str(){ui.label(format!("{}: {name}",t("Environment")));}
        }
        ui.weak(t("This selects the external scientific worker. Jupyter server and Notebook kernels are separate environments."));
        ui.horizontal_wrapped(|ui|{
            if ui.add_enabled(!self.busy,egui::Button::new(t("Inspect environment"))).clicked(){self.busy=true;client.request_labeled(ctx,generation,"environment_info","environment.info",json!({}));client.request_labeled(ctx,generation,"environment_candidates","environment.candidates",json!({}));}
            if ui.add_enabled(!self.busy,egui::Button::new(t("Check dependencies"))).clicked(){self.busy=true;self.message=t("Checking dependencies…").into();client.request_labeled(ctx,generation,"dependency_probe","environment.probe",json!({}));}
        });
        if let Some(environments)=self.candidates["environments"].as_array(){
            egui::ComboBox::from_id_salt("science-environment-choice").selected_text(t("Choose Python / Conda environment")).show_ui(ui,|ui|{
                for environment in environments{if let Some(python)=environment["python"].as_str(){let name=environment["name"].as_str().unwrap_or(python);if ui.selectable_label(self.path==python,name).clicked(){self.path=python.into();}}}
            });
        }
        ui.add(egui::TextEdit::singleline(&mut self.path).hint_text(t("Absolute Python executable path")));
        if ui.add_enabled(!self.busy && !self.path.trim().is_empty(),egui::Button::new(t("Use this Python"))).clicked(){self.busy=true;self.message=t("Switching external Python…").into();client.request_labeled(ctx,generation,"environment_select","environment.select",json!({"python":self.path.trim()}));}
        if self.busy{ui.horizontal(|ui|{ui.spinner();crate::controls::status_message(ui,&self.message);});}else{crate::controls::status_message(ui,&self.message);}
        if let Some(job)=&self.job{if ui.button(t("Cancel installation")).clicked(){client.request_labeled(ctx,generation,"job_cancel","environment.cancel",json!({"job_id":job}));}}
        if !self.installation.is_null(){
            if let Some(status)=self.installation["status"].as_str(){ui.strong(t(status));}
            if let Some(message)=self.installation["message"].as_str(){ui.add(egui::Label::new(message).wrap());}
            if let Some(id)=self.installation["log"]["artifact_id"].as_str(){if let Some(url)=client.artifact_url(id){ui.hyperlink_to(t("Download installation log"),url);}}
        }
        if !self.probe.is_null(){
            ui.separator();
            if let Some(features)=self.probe["features"].as_object(){for (name,status) in features{ui.label(format!("{name}: {}",status["status"].as_str().unwrap_or_else(||status.as_str().unwrap_or("unknown"))));}}
            for key in ["dependencies","packages"]{if let Some(packages)=self.probe[key].as_array(){for package in packages{
                let name=package["package"].as_str().or_else(||package["name"].as_str()).unwrap_or("");
                let status=package["state"].as_str().or_else(||package["status"].as_str()).unwrap_or("unknown");
                ui.horizontal_wrapped(|ui|{ui.label(format!("{name} · {}",t(status)));if let Some(version)=package["version"].as_str(){ui.weak(version);}
                    if !name.is_empty() && status=="missing"{let mut chosen=self.packages.iter().any(|p|p==name);if ui.checkbox(&mut chosen,t("Install")).changed(){if chosen{self.packages.push(name.into());}else{self.packages.retain(|p|p!=name);}}}
                });
                if let Some(purposes)=package["purposes"].as_array(){ui.weak(purposes.iter().filter_map(Value::as_str).map(t).collect::<Vec<_>>().join(" · "));}
                if let Some(error)=package["error"].as_str().filter(|e|!e.is_empty()){ui.add(egui::Label::new(error).wrap());}
            }}}
            if ui.add_enabled(!self.busy && !self.packages.is_empty(),egui::Button::new(t("Install selected missing dependencies"))).clicked(){self.busy=true;self.installing=true;self.message=t("Installing dependencies…").into();client.request_labeled(ctx,generation,"job_start","environment.install",json!({"operation":"dependency_install","params":{"packages":self.packages}}));}
            ui.weak(t("Install only missing packages into the active Python. Version conflicts require explicit resolution."));
            egui::CollapsingHeader::new(t("Dependency details / installation log")).id_salt("dependency-details").show(ui,|ui|{ui.add(egui::Label::new(serde_json::to_string_pretty(&self.probe).unwrap_or_default()).wrap());});
        }
    }
}
