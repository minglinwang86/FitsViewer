//! Bounded view/role leases and tombstones, independent of worker process lifetime.
//! 文档与科研角色分别持有租约；释放记录阻止乱序到达的旧请求重新启动任务。
use serde_json::Value;
use std::{collections::HashMap,time::{Duration,Instant}};

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum Role {View,Pro,Gaia}
impl Role {
    pub fn of(operation:&str,params:&Value)->Self {
        if operation.starts_with("gaia_") || operation=="job_start" && params["operation"]=="gaia_solve" || params["_science_role"]=="gaia"{return Self::Gaia;}
        if operation.starts_with("environment_") || ["dependency_probe","dependency_install"].contains(&operation) || operation=="job_start" && ["dependency_probe","dependency_install"].iter().any(|name|params["operation"]==*name) || ["job_status","job_cancel"].contains(&operation) && params["_science_role"]=="view" || ["capabilities","wcs","artifact_refs","view_heartbeat","release_view","release_document"].contains(&operation){Self::View}else{Self::Pro}
    }
}
#[derive(Clone,Debug)]
pub struct Release {pub owner:String,pub role:Role,pub epoch:Option<u64>,pub document:Option<u64>,pub close_view:bool}
struct Lease {epoch:u64,closed:Option<u64>,active:bool,seen:Instant,document:u64}
impl Lease {
    fn new()->Self{Self{epoch:0,closed:None,active:false,seen:Instant::now(),document:0}}
    fn close(&mut self,epoch:Option<u64>){
        let epoch=epoch.unwrap_or(self.epoch);self.closed=Some(self.closed.map_or(epoch,|old|old.max(epoch)));
        if epoch>=self.epoch{self.active=false;}
    }
}
struct Entry {document:u64,closed_document:Option<u64>,view:bool,seen:Instant,pro:Lease,gaia:Lease}
impl Entry {fn new()->Self{Self{document:0,closed_document:None,view:false,seen:Instant::now(),pro:Lease::new(),gaia:Lease::new()}}}
pub struct Sessions {entries:HashMap<String,Entry>,ttl:Duration}
impl Default for Sessions{fn default()->Self{Self{entries:HashMap::new(),ttl:Duration::from_secs(180)}}}
impl Sessions {
    pub fn prepare_upload(&mut self,owner:&str)->Result<(),String>{
        if self.entries.get(owner).is_some_and(|entry|!entry.view && entry.closed_document.is_some()){return Err("Bridge upload belongs to a closed viewer session".into());}
        self.prepare("artifact_refs",&serde_json::json!({"_viewer_session":owner})).map(|_|())
    }
    pub fn prepare(&mut self,operation:&str,params:&Value)->Result<(String,Vec<Release>),String>{
        let owner=params["_viewer_session"].as_str().unwrap_or("cli").to_owned();
        if owner.is_empty() || owner.len()>128{return Err("Invalid viewer session".into());}
        let generation=params["_document_generation"].as_u64();let epoch=params["_role_epoch"].as_u64();let role=Role::of(operation,params);
        if !self.entries.contains_key(&owner) && self.entries.len()>=4096{return Err("Too many viewer sessions; close unused views".into());}
        let entry=self.entries.entry(owner.clone()).or_insert_with(Entry::new);
        let mut releases=vec![];
        if operation=="release_view" {
            let document=generation.unwrap_or(entry.document);entry.closed_document=Some(entry.closed_document.map_or(document,|old|old.max(document)));
            if document>=entry.document{entry.view=false;entry.pro.close(None);entry.gaia.close(None);}
            entry.seen=Instant::now();return Ok((owner.clone(),vec![Release{owner,role:Role::View,epoch:None,document:Some(document),close_view:document>=entry.document && operation=="release_view"}]));
        }
        if operation=="release_document" {
            let document=generation.unwrap_or(entry.document);entry.closed_document=Some(entry.closed_document.map_or(document,|old|old.max(document)));
            if entry.document<=document{entry.view=false;}
            if entry.gaia.document<=document{entry.gaia.close(None);}
            return Ok((owner.clone(),vec![Release{owner,role:Role::View,epoch:None,document:Some(document),close_view:document>=entry.document && operation=="release_view"}]));
        }
        if operation=="release_pro" || operation=="gaia_release" {
            let lease=if role==Role::Gaia{&mut entry.gaia}else{&mut entry.pro};lease.close(epoch);
            return Ok((owner.clone(),vec![Release{owner,role,epoch,document:None,close_view:false}]));
        }
        // Generation is optional for command-line/legacy clients; GUI requests always carry it.
        // CLI 保持兼容；新 GUI 的每个科研请求都必须满足文档与角色的释放栅栏。
        if let Some(generation)=generation {
            if entry.closed_document.is_some_and(|closed|generation<=closed) || generation<entry.document{return Err("Discarded a request for a closed document".into());}
        }
        if role!=Role::View {
            let lease=if role==Role::Gaia{&mut entry.gaia}else{&mut entry.pro};
            if let Some(epoch)=epoch {
                if lease.closed.is_some_and(|closed|epoch<=closed) || epoch<lease.epoch{return Err("Discarded a request for a released scientific role".into());}
                if operation.ends_with("heartbeat") && (!lease.active || epoch!=lease.epoch){return Err("Scientific role lease has expired".into());}
                if epoch>lease.epoch && lease.active{releases.push(Release{owner:owner.clone(),role,epoch:Some(lease.epoch),document:None,close_view:false});}
                lease.epoch=epoch;
            }
            lease.active=true;lease.seen=Instant::now();lease.document=generation.unwrap_or(entry.document);
        }
        if let Some(generation)=generation{entry.document=generation;}
        entry.view=true;entry.seen=Instant::now();
        Ok((owner,releases))
    }
    pub fn expire(&mut self)->Vec<Release>{
        let mut releases=vec![];
        for (owner,entry) in &mut self.entries {
            if entry.view && entry.seen.elapsed()>=self.ttl {
                entry.view=false;entry.closed_document=Some(entry.closed_document.map_or(entry.document,|old|old.max(entry.document)));
                entry.pro.close(None);entry.gaia.close(None);
                releases.push(Release{owner:owner.clone(),role:Role::View,epoch:None,document:Some(entry.document),close_view:true});continue;
            }
            for (role,lease) in [(Role::Pro,&mut entry.pro),(Role::Gaia,&mut entry.gaia)] {
                if lease.active && lease.seen.elapsed()>=self.ttl{let epoch=lease.epoch;lease.close(Some(epoch));releases.push(Release{owner:owner.clone(),role,epoch:Some(epoch),document:None,close_view:false});}
            }
        }
        // Keep bounded tombstones longer than the worker request deadline, then retire idle entries.
        self.entries.retain(|_,entry|entry.view || entry.pro.active || entry.gaia.active || entry.seen.elapsed()<Duration::from_secs(1800));
        releases
    }
    pub fn counts(&self)->(usize,usize,usize){(self.entries.values().filter(|e|e.view).count(),self.entries.values().filter(|e|e.pro.active).count(),self.entries.values().filter(|e|e.gaia.active).count())}
    #[cfg(feature="pro")]
    pub fn active_pro(&self,owner:&str,document:u64,epoch:u64)->bool{
        self.entries.get(owner).is_some_and(|entry|entry.view && entry.document==document
            && entry.closed_document.is_none_or(|closed|document>closed)
            && entry.pro.active && entry.pro.document==document && entry.pro.epoch==epoch
            && entry.pro.closed.is_none_or(|closed|epoch>closed))
    }
    pub fn scientific_roles(&self)->bool{let (_,pro,gaia)=self.counts();pro+gaia>0}
    pub fn any_views(&self)->bool{let (view,pro,gaia)=self.counts();view+pro+gaia>0}
}

