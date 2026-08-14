//! User-scope Plugin mutation for Codex and Claude Code.
//!
//! Everything this module can execute comes from a fixed Agent/operation table.
//! The frontend names an Agent, an operation and a Plugin identity — never an
//! executable, an option, a scope or a working directory. See
//! `openspec/changes/manage-user-scoped-plugins`.

use std::future::Future;
use std::io::ErrorKind;
use std::pin::Pin;
use std::process::Stdio;
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::core::plugin_inventory::{
    kill_and_reap, read_bounded, PluginAgent, PluginAgentInventoryDto, PluginAvailability,
    PluginCapability, PluginEnablement, PluginInventoryDto, PluginInventoryItemDto, PluginPresence,
    PluginScope, StreamRead,
};

/// An identity component longer than this is not something either CLI names, so
/// it is refused rather than truncated into a selector the user never chose.
pub const MAX_IDENTITY_BYTES: usize = 512;
/// How long a confirmed intent stays valid. Long enough to read a destructive
/// dialog, short enough that an abandoned one cannot be applied later.
pub const PREVIEW_TTL_SECONDS: i64 = 120;
/// The most intents kept at once. A preview costs memory only, but an unbounded
/// map is still a way to grow the process without limit.
pub const MAX_PENDING_PREVIEWS: usize = 128;

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

/// The normalized operations the frontend may name. There is no sixth
/// operation, and none of them carries an option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginOperation {
    Install,
    Update,
    Remove,
    Enable,
    Disable,
}

impl PluginOperation {
    pub const ALL: [PluginOperation; 5] = [
        PluginOperation::Install,
        PluginOperation::Update,
        PluginOperation::Remove,
        PluginOperation::Enable,
        PluginOperation::Disable,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            PluginOperation::Install => "install",
            PluginOperation::Update => "update",
            PluginOperation::Remove => "remove",
            PluginOperation::Enable => "enable",
            PluginOperation::Disable => "disable",
        }
    }

    /// Removal is the only operation the official CLI cannot undo for us, so it
    /// is the only one that earns a destructive confirmation.
    pub fn is_destructive(&self) -> bool {
        matches!(self, PluginOperation::Remove)
    }
}

/// Everything a failed mutation is allowed to say. A raw stream, a path or a
/// parser excerpt could carry a marketplace token or the user's home layout, so
/// a failure is one code and, when a child ran, its exit status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginMutationCode {
    OperationUnsupported,
    IdentityNotFound,
    ScopeUnsupported,
    PreconditionFailed,
    PreviewExpired,
    StalePreview,
    InteractiveConfirmationRequired,
    VerificationFailed,
    CliMissing,
    Timeout,
    NonZeroExit,
    OutputTooLarge,
}

impl PluginMutationCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginMutationCode::OperationUnsupported => "operation_unsupported",
            PluginMutationCode::IdentityNotFound => "identity_not_found",
            PluginMutationCode::ScopeUnsupported => "scope_unsupported",
            PluginMutationCode::PreconditionFailed => "precondition_failed",
            PluginMutationCode::PreviewExpired => "preview_expired",
            PluginMutationCode::StalePreview => "stale_preview",
            PluginMutationCode::InteractiveConfirmationRequired => {
                "interactive_confirmation_required"
            }
            PluginMutationCode::VerificationFailed => "verification_failed",
            PluginMutationCode::CliMissing => "cli_missing",
            PluginMutationCode::Timeout => "timeout",
            PluginMutationCode::NonZeroExit => "non_zero_exit",
            PluginMutationCode::OutputTooLarge => "output_too_large",
        }
    }
}

// ---------------------------------------------------------------------------
// The fixed mutation table
// ---------------------------------------------------------------------------

/// One mutation AgentDeck is allowed to make. Every field is `'static`: an
/// executable, an option or a scope can only enter this table through a code
/// change, never through a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MutationCommand {
    pub agent: PluginAgent,
    pub operation: PluginOperation,
    pub program: &'static str,
    /// Fixed arguments, always ending with the option terminator so the
    /// selector that follows cannot be read as a flag.
    pub args: &'static [&'static str],
}

const MUTATION_COMMANDS: [MutationCommand; 7] = [
    MutationCommand {
        agent: PluginAgent::Codex,
        operation: PluginOperation::Install,
        program: "codex",
        args: &["plugin", "add", "--json", "--"],
    },
    MutationCommand {
        agent: PluginAgent::Codex,
        operation: PluginOperation::Remove,
        program: "codex",
        args: &["plugin", "remove", "--json", "--"],
    },
    MutationCommand {
        agent: PluginAgent::ClaudeCode,
        operation: PluginOperation::Install,
        program: "claude",
        args: &["plugin", "install", "--scope", "user", "--"],
    },
    MutationCommand {
        agent: PluginAgent::ClaudeCode,
        operation: PluginOperation::Update,
        program: "claude",
        args: &["plugin", "update", "--scope", "user", "--"],
    },
    MutationCommand {
        agent: PluginAgent::ClaudeCode,
        operation: PluginOperation::Remove,
        program: "claude",
        args: &["plugin", "uninstall", "--scope", "user", "--"],
    },
    MutationCommand {
        agent: PluginAgent::ClaudeCode,
        operation: PluginOperation::Enable,
        program: "claude",
        args: &["plugin", "enable", "--scope", "user", "--"],
    },
    MutationCommand {
        agent: PluginAgent::ClaudeCode,
        operation: PluginOperation::Disable,
        program: "claude",
        args: &["plugin", "disable", "--scope", "user", "--"],
    },
];

pub fn mutation_commands() -> &'static [MutationCommand] {
    &MUTATION_COMMANDS
}

/// The table entry for one Agent and operation, or nothing when that Agent's
/// CLI has no such documented command.
pub fn mutation_command(
    agent: PluginAgent,
    operation: PluginOperation,
) -> Option<&'static MutationCommand> {
    MUTATION_COMMANDS
        .iter()
        .find(|command| command.agent == agent && command.operation == operation)
}

/// What the Plugins page is allowed to offer for an Agent, in the order the
/// table declares it.
pub fn supported_operations(agent: PluginAgent) -> Vec<PluginOperation> {
    MUTATION_COMMANDS
        .iter()
        .filter(|command| command.agent == agent)
        .map(|command| command.operation)
        .collect()
}

// ---------------------------------------------------------------------------
// Selectors
// ---------------------------------------------------------------------------

/// One mutation, fully resolved and ready to run.
///
/// `scope` is the scope the fixed command acts on, not a scope read from
/// inventory: it is always `user` because no other scope has a table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MutationInvocation {
    pub agent: PluginAgent,
    pub operation: PluginOperation,
    pub scope: PluginScope,
    pub program: &'static str,
    pub args: Vec<String>,
}

impl MutationInvocation {
    /// The command exactly as the confirmation dialog shows it. Every part came
    /// from the fixed table or from a validated identity component, so there is
    /// nothing here to quote or escape.
    pub fn display(&self) -> String {
        std::iter::once(self.program.to_string())
            .chain(self.args.iter().cloned())
            .collect::<Vec<_>>()
            .join(" ")
    }
}

/// A component that could turn into an option, split a selector or carry a
/// terminator is not an identity AgentDeck will act on.
fn validate_component(value: &str) -> Result<(), PluginMutationCode> {
    let usable = !value.is_empty()
        && value.len() <= MAX_IDENTITY_BYTES
        && !value.starts_with('-')
        && !value.chars().any(|character| character.is_control());
    if usable {
        Ok(())
    } else {
        Err(PluginMutationCode::PreconditionFailed)
    }
}

/// Whether an inventory record's scope permits a mutation for this Agent.
///
/// Claude Code publishes a scope per installed Plugin, so update/remove/enable/
/// disable require `user`. Available-only records carry no scope (parsed as
/// `unknown`), and the backend always passes `--scope user`, so install accepts
/// `unknown` as well. Codex publishes none for any record; its `plugin add`/
/// `plugin remove` contract is user-scope by definition.
pub fn scope_permits(
    agent: PluginAgent,
    operation: PluginOperation,
    scope: PluginScope,
) -> Result<(), PluginMutationCode> {
    match agent {
        PluginAgent::Codex => Ok(()),
        PluginAgent::ClaudeCode if scope == PluginScope::User => Ok(()),
        PluginAgent::ClaudeCode
            if operation == PluginOperation::Install && scope == PluginScope::Unknown =>
        {
            Ok(())
        }
        PluginAgent::ClaudeCode => Err(PluginMutationCode::ScopeUnsupported),
    }
}

/// Whether an identity is one AgentDeck will look up at all. Checked before any
/// inventory is collected, so a hostile request costs no CLI invocation.
pub fn validate_identity(plugin_id: &str, marketplace: &str) -> Result<(), PluginMutationCode> {
    validate_component(plugin_id)?;
    validate_component(marketplace)
}

/// The only way production builds a mutation: a table entry plus one selector
/// assembled from two validated identity components.
pub fn invocation_for(
    agent: PluginAgent,
    operation: PluginOperation,
    plugin_id: &str,
    marketplace: &str,
) -> Result<MutationInvocation, PluginMutationCode> {
    let command =
        mutation_command(agent, operation).ok_or(PluginMutationCode::OperationUnsupported)?;
    validate_component(plugin_id)?;
    validate_component(marketplace)?;

    let mut args: Vec<String> = command.args.iter().map(|arg| arg.to_string()).collect();
    args.push(format!("{plugin_id}@{marketplace}"));

    Ok(MutationInvocation {
        agent,
        operation,
        scope: PluginScope::User,
        program: command.program,
        args,
    })
}

// ---------------------------------------------------------------------------
// The outside world, behind one seam
// ---------------------------------------------------------------------------

pub type MutationFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// What a failed child process is allowed to report upwards. Raw output never
/// leaves the runner, so a failure is a code and, when a child ran, its status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MutationRunFailure {
    pub code: PluginMutationCode,
    pub exit_status: Option<i32>,
}

impl MutationRunFailure {
    pub fn of(code: PluginMutationCode) -> Self {
        Self {
            code,
            exit_status: None,
        }
    }
}

/// Everything a mutation needs from outside its own logic: the clock, the token
/// source, the inventory and the CLI. Production wires these to the real thing;
/// tests wire them to fixtures, so every rule below is exercised without
/// touching a user's Plugins.
pub trait MutationHost: Send + Sync {
    fn now(&self) -> DateTime<Utc>;
    fn new_token(&self) -> String;
    fn collect_inventory(&self) -> MutationFuture<'_, PluginInventoryDto>;
    fn run<'a>(
        &'a self,
        invocation: &'a MutationInvocation,
    ) -> MutationFuture<'a, Result<(), MutationRunFailure>>;
}

/// The production token source: unguessable, single-purpose and never persisted.
pub fn live_token() -> String {
    uuid::Uuid::new_v4().to_string()
}

// ---------------------------------------------------------------------------
// Preview state
// ---------------------------------------------------------------------------

/// One confirmed intent, held only in memory until it is applied or expires.
#[derive(Debug, Clone)]
pub struct MutationIntent {
    pub agent: PluginAgent,
    pub operation: PluginOperation,
    pub plugin_id: String,
    pub marketplace: String,
    pub invocation: MutationInvocation,
    pub fingerprint: String,
    /// The versions the preview saw, kept so verification can prove an update
    /// actually moved.
    pub installed_version: Option<String>,
    pub available_version: Option<String>,
}

#[derive(Debug, Clone)]
struct PendingPreview {
    token: String,
    expires_at: DateTime<Utc>,
    intent: MutationIntent,
}

/// The pending previews and the single mutation gate.
///
/// Tauri manages one of these for the whole application. It holds no path, no
/// database handle and no Plugin payload, and nothing in it outlives the
/// process.
#[derive(Default)]
pub struct PluginMutationState {
    pending: Mutex<Vec<PendingPreview>>,
    /// Held for the whole of one apply, so two Agents never write Plugin state
    /// at the same time. A read-only refresh does not take it.
    gate: Mutex<()>,
}

