//! Native PulseAudio protocol client. PipeWire's Pulse server is supported.
//! All Context/mainloop operations run on one worker; GTK never blocks on them.
use anyhow::{Result, anyhow, bail};
use libpulse_binding as pulse;
use pulse::{callbacks::ListResult, context::{Context,FlagSet,State}, mainloop::threaded::Mainloop, volume::{ChannelVolumes,Volume}};
use nirava_core::{AudioDevice,AudioStream,SoundState};
use std::sync::{Arc,atomic::{AtomicU64,Ordering},mpsc::{self,Sender,Receiver}};
use std::time::Duration;

type Reply<T> = Sender<std::result::Result<T,String>>;
enum Request {
    Snapshot(Reply<SoundState>),
    Volume{ generation:u64,id:u32,input:bool,stream:bool,volume:f64,expires_at:std::time::Instant,reply:Reply<()> },
    Mute{ generation:u64,id:u32,input:bool,stream:bool,muted:bool,expires_at:std::time::Instant,reply:Reply<()> },
    Default{ generation:u64,name:String,input:bool,expires_at:std::time::Instant,reply:Reply<()> },
}

#[derive(Clone)]
pub struct AudioClient { tx:Sender<Request> }

impl Default for AudioClient {fn default()->Self{Self::new()}}
impl AudioClient {
    pub fn new()->Self {
        let (tx,rx)=mpsc::channel();
        std::thread::Builder::new().name("nirava-audio".into()).spawn(move||worker(rx)).expect("audio worker creation");
        Self{tx}
    }
    pub fn snapshot(&self)->Result<SoundState> {
        let (tx,rx)=mpsc::channel();self.tx.send(Request::Snapshot(tx))?;receive(rx)
    }
    pub fn set_volume(&self,generation:u64,id:u32,input:bool,stream:bool,volume:f64)->Result<()> {
        if !volume.is_finite()||!(0.0..=1.0).contains(&volume){bail!("Volume must be between 0 and 100%");}
        let (tx,rx)=mpsc::channel();self.tx.send(Request::Volume{generation,id,input,stream,volume,expires_at:std::time::Instant::now()+Duration::from_secs(10),reply:tx})?;receive(rx)
    }
    pub fn set_mute(&self,generation:u64,id:u32,input:bool,stream:bool,muted:bool)->Result<()> {
        let (tx,rx)=mpsc::channel();self.tx.send(Request::Mute{generation,id,input,stream,muted,expires_at:std::time::Instant::now()+Duration::from_secs(10),reply:tx})?;receive(rx)
    }
    pub fn set_default(&self,generation:u64,name:String,input:bool)->Result<()> {
        let (tx,rx)=mpsc::channel();self.tx.send(Request::Default{generation,name,input,expires_at:std::time::Instant::now()+Duration::from_secs(10),reply:tx})?;receive(rx)
    }
}
fn receive<T>(rx:Receiver<std::result::Result<T,String>>)->Result<T> {
    rx.recv_timeout(Duration::from_secs(12)).map_err(|_|anyhow!("Audio reply timed out; the outcome may be unknown. Refresh before retrying"))?.map_err(anyhow::Error::msg)
}

