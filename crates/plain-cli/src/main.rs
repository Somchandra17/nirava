use anyhow::Result;
use clap::{Parser,Subcommand};
use plain_core::*;

#[derive(Parser)]
#[command(about="Inspect Plain's session and stage reviewed settings changes")]
struct Args {#[command(subcommand)] command:Action}

#[derive(Subcommand)]
enum Action {
    Status {#[arg(long)] json:bool,#[arg(long)] demo:bool},
    Doctor,
    StagePower {#[arg(long)] screen_off:u64,#[arg(long,default_value_t=0)] lock:u64,#[arg(long)] sleep:u64},
    Commit {id:String,#[arg(long)] yes:bool},
    Cancel {id:String},
    Restore {id:String,#[arg(long)] yes:bool},
    DisplayPreview {draft:std::path::PathBuf,#[arg(long)] yes:bool},
    DisplayRevert {id:String},
    DisplayKeepSession {id:String,#[arg(long)] yes:bool},
    Caffeine {#[arg(value_parser=["on","off"])] state:String},
    Suspend,
}

async fn proxy(connection:&zbus::Connection)->Result<zbus::Proxy<'_>> {
    let proxy=zbus::Proxy::new(connection,BUS_NAME,OBJECT_PATH,"io.github.Somchandra17.Plain.Manager1").await?;
    let key:String=proxy.call("GetSessionKey",&()).await?;
    if key!=plain_core::session_key(){anyhow::bail!("Plain backend belongs to a different graphical session");}
    Ok(proxy)
}
async fn service_snapshot()->Result<Snapshot> {
    let connection=zbus::Connection::session().await?;
    let result:String=proxy(&connection).await?.call("GetSnapshot",&()).await?;
    Ok(serde_json::from_str(&result)?)
}
async fn local_snapshot()->Snapshot {
    let mut snapshot=plain_backends::discovery::discover().await;
    #[cfg(feature="audio")]
    {let audio=plain_backends::audio::AudioClient::new();snapshot.sound=audio.snapshot().unwrap_or_else(|e|SoundState {error:e.to_string(),..Default::default()});}
    #[cfg(not(feature="audio"))]
    {snapshot.sound.error="Built without native audio support".into();}
    snapshot
}

#[tokio::main]
async fn main()->Result<()> {
    let args=Args::parse();
    match args.command {
        Action::Status {json,demo}=>{
            let snapshot=if demo {Snapshot::demo()} else {match service_snapshot().await{Ok(state)=>state,Err(_)=>local_snapshot().await}};
            if json {println!("{}",serde_json::to_string_pretty(&snapshot)?);} else {print_snapshot(&snapshot);}
        }
        Action::Doctor=>print_snapshot(&local_snapshot().await),
        Action::StagePower{screen_off,lock,sleep}=>{
            let settings=IdleSettings{screen_off_secs:screen_off,lock_secs:lock,suspend_secs:sleep,lock_before_suspend:true};
            settings.validate().map_err(anyhow::Error::msg)?;
            let connection=zbus::Connection::session().await?;
            let result:String=proxy(&connection).await?.call("StagePower",&(serde_json::to_string(&settings)?,)).await?;
            println!("{}",serde_json::to_string_pretty(&serde_json::from_str::<serde_json::Value>(&result)?)?);
        }
        Action::Commit{id,yes}=>{
            if !yes {anyhow::bail!("Review the staged change first. Pass --yes to commit the reviewed change");}
            let connection=zbus::Connection::session().await?;
            let result:String=proxy(&connection).await?.call("CommitChange",&(id,)).await?;
            print_result(&result)?;
        }
        Action::Cancel{id}=>{let connection=zbus::Connection::session().await?;proxy(&connection).await?.call::<_,_,()>("CancelChange",&(id,)).await?;}
        Action::Restore{id,yes}=>{if !yes{anyhow::bail!("Review the backup identifier and pass --yes to restore");}let connection=zbus::Connection::session().await?;let result:String=proxy(&connection).await?.call("RestoreChange",&(id,)).await?;print_result(&result)?;}
        Action::DisplayPreview{draft,yes}=>{
            if !yes{anyhow::bail!("Display preview changes monitor state. Review the draft and pass --yes");}
            let draft=std::fs::read_to_string(draft)?;let _:Vec<OutputState>=serde_json::from_str(&draft)?;
            let connection=zbus::Connection::session().await?;let result:String=proxy(&connection).await?.call("PreviewLayout",&(draft,)).await?;println!("{result}");
        }
        Action::DisplayRevert{id}=>{let connection=zbus::Connection::session().await?;let result:String=proxy(&connection).await?.call("RevertLayout",&(id,)).await?;print_result(&result)?;}
        Action::DisplayKeepSession{id,yes}=>{if !yes{anyhow::bail!("This keeps the layout only for this login; pass --yes");}let connection=zbus::Connection::session().await?;let result:String=proxy(&connection).await?.call("KeepLayoutForSession",&(id,)).await?;print_result(&result)?;}
        Action::Caffeine{state}=>{let connection=zbus::Connection::session().await?;let result:String=proxy(&connection).await?.call("SetCaffeine",&(state=="on",)).await?;print_result(&result)?;}
        Action::Suspend=>{let connection=zbus::Connection::session().await?;let result:String=proxy(&connection).await?.call("Suspend",&()).await?;print_result(&result)?;}
    }
    Ok(())
}
fn print_result(text:&str)->Result<()> {
    let result:OperationResult=serde_json::from_str(text)?;
    println!("{}: {}",result.status,result.message);
    if result.status=="failed"{anyhow::bail!("Operation was not applied");}Ok(())
}
fn print_snapshot(state:&Snapshot) {
    println!("Plain {} | {} | {}",env!("CARGO_PKG_VERSION"),state.session.os,state.session.compositor);
    println!("Mode: {}",if state.read_only{"read-only"}else{"explicit writes enabled"});
    println!("Idle owners: {}",state.session.idle_owners.join(", "));
    println!("Displays: {} ({})",state.displays.outputs.len(),state.displays.persistence_owner);
    for output in &state.displays.outputs {println!("  {}: {}×{} @ {:.2} Hz, scale {}",output.connector,output.mode.width,output.mode.height,output.mode.refresh_millihz as f64/1000.0,output.scale);}
    println!("Audio: {} devices; {}",state.sound.devices.len(),if state.sound.available{&state.sound.server}else{&state.sound.error});
    println!("Suspend: {}; Hibernate: {}",state.power.can_suspend,state.power.can_hibernate);
    for message in &state.session.diagnostics {println!("Diagnostic: {message}");}
    for capability in &state.capabilities {if !capability.reason.is_empty(){println!("{}: {}",capability.id,capability.reason);}}
}
