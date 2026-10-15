use adw::prelude::*;
use gtk::glib;
use nirava_core::*;
use std::{cell::RefCell,rc::Rc,sync::mpsc::{self,Sender},time::Duration};

#[derive(Clone)]
enum Action {
    Volume(u64,u32,bool,bool,f64),
    Mute(u64,u32,bool,bool,bool),
    Default(u64,String,bool),
    StagePower(IdleSettings),
    Commit(String),
    Cancel(String),
    Caffeine(bool),
    PreviewLayout(Vec<OutputState>),
    RevertLayout(String),
    KeepLayout(String),
    Restore(String),
    Suspend,
    Refresh,
    Stop,
}
enum Update {Snapshot(Snapshot),Message(String),Failure(String,Option<usize>),Preview(serde_json::Value),Layout(nirava_backends::display_guard::Lease)}

fn main() {
    let args=std::env::args().skip(1).collect::<Vec<_>>();
    if args.iter().any(|s|s=="--help") {
        println!("Nirava settings\nUsage: nirava [--demo] [--page displays|sound|power|system] [--smoke-test]\nDefault: read-only discovery. Writes require an explicitly writable backend.");return;
    }
    let demo=args.iter().any(|s|s=="--demo");
    let smoke=args.iter().any(|s|s=="--smoke-test");
    let capture=args.windows(2).find(|w|w[0]=="--snapshot").map(|w|w[1].clone());
    let test_width=args.windows(2).find(|w|w[0]=="--test-width").and_then(|w|w[1].parse::<i32>().ok());
    let page=args.windows(2).find(|w|w[0]=="--page").map(|w|w[1].clone()).unwrap_or_else(||"displays".into());
    let app=adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(move|app|build_window(app,demo,smoke,&page,capture.clone(),test_width));
    app.run_with_args(&["nirava"]);
}

