use adw::prelude::*;
use gtk::{gio, glib};
use nirava_core::apps::*;
use std::{cell::RefCell,rc::Rc, sync::mpsc::Sender};
thread_local! {static VIEW:RefCell<(u32,String)>=RefCell::new((0,String::new()));}
use super::{Action, add, row};

pub fn render(state: &AppsInventory, group: &adw::PreferencesGroup, tx: &Sender<Action>, demo: bool, package_actions: bool) {
    let demo=demo||state.simulation;
    let package_actions=package_actions&&!demo&&!nirava_core::in_flatpak();
    super::reset(group, 4);
    group.set_description(Some("Find installed apps and packages. Review package actions before continuing in DNF or Flatpak."));
    let refresh=row("Installed software", &if state.loaded {format!("{} RPM packages · {} Flatpak apps",state.packages.iter().filter(|p|p.source==PackageSource::Rpm).count(),state.packages.iter().filter(|p|p.source!=PackageSource::Rpm).count())}else{"Loading local package inventory…".into()});
    let button=gtk::Button::with_label("Refresh");button.set_valign(gtk::Align::Center);
    let sender=tx.clone();button.connect_clicked(move|_|{let _=sender.send(Action::RefreshApps);});refresh.add_suffix(&button);add(group,4,&refresh);
    if state.immutable {add(group,4,&row("Immutable system", "RPM inventory is available; DNF actions are disabled on an OSTree system."));}
    if !state.rpm_available && state.loaded {add(group,4,&row("RPM unavailable", "This build currently supports RPM/DNF and Flatpak. Other native package managers are not supported yet."));}
    for error in &state.errors {add(group,4,&row("Inventory unavailable",error));}
    if !package_actions {add(group,4,&row("Package actions are read-only", if nirava_core::in_flatpak(){"Review and copy commands here. Use the native app for interactive package actions."}else{"You can review and copy commands. Start Nirava with --enable-package-actions to continue in an interactive terminal."}));}
    let filter=adw::ComboRow::new();filter.set_title("Show");
    filter.set_model(Some(&gtk::StringList::new(&["Applications","RPM packages","Flatpak · User","Flatpak · System"])));
    filter.set_selected(VIEW.with(|saved|saved.borrow().0));
    add(group,4,&filter);
    let search=gtk::SearchEntry::new();search.set_placeholder_text(Some("Search installed software"));
    search.set_text(&VIEW.with(|saved|saved.borrow().1.clone()));
    let search_row=adw::PreferencesRow::new();search_row.set_child(Some(&search));search_row.set_activatable(false);add(group,4,&search_row);
    let count=gtk::Label::new(None);count.set_xalign(0.0);count.set_wrap(true);count.add_css_class("dim-label");
    let count_row=adw::PreferencesRow::new();count_row.set_child(Some(&count));count_row.set_activatable(false);add(group,4,&count_row);
    let list=gtk::ListBox::new();list.set_selection_mode(gtk::SelectionMode::None);list.add_css_class("boxed-list");
    let list_row=adw::PreferencesRow::new();list_row.set_child(Some(&list));list_row.set_activatable(false);add(group,4,&list_row);
    let state=Rc::new(state.clone());
    let desktop_apps=Rc::new(if demo||nirava_core::in_flatpak() {vec![]} else {let mut apps=gio::AppInfo::all().into_iter().filter(|a|a.should_show()).collect::<Vec<_>>();apps.sort_by_key(|a|a.display_name().to_lowercase());apps});
    let redraw: Rc<dyn Fn()>={let filter=filter.downgrade();let search=search.downgrade();let list=list.downgrade();let count=count.downgrade();let state=state.clone();let apps=desktop_apps;
        Rc::new(move||{
            let (Some(filter),Some(search),Some(list),Some(count))=(filter.upgrade(),search.upgrade(),list.upgrade(),count.upgrade()) else{return;};
            while let Some(child)=list.first_child(){list.remove(&child);}
            VIEW.with(|saved|*saved.borrow_mut()=(filter.selected(),search.text().to_string()));
            let query=search.text().to_lowercase();
            if filter.selected()==0 {
                if demo {
                    let mut matches=0;
                    for (name,id) in [("Calculator","org.gnome.Calculator"),("Terminal","kitty.desktop")]{
                        if format!("{name} {id}").to_lowercase().contains(&query){list.append(&row(name,&format!("{id} · demo app")));matches+=1;}
                    }
                    count.set_label(&format!("{matches} fictional apps · launching is disabled in simulation"));
                    if matches==0{list.append(&row("No matches","Try another search."));}return;
                }
                if nirava_core::in_flatpak(){list.append(&row("Use the native app to launch applications","The Flatpak frontend can list host packages through its companion. Choose an RPM or Flatpak view above."));count.set_label("Host application launching is unavailable in this sandbox");return;}
                let matching=apps.iter().filter(|a|format!("{} {}",a.display_name(),a.id().unwrap_or_default()).to_lowercase().contains(&query)).collect::<Vec<_>>();
                count.set_label(&format!("{} applications{}",matching.len(),if matching.len()>100{" · first 100 shown; narrow your search"}else{""}));
                for app in matching.into_iter().take(100){
                    let info=row(&app.display_name(),app.id().as_deref().unwrap_or("Desktop application"));
                    if let Some(icon)=app.icon(){info.add_prefix(&gtk::Image::from_gicon(&icon));}
                    let launch=gtk::Button::with_label("Open");launch.set_valign(gtk::Align::Center);let app=app.clone();
                    launch.connect_clicked(move|button|{if let Err(e)=app.launch(&[],None::<&gio::AppLaunchContext>){show_error(button,&e.to_string());}});
                    info.add_suffix(&launch);list.append(&info);
                }
            } else {
                let source=match filter.selected(){1=>PackageSource::Rpm,2=>PackageSource::FlatpakUser,_=>PackageSource::FlatpakSystem};
                let matching=state.packages.iter().filter(|p|p.source==source&&format!("{} {} {}",p.name,p.id,p.origin).to_lowercase().contains(&query)).collect::<Vec<_>>();
                count.set_label(&format!("{} installed{}",matching.len(),if matching.len()>100{" · first 100 shown; narrow your search"}else{""}));
                for package in matching.into_iter().take(100){
                    let info=adw::ExpanderRow::new();info.set_title(&glib::markup_escape_text(&package.name));
                    info.set_subtitle(&glib::markup_escape_text(&format!("{} · {}",package.version,source.label())));
                    info.add_row(&row("Package",&package.id));info.add_row(&row("Source",&package.origin));
                    for (action,label) in [(PackageAction::Update,"Update"),(PackageAction::Remove,"Remove")]{
                        let line=row(label,if action==PackageAction::Remove{"Review removal in your package manager"}else{"Check and review an update for this package"});
                        let button=gtk::Button::with_label("Review");button.set_valign(gtk::Align::Center);
                        let allowed=available(&state,source);button.set_sensitive(allowed);
                        let request=package.request(action);
                        button.connect_clicked(move|button|review(button,request.clone(),package_actions&&!demo));line.add_suffix(&button);info.add_row(&line);
                    }
                    list.append(&info);
                }
            }
            if list.first_child().is_none(){list.append(&row("No matches", "Try another search or refresh the inventory."));}
        })};
    let update=redraw.clone();filter.connect_selected_notify(move|_|update());
    let update=redraw.clone();search.connect_search_changed(move|_|update());redraw();
    install_form(group,&state,package_actions&&!demo);
}
fn available(state:&AppsInventory,source:PackageSource)->bool {
    match source {PackageSource::Rpm=>state.dnf_available&&!state.immutable,_=>state.flatpak_available}
}
fn install_form(group:&adw::PreferencesGroup,state:&AppsInventory,package_actions:bool) {
    let form=adw::ExpanderRow::new();form.set_title("Install an application or package");form.set_subtitle("Use an exact RPM name or Flatpak application ID");
    let source=adw::ComboRow::new();source.set_title("Package source");source.set_model(Some(&gtk::StringList::new(&["RPM / DNF","Flatpak · User","Flatpak · System"])));form.add_row(&source);
    let target=adw::EntryRow::new();target.set_title("Package name or application ID");form.add_row(&target);
    let remote=adw::ComboRow::new();remote.set_title("Flatpak remote");form.add_row(&remote);
    let remotes=Rc::new(state.remotes.clone());
    let update_remotes:Rc<dyn Fn()>={let source=source.downgrade();let remote=remote.downgrade();let remotes=remotes.clone();Rc::new(move||{
        let (Some(source),Some(remote))=(source.upgrade(),remote.upgrade())else{return;};
        let provider=if source.selected()==1{PackageSource::FlatpakUser}else{PackageSource::FlatpakSystem};
        let names=remotes.iter().filter(|r|r.source==provider).map(|r|r.name.as_str()).collect::<Vec<_>>();
        remote.set_model(Some(&gtk::StringList::new(&names)));remote.set_visible(source.selected()!=0);remote.set_subtitle(if names.is_empty(){"No configured remote in this installation"}else{"Uses an existing configured remote"});
    })};
    let update=update_remotes.clone();source.connect_selected_notify(move|_|update());update_remotes();
    let action=row("Review installation","The package manager resolves dependencies and asks for confirmation in the terminal.");let button=gtk::Button::with_label("Review");button.set_valign(gtk::Align::Center);
    let state=state.clone();button.connect_clicked(move|button|{
        let provider=match source.selected(){0=>PackageSource::Rpm,1=>PackageSource::FlatpakUser,_=>PackageSource::FlatpakSystem};
        if !available(&state,provider){show_error(button,"This package manager is unavailable, or this system does not support DNF transactions.");return;}
        let remote_name=if provider==PackageSource::Rpm{String::new()}else{state.remotes.iter().filter(|r|r.source==provider).nth(remote.selected() as usize).map(|r|r.name.clone()).unwrap_or_default()};
        review(button,PackageRequest {source:provider,action:PackageAction::Install,target:target.text().trim().into(),remote:remote_name},package_actions);
    });action.add_suffix(&button);form.add_row(&action);add(group,4,&form);
}
fn show_error(button:&gtk::Button,message:&str) {
    let dialog=adw::AlertDialog::builder().heading("Could not continue").body(message).build();dialog.add_response("close","Close");
    if let Some(root)=button.root().and_downcast::<gtk::Window>(){dialog.present(Some(&root));}
}
fn review(button:&gtk::Button,request:PackageRequest,package_actions:bool) {
    let plan=match PackagePlan::build(request){Ok(plan)=>plan,Err(e)=>{show_error(button,&e);return;}};
    let command=plan.display_command();
    let body=format!("{}\n\n{}\n\nThe package manager will calculate dependencies and show its final transaction before asking you to confirm. Authentication may be required. Read the complete removal list carefully.\n\nNo automatic yes, repository changes, data deletion or forced transactions are requested. Closing the terminal does not guarantee rollback; let an active transaction finish. Refresh Apps afterward to read the result.",plan.description,command);
    let dialog=adw::AlertDialog::builder().heading("Review package action").body(&body).build();
    dialog.add_response("cancel","Cancel");dialog.add_response("copy","Copy command");
    if package_actions {dialog.add_response("terminal","Continue in Terminal");}
    dialog.set_default_response(Some("cancel"));dialog.set_close_response("cancel");
    let parent=button.root().and_downcast::<gtk::Window>();
    dialog.connect_response(None,move|_,response|match response {
        "copy"=>{if let Some(display)=gtk::gdk::Display::default(){display.clipboard().set_text(&command);}},
        "terminal" if package_actions=>{if let Err(e)=open_terminal(&plan){let alert=adw::AlertDialog::builder().heading("Terminal unavailable").body(format!("{e}\n\nYou can copy the reviewed command and run it in your own terminal.")).build();alert.add_response("close","Close");if let Some(root)=parent.as_ref(){alert.present(Some(root));}}},
        _=>{},
    });
    if let Some(root)=button.root().and_downcast::<gtk::Window>(){dialog.present(Some(&root));}
}
fn open_terminal(plan:&PackagePlan)->std::io::Result<()> {
    // Fixed terminal argument templates; no shell, terminal preference string,
    // or user-supplied executable is evaluated. Package managers keep prompting.
    let candidates:[(&str,&[&str]);5]=[
        ("/usr/bin/kitty", &["--hold","--title","Nirava — package review","--"]),
        ("/usr/bin/foot", &["--hold","--"]),
        ("/usr/bin/gnome-terminal", &["--wait","--"]),
        ("/usr/bin/konsole", &["--hold","-e"]),
        ("/usr/bin/xterm", &["-hold","-e"]),
    ];
    let (terminal,args)=candidates.into_iter().find(|(p,_)|std::path::Path::new(p).is_file())
        .ok_or_else(||std::io::Error::new(std::io::ErrorKind::NotFound,"No supported terminal found"))?;
    let mut child=std::process::Command::new(terminal).args(args).arg(&plan.program).args(&plan.args)
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn()?;
    std::thread::spawn(move||{let _=child.wait();});Ok(())
}
