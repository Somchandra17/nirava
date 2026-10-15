use crate::IdleSettings;
use anyhow::{Result, bail};
use toml_edit::{DocumentMut, value};

pub fn read_noctalia(text: &str) -> Result<IdleSettings> {
    let doc = text.parse::<DocumentMut>()?;
    let behavior = doc.get("idle").and_then(|i|i.get("behavior"));
    let mut state = IdleSettings { lock_before_suspend: true, ..Default::default() };
    let mut found=[false;3];
    if let Some(table) = behavior.and_then(|i|i.as_table_like()) {
        for (_, item) in table.iter() {
            let action=item.get("action").and_then(|i|i.as_str()).unwrap_or("command");
            if !["screen_off","lock","lock_and_suspend","suspend"].contains(&action){continue;}
            let index=match action{"screen_off"=>0,"lock"=>1,_=>2};
            if found[index]{bail!("Multiple behaviors own this action; select a single owner");}found[index]=true;
            if action=="suspend"{state.lock_before_suspend=false;}
            if action=="lock_and_suspend"{state.lock_before_suspend=true;}
            let enabled=match item.get("enabled") {None=>true,Some(value)=>value.as_bool().ok_or_else(||anyhow::anyhow!("Known idle action has a non-boolean enabled flag"))?};
            if !enabled { continue; }
            let timeout = item.get("timeout").and_then(|i|i.as_integer().map(|n|n as f64).or_else(||i.as_float())).ok_or_else(||anyhow::anyhow!("Known idle action has no numeric timeout"))?;
            if !timeout.is_finite() || !(0.0..=86400.0).contains(&timeout) { bail!("Unsupported idle duration"); }
            if timeout.fract()!=0.0{bail!("Fractional idle durations must be edited in the existing settings owner");}
            let seconds = timeout as u64;
            match action {
                "screen_off" => state.screen_off_secs = seconds,
                "lock" => state.lock_secs = seconds,
                "lock_and_suspend" => { state.suspend_secs = seconds; state.lock_before_suspend = true; },
                "suspend" => { state.suspend_secs = seconds; state.lock_before_suspend = false; },
                _ => (),
            }
        }
    }
    Ok(state)
}

pub fn edit_noctalia(text: &str, settings: &IdleSettings) -> Result<String> {
    read_noctalia(text)?;
    settings.validate().map_err(anyhow::Error::msg)?;
    let mut doc = text.parse::<DocumentMut>()?;
    let behavior = doc.get_mut("idle").and_then(|idle|idle.get_mut("behavior")).and_then(|item|item.as_table_like_mut()).ok_or_else(|| anyhow::anyhow!("No known Noctalia idle behaviors; guided import is required"))?;
    let mut found = [false; 3];
    for (_, item) in behavior.iter_mut() {
        let action = item.get("action").and_then(|i|i.as_str()).unwrap_or("command").to_string();
        let (idx, secs) = match action.as_str() {
            "screen_off" => (0, settings.screen_off_secs),
            "lock" => (1, settings.lock_secs),
            "lock_and_suspend" | "suspend" => (2, settings.suspend_secs),
            _ => continue,
        };
        if found[idx] { bail!("Multiple behaviors own this action; select an owner before editing"); }
        found[idx] = true;
        let timeout=item.get("timeout").and_then(|i|i.as_integer().map(|n|n as f64).or_else(||i.as_float()));
        if secs!=0&&timeout!=Some(secs as f64){
            let replacement=if item.get("timeout").is_some_and(|i|i.is_integer()){value(secs as i64)}else{value(secs as f64)};
            replace_value(item,"timeout",replacement);
        }
        let enabled=item.get("enabled").and_then(|i|i.as_bool()).unwrap_or(true);
        if enabled!=(secs!=0){replace_value(item,"enabled",value(secs!=0));}
        if idx == 2 {
            let wanted=if settings.lock_before_suspend{"lock_and_suspend"}else{"suspend"};
            if action!=wanted{replace_value(item,"action",value(wanted));}
        }
    }
    for (idx,secs) in [settings.screen_off_secs,settings.lock_secs,settings.suspend_secs].into_iter().enumerate() {
        if secs != 0 && !found[idx] { bail!("Requested behavior does not exist; guided import is required"); }
    }
    let edited = doc.to_string(); read_noctalia(&edited)?; Ok(edited)
}