fn build_window(app:&adw::Application,demo:bool,smoke:bool,initial_page:&str,capture:Option<String>,test_width:Option<i32>) {
    if let Some(window)=app.active_window(){window.present();return;}
    let builder=gtk::Builder::from_string(include_str!("../../../data/nirava-window.ui"));
    let window:adw::ApplicationWindow=builder.object("window").expect("window template");
    window.set_application(Some(app));
    if smoke{if let Some(width)=test_width{window.set_default_size(width.clamp(400,1600),760);window.set_resizable(false);}}
    let split:adw::NavigationSplitView=builder.object("split").expect("split");
    let navigation:gtk::ListBox=builder.object("navigation").expect("navigation");
    let search:gtk::SearchEntry=builder.object("search").expect("search");
    let stack:gtk::Stack=builder.object("pages").expect("pages");
    let page:adw::NavigationPage=builder.object("content_page").expect("content page");
    let banner:adw::Banner=builder.object("banner").expect("banner");
    let toasts:adw::ToastOverlay=builder.object("toasts").expect("toasts");
    let (tx,rx)=mpsc::channel();let (updates,receive)=mpsc::channel();
    std::thread::Builder::new().name("nirava-backend-client".into()).spawn(move||backend_loop(demo,rx,updates)).expect("backend worker");
    let snapshot=Rc::new(RefCell::new(if demo{Snapshot::demo()}else{Snapshot::default()}));
    let pending_layout=Rc::new(RefCell::new(None::<String>));
    let groups:Rc<RefCell<Vec<adw::PreferencesGroup>>>=Rc::new(RefCell::new(Vec::new()));
    let definitions=[("displays","Displays","video-display-symbolic","monitor resolution refresh scale rotation layout"),
        ("sound","Sound","audio-speakers-symbolic","speaker output volume mute microphone input applications"),
        ("power","Power","preferences-system-power-symbolic","idle sleep screen lock suspend caffeine battery"),
        ("system","System","preferences-system-details-symbolic","session diagnostics hardware compositor backup")];
    for (id,title,icon,_) in definitions {
        let nav=gtk::ListBoxRow::new();nav.set_widget_name(id);
        let content=gtk::Box::new(gtk::Orientation::Horizontal,12);content.set_margin_top(12);content.set_margin_bottom(12);content.set_margin_start(12);content.set_margin_end(12);
        content.append(&gtk::Image::from_icon_name(icon));let label=gtk::Label::new(Some(title));label.set_xalign(0.0);content.append(&label);nav.set_child(Some(&content));navigation.append(&nav);
        let preferences=adw::PreferencesPage::new();preferences.set_title(title);
        let group=adw::PreferencesGroup::new();group.set_title(title);preferences.add(&group);stack.add_named(&preferences,Some(id));groups.borrow_mut().push(group);
    }
    let stack_for_nav=stack.clone();let page_for_nav=page.clone();let split_for_nav=split.clone();
    navigation.connect_row_selected(move|_,row|if let Some(row)=row {
        let id=row.widget_name();stack_for_nav.set_visible_child_name(&id);
        page_for_nav.set_title(match id.as_str(){"sound"=>"Sound","power"=>"Power","system"=>"System",_=>"Displays"});
        split_for_nav.set_show_content(true);
    });
    let nav_for_search=navigation.clone();
    search.connect_search_changed(move|entry|{
        let query=entry.text().to_lowercase();
        for (i,(_,title,_,keywords)) in definitions.iter().enumerate(){if let Some(row)=nav_for_search.row_at_index(i as i32){row.set_visible(query.is_empty()||title.to_lowercase().contains(&query)||keywords.contains(&query));}}
        for i in 0..4 {if let Some(row)=nav_for_search.row_at_index(i){if row.is_visible(){nav_for_search.select_row(Some(&row));break;}}}
    });
    let index=definitions.iter().position(|(id,_,_,_)|*id==initial_page).unwrap_or(0);
    navigation.select_row(navigation.row_at_index(index as i32).as_ref());
    render(&snapshot.borrow(),&groups.borrow(),&tx);
    let weak=window.downgrade();let redraw_groups=groups.clone();let current=snapshot.clone();let actions=tx.clone();
    let last_signature=Rc::new(RefCell::new(String::new()));
    let layout_for_updates=pending_layout.clone();
    glib::timeout_add_local(Duration::from_millis(100),move||{
        let Some(window)=weak.upgrade() else{return glib::ControlFlow::Break};
        while let Ok(update)=receive.try_recv(){match update {
            Update::Snapshot(state)=>{
                let signature=serde_json::to_string(&state).unwrap_or_default();
                if *last_signature.borrow()!=signature {
                    *last_signature.borrow_mut()=signature;
                    banner.set_title(if demo{"Simulation — no hardware or settings changes"}else if state.read_only{"Read-only preview — desktop configuration is unchanged"}else{"Writable backend connected — review file changes before Apply"});
                    *current.borrow_mut()=state;
                }
            }
            Update::Message(message)=>toasts.add_toast(adw::Toast::new(&message)),
            Update::Failure(message,index)=>{toasts.add_toast(adw::Toast::new(&message));if let Some(index)=index{FORCED.with(|pages|pages.borrow_mut()[index]=true);}},
            Update::Preview(preview)=>show_preview(&window,&actions,preview),
            Update::Layout(lease)=>show_layout_confirmation(&window,&actions,lease,layout_for_updates.clone()),
        }}
        render(&current.borrow(),&redraw_groups.borrow(),&actions);
        glib::ControlFlow::Continue
    });
    let close_tx=tx.clone();window.connect_close_request(move|_|{if let Some(id)=pending_layout.borrow_mut().take(){let _=close_tx.send(Action::RevertLayout(id));}let _=close_tx.send(Action::Stop);glib::Propagation::Proceed});
    window.present();
    if smoke {let app=app.clone();let window=window.clone();glib::timeout_add_local_once(Duration::from_millis(800),move||{
        if let Some(path)=capture{if let Err(error)=capture_window(&window,&path){eprintln!("UI capture failed: {error}");}}
        app.quit();
    });}
}

fn capture_window(window:&adw::ApplicationWindow,path:&str)->anyhow::Result<()> {
    if std::path::Path::new(path).exists(){anyhow::bail!("Refusing to overwrite an existing screenshot");}
    let paintable=gtk::WidgetPaintable::new(Some(window));let snapshot=gtk::Snapshot::new();
    paintable.snapshot(&snapshot,window.width() as f64,window.height() as f64);
    let node=snapshot.to_node().ok_or_else(||anyhow::anyhow!("Window has no rendered scene"))?;
    let renderer=window.renderer().ok_or_else(||anyhow::anyhow!("Window has no renderer"))?;
    renderer.render_texture(&node,None).save_to_png(path)?;Ok(())
}

