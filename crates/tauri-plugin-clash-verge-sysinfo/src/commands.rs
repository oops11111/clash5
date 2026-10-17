use parking_lot::RwLock;
use serde::Serialize;
use tauri::{AppHandle, Runtime, State, command};
use tauri_plugin_clipboard_manager::ClipboardExt as _;

use crate::Platform;

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct UptimeMs(pub u64);

impl specta::Type for UptimeMs {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <specta_typescript::Number as specta::Type>::definition(types)
    }
}

#[derive(Serialize, specta::Type)]
pub struct SystemInfo {
    pub system_name: String,
    pub system_version: String,
    pub system_kernel_version: String,
    pub system_arch: String,
    pub app_version: String,
    pub app_core_mode: String,
    pub app_is_admin: bool,
}

impl From<Platform> for SystemInfo {
    fn from(platform: Platform) -> Self {
        Self {
            system_name: platform.sysinfo.system_name,
            system_version: platform.sysinfo.system_version,
            system_kernel_version: platform.sysinfo.system_kernel_version,
            system_arch: platform.sysinfo.system_arch,
            app_version: platform.appinfo.app_version,
            app_core_mode: platform.appinfo.app_core_mode,
            app_is_admin: platform.appinfo.app_is_admin,
        }
    }
}

#[command]
#[specta::specta]
pub fn get_system_info(state: State<'_, RwLock<Platform>>) -> Result<SystemInfo, String> {
    let platform = state.inner().read();
    Ok(SystemInfo::from(platform.clone()))
}

/// 获取应用的运行时间（毫秒）
#[command]
#[specta::specta]
pub fn get_app_uptime(state: State<'_, RwLock<Platform>>) -> Result<UptimeMs, String> {
    let uptime_ms = state.inner().read().appinfo.app_startup_time.elapsed().as_millis();
    Ok(UptimeMs(uptime_ms.min(u64::MAX as u128) as u64))
}

#[command]
#[specta::specta]
pub fn export_diagnostic_info<R: Runtime>(
    app_handle: AppHandle<R>,
    state: State<'_, RwLock<Platform>>,
) -> Result<(), String> {
    let info = state.inner().read().to_string();
    let clipboard = app_handle.clipboard();
    clipboard.write_text(info).map_err(|error| error.to_string())
}
