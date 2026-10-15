//! Local inventory only. No metadata refresh, package installation, or transaction.
use nirava_core::apps::*;

pub fn parse_rpm(text: &str) -> Result<Vec<PackageRecord>, String> {
    text.lines().map(|line| {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 3 || fields.iter().any(|s| s.is_empty()) { return Err("Malformed RPM inventory".into()); }
        Ok(PackageRecord { source: PackageSource::Rpm, id: format!("{}.{}", fields[0], fields[2]),
            name: fields[0].into(), version: fields[1].into(), architecture: fields[2].into(), origin: "Installed RPM database".into() })
    }).collect()
}
pub fn parse_flatpak(text: &str, source: PackageSource) -> Result<Vec<PackageRecord>, String> {
    text.lines().filter(|l| !l.is_empty()).map(|line| {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 4 { return Err("Malformed Flatpak application inventory".into()); }
        // flatpak list --columns=ref prints ID/arch/branch; accept an explicit
        // app/ prefix too, but never infer a missing architecture or branch.
        let reference=if fields[0].split('/').count()==3 {format!("app/{}",fields[0])}else{fields[0].to_owned()};
        if !flatpak_ref(&reference) || !reference.starts_with("app/") {
            return Err("Malformed Flatpak application inventory".into());
        }
        Ok(PackageRecord { source, id: reference.clone(), name: fields[1].into(),
            version: fields[2].into(), architecture: reference.split('/').nth(2).unwrap_or_default().into(), origin: fields[3].into() })
    }).collect()
}
pub async fn discover() -> AppsInventory {
    let mut state = AppsInventory { loaded: true,
        rpm_available: std::path::Path::new("/usr/bin/rpm").is_file(),
        dnf_available: std::path::Path::new("/usr/bin/dnf").is_file() && std::path::Path::new("/usr/bin/sudo").is_file(),
        flatpak_available: std::path::Path::new("/usr/bin/flatpak").is_file(),
        immutable: std::path::Path::new("/run/ostree-booted").exists(), ..Default::default() };
    let rpm = async {
        if !state.rpm_available { return Ok(vec![]); }
        crate::command("/usr/bin/rpm", &["-qa", "--qf", "%{NAME}\t%{VERSION}-%{RELEASE}\t%{ARCH}\n"]).await
            .map_err(|e| e.to_string()).and_then(|s| parse_rpm(&s))
    };
    let flatpak = |scope: &'static str, source| async move {
        if !std::path::Path::new("/usr/bin/flatpak").is_file() { return (Ok(vec![]), Ok(vec![])); }
        let list_args = [scope, "list", "--app", "--columns=ref,name,version,origin"];
        let remote_args = [scope, "remotes", "--columns=name"];
        let (packages, remotes) = tokio::join!(
            crate::command("/usr/bin/flatpak", &list_args),
            crate::command("/usr/bin/flatpak", &remote_args));
        let packages = packages.map_err(|e|e.to_string()).and_then(|s|parse_flatpak(&s,source));
        let remotes = remotes.map_err(|e|e.to_string()).map(|s|s.lines().filter(|l|!l.is_empty())
            .map(|name|PackageRemote {source,name:name.into()}).collect::<Vec<_>>());
        (packages, remotes)
    };
    let (rpm, user, system) = tokio::join!(rpm, flatpak("--user",PackageSource::FlatpakUser), flatpak("--system",PackageSource::FlatpakSystem));
    for (label, result) in [("RPM",rpm),("User Flatpak",user.0),("System Flatpak",system.0)] {
        match result { Ok(mut packages)=>state.packages.append(&mut packages), Err(e)=>state.errors.push(format!("{label}: {e}")) }
    }
    for result in [user.1,system.1] { match result {Ok(mut remotes)=>state.remotes.append(&mut remotes),Err(e)=>state.errors.push(format!("Flatpak remotes: {e}"))} }
    state.packages.sort_by_key(|p| (p.name.to_lowercase(),p.id.clone(),p.source.label()));
    state
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn rpm_retains_multilib_and_installed_kernel_versions() {
        let items = parse_rpm("libfoo\t1-2\tx86_64\nlibfoo\t1-2\ti686\nkernel-core\t6.1-1\tx86_64\nkernel-core\t6.2-1\tx86_64\n").unwrap();
        assert_ne!(items[0].id,items[1].id);
        assert_ne!(items[2].version,items[3].version);
        assert!(parse_rpm("truncated\tdata").is_err());
    }
    #[test] fn flatpak_retains_scope_reference_and_empty_version() {
        let text = "app/org.gnome.Calculator/x86_64/stable\tCalculator\t\tflathub\n";
        let user=parse_flatpak(text,PackageSource::FlatpakUser).unwrap();
        let system=parse_flatpak(text,PackageSource::FlatpakSystem).unwrap();
        assert_eq!(user[0].id,system[0].id);
        assert_ne!(user[0].source,system[0].source);
        assert_eq!(user[0].version, "");
        let actual=parse_flatpak("org.gnome.Calculator/x86_64/stable\tCalculator\t49\tflathub\n",PackageSource::FlatpakUser).unwrap();
        assert_eq!(actual[0].id,user[0].id);
        assert_eq!(actual[0].architecture,"x86_64");
        assert!(parse_flatpak("net.portswigger.BurpSuite-Community/x86_64/stable\tBurp Suite\t2026\tflathub",PackageSource::FlatpakSystem).is_ok());
        assert!(parse_flatpak("runtime/org.gnome.Platform/x86_64/49\tRuntime\t49\tflathub",PackageSource::FlatpakUser).is_err());
    }
}