thread_local! {static ROWS:RefCell<Vec<Vec<gtk::Widget>>>=RefCell::new(vec![vec![],vec![],vec![],vec![]]);}
thread_local! {static SIGNATURES:RefCell<Vec<String>>=RefCell::new(vec![String::new();4]);}
thread_local! {static FORCED:RefCell<Vec<bool>>=RefCell::new(vec![false;4]);}
fn add(group:&adw::PreferencesGroup,index:usize,widget:&impl IsA<gtk::Widget>) {
    group.add(widget);ROWS.with(|rows|rows.borrow_mut()[index].push(widget.clone().upcast()));
}
fn reset(group:&adw::PreferencesGroup,index:usize){ROWS.with(|rows|{for row in rows.borrow_mut()[index].drain(..){group.remove(&row);}});}
fn row(title:&str,subtitle:&str)->adw::ActionRow {
    let row=adw::ActionRow::builder().title(glib::markup_escape_text(title)).subtitle(glib::markup_escape_text(subtitle)).build();row
}
fn render(state:&Snapshot,groups:&[adw::PreferencesGroup],tx:&Sender<Action>) {
    let signatures=[serde_json::to_string(&(&state.displays,&state.capabilities,state.read_only)),serde_json::to_string(&(&state.sound,state.read_only)),
        serde_json::to_string(&(&state.power,&state.capabilities,state.read_only)),serde_json::to_string(&(&state.session,&state.capabilities,&state.backups,&state.power.inhibitors,state.read_only))];
    for (index,signature) in signatures.into_iter().enumerate(){
        let signature=signature.unwrap_or_default();
        let forced=FORCED.with(|pages|pages.borrow()[index]);
        let changed=forced||SIGNATURES.with(|saved|saved.borrow()[index]!=signature);
        if !changed{continue;}
        // Audio polling must not destroy a control while the user is dragging it.
        if !forced{if let Some(root)=groups[index].root(){if let Some(focus)=root.focus(){if focus.is_ancestor(&groups[index]){continue;}}}}
        reset(&groups[index],index);
        match index {0=>render_displays(state,&groups[0],tx),1=>render_sound(state,&groups[1],tx),2=>render_power(state,&groups[2],tx),_=>render_system(state,&groups[3],tx)}
        SIGNATURES.with(|saved|saved.borrow_mut()[index]=signature);
        FORCED.with(|pages|pages.borrow_mut()[index]=false);
    }
}

