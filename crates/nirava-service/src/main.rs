use anyhow::Result;
use clap::Parser;
use nirava_core::{BUS_NAME,OBJECT_PATH,OperationResult,IdleSettings};
use std::{collections::HashMap,path::PathBuf};
use tokio::sync::Mutex;

#[derive(Parser)]
#[command(about="Nirava's user-session backend. Read-only unless --allow-writes is passed.")]
struct Args {
    #[arg(long)] allow_writes:bool,
    #[arg(long)] demo:bool,
    /// Requires this process to be supervised by nirava-sessiond.service.
    #[arg(long)] enable_display_preview:bool,
    /// Enable only for human-operated lock/suspend acceptance testing.
    #[arg(long)] enable_suspend:bool,
}

struct Manager {
    allow_writes:bool,
    demo:bool,
    config_writes:Mutex<()>,
    staged:Mutex<HashMap<String,nirava_core::transaction::FileChange>>,
    caffeine:Mutex<Option<zbus::zvariant::OwnedFd>>,
    session_id:String,
    enable_suspend:bool,
    displays:Option<std::sync::Arc<nirava_backends::display_guard::DisplayGuard<nirava_backends::display_guard::NiriDriver>>>,
    #[cfg(feature="audio")]
    audio:nirava_backends::audio::AudioClient,
}

