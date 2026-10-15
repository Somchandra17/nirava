use crate::{command, niri, power};
use plain_core::*;

pub async fn discover() -> Snapshot {
    let mut snapshot = Snapshot::default();
    snapshot.session.os = std::fs::read_to_string("/etc/os-release").unwrap_or_default().lines()
        .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|x| x.trim_matches('"').to_string())).unwrap_or_else(|| "Linux".into());
    snapshot.session.session_id = std::env::var("XDG_SESSION_ID").unwrap_or_default();
    if std::env::var_os("NIRI_SOCKET").is_some() {
        match command("niri", &["msg","version"]).await {
            Ok(version) => { snapshot.session.compositor = "Niri".into(); snapshot.session.version = version.trim().into(); }
            Err(e) => snapshot.session.diagnostics.push(format!("Niri IPC: {e}")),
        }
    }
    if snapshot.session.compositor.is_empty() && std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some() {
        match command("hyprctl", &["-j","version"]).await {
            Ok(version) => {
                snapshot.session.compositor = "Hyprland".into();
                snapshot.session.version = serde_json::from_str::<serde_json::Value>(&version).ok()
                    .and_then(|v|v["tag"].as_str().map(str::to_owned)).unwrap_or_else(|| "Unknown version".into());
            }
            Err(e) => snapshot.session.diagnostics.push(format!("Hyprland IPC: {e}")),
        }
    }
    if snapshot.session.compositor.is_empty() { snapshot.session.compositor = "Unknown Wayland session".into(); }
    if let Ok(proc) = command("ps", &["-u", &std::env::var("USER").unwrap_or_default(), "-o", "comm="]).await {
        for owner in ["noctalia","hypridle","swayidle"] {
            for _ in proc.lines().filter(|line|line.trim()==owner) { snapshot.session.idle_owners.push(owner.into()); }
        }
        for owner in ["kanshi","shikane"] {
            if proc.lines().any(|line|line.trim()==owner) { snapshot.session.display_owners.push(owner.into()); }
        }
    }
    if snapshot.session.idle_owners.len()>1 { snapshot.session.diagnostics.push("Multiple idle engines detected; Power writes are blocked".into()); }
    if snapshot.session.compositor == "Niri" {
        match niri::read_outputs().await { Ok(outputs) => snapshot.displays.outputs=outputs, Err(e)=> snapshot.displays.error=e.to_string() }
        snapshot.displays.persistence_owner = if snapshot.session.display_owners.is_empty() {"Niri configuration".into()} else {snapshot.session.display_owners.join(", ")};
    } else if snapshot.session.compositor == "Hyprland" {
        match niri::read_hyprland_outputs().await { Ok(outputs)=>snapshot.displays.outputs=outputs, Err(e)=>snapshot.displays.error=e.to_string() }
        snapshot.displays.persistence_owner=snapshot.session.display_owners.join(", ");
    } else { snapshot.displays.error="A tested output adapter is not available for this compositor yet".into(); }
    snapshot.power=power::read(&snapshot.session).await;
    snapshot.capabilities=vec![
        Capability { id:"displays.read".into(),supported:!snapshot.displays.outputs.is_empty(),backend:snapshot.session.compositor.clone(),reason:snapshot.displays.error.clone() },
        Capability { id:"displays.write".into(),supported:false,backend:snapshot.displays.persistence_owner.clone(),reason:"Display writes require the supervised recovery integration; not enabled in the initial preview".into() },
        Capability { id:"power.read".into(),supported:!snapshot.power.owner.is_empty(),backend:snapshot.power.owner.clone(),reason:snapshot.power.error.clone() },
        Capability { id:"power.write".into(),supported:snapshot.power.owner=="noctalia"&&snapshot.session.idle_owners.len()==1&&snapshot.power.error.is_empty(),backend:snapshot.power.owner.clone(),reason:"Read-only by default; a reviewed change and explicit commit are required".into() },
    ];
    snapshot
}