fn render_displays(state:&Snapshot,group:&adw::PreferencesGroup,tx:&Sender<Action>) {
    group.set_description(Some(&format!("Settings owner: {}",state.displays.persistence_owner)));
    if state.displays.outputs.is_empty(){add(group,0,&row("No display adapter",&state.displays.error));return;}
    let drawing=gtk::DrawingArea::new();drawing.set_content_height(200);drawing.set_hexpand(true);
    drawing.update_property(&[gtk::accessible::Property::Label("Display layout overview")]);
    let outputs=state.displays.outputs.clone();
    drawing.set_draw_func(move|_,cr,width,height|{
        let active=outputs.iter().filter(|o|o.enabled).collect::<Vec<_>>();if active.is_empty(){return;}
        let min_x=active.iter().map(|o|o.x).min().unwrap_or(0) as f64;let min_y=active.iter().map(|o|o.y).min().unwrap_or(0) as f64;
        let logical_size=|o:&OutputState|{let (w,h)=if ["90","270","Flipped90","Flipped270"].contains(&o.transform.as_str()){(o.mode.height,o.mode.width)}else{(o.mode.width,o.mode.height)};(w as f64/o.scale,h as f64/o.scale)};
        let max_x=active.iter().map(|o|o.x as f64+logical_size(o).0).fold(f64::NEG_INFINITY,f64::max);
        let max_y=active.iter().map(|o|o.y as f64+logical_size(o).1).fold(f64::NEG_INFINITY,f64::max);
        let scale=((width as f64-40.0)/(max_x-min_x).max(1.0)).min((height as f64-40.0)/(max_y-min_y).max(1.0));
        for (i,o) in active.iter().enumerate(){let x=20.0+(o.x as f64-min_x)*scale;let y=20.0+(o.y as f64-min_y)*scale;
            cr.set_source_rgb(0.20,0.45,0.80);cr.rectangle(x,y,logical_size(o).0*scale,logical_size(o).1*scale);let _=cr.fill();
            cr.set_source_rgb(1.0,1.0,1.0);cr.set_font_size(16.0);cr.move_to(x+12.0,y+26.0);let _=cr.show_text(&format!("{}  {}",i+1,o.connector));}
    });let map=adw::PreferencesRow::new();map.set_child(Some(&drawing));map.set_selectable(false);map.set_activatable(false);add(group,0,&map);
    for output in &state.displays.outputs {
        let details=if output.enabled{format!("{} × {} · {:.2} Hz · scale {} · {}, {} · {}",output.mode.width,output.mode.height,output.mode.refresh_millihz as f64/1000.0,output.scale,output.x,output.y,output.transform)}else{"Disabled".into()};
        add(group,0,&row(&output.connector,&details));
        if output.vrr_supported{add(group,0,&row("Variable refresh rate",if output.vrr_enabled{"Enabled"}else{"Disabled"}));}
    }
    let editable=!state.read_only&&state.capabilities.iter().any(|cap|cap.id=="displays.write"&&cap.supported);
    let draft=Rc::new(RefCell::new(state.displays.outputs.clone()));
    for (index,output) in state.displays.outputs.iter().enumerate(){add_display_editor(group,index,output,draft.clone(),editable);}
    let apply=row("Try display changes","Automatically restored unless confirmed before the recovery deadline");
    let button=gtk::Button::with_label("Preview");button.set_valign(gtk::Align::Center);button.set_sensitive(editable);
    let sender=tx.clone();button.connect_clicked(move|_|{let _=sender.send(Action::PreviewLayout(draft.borrow().clone()));});apply.add_suffix(&button);add(group,0,&apply);
    add(group,0,&row("Save for future logins","Saving display settings is not available in this preview"));
}

fn add_display_editor(group:&adw::PreferencesGroup,index:usize,output:&OutputState,draft:Rc<RefCell<Vec<OutputState>>>,editable:bool) {
    let editor=adw::ExpanderRow::new();editor.set_title(&glib::markup_escape_text(&format!("Configure {}",output.connector)));
    let enabled=adw::SwitchRow::new();enabled.set_title("Enabled");enabled.set_active(output.enabled);enabled.set_sensitive(editable);
    let target=draft.clone();enabled.connect_active_notify(move|row|{target.borrow_mut()[index].enabled=row.is_active();});editor.add_row(&enabled);
    let modes=output.modes.iter().map(|m|format!("{} × {} · {:.3} Hz",m.width,m.height,m.refresh_millihz as f64/1000.0)).collect::<Vec<_>>();
    let refs=modes.iter().map(String::as_str).collect::<Vec<_>>();
    let model=gtk::StringList::new(&refs);let mode=adw::ComboRow::new();mode.set_title("Resolution and refresh rate");mode.set_model(Some(&model));
    let initial=output.modes.iter().position(|m|m==&output.mode).unwrap_or(0);mode.set_selected(initial as u32);mode.set_sensitive(editable&&!modes.is_empty());
    let target=draft.clone();let advertised=output.modes.clone();mode.connect_selected_notify(move|row|{if let Some(mode)=advertised.get(row.selected() as usize){target.borrow_mut()[index].mode=mode.clone();}});editor.add_row(&mode);
    if !output.enabled{if let Some(mode)=output.modes.get(initial){draft.borrow_mut()[index].mode=mode.clone();}}
    let scale=adw::SpinRow::with_range(0.5,4.0,0.05);scale.set_title("Scale");scale.set_digits(2);scale.set_value(output.scale);scale.set_sensitive(editable);
    let target=draft.clone();scale.connect_value_notify(move|row|{target.borrow_mut()[index].scale=row.value();});editor.add_row(&scale);
    for (horizontal,value,title) in [(true,output.x,"Horizontal position"),(false,output.y,"Vertical position")]{
        let position=adw::SpinRow::with_range(-65535.0,65535.0,1.0);position.set_title(title);position.set_value(value as f64);position.set_sensitive(editable);
        let target=draft.clone();position.connect_value_notify(move|row|{let mut draft=target.borrow_mut();if horizontal{draft[index].x=row.value() as i32;}else{draft[index].y=row.value() as i32;}});editor.add_row(&position);
    }
    let transforms=["Normal","90","180","270","Flipped","Flipped90","Flipped180","Flipped270"];
    let model=gtk::StringList::new(&transforms);let transform=adw::ComboRow::new();transform.set_title("Rotation and reflection");transform.set_model(Some(&model));
    transform.set_selected(transforms.iter().position(|t|*t==output.transform).unwrap_or(0) as u32);transform.set_sensitive(editable);
    let target=draft.clone();transform.connect_selected_notify(move|row|{if let Some(value)=transforms.get(row.selected() as usize){target.borrow_mut()[index].transform=(*value).into();}});editor.add_row(&transform);
    let vrr=adw::SwitchRow::new();vrr.set_title("Variable refresh rate");vrr.set_active(output.vrr_enabled);vrr.set_sensitive(editable&&output.vrr_supported);
    if !output.vrr_supported{vrr.set_subtitle("This display does not advertise support");}
    let target=draft.clone();vrr.connect_active_notify(move|row|{target.borrow_mut()[index].vrr_enabled=row.is_active();});editor.add_row(&vrr);
    add(group,0,&editor);
}