impl PluginMutationState {
    /// The tokens currently valid, in insertion order.
    pub async fn pending_tokens(&self) -> Vec<String> {
        self.pending
            .lock()
            .await
            .iter()
            .map(|entry| entry.token.clone())
            .collect()
    }

    async fn store(
        &self,
        token: String,
        expires_at: DateTime<Utc>,
        intent: MutationIntent,
        now: DateTime<Utc>,
    ) {
        let mut pending = self.pending.lock().await;
        pending.retain(|entry| entry.expires_at > now);
        while pending.len() >= MAX_PENDING_PREVIEWS {
            // Deterministic: the intent closest to expiring is the one the user
            // is least likely to still be looking at.
            let earliest = pending
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.expires_at)
                .map(|(index, _)| index)
                .expect("a non-empty list has a minimum");
            pending.remove(earliest);
        }
        pending.push(PendingPreview {
            token,
            expires_at,
            intent,
        });
    }

    /// Takes a token out of the map for good. A token that was expired, already
    /// applied or never issued all look the same from here, which is what makes
    /// a replay indistinguishable from an expiry.
    async fn consume(&self, token: &str, now: DateTime<Utc>) -> Option<MutationIntent> {
        let mut pending = self.pending.lock().await;
        pending.retain(|entry| entry.expires_at > now);
        let index = pending.iter().position(|entry| entry.token == token)?;
        Some(pending.remove(index).intent)
    }
}

// ---------------------------------------------------------------------------
// Preview
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMutationPreviewRequest {
    pub agent: PluginAgent,
    pub operation: PluginOperation,
    pub plugin_id: String,
    pub marketplace: String,
}

/// What the confirmation dialog shows and the apply call carries back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMutationPreviewDto {
    pub token: String,
    pub expires_at: String,
    pub agent: PluginAgent,
    pub operation: PluginOperation,
    pub plugin_id: String,
    pub marketplace: String,
    /// Always `user`: the fixed table has no entry for any other scope.
    pub scope: PluginScope,
    pub argv_display: String,
    pub destructive: bool,
    pub base_fingerprint: String,
}

/// Agent and operation are optional because a token AgentDeck never issued
/// names neither: reporting a guessed Agent would be worse than reporting none.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMutationDiagnosticDto {
    pub agent: Option<PluginAgent>,
    pub operation: Option<PluginOperation>,
    pub code: PluginMutationCode,
    pub exit_status: Option<i32>,
}

impl PluginMutationDiagnosticDto {
    fn of(agent: PluginAgent, operation: PluginOperation, code: PluginMutationCode) -> Self {
        Self {
            agent: Some(agent),
            operation: Some(operation),
            code,
            exit_status: None,
        }
    }

    /// A failure that happened before any intent was recovered.
    fn anonymous(code: PluginMutationCode) -> Self {
        Self {
            agent: None,
            operation: None,
            code,
            exit_status: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PluginMutationPreviewOutcomeDto {
    Ready {
        preview: PluginMutationPreviewDto,
    },
    Failed {
        diagnostic: PluginMutationDiagnosticDto,
    },
}

/// The exact state a preview was taken against. Serialized in declaration order
/// and hashed, so any change the CLIs report between preview and apply — a
/// version, a capability, an enablement — produces a different fingerprint.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FingerprintInput<'a> {
    agent: &'a str,
    operation: &'a str,
    cli_version: Option<&'a str>,
    capabilities: Vec<&'a str>,
    item: &'a PluginInventoryItemDto,
}

fn fingerprint_of(
    block: &PluginAgentInventoryDto,
    operation: PluginOperation,
    item: &PluginInventoryItemDto,
) -> String {
    let input = FingerprintInput {
        agent: block.agent.as_str(),
        operation: operation.as_str(),
        cli_version: block.cli_version.as_deref(),
        capabilities: block
            .capabilities
            .iter()
            .map(PluginCapability::as_str)
            .collect(),
        item,
    };
    let canonical = serde_json::to_vec(&input).expect("a DTO of plain fields serializes");
    hex::encode(Sha256::digest(canonical))
}

fn agent_block(
    inventory: &PluginInventoryDto,
    agent: PluginAgent,
) -> Option<&PluginAgentInventoryDto> {
    inventory.agents.iter().find(|block| block.agent == agent)
}

/// Finds the one record that matches the requested identity exactly. A near
/// match is not a match: acting on a different marketplace's Plugin because the
/// ids looked similar is the failure this rules out.
fn exact_item<'a>(
    block: &'a PluginAgentInventoryDto,
    plugin_id: &str,
    marketplace: &str,
) -> Option<&'a PluginInventoryItemDto> {
    block.items.iter().find(|item| {
        item.plugin_id == plugin_id && item.marketplace.as_deref() == Some(marketplace)
    })
}

/// Whether the record is in a state where the operation makes sense.
///
/// Install accepts a record whose presence is not `installed` rather than one
/// explicitly `not_installed`: Claude Code's available-only records report no
/// presence at all, and the post-mutation verification — not this check — is
/// what proves the Plugin actually arrived.
fn precondition_holds(operation: PluginOperation, item: &PluginInventoryItemDto) -> bool {
    match operation {
        PluginOperation::Install => {
            item.available == PluginAvailability::Available
                && item.installed != PluginPresence::Installed
        }
        PluginOperation::Update | PluginOperation::Remove => {
            item.installed == PluginPresence::Installed
        }
        PluginOperation::Enable => {
            item.installed == PluginPresence::Installed
                && item.enabled == PluginEnablement::Disabled
        }
        PluginOperation::Disable => {
            item.installed == PluginPresence::Installed && item.enabled == PluginEnablement::Enabled
        }
    }
}

/// Builds one reviewable intent from freshly collected inventory.
///
/// Nothing here starts a process: a preview is a promise about what would run,
/// and the checks that can be made without the CLI are made before it is asked.
pub async fn preview_mutation(
    host: &dyn MutationHost,
    state: &PluginMutationState,
    request: PluginMutationPreviewRequest,
) -> PluginMutationPreviewOutcomeDto {
    let PluginMutationPreviewRequest {
        agent,
        operation,
        plugin_id,
        marketplace,
    } = request;
    let refuse = |code| PluginMutationPreviewOutcomeDto::Failed {
        diagnostic: PluginMutationDiagnosticDto::of(agent, operation, code),
    };

    if mutation_command(agent, operation).is_none() {
        return refuse(PluginMutationCode::OperationUnsupported);
    }
    if let Err(code) = validate_identity(&plugin_id, &marketplace) {
        return refuse(code);
    }

    let inventory = host.collect_inventory().await;
    let Some(block) = agent_block(&inventory, agent) else {
        return refuse(PluginMutationCode::IdentityNotFound);
    };
    let Some(item) = exact_item(block, &plugin_id, &marketplace) else {
        return refuse(PluginMutationCode::IdentityNotFound);
    };
    if let Err(code) = scope_permits(agent, operation, item.scope) {
        return refuse(code);
    }
    if !precondition_holds(operation, item) {
        return refuse(PluginMutationCode::PreconditionFailed);
    }

    let invocation = match invocation_for(agent, operation, &plugin_id, &marketplace) {
        Ok(invocation) => invocation,
        Err(code) => return refuse(code),
    };
    let fingerprint = fingerprint_of(block, operation, item);
    let now = host.now();
    let expires_at = now + ChronoDuration::seconds(PREVIEW_TTL_SECONDS);
    let token = host.new_token();

    let preview = PluginMutationPreviewDto {
        token: token.clone(),
        expires_at: expires_at.to_rfc3339(),
        agent,
        operation,
        plugin_id: plugin_id.clone(),
        marketplace: marketplace.clone(),
        scope: PluginScope::User,
        argv_display: invocation.display(),
        destructive: operation.is_destructive(),
        base_fingerprint: fingerprint.clone(),
    };

    state
        .store(
            token,
            expires_at,
            MutationIntent {
                agent,
                operation,
                plugin_id,
                marketplace,
                invocation,
                fingerprint,
                installed_version: item.installed_version.clone(),
                available_version: item.available_version.clone(),
            },
            now,
        )
        .await;

    PluginMutationPreviewOutcomeDto::Ready { preview }
}

// ---------------------------------------------------------------------------
// Bounded mutation runner
// ---------------------------------------------------------------------------

/// Deadline for one mutation. The same ten seconds the read-only runner uses:
/// a Plugin CLI that has not finished by then is not going to be waited for.
pub const MUTATION_TIMEOUT: Duration = Duration::from_secs(10);

/// The phrases that mean "this needs a confirmation AgentDeck will not give".
///
/// Taken from Claude Code 2.1.231's own description of `-y, --yes`: "for a
/// plugin installed by running a marketplace-declared command: accept the
/// displayed command without the confirmation prompt (required when stdin or
/// stdout is not a TTY)". Matching is deliberately narrow — anything not on
/// this list stays an ordinary non-zero exit rather than being explained away.
pub const NON_TTY_CONFIRMATION_PHRASES: [&str; 2] =
    ["marketplace-declared command", "confirmation prompt"];

/// Whether this Agent and operation can hit a marketplace confirmation at all.
/// Only Claude Code's install and update run marketplace-declared commands.
pub fn may_require_confirmation(agent: PluginAgent, operation: PluginOperation) -> bool {
    matches!(
        (agent, operation),
        (
            PluginAgent::ClaudeCode,
            PluginOperation::Install | PluginOperation::Update
        )
    )
}

/// True when bounded stderr matches one of the pinned phrases.
///
/// The bytes are borrowed for the length of this call and never stored: the
/// answer is one boolean, so no marketplace text, path or credential can travel
/// out with it.
fn stderr_asks_for_confirmation(stderr: &[u8]) -> bool {
    let text = String::from_utf8_lossy(stderr).to_lowercase();
    NON_TTY_CONFIRMATION_PHRASES
        .iter()
        .any(|phrase| text.contains(phrase))
}

/// Runs one already-built mutation under a deadline and a per-stream limit.
///
/// Nothing captured survives this function. Success is `()` because exit 0 is
/// only an intermediate state — the fresh inventory collected afterwards is
/// what decides whether the mutation actually happened.
async fn run_bounded_mutation(
    mut command: tokio::process::Command,
    deadline: Duration,
    classify_confirmation: bool,
) -> Result<(), MutationRunFailure> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(MutationRunFailure::of(PluginMutationCode::CliMissing))
        }
        Err(_) => return Err(MutationRunFailure::of(PluginMutationCode::NonZeroExit)),
    };

    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    // Both streams are drained concurrently: a child that fills one pipe while
    // the runner waits on the other would deadlock until the deadline.
    let mut out_task = tokio::spawn(read_bounded(stdout));
    let mut err_task = tokio::spawn(read_bounded(stderr));

    let sleep = tokio::time::sleep(deadline);
    tokio::pin!(sleep);

    let mut out_done: Option<StreamRead> = None;
    let mut err_done: Option<StreamRead> = None;
    let mut timed_out = false;
    let mut too_large = false;

    while !timed_out && !too_large && (out_done.is_none() || err_done.is_none()) {
        tokio::select! {
            joined = &mut out_task, if out_done.is_none() => {
                out_done = Some(joined
                    .map_err(|_| MutationRunFailure::of(PluginMutationCode::NonZeroExit))?
                    .map_err(|_| MutationRunFailure::of(PluginMutationCode::NonZeroExit))?);
            }
            joined = &mut err_task, if err_done.is_none() => {
                err_done = Some(joined
                    .map_err(|_| MutationRunFailure::of(PluginMutationCode::NonZeroExit))?
                    .map_err(|_| MutationRunFailure::of(PluginMutationCode::NonZeroExit))?);
            }
            _ = &mut sleep => timed_out = true,
        }
        too_large = matches!(out_done, Some(StreamRead::TooLarge))
            || matches!(err_done, Some(StreamRead::TooLarge));
    }

    if timed_out || too_large {
        out_task.abort();
        err_task.abort();
        return Err(MutationRunFailure::of(
            kill_and_reap(
                &mut child,
                if timed_out {
                    PluginMutationCode::Timeout
                } else {
                    PluginMutationCode::OutputTooLarge
                },
            )
            .await,
        ));
    }

    let status = tokio::select! {
        status = child.wait() => status,
        _ = &mut sleep => {
            return Err(MutationRunFailure::of(
                kill_and_reap(&mut child, PluginMutationCode::Timeout).await,
            ))
        }
    };
    let status = status.map_err(|_| MutationRunFailure::of(PluginMutationCode::NonZeroExit))?;

    if status.success() {
        return Ok(());
    }

    let asks = classify_confirmation
        && matches!(&err_done, Some(StreamRead::Bounded(bytes)) if stderr_asks_for_confirmation(bytes));

    Err(MutationRunFailure {
        code: if asks {
            PluginMutationCode::InteractiveConfirmationRequired
        } else {
            PluginMutationCode::NonZeroExit
        },
        // A confirmation requirement is described by its own code; an exit
        // status would add nothing the frontend can act on.
        exit_status: if asks { None } else { status.code() },
    })
}

