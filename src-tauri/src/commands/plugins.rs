//! Read-only Plugin inventory command.
//!
//! The request carries nothing: no executable, no path, no argument. Every
//! process comes from the fixed table in `core::plugin_inventory`, and the
//! response lives only in this reply — nothing here writes to disk, to the
//! database or to the log.

use chrono::Utc;

use crate::core::error::AppError;
use crate::core::plugin_inventory::{self, PluginInventoryDto};

/// Returns what Codex and Claude Code currently report about their Plugins.
///
/// This never fails as a whole: an absent CLI or a broken listing becomes a
/// diagnostic inside the Agent it belongs to, so the other Agent stays usable.
#[tauri::command]
pub async fn get_plugin_inventory() -> Result<PluginInventoryDto, AppError> {
    Ok(plugin_inventory::collect(Utc::now().to_rfc3339()).await)
}