fn worker(rx:Receiver<Request>) {
    // Reconnect on subsequent requests after an unavailable/restarted server.
    let mut connected:Option<(Mainloop,Context)>=None;
    let revision=Arc::new(AtomicU64::new(1));
    let mut cache:Option<(u64,SoundState)>=None;
    // Tokens also differ when the whole backend restarts, not only on reconnect.
    let generation=Arc::new(AtomicU64::new(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos() as u64));
    while let Ok(request)=rx.recv() {
        let disconnected=if let Some((ml,ctx))=connected.as_mut() {
            ml.lock();let state=ctx.get_state();ml.unlock();
            state!=State::Ready
        }else{false};
        if disconnected{disconnect(&mut connected);}
        if connected.is_none(){connected=connect(revision.clone(),generation.clone()).ok();cache=None;if connected.is_some(){generation.fetch_add(1,Ordering::SeqCst);}}
        let result = match request {
            Request::Snapshot(reply)=> {
                let version=revision.load(Ordering::Relaxed);
                let token=generation.load(Ordering::SeqCst);
                let state=if let Some((saved,state))=&cache {if *saved==version{Ok(state.clone())}else{connected.as_mut().ok_or_else(||anyhow!("Audio server unavailable")).and_then(|(ml,ctx)|snapshot(ml,ctx))}}
                    else{connected.as_mut().ok_or_else(||anyhow!("PulseAudio/PipeWire server is unavailable")).and_then(|(ml,ctx)|snapshot(ml,ctx))};
                let state=state.and_then(|mut state|{check_generation(token,generation.load(Ordering::SeqCst))?;state.generation=token;Ok(state)});
                if let Ok(state)=&state{cache=Some((version,state.clone()));}
                let _=reply.send(state.map_err(|e|e.to_string()));None
            }
            Request::Volume{generation:expected,id,input,stream,volume,expires_at,reply}=>{
                let res=check_write(expected,&generation,expires_at).and_then(|()|connected.as_mut().ok_or_else(||anyhow!("Audio server unavailable")))
                    .and_then(|(ml,ctx)|set_volume(ml,ctx,id,input,stream,volume,&generation,expected,expires_at));Some((reply,res))
            }
            Request::Mute{generation:expected,id,input,stream,muted,expires_at,reply}=>{
                let res=check_write(expected,&generation,expires_at).and_then(|()|connected.as_mut().ok_or_else(||anyhow!("Audio server unavailable")))
                    .and_then(|(ml,ctx)|set_mute(ml,ctx,id,input,stream,muted,&generation,expected,expires_at));Some((reply,res))
            }
            Request::Default{generation:expected,name,input,expires_at,reply}=>{
                let res=check_write(expected,&generation,expires_at).and_then(|()|connected.as_mut().ok_or_else(||anyhow!("Audio server unavailable")))
                    .and_then(|(ml,ctx)|set_default(ml,ctx,&name,input,&generation,expected,expires_at));Some((reply,res))
            }
        };
        if let Some((reply,result))=result{cache=None;let _=reply.send(result.map_err(|e|e.to_string()));}
    }
    disconnect(&mut connected);
}

fn disconnect(connection:&mut Option<(Mainloop,Context)>) {
    if let Some((mut ml,mut ctx))=connection.take(){
        ml.lock();ctx.disconnect();drop(ctx);ml.unlock();ml.stop();
    }
}

fn check_generation(expected:u64,current:u64)->Result<()> {
    if expected==0||expected!=current{bail!("Audio server or device topology changed; refresh before retrying");}Ok(())
}

fn check_write(expected:u64,generation:&AtomicU64,expires_at:std::time::Instant)->Result<()> {
    if std::time::Instant::now()>=expires_at {bail!("Audio request expired in the queue; no new write was started");}
    check_generation(expected,generation.load(Ordering::SeqCst))
}

