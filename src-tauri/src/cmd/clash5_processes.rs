use super::{CmdResult, coded_error};
use serde::Serialize;
use std::collections::BTreeMap;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

#[derive(Serialize, specta::Type)]
pub struct RunningProcess {
    pub name: String,
    /// Executable path when the system reports it; a process owned by another user may not.
    pub path: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub async fn clash5_list_processes() -> CmdResult<Vec<RunningProcess>> {
    tokio::task::spawn_blocking(collect_processes)
        .await
        .map_err(|_| coded_error("C5-PROC-001", "the process list is unavailable"))
}

/// One entry per program name, sorted by name. Programs with several processes keep the first path seen.
fn collect_processes() -> Vec<RunningProcess> {
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::All,
        true,
        ProcessRefreshKind::nothing().with_exe(UpdateKind::OnlyIfNotSet),
    );
    let mut by_name = BTreeMap::new();
    for process in system.processes().values() {
        let name = process.name().to_string_lossy().into_owned();
        if name.is_empty() {
            continue;
        }
        let path = process.exe().map(|path| path.to_string_lossy().into_owned());
        by_name.entry(name.clone()).or_insert(RunningProcess { name, path });
    }
    by_name.into_values().collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn running_programs_are_listed_once_and_sorted_by_name() {
        let processes = collect_processes();
        assert!(!processes.is_empty());
        let names: Vec<&str> = processes.iter().map(|process| process.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(names, sorted);
    }
}
