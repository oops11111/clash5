use anyhow::{Context as _, Result};
use std::{process::Stdio, time::Duration};
use tokio::{
    io::AsyncWriteExt as _,
    process::{Child, Command},
    sync::Mutex,
};

const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// A supervised sing-box process. Its token-bearing configuration is delivered through stdin
/// and the pipe is closed immediately, so `sing-box run -c stdin` can finish reading it.
pub struct SingboxProcess {
    pid: u32,
    child: Mutex<Child>,
}

impl SingboxProcess {
    /// Starts an already-resolved bundled sidecar command with a complete JSON configuration.
    /// The caller must include `run -c stdin` in `command`.
    pub async fn start(mut command: std::process::Command, config: &[u8]) -> Result<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null());

        let mut child = Command::from(command)
            .kill_on_drop(true)
            .spawn()
            .context("failed to start the sing-box sidecar")?;
        let pid = child.id().context("sing-box did not expose a process ID")?;
        let mut stdin = child.stdin.take().context("sing-box stdin was not piped")?;
        if let Err(error) = stdin.write_all(config).await {
            drop(stdin);
            let _ = child.kill().await;
            return Err(error).context("failed to send the sing-box config through stdin");
        }
        // Closing stdin is required: sing-box reads the JSON document until EOF.
        drop(stdin);

        Ok(Self {
            pid,
            child: Mutex::new(child),
        })
    }

    pub const fn pid(&self) -> u32 {
        self.pid
    }

    pub async fn is_running(&self) -> Result<bool> {
        let mut child = self.child.lock().await;
        Ok(child
            .try_wait()
            .context("failed to inspect the sing-box process")?
            .is_none())
    }

    pub async fn stop(&self) -> Result<()> {
        let mut child = self.child.lock().await;
        if child
            .try_wait()
            .context("failed to inspect the sing-box process before shutdown")?
            .is_some()
        {
            return Ok(());
        }
        child.start_kill().context("failed to signal the sing-box process")?;
        tokio::time::timeout(SHUTDOWN_TIMEOUT, child.wait())
            .await
            .context("sing-box did not exit after shutdown")?
            .context("failed to wait for the sing-box process")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn closes_config_pipe_without_emitting_config_to_logs() -> Result<()> {
        let mut command = std::process::Command::new(if cfg!(windows) { "cmd" } else { "sh" });
        if cfg!(windows) {
            command.args(["/C", "more > NUL"]);
        } else {
            command.args(["-c", "cat >/dev/null"]);
        }

        let config = br#"{"inbounds":[{"password":"ephemeral-session-secret"}]}"#;
        let child = SingboxProcess::start(command, config).await?;
        tokio::time::timeout(Duration::from_secs(3), async {
            while child.is_running().await? {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Result::<()>::Ok(())
        })
        .await
        .context("test child did not exit after stdin was closed")??;
        Ok(())
    }
}
