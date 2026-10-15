//! All tests use disposable files. Hash checks guard both commit and restore.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};

pub fn digest(bytes: &[u8]) -> String { format!("{:x}", Sha256::digest(bytes)) }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileChange {
    pub path: PathBuf,
    pub original_hash: String,
    pub original: String,
    pub proposed: String,
    pub description: String,
}

impl FileChange {
    pub fn read(path: impl Into<PathBuf>, proposed: String, description: String) -> Result<Self> {
        let path = path.into();
        ensure_regular_path(&path)?;
        let original = fs::read_to_string(&path).with_context(|| format!("Read {}", path.display()))?;
        Ok(Self { original_hash: digest(original.as_bytes()), path, original, proposed, description })
    }

    pub fn read_expected(path: impl Into<PathBuf>, original: &str, proposed: String, description: String) -> Result<Self> {
        let change=Self::read(path,proposed,description)?;
        if change.original!=original {bail!("Configuration changed while preparing the preview; review again");}
        Ok(change)
    }

    pub fn check_original(&self) -> Result<()> {
        ensure_regular_path(&self.path)?;
        if digest(&fs::read(&self.path)?) != self.original_hash { bail!("Configuration changed externally; review a fresh preview"); }
        Ok(())
    }

    pub fn commit(&self) -> Result<CommittedChange> {
        self.check_original()?;
        let backup = backup_name(&self.path);
        use std::os::unix::fs::OpenOptionsExt;
        let mut backup_file = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&backup)?;
        backup_file.set_permissions(fs::metadata(&self.path)?.permissions())?;
        backup_file.write_all(self.original.as_bytes())?;
        backup_file.sync_all()?;
        // Recheck after backup, before replacement. Never replace the symlink itself.
        self.check_original()?;
        atomic_replace(&self.path, self.proposed.as_bytes())?;
        Ok(CommittedChange { path: self.path.clone(), backup, written_hash: digest(self.proposed.as_bytes()), original_hash: self.original_hash.clone() })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommittedChange {
    pub path: PathBuf,
    pub backup: PathBuf,
    pub written_hash: String,
    pub original_hash: String,
}

impl CommittedChange {
    pub fn check_restore(&self) -> Result<Vec<u8>> {
        ensure_regular_path(&self.path)?;
        ensure_regular_path(&self.backup)?;
        if digest(&fs::read(&self.path)?) != self.written_hash { bail!("Restore refused: the file was edited after Nirava wrote it"); }
        let original = fs::read(&self.backup)?;
        if digest(&original) != self.original_hash { bail!("Backup was modified; restore refused"); }
        Ok(original)
    }
    pub fn restore(&self) -> Result<()> {
        atomic_replace(&self.path, &self.check_restore()?)
    }
}

pub fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    ensure_regular_path(path)?;
    let parent = path.parent().context("Target has no parent")?;
    let permissions = fs::metadata(path)?.permissions();
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.as_file().set_permissions(permissions)?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    ensure_regular_path(path)?;
    staged.persist(path).map_err(|e| e.error)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

/// Reject symlinked ancestors as well as symlink targets. This reduces path
/// redirection risks; ordinary filesystems still offer no compare-and-swap
/// against an uncooperative external editor between a final check and rename.
pub fn ensure_regular_path(path: &Path) -> Result<()> {
    let absolute=if path.is_absolute(){path.to_path_buf()}else{std::env::current_dir()?.join(path)};
    let mut prefix=PathBuf::new();
    for part in absolute.components() {
        if matches!(part,std::path::Component::ParentDir){bail!("Parent traversal is not supported for configuration writes");}
        prefix.push(part);
        let metadata=fs::symlink_metadata(&prefix)?;
        if metadata.file_type().is_symlink(){bail!("{} is a symlink; explicit target integration is required",prefix.display());}
    }
    if !fs::symlink_metadata(path)?.is_file(){bail!("Configuration target must be a regular file");}
    Ok(())
}

fn backup_name(path: &Path) -> PathBuf {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos();
    // Unique suffix prevents a second update in the same second overwriting a backup.
    let mut name = path.as_os_str().to_os_string();
    name.push(format!(".bak-{}",chrono::Local::now().format("%Y%m%d-%H%M%S")));
    if Path::new(&name).exists() {name.push(format!("-{stamp}"));}
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn commit_restore_and_concurrent_edits() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("settings.toml");
        fs::write(&path, "# user's comment\nvolume = 1\n")?;
        let change = FileChange::read(&path, "# user's comment\nvolume = 2\n".into(), "volume".into())?;
        let committed = change.commit()?;
        assert_eq!(fs::read_to_string(&path)?, change.proposed);
        committed.restore()?;
        assert_eq!(fs::read_to_string(&path)?, change.original);
        fs::write(&path, "external change")?;
        assert!(change.commit().is_err());
        Ok(())
    }
    #[test]
    fn restore_does_not_destroy_later_edits() -> Result<()> {
        let dir = tempfile::tempdir()?; let path = dir.path().join("config");
        fs::write(&path, "before")?;
        let committed = FileChange::read(&path, "after".into(), "change".into())?.commit()?;
        fs::write(&path, "user edited after")?;
        assert!(committed.restore().is_err());
        assert_eq!(fs::read_to_string(path)?, "user edited after");
        Ok(())
    }
    #[test]
    fn staging_refuses_an_intervening_edit() -> Result<()> {
        let dir=tempfile::tempdir()?;let path=dir.path().join("config");fs::write(&path,"new user edit")?;
        assert!(FileChange::read_expected(&path,"older read","proposed from older read".into(),"test".into()).is_err());
        assert_eq!(fs::read_to_string(path)?,"new user edit");Ok(())
    }
    #[test]
    fn symlinked_parents_and_backups_are_refused() -> Result<()> {
        let dir=tempfile::tempdir()?;let real=dir.path().join("real");fs::create_dir(&real)?;
        let path=real.join("config");fs::write(&path,"before")?;
        std::os::unix::fs::symlink(&real,dir.path().join("alias"))?;
        assert!(FileChange::read(dir.path().join("alias/config"),"after".into(),"test".into()).is_err());
        let committed=FileChange::read(&path,"after".into(),"test".into())?.commit()?;
        let other=dir.path().join("other");fs::write(&other,"before")?;fs::remove_file(&committed.backup)?;
        std::os::unix::fs::symlink(other,&committed.backup)?;assert!(committed.restore().is_err());
        assert_eq!(fs::read_to_string(path)?,"after");Ok(())
    }
    #[test]
    fn symlinks_are_not_replaced() -> Result<()> {
        let dir = tempfile::tempdir()?; let target = dir.path().join("target"); let link = dir.path().join("link");
        fs::write(&target,"original")?; std::os::unix::fs::symlink(&target,&link)?;
        assert!(FileChange::read(link,"replacement".into(),"test".into()).is_err());
        assert_eq!(fs::read_to_string(target)?,"original"); Ok(())
    }
}
