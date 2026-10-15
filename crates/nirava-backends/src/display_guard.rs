use anyhow::{Result,bail};
use async_trait::async_trait;
use nirava_core::{OutputState,session_key};
use serde::{Serialize,Deserialize};
use std::{path::PathBuf,sync::Arc,time::Duration};
use tokio::sync::Mutex;

#[async_trait]
pub trait OutputDriver:Send+Sync+'static {
    async fn read(&self)->Result<Vec<OutputState>>;
    async fn apply(&self,layout:&[OutputState])->Result<()>;
}
pub struct NiriDriver;
#[async_trait]
impl OutputDriver for NiriDriver {
    async fn read(&self)->Result<Vec<OutputState>>{crate::niri::read_outputs().await}
    async fn apply(&self,layout:&[OutputState])->Result<()>{crate::niri::apply_layout(layout).await}
}

#[derive(Debug,Clone,Serialize,Deserialize)]
pub struct Lease {
    pub id:String,
    pub session_key:String,
    pub expires_at_millis:u64,
    #[serde(skip)]
    deadline:Option<tokio::time::Instant>,
    pub baseline:Vec<OutputState>,
    pub draft:Vec<OutputState>,
}

pub struct DisplayGuard<D:OutputDriver> {
    driver:D,
    active:Mutex<Option<Lease>>,
    journal:PathBuf,
}
impl<D:OutputDriver> DisplayGuard<D> {
    pub fn new(driver:D,journal:PathBuf)->Arc<Self>{Arc::new(Self{driver,active:Mutex::new(None),journal})}
    pub async fn preview(self:&Arc<Self>,draft:Vec<OutputState>,duration:Duration)->Result<Lease> {
        let mut active=self.active.lock().await;
        if active.is_some(){bail!("Another display preview is active; revert it first");}
        let baseline=self.driver.read().await?;
        crate::niri::validate_layout(&baseline,&draft)?;
        if equivalent(&baseline,&draft){bail!("This layout already matches the current displays");}
        let id=format!("display-{}",std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_nanos());
        let deadline=tokio::time::Instant::now()+duration;
        let lease=Lease{id:id.clone(),session_key:session_key(),baseline,draft,
            expires_at_millis:(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH)?.as_millis()+duration.as_millis()).try_into()?,deadline:Some(deadline)};
        self.write_journal(&lease)?;
        *active=Some(lease.clone());
        let guard=Arc::clone(self);let watchdog_id=id.clone();
        tokio::spawn(async move{
            tokio::time::sleep_until(deadline).await;
            while guard.active().await.is_some_and(|lease|lease.id==watchdog_id) {
                match guard.revert(&watchdog_id).await {
                    Ok(())=>break,
                    Err(error)=>tracing::warn!(%error,"Display watchdog recovery will retry"),
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        });
        let application=tokio::time::timeout(Duration::from_secs(8),self.driver.apply(&lease.draft)).await.map_err(|_|anyhow::anyhow!("Display application timed out")).and_then(|result|result);
        if let Err(error)=application {
            let rollback=self.restore(&lease).await;
            if rollback.is_ok(){*active=None;self.clear_journal()?;}
            return Err(anyhow::anyhow!("Display application failed: {error}; recovery: {}",if rollback.is_ok(){"restored".into()}else{rollback.unwrap_err().to_string()}));
        }
        let actual=match self.driver.read().await {
            Ok(actual)=>actual,
            Err(error)=>{
                if self.restore(&lease).await.is_ok(){*active=None;self.clear_journal()?;}
                return Err(anyhow::anyhow!("Display readback failed: {error}; the recovery watchdog remains responsible for any pending change"));
            }
        };
        if !equivalent(&actual,&lease.draft) {
            self.restore(&lease).await?;*active=None;self.clear_journal()?;bail!("Compositor readback differs from the requested layout; reverted");
        }
        drop(active);
        Ok(lease)
    }
    pub async fn revert(&self,id:&str)->Result<()> {
        let mut active=self.active.lock().await;
        let lease=active.as_ref().ok_or_else(||anyhow::anyhow!("Preview is no longer active"))?;
        if lease.id!=id{bail!("Preview id is stale");}
        self.restore(lease).await?;self.clear_journal()?;*active=None;Ok(())
    }
    pub async fn active(&self)->Option<Lease>{self.active.lock().await.clone()}
    pub async fn keep_session_only(&self,id:&str)->Result<()> {
        let mut active=self.active.lock().await;
        let lease=active.as_ref().ok_or_else(||anyhow::anyhow!("Preview expired"))?;
        if lease.id!=id{bail!("Stale preview id");}
        if lease.deadline.is_some_and(|deadline|tokio::time::Instant::now()>=deadline){bail!("Preview expired; automatic recovery will restore the layout");}
        if !equivalent(&self.driver.read().await?,&lease.draft){bail!("Display state changed since preview; keep refused");}
        if lease.deadline.is_some_and(|deadline|tokio::time::Instant::now()>=deadline){bail!("Preview expired during readback; keep refused");}
        self.clear_journal()?;*active=None;Ok(())
    }
    pub async fn recover(&self)->Result<bool> {
        if !self.journal.exists(){return Ok(false);}
        nirava_core::transaction::ensure_regular_path(&self.journal)?;
        if std::fs::metadata(&self.journal)?.len()>4_194_304{bail!("Display recovery journal is too large");}
        let lease:Lease=serde_json::from_slice(&std::fs::read(&self.journal)?)?;
        if lease.session_key!=session_key(){bail!("Pending preview belongs to another session; recovery did not touch displays");}
        self.restore(&lease).await?;self.clear_journal()?;Ok(true)
    }
    async fn restore(&self,lease:&Lease)->Result<()> {
        tokio::time::timeout(Duration::from_secs(16),self.restore_inner(lease)).await
            .map_err(|_|anyhow::anyhow!("Display recovery timed out; the journal is retained for retry"))?
    }
    async fn restore_inner(&self,lease:&Lease)->Result<()> {
        let current=self.driver.read().await?;
        let mut restore=current.iter().map(|o|lease.baseline.iter().find(|b|b.connector==o.connector&&b.identity==o.identity).cloned().unwrap_or_else(||o.clone())).collect::<Vec<_>>();
        if restore.is_empty(){bail!("No connected display remains for recovery");}
        if !restore.iter().any(|o|o.enabled){restore[0].enabled=true;if !restore[0].modes.contains(&restore[0].mode){restore[0].mode=restore[0].modes.first().cloned().ok_or_else(||anyhow::anyhow!("Recovery display has no modes"))?;}}
        for output in &mut restore {
            let live=current.iter().find(|o|o.connector==output.connector).ok_or_else(||anyhow::anyhow!("Recovery topology changed"))?;
            if output.enabled&&!live.modes.contains(&output.mode){output.mode=live.modes.first().cloned().ok_or_else(||anyhow::anyhow!("Original display mode no longer exists"))?;}
        }
        self.driver.apply(&restore).await?;
        if !equivalent(&self.driver.read().await?,&restore){bail!("Could not verify restored display state");}
        Ok(())
    }
    fn write_journal(&self,lease:&Lease)->Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        use std::io::Write;
        let parent=self.journal.parent().ok_or_else(||anyhow::anyhow!("Journal parent missing"))?;
        std::fs::create_dir_all(parent)?;std::fs::set_permissions(parent,std::os::unix::fs::PermissionsExt::from_mode(0o700))?;
        let mut file=std::fs::OpenOptions::new().create_new(true).write(true).mode(0o600).open(&self.journal)?;
        file.write_all(&serde_json::to_vec(lease)?)?;file.sync_all()?;std::fs::File::open(parent)?.sync_all()?;Ok(())
    }
    fn clear_journal(&self)->Result<()>{if self.journal.exists(){std::fs::remove_file(&self.journal)?;if let Some(parent)=self.journal.parent(){std::fs::File::open(parent)?.sync_all()?;}}Ok(())}
}