/// The only way production starts a mutation: the program and every option come
/// from the fixed table, and the single selector was validated before this.
pub async fn run_live_mutation(invocation: &MutationInvocation) -> Result<(), MutationRunFailure> {
    let mut process = tokio::process::Command::new(invocation.program);
    process.args(&invocation.args);
    run_bounded_mutation(
        process,
        MUTATION_TIMEOUT,
        may_require_confirmation(invocation.agent, invocation.operation),
    )
    .await
}

// ---------------------------------------------------------------------------
// Apply
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMutationApplyRequest {
    /// The only thing the frontend may send. Everything the mutation does was
    /// decided when the preview was built.
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum PluginMutationApplyOutcomeDto {
    // A variant's fields need their own rename: the container attribute renames
    // the variants, not what is inside them.
    #[serde(rename_all = "camelCase")]
    Verified {
        agent: PluginAgent,
        operation: PluginOperation,
        plugin_id: String,
        marketplace: String,
        /// The inventory that proved the operation happened. The page replaces
        /// its own data with this rather than editing the row it clicked.
        inventory: PluginInventoryDto,
    },
    Failed {
        diagnostic: PluginMutationDiagnosticDto,
        /// Present when a fresh inventory was collected before the failure was
        /// decided, so the page can show current state instead of stale rows.
        inventory: Option<PluginInventoryDto>,
    },
}

/// Whether freshly collected inventory proves the operation actually happened.
///
/// Exit 0 is only the CLI's opinion; this is the observable evidence. An
/// unreadable Plugin listing, an unknown state or an unchanged record are all
/// "not proven" — never "probably fine".
fn postcondition_proven(fresh: &PluginInventoryDto, intent: &MutationIntent) -> bool {
    let Some(block) = agent_block(fresh, intent.agent) else {
        return false;
    };
    if !block.capabilities.contains(&PluginCapability::PluginList) {
        // The refresh failed for the one capability that describes Plugins, so
        // this response says nothing about the target either way.
        return false;
    }

    let item = exact_item(block, &intent.plugin_id, &intent.marketplace);
    match intent.operation {
        PluginOperation::Install => {
            matches!(item, Some(record) if record.installed == PluginPresence::Installed)
        }
        PluginOperation::Remove => match item {
            None => true,
            Some(record) => record.installed == PluginPresence::NotInstalled,
        },
        PluginOperation::Enable => {
            matches!(item, Some(record) if record.enabled == PluginEnablement::Enabled)
        }
        PluginOperation::Disable => {
            matches!(item, Some(record) if record.enabled == PluginEnablement::Disabled)
        }
        PluginOperation::Update => match item {
            Some(record) if record.installed == PluginPresence::Installed => {
                let moved = record.installed_version.is_some()
                    && record.installed_version != intent.installed_version;
                match &intent.available_version {
                    // The preview named the version the update was for, so
                    // arriving at a different one is not that update.
                    Some(expected) => moved && record.installed_version.as_ref() == Some(expected),
                    None => moved,
                }
            }
            _ => false,
        },
    }
}

/// Recomputes the preview's fingerprint against freshly collected inventory.
///
/// Returns `None` when the record is gone: an identity that disappeared between
/// preview and apply is drift, not a reason to act on whatever replaced it.
fn current_fingerprint(inventory: &PluginInventoryDto, intent: &MutationIntent) -> Option<String> {
    let block = agent_block(inventory, intent.agent)?;
    let item = exact_item(block, &intent.plugin_id, &intent.marketplace)?;
    Some(fingerprint_of(block, intent.operation, item))
}

/// Consumes one intent and, if the world still looks the way the user confirmed
/// it, runs exactly that mutation.
///
/// The whole body holds the mutation gate, so two Agents never write Plugin
/// state at once, and the token is taken before the CLI starts — a timeout or a
/// failure afterwards cannot be retried by resubmitting the same confirmation.
pub async fn apply_mutation(
    host: &dyn MutationHost,
    state: &PluginMutationState,
    request: PluginMutationApplyRequest,
) -> PluginMutationApplyOutcomeDto {
    let _gate = state.gate.lock().await;

    let Some(intent) = state.consume(&request.token, host.now()).await else {
        return PluginMutationApplyOutcomeDto::Failed {
            diagnostic: PluginMutationDiagnosticDto::anonymous(PluginMutationCode::PreviewExpired),
            inventory: None,
        };
    };

    let refuse = |code, exit_status, inventory| PluginMutationApplyOutcomeDto::Failed {
        diagnostic: PluginMutationDiagnosticDto {
            agent: Some(intent.agent),
            operation: Some(intent.operation),
            code,
            exit_status,
        },
        inventory,
    };

    let fresh = host.collect_inventory().await;
    if current_fingerprint(&fresh, &intent).as_deref() != Some(intent.fingerprint.as_str()) {
        return refuse(PluginMutationCode::StalePreview, None, Some(fresh));
    }

    if let Err(failure) = host.run(&intent.invocation).await {
        return refuse(failure.code, failure.exit_status, None);
    }

    let verified = host.collect_inventory().await;
    if !postcondition_proven(&verified, &intent) {
        return refuse(PluginMutationCode::VerificationFailed, None, Some(verified));
    }

    PluginMutationApplyOutcomeDto::Verified {
        agent: intent.agent,
        operation: intent.operation,
        plugin_id: intent.plugin_id,
        marketplace: intent.marketplace,
        inventory: verified,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;

    use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};

    use crate::core::plugin_inventory::{
        normalize, parse_claude_plugins, PluginAgent, PluginAgentInventoryDto, PluginAvailability,
        PluginCapability, PluginEnablement, PluginInventoryDto, PluginInventoryItemDto,
        PluginPresence, PluginScope, PluginUpdateState,
    };

    /// A string that must never appear in anything AgentDeck serializes or
    /// stores: it stands in for a path, a token or a Plugin payload.
    const SENTINEL: &str = "sentinel-secret";

    fn at(seconds: i64) -> DateTime<Utc> {
        Utc.timestamp_opt(1_700_000_000 + seconds, 0)
            .single()
            .expect("a valid fixture instant")
    }

    #[allow(clippy::too_many_arguments)]
    fn item(
        agent: PluginAgent,
        plugin_id: &str,
        marketplace: &str,
        installed: PluginPresence,
        available: PluginAvailability,
        scope: PluginScope,
        enabled: PluginEnablement,
    ) -> PluginInventoryItemDto {
        PluginInventoryItemDto {
            id: format!("{}:{marketplace}:{plugin_id}", agent.as_str()),
            agent,
            plugin_id: plugin_id.to_string(),
            display_name: plugin_id.to_string(),
            installed,
            available,
            installed_version: None,
            available_version: None,
            scope,
            marketplace: Some(marketplace.to_string()),
            enabled,
            update: PluginUpdateState::Unknown,
        }
    }

    fn inventory(items: Vec<PluginInventoryItemDto>) -> PluginInventoryDto {
        let agent_block = |agent: PluginAgent| PluginAgentInventoryDto {
            agent,
            cli_version: Some(match agent {
                PluginAgent::Codex => "codex-cli 0.144.5".to_string(),
                PluginAgent::ClaudeCode => "2.1.231 (Claude Code)".to_string(),
            }),
            capabilities: vec![
                PluginCapability::Version,
                PluginCapability::PluginList,
                PluginCapability::MarketplaceList,
            ],
            mutations: supported_operations(agent),
            marketplaces: Vec::new(),
            items: items
                .iter()
                .filter(|entry| entry.agent == agent)
                .cloned()
                .collect(),
            diagnostics: Vec::new(),
        };

        PluginInventoryDto {
            agents: vec![
                agent_block(PluginAgent::Codex),
                agent_block(PluginAgent::ClaudeCode),
            ],
            generated_at: "2023-11-14T22:13:20+00:00".to_string(),
        }
    }

    /// A Claude Code Plugin that is offered but not installed — the shape the
    /// spec's install scenario starts from.
    fn claude_available() -> PluginInventoryItemDto {
        item(
            PluginAgent::ClaudeCode,
            "reviewer",
            "official",
            PluginPresence::NotInstalled,
            PluginAvailability::Available,
            PluginScope::User,
            PluginEnablement::Unknown,
        )
    }

    /// Production Claude Code available-only JSON omits installed state and
    /// scope, so the parser must preserve both as unknown.
    fn parsed_claude_available_only() -> PluginInventoryItemDto {
        let parsed = parse_claude_plugins(
            br#"{
              "installed": [],
              "available": [{
                "pluginId": "reviewer@official",
                "name": "reviewer",
                "marketplaceName": "official",
                "version": "1.0",
                "source": "./plugins/reviewer"
              }]
            }"#,
        )
        .expect("production-shaped Claude Code JSON parses");
        normalize(parsed)
            .into_iter()
            .next()
            .expect("one available-only record")
    }

    /// What the fake CLI does when a mutation reaches it.
    #[derive(Debug, Clone, Copy)]
    enum RunOutcome {
        Succeeds,
        Fails(PluginMutationCode, Option<i32>),
    }

    /// The seams a mutation needs from the outside world. A fake answers with
    /// fixtures; production answers with a clock, a UUID and the real CLIs.
    struct FakeHost {
        /// Successive inventory responses; the last one repeats once consumed.
        inventories: Mutex<Vec<PluginInventoryDto>>,
        now: Mutex<DateTime<Utc>>,
        issued: AtomicUsize,
        collected: AtomicUsize,
        ran: AtomicUsize,
        running: AtomicUsize,
        max_running: AtomicUsize,
        invocations: Mutex<Vec<String>>,
        outcome: Mutex<RunOutcome>,
        /// Holds the fake CLI open long enough for a second apply to overlap it
        /// if the mutation gate were not doing its job.
        run_delay_ms: u64,
    }

    impl FakeHost {
        fn new(inventories: Vec<PluginInventoryDto>, start: DateTime<Utc>) -> Self {
            Self {
                inventories: Mutex::new(inventories),
                now: Mutex::new(start),
                issued: AtomicUsize::new(0),
                collected: AtomicUsize::new(0),
                ran: AtomicUsize::new(0),
                running: AtomicUsize::new(0),
                max_running: AtomicUsize::new(0),
                invocations: Mutex::new(Vec::new()),
                outcome: Mutex::new(RunOutcome::Succeeds),
                run_delay_ms: 0,
            }
        }

        fn with(inventory: PluginInventoryDto) -> Self {
            Self::new(vec![inventory], at(0))
        }

        fn advance(&self, milliseconds: i64) {
            let mut now = self.now.lock().expect("clock");
            *now += ChronoDuration::milliseconds(milliseconds);
        }

        fn fails_with(&self, code: PluginMutationCode, exit_status: Option<i32>) {
            *self.outcome.lock().expect("outcome") = RunOutcome::Fails(code, exit_status);
        }

        fn ran(&self) -> usize {
            self.ran.load(Ordering::SeqCst)
        }
    }

    impl MutationHost for FakeHost {
        fn now(&self) -> DateTime<Utc> {
            *self.now.lock().expect("clock")
        }

        fn new_token(&self) -> String {
            let issued = self.issued.fetch_add(1, Ordering::SeqCst);
            format!("token-{issued:04}")
        }

        fn collect_inventory(&self) -> MutationFuture<'_, PluginInventoryDto> {
            self.collected.fetch_add(1, Ordering::SeqCst);
            let mut queued = self.inventories.lock().expect("inventories");
            let next = if queued.len() > 1 {
                queued.remove(0)
            } else {
                queued.first().cloned().expect("at least one fixture")
            };
            Box::pin(async move { next })
        }

        fn run<'a>(
            &'a self,
            invocation: &'a MutationInvocation,
        ) -> MutationFuture<'a, Result<(), MutationRunFailure>> {
            self.ran.fetch_add(1, Ordering::SeqCst);
            self.invocations
                .lock()
                .expect("invocations")
                .push(invocation.display());
            let concurrent = self.running.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_running.fetch_max(concurrent, Ordering::SeqCst);
            let outcome = *self.outcome.lock().expect("outcome");
            let delay = self.run_delay_ms;

            Box::pin(async move {
                if delay > 0 {
                    tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                }
                self.running.fetch_sub(1, Ordering::SeqCst);
                match outcome {
                    RunOutcome::Succeeds => Ok(()),
                    RunOutcome::Fails(code, exit_status) => {
                        Err(MutationRunFailure { code, exit_status })
                    }
                }
            })
        }
    }

    fn ready(outcome: PluginMutationPreviewOutcomeDto) -> PluginMutationPreviewDto {
        match outcome {
            PluginMutationPreviewOutcomeDto::Ready { preview } => preview,
            PluginMutationPreviewOutcomeDto::Failed { diagnostic } => {
                panic!("expected a preview, got {:?}", diagnostic.code)
            }
        }
    }

    fn failed(outcome: PluginMutationPreviewOutcomeDto) -> PluginMutationCode {
        match outcome {
            PluginMutationPreviewOutcomeDto::Ready { preview } => {
                panic!("expected a failure, got {}", preview.argv_display)
            }
            PluginMutationPreviewOutcomeDto::Failed { diagnostic } => diagnostic.code,
        }
    }

    fn request(
        agent: PluginAgent,
        operation: PluginOperation,
        plugin_id: &str,
        marketplace: &str,
    ) -> PluginMutationPreviewRequest {
        PluginMutationPreviewRequest {
            agent,
            operation,
            plugin_id: plugin_id.to_string(),
            marketplace: marketplace.to_string(),
        }
    }

    /// The whole table as text, so a changed program, argument or ordering is a
    /// visible diff rather than a passing test.
    fn table() -> Vec<(String, String, String, Vec<String>)> {
        mutation_commands()
            .iter()
            .map(|command| {
                (
                    command.agent.as_str().to_string(),
                    command.operation.as_str().to_string(),
                    command.program.to_string(),
                    command.args.iter().map(|arg| arg.to_string()).collect(),
                )
            })
            .collect()
    }

    // ── The fixed mutation table ──

    #[test]
    fn matrix_is_the_documented_seven_commands() {
        let expected: Vec<(String, String, String, Vec<String>)> = vec![
            (
                "codex",
                "install",
                "codex",
                vec!["plugin", "add", "--json", "--"],
            ),
            (
                "codex",
                "remove",
                "codex",
                vec!["plugin", "remove", "--json", "--"],
            ),
            (
                "claude_code",
                "install",
                "claude",
                vec!["plugin", "install", "--scope", "user", "--"],
            ),
            (
                "claude_code",
                "update",
                "claude",
                vec!["plugin", "update", "--scope", "user", "--"],
            ),
            (
                "claude_code",
                "remove",
                "claude",
                vec!["plugin", "uninstall", "--scope", "user", "--"],
            ),
            (
                "claude_code",
                "enable",
                "claude",
                vec!["plugin", "enable", "--scope", "user", "--"],
            ),
            (
                "claude_code",
                "disable",
                "claude",
                vec!["plugin", "disable", "--scope", "user", "--"],
            ),
        ]
        .into_iter()
        .map(|(agent, operation, program, args)| {
            (
                agent.to_string(),
                operation.to_string(),
                program.to_string(),
                args.into_iter().map(str::to_string).collect(),
            )
        })
        .collect();

        assert_eq!(table(), expected);
    }

    #[test]
    fn matrix_codex_supports_only_install_and_remove() {
        assert_eq!(
            supported_operations(PluginAgent::Codex),
            vec![PluginOperation::Install, PluginOperation::Remove]
        );
        for absent in [
            PluginOperation::Update,
            PluginOperation::Enable,
            PluginOperation::Disable,
        ] {
            assert_eq!(
                mutation_command(PluginAgent::Codex, absent),
                None,
                "{absent:?} must be absent from the Codex matrix"
            );
        }
    }

    #[test]
    fn matrix_claude_code_supports_all_five_operations() {
        assert_eq!(
            supported_operations(PluginAgent::ClaudeCode),
            vec![
                PluginOperation::Install,
                PluginOperation::Update,
                PluginOperation::Remove,
                PluginOperation::Enable,
                PluginOperation::Disable,
            ]
        );
    }

    #[test]
    fn matrix_unsupported_operation_fails_before_an_argv_exists() {
        for absent in [
            PluginOperation::Update,
            PluginOperation::Enable,
            PluginOperation::Disable,
        ] {
            assert_eq!(
                invocation_for(PluginAgent::Codex, absent, "reviewer", "official"),
                Err(PluginMutationCode::OperationUnsupported)
            );
        }
    }

    #[test]
    fn matrix_claude_code_commands_fix_the_scope_to_user() {
        for command in mutation_commands()
            .iter()
            .filter(|command| command.agent == PluginAgent::ClaudeCode)
        {
            let scope = command
                .args
                .windows(2)
                .position(|pair| pair == ["--scope", "user"])
                .expect("every Claude Code mutation pins --scope user");
            let terminator = command
                .args
                .iter()
                .position(|arg| *arg == "--")
                .expect("every mutation ends the options");
            assert!(
                scope < terminator,
                "--scope user must precede the option terminator"
            );
        }
    }

    #[test]
    fn matrix_every_command_ends_with_the_option_terminator() {
        for command in mutation_commands() {
            assert_eq!(
                command.args.last().copied(),
                Some("--"),
                "{:?}/{:?} must place -- last so the selector cannot become a flag",
                command.agent,
                command.operation
            );
        }
    }

    #[test]
    fn matrix_no_command_carries_a_forbidden_argument() {
        for command in mutation_commands() {
            for forbidden in ["-y", "--config", "--keep-data", "--prune", "--all"] {
                assert!(
                    !command.args.contains(&forbidden),
                    "{:?}/{:?} must not pass {forbidden}",
                    command.agent,
                    command.operation
                );
            }
        }
    }

    #[test]
    fn matrix_marks_only_removal_destructive() {
        for operation in PluginOperation::ALL {
            assert_eq!(
                operation.is_destructive(),
                operation == PluginOperation::Remove,
                "{operation:?} destructiveness"
            );
        }
    }

    // ── Selectors built from inventory identity ──

    #[test]
    fn selector_binds_identity_after_the_option_terminator() {
        let invocation = invocation_for(
            PluginAgent::ClaudeCode,
            PluginOperation::Install,
            "reviewer",
            "official",
        )
        .expect("a supported operation with a clean identity");

        assert_eq!(invocation.program, "claude");
        assert_eq!(
            invocation.args,
            vec![
                "plugin".to_string(),
                "install".to_string(),
                "--scope".to_string(),
                "user".to_string(),
                "--".to_string(),
                "reviewer@official".to_string(),
            ]
        );
        assert_eq!(
            invocation.display(),
            "claude plugin install --scope user -- reviewer@official"
        );
    }

    #[test]
    fn selector_binds_codex_identity_with_the_json_flag() {
        let invocation = invocation_for(
            PluginAgent::Codex,
            PluginOperation::Remove,
            "reviewer",
            "official",
        )
        .expect("Codex remove is in the matrix");

        assert_eq!(
            invocation.display(),
            "codex plugin remove --json -- reviewer@official"
        );
    }

    #[test]
    fn selector_rejects_an_empty_component() {
        assert_eq!(
            invocation_for(PluginAgent::Codex, PluginOperation::Install, "", "official"),
            Err(PluginMutationCode::PreconditionFailed)
        );
        assert_eq!(
            invocation_for(PluginAgent::Codex, PluginOperation::Install, "reviewer", ""),
            Err(PluginMutationCode::PreconditionFailed)
        );
    }

    #[test]
    fn selector_rejects_nul_and_control_characters() {
        for hostile in ["revi\0ewer", "revi\newer", "revi\tewer", "reviewer\u{7f}"] {
            assert_eq!(
                invocation_for(
                    PluginAgent::Codex,
                    PluginOperation::Install,
                    hostile,
                    "official"
                ),
                Err(PluginMutationCode::PreconditionFailed),
                "{hostile:?} must not reach an argv"
            );
        }
    }

    #[test]
    fn selector_rejects_a_component_that_begins_with_a_dash() {
        assert_eq!(
            invocation_for(
                PluginAgent::ClaudeCode,
                PluginOperation::Install,
                "--config",
                "official"
            ),
            Err(PluginMutationCode::PreconditionFailed)
        );
        assert_eq!(
            invocation_for(
                PluginAgent::ClaudeCode,
                PluginOperation::Install,
                "reviewer",
                "-y"
            ),
            Err(PluginMutationCode::PreconditionFailed)
        );
    }

    #[test]
    fn selector_accepts_512_bytes_and_rejects_513() {
        let at_limit = "a".repeat(512);
        let over_limit = "a".repeat(513);

        assert!(invocation_for(
            PluginAgent::Codex,
            PluginOperation::Install,
            &at_limit,
            "official"
        )
        .is_ok());
        assert_eq!(
            invocation_for(
                PluginAgent::Codex,
                PluginOperation::Install,
                &over_limit,
                "official"
            ),
            Err(PluginMutationCode::PreconditionFailed)
        );
        assert_eq!(
            invocation_for(
                PluginAgent::Codex,
                PluginOperation::Install,
                "reviewer",
                &over_limit
            ),
            Err(PluginMutationCode::PreconditionFailed)
        );
    }

    #[test]
    fn selector_requires_user_scope_for_claude_code_non_install() {
        for operation in [
            PluginOperation::Update,
            PluginOperation::Remove,
            PluginOperation::Enable,
            PluginOperation::Disable,
        ] {
            assert_eq!(
                scope_permits(PluginAgent::ClaudeCode, operation, PluginScope::User),
                Ok(())
            );
            for refused in [
                PluginScope::Project,
                PluginScope::Local,
                PluginScope::Unknown,
            ] {
                assert_eq!(
                    scope_permits(PluginAgent::ClaudeCode, operation, refused),
                    Err(PluginMutationCode::ScopeUnsupported),
                    "{operation:?}/{refused:?} is not a scope AgentDeck mutates"
                );
            }
        }
    }

    #[test]
    fn selector_allows_claude_install_with_unknown_scope() {
        assert_eq!(
            scope_permits(
                PluginAgent::ClaudeCode,
                PluginOperation::Install,
                PluginScope::User
            ),
            Ok(())
        );
        assert_eq!(
            scope_permits(
                PluginAgent::ClaudeCode,
                PluginOperation::Install,
                PluginScope::Unknown
            ),
            Ok(()),
            "available-only records carry no scope; the backend fixes it to user"
        );
        for refused in [PluginScope::Project, PluginScope::Local] {
            assert_eq!(
                scope_permits(PluginAgent::ClaudeCode, PluginOperation::Install, refused),
                Err(PluginMutationCode::ScopeUnsupported),
                "install/{refused:?}"
            );
        }
    }

    #[test]
    fn selector_allows_codex_unknown_inventory_scope() {
        for operation in PluginOperation::ALL {
            for scope in [PluginScope::Unknown, PluginScope::User] {
                assert_eq!(scope_permits(PluginAgent::Codex, operation, scope), Ok(()));
            }
        }
    }

    #[test]
    fn selector_reports_a_fixed_user_scope_without_rewriting_inventory() {
        let invocation = invocation_for(
            PluginAgent::Codex,
            PluginOperation::Remove,
            "reviewer",
            "official",
        )
        .expect("Codex remove is in the matrix");

        assert_eq!(invocation.scope, PluginScope::User);
    }

    // ── Preview binds one intent to a transient token ──

    #[tokio::test]
    async fn preview_returns_a_reviewable_fixed_intent() {
        let host = FakeHost::with(inventory(vec![claude_available()]));
        let state = PluginMutationState::default();

        let preview = ready(
            preview_mutation(
                &host,
                &state,
                request(
                    PluginAgent::ClaudeCode,
                    PluginOperation::Install,
                    "reviewer",
                    "official",
                ),
            )
            .await,
        );

        assert_eq!(preview.agent, PluginAgent::ClaudeCode);
        assert_eq!(preview.operation, PluginOperation::Install);
        assert_eq!(preview.plugin_id, "reviewer");
        assert_eq!(preview.marketplace, "official");
        assert_eq!(preview.scope, PluginScope::User);
        assert_eq!(
            preview.argv_display,
            "claude plugin install --scope user -- reviewer@official"
        );
        assert!(!preview.destructive);
        assert!(!preview.token.is_empty());
        assert_eq!(
            preview.expires_at,
            (at(0) + ChronoDuration::seconds(120)).to_rfc3339(),
            "expiry must be exactly 120 seconds after creation"
        );
        assert_eq!(preview.base_fingerprint.len(), 64, "hex SHA-256");
        assert_eq!(host.ran(), 0, "a preview must never start a process");
    }

    #[tokio::test]
    async fn preview_marks_removal_destructive() {
        let host = FakeHost::with(inventory(vec![item(
            PluginAgent::Codex,
            "reviewer",
            "official",
            PluginPresence::Installed,
            PluginAvailability::Unknown,
            PluginScope::Unknown,
            PluginEnablement::Enabled,
        )]));
        let state = PluginMutationState::default();

        let preview = ready(
            preview_mutation(
                &host,
                &state,
                request(
                    PluginAgent::Codex,
                    PluginOperation::Remove,
                    "reviewer",
                    "official",
                ),
            )
            .await,
        );

        assert!(preview.destructive);
        assert_eq!(
            preview.argv_display,
            "codex plugin remove --json -- reviewer@official"
        );
    }

    #[tokio::test]
    async fn preview_rejects_an_identity_absent_from_fresh_inventory() {
        let host = FakeHost::with(inventory(vec![claude_available()]));
        let state = PluginMutationState::default();

        let code = failed(
            preview_mutation(
                &host,
                &state,
                request(
                    PluginAgent::ClaudeCode,
                    PluginOperation::Install,
                    "reviewer",
                    "team",
                ),
            )
            .await,
        );

        assert_eq!(code, PluginMutationCode::IdentityNotFound);
        assert_eq!(state.pending_tokens().await.len(), 0);
    }

    #[tokio::test]
    async fn preview_rejects_an_unsupported_operation_without_collecting() {
        let host = FakeHost::with(inventory(vec![item(
            PluginAgent::Codex,
            "reviewer",
            "official",
            PluginPresence::Installed,
            PluginAvailability::Unknown,
            PluginScope::Unknown,
            PluginEnablement::Enabled,
        )]));
        let state = PluginMutationState::default();

        let code = failed(
            preview_mutation(
                &host,
                &state,
                request(
                    PluginAgent::Codex,
                    PluginOperation::Update,
                    "reviewer",
                    "official",
                ),
            )
            .await,
        );

        assert_eq!(code, PluginMutationCode::OperationUnsupported);
        assert_eq!(
            host.collected.load(Ordering::SeqCst),
            0,
            "an unsupported operation must fail before anything is collected"
        );
    }

    #[tokio::test]
    async fn preview_rejects_an_option_like_identity_without_collecting() {
        let host = FakeHost::with(inventory(vec![claude_available()]));
        let state = PluginMutationState::default();

        let code = failed(
            preview_mutation(
                &host,
                &state,
                request(
                    PluginAgent::ClaudeCode,
                    PluginOperation::Install,
                    "--config",
                    "official",
                ),
            )
            .await,
        );

        assert_eq!(code, PluginMutationCode::PreconditionFailed);
        assert_eq!(host.collected.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn preview_rejects_claude_code_records_outside_user_scope() {
        for scope in [
            PluginScope::Project,
            PluginScope::Local,
            PluginScope::Unknown,
        ] {
            let host = FakeHost::with(inventory(vec![item(
                PluginAgent::ClaudeCode,
                "reviewer",
                "official",
                PluginPresence::Installed,
                PluginAvailability::Unknown,
                scope,
                PluginEnablement::Enabled,
            )]));
            let state = PluginMutationState::default();

            let code = failed(
                preview_mutation(
                    &host,
                    &state,
                    request(
                        PluginAgent::ClaudeCode,
                        PluginOperation::Remove,
                        "reviewer",
                        "official",
                    ),
                )
                .await,
            );

            assert_eq!(code, PluginMutationCode::ScopeUnsupported, "{scope:?}");
        }
    }

    #[tokio::test]
    async fn preview_keeps_codex_unknown_inventory_scope_lossless() {
        let record = item(
            PluginAgent::Codex,
            "reviewer",
            "official",
            PluginPresence::Installed,
            PluginAvailability::Unknown,
            PluginScope::Unknown,
            PluginEnablement::Enabled,
        );
        let fixture = inventory(vec![record]);
        let host = FakeHost::with(fixture.clone());
        let state = PluginMutationState::default();

        let preview = ready(
            preview_mutation(
                &host,
                &state,
                request(
                    PluginAgent::Codex,
                    PluginOperation::Remove,
                    "reviewer",
                    "official",
                ),
            )
            .await,
        );

        assert_eq!(preview.scope, PluginScope::User);
        let untouched = fixture.agents[0].items[0].scope;
        assert_eq!(
            untouched,
            PluginScope::Unknown,
            "the inventory record keeps saying unknown"
        );
    }

    #[tokio::test]
    async fn preview_accepts_claude_available_only_unknown_scope_for_install() {
        let record = parsed_claude_available_only();
        assert_eq!(record.installed, PluginPresence::Unknown);
        assert_eq!(record.scope, PluginScope::Unknown);
        let host = FakeHost::with(inventory(vec![record]));
        let state = PluginMutationState::default();

        let preview = ready(
            preview_mutation(
                &host,
                &state,
                request(
                    PluginAgent::ClaudeCode,
                    PluginOperation::Install,
                    "reviewer",
                    "official",
                ),
            )
            .await,
        );

        assert_eq!(preview.scope, PluginScope::User);
        assert_eq!(
            preview.argv_display,
            "claude plugin install --scope user -- reviewer@official"
        );
    }

    #[tokio::test]
    async fn preview_rejects_claude_available_only_for_non_install() {
        let record = parsed_claude_available_only();

        for operation in [
            PluginOperation::Update,
            PluginOperation::Remove,
            PluginOperation::Enable,
            PluginOperation::Disable,
        ] {
            let host = FakeHost::with(inventory(vec![record.clone()]));
            let state = PluginMutationState::default();

            let code = failed(
                preview_mutation(
                    &host,
                    &state,
                    request(PluginAgent::ClaudeCode, operation, "reviewer", "official"),
                )
                .await,
            );

            assert_eq!(
                code,
                PluginMutationCode::ScopeUnsupported,
                "{operation:?} on an available-only unknown-scope record must be refused"
            );
        }
    }

    #[tokio::test]
    async fn preview_enforces_every_operation_precondition() {
        let installed_enabled = || {
            item(
                PluginAgent::ClaudeCode,
                "reviewer",
                "official",
                PluginPresence::Installed,
                PluginAvailability::Unknown,
                PluginScope::User,
                PluginEnablement::Enabled,
            )
        };
        let not_installed = || {
            item(
                PluginAgent::ClaudeCode,
                "reviewer",
                "official",
                PluginPresence::NotInstalled,
                PluginAvailability::Available,
                PluginScope::User,
                PluginEnablement::Unknown,
            )
        };

        let cases: Vec<(
            PluginOperation,
            PluginInventoryItemDto,
            Option<PluginMutationCode>,
        )> = vec![
            // Install needs an offer that is not already installed.
            (PluginOperation::Install, not_installed(), None),
            (
                PluginOperation::Install,
                installed_enabled(),
                Some(PluginMutationCode::PreconditionFailed),
            ),
            // Update, remove, enable and disable need something installed.
            (
                PluginOperation::Update,
                not_installed(),
                Some(PluginMutationCode::PreconditionFailed),
            ),
            (PluginOperation::Update, installed_enabled(), None),
            (
                PluginOperation::Remove,
                not_installed(),
                Some(PluginMutationCode::PreconditionFailed),
            ),
            (PluginOperation::Remove, installed_enabled(), None),
            // Enable and disable additionally need the opposite known state.
            (
                PluginOperation::Enable,
                installed_enabled(),
                Some(PluginMutationCode::PreconditionFailed),
            ),
            (PluginOperation::Disable, installed_enabled(), None),
            (
                PluginOperation::Enable,
                item(
                    PluginAgent::ClaudeCode,
                    "reviewer",
                    "official",
                    PluginPresence::Installed,
                    PluginAvailability::Unknown,
                    PluginScope::User,
                    PluginEnablement::Unknown,
                ),
                Some(PluginMutationCode::PreconditionFailed),
            ),
            (
                PluginOperation::Enable,
                item(
                    PluginAgent::ClaudeCode,
                    "reviewer",
                    "official",
                    PluginPresence::Installed,
                    PluginAvailability::Unknown,
                    PluginScope::User,
                    PluginEnablement::Disabled,
                ),
                None,
            ),
        ];

        for (operation, record, expected) in cases {
            let host = FakeHost::with(inventory(vec![record.clone()]));
            let state = PluginMutationState::default();
            let outcome = preview_mutation(
                &host,
                &state,
                request(PluginAgent::ClaudeCode, operation, "reviewer", "official"),
            )
            .await;

            match expected {
                Some(code) => assert_eq!(
                    failed(outcome),
                    code,
                    "{operation:?} on installed={:?} enabled={:?}",
                    record.installed,
                    record.enabled
                ),
                None => {
                    ready(outcome);
                }
            }
        }
    }

    #[tokio::test]
    async fn preview_fingerprint_covers_item_version_and_capabilities() {
        let baseline = inventory(vec![claude_available()]);

        let mut other_installed_version = baseline.clone();
        other_installed_version.agents[1].items[0].installed_version = Some("1.0".to_string());

        let mut other_cli_version = baseline.clone();
        other_cli_version.agents[1].cli_version = Some("2.2.0 (Claude Code)".to_string());

        let mut fewer_capabilities = baseline.clone();
        fewer_capabilities.agents[1].capabilities = vec![PluginCapability::PluginList];

        let mut fingerprints = Vec::new();
        for fixture in [
            baseline.clone(),
            baseline,
            other_installed_version,
            other_cli_version,
            fewer_capabilities,
        ] {
            let host = FakeHost::with(fixture);
            let state = PluginMutationState::default();
            let preview = ready(
                preview_mutation(
                    &host,
                    &state,
                    request(
                        PluginAgent::ClaudeCode,
                        PluginOperation::Install,
                        "reviewer",
                        "official",
                    ),
                )
                .await,
            );
            fingerprints.push(preview.base_fingerprint);
        }

        assert_eq!(
            fingerprints[0], fingerprints[1],
            "the same fresh state must fingerprint the same way"
        );
        for changed in &fingerprints[2..] {
            assert_ne!(
                &fingerprints[0], changed,
                "a changed item, CLI version or capability must change the fingerprint"
            );
        }
    }

    #[tokio::test]
    async fn preview_fingerprint_separates_operations() {
        let host = FakeHost::with(inventory(vec![item(
            PluginAgent::ClaudeCode,
            "reviewer",
            "official",
            PluginPresence::Installed,
            PluginAvailability::Unknown,
            PluginScope::User,
            PluginEnablement::Enabled,
        )]));
        let state = PluginMutationState::default();

        let remove = ready(
            preview_mutation(
                &host,
                &state,
                request(
                    PluginAgent::ClaudeCode,
                    PluginOperation::Remove,
                    "reviewer",
                    "official",
                ),
            )
            .await,
        );
        let disable = ready(
            preview_mutation(
                &host,
                &state,
                request(
                    PluginAgent::ClaudeCode,
                    PluginOperation::Disable,
                    "reviewer",
                    "official",
                ),
            )
            .await,
        );

        assert_ne!(remove.base_fingerprint, disable.base_fingerprint);
        assert_ne!(remove.token, disable.token);
    }

    #[tokio::test]
    async fn preview_evicts_the_earliest_expiry_at_capacity() {
        let host = FakeHost::new(vec![inventory(vec![claude_available()])], at(0));
        let state = PluginMutationState::default();

        let mut tokens = Vec::new();
        for _ in 0..MAX_PENDING_PREVIEWS {
            let preview = ready(
                preview_mutation(
                    &host,
                    &state,
                    request(
                        PluginAgent::ClaudeCode,
                        PluginOperation::Install,
                        "reviewer",
                        "official",
                    ),
                )
                .await,
            );
            tokens.push(preview.token);
            host.advance(1);
        }
        assert_eq!(state.pending_tokens().await.len(), MAX_PENDING_PREVIEWS);

        let overflow = ready(
            preview_mutation(
                &host,
                &state,
                request(
                    PluginAgent::ClaudeCode,
                    PluginOperation::Install,
                    "reviewer",
                    "official",
                ),
            )
            .await,
        );

        let pending = state.pending_tokens().await;
        assert_eq!(pending.len(), MAX_PENDING_PREVIEWS);
        assert!(
            !pending.contains(&tokens[0]),
            "the earliest expiry is the one that leaves"
        );
        assert!(pending.contains(&tokens[1]));
        assert!(pending.contains(&overflow.token));
    }

    #[tokio::test]
    async fn preview_drops_expired_entries_before_evicting() {
        let host = FakeHost::new(vec![inventory(vec![claude_available()])], at(0));
        let state = PluginMutationState::default();

        for _ in 0..MAX_PENDING_PREVIEWS {
            ready(
                preview_mutation(
                    &host,
                    &state,
                    request(
                        PluginAgent::ClaudeCode,
                        PluginOperation::Install,
                        "reviewer",
                        "official",
                    ),
                )
                .await,
            );
            host.advance(1);
        }
        host.advance(PREVIEW_TTL_SECONDS * 1_000);

        let survivor = ready(
            preview_mutation(
                &host,
                &state,
                request(
                    PluginAgent::ClaudeCode,
                    PluginOperation::Install,
                    "reviewer",
                    "official",
                ),
            )
            .await,
        );

        assert_eq!(state.pending_tokens().await, vec![survivor.token]);
    }

    #[tokio::test]
    async fn preview_serializes_only_reviewable_fields() {
        let host = FakeHost::with(inventory(vec![claude_available()]));
        let state = PluginMutationState::default();

        let outcome = preview_mutation(
            &host,
            &state,
            request(
                PluginAgent::ClaudeCode,
                PluginOperation::Install,
                "reviewer",
                "official",
            ),
        )
        .await;
        let json = serde_json::to_value(&outcome).expect("serialize");

        assert_eq!(json["status"], "ready");
        let mut keys: Vec<&str> = json["preview"]
            .as_object()
            .expect("a preview object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "agent",
                "argvDisplay",
                "baseFingerprint",
                "destructive",
                "expiresAt",
                "marketplace",
                "operation",
                "pluginId",
                "scope",
                "token",
            ]
        );

        let text = serde_json::to_string(&outcome).expect("serialize");
        assert!(!text.contains(SENTINEL));
        for leaked in ["/Users", "home", "path", "stderr", "stdout"] {
            assert!(
                !text.contains(leaked),
                "{leaked} must not reach the frontend"
            );
        }
    }

    #[tokio::test]
    async fn preview_failure_serializes_only_fixed_diagnostic_fields() {
        let host = FakeHost::with(inventory(vec![claude_available()]));
        let state = PluginMutationState::default();

        let outcome = preview_mutation(
            &host,
            &state,
            request(
                PluginAgent::ClaudeCode,
                PluginOperation::Remove,
                "reviewer",
                "official",
            ),
        )
        .await;
        let json = serde_json::to_value(&outcome).expect("serialize");

        assert_eq!(json["status"], "failed");
        let mut keys: Vec<&str> = json["diagnostic"]
            .as_object()
            .expect("a diagnostic object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["agent", "code", "exitStatus", "operation"]);
        assert_eq!(json["diagnostic"]["code"], "precondition_failed");
        assert_eq!(json["diagnostic"]["exitStatus"], serde_json::Value::Null);
    }

    // ── Apply consumes the token and rejects stale state ──

    /// A Claude Code Plugin installed at `1.0` with `1.1` on offer — the state
    /// the spec's stale-update scenario starts from.
    fn claude_installed(version: &str) -> PluginInventoryItemDto {
        let mut record = item(
            PluginAgent::ClaudeCode,
            "reviewer",
            "official",
            PluginPresence::Installed,
            PluginAvailability::Available,
            PluginScope::User,
            PluginEnablement::Enabled,
        );
        record.installed_version = Some(version.to_string());
        record.available_version = Some("1.1".to_string());
        record
    }

    fn apply_request(token: &str) -> PluginMutationApplyRequest {
        PluginMutationApplyRequest {
            token: token.to_string(),
        }
    }

    fn apply_failed(outcome: PluginMutationApplyOutcomeDto) -> PluginMutationDiagnosticDto {
        match outcome {
            PluginMutationApplyOutcomeDto::Verified { operation, .. } => {
                panic!("expected a failure, got a verified {operation:?}")
            }
            PluginMutationApplyOutcomeDto::Failed { diagnostic, .. } => diagnostic,
        }
    }

    /// Previews one intent against `host` and returns its token.
    async fn token_for(
        host: &FakeHost,
        state: &PluginMutationState,
        agent: PluginAgent,
        operation: PluginOperation,
    ) -> String {
        ready(
            preview_mutation(
                host,
                state,
                request(agent, operation, "reviewer", "official"),
            )
            .await,
        )
        .token
    }

    #[tokio::test]
    async fn apply_rejects_a_token_that_was_never_issued() {
        let host = FakeHost::with(inventory(vec![claude_installed("1.0")]));
        let state = PluginMutationState::default();

        let diagnostic = apply_failed(
            apply_mutation(&host, &state, apply_request("token-does-not-exist")).await,
        );

        assert_eq!(diagnostic.code, PluginMutationCode::PreviewExpired);
        assert_eq!(diagnostic.agent, None);
        assert_eq!(diagnostic.operation, None);
        assert_eq!(host.ran(), 0);
    }

    #[tokio::test]
    async fn apply_rejects_an_expired_token() {
        let host = FakeHost::with(inventory(vec![claude_installed("1.0")]));
        let state = PluginMutationState::default();
        let token = token_for(
            &host,
            &state,
            PluginAgent::ClaudeCode,
            PluginOperation::Update,
        )
        .await;

        host.advance(PREVIEW_TTL_SECONDS * 1_000 + 1);
        let diagnostic = apply_failed(apply_mutation(&host, &state, apply_request(&token)).await);

        assert_eq!(diagnostic.code, PluginMutationCode::PreviewExpired);
        assert_eq!(host.ran(), 0, "an expired intent must not reach the CLI");
    }

    #[tokio::test]
    async fn apply_rejects_a_replayed_token() {
        let host = FakeHost::with(inventory(vec![claude_installed("1.0")]));
        let state = PluginMutationState::default();
        let token = token_for(
            &host,
            &state,
            PluginAgent::ClaudeCode,
            PluginOperation::Remove,
        )
        .await;

        let _first = apply_mutation(&host, &state, apply_request(&token)).await;
        let replayed = apply_failed(apply_mutation(&host, &state, apply_request(&token)).await);

        assert_eq!(replayed.code, PluginMutationCode::PreviewExpired);
        assert_eq!(host.ran(), 1, "a replay must not repeat the mutation");
        assert!(state.pending_tokens().await.is_empty());
    }

    #[tokio::test]
    async fn apply_rejects_every_kind_of_drift() {
        // Each case previews against the baseline, then hands apply an
        // inventory that differs in exactly one way.
        let mutate: Vec<(&str, Box<dyn Fn(&mut PluginInventoryDto)>)> = vec![
            (
                "installed version",
                Box::new(|fresh: &mut PluginInventoryDto| {
                    fresh.agents[1].items[0].installed_version = Some("1.1".to_string());
                }),
            ),
            (
                "available version",
                Box::new(|fresh: &mut PluginInventoryDto| {
                    fresh.agents[1].items[0].available_version = Some("2.0".to_string());
                }),
            ),
            (
                "CLI version",
                Box::new(|fresh: &mut PluginInventoryDto| {
                    fresh.agents[1].cli_version = Some("2.2.0 (Claude Code)".to_string());
                }),
            ),
            (
                "capabilities",
                Box::new(|fresh: &mut PluginInventoryDto| {
                    fresh.agents[1].capabilities = vec![PluginCapability::PluginList];
                }),
            ),
            (
                "presence",
                Box::new(|fresh: &mut PluginInventoryDto| {
                    fresh.agents[1].items[0].installed = PluginPresence::NotInstalled;
                }),
            ),
            (
                "enablement",
                Box::new(|fresh: &mut PluginInventoryDto| {
                    fresh.agents[1].items[0].enabled = PluginEnablement::Disabled;
                }),
            ),
            (
                "scope",
                Box::new(|fresh: &mut PluginInventoryDto| {
                    fresh.agents[1].items[0].scope = PluginScope::Project;
                }),
            ),
            (
                "identity",
                Box::new(|fresh: &mut PluginInventoryDto| {
                    fresh.agents[1].items.clear();
                }),
            ),
        ];

        for (label, change) in mutate {
            let baseline = inventory(vec![claude_installed("1.0")]);
            let mut drifted = baseline.clone();
            change(&mut drifted);

            let host = FakeHost::new(vec![baseline, drifted], at(0));
            let state = PluginMutationState::default();
            let token = token_for(
                &host,
                &state,
                PluginAgent::ClaudeCode,
                PluginOperation::Update,
            )
            .await;

            let diagnostic =
                apply_failed(apply_mutation(&host, &state, apply_request(&token)).await);

            assert_eq!(
                diagnostic.code,
                PluginMutationCode::StalePreview,
                "drifted {label}"
            );
            assert_eq!(host.ran(), 0, "drifted {label} must not start the mutation");
        }
    }

    #[tokio::test]
    async fn apply_passes_only_the_fixed_invocation_to_the_runner() {
        let host = FakeHost::with(inventory(vec![claude_installed("1.0")]));
        let state = PluginMutationState::default();
        let token = token_for(
            &host,
            &state,
            PluginAgent::ClaudeCode,
            PluginOperation::Remove,
        )
        .await;

        let _ = apply_mutation(&host, &state, apply_request(&token)).await;

        assert_eq!(
            *host.invocations.lock().expect("invocations"),
            vec!["claude plugin uninstall --scope user -- reviewer@official".to_string()]
        );
    }

    #[tokio::test]
    async fn apply_reports_a_runner_failure_without_any_output() {
        let host = FakeHost::with(inventory(vec![claude_installed("1.0")]));
        host.fails_with(PluginMutationCode::NonZeroExit, Some(3));
        let state = PluginMutationState::default();
        let token = token_for(
            &host,
            &state,
            PluginAgent::ClaudeCode,
            PluginOperation::Remove,
        )
        .await;

        let outcome = apply_mutation(&host, &state, apply_request(&token)).await;
        let json = serde_json::to_string(&outcome).expect("serialize");
        let diagnostic = apply_failed(outcome);

        assert_eq!(diagnostic.code, PluginMutationCode::NonZeroExit);
        assert_eq!(diagnostic.exit_status, Some(3));
        assert_eq!(diagnostic.agent, Some(PluginAgent::ClaudeCode));
        assert!(!json.contains(SENTINEL));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn apply_runs_one_mutation_at_a_time_across_agents() {
        let codex = item(
            PluginAgent::Codex,
            "reviewer",
            "official",
            PluginPresence::Installed,
            PluginAvailability::Unknown,
            PluginScope::Unknown,
            PluginEnablement::Enabled,
        );
        let mut host = FakeHost::with(inventory(vec![codex, claude_installed("1.0")]));
        host.run_delay_ms = 40;
        let state = PluginMutationState::default();

        let codex_token =
            token_for(&host, &state, PluginAgent::Codex, PluginOperation::Remove).await;
        let claude_token = token_for(
            &host,
            &state,
            PluginAgent::ClaudeCode,
            PluginOperation::Remove,
        )
        .await;

        let (_left, _right) = tokio::join!(
            apply_mutation(&host, &state, apply_request(&codex_token)),
            apply_mutation(&host, &state, apply_request(&claude_token)),
        );

        assert_eq!(host.ran(), 2);
        assert_eq!(
            host.max_running.load(Ordering::SeqCst),
            1,
            "both Agents' Plugin mutations must be serialized"
        );
    }

    // ── Post-operation inventory proof ──

    /// Applies one intent against `before`, lets the fake CLI succeed, and hands
    /// verification the `after` inventory.
    async fn apply_between(
        agent: PluginAgent,
        operation: PluginOperation,
        before: PluginInventoryDto,
        after: PluginInventoryDto,
    ) -> PluginMutationApplyOutcomeDto {
        let host = FakeHost::new(vec![before.clone(), before, after], at(0));
        let state = PluginMutationState::default();
        let token = token_for(&host, &state, agent, operation).await;
        apply_mutation(&host, &state, apply_request(&token)).await
    }

    fn claude_with(
        installed: PluginPresence,
        available: PluginAvailability,
        enabled: PluginEnablement,
        installed_version: Option<&str>,
        available_version: Option<&str>,
    ) -> PluginInventoryDto {
        let mut record = item(
            PluginAgent::ClaudeCode,
            "reviewer",
            "official",
            installed,
            available,
            PluginScope::User,
            enabled,
        );
        record.installed_version = installed_version.map(str::to_string);
        record.available_version = available_version.map(str::to_string);
        inventory(vec![record])
    }

    #[tokio::test]
    async fn verify_accepts_the_documented_postconditions() {
        let cases: Vec<(PluginOperation, PluginInventoryDto, PluginInventoryDto)> = vec![
            (
                PluginOperation::Install,
                claude_with(
                    PluginPresence::NotInstalled,
                    PluginAvailability::Available,
                    PluginEnablement::Unknown,
                    None,
                    Some("1.1"),
                ),
                claude_with(
                    PluginPresence::Installed,
                    PluginAvailability::Available,
                    PluginEnablement::Enabled,
                    Some("1.1"),
                    Some("1.1"),
                ),
            ),
            (
                PluginOperation::Remove,
                claude_with(
                    PluginPresence::Installed,
                    PluginAvailability::Unknown,
                    PluginEnablement::Enabled,
                    Some("1.0"),
                    None,
                ),
                claude_with(
                    PluginPresence::NotInstalled,
                    PluginAvailability::Unknown,
                    PluginEnablement::Unknown,
                    None,
                    None,
                ),
            ),
            (
                PluginOperation::Enable,
                claude_with(
                    PluginPresence::Installed,
                    PluginAvailability::Unknown,
                    PluginEnablement::Disabled,
                    Some("1.0"),
                    None,
                ),
                claude_with(
                    PluginPresence::Installed,
                    PluginAvailability::Unknown,
                    PluginEnablement::Enabled,
                    Some("1.0"),
                    None,
                ),
            ),
            (
                PluginOperation::Disable,
                claude_with(
                    PluginPresence::Installed,
                    PluginAvailability::Unknown,
                    PluginEnablement::Enabled,
                    Some("1.0"),
                    None,
                ),
                claude_with(
                    PluginPresence::Installed,
                    PluginAvailability::Unknown,
                    PluginEnablement::Disabled,
                    Some("1.0"),
                    None,
                ),
            ),
            (
                PluginOperation::Update,
                claude_with(
                    PluginPresence::Installed,
                    PluginAvailability::Available,
                    PluginEnablement::Enabled,
                    Some("1.0"),
                    Some("1.1"),
                ),
                claude_with(
                    PluginPresence::Installed,
                    PluginAvailability::Available,
                    PluginEnablement::Enabled,
                    Some("1.1"),
                    Some("1.1"),
                ),
            ),
        ];

        for (operation, before, after) in cases {
            let outcome =
                apply_between(PluginAgent::ClaudeCode, operation, before, after.clone()).await;
            match outcome {
                PluginMutationApplyOutcomeDto::Verified {
                    operation: verified,
                    inventory,
                    ..
                } => {
                    assert_eq!(verified, operation);
                    assert_eq!(
                        inventory, after,
                        "the page gets the inventory that proved it"
                    );
                }
                PluginMutationApplyOutcomeDto::Failed { diagnostic, .. } => {
                    panic!(
                        "{operation:?} should have verified, got {:?}",
                        diagnostic.code
                    )
                }
            }
        }
    }

    #[tokio::test]
    async fn verify_rejects_exit_zero_without_an_observable_change() {
        let before = claude_with(
            PluginPresence::Installed,
            PluginAvailability::Available,
            PluginEnablement::Enabled,
            Some("1.0"),
            Some("1.1"),
        );
        let outcome = apply_between(
            PluginAgent::ClaudeCode,
            PluginOperation::Update,
            before.clone(),
            before.clone(),
        )
        .await;

        match outcome {
            PluginMutationApplyOutcomeDto::Failed {
                diagnostic,
                inventory,
            } => {
                assert_eq!(diagnostic.code, PluginMutationCode::VerificationFailed);
                assert_eq!(
                    inventory,
                    Some(before),
                    "a verification failure still shows the latest state"
                );
            }
            PluginMutationApplyOutcomeDto::Verified { .. } => {
                panic!("an unchanged version is not an update")
            }
        }
    }

    #[tokio::test]
    async fn verify_rejects_every_unmet_or_unknown_postcondition() {
        let installed_enabled = || {
            claude_with(
                PluginPresence::Installed,
                PluginAvailability::Available,
                PluginEnablement::Enabled,
                Some("1.0"),
                Some("1.1"),
            )
        };
        let offered = || {
            claude_with(
                PluginPresence::NotInstalled,
                PluginAvailability::Available,
                PluginEnablement::Unknown,
                None,
                Some("1.1"),
            )
        };
        let disabled = || {
            claude_with(
                PluginPresence::Installed,
                PluginAvailability::Unknown,
                PluginEnablement::Disabled,
                Some("1.0"),
                None,
            )
        };
        let unknown_presence = || {
            claude_with(
                PluginPresence::Unknown,
                PluginAvailability::Available,
                PluginEnablement::Unknown,
                None,
                Some("1.1"),
            )
        };
        let unknown_enablement = || {
            claude_with(
                PluginPresence::Installed,
                PluginAvailability::Unknown,
                PluginEnablement::Unknown,
                Some("1.0"),
                None,
            )
        };

        let cases: Vec<(
            &str,
            PluginOperation,
            PluginInventoryDto,
            PluginInventoryDto,
        )> = vec![
            (
                "install stayed absent",
                PluginOperation::Install,
                offered(),
                offered(),
            ),
            (
                "install ended unknown",
                PluginOperation::Install,
                offered(),
                unknown_presence(),
            ),
            (
                "remove left it installed",
                PluginOperation::Remove,
                installed_enabled(),
                installed_enabled(),
            ),
            (
                "remove ended unknown",
                PluginOperation::Remove,
                installed_enabled(),
                unknown_presence(),
            ),
            (
                "enable ended unknown",
                PluginOperation::Enable,
                disabled(),
                unknown_enablement(),
            ),
            (
                "enable stayed disabled",
                PluginOperation::Enable,
                disabled(),
                disabled(),
            ),
            (
                "disable stayed enabled",
                PluginOperation::Disable,
                installed_enabled(),
                installed_enabled(),
            ),
            (
                "update lost its version",
                PluginOperation::Update,
                installed_enabled(),
                claude_with(
                    PluginPresence::Installed,
                    PluginAvailability::Available,
                    PluginEnablement::Enabled,
                    None,
                    Some("1.1"),
                ),
            ),
            (
                "update moved to the wrong version",
                PluginOperation::Update,
                installed_enabled(),
                claude_with(
                    PluginPresence::Installed,
                    PluginAvailability::Available,
                    PluginEnablement::Enabled,
                    Some("1.2"),
                    Some("1.1"),
                ),
            ),
        ];

        for (label, operation, before, after) in cases {
            let outcome = apply_between(PluginAgent::ClaudeCode, operation, before, after).await;
            let diagnostic = apply_failed(outcome);
            assert_eq!(
                diagnostic.code,
                PluginMutationCode::VerificationFailed,
                "{label}"
            );
        }
    }

    #[tokio::test]
    async fn verify_accepts_an_update_without_a_known_available_version() {
        let before = claude_with(
            PluginPresence::Installed,
            PluginAvailability::Unknown,
            PluginEnablement::Enabled,
            Some("1.0"),
            None,
        );
        let after = claude_with(
            PluginPresence::Installed,
            PluginAvailability::Unknown,
            PluginEnablement::Enabled,
            Some("1.4"),
            None,
        );

        let outcome = apply_between(
            PluginAgent::ClaudeCode,
            PluginOperation::Update,
            before,
            after,
        )
        .await;

        assert!(matches!(
            outcome,
            PluginMutationApplyOutcomeDto::Verified { .. }
        ));
    }

    #[tokio::test]
    async fn verify_accepts_a_removal_that_left_no_record() {
        let before = claude_with(
            PluginPresence::Installed,
            PluginAvailability::Unknown,
            PluginEnablement::Enabled,
            Some("1.0"),
            None,
        );
        let after = inventory(Vec::new());

        let outcome = apply_between(
            PluginAgent::ClaudeCode,
            PluginOperation::Remove,
            before,
            after,
        )
        .await;

        assert!(matches!(
            outcome,
            PluginMutationApplyOutcomeDto::Verified { .. }
        ));
    }

    #[tokio::test]
    async fn verify_rejects_a_refresh_that_could_not_read_plugins() {
        let before = claude_with(
            PluginPresence::Installed,
            PluginAvailability::Unknown,
            PluginEnablement::Enabled,
            Some("1.0"),
            None,
        );
        // The Plugin listing failed, so this response is not evidence about the
        // Plugin either way.
        let mut after = inventory(Vec::new());
        after.agents[1].capabilities = vec![PluginCapability::Version];

        let outcome = apply_between(
            PluginAgent::ClaudeCode,
            PluginOperation::Remove,
            before,
            after.clone(),
        )
        .await;

        match outcome {
            PluginMutationApplyOutcomeDto::Failed {
                diagnostic,
                inventory,
            } => {
                assert_eq!(diagnostic.code, PluginMutationCode::VerificationFailed);
                assert_eq!(inventory, Some(after));
            }
            PluginMutationApplyOutcomeDto::Verified { .. } => {
                panic!("an unreadable listing must not prove a removal")
            }
        }
    }

    #[tokio::test]
    async fn verify_serializes_success_as_identity_plus_fresh_inventory() {
        let before = claude_with(
            PluginPresence::Installed,
            PluginAvailability::Unknown,
            PluginEnablement::Enabled,
            Some("1.0"),
            None,
        );
        let after = claude_with(
            PluginPresence::Installed,
            PluginAvailability::Unknown,
            PluginEnablement::Disabled,
            Some("1.0"),
            None,
        );

        let outcome = apply_between(
            PluginAgent::ClaudeCode,
            PluginOperation::Disable,
            before,
            after,
        )
        .await;
        let json = serde_json::to_value(&outcome).expect("serialize");

        assert_eq!(json["status"], "verified");
        let mut keys: Vec<&str> = json
            .as_object()
            .expect("an object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            vec![
                "agent",
                "inventory",
                "marketplace",
                "operation",
                "pluginId",
                "status"
            ]
        );
        assert_eq!(json["agent"], "claude_code");
        assert_eq!(json["operation"], "disable");
        assert_eq!(json["pluginId"], "reviewer");
        assert_eq!(json["marketplace"], "official");
        assert!(json["inventory"]["agents"].is_array());
        assert!(!serde_json::to_string(&outcome)
            .expect("serialize")
            .contains(SENTINEL));
    }

    // ── The bounded mutation runner ──

    /// A deadline long enough that a healthy fake command always finishes, and
    /// short enough that a hung one does not stall the suite.
    const TEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

    fn fake(program: &str, args: &[&str]) -> tokio::process::Command {
        let mut command = tokio::process::Command::new(program);
        command.args(args);
        command
    }

    /// Writes exactly `total` bytes and returns the file it wrote them to.
    fn bytes_of_exact_size(path: &std::path::Path, total: usize) -> &std::path::Path {
        std::fs::write(path, "a".repeat(total)).expect("write payload");
        path
    }

    #[cfg(unix)]
    fn is_alive_or_zombie(pid: u32) -> bool {
        let out = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .expect("ps");
        !String::from_utf8_lossy(&out.stdout).trim().is_empty()
    }

    #[test]
    fn runner_bounds_match_the_read_only_runner() {
        assert_eq!(MUTATION_TIMEOUT, std::time::Duration::from_secs(10));
        assert_eq!(
            crate::core::plugin_inventory::MAX_STREAM_BYTES,
            1_048_576,
            "both runners share one per-stream limit"
        );
    }

    #[test]
    fn runner_classifies_confirmation_only_for_claude_install_and_update() {
        let expected = [
            (PluginAgent::ClaudeCode, PluginOperation::Install, true),
            (PluginAgent::ClaudeCode, PluginOperation::Update, true),
            (PluginAgent::ClaudeCode, PluginOperation::Remove, false),
            (PluginAgent::ClaudeCode, PluginOperation::Enable, false),
            (PluginAgent::ClaudeCode, PluginOperation::Disable, false),
            (PluginAgent::Codex, PluginOperation::Install, false),
            (PluginAgent::Codex, PluginOperation::Remove, false),
        ];
        for (agent, operation, classifies) in expected {
            assert_eq!(
                may_require_confirmation(agent, operation),
                classifies,
                "{agent:?}/{operation:?}"
            );
        }
    }

    #[test]
    fn runner_pins_the_official_non_tty_phrases() {
        assert_eq!(
            NON_TTY_CONFIRMATION_PHRASES,
            ["marketplace-declared command", "confirmation prompt"]
        );
    }

    #[tokio::test]
    async fn runner_missing_executable_reports_cli_missing() {
        let failure = run_bounded_mutation(
            fake("agentdeck-plugin-cli-that-does-not-exist", &[]),
            TEST_TIMEOUT,
            false,
        )
        .await
        .expect_err("a missing executable must fail");

        assert_eq!(failure.code, PluginMutationCode::CliMissing);
        assert_eq!(failure.exit_status, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_closes_stdin() {
        // `cat` returns immediately on an already-closed stdin, and blocks past
        // the deadline on an inherited one.
        run_bounded_mutation(
            fake("/bin/sh", &["-c", "cat; exit 0"]),
            std::time::Duration::from_millis(500),
            false,
        )
        .await
        .expect("a closed stdin lets the child finish");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_never_uses_a_shell() {
        let payload = "$(printf pwned); echo pwned | tee /dev/null";
        let marker = std::env::temp_dir().join("agentdeck-plugin-mutation-shell-marker");
        let _ = std::fs::remove_file(&marker);

        run_bounded_mutation(fake("/bin/echo", &[payload]), TEST_TIMEOUT, false)
            .await
            .expect("echo succeeds");

        assert!(
            !marker.exists(),
            "an argument must never be interpreted as a command"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_accepts_the_exact_stream_limits() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("payload");
        let limit = crate::core::plugin_inventory::MAX_STREAM_BYTES;
        bytes_of_exact_size(&file, limit);
        let path = file.to_str().unwrap();

        run_bounded_mutation(fake("/bin/cat", &[path]), TEST_TIMEOUT, false)
            .await
            .expect("exactly the stdout limit is accepted");

        run_bounded_mutation(
            fake("/bin/sh", &["-c", "cat \"$0\" >&2", path]),
            TEST_TIMEOUT,
            false,
        )
        .await
        .expect("exactly the stderr limit is accepted");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_rejects_one_byte_over_either_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("payload");
        let over = crate::core::plugin_inventory::MAX_STREAM_BYTES + 1;
        bytes_of_exact_size(&file, over);
        let path = file.to_str().unwrap();

        for command in [
            fake("/bin/cat", &[path]),
            fake("/bin/sh", &["-c", "cat \"$0\" >&2", path]),
        ] {
            let failure = run_bounded_mutation(command, TEST_TIMEOUT, false)
                .await
                .expect_err("one byte over the limit fails closed");
            assert_eq!(failure.code, PluginMutationCode::OutputTooLarge);
            assert_eq!(failure.exit_status, None);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_times_out_and_reaps_the_child() {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("pid");

        // The deadline is a parameter so this test does not wait the production
        // ten seconds; `runner_bounds_match_the_read_only_runner` pins that.
        let failure = run_bounded_mutation(
            fake(
                "/bin/sh",
                &[
                    "-c",
                    "echo $$ > \"$0\"; exec sleep 30",
                    pidfile.to_str().unwrap(),
                ],
            ),
            std::time::Duration::from_millis(300),
            false,
        )
        .await
        .expect_err("a hung child must time out");

        assert_eq!(failure.code, PluginMutationCode::Timeout);

        let pid: u32 = std::fs::read_to_string(&pidfile)
            .expect("the child recorded its pid")
            .trim()
            .parse()
            .expect("a numeric pid");
        assert!(
            !is_alive_or_zombie(pid),
            "the timed-out child was not reaped"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_non_zero_exit_keeps_numeric_status_without_output() {
        let failure = run_bounded_mutation(
            fake(
                "/bin/sh",
                &[
                    "-c",
                    "printf sentinel-secret >&2; printf sentinel-secret; exit 23",
                ],
            ),
            TEST_TIMEOUT,
            false,
        )
        .await
        .expect_err("a non-zero exit must fail");

        assert_eq!(failure.code, PluginMutationCode::NonZeroExit);
        assert_eq!(failure.exit_status, Some(23));

        let serialized = serde_json::to_string(&PluginMutationDiagnosticDto {
            agent: Some(PluginAgent::ClaudeCode),
            operation: Some(PluginOperation::Install),
            code: failure.code,
            exit_status: failure.exit_status,
        })
        .unwrap();
        assert!(!serialized.contains(SENTINEL), "{serialized}");
        assert_eq!(
            serialized,
            r#"{"agent":"claude_code","operation":"install","code":"non_zero_exit","exitStatus":23}"#
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_maps_a_pinned_confirmation_phrase_to_its_own_code() {
        let failure = run_bounded_mutation(
            fake(
                "/bin/sh",
                &[
                    "-c",
                    "printf 'sentinel-secret: this plugin is installed by running a \
                     marketplace-declared command; rerun with --yes\\n' >&2; exit 1",
                ],
            ),
            TEST_TIMEOUT,
            true,
        )
        .await
        .expect_err("a confirmation requirement is still a failure");

        assert_eq!(
            failure.code,
            PluginMutationCode::InteractiveConfirmationRequired
        );

        let serialized = serde_json::to_string(&PluginMutationDiagnosticDto {
            agent: Some(PluginAgent::ClaudeCode),
            operation: Some(PluginOperation::Install),
            code: failure.code,
            exit_status: failure.exit_status,
        })
        .unwrap();
        assert!(
            !serialized.contains(SENTINEL),
            "the captured stderr must not reach the frontend: {serialized}"
        );
        assert!(!serialized.contains("marketplace-declared"), "{serialized}");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_leaves_every_other_failure_as_a_non_zero_exit() {
        let failure = run_bounded_mutation(
            fake(
                "/bin/sh",
                &[
                    "-c",
                    "printf 'sentinel-secret: network unreachable\\n' >&2; exit 7",
                ],
            ),
            TEST_TIMEOUT,
            true,
        )
        .await
        .expect_err("an unrelated failure must not be reinterpreted");

        assert_eq!(failure.code, PluginMutationCode::NonZeroExit);
        assert_eq!(failure.exit_status, Some(7));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_ignores_confirmation_phrases_where_no_prompt_is_possible() {
        let failure = run_bounded_mutation(
            fake(
                "/bin/sh",
                &[
                    "-c",
                    "printf 'marketplace-declared command / confirmation prompt\\n' >&2; exit 1",
                ],
            ),
            TEST_TIMEOUT,
            false,
        )
        .await
        .expect_err("a non-zero exit must fail");

        assert_eq!(
            failure.code,
            PluginMutationCode::NonZeroExit,
            "only Claude install and update may claim a confirmation requirement"
        );
    }

    #[test]
    fn runner_live_commands_never_carry_a_confirmation_flag() {
        for command in mutation_commands() {
            let invocation =
                invocation_for(command.agent, command.operation, "reviewer", "official")
                    .expect("every table entry builds");
            assert!(
                !invocation
                    .args
                    .iter()
                    .any(|arg| arg == "-y" || arg == "--yes"),
                "{} must never accept a marketplace command for the user",
                invocation.display()
            );
        }
    }

    #[test]
    fn preview_live_tokens_are_unique_version_four_uuids() {
        let first = live_token();
        let second = live_token();

        assert_ne!(first, second);
        for token in [first, second] {
            let parsed = uuid::Uuid::parse_str(&token).expect("a UUID");
            assert_eq!(parsed.get_version_num(), 4);
        }
    }
}