fn connect(revision:Arc<AtomicU64>,generation:Arc<AtomicU64>)->Result<(Mainloop,Context)> {
    let mut ml=Mainloop::new().ok_or_else(||anyhow!("Cannot create audio loop"))?;
    let mut ctx=Context::new(&ml,"Nirava settings").ok_or_else(||anyhow!("Cannot create audio context"))?;
    ctx.connect(None,FlagSet::NOAUTOSPAWN,None).map_err(|e|anyhow!("Audio connect: {e:?}"))?;
    ml.start().map_err(|_|anyhow!("Cannot start audio loop"))?;
    let deadline=std::time::Instant::now()+Duration::from_secs(3);
    loop {
        ml.lock();let state=ctx.get_state();ml.unlock();
        if state==State::Ready{break;}
        if matches!(state,State::Failed|State::Terminated)||std::time::Instant::now()>deadline {ml.lock();ctx.disconnect();ml.unlock();ml.stop();bail!("Audio server did not become ready");}
        std::thread::sleep(Duration::from_millis(10));
    }
    let (tx,rx)=mpsc::channel();
    ml.lock();ctx.set_subscribe_callback(Some(Box::new(move|facility,operation,_|{revision.fetch_add(1,Ordering::Relaxed);if topology_changed(facility,operation){generation.fetch_add(1,Ordering::SeqCst);}})));let mut subscription=ctx.subscribe(pulse::context::subscribe::InterestMaskSet::ALL,move|success|{let _=tx.send(success);});ml.unlock();
    let subscribed=rx.recv_timeout(Duration::from_secs(2));ml.lock();subscription.cancel();drop(subscription);ml.unlock();
    if subscribed!=Ok(true){ml.lock();ctx.disconnect();ml.unlock();ml.stop();bail!("Audio server did not acknowledge change subscriptions");}
    Ok((ml,ctx))
}

fn topology_changed(facility:Option<pulse::context::subscribe::Facility>,operation:Option<pulse::context::subscribe::Operation>)->bool {
    use pulse::context::subscribe::{Facility,Operation};
    matches!(facility,Some(Facility::Sink|Facility::Source|Facility::SinkInput))&&matches!(operation,Some(Operation::New|Operation::Removed))
}
fn channels(volumes:&ChannelVolumes)->Vec<u32>{volumes.get().iter().map(|v|v.0).collect()}
fn percent(volumes:&ChannelVolumes)->f64{volumes.max().0 as f64/Volume::NORMAL.0 as f64}

fn snapshot(ml:&mut Mainloop,ctx:&mut Context)->Result<SoundState> {
    let mut state=SoundState{available:true,..Default::default()};
    let (tx,rx)=mpsc::channel();
    ml.lock();let mut op=ctx.introspect().get_server_info(move|info|{
        let _=tx.send((info.server_name.as_deref().unwrap_or_default().to_string(),info.default_sink_name.as_deref().unwrap_or_default().to_string(),info.default_source_name.as_deref().unwrap_or_default().to_string()));
    });ml.unlock();
    let result=rx.recv_timeout(Duration::from_secs(2));ml.lock();op.cancel();drop(op);ml.unlock();
    let (server,output,input)=result?;state.server=server;state.default_output=output;state.default_input=input;
    let (tx,rx)=mpsc::channel();
    ml.lock();let mut op=ctx.introspect().get_sink_info_list(move|res|match res {
        ListResult::Item(info)=>{let _=tx.send(Ok(Some(AudioDevice{id:info.index,name:info.name.as_deref().unwrap_or_default().into(),
            description:info.description.as_deref().unwrap_or("Audio output").into(),volume:percent(&info.volume),channels:channels(&info.volume),
            muted:info.mute,input:false,port:info.active_port.as_ref().and_then(|p|p.description.as_deref()).unwrap_or_default().into()})));},
        ListResult::End=>{let _=tx.send(Ok(None));},ListResult::Error=>{let _=tx.send(Err("Audio enumeration failed".to_string()));}
    });ml.unlock();
    let result=collect(rx);ml.lock();op.cancel();drop(op);ml.unlock();state.devices.extend(result?);
    let (tx,rx)=mpsc::channel();
    ml.lock();let mut op=ctx.introspect().get_source_info_list(move|res|match res {
        ListResult::Item(info)=>{
            if info.monitor_of_sink.is_none(){let _=tx.send(Ok(Some(AudioDevice{id:info.index,name:info.name.as_deref().unwrap_or_default().into(),
                description:info.description.as_deref().unwrap_or("Audio input").into(),volume:percent(&info.volume),channels:channels(&info.volume),
                muted:info.mute,input:true,port:info.active_port.as_ref().and_then(|p|p.description.as_deref()).unwrap_or_default().into()})));}
        },ListResult::End=>{let _=tx.send(Ok(None));},ListResult::Error=>{let _=tx.send(Err("Audio enumeration failed".to_string()));}
    });ml.unlock();
    let result=collect(rx);ml.lock();op.cancel();drop(op);ml.unlock();state.devices.extend(result?);
    let (tx,rx)=mpsc::channel();
    ml.lock();let mut op=ctx.introspect().get_sink_input_info_list(move|res|match res {
        ListResult::Item(info)=>{let _=tx.send(Ok(Some(AudioStream{id:info.index,name:info.name.as_deref().unwrap_or("Playback").into(),
            application:info.proplist.get_str("application.name").unwrap_or_else(||"Application".into()),device:info.sink,
            volume:percent(&info.volume),channels:channels(&info.volume),muted:info.mute})));},ListResult::End=>{let _=tx.send(Ok(None));},ListResult::Error=>{let _=tx.send(Err("Audio enumeration failed".to_string()));}
    });ml.unlock();
    let result=collect(rx);ml.lock();op.cancel();drop(op);ml.unlock();state.streams=result?;
    Ok(state)
}

