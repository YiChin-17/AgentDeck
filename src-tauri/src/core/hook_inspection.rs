//! Read-only Hook configuration inspection for Codex and Claude Code.
//!
//! This module only reads fixed source locations and never writes, executes or
//! persists anything. See `openspec/changes/inspect-codex-claude-hooks`.

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::{Map as JsonMap, Value as JsonValue};
use toml_edit::{Decor, DocumentMut, Item as TomlItem, Value as TomlValue};

use crate::core::log_sanitize;

/// A config file above this size is never read into memory.
pub const MAX_SOURCE_BYTES: u64 = 1_048_576;
/// A canonical Hook fragment above this size never reaches the O(n²) line diff.
pub const MAX_DIFF_BYTES: usize = 262_144;
/// Same guard, expressed in lines.
pub const MAX_DIFF_LINES: usize = 4_000;
/// The documentation snapshot the compatibility registry is pinned to.
pub const COMPATIBILITY_SNAPSHOT_DATE: &str = "2026-08-12";

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookAgent {
    Codex,
    ClaudeCode,
}

impl HookAgent {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookAgent::Codex => "codex",
            HookAgent::ClaudeCode => "claude_code",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookScope {
    User,
    Project,
    ProjectLocal,
}

impl HookScope {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookScope::User => "user",
            HookScope::Project => "project",
            HookScope::ProjectLocal => "project_local",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookFormat {
    Json,
    Toml,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookSourceStatus {
    Missing,
    Valid,
    Invalid,
    TooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookDiagnosticKind {
    /// The file exists but could not be opened — permissions, or not a file.
    NotReadable,
    InvalidEncoding,
    InvalidSyntax,
    /// Parsed, but the Hook subtree is not the documented three-layer shape.
    InvalidShape,
    TooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookSupport {
    Supported,
    Unsupported,
    Unknown,
}

impl HookSupport {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookSupport::Supported => "supported",
            HookSupport::Unsupported => "unsupported",
            HookSupport::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CompatibilityCategory {
    Event,
    Handler,
}

/// Failure of the command as a whole, as opposed to a per-source diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookInspectionError {
    InvalidProject,
}

impl HookInspectionError {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookInspectionError::InvalidProject => "invalid_project",
        }
    }
}

// ---------------------------------------------------------------------------
// Fixed source descriptors
// ---------------------------------------------------------------------------

/// One inspectable config file. The frontend only ever sees `id`, so it cannot
/// ask for an arbitrary path.
#[derive(Debug, Clone)]
pub struct HookSourceDescriptor {
    pub id: &'static str,
    pub agent: HookAgent,
    pub scope: HookScope,
    pub format: HookFormat,
    pub path: PathBuf,
}

fn descriptor(
    id: &'static str,
    agent: HookAgent,
    scope: HookScope,
    format: HookFormat,
    path: PathBuf,
) -> HookSourceDescriptor {
    HookSourceDescriptor {
        id,
        agent,
        scope,
        format,
        path,
    }
}

/// The complete set of locations this change reads, in display order.
///
/// Nothing is discovered by scanning: a recursive walk of home or of a project
/// would widen the read surface to managed policy, plugin bundles and component
/// files that are all out of scope here.
pub fn source_descriptors(home: &Path, project_root: Option<&Path>) -> Vec<HookSourceDescriptor> {
    let mut sources = vec![
        descriptor(
            "codex:user:hooks-json",
            HookAgent::Codex,
            HookScope::User,
            HookFormat::Json,
            home.join(".codex").join("hooks.json"),
        ),
        descriptor(
            "codex:user:config-toml",
            HookAgent::Codex,
            HookScope::User,
            HookFormat::Toml,
            home.join(".codex").join("config.toml"),
        ),
        descriptor(
            "claude_code:user:settings-json",
            HookAgent::ClaudeCode,
            HookScope::User,
            HookFormat::Json,
            home.join(".claude").join("settings.json"),
        ),
    ];

    if let Some(root) = project_root {
        sources.extend([
            descriptor(
                "codex:project:hooks-json",
                HookAgent::Codex,
                HookScope::Project,
                HookFormat::Json,
                root.join(".codex").join("hooks.json"),
            ),
            descriptor(
                "codex:project:config-toml",
                HookAgent::Codex,
                HookScope::Project,
                HookFormat::Toml,
                root.join(".codex").join("config.toml"),
            ),
            descriptor(
                "claude_code:project:settings-json",
                HookAgent::ClaudeCode,
                HookScope::Project,
                HookFormat::Json,
                root.join(".claude").join("settings.json"),
            ),
            descriptor(
                "claude_code:project_local:settings-local-json",
                HookAgent::ClaudeCode,
                HookScope::ProjectLocal,
                HookFormat::Json,
                root.join(".claude").join("settings.local.json"),
            ),
        ]);
    }

    sources
}

/// Turns an optional Project id into a project root through `lookup`.
///
/// An id that does not resolve is an error rather than an empty result: falling
/// back to the process working directory would read Hook files the user never
/// linked.
pub fn resolve_project_root(
    project_id: Option<&str>,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<Option<PathBuf>, HookInspectionError> {
    match project_id {
        None => Ok(None),
        Some(id) => lookup(id)
            .map(|root| Some(PathBuf::from(root)))
            .ok_or(HookInspectionError::InvalidProject),
    }
}

// ---------------------------------------------------------------------------
// Response shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookDiagnosticDto {
    pub source_id: String,
    pub kind: HookDiagnosticKind,
    /// Parser message only — never the file's text.
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookSourceDto {
    pub id: String,
    pub agent: HookAgent,
    pub scope: HookScope,
    pub format: HookFormat,
    pub display_path: String,
    pub status: HookSourceStatus,
    pub diagnostic: Option<HookDiagnosticDto>,
    pub entry_count: usize,
    /// Deterministic text of the Hook subtree alone.
    pub canonical_text: String,
    pub diff_available: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HookFieldDto {
    pub key: String,
    pub value: String,
    /// False when the field is absent from this Agent's documented snapshot.
    pub known: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookEntryDto {
    /// Stable within one response; deliberately not a database identity.
    pub id: String,
    pub source_id: String,
    pub agent: HookAgent,
    pub scope: HookScope,
    pub event: String,
    pub event_known: bool,
    pub matcher: Option<String>,
    pub group_index: usize,
    pub handler_index: usize,
    pub handler_type: String,
    pub handler_type_known: bool,
    pub fields: Vec<HookFieldDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompatibilityCellDto {
    pub support: HookSupport,
    /// Stable note code the frontend localizes; never free-form prose.
    pub note: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompatibilityRowDto {
    pub category: CompatibilityCategory,
    pub name: &'static str,
    pub codex: CompatibilityCellDto,
    pub claude_code: CompatibilityCellDto,
}

/// The value shape a documented field name implies.
///
// SIMPLE: the pinned snapshot records field names, not value types, so only
// shapes that the field name makes unambiguous are pinned; upgrade to a typed
// registry when the compatibility snapshot carries types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookFieldShape {
    Text,
    Integer,
    Bool,
    TextList,
    Table,
    /// Documented, but the snapshot does not pin one shape.
    Any,
}

pub fn field_shape(name: &str) -> HookFieldShape {
    match name {
        "command" | "commandWindows" | "statusMessage" | "url" | "prompt" | "model" | "server"
        | "tool" | "if" => HookFieldShape::Text,
        "timeout" | "additionalContextLimit" => HookFieldShape::Integer,
        "async" | "asyncRewake" | "once" => HookFieldShape::Bool,
        "args" | "allowedEnvVars" => HookFieldShape::TextList,
        "headers" | "input" => HookFieldShape::Table,
        _ => HookFieldShape::Any,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HookFieldDescriptorDto {
    pub name: &'static str,
    pub shape: HookFieldShape,
}

/// What one Agent's pinned snapshot allows, so the editor can build its form
/// from the same registry the backend validates against.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookAgentRegistryDto {
    pub events: &'static [&'static str],
    pub handler_types: &'static [&'static str],
    pub fields: Vec<HookFieldDescriptorDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookRegistryDto {
    pub codex: HookAgentRegistryDto,
    pub claude_code: HookAgentRegistryDto,
}

fn agent_registry(agent: HookAgent) -> HookAgentRegistryDto {
    HookAgentRegistryDto {
        events: known_events(agent),
        handler_types: known_handlers(agent),
        fields: known_fields(agent)
            .iter()
            .map(|name| HookFieldDescriptorDto {
                name,
                shape: field_shape(name),
            })
            .collect(),
    }
}

pub fn registry() -> HookRegistryDto {
    HookRegistryDto {
        codex: agent_registry(HookAgent::Codex),
        claude_code: agent_registry(HookAgent::ClaudeCode),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookInspectionDto {
    pub sources: Vec<HookSourceDto>,
    pub entries: Vec<HookEntryDto>,
    pub compatibility: Vec<CompatibilityRowDto>,
    /// What the editor may offer, taken from the same pinned snapshot the write
    /// path validates against.
    pub registry: HookRegistryDto,
    pub selected_project_id: Option<String>,
    pub snapshot_date: &'static str,
    pub generated_at: i64,
}

// ---------------------------------------------------------------------------
// Compatibility registry — 2026-08-12 documentation snapshot
// ---------------------------------------------------------------------------

/// Note codes. The frontend maps them to localized text; the backend never
/// ships prose.
const NOTE_SHARED: &str = "shared_name_distinct_contract";
const NOTE_ABSENT: &str = "not_in_snapshot";
const NOTE_SKIPPED: &str = "parsed_but_skipped";

/// Codex lifecycle events, in the order the snapshot documents them.
const CODEX_EVENTS: &[&str] = &[
    "SessionStart",
    "SessionEnd",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "PreCompact",
    "PostCompact",
    "UserPromptSubmit",
    "SubagentStart",
    "SubagentStop",
    "Stop",
];

/// Claude Code hook events, in documented order. This list is the superset in
/// the 2026-08-12 snapshot, so it also fixes the matrix row order.
const CLAUDE_EVENTS: &[&str] = &[
    "SessionStart",
    "Setup",
    "UserPromptSubmit",
    "UserPromptExpansion",
    "PreToolUse",
    "PermissionRequest",
    "PermissionDenied",
    "PostToolUse",
    "PostToolUseFailure",
    "PostToolBatch",
    "Notification",
    "MessageDisplay",
    "SubagentStart",
    "SubagentStop",
    "TaskCreated",
    "TaskCompleted",
    "Stop",
    "StopFailure",
    "TeammateIdle",
    "InstructionsLoaded",
    "ConfigChange",
    "CwdChanged",
    "DirectoryAdded",
    "FileChanged",
    "WorktreeCreate",
    "WorktreeRemove",
    "PreCompact",
    "PostCompact",
    "Elicitation",
    "ElicitationResult",
    "SessionEnd",
];

const CODEX_HANDLERS: &[&str] = &["command"];
/// Documented as accepted by the parser but not executed.
const CODEX_SKIPPED_HANDLERS: &[&str] = &["prompt", "agent"];
const CLAUDE_HANDLERS: &[&str] = &["command", "http", "mcp_tool", "prompt", "agent"];

/// Handler fields the snapshot documents, unioned per Agent rather than per
/// handler type: an unknown marker should mean "not in this Agent's snapshot",
/// not "valid field on the wrong handler".
const CODEX_FIELDS: &[&str] = &[
    "command",
    "commandWindows",
    "timeout",
    "statusMessage",
    "additionalContextLimit",
    "async",
];
const CLAUDE_FIELDS: &[&str] = &[
    "if",
    "timeout",
    "statusMessage",
    "once",
    "command",
    "args",
    "async",
    "asyncRewake",
    "shell",
    "url",
    "headers",
    "allowedEnvVars",
    "server",
    "tool",
    "input",
    "prompt",
    "model",
];

fn cell(support: HookSupport, note: Option<&'static str>) -> CompatibilityCellDto {
    CompatibilityCellDto { support, note }
}

/// The matrix. Support states come from the pinned snapshot only — a value seen
/// during discovery never edits this table.
pub fn compatibility_registry() -> Vec<CompatibilityRowDto> {
    let mut rows = Vec::with_capacity(CLAUDE_EVENTS.len() + CLAUDE_HANDLERS.len());

    let mut event_names: Vec<&'static str> = CLAUDE_EVENTS.to_vec();
    for name in CODEX_EVENTS {
        if !event_names.contains(name) {
            event_names.push(name);
        }
    }

    for name in event_names {
        let in_codex = CODEX_EVENTS.contains(&name);
        let in_claude = CLAUDE_EVENTS.contains(&name);
        let shared = in_codex && in_claude;
        rows.push(CompatibilityRowDto {
            category: CompatibilityCategory::Event,
            name,
            codex: if in_codex {
                cell(
                    HookSupport::Supported,
                    if shared { Some(NOTE_SHARED) } else { None },
                )
            } else {
                cell(HookSupport::Unsupported, Some(NOTE_ABSENT))
            },
            claude_code: if in_claude {
                cell(
                    HookSupport::Supported,
                    if shared { Some(NOTE_SHARED) } else { None },
                )
            } else {
                cell(HookSupport::Unsupported, Some(NOTE_ABSENT))
            },
        });
    }

    for name in CLAUDE_HANDLERS {
        let in_codex = CODEX_HANDLERS.contains(name);
        let skipped = CODEX_SKIPPED_HANDLERS.contains(name);
        rows.push(CompatibilityRowDto {
            category: CompatibilityCategory::Handler,
            name,
            codex: if in_codex {
                cell(HookSupport::Supported, Some(NOTE_SHARED))
            } else if skipped {
                cell(HookSupport::Unsupported, Some(NOTE_SKIPPED))
            } else {
                cell(HookSupport::Unsupported, Some(NOTE_ABSENT))
            },
            claude_code: cell(
                HookSupport::Supported,
                if in_codex { Some(NOTE_SHARED) } else { None },
            ),
        });
    }

    rows
}

/// The events this Agent's pinned snapshot documents.
pub fn known_events(agent: HookAgent) -> &'static [&'static str] {
    match agent {
        HookAgent::Codex => CODEX_EVENTS,
        HookAgent::ClaudeCode => CLAUDE_EVENTS,
    }
}

/// The handler types this Agent's pinned snapshot documents as executed.
pub fn known_handlers(agent: HookAgent) -> &'static [&'static str] {
    match agent {
        HookAgent::Codex => CODEX_HANDLERS,
        HookAgent::ClaudeCode => CLAUDE_HANDLERS,
    }
}

/// The handler fields this Agent's pinned snapshot documents.
pub fn known_fields(agent: HookAgent) -> &'static [&'static str] {
    match agent {
        HookAgent::Codex => CODEX_FIELDS,
        HookAgent::ClaudeCode => CLAUDE_FIELDS,
    }
}

// ---------------------------------------------------------------------------
// Reading and parsing, one source at a time
// ---------------------------------------------------------------------------

enum SourceRead {
    Missing,
    TooLarge(u64),
    NotReadable(String),
    NotUtf8,
    Text(String),
}

fn read_source(path: &Path) -> SourceRead {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => return SourceRead::Missing,
        Err(err) => return SourceRead::NotReadable(log_sanitize::sanitize(&err.to_string())),
    };
    if !metadata.is_file() {
        return SourceRead::NotReadable("source is not a regular file".to_string());
    }
    if metadata.len() > MAX_SOURCE_BYTES {
        return SourceRead::TooLarge(metadata.len());
    }
    match fs::read(path) {
        Ok(bytes) => match String::from_utf8(bytes) {
            Ok(text) => SourceRead::Text(text),
            Err(_) => SourceRead::NotUtf8,
        },
        Err(err) if err.kind() == ErrorKind::NotFound => SourceRead::Missing,
        Err(err) => SourceRead::NotReadable(log_sanitize::sanitize(&err.to_string())),
    }
}

struct ParseFailure {
    kind: HookDiagnosticKind,
    message: String,
}

/// The Hook subtree, normalized to JSON for entry extraction, plus the
/// format-native canonical text used for comparison.
pub struct HookSubtree {
    pub value: Option<JsonValue>,
    pub canonical_text: String,
}

/// Parses one source document and returns its Hook subtree.
///
/// The write path shares this so reading and writing cannot disagree about what
/// counts as a valid Hook shape. The error is a sanitized parser message, never
/// the document's own text.
pub fn parse_hook_subtree(format: HookFormat, text: &str) -> Result<HookSubtree, String> {
    match format {
        HookFormat::Json => parse_json_subtree(text),
        HookFormat::Toml => parse_toml_subtree(text),
    }
    .map_err(|failure| failure.message)
}

/// Renders a Hook subtree the same way inspection does, so a preview diff and
/// the Inspector show the same text.
pub fn canonical_json_text(hooks: &JsonValue) -> String {
    serde_json::to_string_pretty(hooks).unwrap_or_default()
}

fn parse_json_subtree(text: &str) -> Result<HookSubtree, ParseFailure> {
    let root: JsonValue = serde_json::from_str(text).map_err(|err| ParseFailure {
        kind: HookDiagnosticKind::InvalidSyntax,
        message: log_sanitize::sanitize(&err.to_string()),
    })?;
    let object = root.as_object().ok_or_else(|| ParseFailure {
        kind: HookDiagnosticKind::InvalidShape,
        message: "document root is not an object".to_string(),
    })?;

    match object.get("hooks") {
        None => Ok(HookSubtree {
            value: None,
            canonical_text: String::new(),
        }),
        Some(hooks) => {
            if !hooks.is_object() {
                return Err(ParseFailure {
                    kind: HookDiagnosticKind::InvalidShape,
                    message: "hooks is not an object of events".to_string(),
                });
            }
            let canonical_text =
                serde_json::to_string_pretty(hooks).map_err(|err| ParseFailure {
                    kind: HookDiagnosticKind::InvalidShape,
                    message: log_sanitize::sanitize(&err.to_string()),
                })?;
            Ok(HookSubtree {
                value: Some(hooks.clone()),
                canonical_text,
            })
        }
    }
}

fn parse_toml_subtree(text: &str) -> Result<HookSubtree, ParseFailure> {
    let document = text.parse::<DocumentMut>().map_err(|err| ParseFailure {
        kind: HookDiagnosticKind::InvalidSyntax,
        message: log_sanitize::sanitize(err.message()),
    })?;

    let hooks = match document.get("hooks") {
        None => {
            return Ok(HookSubtree {
                value: None,
                canonical_text: String::new(),
            })
        }
        Some(item) => item,
    };

    let value = toml_item_to_json(hooks);
    if !value.is_object() {
        return Err(ParseFailure {
            kind: HookDiagnosticKind::InvalidShape,
            message: "hooks is not a table of events".to_string(),
        });
    }

    Ok(HookSubtree {
        value: Some(value),
        canonical_text: toml_subtree_text(hooks),
    })
}

/// Renders the Hook subtree on its own, with formatting stripped so a comment
/// written above `[hooks]` — which may describe an unrelated setting — cannot
/// travel with it.
fn toml_subtree_text(item: &TomlItem) -> String {
    let mut cleaned = item.clone();
    clear_decor(&mut cleaned);
    let mut document = DocumentMut::new();
    document.as_table_mut().insert("hooks", cleaned);
    document.to_string()
}

fn blank_decor(decor: &mut Decor) {
    *decor = Decor::new("", "");
}

fn clear_decor(item: &mut TomlItem) {
    match item {
        TomlItem::None => {}
        TomlItem::Value(value) => clear_value_decor(value),
        TomlItem::Table(table) => {
            blank_decor(table.decor_mut());
            for (mut key, child) in table.iter_mut() {
                blank_decor(key.leaf_decor_mut());
                blank_decor(key.dotted_decor_mut());
                clear_decor(child);
            }
        }
        TomlItem::ArrayOfTables(tables) => {
            for table in tables.iter_mut() {
                blank_decor(table.decor_mut());
                for (mut key, child) in table.iter_mut() {
                    blank_decor(key.leaf_decor_mut());
                    blank_decor(key.dotted_decor_mut());
                    clear_decor(child);
                }
            }
        }
    }
}

fn clear_value_decor(value: &mut TomlValue) {
    blank_decor(value.decor_mut());
    match value {
        TomlValue::Array(array) => {
            for element in array.iter_mut() {
                clear_value_decor(element);
            }
        }
        TomlValue::InlineTable(table) => {
            for (mut key, element) in table.iter_mut() {
                blank_decor(key.leaf_decor_mut());
                blank_decor(key.dotted_decor_mut());
                clear_value_decor(element);
            }
        }
        _ => {}
    }
}

/// Projects a TOML node onto JSON so both formats share one comparison and
/// entry-extraction path.
pub fn toml_item_to_json(item: &TomlItem) -> JsonValue {
    match item {
        TomlItem::None => JsonValue::Null,
        TomlItem::Value(value) => toml_value_to_json(value),
        TomlItem::Table(table) => {
            let mut map = JsonMap::new();
            for (key, child) in table.iter() {
                map.insert(key.to_string(), toml_item_to_json(child));
            }
            JsonValue::Object(map)
        }
        TomlItem::ArrayOfTables(tables) => JsonValue::Array(
            tables
                .iter()
                .map(|table| {
                    let mut map = JsonMap::new();
                    for (key, child) in table.iter() {
                        map.insert(key.to_string(), toml_item_to_json(child));
                    }
                    JsonValue::Object(map)
                })
                .collect(),
        ),
    }
}

fn toml_value_to_json(value: &TomlValue) -> JsonValue {
    match value {
        TomlValue::String(v) => JsonValue::String(v.value().clone()),
        TomlValue::Integer(v) => JsonValue::from(*v.value()),
        TomlValue::Float(v) => serde_json::Number::from_f64(*v.value())
            .map(JsonValue::Number)
            .unwrap_or(JsonValue::Null),
        TomlValue::Boolean(v) => JsonValue::Bool(*v.value()),
        TomlValue::Datetime(v) => JsonValue::String(v.value().to_string()),
        TomlValue::Array(array) => {
            JsonValue::Array(array.iter().map(toml_value_to_json).collect())
        }
        TomlValue::InlineTable(table) => {
            let mut map = JsonMap::new();
            for (key, child) in table.iter() {
                map.insert(key.to_string(), toml_value_to_json(child));
            }
            JsonValue::Object(map)
        }
    }
}

/// Renders one handler field for display. Strings show as written; anything
/// else shows as compact JSON so nothing is silently dropped.
fn field_display(value: &JsonValue) -> String {
    match value {
        JsonValue::String(text) => text.clone(),
        other => other.to_string(),
    }
}

fn entries_from_subtree(
    descriptor: &HookSourceDescriptor,
    hooks: &JsonValue,
) -> Result<Vec<HookEntryDto>, ParseFailure> {
    let shape = |message: &str| ParseFailure {
        kind: HookDiagnosticKind::InvalidShape,
        message: message.to_string(),
    };

    let events = hooks
        .as_object()
        .ok_or_else(|| shape("hooks is not an object of events"))?;

    let mut names: Vec<&String> = events.keys().collect();
    names.sort();

    let mut entries = Vec::new();
    for event in names {
        let groups = events
            .get(event)
            .and_then(JsonValue::as_array)
            .ok_or_else(|| shape("an event is not an array of matcher groups"))?;

        for (group_index, group) in groups.iter().enumerate() {
            let group = group
                .as_object()
                .ok_or_else(|| shape("a matcher group is not an object"))?;
            let matcher = group
                .get("matcher")
                .and_then(JsonValue::as_str)
                .map(str::to_string);
            let handlers = group
                .get("hooks")
                .and_then(JsonValue::as_array)
                .ok_or_else(|| shape("a matcher group has no handler array"))?;

            for (handler_index, handler) in handlers.iter().enumerate() {
                let handler = handler
                    .as_object()
                    .ok_or_else(|| shape("a handler is not an object"))?;
                let handler_type = handler
                    .get("type")
                    .and_then(JsonValue::as_str)
                    .unwrap_or_default()
                    .to_string();

                let fields = handler
                    .iter()
                    .filter(|(key, _)| key.as_str() != "type")
                    .map(|(key, value)| HookFieldDto {
                        key: key.clone(),
                        value: field_display(value),
                        known: known_fields(descriptor.agent).contains(&key.as_str()),
                    })
                    .collect();

                entries.push(HookEntryDto {
                    id: format!(
                        "{}#{}#{}#{}",
                        descriptor.id, event, group_index, handler_index
                    ),
                    source_id: descriptor.id.to_string(),
                    agent: descriptor.agent,
                    scope: descriptor.scope,
                    event: event.clone(),
                    event_known: known_events(descriptor.agent).contains(&event.as_str()),
                    matcher: matcher.clone(),
                    group_index,
                    handler_index,
                    handler_type: handler_type.clone(),
                    handler_type_known: known_handlers(descriptor.agent)
                        .contains(&handler_type.as_str()),
                    fields,
                });
            }
        }
    }

    Ok(entries)
}

fn source_dto(
    descriptor: &HookSourceDescriptor,
    status: HookSourceStatus,
    diagnostic: Option<(HookDiagnosticKind, String)>,
    entry_count: usize,
    canonical_text: String,
) -> HookSourceDto {
    let diff_available = status == HookSourceStatus::Valid
        && canonical_text.len() <= MAX_DIFF_BYTES
        && canonical_text.lines().count() <= MAX_DIFF_LINES;

    HookSourceDto {
        id: descriptor.id.to_string(),
        agent: descriptor.agent,
        scope: descriptor.scope,
        format: descriptor.format,
        display_path: descriptor.path.to_string_lossy().to_string(),
        status,
        diagnostic: diagnostic.map(|(kind, message)| HookDiagnosticDto {
            source_id: descriptor.id.to_string(),
            kind,
            message,
        }),
        entry_count,
        canonical_text,
        diff_available,
    }
}

/// Inspects one source. Every failure stays inside the returned DTO so a broken
/// file cannot suppress the other sources.
pub fn inspect_source(descriptor: &HookSourceDescriptor) -> (HookSourceDto, Vec<HookEntryDto>) {
    let failed = |kind: HookDiagnosticKind, message: String| {
        (
            source_dto(
                descriptor,
                HookSourceStatus::Invalid,
                Some((kind, message)),
                0,
                String::new(),
            ),
            Vec::new(),
        )
    };

    let text = match read_source(&descriptor.path) {
        SourceRead::Missing => {
            return (
                source_dto(
                    descriptor,
                    HookSourceStatus::Missing,
                    None,
                    0,
                    String::new(),
                ),
                Vec::new(),
            )
        }
        SourceRead::TooLarge(size) => {
            return (
                source_dto(
                    descriptor,
                    HookSourceStatus::TooLarge,
                    Some((
                        HookDiagnosticKind::TooLarge,
                        format!("{size} bytes exceeds the {MAX_SOURCE_BYTES} byte read limit"),
                    )),
                    0,
                    String::new(),
                ),
                Vec::new(),
            )
        }
        SourceRead::NotReadable(message) => {
            return failed(HookDiagnosticKind::NotReadable, message)
        }
        SourceRead::NotUtf8 => {
            return failed(
                HookDiagnosticKind::InvalidEncoding,
                "file is not valid UTF-8".to_string(),
            )
        }
        SourceRead::Text(text) => text,
    };

    let subtree = match descriptor.format {
        HookFormat::Json => parse_json_subtree(&text),
        HookFormat::Toml => parse_toml_subtree(&text),
    };
    let subtree = match subtree {
        Ok(subtree) => subtree,
        Err(failure) => return failed(failure.kind, failure.message),
    };

    let entries = match &subtree.value {
        None => Vec::new(),
        Some(value) => match entries_from_subtree(descriptor, value) {
            Ok(entries) => entries,
            Err(failure) => return failed(failure.kind, failure.message),
        },
    };

    (
        source_dto(
            descriptor,
            HookSourceStatus::Valid,
            None,
            entries.len(),
            subtree.canonical_text,
        ),
        entries,
    )
}

/// Reads every fixed source and assembles the read-only response.
///
/// `home` is optional: when the home directory cannot be resolved the user
/// sources report a diagnostic, because resolving them against the process
/// working directory would inspect files the user never pointed at.
pub fn inspect(
    home: Option<&Path>,
    project_root: Option<&Path>,
    selected_project_id: Option<String>,
) -> HookInspectionDto {
    let mut sources = Vec::new();
    let mut entries = Vec::new();

    let home_base = home.unwrap_or_else(|| Path::new("~"));
    for descriptor in source_descriptors(home_base, project_root) {
        if home.is_none() && descriptor.scope == HookScope::User {
            sources.push(source_dto(
                &descriptor,
                HookSourceStatus::Invalid,
                Some((
                    HookDiagnosticKind::NotReadable,
                    "home directory could not be resolved".to_string(),
                )),
                0,
                String::new(),
            ));
            continue;
        }
        let (source, mut source_entries) = inspect_source(&descriptor);
        entries.append(&mut source_entries);
        sources.push(source);
    }

    HookInspectionDto {
        sources,
        entries,
        compatibility: compatibility_registry(),
        registry: registry(),
        selected_project_id,
        snapshot_date: COMPATIBILITY_SNAPSHOT_DATE,
        generated_at: chrono::Utc::now().timestamp(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    /// Writes a fixture file, creating parent directories as needed.
    fn write_fixture(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture dir");
        fs::write(path, content).expect("write fixture");
    }

    fn source<'a>(dto: &'a HookInspectionDto, id: &str) -> &'a HookSourceDto {
        dto.sources
            .iter()
            .find(|s| s.id == id)
            .unwrap_or_else(|| panic!("source {id} missing from response"))
    }

    fn entries_of<'a>(dto: &'a HookInspectionDto, id: &str) -> Vec<&'a HookEntryDto> {
        dto.entries.iter().filter(|e| e.source_id == id).collect()
    }

    fn field_value<'a>(entry: &'a HookEntryDto, key: &str) -> Option<&'a HookFieldDto> {
        entry.fields.iter().find(|f| f.key == key)
    }

