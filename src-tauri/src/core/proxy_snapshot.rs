use super::proxy_lease::ProxyTarget;
use anyhow::Result;
#[cfg(not(windows))]
use anyhow::ensure;

#[cfg(windows)]
pub(crate) fn enabled_targets() -> Result<Vec<ProxyTarget>> {
    use winreg::{
        RegKey,
        enums::{HKEY_CURRENT_USER, KEY_QUERY_VALUE},
        types::FromRegValue,
    };
    fn value<T: FromRegValue + Default>(key: &RegKey, name: &str) -> std::io::Result<T> {
        match key.get_value(name) {
            Ok(value) => Ok(value),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(T::default()),
            Err(error) => Err(error),
        }
    }
    let key = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(
        "Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings",
        KEY_QUERY_VALUE,
    )?;
    super::proxy_lease::windows_targets(
        value::<u32>(&key, "ProxyEnable")? != 0,
        &value::<String>(&key, "ProxyServer")?,
        &value::<String>(&key, "AutoConfigURL")?,
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn enabled_targets() -> Result<Vec<ProxyTarget>> {
    let snapshot = sysproxy::Sysproxy::snapshot()?;
    let mut targets = Vec::new();
    for endpoint in [snapshot.http, snapshot.https, snapshot.socks] {
        if endpoint.switched_on {
            ensure!(endpoint.enable, "Enabled proxy endpoint could not be read");
            targets.push(ProxyTarget::global(&format!("{}:{}", endpoint.host, endpoint.port))?);
        }
    }
    if snapshot.auto_switched_on {
        ensure!(snapshot.auto.enable, "Enabled PAC URL could not be read");
        targets.push(ProxyTarget::Pac { url: snapshot.auto.url });
    }
    Ok(targets)
}

#[cfg(target_os = "linux")]
pub(crate) fn enabled_targets() -> Result<Vec<ProxyTarget>> {
    let mut targets = Vec::new();
    if sysproxy::Sysproxy::get_enable()? {
        for endpoint in [
            sysproxy::Sysproxy::get_http()?,
            sysproxy::Sysproxy::get_https()?,
            sysproxy::Sysproxy::get_socks()?,
        ] {
            if !endpoint.host.is_empty() {
                targets.push(ProxyTarget::global(&format!("{}:{}", endpoint.host, endpoint.port))?);
            }
        }
        ensure!(!targets.is_empty(), "Enabled Linux proxy has no readable endpoint");
    }
    let pac = sysproxy::Autoproxy::get_auto_proxy()?;
    if pac.enable {
        ensure!(!pac.url.is_empty(), "Enabled PAC URL could not be read");
        targets.push(ProxyTarget::Pac { url: pac.url });
    }
    Ok(targets)
}
