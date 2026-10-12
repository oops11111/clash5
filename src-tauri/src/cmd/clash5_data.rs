use super::{CmdResult, coded_error};
use clash5_data::{Binding, DataError, Node, Source};
use serde::Serialize;

fn failure(error: DataError) -> super::CommandFailure {
    coded_error(&error.to_string(), "clash5 node data operation failed")
}

#[derive(Serialize)]
pub struct ManualNodeIdentity {
    stable_id: String,
}

#[tauri::command]
pub async fn clash5_register_manual_node(node: Node) -> CmdResult<ManualNodeIdentity> {
    crate::config::profiles::with_clash5_binding_store(move |store| {
        let stable_id = store.upsert_manual(&node)?;
        Ok(ManualNodeIdentity { stable_id })
    })
    .await
    .map_err(failure)
}

#[tauri::command]
pub async fn clash5_bind_node(binding: Binding) -> CmdResult {
    crate::config::profiles::with_clash5_binding_store(move |store| store.upsert_binding(&binding))
        .await
        .map_err(failure)
}

#[tauri::command]
pub async fn clash5_list_bindings(source: Option<Source>) -> CmdResult<Vec<Binding>> {
    crate::config::profiles::with_clash5_binding_store(move |store| store.bindings(source.as_ref()))
        .await
        .map_err(failure)
}

#[tauri::command]
pub async fn clash5_purge_orphans(source: Option<Source>) -> CmdResult<usize> {
    crate::config::profiles::with_clash5_binding_store(move |store| store.purge_orphans(source.as_ref()))
        .await
        .map_err(failure)
}