fn show_layout_confirmation(window:&adw::ApplicationWindow,tx:&Sender<Action>,lease:nirava_backends::display_guard::Lease,pending:Rc<RefCell<Option<String>>>) {
    *pending.borrow_mut()=Some(lease.id.clone());
    let dialog=adw::AlertDialog::builder().heading("Keep these display changes?")
        .body("The original layout will return automatically at the recovery deadline. Keep applies only to this login; configuration files are not saved.").build();
    dialog.add_response("revert","Revert");dialog.add_response("keep","Keep for this login");dialog.set_default_response(Some("revert"));dialog.set_close_response("revert");
    let sender=tx.clone();let id=lease.id.clone();
    dialog.connect_response(None,move|_,response|{
        if pending.borrow().as_deref()==Some(id.as_str()){pending.borrow_mut().take();let _=sender.send(if response=="keep"{Action::KeepLayout(id.clone())}else{Action::RevertLayout(id.clone())});}
    });
    let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis() as u64;
    let remaining=lease.expires_at_millis.saturating_sub(now);let deadline=std::time::Instant::now()+Duration::from_millis(remaining);
    let weak=dialog.downgrade();
    glib::timeout_add_local(Duration::from_millis(100),move||{
        let Some(dialog)=weak.upgrade()else{return glib::ControlFlow::Break};
        let seconds=deadline.saturating_duration_since(std::time::Instant::now()).as_secs_f64().ceil() as u64;
        dialog.set_response_label("keep",&format!("Keep for this login ({seconds}s)"));
        if seconds==0{dialog.close();return glib::ControlFlow::Break;}
        glib::ControlFlow::Continue
    });
    dialog.present(Some(window));
}

fn render_sound(state:&Snapshot,group:&adw::PreferencesGroup,tx:&Sender<Action>) {
    if !state.sound.available{add(group,1,&row("Audio unavailable",&state.sound.error));return;}
    group.set_description(Some(&state.sound.server));
    for device in &state.sound.devices {
        let default=if device.input{&state.sound.default_input}else{&state.sound.default_output};
        let title=format!("{}{}",device.description,if &device.name==default{" · Default"}else{""});
        let audio_row=row(&title,if device.input{"Microphone"}else{"Output device"});
        if &device.name!=default{
            let button=gtk::Button::with_label("Use");button.set_valign(gtk::Align::Center);button.set_sensitive(!state.read_only);
            let tx=tx.clone();let name=device.name.clone();let input=device.input;let generation=state.sound.generation;button.connect_clicked(move|_|{let _=tx.send(Action::Default(generation,name.clone(),input));});audio_row.add_suffix(&button);
        }
        add(group,1,&audio_row);
        add_volume(group,state.sound.generation,&device.description,device.id,device.input,false,device.volume,device.muted,state.read_only,tx);
    }
    for stream in &state.sound.streams {add_volume(group,state.sound.generation,&stream.application,stream.id,false,true,stream.volume,stream.muted,state.read_only,tx);}
    if state.read_only{add(group,1,&row("Audio controls are read-only","An explicitly writable Nirava backend must be connected before controls are enabled"));}
}

