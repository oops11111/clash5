use super::CmdResult;
use crate::enhance::rules::{RuleOverview, rule_overview};

#[tauri::command]
#[specta::specta]
pub async fn clash5_rule_overview() -> CmdResult<RuleOverview> {
    Ok(rule_overview())
}
