use super::CmdResult;
use crate::config::{Config, PerAppProxySettings};
use serde::Serialize;

/// A process that can be targeted by per-app proxy.
#[derive(Clone, Debug, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProcessCandidate {
    pub pid: u32,
    pub process_name: String,
    pub process_path: String,
    /// Windows process creation time (100-nanosecond intervals since epoch).
    pub created_at: u64,
}

/// List processes that have recently routed traffic through the core.
/// On Windows this supplements the list with a live process enumeration.
#[tauri::command]
#[specta::specta]
pub async fn clash5_list_process_candidates() -> CmdResult<Vec<ProcessCandidate>> {
    #[cfg(windows)]
    {
        enumerate_windows_processes()
    }
    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

#[cfg(windows)]
fn enumerate_windows_processes() -> CmdResult<Vec<ProcessCandidate>> {
    use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};

    let mut candidates = Vec::new();
    // Enumerate all processes via CreateToolhelp32Snapshot.
    unsafe {
        let snapshot = windows_sys::Win32::System::Diagnostics::ToolHelp::CreateToolhelp32Snapshot(
            windows_sys::Win32::System::Diagnostics::ToolHelp::TH32CS_SNAPPROCESS,
            0,
        );
        if snapshot == windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE {
            return Err(super::coded_error("C5-PROXY-002", "failed to create process snapshot"));
        }
        let snapshot_handle = OwnedHandle::from_raw_handle(snapshot);

        let mut entry: windows_sys::Win32::System::Diagnostics::ToolHelp::PROCESSENTRY32 = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<windows_sys::Win32::System::Diagnostics::ToolHelp::PROCESSENTRY32>() as u32;

        let has_first = windows_sys::Win32::System::Diagnostics::ToolHelp::Process32First(
            snapshot_handle.as_raw_handle(),
            &mut entry,
        );
        if has_first == 0 {
            return Ok(Vec::new());
        }

        loop {
            let pid = entry.th32ProcessID;
            if pid != 0 && pid != std::process::id() {
                let name = String::from_utf8_lossy(
                    &entry
                        .szExeFile
                        .iter()
                        .take_while(|&&c| c != 0)
                        .map(|&c| c as u8)
                        .collect::<Vec<_>>(),
                )
                .to_string();

                // Try to get creation time.
                let created_at = get_process_creation_time(pid);
                if let Some(created_at) = created_at {
                    candidates.push(ProcessCandidate {
                        pid,
                        process_name: name,
                        process_path: String::new(), // path resolution requires additional privileges
                        created_at,
                    });
                }
            }

            let has_next = windows_sys::Win32::System::Diagnostics::ToolHelp::Process32Next(
                snapshot_handle.as_raw_handle(),
                &mut entry,
            );
            if has_next == 0 {
                break;
            }
        }
    }

    Ok(candidates)
}

#[cfg(windows)]
fn get_process_creation_time(pid: u32) -> Option<u64> {
    use std::os::windows::io::{AsRawHandle as _, FromRawHandle as _, OwnedHandle};
    use windows_sys::Win32::{
        Foundation::FILETIME,
        System::Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
    };

    unsafe {
        let raw = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if raw.is_null() {
            return None;
        }
        let handle = OwnedHandle::from_raw_handle(raw);
        let mut creation = FILETIME::default();
        let mut exit = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        if GetProcessTimes(handle.as_raw_handle(), &mut creation, &mut exit, &mut kernel, &mut user) == 0 {
            return None;
        }
        Some((u64::from(creation.dwHighDateTime) << 32) | u64::from(creation.dwLowDateTime))
    }
}

/// Read the typed per-app proxy configuration.
#[tauri::command]
#[specta::specta]
pub async fn clash5_get_per_app_proxy_settings() -> CmdResult<PerAppProxySettings> {
    let verge = Config::verge().await;
    let settings = verge.latest_arc().per_app_proxy.clone().unwrap_or_default();
    Ok(settings)
}

/// Update the typed per-app proxy configuration.
#[tauri::command]
#[specta::specta]
pub async fn clash5_set_per_app_proxy_settings(settings: PerAppProxySettings) -> CmdResult<PerAppProxySettings> {
    let verge = Config::verge().await;
    verge.edit_draft(|config| {
        config.per_app_proxy = Some(settings.clone());
    });
    verge.apply();
    verge
        .data_arc()
        .save_file()
        .await
        .map_err(|error| super::coded_error("C5-PROXY-005", error))?;
    Ok(settings)
}
