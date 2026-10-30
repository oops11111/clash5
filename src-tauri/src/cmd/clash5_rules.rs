use super::CmdResult;
use crate::enhance::rules::{RuleNote, rule_notes};

#[tauri::command]
#[specta::specta]
pub async fn clash5_rule_notes() -> CmdResult<Vec<RuleNote>> {
    Ok(rule_notes())
}