fn add_volume(group:&adw::PreferencesGroup,generation:u64,title:&str,id:u32,input:bool,stream:bool,volume:f64,muted:bool,read_only:bool,tx:&Sender<Action>) {
    let line=adw::ActionRow::new();line.set_title(&glib::markup_escape_text(title));
    let scale=gtk::Scale::with_range(gtk::Orientation::Horizontal,0.0,100.0,1.0);scale.set_size_request(180,-1);scale.set_valign(gtk::Align::Center);scale.set_draw_value(true);
    scale.set_value((volume*100.0).clamp(0.0,100.0));scale.set_sensitive(!read_only);scale.update_property(&[gtk::accessible::Property::Label(&format!("{title} volume"))]);
    let sender=tx.clone();
    let timer=Rc::new(RefCell::new(None::<glib::SourceId>));
    scale.connect_value_changed(move|scale|{
        if let Some(previous)=timer.borrow_mut().take(){previous.remove();}
        let weak=scale.downgrade();let sender=sender.clone();let pending=timer.clone();
        let source=glib::timeout_add_local_once(Duration::from_millis(180),move||{
            pending.borrow_mut().take();
            if let Some(scale)=weak.upgrade(){let _=sender.send(Action::Volume(generation,id,input,stream,scale.value()/100.0));}
        });*timer.borrow_mut()=Some(source);
    });
    let mute=gtk::Switch::new();mute.set_valign(gtk::Align::Center);mute.set_active(muted);mute.set_sensitive(!read_only);
    mute.update_property(&[gtk::accessible::Property::Label(&format!("Mute {title}"))]);let sender=tx.clone();mute.connect_active_notify(move|switch|{let _=sender.send(Action::Mute(generation,id,input,stream,switch.is_active()));});
    line.add_suffix(&scale);line.add_suffix(&mute);add(group,1,&line);
}

fn render_power(state:&Snapshot,group:&adw::PreferencesGroup,tx:&Sender<Action>) {
    let editable=!state.read_only&&state.power.owner=="noctalia"&&state.power.error.is_empty()&&state.session.idle_owners.as_slice()==["noctalia"];
    group.set_description(Some(&format!("Existing idle engine: {}",state.power.owner)));
    if !state.power.error.is_empty(){add(group,2,&row("Power integration",&state.power.error));}
    let mut spins=vec![];
    for (title,value) in [("Turn screen off after",state.power.settings.screen_off_secs),("Lock after",state.power.settings.lock_secs),("Automatic suspend after",state.power.settings.suspend_secs)]{
        let spin=adw::SpinRow::with_range(0.0,1440.0,1.0);spin.set_title(title);spin.set_subtitle("Minutes · 0 disables this action");spin.set_value(value as f64/60.0);spin.set_sensitive(editable);add(group,2,&spin);spins.push(spin);
    }
    let lock=adw::SwitchRow::new();lock.set_title("Lock before automatic suspend");lock.set_active(state.power.settings.lock_before_suspend);lock.set_sensitive(editable);add(group,2,&lock);
    let stage=row("Review idle changes","Preview the affected file and durations before saving");let button=gtk::Button::with_label("Review");button.set_valign(gtk::Align::Center);button.set_sensitive(editable);
    let sender=tx.clone();button.connect_clicked(move|_|{let settings=IdleSettings {screen_off_secs:(spins[0].value()*60.0).round() as u64,lock_secs:(spins[1].value()*60.0).round() as u64,suspend_secs:(spins[2].value()*60.0).round() as u64,lock_before_suspend:lock.is_active()};let _=sender.send(Action::StagePower(settings));});stage.add_suffix(&button);add(group,2,&stage);
    let caffeine=adw::SwitchRow::new();caffeine.set_title("Keep awake");caffeine.set_subtitle("Blocks automatic idle actions until turned off or the backend exits");caffeine.set_active(state.power.caffeine);caffeine.set_sensitive(!state.read_only&&state.power.owner=="noctalia");
    let sender=tx.clone();caffeine.connect_active_notify(move|switch|{let _=sender.send(Action::Caffeine(switch.is_active()));});add(group,2,&caffeine);
    add(group,2,&row("Suspend availability",power_availability(&state.power.can_suspend)));add(group,2,&row("Hibernate availability",power_availability(&state.power.can_hibernate)));
    if !state.read_only&&state.capabilities.iter().any(|cap|cap.id=="power.suspend"&&cap.supported){
        let suspend=row("Suspend now","Locks the screen before suspending");let button=gtk::Button::with_label("Suspend");button.set_valign(gtk::Align::Center);
        let sender=tx.clone();button.connect_clicked(move|_|{let _=sender.send(Action::Suspend);});suspend.add_suffix(&button);add(group,2,&suspend);
    }else{add(group,2,&row("Suspend now","Suspend is not available in this preview"));}
}

