use crate::{command, state_home};
use plain_core::{PowerState,SessionState};
use anyhow::Result;

#[zbus::proxy(interface="org.freedesktop.login1.Manager",default_service="org.freedesktop.login1",default_path="/org/freedesktop/login1")]
trait LoginManager {
    fn can_suspend(&self)->zbus::Result<String>;
    fn can_hibernate(&self)->zbus::Result<String>;
    fn list_inhibitors(&self)->zbus::Result<Vec<(String,String,String,String,u32,u32)>>;
    fn inhibit(&self,what:&str,who:&str,why:&str,mode:&str)->zbus::Result<zbus::zvariant::OwnedFd>;
}

pub fn noctalia_config() -> std::path::PathBuf {state_home().join("noctalia/settings.toml")}

pub async fn read(session:&SessionState)->PowerState {
    let mut state=PowerState::default();
    if session.idle_owners.len()==1 {state.owner=session.idle_owners[0].clone();}
    if state.owner=="noctalia" {
        match std::fs::read_to_string(noctalia_config()).and_then(|text|plain_core::config::read_noctalia(&text).map_err(std::io::Error::other)) {
            Ok(settings)=>state.settings=settings,Err(e)=>state.error=e.to_string(),
        }
        if let Ok(status)=command("noctalia",&["msg","status"]).await {
            if let Ok(value)=serde_json::from_str::<serde_json::Value>(&status) {state.locked=value["locked"].as_bool().unwrap_or(false);}
        }
        // An absent flag is Noctalia's documented enabled default. The live command is still opt-in.
        state.lock_enabled=std::fs::read_to_string(noctalia_config()).ok().and_then(|s|s.parse::<toml_edit::DocumentMut>().ok())
            .map(|d|d.get("lockscreen").and_then(|i|i.get("enabled")).and_then(|i|i.as_bool()).unwrap_or(true)).unwrap_or(false);
    } else if state.owner=="hypridle" {
        state.error="hypridle configuration import and guided service integration are not enabled in this preview".into();
    }
    if let Ok(connection)=zbus::Connection::system().await {
        if let Ok(proxy)=LoginManagerProxy::new(&connection).await {
            state.can_suspend=proxy.can_suspend().await.unwrap_or_else(|_|"unknown".into());
            state.can_hibernate=proxy.can_hibernate().await.unwrap_or_else(|_|"unknown".into());
            state.inhibitors=proxy.list_inhibitors().await.unwrap_or_default().into_iter()
                .map(|(what,who,why,mode,_,_)|format!("{who}: {what} ({mode}) — {why}")).collect();
        }
    }
    state
}

pub async fn caffeine()->Result<zbus::zvariant::OwnedFd> {
    let connection=zbus::Connection::system().await?;
    Ok(LoginManagerProxy::new(&connection).await?.inhibit("idle","Plain","Plain caffeine","block").await?)
}