fn replace_value(item:&mut toml_edit::Item,key:&str,mut replacement:toml_edit::Item) {
    if let (Some(old),Some(new))=(item.get(key).and_then(|i|i.as_value()),replacement.as_value_mut()){
        *new.decor_mut()=old.decor().clone();
    }
    item[key]=replacement;
}

pub fn validate_niri_syntax(text: &str) -> Result<()> {
    // Accept the KDL versions used by niri. Compositor validation is also required at commit.
    if text.parse::<kdl::KdlDocument>().is_err() { kdl::KdlDocument::parse_v1(text)?; }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    const ORIGINAL: &str = "# hand edited\n[other]\nkeep = 'mine'\n[idle.behavior.screen-off]\naction = 'screen_off'\ntimeout = 1800.0\nenabled = true\n[idle.behavior.lock]\naction = 'lock'\ntimeout = 600.0\nenabled = false\n[idle.behavior.sleep]\naction = 'lock_and_suspend'\ntimeout = 3600.0\nenabled = true\n[idle.behavior.custom]\naction = 'command'\ncommand = 'notify-send hi'\ntimeout = 120.0\n";
    #[test]
    fn preserve_custom_settings_and_comments() -> Result<()> {
        let mut settings = read_noctalia(ORIGINAL)?;
        assert_eq!(settings.screen_off_secs, 1800); assert_eq!(settings.lock_secs, 0);
        settings.screen_off_secs = 600;
        let edited = edit_noctalia(ORIGINAL,&settings)?;
        assert!(edited.starts_with("# hand edited\n[other]\nkeep = 'mine'\n"));
        assert!(edited.contains("command = 'notify-send hi'"));
        assert_eq!(read_noctalia(&edited)?,settings); Ok(())
    }
    #[test]
    fn conflicting_owners_are_rejected() {
        let duplicate = format!("{ORIGINAL}\n[idle.behavior.other-off]\naction = 'screen_off'\ntimeout = 900\n");
        assert!(edit_noctalia(&duplicate,&IdleSettings::default()).is_err());
        assert!(read_noctalia(&duplicate).is_err());
    }
    #[test]
    fn timing_validation_rejects_invalid_order() {
        assert!(IdleSettings { screen_off_secs:900,suspend_secs:600,..Default::default() }.validate().is_err());
    }
    #[test]
    fn suspend_without_lock_is_reported_and_round_trips()->Result<()> {
        let mut settings=read_noctalia(ORIGINAL)?;settings.lock_before_suspend=false;
        let edited=edit_noctalia(ORIGINAL,&settings)?;
        assert_eq!(read_noctalia(&edited)?,settings);
        settings.suspend_secs=0;
        assert_eq!(read_noctalia(&edit_noctalia(ORIGINAL,&settings)?)?,settings);Ok(())
    }
    #[test]
    fn long_custom_behaviors_are_preserved()->Result<()> {
        let text=ORIGINAL.replace("timeout = 120.0","timeout = 200000.0");
        let edited=edit_noctalia(&text,&read_noctalia(&text)?)?;
        assert!(edited.contains("timeout = 200000.0"));Ok(())
    }
    #[test]
    fn idle_edits_preserve_inline_comments_and_unchanged_values()->Result<()> {
        let text=ORIGINAL.replace("timeout = 1800.0","timeout = 1800.0 # display timing").replace("enabled = false","enabled = false # locker disabled");
        let mut settings=read_noctalia(&text)?;
        assert_eq!(edit_noctalia(&text,&settings)?,text);
        settings.screen_off_secs=600;settings.lock_secs=300;
        let edited=edit_noctalia(&text,&settings)?;
        assert!(edited.contains("timeout = 600.0 # display timing"));assert!(edited.contains("enabled = true # locker disabled"));
        assert!(edited.contains("timeout = 3600.0"));Ok(())
    }
    #[test]
    fn fractional_durations_are_not_silently_rounded(){assert!(read_noctalia(&ORIGINAL.replace("1800.0","1800.5")).is_err());}
    #[test]
    fn malformed_idle_tables_and_enabled_flags_are_refused_without_panicking() {
        assert!(edit_noctalia("idle = 42",&IdleSettings::default()).is_err());
        assert!(edit_noctalia("[idle]\nbehavior = false",&IdleSettings::default()).is_err());
        assert!(read_noctalia(&ORIGINAL.replace("enabled = true","enabled = 'yes'")).is_err());
    }
    #[test]
    fn invalid_timeout_types_are_not_reported_as_disabled(){assert!(read_noctalia(&ORIGINAL.replace("1800.0","'30 minutes'")).is_err());}
}
