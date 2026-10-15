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
        if fs::symlink_metadata(&path)?.file_type().is_symlink() {
            bail!("{} is a symlink; explicit target integration is required", path.display());
        }
        let original = fs::read_to_string(&path).with_context(|| format!("Read {}", path.display()))?;
        Ok(Self { original_hash: digest(original.as_bytes()), path, original, proposed, description })
    }

    pub fn check_original(&self) -> Result<()> {
        if fs::symlink_metadata(&self.path)?.file_type().is_symlink() { bail!("Target became a symlink"); }
        if digest(&fs::read(&self.path)?) != self.original_hash { bail!("Configuration changed externally; review a fresh preview"); }
        Ok(())
    }

    pub fn commit(&self) -> Result<CommittedChange> {
        self.check_original()?;
        let backup = backup_name(&self.path);
        let mut backup_file = fs::OpenOptions::new().write(true).create_new(true).open(&backup)?;
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
        if fs::symlink_metadata(&self.path)?.file_type().is_symlink() { bail!("Restore target became a symlink"); }
        if digest(&fs::read(&self.path)?) != self.written_hash { bail!("Restore refused: the file was edited after Plain wrote it"); }
        let original = fs::read(&self.backup)?;
        if digest(&original) != self.original_hash { bail!("Backup was modified; restore refused"); }
        Ok(original)
    }
    pub fn restore(&self) -> Result<()> {
        atomic_replace(&self.path, &self.check_restore()?)
    }
}

pub fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path.parent().context("Target has no parent")?;
    let permissions = fs::metadata(path)?.permissions();
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.as_file().set_permissions(permissions)?;
    staged.write_all(bytes)?;
    staged.as_file().sync_all()?;
    staged.persist(path).map_err(|e| e.error)?;
    fs::File::open(parent)?.sync_all()?;
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
    fn symlinks_are_not_replaced() -> Result<()> {
        let dir = tempfile::tempdir()?; let target = dir.path().join("target"); let link = dir.path().join("link");
        fs::write(&target,"original")?; std::os::unix::fs::symlink(&target,&link)?;
        assert!(FileChange::read(link,"replacement".into(),"test".into()).is_err());
        assert_eq!(fs::read_to_string(target)?,"original"); Ok(())
    }
}
