//! User-scope Plugin mutation commands.
//!
//! The preview request names an Agent, an operation and a Plugin identity; the
//! apply request names a token and nothing else. Neither carries an executable,
//! an argument, a scope or a filesystem location, and neither command receives
//! the database or the Library — the only durable state a mutation produces is
//! the one the official CLI writes for itself.

use chrono::{DateTime, Utc};

use crate::core::error::AppError;
use crate::core::plugin_inventory::{self, PluginInventoryDto};
use crate::core::plugin_mutation::{
    self, MutationFuture, MutationHost, MutationInvocation, MutationRunFailure,
    PluginMutationApplyOutcomeDto, PluginMutationApplyRequest, PluginMutationPreviewOutcomeDto,
    PluginMutationPreviewRequest, PluginMutationState,
};

/// The real clock, the real token source and the two real CLIs.
///
/// Every seam a test replaces with a fixture is answered here by the thing it
/// stands in for, so the rules the tests pin are the rules production runs.
struct LiveHost;

impl MutationHost for LiveHost {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }

    fn new_token(&self) -> String {
        plugin_mutation::live_token()
    }

    fn collect_inventory(&self) -> MutationFuture<'_, PluginInventoryDto> {
        Box::pin(async { plugin_inventory::collect(Utc::now().to_rfc3339()).await })
    }

    fn run<'a>(
        &'a self,
        invocation: &'a MutationInvocation,
    ) -> MutationFuture<'a, Result<(), MutationRunFailure>> {
        Box::pin(plugin_mutation::run_live_mutation(invocation))
    }
}

/// Describes exactly what a mutation would run, without running anything.
///
/// Never fails as a whole: an unsupported operation, a missing record or an
/// unmet precondition is a typed diagnostic the page can localize, not an
/// error that would hide which of them happened.
#[tauri::command]
pub async fn preview_plugin_mutation(
    state: tauri::State<'_, PluginMutationState>,
    request: PluginMutationPreviewRequest,
) -> Result<PluginMutationPreviewOutcomeDto, AppError> {
    Ok(plugin_mutation::preview_mutation(&LiveHost, state.inner(), request).await)
}

/// Consumes one preview token and runs exactly the mutation it described.
#[tauri::command]
pub async fn apply_plugin_mutation(
    state: tauri::State<'_, PluginMutationState>,
    request: PluginMutationApplyRequest,
) -> Result<PluginMutationApplyOutcomeDto, AppError> {
    Ok(plugin_mutation::apply_mutation(&LiveHost, state.inner(), request).await)
}
