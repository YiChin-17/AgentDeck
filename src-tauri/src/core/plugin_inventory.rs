//! Read-only Plugin inventory for Codex and Claude Code.
//!
//! Every process this module starts comes from a fixed table: the frontend can
//! name neither an executable nor an argument, nothing runs through a shell and
//! nothing captured from a CLI is persisted or logged. See
//! `openspec/changes/inspect-codex-claude-plugins`.

use std::io::ErrorKind;
use std::process::Stdio;
use std::time::Duration;

use serde::Serialize;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Each captured stream stops at this many bytes. Exactly this size is
/// accepted; one byte more fails the command closed.
pub const MAX_STREAM_BYTES: usize = 1_048_576;
/// Deadline for one fixed CLI invocation.
pub const COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
/// A version line longer than this is not a version AgentDeck can show, so it
/// is reported as an unusable CLI contract instead of being truncated into a
/// value the CLI never printed.
pub const MAX_VERSION_CHARS: usize = 128;

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginAgent {
    Codex,
    ClaudeCode,
}

impl PluginAgent {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginAgent::Codex => "codex",
            PluginAgent::ClaudeCode => "claude_code",
        }
    }
}

/// The read-only things AgentDeck asks a Plugin CLI. There is no fourth
/// capability, and no capability that changes CLI state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    Version,
    PluginList,
    MarketplaceList,
}

impl PluginCapability {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginCapability::Version => "version",
            PluginCapability::PluginList => "plugin_list",
            PluginCapability::MarketplaceList => "marketplace_list",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginDiagnosticCode {
    CliMissing,
    UnsupportedCli,
    Timeout,
    NonZeroExit,
    InvalidJson,
    OutputTooLarge,
    MarketplaceUnavailable,
}

impl PluginDiagnosticCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            PluginDiagnosticCode::CliMissing => "cli_missing",
            PluginDiagnosticCode::UnsupportedCli => "unsupported_cli",
            PluginDiagnosticCode::Timeout => "timeout",
            PluginDiagnosticCode::NonZeroExit => "non_zero_exit",
            PluginDiagnosticCode::InvalidJson => "invalid_json",
            PluginDiagnosticCode::OutputTooLarge => "output_too_large",
            PluginDiagnosticCode::MarketplaceUnavailable => "marketplace_unavailable",
        }
    }
}

/// Everything a failed invocation is allowed to say. A raw stream, a parser
/// excerpt or a path could carry a marketplace token or the user's home layout,
/// so a failure is one code and, when the child ran, its exit status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginDiagnosticDto {
    pub agent: PluginAgent,
    pub capability: PluginCapability,
    pub code: PluginDiagnosticCode,
    pub exit_status: Option<i32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunFailure {
    pub code: PluginDiagnosticCode,
    pub exit_status: Option<i32>,
}

impl RunFailure {
    fn of(code: PluginDiagnosticCode) -> Self {
        Self {
            code,
            exit_status: None,
        }
    }
}

// ---------------------------------------------------------------------------
// The fixed capability table
// ---------------------------------------------------------------------------

/// One invocation AgentDeck is allowed to make. Both fields are `'static`: an
/// executable or an argument can only enter this table through a code change,
/// never through a request.
#[derive(Debug, Clone, Copy)]
pub struct FixedCommand {
    pub agent: PluginAgent,
    pub capability: PluginCapability,
    pub program: &'static str,
    pub args: &'static [&'static str],
}

const FIXED_COMMANDS: [FixedCommand; 6] = [
    FixedCommand {
        agent: PluginAgent::Codex,
        capability: PluginCapability::Version,
        program: "codex",
        args: &["--version"],
    },
    FixedCommand {
        agent: PluginAgent::Codex,
        capability: PluginCapability::PluginList,
        program: "codex",
        args: &["plugin", "list", "--available", "--json"],
    },
    FixedCommand {
        agent: PluginAgent::Codex,
        capability: PluginCapability::MarketplaceList,
        program: "codex",
        args: &["plugin", "marketplace", "list", "--json"],
    },
    FixedCommand {
        agent: PluginAgent::ClaudeCode,
        capability: PluginCapability::Version,
        program: "claude",
        args: &["--version"],
    },
    FixedCommand {
        agent: PluginAgent::ClaudeCode,
        capability: PluginCapability::PluginList,
        program: "claude",
        args: &["plugin", "list", "--available", "--json"],
    },
    FixedCommand {
        agent: PluginAgent::ClaudeCode,
        capability: PluginCapability::MarketplaceList,
        program: "claude",
        args: &["plugin", "marketplace", "list", "--json"],
    },
];

pub fn fixed_commands() -> &'static [FixedCommand] {
    &FIXED_COMMANDS
}

pub fn fixed_command(agent: PluginAgent, capability: PluginCapability) -> &'static FixedCommand {
    FIXED_COMMANDS
        .iter()
        .find(|command| command.agent == agent && command.capability == capability)
        .expect("the fixed table covers every Agent and capability")
}

// ---------------------------------------------------------------------------
// Bounded process runner
// ---------------------------------------------------------------------------

enum StreamRead {
    Bounded(Vec<u8>),
    TooLarge,
}

/// Reads at most `MAX_STREAM_BYTES` and stops. Reading one byte past the limit
/// is what makes the exact limit acceptable and the next byte a failure.
async fn read_bounded<R>(mut stream: R) -> std::io::Result<StreamRead>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = stream.read(&mut chunk).await?;
        if read == 0 {
            return Ok(StreamRead::Bounded(buffer));
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > MAX_STREAM_BYTES {
            return Ok(StreamRead::TooLarge);
        }
    }
}

/// Runs one already-built command under a deadline and a per-stream size limit,
/// returning its stdout or a sanitized failure.
///
/// This seam takes a `Command` so tests can drive a fake child through the real
/// bounds; production reaches it only through `run_fixed`, which builds the
/// command from the fixed table.
async fn run_bounded(mut command: Command, deadline: Duration) -> Result<Vec<u8>, RunFailure> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) if error.kind() == ErrorKind::NotFound => {
            return Err(RunFailure::of(PluginDiagnosticCode::CliMissing))
        }
        Err(_) => return Err(RunFailure::of(PluginDiagnosticCode::UnsupportedCli)),
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
                out_done = Some(joined.map_err(|_| RunFailure::of(PluginDiagnosticCode::UnsupportedCli))?
                    .map_err(|_| RunFailure::of(PluginDiagnosticCode::UnsupportedCli))?);
            }
            joined = &mut err_task, if err_done.is_none() => {
                err_done = Some(joined.map_err(|_| RunFailure::of(PluginDiagnosticCode::UnsupportedCli))?
                    .map_err(|_| RunFailure::of(PluginDiagnosticCode::UnsupportedCli))?);
            }
            _ = &mut sleep => timed_out = true,
        }
        too_large = matches!(out_done, Some(StreamRead::TooLarge))
            || matches!(err_done, Some(StreamRead::TooLarge));
    }

    if timed_out || too_large {
        out_task.abort();
        err_task.abort();
        return Err(RunFailure::of(kill_and_reap(
            &mut child,
            if timed_out {
                PluginDiagnosticCode::Timeout
            } else {
                PluginDiagnosticCode::OutputTooLarge
            },
        )
        .await));
    }

    let status = tokio::select! {
        status = child.wait() => status,
        _ = &mut sleep => {
            return Err(RunFailure::of(
                kill_and_reap(&mut child, PluginDiagnosticCode::Timeout).await,
            ))
        }
    };
    let status = status.map_err(|_| RunFailure::of(PluginDiagnosticCode::UnsupportedCli))?;

    if !status.success() {
        return Err(RunFailure {
            code: PluginDiagnosticCode::NonZeroExit,
            exit_status: status.code(),
        });
    }

    match out_done {
        Some(StreamRead::Bounded(bytes)) => Ok(bytes),
        _ => Err(RunFailure::of(PluginDiagnosticCode::OutputTooLarge)),
    }
}

/// Terminates the child and waits for it, so a killed process never stays a
/// zombie for the lifetime of the application.
async fn kill_and_reap(
    child: &mut tokio::process::Child,
    code: PluginDiagnosticCode,
) -> PluginDiagnosticCode {
    let _ = child.start_kill();
    let _ = child.wait().await;
    code
}

/// The only way production starts a process: the argument list comes from the
/// fixed table and nothing else.
pub async fn run_fixed(command: &FixedCommand) -> Result<Vec<u8>, RunFailure> {
    let mut process = Command::new(command.program);
    process.args(command.args);
    run_bounded(process, COMMAND_TIMEOUT).await
}

