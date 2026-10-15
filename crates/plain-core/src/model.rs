use serde::{Deserialize, Serialize};

pub const APP_ID: &str = "io.github.Somchandra17.Plain";
pub const BUS_NAME: &str = "io.github.Somchandra17.Plain.Session";
pub const OBJECT_PATH: &str = "/io/github/Somchandra17/Plain";
pub const SCHEMA_VERSION: u32 = 1;

pub fn session_key()->String {
    let context=["XDG_SESSION_ID","WAYLAND_DISPLAY","NIRI_SOCKET","HYPRLAND_INSTANCE_SIGNATURE"].map(|key|std::env::var(key).unwrap_or_default()).join("\0");
    crate::transaction::digest(context.as_bytes())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Capability {
    pub id: String,
    pub supported: bool,
    pub backend: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionState {
    pub compositor: String,
    pub version: String,
    pub os: String,
    pub session_id: String,
    pub idle_owners: Vec<String>,
    pub display_owners: Vec<String>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioDevice {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub volume: f64,
    pub muted: bool,
    pub input: bool,
    pub channels: Vec<u32>,
    pub port: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AudioStream {
    pub id: u32,
    pub name: String,
    pub application: String,
    pub device: u32,
    pub volume: f64,
    pub muted: bool,
    pub channels: Vec<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SoundState {
    #[serde(default)]
    pub generation: u64,
    pub available: bool,
    pub server: String,
    pub default_output: String,
    pub default_input: String,
    pub devices: Vec<AudioDevice>,
    pub streams: Vec<AudioStream>,
    pub error: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OutputMode {
    pub width: u32,
    pub height: u32,
    pub refresh_millihz: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OutputState {
    pub connector: String,
    pub identity: String,
    pub enabled: bool,
    pub mode: OutputMode,
    pub modes: Vec<OutputMode>,
    pub scale: f64,
    pub x: i32,
    pub y: i32,
    pub transform: String,
    pub vrr_supported: bool,
    pub vrr_enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DisplayState {
    pub outputs: Vec<OutputState>,
    pub persistence_owner: String,
    pub error: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct IdleSettings {
    pub screen_off_secs: u64,
    pub lock_secs: u64,
    pub suspend_secs: u64,
    pub lock_before_suspend: bool,
}

impl IdleSettings {
    pub fn validate(&self) -> Result<(), String> {
        for value in [self.screen_off_secs, self.lock_secs, self.suspend_secs] {
            if value > 86_400 { return Err("Idle durations cannot exceed 24 hours".into()); }
        }
        if self.suspend_secs != 0 && self.screen_off_secs > self.suspend_secs {
            return Err("Screen-off must not be later than automatic suspend".into());
        }
        if self.suspend_secs != 0 && self.lock_secs > self.suspend_secs {
            return Err("Automatic lock must not be later than automatic suspend".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PowerState {
    pub owner: String,
    pub settings: IdleSettings,
    pub can_suspend: String,
    pub can_hibernate: String,
    pub inhibitors: Vec<String>,
    pub locked: bool,
    pub lock_enabled: bool,
    pub caffeine: bool,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupInfo {
    pub id:String,
    pub path:String,
    pub backup:String,
    pub can_restore:bool,
    pub reason:String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    pub schema_version: u32,
    pub read_only: bool,
    pub session: SessionState,
    pub sound: SoundState,
    pub displays: DisplayState,
    pub power: PowerState,
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub backups: Vec<BackupInfo>,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self { schema_version: SCHEMA_VERSION, read_only: true, session: SessionState::default(),
            sound: SoundState::default(), displays: DisplayState::default(), power: PowerState::default(), capabilities: vec![],backups:vec![] }
    }
}

impl Snapshot {
    pub fn demo() -> Self {
        let mode = OutputMode { width: 3440, height: 1440, refresh_millihz: 160_000 };
        Self {
            schema_version: SCHEMA_VERSION,
            read_only: true,
            session: SessionState { compositor: "Niri (simulation)".into(), version: "26.04".into(),
                os: "Fedora Linux — demo data".into(), idle_owners: vec!["Noctalia".into()],
                display_owners: vec!["Niri configuration".into()], ..Default::default() },
            sound: SoundState { available: true, server: "PipeWire (simulation)".into(), default_output: "demo-headset".into(),
                default_input: "demo-microphone".into(), devices: vec![
                    AudioDevice { id: 1, name: "demo-headset".into(), description: "USB headset".into(), volume: 0.45, ..Default::default() },
                    AudioDevice { id: 2, name: "demo-microphone".into(), description: "Headset microphone".into(), input: true, volume: 0.65, ..Default::default() },
                ], streams: vec![AudioStream { id: 3, name: "Playback".into(), application: "Music".into(), volume: 0.7, ..Default::default() }], ..Default::default() },
            displays: DisplayState { outputs: vec![OutputState { connector: "DP-1".into(), identity: "Ultrawide display".into(),
                enabled: true, mode: mode.clone(), modes: vec![mode], scale: 1.0, transform: "Normal".into(), ..Default::default() }],
                persistence_owner: "Niri".into(), ..Default::default() },
            power: PowerState { owner: "Noctalia".into(), settings: IdleSettings { screen_off_secs: 1800, lock_secs: 0,
                suspend_secs: 3600, lock_before_suspend: true }, can_suspend: "yes".into(), can_hibernate: "na".into(), lock_enabled: true, ..Default::default() },
            capabilities: vec![],
            backups:vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationResult {
    pub id: String,
    pub status: String,
    pub message: String,
}

impl OperationResult {
    pub fn applied(message: impl Into<String>) -> Self {
        Self { id: String::new(), status: "applied".into(), message: message.into() }
    }
    pub fn failed(message: impl Into<String>) -> Self {
        Self { id: String::new(), status: "failed".into(), message: message.into() }
    }
}
