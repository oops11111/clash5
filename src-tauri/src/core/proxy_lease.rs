use anyhow::{Context as _, Result, ensure};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

const MAX_LEASE_BYTES: u64 = 16 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum ProxyTarget {
    Global { host: String, port: u16 },
    Pac { url: String },
}

impl ProxyTarget {
    pub(crate) fn global(address: &str) -> Result<Self> {
        let (host, port) = address.rsplit_once(':').context("Invalid OS proxy endpoint")?;
        let host = host.trim().trim_matches(['[', ']']);
        ensure!(!host.is_empty(), "OS proxy host is empty");
        let port = port.parse()?;
        ensure!(port != 0, "OS proxy port is zero");
        let host = host
            .parse::<std::net::IpAddr>()
            .map_or_else(|_| host.to_ascii_lowercase(), |ip| ip.to_string());
        Ok(Self::Global { host, port })
    }
}

#[cfg(any(windows, test))]
pub(crate) fn windows_targets(global_enabled: bool, server: &str, pac: &str) -> Result<Vec<ProxyTarget>> {
    let mut targets = Vec::new();
    if global_enabled {
        for part in server.split(';').map(str::trim).filter(|part| !part.is_empty()) {
            let address = part.split_once('=').map_or(part, |(_, address)| address);
            targets.push(ProxyTarget::global(address)?);
        }
        ensure!(!targets.is_empty(), "Enabled system proxy has no readable endpoint");
    }
    if !pac.trim().is_empty() {
        targets.push(ProxyTarget::Pac { url: pac.into() });
    }
    Ok(targets)
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProxyLease {
    version: u8,
    pub(crate) owner: String,
    targets: Vec<ProxyTarget>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Ownership {
    Ours,
    Other,
    Mixed,
}

#[derive(Clone, Copy)]
pub(crate) enum ForeignProxyPolicy {
    PreserveMarker,
    RetireMarker,
}

impl ProxyLease {
    pub(crate) fn ownership(&self, enabled: &[ProxyTarget]) -> Ownership {
        let owned = enabled.iter().filter(|target| self.targets.contains(target)).count();
        if owned == 0 {
            Ownership::Other
        } else if owned == enabled.len() {
            Ownership::Ours
        } else {
            Ownership::Mixed
        }
    }
}

pub(crate) async fn recover_previous<Read, ReadFuture, Clear, ClearFuture>(
    store: &LeaseStore,
    owner: Option<&str>,
    foreign_policy: ForeignProxyPolicy,
    read_os: Read,
    clear: Clear,
) -> Result<()>
where
    Read: FnOnce() -> ReadFuture,
    ReadFuture: std::future::Future<Output = Result<Vec<ProxyTarget>>>,
    Clear: FnOnce() -> ClearFuture,
    ClearFuture: std::future::Future<Output = Result<()>>,
{
    let Some(lease) = store.read()? else {
        return Ok(());
    };
    if owner == Some(lease.owner.as_str()) {
        return Ok(());
    }
    let enabled = read_os().await?;
    match lease.ownership(&enabled) {
        Ownership::Other => {
            ensure!(
                enabled.is_empty() || matches!(foreign_policy, ForeignProxyPolicy::RetireMarker),
                "Another system proxy is active; recovery was stopped. Disable the clash5 system proxy switch before starting again to preserve it"
            );
            store.remove()
        }
        Ownership::Ours => {
            clear().await?;
            store.remove()
        }
        Ownership::Mixed => {
            anyhow::bail!("Proxy recovery found both clash5 and another proxy target; existing settings were preserved")
        }
    }
}

pub(crate) struct LeaseStore(PathBuf);

impl LeaseStore {
    pub(crate) fn new(home: &Path) -> Self {
        Self(home.join(".proxy-lease.json"))
    }

    pub(crate) fn read(&self) -> Result<Option<ProxyLease>> {
        let metadata = match fs::symlink_metadata(&self.0) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        ensure!(
            metadata.is_file() && !metadata.file_type().is_symlink(),
            "Proxy recovery marker is not a regular file"
        );
        ensure!(metadata.len() <= MAX_LEASE_BYTES, "Proxy recovery marker is oversized");
        let mut bytes = Vec::new();
        fs::File::open(&self.0)?
            .take(MAX_LEASE_BYTES + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() as u64 <= MAX_LEASE_BYTES,
            "Proxy recovery marker is oversized"
        );
        let lease: ProxyLease = serde_json::from_slice(&bytes)?;
        ensure!(
            lease.version == 1 && !lease.targets.is_empty(),
            "Unsupported proxy recovery marker"
        );
        Ok(Some(lease))
    }

    fn write(&self, lease: &ProxyLease) -> Result<()> {
        let bytes = serde_json::to_vec(lease)?;
        ensure!(
            bytes.len() as u64 <= MAX_LEASE_BYTES,
            "Proxy recovery marker is oversized"
        );
        let temporary = self.0.with_extension(format!("tmp-{}", nanoid::nanoid!()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt as _;
            options.mode(0o600);
        }
        let mut output = options.open(&temporary)?;
        let _cleanup = scopeguard::guard(temporary.clone(), |path| {
            let _ = fs::remove_file(path);
        });
        output.write_all(&bytes)?;
        output.sync_all()?;
        drop(output);
        fs::rename(temporary, &self.0)?;
        Ok(())
    }

    // Keep both endpoints until the OS write has settled, including across a crash mid-switch.
    pub(crate) fn begin(&self, owner: &str, target: &ProxyTarget) -> Result<()> {
        let mut lease = self.read()?.unwrap_or_else(|| ProxyLease {
            version: 1,
            owner: owner.into(),
            targets: Vec::new(),
        });
        ensure!(lease.owner == owner, "Previous proxy cleanup has not completed");
        if !lease.targets.contains(target) {
            lease.targets.push(target.clone());
        }
        self.write(&lease)
    }

    pub(crate) fn settle(&self, owner: &str, target: ProxyTarget) -> Result<()> {
        self.write(&ProxyLease {
            version: 1,
            owner: owner.into(),
            targets: vec![target],
        })
    }

    pub(crate) fn remove(&self) -> Result<()> {
        match fs::remove_file(&self.0) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}