fn collect<T>(rx:Receiver<std::result::Result<Option<T>,String>> )->Result<Vec<T>> {
    let mut values=vec![];
    let deadline=std::time::Instant::now()+Duration::from_secs(2);
    loop {match rx.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now()))?.map_err(anyhow::Error::msg)? {
        Some(v)=>{if values.len()>=4096{bail!("Audio inventory is too large");}values.push(v)},None=>return Ok(values)
    }}
}

fn set_volume(ml:&mut Mainloop,ctx:&mut Context,id:u32,input:bool,stream:bool,volume:f64,generation:&AtomicU64,expected:u64,expires_at:std::time::Instant)->Result<()> {
    let state=snapshot(ml,ctx)?;
    check_write(expected,generation,expires_at)?;
    let saved=if stream{state.streams.iter().find(|s|s.id==id).map(|s|&s.channels)}
        else{state.devices.iter().find(|d|d.id==id&&d.input==input).map(|d|&d.channels)}.ok_or_else(||anyhow!("Audio target disappeared"))?;
    let mut channels=ChannelVolumes::default();channels.set_len(saved.len() as u8);
    for (dst,src) in channels.get_mut().iter_mut().zip(saved){*dst=Volume(*src);}
    channels.scale(Volume((volume*Volume::NORMAL.0 as f64).round() as u32));
    let (tx,rx)=mpsc::channel();
    ml.lock();let mut introspect=ctx.introspect();
    let callback=Some(Box::new(move|success|{let _=tx.send(success);}) as Box<dyn FnMut(bool)>);
    let mut op=if stream{introspect.set_sink_input_volume(id,&channels,callback)}else if input{introspect.set_source_volume_by_index(id,&channels,callback)}else{introspect.set_sink_volume_by_index(id,&channels,callback)};
    ml.unlock();let result=rx.recv_timeout(Duration::from_secs(2));ml.lock();op.cancel();drop(op);ml.unlock();
    if !result?{bail!("Audio server rejected the volume change");}
    let state=snapshot(ml,ctx)?;
    check_generation(expected,generation.load(Ordering::SeqCst))?;
    let actual=if stream{state.streams.iter().find(|s|s.id==id).map(|s|s.volume)}else{state.devices.iter().find(|d|d.id==id&&d.input==input).map(|d|d.volume)};
    if actual.is_none_or(|v|(v-volume).abs()>0.0001){bail!("Audio volume readback differs from the request; refresh before retrying");}Ok(())
}
fn set_mute(ml:&mut Mainloop,ctx:&mut Context,id:u32,input:bool,stream:bool,muted:bool,generation:&AtomicU64,expected:u64,expires_at:std::time::Instant)->Result<()> {
    let state=snapshot(ml,ctx)?;
    check_write(expected,generation,expires_at)?;
    let exists=if stream{state.streams.iter().any(|s|s.id==id)}else{state.devices.iter().any(|d|d.id==id&&d.input==input)};
    if !exists{bail!("Audio target disappeared");}
    let (tx,rx)=mpsc::channel();ml.lock();let mut introspect=ctx.introspect();
    let callback=Some(Box::new(move|success|{let _=tx.send(success);}) as Box<dyn FnMut(bool)>);
    let mut op=if stream{introspect.set_sink_input_mute(id,muted,callback)}else if input{introspect.set_source_mute_by_index(id,muted,callback)}else{introspect.set_sink_mute_by_index(id,muted,callback)};
    ml.unlock();let result=rx.recv_timeout(Duration::from_secs(2));ml.lock();op.cancel();drop(op);ml.unlock();
    if !result?{bail!("Audio server rejected the mute change");}
    let state=snapshot(ml,ctx)?;
    check_generation(expected,generation.load(Ordering::SeqCst))?;
    let actual=if stream{state.streams.iter().find(|s|s.id==id).map(|s|s.muted)}else{state.devices.iter().find(|d|d.id==id&&d.input==input).map(|d|d.muted)};
    if actual!=Some(muted){bail!("Audio mute readback differs from the request");}Ok(())
}
fn set_default(ml:&mut Mainloop,ctx:&mut Context,name:&str,input:bool,generation:&AtomicU64,expected:u64,expires_at:std::time::Instant)->Result<()> {
    let state=snapshot(ml,ctx)?;
    check_write(expected,generation,expires_at)?;
    if !state.devices.iter().any(|d|d.name==name&&d.input==input){bail!("Audio device disappeared");}
    let (tx,rx)=mpsc::channel();ml.lock();
    let callback=Box::new(move|success|{let _=tx.send(success);}) as Box<dyn FnMut(bool)>;
    let mut op=if input{ctx.set_default_source(name,callback)}else{ctx.set_default_sink(name,callback)};
    ml.unlock();let result=rx.recv_timeout(Duration::from_secs(2));ml.lock();op.cancel();drop(op);ml.unlock();
    if !result?{bail!("Audio server rejected the default device");}
    let state=snapshot(ml,ctx)?;
    check_generation(expected,generation.load(Ordering::SeqCst))?;
    if (if input{&state.default_input}else{&state.default_output})!=name{bail!("Default audio device readback differs from the request");}Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]fn stale_server_generation_is_rejected(){assert!(check_generation(1,2).is_err());assert!(check_generation(0,0).is_err());assert!(check_generation(2,2).is_ok());}
    #[test]fn expired_queued_writes_are_refused(){
        let generation=AtomicU64::new(2);
        assert!(check_write(2,&generation,std::time::Instant::now()-Duration::from_secs(1)).is_err());
        assert!(check_write(2,&generation,std::time::Instant::now()+Duration::from_secs(10)).is_ok());
    }
    #[test]fn enumeration_errors_are_not_empty_successes(){
        let(tx,rx)=mpsc::channel::<std::result::Result<Option<u32>,String>>();let _=tx.send(Err("server error".into()));
        assert!(collect(rx).is_err());
    }
    #[test]fn only_topology_changes_invalidate_device_tokens(){
        use pulse::context::subscribe::{Facility,Operation};
        assert!(topology_changed(Some(Facility::Sink),Some(Operation::Removed)));
        assert!(topology_changed(Some(Facility::SinkInput),Some(Operation::New)));
        assert!(!topology_changed(Some(Facility::Sink),Some(Operation::Changed)));
        assert!(!topology_changed(Some(Facility::Client),Some(Operation::New)));
    }
    #[test]fn scaling_preserves_channel_balance(){
        let mut channels=ChannelVolumes::default();channels.set_len(2);channels.get_mut()[0]=Volume(Volume::NORMAL.0);channels.get_mut()[1]=Volume(Volume::NORMAL.0/2);
        channels.scale(Volume(Volume::NORMAL.0/2));assert_eq!(channels.get()[0].0,Volume::NORMAL.0/2);assert_eq!(channels.get()[1].0,Volume::NORMAL.0/4);
    }
}