// ---------------------------------------------------------------------------
// Normalized inventory
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginPresence {
    Installed,
    NotInstalled,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginAvailability {
    Available,
    NotAvailable,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginScope {
    User,
    Project,
    Local,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginEnablement {
    Enabled,
    Disabled,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginUpdateState {
    UpToDate,
    UpdateAvailable,
    Unknown,
}

/// One Plugin as AgentDeck displays it.
///
/// Only fields both CLIs describe in their public JSON reach this DTO: install
/// paths, marketplace roots and auth policies stay in the process output that
/// produced them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInventoryItemDto {
    /// `<agent>:<marketplace>:<plugin id>` — route-local, and the merge key.
    pub id: String,
    pub agent: PluginAgent,
    pub plugin_id: String,
    pub display_name: String,
    pub installed: PluginPresence,
    pub available: PluginAvailability,
    pub installed_version: Option<String>,
    pub available_version: Option<String>,
    pub scope: PluginScope,
    pub marketplace: Option<String>,
    pub enabled: PluginEnablement,
    pub update: PluginUpdateState,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginMarketplaceDto {
    pub name: String,
    /// How the CLI describes the origin (`github`, `local`, …). The repository
    /// and the install location are deliberately left out.
    pub source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAgentInventoryDto {
    pub agent: PluginAgent,
    pub cli_version: Option<String>,
    /// The read capabilities that actually answered for this response.
    pub capabilities: Vec<PluginCapability>,
    pub marketplaces: Vec<PluginMarketplaceDto>,
    pub items: Vec<PluginInventoryItemDto>,
    pub diagnostics: Vec<PluginDiagnosticDto>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInventoryDto {
    pub agents: Vec<PluginAgentInventoryDto>,
    pub generated_at: String,
}

/// The response was not the shape this Agent's adapter contracts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PluginParseError;

// ---------------------------------------------------------------------------
// Agent-specific parsers
// ---------------------------------------------------------------------------

/// Which listing an item came out of. The array is the CLI's own statement
/// about the record, so it may set presence; nothing else may.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Feed {
    Installed,
    Available,
}

fn text(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(|field| field.as_str())
        .map(str::trim)
        .filter(|field| !field.is_empty())
        .map(str::to_string)
}

fn flag(value: &serde_json::Value, key: &str) -> Option<bool> {
    value.get(key).and_then(|field| field.as_bool())
}

/// Splits a `<plugin>@<marketplace>` composite id. Both CLIs publish this
/// format, so reading it is parsing, not guessing — but a value without the
/// separator keeps its whole text as the plugin id and no marketplace.
fn split_composite_id(id: &str) -> (String, Option<String>) {
    match id.rsplit_once('@') {
        Some((plugin, market)) if !plugin.is_empty() && !market.is_empty() => {
            (plugin.to_string(), Some(market.to_string()))
        }
        _ => (id.to_string(), None),
    }
}

fn scope_of(raw: Option<String>) -> PluginScope {
    match raw.as_deref() {
        Some("user") => PluginScope::User,
        Some("project") => PluginScope::Project,
        Some("local") => PluginScope::Local,
        // An unrecognized scope is a scope AgentDeck does not know, not a
        // default one.
        _ => PluginScope::Unknown,
    }
}

fn enablement_of(raw: Option<bool>) -> PluginEnablement {
    match raw {
        Some(true) => PluginEnablement::Enabled,
        Some(false) => PluginEnablement::Disabled,
        None => PluginEnablement::Unknown,
    }
}

fn route_local_id(agent: PluginAgent, marketplace: Option<&str>, plugin_id: &str) -> String {
    format!(
        "{}:{}:{}",
        agent.as_str(),
        marketplace.unwrap_or("unknown"),
        plugin_id
    )
}

#[allow(clippy::too_many_arguments)]
fn build_item(
    agent: PluginAgent,
    feed: Feed,
    plugin_id: String,
    display_name: String,
    marketplace: Option<String>,
    version: Option<String>,
    installed_flag: Option<bool>,
    scope: PluginScope,
    enabled: PluginEnablement,
) -> PluginInventoryItemDto {
    let installed = match installed_flag {
        Some(true) => PluginPresence::Installed,
        Some(false) => PluginPresence::NotInstalled,
        None if feed == Feed::Installed => PluginPresence::Installed,
        None => PluginPresence::Unknown,
    };
    let available = match feed {
        Feed::Available => PluginAvailability::Available,
        // A plugin listing never states whether a marketplace still offers an
        // installed plugin.
        Feed::Installed => PluginAvailability::Unknown,
    };
    let (installed_version, available_version) = match feed {
        Feed::Installed => (version, None),
        Feed::Available => (None, version),
    };

    PluginInventoryItemDto {
        id: route_local_id(agent, marketplace.as_deref(), &plugin_id),
        agent,
        plugin_id,
        display_name,
        installed,
        available,
        installed_version,
        available_version,
        scope,
        marketplace,
        enabled,
        update: PluginUpdateState::Unknown,
    }
}

/// Reads the `installed` and `available` arrays of a plugin listing. An
/// unrecognized sibling field is ignored; a root that is not an object is not
/// this contract at all.
fn feeds_of(bytes: &[u8]) -> Result<Vec<(Feed, Vec<serde_json::Value>)>, PluginParseError> {
    let root: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| PluginParseError)?;
    let object = root.as_object().ok_or(PluginParseError)?;
    let array_of = |key: &str| -> Vec<serde_json::Value> {
        object
            .get(key)
            .and_then(|value| value.as_array())
            .cloned()
            .unwrap_or_default()
    };
    Ok(vec![
        (Feed::Installed, array_of("installed")),
        (Feed::Available, array_of("available")),
    ])
}

pub fn parse_codex_plugins(bytes: &[u8]) -> Result<Vec<PluginInventoryItemDto>, PluginParseError> {
    let mut items = Vec::new();
    for (feed, records) in feeds_of(bytes)? {
        for record in records {
            let composite = text(&record, "pluginId");
            let (from_id, market_from_id) = match composite.as_deref() {
                Some(id) => {
                    let (plugin, market) = split_composite_id(id);
                    (Some(plugin), market)
                }
                None => (None, None),
            };
            let plugin_id = match text(&record, "name").or(from_id) {
                Some(id) => id,
                // Without an identity the record cannot be merged or displayed.
                None => continue,
            };
            let marketplace = text(&record, "marketplaceName").or(market_from_id);
            let display_name = text(&record, "name").unwrap_or_else(|| plugin_id.clone());

            items.push(build_item(
                PluginAgent::Codex,
                feed,
                plugin_id,
                display_name,
                marketplace,
                text(&record, "version"),
                flag(&record, "installed"),
                // Codex reports no scope for a plugin.
                PluginScope::Unknown,
                enablement_of(flag(&record, "enabled")),
            ));
        }
    }
    Ok(items)
}

pub fn parse_claude_plugins(bytes: &[u8]) -> Result<Vec<PluginInventoryItemDto>, PluginParseError> {
    let mut items = Vec::new();
    for (feed, records) in feeds_of(bytes)? {
        for record in records {
            // The installed listing keys a record by `id`; the available
            // listing by `pluginId` plus explicit name and marketplace.
            let composite = text(&record, "id").or_else(|| text(&record, "pluginId"));
            let (from_id, market_from_id) = match composite.as_deref() {
                Some(id) => {
                    let (plugin, market) = split_composite_id(id);
                    (Some(plugin), market)
                }
                None => (None, None),
            };
            let plugin_id = match text(&record, "name").or(from_id) {
                Some(id) => id,
                None => continue,
            };
            let marketplace = text(&record, "marketplaceName").or(market_from_id);
            let display_name = text(&record, "name").unwrap_or_else(|| plugin_id.clone());

            items.push(build_item(
                PluginAgent::ClaudeCode,
                feed,
                plugin_id,
                display_name,
                marketplace,
                text(&record, "version"),
                flag(&record, "installed"),
                scope_of(text(&record, "scope")),
                enablement_of(flag(&record, "enabled")),
            ));
        }
    }
    Ok(items)
}

pub fn parse_codex_marketplaces(
    bytes: &[u8],
) -> Result<Vec<PluginMarketplaceDto>, PluginParseError> {
    let root: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| PluginParseError)?;
    let records = root
        .as_object()
        .ok_or(PluginParseError)?
        .get("marketplaces")
        .and_then(|value| value.as_array())
        .cloned()
        .unwrap_or_default();

    Ok(records
        .iter()
        .filter_map(|record| {
            Some(PluginMarketplaceDto {
                name: text(record, "name")?,
                source: record
                    .get("marketplaceSource")
                    .and_then(|source| text(source, "sourceType")),
            })
        })
        .collect())
}

pub fn parse_claude_marketplaces(
    bytes: &[u8],
) -> Result<Vec<PluginMarketplaceDto>, PluginParseError> {
    let root: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| PluginParseError)?;
    let records = root.as_array().ok_or(PluginParseError)?;

    Ok(records
        .iter()
        .filter_map(|record| {
            Some(PluginMarketplaceDto {
                name: text(record, "name")?,
                source: text(record, "source"),
            })
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Identity, merge and ordering
// ---------------------------------------------------------------------------

fn merge_presence(base: PluginPresence, other: PluginPresence) -> PluginPresence {
    match (base, other) {
        (PluginPresence::Installed, _) | (_, PluginPresence::Installed) => PluginPresence::Installed,
        (PluginPresence::NotInstalled, _) | (_, PluginPresence::NotInstalled) => {
            PluginPresence::NotInstalled
        }
        _ => PluginPresence::Unknown,
    }
}

fn merge_availability(base: PluginAvailability, other: PluginAvailability) -> PluginAvailability {
    match (base, other) {
        (PluginAvailability::Available, _) | (_, PluginAvailability::Available) => {
            PluginAvailability::Available
        }
        (PluginAvailability::NotAvailable, _) | (_, PluginAvailability::NotAvailable) => {
            PluginAvailability::NotAvailable
        }
        _ => PluginAvailability::Unknown,
    }
}

fn merge_into(base: &mut PluginInventoryItemDto, other: PluginInventoryItemDto) {
    base.installed = merge_presence(base.installed, other.installed);
    base.available = merge_availability(base.available, other.available);
    base.installed_version = base.installed_version.take().or(other.installed_version);
    base.available_version = base.available_version.take().or(other.available_version);
    if base.scope == PluginScope::Unknown {
        base.scope = other.scope;
    }
    if base.enabled == PluginEnablement::Unknown {
        base.enabled = other.enabled;
    }
    if base.update == PluginUpdateState::Unknown {
        base.update = other.update;
    }
    // A record that had no display name of its own fell back to its plugin id;
    // a feed that publishes one is more informative.
    if base.display_name == base.plugin_id && other.display_name != other.plugin_id {
        base.display_name = other.display_name;
    }
}

/// Merges records that share an Agent, a marketplace and a plugin id, then puts
/// the result in a display order that does not depend on CLI array order.
pub fn normalize(items: Vec<PluginInventoryItemDto>) -> Vec<PluginInventoryItemDto> {
    let mut merged: Vec<PluginInventoryItemDto> = Vec::new();
    for item in items {
        match merged.iter_mut().find(|existing| existing.id == item.id) {
            Some(existing) => merge_into(existing, item),
            None => merged.push(item),
        }
    }
    merged.sort_by(|left, right| {
        left.agent
            .as_str()
            .cmp(right.agent.as_str())
            .then_with(|| left.display_name.cmp(&right.display_name))
            .then_with(|| left.marketplace.cmp(&right.marketplace))
            .then_with(|| left.plugin_id.cmp(&right.plugin_id))
    });
    merged
}

// ---------------------------------------------------------------------------
// Agent-isolated aggregation
// ---------------------------------------------------------------------------

/// What one fixed command returned, before any of it is interpreted.
pub struct CommandOutput {
    pub agent: PluginAgent,
    pub capability: PluginCapability,
    pub result: Result<Vec<u8>, RunFailure>,
}

/// Maps a runner failure onto the vocabulary the frontend localizes.
///
/// Exit codes are not a portable way to tell "this CLI has no such subcommand"
/// from any other failure, so `unsupported_cli` is claimed only where the
/// fixed CLI contract cannot be established — the version probe. A marketplace
/// listing that fails is reported as unavailable without inspecting stderr.
fn diagnostic_code(
    capability: PluginCapability,
    failure: &RunFailure,
) -> PluginDiagnosticCode {
    match (failure.code, capability) {
        (PluginDiagnosticCode::NonZeroExit, PluginCapability::Version) => {
            PluginDiagnosticCode::UnsupportedCli
        }
        (PluginDiagnosticCode::NonZeroExit, PluginCapability::MarketplaceList) => {
            PluginDiagnosticCode::MarketplaceUnavailable
        }
        (code, _) => code,
    }
}

fn output_of<'a>(
    outputs: &'a [CommandOutput],
    agent: PluginAgent,
    capability: PluginCapability,
) -> Option<&'a CommandOutput> {
    outputs
        .iter()
        .find(|output| output.agent == agent && output.capability == capability)
}

/// The version line as the CLI printed it, or nothing when the response is not
/// a version AgentDeck can show.
fn version_of(bytes: &[u8]) -> Option<String> {
    let text = std::str::from_utf8(bytes).ok()?;
    let line = text.lines().next()?.trim();
    if line.is_empty() || line.chars().count() > MAX_VERSION_CHARS {
        return None;
    }
    Some(line.to_string())
}

/// Builds the response from whatever the six fixed commands returned.
///
/// Pure by design: it is given no store, no Library path and no filesystem
/// handle, so a refresh cannot persist anything even by mistake.
pub fn assemble(outputs: Vec<CommandOutput>, generated_at: String) -> PluginInventoryDto {
    let agents = [PluginAgent::Codex, PluginAgent::ClaudeCode]
        .into_iter()
        .map(|agent| assemble_agent(&outputs, agent))
        .collect();

    PluginInventoryDto {
        agents,
        generated_at,
    }
}

fn assemble_agent(outputs: &[CommandOutput], agent: PluginAgent) -> PluginAgentInventoryDto {
    let mut capabilities = Vec::new();
    let mut diagnostics = Vec::new();
    let mut cli_version = None;
    let mut items = Vec::new();
    let mut marketplaces = Vec::new();

    let mut record = |capability: PluginCapability, code: PluginDiagnosticCode, exit: Option<i32>| {
        diagnostics.push(PluginDiagnosticDto {
            agent,
            capability,
            code,
            exit_status: exit,
        });
    };

    match output_of(outputs, agent, PluginCapability::Version).map(|output| &output.result) {
        Some(Ok(bytes)) => match version_of(bytes) {
            Some(version) => {
                cli_version = Some(version);
                capabilities.push(PluginCapability::Version);
            }
            None => record(
                PluginCapability::Version,
                PluginDiagnosticCode::UnsupportedCli,
                None,
            ),
        },
        Some(Err(failure)) => record(
            PluginCapability::Version,
            diagnostic_code(PluginCapability::Version, failure),
            failure.exit_status,
        ),
        None => {}
    }

    match output_of(outputs, agent, PluginCapability::PluginList).map(|output| &output.result) {
        Some(Ok(bytes)) => {
            let parsed = match agent {
                PluginAgent::Codex => parse_codex_plugins(bytes),
                PluginAgent::ClaudeCode => parse_claude_plugins(bytes),
            };
            match parsed {
                Ok(parsed) => {
                    items = normalize(parsed);
                    capabilities.push(PluginCapability::PluginList);
                }
                Err(_) => record(
                    PluginCapability::PluginList,
                    PluginDiagnosticCode::InvalidJson,
                    None,
                ),
            }
        }
        Some(Err(failure)) => record(
            PluginCapability::PluginList,
            diagnostic_code(PluginCapability::PluginList, failure),
            failure.exit_status,
        ),
        None => {}
    }

    match output_of(outputs, agent, PluginCapability::MarketplaceList).map(|output| &output.result) {
        Some(Ok(bytes)) => {
            let parsed = match agent {
                PluginAgent::Codex => parse_codex_marketplaces(bytes),
                PluginAgent::ClaudeCode => parse_claude_marketplaces(bytes),
            };
            match parsed {
                Ok(parsed) => {
                    marketplaces = parsed;
                    capabilities.push(PluginCapability::MarketplaceList);
                }
                Err(_) => record(
                    PluginCapability::MarketplaceList,
                    PluginDiagnosticCode::InvalidJson,
                    None,
                ),
            }
        }
        Some(Err(failure)) => record(
            PluginCapability::MarketplaceList,
            diagnostic_code(PluginCapability::MarketplaceList, failure),
            failure.exit_status,
        ),
        None => {}
    }

    PluginAgentInventoryDto {
        agent,
        cli_version,
        capabilities,
        marketplaces,
        items,
        diagnostics,
    }
}

/// Runs the six fixed commands and assembles the response.
///
/// Each Agent's version probe runs first: a working version gates the two
/// inventory commands, and a failed version propagates its diagnostic to
/// the capabilities that were never attempted. Both Agents' phases run
/// concurrently so one absent CLI does not delay the other.
pub async fn collect(generated_at: String) -> PluginInventoryDto {
    let (codex_ver, claude_ver) = tokio::join!(
        run_fixed(fixed_command(PluginAgent::Codex, PluginCapability::Version)),
        run_fixed(fixed_command(PluginAgent::ClaudeCode, PluginCapability::Version))
    );
    let (codex_out, claude_out) = tokio::join!(
        collect_after_version(PluginAgent::Codex, codex_ver),
        collect_after_version(PluginAgent::ClaudeCode, claude_ver)
    );
    let mut outputs = codex_out;
    outputs.extend(claude_out);
    assemble(outputs, generated_at)
}

async fn collect_after_version(
    agent: PluginAgent,
    version_result: Result<Vec<u8>, RunFailure>,
) -> Vec<CommandOutput> {
    let mut outputs = Vec::new();
    match version_result {
        Ok(bytes) => {
            let version_supported = version_of(&bytes).is_some();
            outputs.push(CommandOutput {
                agent,
                capability: PluginCapability::Version,
                result: Ok(bytes),
            });
            if !version_supported {
                outputs.push(CommandOutput {
                    agent,
                    capability: PluginCapability::PluginList,
                    result: Err(RunFailure::of(PluginDiagnosticCode::UnsupportedCli)),
                });
                outputs.push(CommandOutput {
                    agent,
                    capability: PluginCapability::MarketplaceList,
                    result: Err(RunFailure::of(PluginDiagnosticCode::UnsupportedCli)),
                });
                return outputs;
            }
            let (pl, ml) = tokio::join!(
                run_fixed(fixed_command(agent, PluginCapability::PluginList)),
                run_fixed(fixed_command(agent, PluginCapability::MarketplaceList))
            );
            outputs.push(CommandOutput {
                agent,
                capability: PluginCapability::PluginList,
                result: pl,
            });
            outputs.push(CommandOutput {
                agent,
                capability: PluginCapability::MarketplaceList,
                result: ml,
            });
        }
        Err(failure) => {
            let propagated = diagnostic_code(PluginCapability::Version, &failure);
            outputs.push(CommandOutput {
                agent,
                capability: PluginCapability::Version,
                result: Err(failure),
            });
            outputs.push(CommandOutput {
                agent,
                capability: PluginCapability::PluginList,
                result: Err(RunFailure::of(propagated)),
            });
            outputs.push(CommandOutput {
                agent,
                capability: PluginCapability::MarketplaceList,
                result: Err(RunFailure::of(propagated)),
            });
        }
    }
    outputs
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;
    use std::process::Stdio;
    use std::time::Duration;

    use tokio::process::Command;

    /// A deadline long enough that a healthy fake command always finishes, and
    /// short enough that a hung one does not stall the suite.
    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    fn fake(program: &str, args: &[&str]) -> Command {
        let mut command = Command::new(program);
        command.args(args);
        command
    }

    /// Writes valid JSON padded to exactly `total` bytes.
    fn payload_of_exact_size(path: &Path, total: usize) -> String {
        let prefix = r#"{"installed":[],"available":[],"pad":""#;
        let suffix = r#""}"#;
        let pad = total - prefix.len() - suffix.len();
        let text = format!("{prefix}{}{suffix}", "a".repeat(pad));
        assert_eq!(text.len(), total);
        std::fs::write(path, &text).expect("write payload");
        text
    }

    #[cfg(unix)]
    fn is_alive_or_zombie(pid: u32) -> bool {
        let out = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .expect("ps");
        !String::from_utf8_lossy(&out.stdout).trim().is_empty()
    }

    // ── The fixed capability table ──

    #[test]
    fn runner_fixed_commands_are_the_documented_six() {
        let actual: Vec<(String, String, String, Vec<String>)> = fixed_commands()
            .iter()
            .map(|command| {
                (
                    command.agent.as_str().to_string(),
                    command.capability.as_str().to_string(),
                    command.program.to_string(),
                    command.args.iter().map(|arg| arg.to_string()).collect(),
                )
            })
            .collect();

        let expected = vec![
            ("codex", "version", "codex", vec!["--version"]),
            (
                "codex",
                "plugin_list",
                "codex",
                vec!["plugin", "list", "--available", "--json"],
            ),
            (
                "codex",
                "marketplace_list",
                "codex",
                vec!["plugin", "marketplace", "list", "--json"],
            ),
            ("claude_code", "version", "claude", vec!["--version"]),
            (
                "claude_code",
                "plugin_list",
                "claude",
                vec!["plugin", "list", "--available", "--json"],
            ),
            (
                "claude_code",
                "marketplace_list",
                "claude",
                vec!["plugin", "marketplace", "list", "--json"],
            ),
        ];
        let expected: Vec<(String, String, String, Vec<String>)> = expected
            .into_iter()
            .map(|(agent, capability, program, args)| {
                (
                    agent.to_string(),
                    capability.to_string(),
                    program.to_string(),
                    args.into_iter().map(|arg| arg.to_string()).collect(),
                )
            })
            .collect();

        assert_eq!(actual, expected);
    }

    #[test]
    fn runner_fixed_arguments_carry_no_mutation_or_config_override() {
        // A read-only guarantee that survives a careless edit of the table: no
        // entry may gain a verb that changes CLI state, a config override or a
        // shell metacharacter.
        const MUTATIONS: [&str; 12] = [
            "add", "install", "i", "update", "remove", "rm", "uninstall", "enable", "disable",
            "validate", "details", "eval",
        ];
        for command in fixed_commands() {
            for arg in command.args {
                assert!(
                    !MUTATIONS.contains(arg),
                    "{} {:?} carries mutation verb {arg}",
                    command.program,
                    command.args
                );
                assert!(
                    !arg.starts_with("-c") && !arg.starts_with("--config"),
                    "{} {:?} carries a config override",
                    command.program,
                    command.args
                );
                assert!(
                    !arg.contains(['|', ';', '&', '$', '`', '>', '<', '\n']),
                    "{} {:?} carries a shell metacharacter",
                    command.program,
                    command.args
                );
            }
            assert!(
                !command.program.contains(['/', '\\']),
                "{} must be resolved by name, not by a path",
                command.program
            );
        }
    }

    #[test]
    fn runner_lookup_covers_every_agent_and_capability() {
        for agent in [PluginAgent::Codex, PluginAgent::ClaudeCode] {
            for capability in [
                PluginCapability::Version,
                PluginCapability::PluginList,
                PluginCapability::MarketplaceList,
            ] {
                let command = fixed_command(agent, capability);
                assert_eq!(command.agent, agent);
                assert_eq!(command.capability, capability);
            }
        }
    }

    #[test]
    fn runner_deadline_is_ten_seconds() {
        assert_eq!(COMMAND_TIMEOUT, Duration::from_secs(10));
        assert_eq!(MAX_STREAM_BYTES, 1_048_576);
    }

    // ── Process boundary ──

    #[tokio::test]
    async fn runner_missing_executable_reports_cli_missing() {
        let result = run_bounded(
            fake("agentdeck-plugin-cli-that-does-not-exist", &[]),
            TEST_TIMEOUT,
        )
        .await;

        let failure = result.expect_err("a missing executable must fail");
        assert_eq!(failure.code, PluginDiagnosticCode::CliMissing);
        assert_eq!(failure.exit_status, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_accepts_the_exact_stdout_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("payload.json");
        let text = payload_of_exact_size(&file, MAX_STREAM_BYTES);

        let stdout = run_bounded(fake("/bin/cat", &[file.to_str().unwrap()]), TEST_TIMEOUT)
            .await
            .expect("the exact limit is accepted");

        assert_eq!(stdout.len(), MAX_STREAM_BYTES);
        assert_eq!(stdout, text.into_bytes());
        assert!(serde_json::from_slice::<serde_json::Value>(&stdout).is_ok());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_rejects_one_byte_over_the_stdout_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("payload.json");
        payload_of_exact_size(&file, MAX_STREAM_BYTES + 1);

        let failure = run_bounded(fake("/bin/cat", &[file.to_str().unwrap()]), TEST_TIMEOUT)
            .await
            .expect_err("one byte over the limit fails closed");

        assert_eq!(failure.code, PluginDiagnosticCode::OutputTooLarge);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_rejects_one_byte_over_the_stderr_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("payload.json");
        payload_of_exact_size(&file, MAX_STREAM_BYTES + 1);

        let failure = run_bounded(
            fake(
                "/bin/sh",
                &["-c", "cat \"$0\" >&2", file.to_str().unwrap()],
            ),
            TEST_TIMEOUT,
        )
        .await
        .expect_err("an oversized stderr fails closed");

        assert_eq!(failure.code, PluginDiagnosticCode::OutputTooLarge);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_times_out_and_reaps_the_child() {
        let dir = tempfile::tempdir().unwrap();
        let pidfile = dir.path().join("pid");

        // The deadline is a parameter so this test does not have to wait the
        // full production ten seconds; `runner_deadline_is_ten_seconds` pins the
        // value production passes.
        let failure = run_bounded(
            fake(
                "/bin/sh",
                &["-c", "echo $$ > \"$0\"; exec sleep 30", pidfile.to_str().unwrap()],
            ),
            Duration::from_millis(300),
        )
        .await
        .expect_err("a hung child must time out");

        assert_eq!(failure.code, PluginDiagnosticCode::Timeout);

        let pid: u32 = std::fs::read_to_string(&pidfile)
            .expect("the child recorded its pid")
            .trim()
            .parse()
            .expect("a numeric pid");
        // A killed but unreaped child stays visible to `ps` as a zombie.
        assert!(
            !is_alive_or_zombie(pid),
            "the timed-out child was not reaped"
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_non_zero_exit_keeps_status_without_output() {
        let failure = run_bounded(
            fake(
                "/bin/sh",
                &["-c", "printf sentinel-secret >&2; printf sentinel-secret; exit 23"],
            ),
            TEST_TIMEOUT,
        )
        .await
        .expect_err("a non-zero exit must fail");

        assert_eq!(failure.code, PluginDiagnosticCode::NonZeroExit);
        assert_eq!(failure.exit_status, Some(23));

        let diagnostic = PluginDiagnosticDto {
            agent: PluginAgent::Codex,
            capability: PluginCapability::PluginList,
            code: failure.code,
            exit_status: failure.exit_status,
        };
        let serialized = serde_json::to_string(&diagnostic).unwrap();
        assert!(!serialized.contains("sentinel-secret"), "{serialized}");
        assert_eq!(
            serialized,
            r#"{"agent":"codex","capability":"plugin_list","code":"non_zero_exit","exitStatus":23}"#
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_closes_stdin() {
        // `cat` returns immediately on an already-closed stdin, and blocks past
        // the deadline on an inherited one.
        let stdout = run_bounded(
            fake("/bin/sh", &["-c", "cat; printf closed"]),
            Duration::from_millis(500),
        )
        .await
        .expect("a closed stdin lets the child finish");

        assert_eq!(stdout, b"closed".to_vec());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_never_uses_a_shell() {
        let payload = "$(printf pwned); echo pwned | tee /dev/null";

        let stdout = run_bounded(fake("/bin/echo", &[payload]), TEST_TIMEOUT)
            .await
            .expect("echo succeeds");

        assert_eq!(String::from_utf8_lossy(&stdout).trim_end(), payload);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_returns_stdout_of_a_successful_command() {
        let stdout = run_bounded(fake("/bin/echo", &["codex-cli 0.144.5"]), TEST_TIMEOUT)
            .await
            .expect("echo succeeds");

        assert_eq!(String::from_utf8_lossy(&stdout).trim_end(), "codex-cli 0.144.5");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_gives_the_child_no_inherited_streams() {
        // The production runner must own both pipes; an inherited stdout would
        // leak CLI output into the application log.
        let mut command = fake("/bin/echo", &["ok"]);
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let stdout = run_bounded(command, TEST_TIMEOUT).await.expect("echo succeeds");
        assert_eq!(String::from_utf8_lossy(&stdout).trim_end(), "ok");
    }

    // ── Fixtures ──
    //
    // Shapes captured from Codex CLI 0.144.5 and Claude Code 2.1.x, reduced to
    // the fields this change reads plus the fields it must ignore.

    /// Two Codex marketplaces list the same plugin id, and one record omits
    /// every optional status field.
    const CODEX_PLUGINS: &str = r#"{
      "installed": [
        {
          "pluginId": "documents@official",
          "name": "documents",
          "marketplaceName": "official",
          "version": "26.812.11052",
          "installed": true,
          "enabled": true,
          "source": { "source": "local", "path": "/Users/someone/.cache/codex/documents" },
          "marketplaceSource": { "sourceType": "local", "source": "/Users/someone/.cache/codex" },
          "installPolicy": "AVAILABLE",
          "authPolicy": "ON_USE"
        },
        {
          "pluginId": "reviewer@official",
          "marketplaceName": "official"
        }
      ],
      "available": [
        {
          "pluginId": "reviewer@team",
          "name": "reviewer",
          "marketplaceName": "team",
          "version": "release-2026.08+vendor",
          "installed": false,
          "enabled": false,
          "source": { "source": "local", "path": "/Users/someone/.codex/team/reviewer" },
          "marketplaceSource": { "sourceType": "local", "source": "/Users/someone/.codex/team" }
        }
      ]
    }"#;

    const CODEX_MARKETPLACES: &str = r#"{
      "marketplaces": [
        {
          "name": "official",
          "root": "/Users/someone/.cache/codex/official",
          "marketplaceSource": { "sourceType": "local", "source": "/Users/someone/.cache/codex/official" }
        },
        {
          "name": "team",
          "root": "/Users/someone/.codex/team",
          "marketplaceSource": { "sourceType": "git", "source": "/Users/someone/.codex/team" }
        }
      ]
    }"#;

    const CLAUDE_PLUGINS: &str = r#"{
      "installed": [
        {
          "id": "reviewer@official",
          "version": "1.0",
          "scope": "user",
          "enabled": false,
          "installPath": "/Users/someone/.claude/plugins/cache/official/reviewer/1.0",
          "installedAt": "2026-02-06T13:39:22.142Z",
          "lastUpdated": "2026-08-11T12:31:02.961Z"
        },
        {
          "id": "surveyor@official",
          "version": "3.2",
          "scope": "orbital",
          "enabled": true,
          "installPath": "/Users/someone/.claude/plugins/cache/official/surveyor/3.2"
        }
      ],
      "available": [
        {
          "pluginId": "reviewer@official",
          "name": "reviewer",
          "description": "Reviews changes",
          "marketplaceName": "official",
          "version": "1.1",
          "source": "./plugins/reviewer"
        },
        {
          "pluginId": "planner@official",
          "name": "planner",
          "description": "Plans work",
          "marketplaceName": "official",
          "source": "./plugins/planner"
        }
      ]
    }"#;

    const CLAUDE_MARKETPLACES: &str = r#"[
      {
        "name": "official",
        "source": "github",
        "repo": "example/official",
        "installLocation": "/Users/someone/.claude/plugins/marketplaces/official"
      }
    ]"#;

    fn item<'a>(items: &'a [PluginInventoryItemDto], id: &str) -> &'a PluginInventoryItemDto {
        items
            .iter()
            .find(|item| item.id == id)
            .unwrap_or_else(|| panic!("no item {id} in {:?}", ids(items)))
    }

    fn ids(items: &[PluginInventoryItemDto]) -> Vec<String> {
        items.iter().map(|item| item.id.clone()).collect()
    }

    // ── Agent-specific parsers ──

    #[test]
    fn parse_codex_reads_installed_and_available_arrays() {
        let items = parse_codex_plugins(CODEX_PLUGINS.as_bytes()).expect("valid Codex JSON");
        let installed = item(&items, "codex:official:documents");

        assert_eq!(installed.agent, PluginAgent::Codex);
        assert_eq!(installed.plugin_id, "documents");
        assert_eq!(installed.display_name, "documents");
        assert_eq!(installed.marketplace.as_deref(), Some("official"));
        assert_eq!(installed.installed, PluginPresence::Installed);
        assert_eq!(installed.installed_version.as_deref(), Some("26.812.11052"));
        assert_eq!(installed.available_version, None);
        assert_eq!(installed.enabled, PluginEnablement::Enabled);
        // The plugin listing never states whether a marketplace still offers an
        // installed plugin, so availability stays unknown rather than false.
        assert_eq!(installed.available, PluginAvailability::Unknown);
        assert_eq!(installed.scope, PluginScope::Unknown);
        assert_eq!(installed.update, PluginUpdateState::Unknown);
    }

    #[test]
    fn parse_codex_missing_fields_stay_unknown() {
        let items = parse_codex_plugins(CODEX_PLUGINS.as_bytes()).expect("valid Codex JSON");
        let sparse = item(&items, "codex:official:reviewer");

        assert_eq!(sparse.plugin_id, "reviewer");
        assert_eq!(sparse.marketplace.as_deref(), Some("official"));
        assert_eq!(sparse.scope, PluginScope::Unknown);
        assert_eq!(sparse.enabled, PluginEnablement::Unknown);
        assert_eq!(sparse.available_version, None);
        assert_eq!(sparse.update, PluginUpdateState::Unknown);
        // Claude Code reports `scope` and this record does not: an Agent never
        // borrows another Agent's defaults.
        assert_eq!(sparse.installed_version, None);
    }

    #[test]
    fn parse_codex_versions_remain_opaque() {
        let items = parse_codex_plugins(CODEX_PLUGINS.as_bytes()).expect("valid Codex JSON");
        let offered = item(&items, "codex:team:reviewer");

        assert_eq!(offered.available_version.as_deref(), Some("release-2026.08+vendor"));
        assert_eq!(offered.installed, PluginPresence::NotInstalled);
        assert_eq!(offered.available, PluginAvailability::Available);
    }

    #[test]
    fn parse_codex_additive_field_does_not_invalidate_a_response() {
        let text = CODEX_PLUGINS.replace(
            r#""name": "documents","#,
            r#""name": "documents", "futureMetadata": { "sentinel-secret": true },"#,
        );
        let items = parse_codex_plugins(text.as_bytes()).expect("an additive field is ignored");
        let installed = item(&items, "codex:official:documents");

        assert_eq!(installed.plugin_id, "documents");
        let serialized = serde_json::to_string(&items).unwrap();
        assert!(!serialized.contains("futureMetadata"), "{serialized}");
        assert!(!serialized.contains("sentinel-secret"), "{serialized}");
    }

    #[test]
    fn parse_codex_drops_local_paths() {
        let items = parse_codex_plugins(CODEX_PLUGINS.as_bytes()).expect("valid Codex JSON");
        let serialized = serde_json::to_string(&items).unwrap();
        assert!(!serialized.contains("/Users/"), "{serialized}");
        assert!(!serialized.contains("installPolicy"), "{serialized}");
    }

    #[test]
    fn parse_codex_invalid_json_is_rejected() {
        assert!(parse_codex_plugins(b"{\"installed\": [").is_err());
        assert!(parse_codex_plugins(b"[]").is_err());
        assert!(parse_codex_marketplaces(b"not json").is_err());
    }

    #[test]
    fn parse_codex_marketplaces_keeps_names_without_paths() {
        let markets =
            parse_codex_marketplaces(CODEX_MARKETPLACES.as_bytes()).expect("valid marketplaces");

        assert_eq!(
            markets
                .iter()
                .map(|market| (market.name.as_str(), market.source.as_deref()))
                .collect::<Vec<_>>(),
            vec![("official", Some("local")), ("team", Some("git"))]
        );
        let serialized = serde_json::to_string(&markets).unwrap();
        assert!(!serialized.contains("/Users/"), "{serialized}");
    }

    #[test]
    fn parse_claude_reads_installed_records() {
        let items = parse_claude_plugins(CLAUDE_PLUGINS.as_bytes()).expect("valid Claude JSON");
        let installed = item(&items, "claude_code:official:reviewer");

        assert_eq!(installed.agent, PluginAgent::ClaudeCode);
        assert_eq!(installed.plugin_id, "reviewer");
        assert_eq!(installed.marketplace.as_deref(), Some("official"));
        assert_eq!(installed.installed, PluginPresence::Installed);
        assert_eq!(installed.installed_version.as_deref(), Some("1.0"));
        assert_eq!(installed.scope, PluginScope::User);
        assert_eq!(installed.enabled, PluginEnablement::Disabled);
    }

    #[test]
    fn parse_claude_unrecognized_scope_stays_unknown() {
        let items = parse_claude_plugins(CLAUDE_PLUGINS.as_bytes()).expect("valid Claude JSON");
        assert_eq!(
            item(&items, "claude_code:official:surveyor").scope,
            PluginScope::Unknown
        );
    }

    #[test]
    fn parse_claude_available_without_version_stays_unknown() {
        let items = parse_claude_plugins(CLAUDE_PLUGINS.as_bytes()).expect("valid Claude JSON");
        let planner = item(&items, "claude_code:official:planner");

        assert_eq!(planner.available, PluginAvailability::Available);
        assert_eq!(planner.available_version, None);
        // The available feed states nothing about installation.
        assert_eq!(planner.installed, PluginPresence::Unknown);
        assert_eq!(planner.enabled, PluginEnablement::Unknown);
    }

    #[test]
    fn parse_claude_drops_local_paths() {
        let items = parse_claude_plugins(CLAUDE_PLUGINS.as_bytes()).expect("valid Claude JSON");
        let serialized = serde_json::to_string(&items).unwrap();
        assert!(!serialized.contains("/Users/"), "{serialized}");
        assert!(!serialized.contains("installedAt"), "{serialized}");
    }

    #[test]
    fn parse_claude_marketplaces_keeps_names_without_paths() {
        let markets =
            parse_claude_marketplaces(CLAUDE_MARKETPLACES.as_bytes()).expect("valid marketplaces");

        assert_eq!(markets.len(), 1);
        assert_eq!(markets[0].name, "official");
        assert_eq!(markets[0].source.as_deref(), Some("github"));
        let serialized = serde_json::to_string(&markets).unwrap();
        assert!(!serialized.contains("/Users/"), "{serialized}");
        assert!(!serialized.contains("example/official"), "{serialized}");
    }

    #[test]
    fn parse_claude_invalid_json_is_rejected() {
        assert!(parse_claude_plugins(b"[]").is_err());
        assert!(parse_claude_marketplaces(b"{}").is_err());
    }

    // ── Identity, merge and ordering ──

    #[test]
    fn normalize_same_plugin_id_in_two_marketplaces_stays_distinct() {
        let items = normalize(parse_codex_plugins(CODEX_PLUGINS.as_bytes()).unwrap());
        let reviewers: Vec<&PluginInventoryItemDto> = items
            .iter()
            .filter(|item| item.plugin_id == "reviewer")
            .collect();

        assert_eq!(reviewers.len(), 2);
        assert_eq!(
            reviewers.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(),
            vec!["codex:official:reviewer", "codex:team:reviewer"]
        );
        assert_eq!(reviewers[0].available_version, None);
        assert_eq!(
            reviewers[1].available_version.as_deref(),
            Some("release-2026.08+vendor")
        );
    }

    #[test]
    fn normalize_installed_and_available_merge_within_one_key() {
        let items = normalize(parse_claude_plugins(CLAUDE_PLUGINS.as_bytes()).unwrap());
        let merged = item(&items, "claude_code:official:reviewer");

        assert_eq!(merged.installed, PluginPresence::Installed);
        assert_eq!(merged.installed_version.as_deref(), Some("1.0"));
        assert_eq!(merged.available_version.as_deref(), Some("1.1"));
        assert_eq!(merged.available, PluginAvailability::Available);
        // Neither CLI reports an update verdict, and 1.0 vs 1.1 is not compared.
        assert_eq!(merged.update, PluginUpdateState::Unknown);
        assert_eq!(
            items.iter().filter(|i| i.plugin_id == "reviewer").count(),
            1
        );
    }

    #[test]
    fn normalize_keeps_equal_plugin_ids_from_different_agents_apart() {
        let mut items = parse_codex_plugins(CODEX_PLUGINS.as_bytes()).unwrap();
        items.extend(parse_claude_plugins(CLAUDE_PLUGINS.as_bytes()).unwrap());
        let items = normalize(items);

        let reviewers: Vec<&str> = items
            .iter()
            .filter(|item| item.plugin_id == "reviewer")
            .map(|item| item.id.as_str())
            .collect();
        assert_eq!(
            reviewers,
            vec![
                "claude_code:official:reviewer",
                "codex:official:reviewer",
                "codex:team:reviewer"
            ]
        );
    }

    #[test]
    fn normalize_ordering_is_stable_across_shuffled_arrays() {
        let forward = normalize(parse_claude_plugins(CLAUDE_PLUGINS.as_bytes()).unwrap());

        let mut shuffled = parse_claude_plugins(CLAUDE_PLUGINS.as_bytes()).unwrap();
        shuffled.reverse();
        let shuffled = normalize(shuffled);

        assert_eq!(ids(&forward), ids(&shuffled));
        assert_eq!(forward, shuffled);
        assert_eq!(
            ids(&forward),
            vec![
                "claude_code:official:planner",
                "claude_code:official:reviewer",
                "claude_code:official:surveyor"
            ]
        );
    }

    #[test]
    fn normalize_never_promotes_unknown_to_a_concrete_state() {
        let items = normalize(parse_claude_plugins(CLAUDE_PLUGINS.as_bytes()).unwrap());
        let planner = item(&items, "claude_code:official:planner");

        assert_eq!(planner.installed, PluginPresence::Unknown);
        assert_eq!(planner.scope, PluginScope::Unknown);
        assert_eq!(planner.enabled, PluginEnablement::Unknown);
        assert_eq!(planner.update, PluginUpdateState::Unknown);
        assert_eq!(planner.installed_version, None);
    }

    #[test]
    fn normalize_serializes_status_fields_as_closed_enums() {
        let items = normalize(parse_claude_plugins(CLAUDE_PLUGINS.as_bytes()).unwrap());
        let value = serde_json::to_value(item(&items, "claude_code:official:planner")).unwrap();

        assert_eq!(value["installed"], "unknown");
        assert_eq!(value["available"], "available");
        assert_eq!(value["scope"], "unknown");
        assert_eq!(value["enabled"], "unknown");
        assert_eq!(value["update"], "unknown");
        assert_eq!(value["installedVersion"], serde_json::Value::Null);
        assert_eq!(value["marketplace"], "official");
    }

    // ── Agent-isolated aggregation ──

    const GENERATED_AT: &str = "2026-08-13T00:00:00Z";

    fn ok(agent: PluginAgent, capability: PluginCapability, body: &str) -> CommandOutput {
        CommandOutput {
            agent,
            capability,
            result: Ok(body.as_bytes().to_vec()),
        }
    }

    fn failed(
        agent: PluginAgent,
        capability: PluginCapability,
        code: PluginDiagnosticCode,
        exit_status: Option<i32>,
    ) -> CommandOutput {
        CommandOutput {
            agent,
            capability,
            result: Err(RunFailure { code, exit_status }),
        }
    }

    /// Every command of one Agent succeeding, so a test only has to state the
    /// one outcome it is about.
    fn healthy(agent: PluginAgent) -> Vec<CommandOutput> {
        let (version, plugins, markets) = match agent {
            PluginAgent::Codex => ("codex-cli 0.144.5", CODEX_PLUGINS, CODEX_MARKETPLACES),
            PluginAgent::ClaudeCode => ("2.1.229 (Claude Code)", CLAUDE_PLUGINS, CLAUDE_MARKETPLACES),
        };
        vec![
            ok(agent, PluginCapability::Version, version),
            ok(agent, PluginCapability::PluginList, plugins),
            ok(agent, PluginCapability::MarketplaceList, markets),
        ]
    }

    fn agent_result(dto: &PluginInventoryDto, agent: PluginAgent) -> &PluginAgentInventoryDto {
        dto.agents
            .iter()
            .find(|result| result.agent == agent)
            .expect("both Agents are always reported")
    }

    fn codes(result: &PluginAgentInventoryDto) -> Vec<(String, String)> {
        result
            .diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    diagnostic.capability.as_str().to_string(),
                    diagnostic.code.as_str().to_string(),
                )
            })
            .collect()
    }

    #[test]
    fn aggregate_reports_both_agents_in_a_fixed_order() {
        let mut outputs = healthy(PluginAgent::ClaudeCode);
        outputs.extend(healthy(PluginAgent::Codex));
        let dto = assemble(outputs, GENERATED_AT.to_string());

        assert_eq!(
            dto.agents.iter().map(|a| a.agent).collect::<Vec<_>>(),
            vec![PluginAgent::Codex, PluginAgent::ClaudeCode]
        );
        assert_eq!(dto.generated_at, GENERATED_AT);
    }

    #[test]
    fn aggregate_records_only_the_capabilities_that_returned_data() {
        let mut outputs = healthy(PluginAgent::Codex);
        outputs.extend(healthy(PluginAgent::ClaudeCode));
        let dto = assemble(outputs, GENERATED_AT.to_string());
        let codex = agent_result(&dto, PluginAgent::Codex);

        assert_eq!(
            codex.capabilities,
            vec![
                PluginCapability::Version,
                PluginCapability::PluginList,
                PluginCapability::MarketplaceList
            ]
        );
        assert_eq!(codex.cli_version.as_deref(), Some("codex-cli 0.144.5"));
        assert_eq!(
            codex.marketplaces.iter().map(|m| m.name.as_str()).collect::<Vec<_>>(),
            vec!["official", "team"]
        );
        assert!(codex.diagnostics.is_empty());
        assert_eq!(codex.items.len(), 3);
    }

    #[test]
    fn aggregate_missing_codex_cli_keeps_claude_inventory() {
        let mut outputs = vec![
            failed(
                PluginAgent::Codex,
                PluginCapability::Version,
                PluginDiagnosticCode::CliMissing,
                None,
            ),
            failed(
                PluginAgent::Codex,
                PluginCapability::PluginList,
                PluginDiagnosticCode::CliMissing,
                None,
            ),
            failed(
                PluginAgent::Codex,
                PluginCapability::MarketplaceList,
                PluginDiagnosticCode::CliMissing,
                None,
            ),
        ];
        outputs.extend(healthy(PluginAgent::ClaudeCode));
        let dto = assemble(outputs, GENERATED_AT.to_string());

        let codex = agent_result(&dto, PluginAgent::Codex);
        assert!(codex.items.is_empty());
        assert!(codex.capabilities.is_empty());
        assert_eq!(codex.cli_version, None);
        assert!(codex
            .diagnostics
            .iter()
            .all(|d| d.code == PluginDiagnosticCode::CliMissing));

        let claude = agent_result(&dto, PluginAgent::ClaudeCode);
        assert!(claude.diagnostics.is_empty());
        assert_eq!(claude.items.len(), 3);
    }

    #[test]
    fn aggregate_marketplace_failure_keeps_installed_items() {
        let mut outputs = vec![
            ok(
                PluginAgent::ClaudeCode,
                PluginCapability::Version,
                "2.1.229 (Claude Code)",
            ),
            ok(
                PluginAgent::ClaudeCode,
                PluginCapability::PluginList,
                CLAUDE_PLUGINS,
            ),
            failed(
                PluginAgent::ClaudeCode,
                PluginCapability::MarketplaceList,
                PluginDiagnosticCode::NonZeroExit,
                Some(1),
            ),
        ];
        outputs.extend(healthy(PluginAgent::Codex));
        let dto = assemble(outputs, GENERATED_AT.to_string());

        let claude = agent_result(&dto, PluginAgent::ClaudeCode);
        assert!(claude
            .items
            .iter()
            .any(|item| item.installed == PluginPresence::Installed));
        assert_eq!(
            codes(claude),
            vec![(
                "marketplace_list".to_string(),
                "marketplace_unavailable".to_string()
            )]
        );
        assert_eq!(
            claude.capabilities,
            vec![PluginCapability::Version, PluginCapability::PluginList]
        );
        assert!(claude.marketplaces.is_empty());

        assert!(agent_result(&dto, PluginAgent::Codex).diagnostics.is_empty());
    }

    #[test]
    fn aggregate_invalid_codex_json_does_not_suppress_claude() {
        let mut outputs = vec![
            ok(PluginAgent::Codex, PluginCapability::Version, "codex-cli 0.144.5"),
            ok(
                PluginAgent::Codex,
                PluginCapability::PluginList,
                "{\"installed\": [",
            ),
            ok(
                PluginAgent::Codex,
                PluginCapability::MarketplaceList,
                CODEX_MARKETPLACES,
            ),
        ];
        outputs.extend(healthy(PluginAgent::ClaudeCode));
        let dto = assemble(outputs, GENERATED_AT.to_string());

        let codex = agent_result(&dto, PluginAgent::Codex);
        assert!(codex.items.is_empty());
        assert_eq!(
            codes(codex),
            vec![("plugin_list".to_string(), "invalid_json".to_string())]
        );
        // The marketplace capability of the same Agent survives.
        assert_eq!(codex.marketplaces.len(), 2);
        assert_eq!(
            codex.capabilities,
            vec![PluginCapability::Version, PluginCapability::MarketplaceList]
        );

        let claude = agent_result(&dto, PluginAgent::ClaudeCode);
        assert_eq!(
            claude
                .items
                .iter()
                .filter(|item| item.installed == PluginPresence::Installed)
                .count(),
            2
        );
    }

    #[test]
    fn aggregate_maps_each_failure_to_its_fixed_code() {
        let outputs = vec![
            failed(
                PluginAgent::Codex,
                PluginCapability::Version,
                PluginDiagnosticCode::NonZeroExit,
                Some(2),
            ),
            failed(
                PluginAgent::Codex,
                PluginCapability::PluginList,
                PluginDiagnosticCode::NonZeroExit,
                Some(23),
            ),
            failed(
                PluginAgent::Codex,
                PluginCapability::MarketplaceList,
                PluginDiagnosticCode::Timeout,
                None,
            ),
            failed(
                PluginAgent::ClaudeCode,
                PluginCapability::Version,
                PluginDiagnosticCode::Timeout,
                None,
            ),
            failed(
                PluginAgent::ClaudeCode,
                PluginCapability::PluginList,
                PluginDiagnosticCode::OutputTooLarge,
                None,
            ),
            failed(
                PluginAgent::ClaudeCode,
                PluginCapability::MarketplaceList,
                PluginDiagnosticCode::OutputTooLarge,
                None,
            ),
        ];
        let dto = assemble(outputs, GENERATED_AT.to_string());

        assert_eq!(
            codes(agent_result(&dto, PluginAgent::Codex)),
            vec![
                ("version".to_string(), "unsupported_cli".to_string()),
                ("plugin_list".to_string(), "non_zero_exit".to_string()),
                ("marketplace_list".to_string(), "timeout".to_string()),
            ]
        );
        assert_eq!(
            codes(agent_result(&dto, PluginAgent::ClaudeCode)),
            vec![
                ("version".to_string(), "timeout".to_string()),
                ("plugin_list".to_string(), "output_too_large".to_string()),
                ("marketplace_list".to_string(), "output_too_large".to_string()),
            ]
        );

        let exits: Vec<Option<i32>> = agent_result(&dto, PluginAgent::Codex)
            .diagnostics
            .iter()
            .map(|d| d.exit_status)
            .collect();
        assert_eq!(exits, vec![Some(2), Some(23), None]);
    }

    #[test]
    fn aggregate_unusable_version_output_is_unsupported() {
        let outputs = vec![
            ok(
                PluginAgent::Codex,
                PluginCapability::Version,
                &"v".repeat(MAX_VERSION_CHARS + 1),
            ),
            ok(PluginAgent::Codex, PluginCapability::PluginList, CODEX_PLUGINS),
            ok(
                PluginAgent::Codex,
                PluginCapability::MarketplaceList,
                CODEX_MARKETPLACES,
            ),
        ];
        let dto = assemble(outputs, GENERATED_AT.to_string());
        let codex = agent_result(&dto, PluginAgent::Codex);

        assert_eq!(codex.cli_version, None);
        assert_eq!(
            codes(codex),
            vec![("version".to_string(), "unsupported_cli".to_string())]
        );
        // A version AgentDeck cannot read never removes the inventory it can.
        assert_eq!(codex.items.len(), 3);
    }

    #[test]
    fn aggregate_response_never_carries_captured_output() {
        let secret = format!(
            r#"{{"installed":[{{"pluginId":"reviewer@official","name":"reviewer","marketplaceName":"official","version":"1.0","installed":true,"enabled":true,"token":"sentinel-secret"}}],"available":[]}}"#
        );
        let outputs = vec![
            ok(PluginAgent::Codex, PluginCapability::Version, "codex-cli 0.144.5"),
            ok(PluginAgent::Codex, PluginCapability::PluginList, &secret),
            failed(
                PluginAgent::Codex,
                PluginCapability::MarketplaceList,
                PluginDiagnosticCode::NonZeroExit,
                Some(23),
            ),
        ];
        let dto = assemble(outputs, GENERATED_AT.to_string());
        let serialized = serde_json::to_string(&dto).unwrap();

        assert!(!serialized.contains("sentinel-secret"), "{serialized}");
        assert!(!serialized.contains("token"), "{serialized}");
        assert!(serialized.contains("reviewer"));
    }

    #[test]
    fn aggregate_serializes_a_fixed_response_shape() {
        let dto = assemble(healthy(PluginAgent::Codex), GENERATED_AT.to_string());
        let value = serde_json::to_value(&dto).unwrap();

        assert_eq!(value["generatedAt"], GENERATED_AT);
        let codex = &value["agents"][0];
        assert_eq!(codex["agent"], "codex");
        assert_eq!(codex["cliVersion"], "codex-cli 0.144.5");
        assert_eq!(codex["capabilities"][0], "version");
        assert_eq!(codex["marketplaces"][0]["name"], "official");
        assert_eq!(codex["items"][0]["id"], "codex:official:documents");
        assert_eq!(codex["diagnostics"], serde_json::json!([]));
        assert_eq!(value["agents"][1]["agent"], "claude_code");
    }

    #[test]
    fn aggregate_version_failure_propagates_to_all_capabilities() {
        let outputs = vec![
            failed(
                PluginAgent::Codex,
                PluginCapability::Version,
                PluginDiagnosticCode::NonZeroExit,
                Some(1),
            ),
            failed(
                PluginAgent::Codex,
                PluginCapability::PluginList,
                PluginDiagnosticCode::UnsupportedCli,
                None,
            ),
            failed(
                PluginAgent::Codex,
                PluginCapability::MarketplaceList,
                PluginDiagnosticCode::UnsupportedCli,
                None,
            ),
        ];
        let dto = assemble(outputs, GENERATED_AT.to_string());
        let codex = agent_result(&dto, PluginAgent::Codex);

        assert_eq!(
            codes(codex),
            vec![
                ("version".to_string(), "unsupported_cli".to_string()),
                ("plugin_list".to_string(), "unsupported_cli".to_string()),
                ("marketplace_list".to_string(), "unsupported_cli".to_string()),
            ]
        );
        assert!(codex.items.is_empty());
        assert!(codex.marketplaces.is_empty());
        assert!(codex.capabilities.is_empty());
        assert_eq!(codex.cli_version, None);
    }

    #[tokio::test]
    async fn collect_after_version_rejects_an_unusable_version_before_inventory() {
        let outputs = collect_after_version(
            PluginAgent::Codex,
            Ok("v".repeat(MAX_VERSION_CHARS + 1).into_bytes()),
        )
        .await;

        assert_eq!(outputs.len(), 3);
        assert!(outputs[0].result.is_ok());
        assert!(outputs[1..].iter().all(|output| {
            matches!(
                output.result,
                Err(RunFailure {
                    code: PluginDiagnosticCode::UnsupportedCli,
                    exit_status: None,
                })
            )
        }));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn runner_accepts_the_exact_stderr_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("stderr_payload");
        let pidfile = dir.path().join("pid");
        payload_of_exact_size(&file, MAX_STREAM_BYTES);

        let stdout = run_bounded(
            fake(
                "/bin/sh",
                &[
                    "-c",
                    "echo $$ > \"$1\"; cat \"$0\" >&2; printf '{}'",
                    file.to_str().unwrap(),
                    pidfile.to_str().unwrap(),
                ],
            ),
            TEST_TIMEOUT,
        )
        .await
        .expect("stderr at the exact limit is accepted");

        assert_eq!(stdout, b"{}");

        let pid: u32 = std::fs::read_to_string(&pidfile)
            .expect("the child recorded its pid")
            .trim()
            .parse()
            .expect("a numeric pid");
        assert!(
            !is_alive_or_zombie(pid),
            "the child was not reaped after a normal exit"
        );
    }

    #[test]
    fn aggregate_command_module_takes_no_store_and_writes_nothing() {
        // The command that exposes this inventory must stay a pure read: the
        // whole change owes its safety to there being no persistence call to
        // review in the first place.
        let source = include_str!("../commands/plugins.rs");
        for symbol in [
            "SkillStore",
            "central_repo",
            "library",
            "Library",
            "rusqlite",
            "fs::write",
            "fs::create_dir",
            "log::info",
            "log::warn",
            "log::error",
            "println!",
        ] {
            assert!(
                !source.contains(symbol),
                "commands/plugins.rs must not reference {symbol}"
            );
        }
        assert!(source.contains("pub async fn get_plugin_inventory"));
    }
}