fn equivalent(a:&[OutputState],b:&[OutputState])->bool {
    a.len()==b.len()&&a.iter().all(|o|b.iter().any(|p|p.connector==o.connector&&p.identity==o.identity&&p.enabled==o.enabled&&(!o.enabled||(p.mode==o.mode&&(p.scale-o.scale).abs()<0.01&&p.x==o.x&&p.y==o.y&&p.transform==o.transform&&p.vrr_enabled==o.vrr_enabled))))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone)]struct Fake {state:Arc<Mutex<Vec<OutputState>>>,fail_apply:Arc<std::sync::atomic::AtomicBool>,read_delay:Arc<std::sync::atomic::AtomicU64>}
    #[async_trait]impl OutputDriver for Fake {
        async fn read(&self)->Result<Vec<OutputState>>{let delay=self.read_delay.load(std::sync::atomic::Ordering::SeqCst);if delay>0{tokio::time::sleep(Duration::from_millis(delay)).await;}Ok(self.state.lock().await.clone())}
        async fn apply(&self,layout:&[OutputState])->Result<()>{if self.fail_apply.swap(false,std::sync::atomic::Ordering::SeqCst){bail!("Temporary driver failure");}*self.state.lock().await=layout.to_vec();Ok(())}
    }
    fn setup(dir:&std::path::Path)->(Arc<DisplayGuard<Fake>>,Fake,Vec<OutputState>){let baseline=nirava_core::Snapshot::demo().displays.outputs;let fake=Fake{state:Arc::new(Mutex::new(baseline.clone())),fail_apply:Arc::new(std::sync::atomic::AtomicBool::new(false)),read_delay:Arc::new(std::sync::atomic::AtomicU64::new(0))};(DisplayGuard::new(fake.clone(),dir.join("pending.json")),fake,baseline)}
    #[tokio::test(start_paused=true)]async fn watchdog_reverts_without_gui()->Result<()> {
        let dir=tempfile::tempdir()?;let(guard,fake,baseline)=setup(dir.path());let mut draft=baseline.clone();draft[0].scale=1.5;
        guard.preview(draft,Duration::from_secs(15)).await?;tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(16)).await;tokio::task::yield_now().await;
        assert!(equivalent(&fake.read().await?,&baseline));assert!(guard.active().await.is_none());Ok(())
    }
    #[tokio::test(start_paused=true)]async fn watchdog_retries_a_transient_recovery_failure()->Result<()> {
        let dir=tempfile::tempdir()?;let(guard,fake,baseline)=setup(dir.path());let mut draft=baseline.clone();draft[0].scale=1.5;
        guard.preview(draft,Duration::from_secs(15)).await?;
        fake.fail_apply.store(true,std::sync::atomic::Ordering::SeqCst);tokio::task::yield_now().await;
        tokio::time::advance(Duration::from_secs(16)).await;tokio::task::yield_now().await;
        assert!(guard.active().await.is_some());assert!(dir.path().join("pending.json").exists());
        tokio::time::advance(Duration::from_secs(2)).await;tokio::task::yield_now().await;
        assert!(guard.active().await.is_none());assert!(equivalent(&fake.read().await?,&baseline));Ok(())
    }
    #[tokio::test(start_paused=true)]async fn keep_refuses_readback_that_crosses_the_deadline()->Result<()> {
        let dir=tempfile::tempdir()?;let(guard,fake,baseline)=setup(dir.path());let mut draft=baseline;draft[0].scale=1.5;
        let lease=guard.preview(draft,Duration::from_secs(15)).await?;
        fake.read_delay.store(16000,std::sync::atomic::Ordering::SeqCst);
        assert!(guard.keep_session_only(&lease.id).await.is_err());
        assert!(dir.path().join("pending.json").exists());Ok(())
    }
    #[tokio::test]async fn process_restart_recovers_from_journal()->Result<()> {
        let dir=tempfile::tempdir()?;let(guard,fake,baseline)=setup(dir.path());let mut draft=baseline.clone();draft[0].scale=1.25;
        guard.preview(draft,Duration::from_secs(15)).await?;
        let recovered=DisplayGuard::new(fake.clone(),dir.path().join("pending.json"));assert!(recovered.recover().await?);
        assert!(equivalent(&fake.read().await?,&baseline));Ok(())
    }
    #[tokio::test]async fn hotplug_preserves_new_display_during_revert()->Result<()> {
        let dir=tempfile::tempdir()?;let(guard,fake,baseline)=setup(dir.path());let mut draft=baseline.clone();draft[0].scale=1.5;
        let lease=guard.preview(draft,Duration::from_secs(15)).await?;
        let mut added=baseline[0].clone();added.connector="DP-2".into();added.identity="New monitor".into();added.x=3440;
        fake.state.lock().await.push(added.clone());guard.revert(&lease.id).await?;
        let current=fake.read().await?;assert_eq!(current.len(),2);assert!(current.contains(&added));Ok(())
    }
    #[tokio::test]async fn stale_keep_cannot_persist_a_different_layout()->Result<()> {
        let dir=tempfile::tempdir()?;let(guard,fake,baseline)=setup(dir.path());let mut draft=baseline;draft[0].scale=1.5;
        let lease=guard.preview(draft,Duration::from_secs(15)).await?;fake.state.lock().await[0].scale=2.0;
        assert!(guard.keep_session_only(&lease.id).await.is_err());Ok(())
    }
}