fn power_availability(value:&str)->&'static str{match value{"yes"=>"Available","challenge"=>"Authentication may be required","na"=>"Not supported","no"=>"Not allowed",_=>"Unknown"}}

fn render_system(state:&Snapshot,group:&adw::PreferencesGroup,tx:&Sender<Action>) {
    for (title,value) in [("Operating system",state.session.os.clone()),("Compositor",format!("{} {}",state.session.compositor,state.session.version)),("Idle owners",state.session.idle_owners.join(", ")),("Display owners",state.displays.persistence_owner.clone()),("Session",state.session.session_id.clone())]{add(group,3,&row(title,&value));}
    for message in &state.session.diagnostics{add(group,3,&row("Diagnostic",message));}
    for capability in &state.capabilities{add(group,3,&row(&capability.id,&format!("{} · {}",if capability.supported{"Available"}else{"Unavailable"},capability.reason)));}
    for inhibitor in &state.power.inhibitors{add(group,3,&row("Power inhibitor",inhibitor));}
    for backup in state.backups.iter().take(10){
        let info=row("Configuration backup",&format!("{} · {}",backup.path,if backup.can_restore{"Ready to restore"}else{&backup.reason}));
        let button=gtk::Button::with_label("Restore");button.set_valign(gtk::Align::Center);button.set_sensitive(backup.can_restore);
        let sender=tx.clone();let backup=backup.clone();
        button.connect_clicked(move|button|{
            let dialog=adw::AlertDialog::builder().heading("Restore this configuration?")
                .body(format!("File: {}\n\nBackup: {}\n\nNirava refuses to restore if either file has changed since this backup was created.",backup.path,backup.backup)).build();
            dialog.add_response("cancel","Cancel");dialog.add_response("restore","Restore");dialog.set_default_response(Some("cancel"));dialog.set_close_response("cancel");
            let sender=sender.clone();let id=backup.id.clone();dialog.connect_response(None,move|_,response|{if response=="restore"{let _=sender.send(Action::Restore(id.clone()));}});
            if let Some(root)=button.root().and_downcast::<gtk::Window>(){dialog.present(Some(&root));}
        });info.add_suffix(&button);add(group,3,&info);
    }
    let refresh=row("Refresh discovery","Reads the session again without changing its configuration");let button=gtk::Button::with_label("Refresh");button.set_valign(gtk::Align::Center);let sender=tx.clone();button.connect_clicked(move|_|{let _=sender.send(Action::Refresh);});refresh.add_suffix(&button);add(group,3,&refresh);
}

fn show_preview(window:&adw::ApplicationWindow,tx:&Sender<Action>,preview:serde_json::Value) {
    let id=preview["id"].as_str().unwrap_or_default().to_string();
    let body=format!("File: {}\n\nBefore:\n{}\n\nAfter:\n{}\n\nNirava will back up the original file and reload Noctalia.",preview["path"].as_str().unwrap_or_default(),serde_json::to_string_pretty(&preview["before"]).unwrap_or_default(),serde_json::to_string_pretty(&preview["after"]).unwrap_or_default());
    let dialog=adw::AlertDialog::builder().heading("Apply these idle settings?").body(&body).build();dialog.add_response("cancel","Cancel");dialog.add_response("apply","Apply");dialog.set_close_response("cancel");dialog.set_default_response(Some("cancel"));
    let sender=tx.clone();dialog.connect_response(None,move|_,response|{let _=sender.send(if response=="apply"{Action::Commit(id.clone())}else{Action::Cancel(id.clone())});});dialog.present(Some(window));
}