    const CODEX_HOOKS_JSON: &str = r#"{
  "description": "workspace hooks",
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "echo pre", "statusMessage": "Checking Bash" }
        ]
      }
    ]
  }
}"#;

    const CODEX_CONFIG_TOML: &str = r#"model = "gpt-5-codex"

[[hooks.SessionStart]]
matcher = "startup|resume"

[[hooks.SessionStart.hooks]]
type = "command"
command = "echo start"
additionalContextLimit = 5000
"#;

    fn claude_settings(command: &str) -> String {
        format!(
            r#"{{
  "hooks": {{
    "PostToolUse": [
      {{
        "matcher": "Write",
        "hooks": [ {{ "type": "command", "command": "{command}" }} ]
      }}
    ]
  }}
}}"#
        )
    }

    fn ids(descriptors: &[HookSourceDescriptor]) -> Vec<&'static str> {
        descriptors.iter().map(|d| d.id).collect()
    }

    fn paths(descriptors: &[HookSourceDescriptor]) -> Vec<PathBuf> {
        descriptors.iter().map(|d| d.path.clone()).collect()
    }

    // Requirement: Hook discovery reads only fixed user and linked-project sources
    // Scenario: User sources are enumerated without a Project
    #[test]
    fn user_sources_are_enumerated_without_a_project() {
        let home = Path::new("/home/demo");
        let descriptors = source_descriptors(home, None);

        assert_eq!(
            ids(&descriptors),
            vec![
                "codex:user:hooks-json",
                "codex:user:config-toml",
                "claude_code:user:settings-json",
            ]
        );
        assert_eq!(
            paths(&descriptors),
            vec![
                home.join(".codex").join("hooks.json"),
                home.join(".codex").join("config.toml"),
                home.join(".claude").join("settings.json"),
            ]
        );
        assert!(descriptors.iter().all(|d| d.scope == HookScope::User));
        assert_eq!(descriptors[0].agent, HookAgent::Codex);
        assert_eq!(descriptors[0].format, HookFormat::Json);
        assert_eq!(descriptors[1].format, HookFormat::Toml);
        assert_eq!(descriptors[2].agent, HookAgent::ClaudeCode);
    }

    // Scenario: Linked Project adds fixed project sources
    #[test]
    fn linked_project_adds_fixed_project_sources() {
        let home = Path::new("/home/demo");
        let root = Path::new("/workspace/demo");
        let descriptors = source_descriptors(home, Some(root));

        assert_eq!(
            ids(&descriptors),
            vec![
                "codex:user:hooks-json",
                "codex:user:config-toml",
                "claude_code:user:settings-json",
                "codex:project:hooks-json",
                "codex:project:config-toml",
                "claude_code:project:settings-json",
                "claude_code:project_local:settings-local-json",
            ]
        );

        let project_paths: Vec<PathBuf> = descriptors
            .iter()
            .filter(|d| d.scope != HookScope::User)
            .map(|d| d.path.clone())
            .collect();
        assert_eq!(
            project_paths,
            vec![
                root.join(".codex").join("hooks.json"),
                root.join(".codex").join("config.toml"),
                root.join(".claude").join("settings.json"),
                root.join(".claude").join("settings.local.json"),
            ]
        );

        // No descriptor points outside the home or the linked project root.
        assert!(descriptors
            .iter()
            .all(|d| d.path.starts_with(home) || d.path.starts_with(root)));

        let local = descriptors.last().expect("project-local descriptor");
        assert_eq!(local.scope, HookScope::ProjectLocal);
        assert_eq!(local.agent, HookAgent::ClaudeCode);
    }

    // Source ids are the only handle the frontend gets; they must stay stable
    // and must never be a filesystem path.
    #[test]
    fn source_ids_are_stable_and_carry_no_path() {
        let descriptors = source_descriptors(Path::new("/home/demo"), Some(Path::new("/w/demo")));
        for descriptor in &descriptors {
            assert!(
                !descriptor.id.contains('/') && !descriptor.id.contains('\\'),
                "source id must not embed a path: {}",
                descriptor.id
            );
            let expected_prefix = format!(
                "{}:{}:",
                descriptor.agent.as_str(),
                descriptor.scope.as_str()
            );
            assert!(
                descriptor.id.starts_with(&expected_prefix),
                "{} should start with {}",
                descriptor.id,
                expected_prefix
            );
        }
        let mut sorted = ids(&descriptors);
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), descriptors.len(), "source ids must be unique");
    }

    // Scenario: Unknown Project fails closed
    #[test]
    fn unknown_project_is_rejected_without_cwd_fallback() {
        let result = resolve_project_root(Some("missing-project"), |_| None);
        assert_eq!(result, Err(HookInspectionError::InvalidProject));

        // A path-shaped id is not a path: it still has to resolve through the
        // Project lookup, so it fails closed instead of reading `/etc`.
        let escaped = resolve_project_root(Some("/etc"), |_| None);
        assert_eq!(escaped, Err(HookInspectionError::InvalidProject));
    }

    #[test]
    fn known_project_resolves_to_its_recorded_root() {
        let resolved = resolve_project_root(Some("project-1"), |id| {
            if id == "project-1" {
                Some("/workspace/demo".to_string())
            } else {
                None
            }
        });
        assert_eq!(resolved, Ok(Some(PathBuf::from("/workspace/demo"))));

        let none = resolve_project_root(None, |_| Some("/workspace/demo".to_string()));
        assert_eq!(none, Ok(None));
    }

    // Requirement: Each Hook source is parsed and diagnosed independently
    // Scenario: Codex JSON and inline TOML are both retained
    #[test]
    fn codex_json_and_inline_toml_are_both_retained() {
        let home = tempfile::tempdir().expect("home");
        write_fixture(
            &home.path().join(".codex").join("hooks.json"),
            CODEX_HOOKS_JSON,
        );
        write_fixture(
            &home.path().join(".codex").join("config.toml"),
            CODEX_CONFIG_TOML,
        );

        let dto = inspect(Some(home.path()), None, None);

        let json_source = source(&dto, "codex:user:hooks-json");
        let toml_source = source(&dto, "codex:user:config-toml");
        assert_eq!(json_source.status, HookSourceStatus::Valid);
        assert_eq!(toml_source.status, HookSourceStatus::Valid);
        assert_eq!(json_source.entry_count, 1);
        assert_eq!(toml_source.entry_count, 1);

        let json_entry = entries_of(&dto, "codex:user:hooks-json")[0];
        assert_eq!(json_entry.event, "PreToolUse");
        assert_eq!(json_entry.matcher.as_deref(), Some("Bash"));
        assert_eq!(json_entry.handler_type, "command");
        assert_eq!(json_entry.handler_index, 0);
        assert_eq!(
            field_value(json_entry, "command").map(|f| f.value.as_str()),
            Some("echo pre")
        );

        let toml_entry = entries_of(&dto, "codex:user:config-toml")[0];
        assert_eq!(toml_entry.event, "SessionStart");
        assert_eq!(toml_entry.matcher.as_deref(), Some("startup|resume"));
        assert_eq!(
            field_value(toml_entry, "command").map(|f| f.value.as_str()),
            Some("echo start")
        );
        assert_eq!(
            field_value(toml_entry, "additionalContextLimit").map(|f| f.value.as_str()),
            Some("5000")
        );

        // Neither source replaces the other: both keep their own identity.
        assert_ne!(json_entry.id, toml_entry.id);
        assert_eq!(dto.entries.len(), 2);
    }

    // Scenario: Claude Code source layers remain distinct
    #[test]
    fn claude_code_source_layers_remain_distinct() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        write_fixture(
            &home.path().join(".claude").join("settings.json"),
            &claude_settings("echo user"),
        );
        write_fixture(
            &project.path().join(".claude").join("settings.json"),
            &claude_settings("echo project"),
        );
        write_fixture(
            &project.path().join(".claude").join("settings.local.json"),
            &claude_settings("echo local"),
        );

        let dto = inspect(Some(home.path()), Some(project.path()), Some("p1".to_string()));

        let claude: Vec<&HookEntryDto> = dto
            .entries
            .iter()
            .filter(|e| e.agent == HookAgent::ClaudeCode)
            .collect();
        assert_eq!(claude.len(), 3);
        assert_eq!(
            claude.iter().map(|e| e.scope).collect::<Vec<_>>(),
            vec![HookScope::User, HookScope::Project, HookScope::ProjectLocal]
        );
        assert_eq!(
            claude
                .iter()
                .map(|e| field_value(e, "command").expect("command").value.clone())
                .collect::<Vec<_>>(),
            vec!["echo user", "echo project", "echo local"]
        );
        assert_eq!(dto.selected_project_id.as_deref(), Some("p1"));
    }

    // Scenario: Invalid source is isolated
    #[test]
    fn invalid_json_source_is_isolated() {
        let home = tempfile::tempdir().expect("home");
        write_fixture(&home.path().join(".codex").join("hooks.json"), "{ not json");
        write_fixture(
            &home.path().join(".claude").join("settings.json"),
            &claude_settings("echo user"),
        );

        let dto = inspect(Some(home.path()), None, None);

        let broken = source(&dto, "codex:user:hooks-json");
        assert_eq!(broken.status, HookSourceStatus::Invalid);
        let diagnostic = broken.diagnostic.as_ref().expect("diagnostic");
        assert_eq!(diagnostic.kind, HookDiagnosticKind::InvalidSyntax);
        assert!(!diagnostic.message.is_empty());
        assert!(!diagnostic.message.contains("not json"));
        assert_eq!(broken.entry_count, 0);

        let healthy = source(&dto, "claude_code:user:settings-json");
        assert_eq!(healthy.status, HookSourceStatus::Valid);
        assert_eq!(entries_of(&dto, "claude_code:user:settings-json").len(), 1);
        assert!(!dto.compatibility.is_empty());
    }

    #[test]
    fn invalid_toml_source_is_isolated() {
        let home = tempfile::tempdir().expect("home");
        write_fixture(
            &home.path().join(".codex").join("config.toml"),
            "[[hooks.SessionStart]\nmatcher = ",
        );
        write_fixture(
            &home.path().join(".codex").join("hooks.json"),
            CODEX_HOOKS_JSON,
        );

        let dto = inspect(Some(home.path()), None, None);

        let broken = source(&dto, "codex:user:config-toml");
        assert_eq!(broken.status, HookSourceStatus::Invalid);
        assert_eq!(
            broken.diagnostic.as_ref().expect("diagnostic").kind,
            HookDiagnosticKind::InvalidSyntax
        );
        assert_eq!(source(&dto, "codex:user:hooks-json").status, HookSourceStatus::Valid);
        assert_eq!(entries_of(&dto, "codex:user:hooks-json").len(), 1);
    }

    #[test]
    fn hook_subtree_shape_error_is_reported_per_source() {
        let home = tempfile::tempdir().expect("home");
        write_fixture(
            &home.path().join(".claude").join("settings.json"),
            r#"{ "hooks": "not-an-object" }"#,
        );

        let dto = inspect(Some(home.path()), None, None);
        let broken = source(&dto, "claude_code:user:settings-json");
        assert_eq!(broken.status, HookSourceStatus::Invalid);
        assert_eq!(
            broken.diagnostic.as_ref().expect("diagnostic").kind,
            HookDiagnosticKind::InvalidShape
        );
    }

    #[test]
    fn missing_source_is_a_normal_empty_state() {
        let home = tempfile::tempdir().expect("home");
        let dto = inspect(Some(home.path()), None, None);

        assert_eq!(dto.sources.len(), 3);
        for dto_source in &dto.sources {
            assert_eq!(dto_source.status, HookSourceStatus::Missing);
            assert!(dto_source.diagnostic.is_none());
            assert_eq!(dto_source.entry_count, 0);
            assert!(dto_source.canonical_text.is_empty());
            assert!(!dto_source.diff_available);
        }
        assert!(dto.entries.is_empty());
    }

    // Failure mode: home directory cannot be resolved.
    #[test]
    fn unresolved_home_marks_user_sources_invalid_without_cwd_fallback() {
        let project = tempfile::tempdir().expect("project");
        write_fixture(
            &project.path().join(".claude").join("settings.json"),
            &claude_settings("echo project"),
        );

        let dto = inspect(None, Some(project.path()), Some("p1".to_string()));

        for user_source in dto.sources.iter().filter(|s| s.scope == HookScope::User) {
            assert_eq!(user_source.status, HookSourceStatus::Invalid);
            assert_eq!(
                user_source.diagnostic.as_ref().expect("diagnostic").kind,
                HookDiagnosticKind::NotReadable
            );
            assert!(!user_source.display_path.starts_with('/'));
        }
        // The linked Project is still readable.
        assert_eq!(
            source(&dto, "claude_code:project:settings-json").status,
            HookSourceStatus::Valid
        );
        assert_eq!(dto.entries.len(), 1);
    }

    #[test]
    fn invalid_utf8_source_is_diagnosed() {
        let home = tempfile::tempdir().expect("home");
        let path = home.path().join(".codex").join("hooks.json");
        fs::create_dir_all(path.parent().expect("parent")).expect("dir");
        fs::write(&path, [0x7b, 0xff, 0xfe, 0x7d]).expect("write bytes");

        let dto = inspect(Some(home.path()), None, None);
        let broken = source(&dto, "codex:user:hooks-json");
        assert_eq!(broken.status, HookSourceStatus::Invalid);
        assert_eq!(
            broken.diagnostic.as_ref().expect("diagnostic").kind,
            HookDiagnosticKind::InvalidEncoding
        );
    }

    #[cfg(unix)]
    #[test]
    fn permission_denied_source_is_diagnosed() {
        use std::os::unix::fs::PermissionsExt;

        let home = tempfile::tempdir().expect("home");
        let path = home.path().join(".claude").join("settings.json");
        write_fixture(&path, &claude_settings("echo user"));
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("chmod");

        // Root bypasses the mode bits, so the denial is not reproducible there.
        if fs::read(&path).is_ok() {
            fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("restore chmod");
            return;
        }

        let dto = inspect(Some(home.path()), None, None);
        let broken = source(&dto, "claude_code:user:settings-json");
        assert_eq!(broken.status, HookSourceStatus::Invalid);
        assert_eq!(
            broken.diagnostic.as_ref().expect("diagnostic").kind,
            HookDiagnosticKind::NotReadable
        );

        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("restore chmod");
    }

    #[test]
    fn multi_handler_and_group_order_is_deterministic() {
        let home = tempfile::tempdir().expect("home");
        write_fixture(
            &home.path().join(".claude").join("settings.json"),
            r#"{
  "hooks": {
    "PreToolUse": [
      { "matcher": "Bash", "hooks": [
        { "type": "command", "command": "a0" },
        { "type": "command", "command": "a1" }
      ] },
      { "matcher": "Write", "hooks": [
        { "type": "command", "command": "b0" }
      ] }
    ],
    "Notification": [
      { "hooks": [ { "type": "command", "command": "n0" } ] }
    ]
  }
}"#,
        );

        let first = inspect(Some(home.path()), None, None);
        let second = inspect(Some(home.path()), None, None);

        let shape: Vec<(String, usize, usize, String)> = first
            .entries
            .iter()
            .map(|e| {
                (
                    e.event.clone(),
                    e.group_index,
                    e.handler_index,
                    field_value(e, "command").expect("command").value.clone(),
                )
            })
            .collect();

        // Events sort by name, then matcher group order, then handler order.
        assert_eq!(
            shape,
            vec![
                ("Notification".to_string(), 0, 0, "n0".to_string()),
                ("PreToolUse".to_string(), 0, 0, "a0".to_string()),
                ("PreToolUse".to_string(), 0, 1, "a1".to_string()),
                ("PreToolUse".to_string(), 1, 0, "b0".to_string()),
            ]
        );
        assert_eq!(
            first.entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>(),
            second.entries.iter().map(|e| e.id.clone()).collect::<Vec<_>>()
        );
        assert_eq!(first.entries[0].matcher, None);
    }

    // Scenario: Unknown Hook values remain visible
    #[test]
    fn unknown_hook_values_remain_visible() {
        let home = tempfile::tempdir().expect("home");
        write_fixture(
            &home.path().join(".claude").join("settings.json"),
            r#"{
  "hooks": {
    "FutureEvent": [
      { "matcher": "*", "hooks": [
        { "type": "future_handler", "vendorOption": "keep-me", "command": "echo ok" }
      ] }
    ]
  }
}"#,
        );

        let dto = inspect(Some(home.path()), None, None);
        let entries = entries_of(&dto, "claude_code:user:settings-json");
        assert_eq!(entries.len(), 1);
        let entry = entries[0];

        assert_eq!(entry.event, "FutureEvent");
        assert!(!entry.event_known);
        assert_eq!(entry.handler_type, "future_handler");
        assert!(!entry.handler_type_known);

        let vendor = field_value(entry, "vendorOption").expect("vendorOption retained");
        assert_eq!(vendor.value, "keep-me");
        assert!(!vendor.known);

        let command = field_value(entry, "command").expect("command retained");
        assert!(command.known);

        // Nothing was normalized into a known capability.
        assert!(!dto
            .compatibility
            .iter()
            .any(|row| row.name == "FutureEvent" || row.name == "future_handler"));
    }

    // Requirement: Inspection responses exclude non-Hook configuration and persistence
    // Scenario: Non-Hook secret sibling is excluded
    #[test]
    fn non_hook_sibling_secret_is_excluded_from_response() {
        let home = tempfile::tempdir().expect("home");
        write_fixture(
            &home.path().join(".claude").join("settings.json"),
            r#"{
  "apiToken": "sentinel-secret",
  "env": { "OTHER": "sentinel-secret" },
  "hooks": {
    "PostToolUse": [
      { "matcher": "Write", "hooks": [ { "type": "command", "command": "echo ok" } ] }
    ]
  }
}"#,
        );
        // A broken source whose text carries the sentinel must not echo it back
        // through the diagnostic message either.
        write_fixture(
            &home.path().join(".codex").join("config.toml"),
            "token = \"sentinel-secret\"\n[[hooks.SessionStart]\n",
        );

        let dto = inspect(Some(home.path()), None, None);
        let settings = source(&dto, "claude_code:user:settings-json");
        assert_eq!(settings.status, HookSourceStatus::Valid);
        assert_eq!(settings.entry_count, 1);
        assert!(settings.canonical_text.contains("PostToolUse"));
        assert_eq!(
            source(&dto, "codex:user:config-toml").status,
            HookSourceStatus::Invalid
        );

        let serialized = serde_json::to_string(&dto).expect("serialize inspection");
        assert!(
            !serialized.contains("sentinel-secret"),
            "non-Hook sibling values must not reach the frontend"
        );
        assert!(!serialized.contains("apiToken"));
        assert!(serialized.contains("PostToolUse"));
    }

    /// Builds a JSON settings document of exactly `total` bytes whose Hook
    /// subtree stays valid, padding with a non-Hook sibling.
    fn settings_of_exact_size(total: usize) -> String {
        let prefix = r#"{"hooks":{"PreToolUse":[{"matcher":"Bash","hooks":[{"type":"command","command":"echo ok"}]}]},"pad":""#;
        let suffix = r#""}"#;
        let pad = total - prefix.len() - suffix.len();
        format!("{prefix}{}{suffix}", "x".repeat(pad))
    }

    // Scenario: Oversized source fails before parsing
    #[test]
    fn source_at_the_byte_limit_is_still_parsed() {
        let home = tempfile::tempdir().expect("home");
        let content = settings_of_exact_size(MAX_SOURCE_BYTES as usize);
        assert_eq!(content.len(), 1_048_576);
        write_fixture(&home.path().join(".claude").join("settings.json"), &content);

        let dto = inspect(Some(home.path()), None, None);
        let settings = source(&dto, "claude_code:user:settings-json");
        assert_eq!(settings.status, HookSourceStatus::Valid);
        assert_eq!(settings.entry_count, 1);
    }

    #[test]
    fn source_over_the_byte_limit_is_too_large() {
        let home = tempfile::tempdir().expect("home");
        let content = settings_of_exact_size(MAX_SOURCE_BYTES as usize + 1);
        assert_eq!(content.len(), 1_048_577);
        write_fixture(&home.path().join(".claude").join("settings.json"), &content);
        write_fixture(
            &home.path().join(".codex").join("hooks.json"),
            CODEX_HOOKS_JSON,
        );

        let dto = inspect(Some(home.path()), None, None);
        let settings = source(&dto, "claude_code:user:settings-json");
        assert_eq!(settings.status, HookSourceStatus::TooLarge);
        assert_eq!(
            settings.diagnostic.as_ref().expect("diagnostic").kind,
            HookDiagnosticKind::TooLarge
        );
        assert_eq!(settings.entry_count, 0);
        assert!(settings.canonical_text.is_empty());
        assert!(!settings.diff_available);
        assert!(dto.entries.iter().all(|e| e.source_id != settings.id));

        // Other sources stay available.
        assert_eq!(
            source(&dto, "codex:user:hooks-json").status,
            HookSourceStatus::Valid
        );
    }

    /// Inspects one Claude Code settings document and returns its source DTO.
    fn inspect_settings(home: &tempfile::TempDir, content: &str) -> HookSourceDto {
        write_fixture(&home.path().join(".claude").join("settings.json"), content);
        let dto = inspect(Some(home.path()), None, None);
        source(&dto, "claude_code:user:settings-json").clone()
    }

    fn settings_with_command(command_len: usize) -> String {
        format!(
            r#"{{"hooks":{{"PreToolUse":[{{"matcher":"Bash","hooks":[{{"type":"command","command":"{}"}}]}}]}}}}"#,
            "e".repeat(command_len)
        )
    }

    /// Grows the command string until the canonical Hook text is exactly
    /// `target` bytes — one ASCII character in, one byte out.
    fn settings_with_canonical_bytes(home: &tempfile::TempDir, target: usize) -> String {
        let probe_len = 1_000;
        let probe = inspect_settings(home, &settings_with_command(probe_len));
        let probe_size = probe.canonical_text.len();
        assert!(probe_size < target, "probe {probe_size} must be below {target}");
        settings_with_command(probe_len + (target - probe_size))
    }

    fn settings_with_padding_fields(count: usize) -> String {
        let padding: String = (0..count)
            .map(|i| format!(r#","pad{i}":{i}"#))
            .collect::<Vec<_>>()
            .join("");
        format!(
            r#"{{"hooks":{{"PreToolUse":[{{"matcher":"Bash","hooks":[{{"type":"command","command":"echo ok"{padding}}}]}}]}}}}"#
        )
    }

    /// Adds one padding field per extra canonical line.
    fn settings_with_canonical_lines(home: &tempfile::TempDir, target: usize) -> String {
        let probe_count = 10;
        let probe = inspect_settings(home, &settings_with_padding_fields(probe_count));
        let probe_lines = probe.canonical_text.lines().count();
        assert!(probe_lines < target, "probe {probe_lines} must be below {target}");
        settings_with_padding_fields(probe_count + (target - probe_lines))
    }

    // Requirement: Source comparison is bounded and same-Agent only
    // Scenario: Canonical fragment reaches the diff limit
    #[test]
    fn canonical_text_at_the_byte_limit_is_diffable() {
        let home = tempfile::tempdir().expect("home");
        let content = settings_with_canonical_bytes(&home, MAX_DIFF_BYTES);
        let settings = inspect_settings(&home, &content);

        assert_eq!(settings.canonical_text.len(), 262_144);
        assert_eq!(settings.status, HookSourceStatus::Valid);
        assert!(settings.diff_available);
    }

    #[test]
    fn canonical_text_over_the_byte_limit_keeps_entries_without_diff() {
        let home = tempfile::tempdir().expect("home");
        let content = settings_with_canonical_bytes(&home, MAX_DIFF_BYTES + 1);
        let settings = inspect_settings(&home, &content);

        assert_eq!(settings.canonical_text.len(), 262_145);
        assert_eq!(settings.status, HookSourceStatus::Valid);
        assert_eq!(settings.entry_count, 1);
        assert!(!settings.diff_available);
    }

    #[test]
    fn canonical_text_at_the_line_limit_is_diffable() {
        let home = tempfile::tempdir().expect("home");
        let content = settings_with_canonical_lines(&home, MAX_DIFF_LINES);
        let settings = inspect_settings(&home, &content);

        assert_eq!(settings.canonical_text.lines().count(), 4_000);
        assert!(settings.diff_available);
    }

    #[test]
    fn canonical_text_over_the_line_limit_keeps_entries_without_diff() {
        let home = tempfile::tempdir().expect("home");
        let content = settings_with_canonical_lines(&home, MAX_DIFF_LINES + 1);
        let settings = inspect_settings(&home, &content);

        assert_eq!(settings.canonical_text.lines().count(), 4_001);
        assert_eq!(settings.entry_count, 1);
        assert!(!settings.diff_available);
    }

    #[test]
    fn empty_hook_subtree_is_valid_with_empty_canonical_text() {
        let home = tempfile::tempdir().expect("home");
        write_fixture(
            &home.path().join(".claude").join("settings.json"),
            r#"{ "model": "sonnet" }"#,
        );

        let dto = inspect(Some(home.path()), None, None);
        let settings = source(&dto, "claude_code:user:settings-json");
        assert_eq!(settings.status, HookSourceStatus::Valid);
        assert_eq!(settings.entry_count, 0);
        assert_eq!(settings.canonical_text, "");
        assert!(settings.diff_available);
    }

    // Requirement: Compatibility matrix is explicit and snapshot-based
    //
    // The expected table is the 2026-08-12 documentation snapshot written out
    // by hand. Deriving it from the same lists the production registry uses
    // would only assert that the code equals itself.
    type ExpectedRow = (&'static str, &'static str, Option<&'static str>, &'static str, Option<&'static str>);

    const SHARED: Option<&str> = Some("shared_name_distinct_contract");
    const ABSENT: Option<&str> = Some("not_in_snapshot");
    const SKIPPED: Option<&str> = Some("parsed_but_skipped");

    /// (name, codex support, codex note, claude_code support, claude_code note)
    const EXPECTED_EVENTS: &[ExpectedRow] = &[
        ("SessionStart", "supported", SHARED, "supported", SHARED),
        ("Setup", "unsupported", ABSENT, "supported", None),
        ("UserPromptSubmit", "supported", SHARED, "supported", SHARED),
        ("UserPromptExpansion", "unsupported", ABSENT, "supported", None),
        ("PreToolUse", "supported", SHARED, "supported", SHARED),
        ("PermissionRequest", "supported", SHARED, "supported", SHARED),
        ("PermissionDenied", "unsupported", ABSENT, "supported", None),
        ("PostToolUse", "supported", SHARED, "supported", SHARED),
        ("PostToolUseFailure", "unsupported", ABSENT, "supported", None),
        ("PostToolBatch", "unsupported", ABSENT, "supported", None),
        ("Notification", "unsupported", ABSENT, "supported", None),
        ("MessageDisplay", "unsupported", ABSENT, "supported", None),
        ("SubagentStart", "supported", SHARED, "supported", SHARED),
        ("SubagentStop", "supported", SHARED, "supported", SHARED),
        ("TaskCreated", "unsupported", ABSENT, "supported", None),
        ("TaskCompleted", "unsupported", ABSENT, "supported", None),
        ("Stop", "supported", SHARED, "supported", SHARED),
        ("StopFailure", "unsupported", ABSENT, "supported", None),
        ("TeammateIdle", "unsupported", ABSENT, "supported", None),
        ("InstructionsLoaded", "unsupported", ABSENT, "supported", None),
        ("ConfigChange", "unsupported", ABSENT, "supported", None),
        ("CwdChanged", "unsupported", ABSENT, "supported", None),
        ("DirectoryAdded", "unsupported", ABSENT, "supported", None),
        ("FileChanged", "unsupported", ABSENT, "supported", None),
        ("WorktreeCreate", "unsupported", ABSENT, "supported", None),
        ("WorktreeRemove", "unsupported", ABSENT, "supported", None),
        ("PreCompact", "supported", SHARED, "supported", SHARED),
        ("PostCompact", "supported", SHARED, "supported", SHARED),
        ("Elicitation", "unsupported", ABSENT, "supported", None),
        ("ElicitationResult", "unsupported", ABSENT, "supported", None),
        ("SessionEnd", "supported", SHARED, "supported", SHARED),
    ];

    const EXPECTED_HANDLERS: &[ExpectedRow] = &[
        ("command", "supported", SHARED, "supported", SHARED),
        ("http", "unsupported", ABSENT, "supported", None),
        ("mcp_tool", "unsupported", ABSENT, "supported", None),
        ("prompt", "unsupported", SKIPPED, "supported", None),
        ("agent", "unsupported", SKIPPED, "supported", None),
    ];

    fn render(row: &ExpectedRow) -> String {
        format!(
            "{}|{}|{}|{}|{}",
            row.0,
            row.1,
            row.2.unwrap_or("-"),
            row.3,
            row.4.unwrap_or("-")
        )
    }

    fn actual_rows(category: CompatibilityCategory) -> Vec<String> {
        compatibility_registry()
            .iter()
            .filter(|row| row.category == category)
            .map(|row| {
                render(&(
                    row.name,
                    row.codex.support.as_str(),
                    row.codex.note,
                    row.claude_code.support.as_str(),
                    row.claude_code.note,
                ))
            })
            .collect()
    }

    fn expected_rows(rows: &[ExpectedRow]) -> Vec<String> {
        rows.iter().map(render).collect()
    }

    #[test]
    fn compatibility_events_match_the_official_snapshot() {
        assert_eq!(COMPATIBILITY_SNAPSHOT_DATE, "2026-08-12");
        assert_eq!(
            actual_rows(CompatibilityCategory::Event),
            expected_rows(EXPECTED_EVENTS)
        );
    }

    // Scenario: Handler support remains Agent-specific
    #[test]
    fn handler_support_remains_agent_specific() {
        assert_eq!(
            actual_rows(CompatibilityCategory::Handler),
            expected_rows(EXPECTED_HANDLERS)
        );

        let registry = compatibility_registry();
        let handler = |name: &str| {
            registry
                .iter()
                .find(|row| row.category == CompatibilityCategory::Handler && row.name == name)
                .unwrap_or_else(|| panic!("handler row {name} missing"))
        };
        assert_eq!(handler("command").codex.support, HookSupport::Supported);
        for name in ["command", "http", "mcp_tool", "prompt", "agent"] {
            assert_eq!(handler(name).claude_code.support, HookSupport::Supported);
        }
        for name in ["http", "mcp_tool", "prompt", "agent"] {
            assert_ne!(handler(name).codex.support, HookSupport::Supported);
        }
    }

    // Scenario: Shared event name keeps separate notes
    #[test]
    fn shared_event_name_keeps_agent_specific_notes() {
        let registry = compatibility_registry();
        let row = registry
            .iter()
            .find(|row| row.category == CompatibilityCategory::Event && row.name == "PreToolUse")
            .expect("PreToolUse row");

        assert_eq!(row.codex.support, HookSupport::Supported);
        assert_eq!(row.claude_code.support, HookSupport::Supported);
        assert!(row.codex.note.is_some());
        assert!(row.claude_code.note.is_some());
    }

    // The three support states are the frontend's whole vocabulary. The
    // 2026-08-12 snapshot documents a closed set for both Agents, so no cell is
    // `unknown` today; the state is reserved for a registry row whose support a
    // later snapshot leaves unstated, and for discovery values off the registry.
    #[test]
    fn support_states_are_the_fixed_three() {
        assert_eq!(HookSupport::Supported.as_str(), "supported");
        assert_eq!(HookSupport::Unsupported.as_str(), "unsupported");
        assert_eq!(HookSupport::Unknown.as_str(), "unknown");
    }

    // Scenario: Future value remains unknown
    #[test]
    fn future_value_is_not_promoted_to_supported() {
        let home = tempfile::tempdir().expect("home");
        write_fixture(
            &home.path().join(".claude").join("settings.json"),
            r#"{ "hooks": { "FutureEvent": [ { "hooks": [ { "type": "command", "command": "x" } ] } ] } }"#,
        );

        let before = compatibility_registry();
        let dto = inspect(Some(home.path()), None, None);
        let entry = entries_of(&dto, "claude_code:user:settings-json")[0];

        assert_eq!(entry.event, "FutureEvent");
        assert!(!entry.event_known);
        assert_eq!(dto.compatibility.len(), before.len());
        assert!(!dto.compatibility.iter().any(|row| row.name == "FutureEvent"));
        assert_eq!(
            dto.compatibility
                .iter()
                .filter(|row| row.codex.support == HookSupport::Supported)
                .count(),
            before
                .iter()
                .filter(|row| row.codex.support == HookSupport::Supported)
                .count()
        );
    }

    #[test]
    fn invalid_and_missing_sources_are_never_diffable() {
        let home = tempfile::tempdir().expect("home");
        write_fixture(&home.path().join(".codex").join("hooks.json"), "{ broken");

        let dto = inspect(Some(home.path()), None, None);
        for dto_source in &dto.sources {
            assert!(
                !dto_source.diff_available,
                "{} must not be diffable",
                dto_source.id
            );
        }
    }
}
