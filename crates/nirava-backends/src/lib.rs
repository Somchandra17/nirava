pub mod apps;
pub mod discovery;
pub mod display_guard;
pub mod niri;
pub mod power;
#[cfg(feature = "audio")]
pub mod audio;

use anyhow::{Result, bail};
use std::path::PathBuf;
use tokio::process::Command;

pub fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"))
}

pub fn state_home() -> PathBuf {
    std::env::var_os("XDG_STATE_HOME").map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/state"))
}

pub async fn command(program: &str, args: &[&str]) -> Result<String> {
    use tokio::io::AsyncReadExt;
    use std::process::Stdio;
    let mut child = Command::new(program).args(args).kill_on_drop(true)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    let stdout=child.stdout.take().ok_or_else(||anyhow::anyhow!("Missing command stdout"))?;
    let stderr=child.stderr.take().ok_or_else(||anyhow::anyhow!("Missing command stderr"))?;
    let collect=async {
        let read=async {
            let mut out=Vec::new();let mut err=Vec::new();
            let mut bounded_out=stdout.take(4_194_305);
            let mut bounded_err=stderr.take(65_537);
            tokio::try_join!(bounded_out.read_to_end(&mut out),bounded_err.read_to_end(&mut err))?;
            if out.len()>4_194_304||err.len()>65_536{bail!("Backend response too large");}
            Ok::<_,anyhow::Error>((out,err))
        };
        let ((out,err),status)=tokio::try_join!(read,async {Ok::<_,anyhow::Error>(child.wait().await?)})?;
        if !status.success(){bail!("{program}: {}",String::from_utf8_lossy(&err).trim());}
        Ok::<_,anyhow::Error>(String::from_utf8(out)?)
    };
    tokio::time::timeout(std::time::Duration::from_secs(4),collect).await?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]async fn argument_vectors_preserve_metacharacters()->Result<()> {
        assert_eq!(command("printf",&["%s","$(touch never-created);quoted"]).await?,"$(touch never-created);quoted");Ok(())
    }
    #[tokio::test]async fn output_is_bounded(){assert!(command("head",&["-c","4194305","/dev/zero"]).await.is_err());}
    #[tokio::test]async fn command_failure_is_reported(){assert!(command("false",&[]).await.is_err());}
}