#[cfg(test)]mod tests{
    use super::*;use serde_json::json;
    fn p(document:u64,epoch:u64)->Value{json!({"_viewer_session":"a","_document_generation":document,"_role_epoch":epoch})}
    #[test]fn minimal_dependency_polling_never_acquires_pro(){
        let mut s=Sessions::default();let mut params=p(1,0);params["_science_role"]=json!("view");params["operation"]=json!("dependency_install");
        s.prepare("job_start",&params).unwrap();assert_eq!(s.counts(),(1,0,0));
        s.prepare("job_status",&params).unwrap();assert_eq!(s.counts(),(1,0,0));
        s.prepare("release_pro",&params).unwrap();s.prepare("job_cancel",&params).unwrap();assert_eq!(s.counts(),(1,0,0));
        s.prepare("release_view",&params).unwrap();assert_eq!(s.counts(),(0,0,0));
    }
    #[test]fn late_start_cannot_resurrect_released_role(){let mut s=Sessions::default();s.prepare("pro_acquire",&p(1,1)).unwrap();s.prepare("release_pro",&p(1,1)).unwrap();assert!(s.prepare("job_start",&p(1,1)).is_err());assert!(s.prepare("pro_acquire",&p(1,3)).is_ok());}
    #[test]fn old_release_keeps_new_role(){let mut s=Sessions::default();s.prepare("pro_acquire",&p(1,1)).unwrap();let (_,old)=s.prepare("pro_acquire",&p(1,3)).unwrap();assert_eq!(old[0].epoch,Some(1));s.prepare("release_pro",&p(1,1)).unwrap();assert_eq!(s.counts(),(1,1,0));}
    #[test]fn cached_selection_rejects_old_document(){let mut s=Sessions::default();s.prepare("pro_acquire",&p(1,1)).unwrap();s.prepare("release_document",&p(1,1)).unwrap();assert!(s.prepare("job_start",&p(1,1)).is_err());assert!(s.prepare("job_start",&p(2,1)).is_ok());}
    #[test]fn old_close_does_not_release_new_view(){let mut s=Sessions::default();s.prepare("view_heartbeat",&p(2,1)).unwrap();s.prepare("release_view",&p(1,1)).unwrap();assert_eq!(s.counts().0,1);}
    #[test]fn ttl_expires_without_another_request_and_cannot_heartbeat_back(){let mut s=Sessions::default();s.ttl=Duration::ZERO;s.prepare("pro_acquire",&p(1,1)).unwrap();assert_eq!(s.expire().len(),1);assert_eq!(s.counts(),(0,0,0));assert!(s.prepare("pro_heartbeat",&p(1,1)).is_err());}
    #[test]fn pro_release_preserves_gaia(){let mut s=Sessions::default();s.prepare("pro_acquire",&p(1,1)).unwrap();s.prepare("gaia_acquire",&p(1,1)).unwrap();s.prepare("release_pro",&p(1,1)).unwrap();assert_eq!(s.counts(),(1,0,1));}
    #[test]fn rejected_role_does_not_reopen_view(){let mut s=Sessions::default();s.prepare("pro_acquire",&p(1,1)).unwrap();s.prepare("release_view",&p(1,1)).unwrap();assert!(s.prepare("job_start",&p(2,1)).is_err());assert_eq!(s.counts(),(0,0,0));assert!(s.prepare("view_heartbeat",&p(2,1)).is_ok());}
    #[test]fn upload_cannot_reopen_a_closed_owner(){let mut s=Sessions::default();s.prepare_upload("bridge-a").unwrap();s.prepare("release_view",&serde_json::json!({"_viewer_session":"bridge-a"})).unwrap();assert!(s.prepare_upload("bridge-a").is_err());assert_eq!(s.counts(),(0,0,0));}
}
