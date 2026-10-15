//! Typed package operations. Plans are handed to an interactive package manager;
//! this module never executes a transaction or accepts arbitrary commands.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageSource { Rpm, FlatpakUser, FlatpakSystem }
impl PackageSource {
    pub fn label(self) -> &'static str { match self {
        Self::Rpm => "RPM", Self::FlatpakUser => "Flatpak · User", Self::FlatpakSystem => "Flatpak · System",
    }}
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageAction { Install, Update, Remove }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageRecord {
    pub source: PackageSource, pub id: String, pub name: String,
    pub version: String, pub architecture: String, pub origin: String,
}
impl PackageRecord {
    pub fn request(&self, action: PackageAction) -> PackageRequest {
        let target=if self.source==PackageSource::Rpm && action==PackageAction::Remove {
            format!("{}-{}.{}",self.name,self.version,self.architecture)
        } else {self.id.clone()};
        PackageRequest {source:self.source,action,target,remote:String::new()}
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageRemote { pub source: PackageSource, pub name: String }
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppsInventory {
    #[serde(default)] pub simulation: bool,
    pub loaded: bool, pub rpm_available: bool, pub dnf_available: bool,
    pub flatpak_available: bool, pub immutable: bool,
    pub packages: Vec<PackageRecord>, pub remotes: Vec<PackageRemote>, pub errors: Vec<String>,
}
impl AppsInventory {
    pub fn demo() -> Self {
        Self { loaded: true, simulation: true, rpm_available: true, dnf_available: true, flatpak_available: true,
            packages: vec![
                PackageRecord { source: PackageSource::Rpm, id: "kitty.x86_64".into(), name: "kitty".into(), version: "0.40.0-1.demo".into(), architecture: "x86_64".into(), origin: "Installed RPM database".into() },
                PackageRecord { source: PackageSource::FlatpakUser, id: "app/org.gnome.Calculator/x86_64/stable".into(), name: "Calculator".into(), version: "49 (demo)".into(), architecture: "x86_64".into(), origin: "flathub".into() },
            ], remotes: vec![PackageRemote { source: PackageSource::FlatpakUser, name: "flathub".into() }], ..Self::default() }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageRequest {
    pub source: PackageSource, pub action: PackageAction, pub target: String,
    #[serde(default)] pub remote: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackagePlan {
    pub request: PackageRequest, pub program: String, pub args: Vec<String>,
    pub description: String,
}
fn atom(value: &str) -> bool {
    !value.is_empty() && value.len() <= 255 && value.as_bytes()[0].is_ascii_alphanumeric()
        && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"._+-".contains(&b))
}
pub fn flatpak_ref(value: &str) -> bool {
    let parts: Vec<_> = value.split('/').collect();
    if parts.len() == 4 {
        return parts[0] == "app" && flatpak_ref(parts[1]) && atom(parts[2]) && atom(parts[3]);
    }
    parts.len() == 1 && value.len() <= 255 && value.split('.').count() >= 3
        && value.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
}
impl PackagePlan {
    pub fn build(request: PackageRequest) -> Result<Self, String> {
        use PackageAction::*;
        use PackageSource::*;
        let verb = match request.action { Install => "install", Update => "update", Remove => "remove" };
        let (program, args) = if request.source == Rpm {
            if !atom(&request.target) || request.target.ends_with(".rpm") { return Err("Use one exact RPM package name; paths, options, patterns and shell syntax are not accepted".into()); }
            if !request.remote.is_empty() { return Err("RPM actions use the configured DNF repositories".into()); }
            let mut args = vec!["--".into(), "/usr/bin/dnf".into(), "--setopt=assumeyes=False".into(),
                "--setopt=defaultyes=False".into(), "--setopt=assumeno=False".into(),
                if request.action == Update { "upgrade".into() } else { verb.into() }, "--".into(), request.target.clone()];
            if request.action == Remove {args.insert(args.len()-2,"--noautoremove".into());}
            ("/usr/bin/sudo",args)
        } else {
            if !flatpak_ref(&request.target) { return Err("Use a complete Flatpak application ID or app/ID/architecture/branch reference".into()); }
            if request.action == Install && !atom(&request.remote) { return Err("Choose a configured Flatpak remote".into()); }
            if request.action != Install && !request.remote.is_empty() { return Err("Only installation takes a remote".into()); }
            let mut args = vec![if request.source == FlatpakUser { "--user".into() } else { "--system".into() },
                if request.action == Remove { "uninstall".into() } else { verb.into() }, "--app".into(), "--".into()];
            if request.action == Install { args.push(request.remote.clone()); }
            args.push(request.target.clone());
            ("/usr/bin/flatpak", args)
        };
        let description = format!("{} {} ({})", verb, request.target, request.source.label());
        Ok(Self { request, program: program.into(), args, description })
    }
    pub fn display_command(&self) -> String {
        // Every dynamic argument passed build()'s strict identifier validation.
        std::iter::once(self.program.as_str()).chain(self.args.iter().map(String::as_str)).collect::<Vec<_>>().join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(source: PackageSource, action: PackageAction, target: &str) -> PackageRequest {
        PackageRequest { source, action, target: target.into(), remote: String::new() }
    }
    #[test]
    fn reject_commands_paths_options_patterns_and_multiple_packages() {
        for target in ["--all", "-y", "/tmp/pkg.rpm", "https://host/pkg.rpm", "vim;reboot", "$(reboot)", "a b", "kernel*", "", "a\nb", "a\t-y", "download.rpm"] {
            assert!(PackagePlan::build(request(PackageSource::Rpm, PackageAction::Install, target)).is_err(), "{target}");
        }
    }
    #[test]
    fn rpm_keeps_interactive_confirmation_and_exact_target() {
        let plan = PackagePlan::build(request(PackageSource::Rpm, PackageAction::Remove, "kitty.x86_64")).unwrap();
        assert_eq!(plan.program, "/usr/bin/sudo");
        assert!(plan.args.iter().any(|arg| arg == "--setopt=assumeyes=False"));
        assert_eq!(&plan.args[plan.args.len()-3..], ["--noautoremove", "--", "kitty.x86_64"]);
    }
    #[test]
    fn flatpak_preserves_scope_architecture_branch_and_data() {
        let plan = PackagePlan::build(request(PackageSource::FlatpakSystem, PackageAction::Remove, "app/org.gnome.Calculator/aarch64/stable")).unwrap();
        assert_eq!(plan.args, ["--system", "uninstall", "--app", "--", "app/org.gnome.Calculator/aarch64/stable"]);
        assert!(!plan.args.iter().any(|a| ["-y", "--delete-data", "--force-remove", "--noninteractive"].contains(&a.as_str())));
        for target in ["org.gnome.*", "runtime/org.gnome.Platform/x86_64/49", "org.gnome.App;id", "org..App", "org.gnome.App/../../etc"] {
            assert!(PackagePlan::build(request(PackageSource::FlatpakUser, PackageAction::Remove, target)).is_err());
        }
    }
    #[test]
    fn removing_an_installed_rpm_targets_its_version_and_architecture() {
        let package=PackageRecord {source:PackageSource::Rpm,id:"kernel-core.x86_64".into(),name:"kernel-core".into(),version:"6.2-1.fc44".into(),architecture:"x86_64".into(),origin:String::new()};
        let plan=PackagePlan::build(package.request(PackageAction::Remove)).unwrap();
        assert_eq!(plan.args.last().unwrap(),"kernel-core-6.2-1.fc44.x86_64");
        let update=PackagePlan::build(package.request(PackageAction::Update)).unwrap();
        assert_eq!(update.args.last().unwrap(),"kernel-core.x86_64");
    }
    #[test]
    fn installation_requires_an_explicit_remote() {
        let mut req = request(PackageSource::FlatpakUser, PackageAction::Install, "org.gnome.Calculator");
        assert!(PackagePlan::build(req.clone()).is_err());
        req.remote = "flathub".into();
        assert!(PackagePlan::build(req.clone()).is_ok());
        req.remote = "https://untrusted.example/repo".into();
        assert!(PackagePlan::build(req).is_err());
    }
}
