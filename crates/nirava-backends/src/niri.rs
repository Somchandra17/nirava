use crate::command;
use anyhow::{Result, bail};
use nirava_core::{OutputMode, OutputState};
use serde_json::Value;

pub async fn read_outputs() -> Result<Vec<OutputState>> {
    parse_outputs(&command("niri",&["msg","--json","outputs"]).await?)
}

pub fn parse_outputs(text: &str) -> Result<Vec<OutputState>> {
    let value: Value=serde_json::from_str(text)?;
    let object=value.as_object().ok_or_else(||anyhow::anyhow!("Expected Niri output map"))?;
    object.iter().map(|(connector,o)| {
        let modes=o["modes"].as_array().ok_or_else(||anyhow::anyhow!("Missing output modes"))?
            .iter().map(|m|OutputMode {width:m["width"].as_u64().unwrap_or(0) as u32,height:m["height"].as_u64().unwrap_or(0) as u32,
                refresh_millihz:m["refresh_rate"].as_u64().unwrap_or(0) as u32}).collect::<Vec<_>>();
        let current=o["current_mode"].as_u64().and_then(|n|modes.get(n as usize)).cloned();
        let identity=["make","model","serial"].iter().filter_map(|k|o[*k].as_str()).collect::<Vec<_>>().join(" ");
        Ok(OutputState {connector:connector.clone(),identity,enabled:current.is_some(),mode:current.unwrap_or_default(),modes,
            scale:o["logical"]["scale"].as_f64().unwrap_or(1.0),x:o["logical"]["x"].as_i64().unwrap_or(0) as i32,
            y:o["logical"]["y"].as_i64().unwrap_or(0) as i32,transform:o["logical"]["transform"].as_str().unwrap_or("Normal").into(),
            vrr_supported:o["vrr_supported"].as_bool().unwrap_or(false),vrr_enabled:o["vrr_enabled"].as_bool().unwrap_or(false)})
    }).collect()
}

pub async fn read_hyprland_outputs() -> Result<Vec<OutputState>> {
    let value:Value=serde_json::from_str(&command("hyprctl",&["-j","monitors","all"]).await?)?;
    let entries=value.as_array().ok_or_else(||anyhow::anyhow!("Expected Hyprland output list"))?;
    Ok(entries.iter().map(|o|OutputState {connector:o["name"].as_str().unwrap_or_default().into(),
        identity:o["description"].as_str().unwrap_or_default().into(),enabled:!o["disabled"].as_bool().unwrap_or(false),
        mode:OutputMode {width:o["width"].as_u64().unwrap_or(0) as u32,height:o["height"].as_u64().unwrap_or(0) as u32,
            refresh_millihz:(o["refreshRate"].as_f64().unwrap_or(0.0)*1000.0).round() as u32},
        scale:o["scale"].as_f64().unwrap_or(1.0),x:o["x"].as_i64().unwrap_or(0) as i32,y:o["y"].as_i64().unwrap_or(0) as i32,
        transform:o["transform"].to_string(),..Default::default()}).collect())
}

pub fn validate_layout(baseline:&[OutputState],draft:&[OutputState]) -> Result<()> {
    if baseline.len()!=draft.len() {bail!("Output topology changed; start a fresh preview");}
    if !draft.iter().any(|o|o.enabled) {bail!("At least one display must remain enabled");}
    let mut seen=std::collections::HashSet::new();
    for output in draft {
        if !seen.insert(&output.connector) {bail!("Duplicate output in layout");}
        let original=baseline.iter().find(|o|o.connector==output.connector).ok_or_else(||anyhow::anyhow!("Output disappeared"))?;
        if output.identity!=original.identity{bail!("Output identity cannot be changed by a layout request");}
        if output.modes!=original.modes||output.vrr_supported!=original.vrr_supported{bail!("Output capabilities changed; request a fresh inventory");}
        if !(-65535..=65535).contains(&output.x)||!(-65535..=65535).contains(&output.y){bail!("Display position is outside the supported range");}
        if !output.enabled {continue;}
        if !output.scale.is_finite() || !(0.5..=4.0).contains(&output.scale) {bail!("Scale must be between 0.5 and 4");}
        if !original.modes.contains(&output.mode) {bail!("Mode was not advertised by {}",output.connector);}
        if output.vrr_enabled&&!original.vrr_supported {bail!("VRR is not supported on this output");}
        if !["Normal","90","180","270","Flipped","Flipped90","Flipped180","Flipped270"].contains(&output.transform.as_str()) {bail!("Unsupported output transform");}
    }
    Ok(())
}

pub async fn apply_layout(outputs:&[OutputState])->Result<()> {
    // Enable destinations before disabling sources. This is a guarded sequence, not an atomic transaction.
    for output in outputs.iter().filter(|o|o.enabled) {
        command("niri",&["msg","output",&output.connector,"on"]).await?;
        let mode=format!("{}x{}@{:.3}",output.mode.width,output.mode.height,output.mode.refresh_millihz as f64/1000.0);
        command("niri",&["msg","output",&output.connector,"mode",&mode]).await?;
        command("niri",&["msg","output",&output.connector,"scale",&output.scale.to_string()]).await?;
        command("niri",&["msg","output",&output.connector,"position","set",&output.x.to_string(),&output.y.to_string()]).await?;
        let transform=match output.transform.as_str(){"Normal"=>"normal","Flipped"=>"flipped","Flipped90"=>"flipped-90","Flipped180"=>"flipped-180","Flipped270"=>"flipped-270",s=>s};
        command("niri",&["msg","output",&output.connector,"transform",transform]).await?;
        if output.vrr_supported{command("niri",&["msg","output",&output.connector,"vrr",if output.vrr_enabled{"on"}else{"off"}]).await?;}
    }
    for output in outputs.iter().filter(|o|!o.enabled){command("niri",&["msg","output",&output.connector,"off"]).await?;}
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parse_disabled_and_active_outputs() -> Result<()> {
        let fixture=r#"{"DP-1":{"make":"Example","model":"Screen","serial":"test","modes":[{"width":1920,"height":1080,"refresh_rate":60000}],"current_mode":0,"logical":{"x":0,"y":0,"scale":1,"transform":"Normal"}},"DP-2":{"modes":[],"current_mode":null,"logical":null}}"#;
        let outputs=parse_outputs(fixture)?;assert!(outputs[0].enabled);assert!(!outputs[1].enabled);Ok(())
    }
    #[test]
    fn reject_disabling_every_monitor() {
        let baseline=nirava_core::Snapshot::demo().displays.outputs;
        let mut draft=baseline.clone();draft[0].enabled=false;
        assert!(validate_layout(&baseline,&draft).is_err());
    }
    #[test]
    fn reject_capability_changes_and_extreme_coordinates() {
        let baseline=nirava_core::Snapshot::demo().displays.outputs;let mut draft=baseline.clone();draft[0].vrr_supported=!draft[0].vrr_supported;
        assert!(validate_layout(&baseline,&draft).is_err());draft=baseline.clone();draft[0].x=i32::MAX;
        assert!(validate_layout(&baseline,&draft).is_err());
    }
    #[test]
    fn reject_unadvertised_mode() {
        let baseline=nirava_core::Snapshot::demo().displays.outputs;
        let mut draft=baseline.clone();draft[0].mode.width=9999;
        assert!(validate_layout(&baseline,&draft).is_err());
    }
}