fn backend_loop(demo:bool,rx:mpsc::Receiver<Action>,updates:Sender<Update>) {
    let runtime=tokio::runtime::Runtime::new().expect("backend runtime");
    let audio=nirava_backends::audio::AudioClient::new();
    let mut connection=runtime.block_on(zbus::Connection::session()).ok();
    loop {
        if connection.is_none(){connection=runtime.block_on(zbus::Connection::session()).ok();}
        let state=runtime.block_on(async {
            if demo{return Snapshot::demo();}
            if let Some(connection)=connection.as_ref(){if let Ok(proxy)=service_proxy(connection).await{if let Ok(json)=proxy.call::<_,_,String>("GetSnapshot",&()).await{if let Ok(state)=serde_json::from_str(&json){return state;}}}}
            let mut state=nirava_backends::discovery::discover().await;
            state.sound=audio.snapshot().unwrap_or_else(|e|SoundState{error:e.to_string(),..Default::default()});state
        });
        let _=updates.send(Update::Snapshot(state));
        match rx.recv_timeout(Duration::from_secs(2)) {
            Ok(Action::Stop)|Err(mpsc::RecvTimeoutError::Disconnected)=>break,
            Ok(Action::Refresh)|Err(mpsc::RecvTimeoutError::Timeout)=>continue,
            Ok(action)=>{
                if demo{let _=updates.send(Update::Message("Simulation does not write to hardware".into()));continue;}
                let reset_page=match &action{Action::Volume(..)|Action::Mute(..)|Action::Default(..)=>Some(1),Action::Caffeine(..)=>Some(2),_=>None};
                let result=runtime.block_on(async {
                    let connection=connection.as_ref().ok_or_else(||anyhow::anyhow!("Session bus unavailable"))?;
                    let proxy=service_proxy(connection).await?;
                    let json:String=match action {
                        Action::Volume(generation,id,input,stream,value)=>proxy.call("SetVolume",&(generation,id,input,stream,value)).await?,
                        Action::Mute(generation,id,input,stream,value)=>proxy.call("SetMute",&(generation,id,input,stream,value)).await?,
                        Action::Default(generation,name,input)=>proxy.call("SetDefaultDevice",&(generation,name,input)).await?,
                        Action::StagePower(settings)=>{let preview:String=proxy.call("StagePower",&(serde_json::to_string(&settings)?,)).await?;return Ok(Update::Preview(serde_json::from_str(&preview)?));},
                        Action::Commit(id)=>proxy.call("CommitChange",&(id,)).await?,
                        Action::Cancel(id)=>{proxy.call::<_,_,()>("CancelChange",&(id,)).await?;return Ok(Update::Message("Change cancelled".into()));},
                        Action::Caffeine(enabled)=>proxy.call("SetCaffeine",&(enabled,)).await?,
                        Action::PreviewLayout(draft)=>{let json:String=proxy.call("PreviewLayout",&(serde_json::to_string(&draft)?,)).await?;return Ok(Update::Layout(serde_json::from_str(&json)?));},
                        Action::RevertLayout(id)=>proxy.call("RevertLayout",&(id,)).await?,
                        Action::KeepLayout(id)=>proxy.call("KeepLayoutForSession",&(id,)).await?,
                        Action::Restore(id)=>proxy.call("RestoreChange",&(id,)).await?,
                        Action::Suspend=>proxy.call("Suspend",&()).await?,
                        _=>return Ok(Update::Message(String::new())),
                    };
                    let result:OperationResult=serde_json::from_str(&json)?;Ok::<_,anyhow::Error>(if result.status=="failed"{Update::Failure(result.message,reset_page)}else{Update::Message(result.message)})
                });
                let _=updates.send(result.unwrap_or_else(|e|Update::Failure(e.to_string(),reset_page)));
            }
        }
    }
}
async fn service_proxy(connection:&zbus::Connection)->anyhow::Result<zbus::Proxy<'_>> {
    let proxy=zbus::Proxy::new(connection,BUS_NAME,OBJECT_PATH,"io.github.Somchandra17.Nirava.Manager1").await?;
    let key:String=proxy.call("GetSessionKey",&()).await?;
    if key!=nirava_core::session_key(){anyhow::bail!("Backend belongs to another graphical session; reconnect from this session");}
    Ok(proxy)
}