impl Manager {
    fn permit(&self)->Result<()> {
        if !self.allow_writes||self.demo {anyhow::bail!("Backend is read-only. No configuration or device settings were changed");}
        Ok(())
    }
    async fn snapshot(&self)->nirava_core::Snapshot {
        let mut state=if self.demo {nirava_core::Snapshot::demo()} else {nirava_backends::discovery::discover().await};
        state.read_only=!self.allow_writes||self.demo;
        state.power.caffeine=self.caffeine.lock().await.is_some();
        if !self.demo{state.backups=read_backups(!state.read_only);}
        if self.displays.is_some(){if let Some(capability)=state.capabilities.iter_mut().find(|c|c.id=="displays.write"){capability.supported=true;capability.reason="Supervised session-only previews; persistent Save is not enabled yet".into();}}
        state.capabilities.push(nirava_core::Capability{id:"power.suspend".into(),supported:self.enable_suspend&&self.allow_writes&&!self.demo&&state.session.idle_owners.as_slice()==["noctalia"]&&state.power.lock_enabled&&["yes","challenge"].contains(&state.power.can_suspend.as_str()),backend:state.power.owner.clone(),reason:if self.enable_suspend{"Manual lock/suspend acceptance mode"}else{"Requires manual lock-readiness acceptance testing"}.into()});
        #[cfg(feature="audio")]
        if !self.demo {
            let audio=self.audio.clone();
            state.sound=tokio::task::spawn_blocking(move||audio.snapshot()).await
                .map_err(anyhow::Error::from).and_then(|result|result)
                .unwrap_or_else(|e|nirava_core::SoundState{error:e.to_string(),..Default::default()});
            state.capabilities.push(nirava_core::Capability{id:"sound.write".into(),supported:state.sound.available,backend:state.sound.server.clone(),reason:String::new()});
        }
        #[cfg(not(feature="audio"))]
        {state.sound.error="Built without native audio support".into();}
        state
    }
    async fn stage_power_impl(&self,settings:IdleSettings)->Result<String> {
        self.permit()?;
        let state=self.snapshot().await;
        if state.session.idle_owners.as_slice()!=["noctalia"] {anyhow::bail!("A single Noctalia idle owner is required");}
        if !state.power.error.is_empty(){anyhow::bail!("Existing settings could not be imported safely: {}",state.power.error);}
        let path=nirava_backends::power::noctalia_config();
        let original=std::fs::read_to_string(&path)?;
        let proposed=nirava_core::config::edit_noctalia(&original,&settings)?;
        if proposed==original{anyhow::bail!("These settings already match the configuration");}
        let change=nirava_core::transaction::FileChange::read_expected(path,&original,proposed,"Update Noctalia idle durations; reload the shell".into())?;
        let id=format!("power-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos());
        let response=serde_json::json!({"id":id,"path":change.path,"before":settings_summary(&original),"after":settings,"description":change.description});
        let mut staged=self.staged.lock().await;
        if staged.len()>=32{anyhow::bail!("Too many pending reviews; cancel existing reviews first");}
        staged.insert(id,change);
        Ok(response.to_string())
    }
    async fn commit_impl(&self,id:&str)->Result<OperationResult> {
        self.permit()?;
        let _write_guard=self.config_writes.lock().await;
        let change=self.staged.lock().await.remove(id).ok_or_else(||anyhow::anyhow!("Preview not found; create a fresh preview"))?;
        let state=self.snapshot().await;
        if state.session.idle_owners.as_slice()!=["noctalia"] {anyhow::bail!("Settings owner changed; commit refused");}
        let journal_dir=nirava_backends::state_home().join("nirava/transactions");
        std::fs::create_dir_all(&journal_dir)?;
        std::fs::set_permissions(&journal_dir,std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
        let journal=journal_dir.join(format!("{id}.json"));
        write_private(&journal,&serde_json::to_vec_pretty(&serde_json::json!({"status":"staged","path":change.path,"original_hash":change.original_hash,"proposed_hash":nirava_core::transaction::digest(change.proposed.as_bytes())}))?)?;
        let committed=change.commit()?;
        if let Err(error)=nirava_core::transaction::atomic_replace(&journal,&serde_json::to_vec_pretty(&committed)?) {
            committed.restore()?;anyhow::bail!("Journal update failed; configuration restored: {error}");
        }
        if let Err(error)=nirava_backends::command("noctalia",&["msg","config-reload"]).await {
            committed.restore()?;
            let _=nirava_backends::command("noctalia",&["msg","config-reload"]).await;
            anyhow::bail!("Reload failed; original configuration restored: {error}");
        }
        let actual=nirava_core::config::read_noctalia(&std::fs::read_to_string(&change.path)?)?;
        if actual!=nirava_core::config::read_noctalia(&change.proposed)? {anyhow::bail!("Configuration readback did not match the proposed settings");}
        Ok(OperationResult{id:id.into(),status:"applied".into(),message:format!("Saved and requested Noctalia reload. Backup: {}",committed.backup.display())})
    }
}

fn settings_summary(text:&str)->Option<IdleSettings>{nirava_core::config::read_noctalia(text).ok()}
fn read_backups(writable:bool)->Vec<nirava_core::BackupInfo> {
    let directory=nirava_backends::state_home().join("nirava/transactions");
    let Ok(entries)=std::fs::read_dir(directory)else{return vec![]};
    let mut paths=entries.filter_map(Result::ok).map(|e|e.path()).filter(|p|p.extension().is_some_and(|e|e=="json")).collect::<Vec<_>>();
    paths.sort();paths.reverse();paths.truncate(100);
    paths.into_iter().filter_map(|path|{
        let id=path.file_stem()?.to_str()?.to_string();if !id.starts_with("power-"){return None;}
        let committed=read_committed(&path).ok()?;
        if committed.path!=nirava_backends::power::noctalia_config(){return None;}
        let check=committed.check_restore();
        Some(nirava_core::BackupInfo{id,path:committed.path.display().to_string(),backup:committed.backup.display().to_string(),
            can_restore:writable&&check.is_ok(),reason:if !writable{"Read-only mode".into()}else{check.err().map(|e|e.to_string()).unwrap_or_default()}})
    }).collect()
}
fn read_committed(path:&std::path::Path)->Result<nirava_core::transaction::CommittedChange> {
    nirava_core::transaction::ensure_regular_path(path)?;
    if std::fs::metadata(path)?.len()>65536{anyhow::bail!("Backup journal is too large");}
    use std::io::Read;
    let mut bytes=Vec::new();std::fs::File::open(path)?.take(65537).read_to_end(&mut bytes)?;
    if bytes.len()>65536{anyhow::bail!("Backup journal is too large");}
    Ok(serde_json::from_slice(&bytes)?)
}
fn write_private(path:&PathBuf,bytes:&[u8])->Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    use std::io::Write;
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(path)?;
    file.write_all(bytes)?;file.sync_all()?;Ok(())
}
fn json_result(result:Result<OperationResult>)->String {
    serde_json::to_string(&result.unwrap_or_else(|e|OperationResult::failed(e.to_string()))).unwrap_or_else(|_|"{}".into())
}

#[zbus::interface(name="io.github.Somchandra17.Nirava.Manager1")]
impl Manager {
    async fn get_apps(&self)->String {
        let inventory=if self.demo {nirava_core::apps::AppsInventory::demo()} else {nirava_backends::apps::discover().await};
        serde_json::to_string(&inventory).unwrap_or_default()
    }
    async fn get_snapshot(&self)->String {serde_json::to_string(&self.snapshot().await).unwrap_or_default()}
    fn get_session_id(&self)->String {self.session_id.clone()}
    fn get_session_key(&self)->String {nirava_core::session_key()}
    async fn stage_power(&self,settings_json:&str)->zbus::fdo::Result<String> {
        let settings=serde_json::from_str(settings_json).map_err(|e|zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        self.stage_power_impl(settings).await.map_err(|e|zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn commit_change(&self,id:&str)->String {json_result(self.commit_impl(id).await)}
    async fn cancel_change(&self,id:&str) {self.staged.lock().await.remove(id);}
    async fn set_caffeine(&self,enabled:bool)->String {
        let result=async {
            self.permit()?;
            let state=self.snapshot().await;
            if state.power.owner!="noctalia" {anyhow::bail!("Inhibitor support is not verified for this settings owner");}
            let mut inhibitor=self.caffeine.lock().await;
            if enabled&&inhibitor.is_none(){*inhibitor=Some(nirava_backends::power::caffeine().await?);}
            if !enabled{*inhibitor=None;}
            Ok(OperationResult::applied(if enabled{"Nirava idle inhibitor is active"}else{"Nirava idle inhibitor released"}))
        }.await;
        json_result(result)
    }
    async fn suspend(&self)->String {
        let result=async {
            self.permit()?;
            if !self.enable_suspend{anyhow::bail!("Suspend remains disabled until the locker readiness path passes the manual acceptance check");}
            let state=self.snapshot().await;
            if state.session.idle_owners.as_slice()!=["noctalia"]||!state.power.lock_enabled{anyhow::bail!("A single Noctalia owner with an enabled lockscreen is required");}
            if !["yes","challenge"].contains(&state.power.can_suspend.as_str()){anyhow::bail!("logind does not permit suspend");}
            let response=nirava_backends::command("noctalia",&["msg","session","lock-and-suspend"]).await?;
            if response.trim()!="ok"{anyhow::bail!("Noctalia did not acknowledge lock-and-suspend: {}",response.trim());}
            Ok(OperationResult{id:String::new(),status:"accepted".into(),message:"Noctalia accepted lock-and-suspend; command acceptance does not prove that sleep occurred".into()})
        }.await;
        json_result(result)
    }
    async fn preview_layout(&self,draft:&str)->zbus::fdo::Result<String> {
        self.permit().map_err(|e|zbus::fdo::Error::AccessDenied(e.to_string()))?;
        let state=self.snapshot().await;
        if !state.session.display_owners.is_empty(){return Err(zbus::fdo::Error::Failed("Another display manager is running; preview refused".into()));}
        let guard=self.displays.as_ref().ok_or_else(||zbus::fdo::Error::Failed("Supervised display preview is not enabled".into()))?;
        let draft=serde_json::from_str(draft).map_err(|e|zbus::fdo::Error::InvalidArgs(e.to_string()))?;
        let lease=guard.preview(draft,std::time::Duration::from_secs(15)).await.map_err(|e|zbus::fdo::Error::Failed(e.to_string()))?;
        serde_json::to_string(&lease).map_err(|e|zbus::fdo::Error::Failed(e.to_string()))
    }
    async fn revert_layout(&self,id:&str)->String {
        let result=async{let guard=self.displays.as_ref().ok_or_else(||anyhow::anyhow!("Display preview is not enabled"))?;guard.revert(id).await?;Ok(OperationResult::applied("Original display layout restored"))}.await;json_result(result)
    }
    async fn keep_layout_for_session(&self,id:&str)->String {
        let result=async{self.permit()?;let guard=self.displays.as_ref().ok_or_else(||anyhow::anyhow!("Display preview is not enabled"))?;guard.keep_session_only(id).await?;Ok(OperationResult::applied("Kept for this session only; compositor configuration was not saved"))}.await;json_result(result)
    }
    async fn restore_change(&self,id:&str)->String {
        let result=async{
            self.permit()?;
            let _write_guard=self.config_writes.lock().await;
            if id.len()<=6||!id.starts_with("power-")||!id[6..].chars().all(|c|c.is_ascii_digit())||id.len()>50{anyhow::bail!("Invalid backup identifier");}
            let path=nirava_backends::state_home().join("nirava/transactions").join(format!("{id}.json"));
            let committed=read_committed(&path)?;
            if committed.path!=nirava_backends::power::noctalia_config(){anyhow::bail!("Backup belongs to an unsupported integration");}
            committed.restore()?;nirava_backends::command("noctalia",&["msg","config-reload"]).await?;
            Ok(OperationResult::applied("Previous Noctalia configuration restored and reload requested"))
        }.await;json_result(result)
    }
    async fn set_volume(&self,generation:u64,id:u32,input:bool,stream:bool,volume:f64)->String {
        let result=async {
            self.permit()?;
            if !volume.is_finite()||!(0.0..=1.0).contains(&volume){anyhow::bail!("Volume must be between 0 and 100%");}
            #[cfg(feature="audio")]
            {let audio=self.audio.clone();tokio::task::spawn_blocking(move||audio.set_volume(generation,id,input,stream,volume)).await??;Ok(OperationResult::applied("Audio volume updated"))}
            #[cfg(not(feature="audio"))]
            {let _=(generation,id,input,stream);anyhow::bail!("Native audio support is not compiled in")}
        }.await;json_result(result)
    }
    async fn set_mute(&self,generation:u64,id:u32,input:bool,stream:bool,muted:bool)->String {
        let result=async {
            self.permit()?;
            #[cfg(feature="audio")]
            {let audio=self.audio.clone();tokio::task::spawn_blocking(move||audio.set_mute(generation,id,input,stream,muted)).await??;Ok(OperationResult::applied("Audio mute updated"))}
            #[cfg(not(feature="audio"))]
            {let _=(generation,id,input,stream,muted);anyhow::bail!("Native audio support is not compiled in")}
        }.await;json_result(result)
    }
    async fn set_default_device(&self,generation:u64,name:&str,input:bool)->String {
        let result=async {
            self.permit()?;
            #[cfg(feature="audio")]
            {let audio=self.audio.clone();let name=name.to_string();tokio::task::spawn_blocking(move||audio.set_default(generation,name,input)).await??;Ok(OperationResult::applied("Default audio device updated"))}
            #[cfg(not(feature="audio"))]
            {let _=(generation,name,input);anyhow::bail!("Native audio support is not compiled in")}
        }.await;json_result(result)
    }
}

#[tokio::main]
async fn main()->Result<()> {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let args=Args::parse();
    if unsafe{libc::geteuid()}==0&&!args.demo{anyhow::bail!("Nirava's real session backend must run as your desktop user; do not run it with sudo");}
    let displays=if args.enable_display_preview {
        if !args.allow_writes||args.demo{anyhow::bail!("Display preview requires --allow-writes and a real supervised session");}
        let pid=nirava_backends::command("systemctl",&["--user","show","nirava-sessiond.service","--property=MainPID","--value"]).await?;
        if pid.trim()!=std::process::id().to_string(){anyhow::bail!("Display preview requires supervision by nirava-sessiond.service; nothing was applied");}
        let session=nirava_backends::discovery::discover().await;
        if session.session.compositor!="Niri"{anyhow::bail!("Only Niri's runtime preview driver is implemented");}
        let runtime=std::env::var_os("XDG_RUNTIME_DIR").ok_or_else(||anyhow::anyhow!("No user runtime directory"))?;
        let guard=nirava_backends::display_guard::DisplayGuard::new(nirava_backends::display_guard::NiriDriver,PathBuf::from(runtime).join("nirava/pending-display.json"));
        guard.recover().await?;Some(guard)
    } else {None};
    let manager=Manager {allow_writes:args.allow_writes,demo:args.demo,config_writes:Mutex::new(()),staged:Mutex::new(HashMap::new()),caffeine:Mutex::new(None),
        enable_suspend:args.enable_suspend,displays:displays.clone(),
        session_id:std::env::var("XDG_SESSION_ID").unwrap_or_default(),
        #[cfg(feature="audio")] audio:nirava_backends::audio::AudioClient::new(),};
    let _connection=zbus::connection::Builder::session()?.name(BUS_NAME)?.serve_at(OBJECT_PATH,manager)?.build().await?;
    tracing::info!(read_only=!args.allow_writes||args.demo,"Nirava session service ready");
    let mut terminate=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    tokio::select!{result=tokio::signal::ctrl_c()=>result?,_=terminate.recv()=>{}}
    if let Some(guard)=displays{if let Some(lease)=guard.active().await{guard.revert(&lease.id).await?;}}
    Ok(())
}
