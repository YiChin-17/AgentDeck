//! Read-only Config Profile inspection for Codex and Claude Code.
//!
//! Every path this module touches is composed by the backend from a fixed table
//! plus, at most, one registered Project root. A caller supplies an opaque
//! Project id and filters — never a directory, a filename or an environment.
//!
//! Nothing here writes, executes or persists. See
//! `openspec/changes/inspect-codex-claude-config-profiles`.

use std::fs;
use std::io::{ErrorKind, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A config file above this size is never read into memory.
pub const MAX_SOURCE_BYTES: u64 = 1_048_576;

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigAgent {
    Codex,
    ClaudeCode,
}

impl ConfigAgent {
    /// The persisted and IPC spelling. Kept in step with the serde rename so a
    /// stored row and a serialized DTO can never disagree about an Agent.
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigAgent::Codex => "codex",
            ConfigAgent::ClaudeCode => "claude_code",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "codex" => Some(ConfigAgent::Codex),
            "claude_code" => Some(ConfigAgent::ClaudeCode),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigScope {
    User,
    Project,
    ProjectLocal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFormat {
    Toml,
    Json,
}

/// The complete status vocabulary of one fixed source. Exactly one applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigSourceStatus {
    Missing,
    Available,
    Unreadable,
    TooLarge,
    UnsupportedSymlink,
    InvalidFormat,
}

/// Stable diagnostic codes. The code is the whole message: a parser string or an
/// OS error string could carry a path or a fragment of the user's config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigDiagnosticCode {
    Unreadable,
    TooLarge,
    UnsupportedSymlink,
    InvalidFormat,
    InvalidAllowedValue,
}

/// Failure of the request as a whole, as opposed to a per-source diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigInventoryError {
    ProjectNotFound,
}

impl ConfigInventoryError {
    pub fn as_str(&self) -> &'static str {
        match self {
            ConfigInventoryError::ProjectNotFound => "project_not_found",
        }
    }
}

/// Where a value sits once the supported sources are ordered.
///
/// `ProjectCandidate` exists because Codex loads a project config only after the
/// user trusts that directory, which this capability does not inspect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigResolution {
    ObservedActive,
    ObservedOverridden,
    ProjectCandidate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigDiffStatus {
    Same,
    Added,
    Changed,
    Removed,
}

/// The only value shapes that cross the boundary.
///
/// A generic JSON or TOML value would let an unknown table or array through, so
/// the DTO can express nothing but these three scalars.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ConfigValueDto {
    String(String),
    Boolean(bool),
    Integer(i64),
}

/// The scalar type of one allowlisted key, without a value attached.
///
/// Management needs to state "this key is an integer" before the user has typed
/// anything, which a `ConfigValueDto` cannot express.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigValueKind {
    String,
    Boolean,
    Integer,
}

impl ConfigValueDto {
    pub fn kind(&self) -> ConfigValueKind {
        match self {
            ConfigValueDto::String(_) => ConfigValueKind::String,
            ConfigValueDto::Boolean(_) => ConfigValueKind::Boolean,
            ConfigValueDto::Integer(_) => ConfigValueKind::Integer,
        }
    }
}

// ---------------------------------------------------------------------------
// Allowlist
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AllowedType {
    Text,
    Boolean,
    Integer,
}

/// One exact key with one exact scalar type.
///
/// `native` is the key path as it appears in the file; `canonical` is the name
/// the inventory compares across scopes of the same Agent.
#[derive(Debug, Clone, Copy)]
struct AllowedKey {
    canonical: &'static str,
    native: &'static str,
    value_type: AllowedType,
}

const CODEX_ALLOWLIST: &[AllowedKey] = &[
    AllowedKey { canonical: "model", native: "model", value_type: AllowedType::Text },
    AllowedKey {
        canonical: "model_reasoning_effort",
        native: "model_reasoning_effort",
        value_type: AllowedType::Text,
    },
    AllowedKey {
        canonical: "model_verbosity",
        native: "model_verbosity",
        value_type: AllowedType::Text,
    },
    // Codex also accepts a table form; only the string form is allowlisted, so a
    // table fails closed instead of being flattened into unknown sub-keys.
    AllowedKey {
        canonical: "approval_policy",
        native: "approval_policy",
        value_type: AllowedType::Text,
    },
    AllowedKey { canonical: "sandbox_mode", native: "sandbox_mode", value_type: AllowedType::Text },
    AllowedKey { canonical: "web_search", native: "web_search", value_type: AllowedType::Boolean },
    AllowedKey { canonical: "service_tier", native: "service_tier", value_type: AllowedType::Text },
    AllowedKey { canonical: "personality", native: "personality", value_type: AllowedType::Text },
];

const CLAUDE_ALLOWLIST: &[AllowedKey] = &[
    AllowedKey { canonical: "model", native: "model", value_type: AllowedType::Text },
    AllowedKey {
        canonical: "always_thinking_enabled",
        native: "alwaysThinkingEnabled",
        value_type: AllowedType::Boolean,
    },
    AllowedKey {
        canonical: "auto_updates_channel",
        native: "autoUpdatesChannel",
        value_type: AllowedType::Text,
    },
    AllowedKey {
        canonical: "cleanup_period_days",
        native: "cleanupPeriodDays",
        value_type: AllowedType::Integer,
    },
    AllowedKey { canonical: "fast_mode", native: "fastMode", value_type: AllowedType::Boolean },
    AllowedKey {
        canonical: "permission_default_mode",
        native: "permissions.defaultMode",
        value_type: AllowedType::Text,
    },
];

fn allowlist(agent: ConfigAgent) -> &'static [AllowedKey] {
    match agent {
        ConfigAgent::Codex => CODEX_ALLOWLIST,
        ConfigAgent::ClaudeCode => CLAUDE_ALLOWLIST,
    }
}

impl AllowedType {
    fn kind(self) -> ConfigValueKind {
        match self {
            AllowedType::Text => ConfigValueKind::String,
            AllowedType::Boolean => ConfigValueKind::Boolean,
            AllowedType::Integer => ConfigValueKind::Integer,
        }
    }
}

/// One writable setting, as the management editor and validator see it.
///
/// `native` is exposed because the transform has to address the key as it
/// appears in the document; nothing else outside this module may compose a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AllowlistedKey {
    pub canonical: &'static str,
    pub native: &'static str,
    pub kind: ConfigValueKind,
}

/// Every key one Agent may carry in a profile, in allowlist order.
///
/// Management never widens this set: a profile entry outside it has no
/// transform, so it could only be persisted as content nobody can apply.
pub fn allowlisted_keys(agent: ConfigAgent) -> Vec<AllowlistedKey> {
    allowlist(agent)
        .iter()
        .map(|key| AllowlistedKey {
            canonical: key.canonical,
            native: key.native,
            kind: key.value_type.kind(),
        })
        .collect()
}

/// The one exact key an Agent and canonical name resolve to, if any.
pub fn allowlisted_key(agent: ConfigAgent, canonical: &str) -> Option<AllowlistedKey> {
    allowlisted_keys(agent)
        .into_iter()
        .find(|key| key.canonical == canonical)
}

// ---------------------------------------------------------------------------
// Fixed source descriptors
// ---------------------------------------------------------------------------

/// One inspectable config file. The frontend only ever sees `id`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSourceDescriptor {
    pub id: &'static str,
    pub agent: ConfigAgent,
    pub scope: ConfigScope,
    pub format: ConfigFormat,
    pub path: PathBuf,
}

fn descriptor(
    id: &'static str,
    agent: ConfigAgent,
    scope: ConfigScope,
    format: ConfigFormat,
    path: PathBuf,
) -> ConfigSourceDescriptor {
    ConfigSourceDescriptor { id, agent, scope, format, path }
}

/// The complete set of locations this capability reads, in display order.
///
/// Nothing is discovered by scanning: a walk of home or of a project root would
/// reach managed policy, named profile files and credential stores that are all
/// out of scope here.
pub fn source_descriptors(
    home: &Path,
    project_root: Option<&Path>,
) -> Vec<ConfigSourceDescriptor> {
    let mut sources = vec![
        descriptor(
            "codex:user:config-toml",
            ConfigAgent::Codex,
            ConfigScope::User,
            ConfigFormat::Toml,
            home.join(".codex").join("config.toml"),
        ),
        descriptor(
            "claude_code:user:settings-json",
            ConfigAgent::ClaudeCode,
            ConfigScope::User,
            ConfigFormat::Json,
            home.join(".claude").join("settings.json"),
        ),
    ];

    if let Some(root) = project_root {
        sources.extend([
            descriptor(
                "codex:project:config-toml",
                ConfigAgent::Codex,
                ConfigScope::Project,
                ConfigFormat::Toml,
                root.join(".codex").join("config.toml"),
            ),
            descriptor(
                "claude_code:project:settings-json",
                ConfigAgent::ClaudeCode,
                ConfigScope::Project,
                ConfigFormat::Json,
                root.join(".claude").join("settings.json"),
            ),
            descriptor(
                "claude_code:project_local:settings-local-json",
                ConfigAgent::ClaudeCode,
                ConfigScope::ProjectLocal,
                ConfigFormat::Json,
                root.join(".claude").join("settings.local.json"),
            ),
        ]);
    }

    sources
}

/// Turns an optional Project id into a Project root through `lookup`.
///
/// An id that does not resolve is an error rather than an empty result: falling
/// back to the process working directory would read files the user never linked.
pub fn resolve_project_root(
    project_id: Option<&str>,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<Option<PathBuf>, ConfigInventoryError> {
    match project_id {
        None => Ok(None),
        Some(id) => lookup(id)
            .map(|root| Some(PathBuf::from(root)))
            .ok_or(ConfigInventoryError::ProjectNotFound),
    }
}

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

/// Everything the frontend may say. `deny_unknown_fields` is the enforcement:
/// a request carrying `path`, `cwd` or `env` fails to deserialize.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigInventoryRequest {
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub agent: Option<ConfigAgent>,
    #[serde(default)]
    pub scope: Option<ConfigScope>,
}

// ---------------------------------------------------------------------------
// Response shapes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDiagnosticDto {
    pub code: ConfigDiagnosticCode,
    pub agent: ConfigAgent,
    pub scope: ConfigScope,
    pub project_id: Option<String>,
    pub source_id: String,
    /// Present only for `invalid_allowed_value`, and then it is an allowlisted
    /// key name — never the rejected value.
    pub key: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSourceDto {
    pub id: String,
    pub agent: ConfigAgent,
    pub scope: ConfigScope,
    pub project_id: Option<String>,
    pub format: ConfigFormat,
    pub display_path: String,
    pub status: ConfigSourceStatus,
    /// SHA-256 of the bytes parsed for this response; `available` sources only.
    pub fingerprint: Option<String>,
    /// That more content exists — never how much, which keys, or which values.
    pub has_unexposed_fields: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigSettingDto {
    pub agent: ConfigAgent,
    pub canonical_key: String,
    pub native_key: String,
    pub value: ConfigValueDto,
    pub scope: ConfigScope,
    pub source_id: String,
    pub project_id: Option<String>,
    pub resolution: ConfigResolution,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigDiffEntryDto {
    pub agent: ConfigAgent,
    pub canonical_key: String,
    pub base_scope: ConfigScope,
    pub compare_scope: ConfigScope,
    pub status: ConfigDiffStatus,
    pub base_value: Option<ConfigValueDto>,
    pub compare_value: Option<ConfigValueDto>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigProfileInventoryDto {
    pub sources: Vec<ConfigSourceDto>,
    pub settings: Vec<ConfigSettingDto>,
    /// Adjacent-layer comparison of allowlisted typed values. It is part of the
    /// response rather than a frontend computation so the "no raw document text"
    /// rule is enforced on the side of the boundary that holds the documents.
    pub diffs: Vec<ConfigDiffEntryDto>,
    pub diagnostics: Vec<ConfigDiagnosticDto>,
    pub selected_project_id: Option<String>,
    pub generated_at: i64,
}

// ---------------------------------------------------------------------------
// Reading one source
// ---------------------------------------------------------------------------

#[cfg(test)]
thread_local! {
    /// Counts filesystem reads on the current test's thread, so a test can prove
    /// that a rejected request touched nothing.
    static READ_PROBE: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn note_read() {
    READ_PROBE.with(|count| count.set(count.get() + 1));
}

#[cfg(not(test))]
fn note_read() {}

enum SourceRead {
    Missing,
    Symlink,
    TooLarge,
    Unreadable,
    Bytes(Vec<u8>),
}

fn read_source(path: &Path) -> SourceRead {
    note_read();
    // `symlink_metadata` does not follow the link, so a link inside a registered
    // project cannot turn into a read of whatever it points at.
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == ErrorKind::NotFound => return SourceRead::Missing,
        Err(_) => return SourceRead::Unreadable,
    };
    if metadata.file_type().is_symlink() {
        return SourceRead::Symlink;
    }
    if !metadata.is_file() {
        return SourceRead::Unreadable;
    }
    if metadata.len() > MAX_SOURCE_BYTES {
        return SourceRead::TooLarge;
    }
    // Bounded read: even if the file grew after the metadata check, at most
    // MAX_SOURCE_BYTES + 1 bytes are read. The extra byte detects growth.
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(err) if err.kind() == ErrorKind::NotFound => return SourceRead::Missing,
        Err(_) => return SourceRead::Unreadable,
    };
    let mut limited = file.take(MAX_SOURCE_BYTES + 1);
    let mut buf = Vec::with_capacity(metadata.len() as usize);
    match limited.read_to_end(&mut buf) {
        Ok(_) => {
            if buf.len() as u64 > MAX_SOURCE_BYTES {
                SourceRead::TooLarge
            } else {
                SourceRead::Bytes(buf)
            }
        }
        Err(err) if err.kind() == ErrorKind::NotFound => SourceRead::Missing,
        Err(_) => SourceRead::Unreadable,
    }
}

fn fingerprint(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

// ---------------------------------------------------------------------------
// Extracting allowlisted scalars
// ---------------------------------------------------------------------------

/// What one parsed source yielded.
struct Extraction {
    settings: Vec<(&'static AllowedKey, ConfigValueDto)>,
    /// Allowlisted keys that were present with the wrong shape.
    rejected: Vec<&'static str>,
    has_unexposed_fields: bool,
}

/// The outcome of an allowlist lookup in one document.
enum Lookup {
    Absent,
    /// Present with a shape the allowlist does not accept.
    WrongShape,
    Value(ConfigValueDto),
}

fn extract_toml(text: &str, keys: &'static [AllowedKey]) -> Result<Extraction, ()> {
    let document = text.parse::<toml_edit::DocumentMut>().map_err(|_| ())?;

    let lookup = |key: &AllowedKey| -> Lookup {
        let Some(item) = document.get(key.native) else {
            return Lookup::Absent;
        };
        match key.value_type {
            AllowedType::Text => match item.as_str() {
                Some(value) => Lookup::Value(ConfigValueDto::String(value.to_string())),
                None => Lookup::WrongShape,
            },
            AllowedType::Boolean => match item.as_bool() {
                Some(value) => Lookup::Value(ConfigValueDto::Boolean(value)),
                None => Lookup::WrongShape,
            },
            AllowedType::Integer => match item.as_integer() {
                Some(value) => Lookup::Value(ConfigValueDto::Integer(value)),
                None => Lookup::WrongShape,
            },
        }
    };

    let mut extraction = collect_extraction(keys, lookup);
    extraction.has_unexposed_fields = document
        .as_table()
        .iter()
        .any(|(name, _)| !is_fully_exposed(name, keys, &extraction));
    Ok(extraction)
}

fn extract_json(text: &str, keys: &'static [AllowedKey]) -> Result<Extraction, ()> {
    let document: serde_json::Value = serde_json::from_str(text).map_err(|_| ())?;
    let Some(object) = document.as_object() else {
        // A settings file that is not an object is not a settings file.
        return Err(());
    };

    let lookup = |key: &AllowedKey| -> Lookup {
        let mut node = &document;
        for segment in key.native.split('.') {
            match node.get(segment) {
                Some(child) => node = child,
                None => return Lookup::Absent,
            }
        }
        match key.value_type {
            AllowedType::Text => match node.as_str() {
                Some(value) => Lookup::Value(ConfigValueDto::String(value.to_string())),
                None => Lookup::WrongShape,
            },
            AllowedType::Boolean => match node.as_bool() {
                Some(value) => Lookup::Value(ConfigValueDto::Boolean(value)),
                None => Lookup::WrongShape,
            },
            AllowedType::Integer => match node.as_i64() {
                Some(value) => Lookup::Value(ConfigValueDto::Integer(value)),
                None => Lookup::WrongShape,
            },
        }
    };

    let mut extraction = collect_extraction(keys, lookup);
    extraction.has_unexposed_fields =
        object.keys().any(|name| !is_fully_exposed(name, keys, &extraction));
    Ok(extraction)
}

fn collect_extraction(
    keys: &'static [AllowedKey],
    lookup: impl Fn(&AllowedKey) -> Lookup,
) -> Extraction {
    let mut settings = Vec::new();
    let mut rejected = Vec::new();
    for key in keys {
        match lookup(key) {
            Lookup::Absent => {}
            Lookup::WrongShape => rejected.push(key.canonical),
            Lookup::Value(value) => settings.push((key, value)),
        }
    }
    Extraction { settings, rejected, has_unexposed_fields: false }
}

/// Whether a top-level key of the document was fully turned into an exposed
/// setting.
///
/// A nested allowlist entry such as `permissions.defaultMode` never qualifies:
/// its parent table can hold anything else, so the source is reported as having
/// unexposed content whenever the parent is present. Over-reporting is the safe
/// direction — it can only understate what the page is showing.
fn is_fully_exposed(name: &str, keys: &'static [AllowedKey], extraction: &Extraction) -> bool {
    keys.iter().any(|key| {
        key.native == name
            && extraction.settings.iter().any(|(exposed, _)| exposed.canonical == key.canonical)
    })
}

// ---------------------------------------------------------------------------
// Inspecting the fixed set
// ---------------------------------------------------------------------------

/// The precedence order of one Agent's supported scopes, lowest first.
fn scope_order(agent: ConfigAgent) -> &'static [ConfigScope] {
    match agent {
        ConfigAgent::Codex => &[ConfigScope::User, ConfigScope::Project],
        ConfigAgent::ClaudeCode => {
            &[ConfigScope::User, ConfigScope::Project, ConfigScope::ProjectLocal]
        }
    }
}

/// Reads every requested fixed source and assembles the read-only response.
///
/// `home` is optional: when it cannot be resolved the user sources report
/// `unreadable` instead of being composed against the process working
/// directory, which would inspect files the user never pointed at.
pub fn collect(
    home: Option<&Path>,
    project_root: Option<&Path>,
    selected_project_id: Option<String>,
    request: &ConfigInventoryRequest,
    generated_at: i64,
) -> ConfigProfileInventoryDto {
    let mut sources = Vec::new();
    let mut settings = Vec::new();
    let mut diagnostics = Vec::new();

    let home_base = home.unwrap_or_else(|| Path::new("~"));
    for source in source_descriptors(home_base, project_root) {
        if request.agent.is_some_and(|agent| agent != source.agent) {
            continue;
        }
        if request.scope.is_some_and(|scope| scope != source.scope) {
            continue;
        }

        let project_id = match source.scope {
            ConfigScope::User => None,
            _ => selected_project_id.clone(),
        };
        let mut record = ConfigSourceDto {
            id: source.id.to_string(),
            agent: source.agent,
            scope: source.scope,
            project_id: project_id.clone(),
            format: source.format,
            display_path: source.path.to_string_lossy().to_string(),
            status: ConfigSourceStatus::Missing,
            fingerprint: None,
            has_unexposed_fields: false,
        };
        let fail = |record: &mut ConfigSourceDto,
                        diagnostics: &mut Vec<ConfigDiagnosticDto>,
                        status: ConfigSourceStatus,
                        code: ConfigDiagnosticCode| {
            record.status = status;
            diagnostics.push(ConfigDiagnosticDto {
                code,
                agent: source.agent,
                scope: source.scope,
                project_id: project_id.clone(),
                source_id: source.id.to_string(),
                key: None,
            });
        };

        if home.is_none() && source.scope == ConfigScope::User {
            fail(
                &mut record,
                &mut diagnostics,
                ConfigSourceStatus::Unreadable,
                ConfigDiagnosticCode::Unreadable,
            );
            sources.push(record);
            continue;
        }

        let bytes = match read_source(&source.path) {
            SourceRead::Missing => {
                record.status = ConfigSourceStatus::Missing;
                sources.push(record);
                continue;
            }
            SourceRead::Symlink => {
                fail(
                    &mut record,
                    &mut diagnostics,
                    ConfigSourceStatus::UnsupportedSymlink,
                    ConfigDiagnosticCode::UnsupportedSymlink,
                );
                sources.push(record);
                continue;
            }
            SourceRead::TooLarge => {
                fail(
                    &mut record,
                    &mut diagnostics,
                    ConfigSourceStatus::TooLarge,
                    ConfigDiagnosticCode::TooLarge,
                );
                sources.push(record);
                continue;
            }
            SourceRead::Unreadable => {
                fail(
                    &mut record,
                    &mut diagnostics,
                    ConfigSourceStatus::Unreadable,
                    ConfigDiagnosticCode::Unreadable,
                );
                sources.push(record);
                continue;
            }
            SourceRead::Bytes(bytes) => bytes,
        };

        // Non-UTF-8 is a format failure: the status vocabulary is fixed, and a
        // file the parser cannot even see as text is not a valid document.
        let text = match std::str::from_utf8(&bytes) {
            Ok(text) => text,
            Err(_) => {
                fail(
                    &mut record,
                    &mut diagnostics,
                    ConfigSourceStatus::InvalidFormat,
                    ConfigDiagnosticCode::InvalidFormat,
                );
                sources.push(record);
                continue;
            }
        };

        let keys = allowlist(source.agent);
        let extracted = match source.format {
            ConfigFormat::Toml => extract_toml(text, keys),
            ConfigFormat::Json => extract_json(text, keys),
        };
        let extracted = match extracted {
            Ok(extracted) => extracted,
            Err(()) => {
                fail(
                    &mut record,
                    &mut diagnostics,
                    ConfigSourceStatus::InvalidFormat,
                    ConfigDiagnosticCode::InvalidFormat,
                );
                sources.push(record);
                continue;
            }
        };

        record.status = ConfigSourceStatus::Available;
        record.fingerprint = Some(fingerprint(&bytes));
        record.has_unexposed_fields = extracted.has_unexposed_fields;

        for canonical in &extracted.rejected {
            diagnostics.push(ConfigDiagnosticDto {
                code: ConfigDiagnosticCode::InvalidAllowedValue,
                agent: source.agent,
                scope: source.scope,
                project_id: project_id.clone(),
                source_id: source.id.to_string(),
                key: Some(canonical),
            });
        }

        for (key, value) in extracted.settings {
            settings.push(ConfigSettingDto {
                agent: source.agent,
                canonical_key: key.canonical.to_string(),
                native_key: key.native.to_string(),
                value,
                scope: source.scope,
                source_id: source.id.to_string(),
                project_id: project_id.clone(),
                // Replaced by `apply_resolution` once every source is in.
                resolution: ConfigResolution::ObservedActive,
            });
        }

        sources.push(record);
    }

    apply_resolution(&mut settings);
    let diffs = build_diffs(&settings, &sources);

    ConfigProfileInventoryDto {
        sources,
        settings,
        diffs,
        diagnostics,
        selected_project_id,
        generated_at,
    }
}

/// Labels each setting against the supported sources alone.
///
/// Codex project values stay `project_candidate` in both directions: Codex loads
/// them only for a trusted directory, so neither claiming they are active nor
/// marking the user value overridden would be true.
pub fn apply_resolution(settings: &mut [ConfigSettingDto]) {
    let keys: Vec<(ConfigAgent, String)> =
        settings.iter().map(|s| (s.agent, s.canonical_key.clone())).collect();

    for index in 0..settings.len() {
        let (agent, ref key) = keys[index];
        let scope = settings[index].scope;

        if agent == ConfigAgent::Codex && scope != ConfigScope::User {
            settings[index].resolution = ConfigResolution::ProjectCandidate;
            continue;
        }

        let order = scope_order(agent);
        let rank = |scope: ConfigScope| order.iter().position(|value| *value == scope);
        let overridden = keys.iter().enumerate().any(|(other, (other_agent, other_key))| {
            *other_agent == agent
                && other_key == key
                && other != index
                && settings[other].scope != ConfigScope::User
                && agent != ConfigAgent::Codex
                && rank(settings[other].scope) > rank(scope)
        });

        settings[index].resolution = if overridden {
            ConfigResolution::ObservedOverridden
        } else {
            ConfigResolution::ObservedActive
        };
    }
}

/// Compares adjacent supported layers of the same Agent.
///
/// Adjacent rather than every pair: the pairs the user is asking about are the
/// steps of the precedence chain, and a full cross product would repeat the same
/// facts under a different heading.
///
/// `sources` decides which pairs exist at all. Comparing against a layer that
/// was never read — no Project selected, a scope filter, a file that failed to
/// parse — would report every value on the other side as `removed`, which reads
/// as "the Project deletes this setting" when the truth is that nothing was
/// looked at.
pub fn build_diffs(
    settings: &[ConfigSettingDto],
    sources: &[ConfigSourceDto],
) -> Vec<ConfigDiffEntryDto> {
    let mut diffs = Vec::new();
    let comparable = |agent: ConfigAgent, scope: ConfigScope| {
        sources.iter().any(|source| {
            source.agent == agent
                && source.scope == scope
                && source.status == ConfigSourceStatus::Available
        })
    };

    for agent in [ConfigAgent::Codex, ConfigAgent::ClaudeCode] {
        let order = scope_order(agent);
        for pair in order.windows(2) {
            let (base_scope, compare_scope) = (pair[0], pair[1]);
            if !comparable(agent, base_scope) || !comparable(agent, compare_scope) {
                continue;
            }
            let layer = |scope: ConfigScope| -> Vec<&ConfigSettingDto> {
                settings.iter().filter(|s| s.agent == agent && s.scope == scope).collect()
            };
            let base = layer(base_scope);
            let compare = layer(compare_scope);
            if base.is_empty() && compare.is_empty() {
                continue;
            }

            let mut canonical_keys: Vec<&str> = base
                .iter()
                .chain(compare.iter())
                .map(|s| s.canonical_key.as_str())
                .collect();
            canonical_keys.sort_unstable();
            canonical_keys.dedup();

            for key in canonical_keys {
                let base_value = base
                    .iter()
                    .find(|s| s.canonical_key == key)
                    .map(|s| s.value.clone());
                let compare_value = compare
                    .iter()
                    .find(|s| s.canonical_key == key)
                    .map(|s| s.value.clone());
                let status = match (&base_value, &compare_value) {
                    (Some(left), Some(right)) if left == right => ConfigDiffStatus::Same,
                    (Some(_), Some(_)) => ConfigDiffStatus::Changed,
                    (None, Some(_)) => ConfigDiffStatus::Added,
                    (Some(_), None) => ConfigDiffStatus::Removed,
                    (None, None) => unreachable!("key came from one of the two layers"),
                };
                diffs.push(ConfigDiffEntryDto {
                    agent,
                    canonical_key: key.to_string(),
                    base_scope,
                    compare_scope,
                    status,
                    base_value,
                    compare_value,
                });
            }
        }
    }

    diffs
}

/// Resolves the request and inspects the fixed sources.
///
/// `lookup` returns a registered Project's root. It is a parameter so the
/// boundary is testable without a database, and so this function can never turn
/// a caller-supplied string into a path on its own.
pub fn build_inventory(
    home: Option<&Path>,
    request: &ConfigInventoryRequest,
    generated_at: i64,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<ConfigProfileInventoryDto, ConfigInventoryError> {
    let project_root = resolve_project_root(request.project_id.as_deref(), lookup)?;
    Ok(collect(
        home,
        project_root.as_deref(),
        request.project_id.clone(),
        request,
        generated_at,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const NOW: i64 = 1_760_000_000;

    fn request(project_id: Option<&str>) -> ConfigInventoryRequest {
        ConfigInventoryRequest {
            project_id: project_id.map(str::to_string),
            agent: None,
            scope: None,
        }
    }

    fn write(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().expect("path has a parent")).expect("create parent");
        fs::write(path, contents).expect("write source");
    }

    fn read_probe() -> usize {
        READ_PROBE.with(|count| count.get())
    }

    fn reset_probe() {
        READ_PROBE.with(|count| count.set(0));
    }

    // ── 1.1 Fixed sources and Project identity ──

    #[test]
    fn fixed_sources_user_only_probes_two_user_paths() {
        let home = Path::new("/tmp/agentdeck-home");
        let paths: Vec<PathBuf> =
            source_descriptors(home, None).into_iter().map(|s| s.path).collect();

        assert_eq!(
            paths,
            vec![
                home.join(".codex").join("config.toml"),
                home.join(".claude").join("settings.json"),
            ]
        );
    }

    #[test]
    fn fixed_sources_registered_project_adds_exactly_three_paths() {
        let home = Path::new("/tmp/agentdeck-home");
        let root = Path::new("/tmp/demo");
        let project: Vec<PathBuf> = source_descriptors(home, Some(root))
            .into_iter()
            .filter(|s| s.scope != ConfigScope::User)
            .map(|s| s.path)
            .collect();

        assert_eq!(
            project,
            vec![
                root.join(".codex").join("config.toml"),
                root.join(".claude").join("settings.json"),
                root.join(".claude").join("settings.local.json"),
            ]
        );
    }

    #[test]
    fn fixed_sources_project_root_comes_from_the_stored_record() {
        let root = resolve_project_root(Some("project-1"), |id| {
            assert_eq!(id, "project-1");
            Some("/tmp/demo".to_string())
        })
        .expect("known project resolves");

        assert_eq!(root, Some(PathBuf::from("/tmp/demo")));
    }

    #[test]
    fn fixed_sources_unknown_project_is_rejected_before_reading() {
        let home = tempfile::tempdir().expect("temp home");
        write(&home.path().join(".codex").join("config.toml"), "model = \"gpt-5\"\n");
        reset_probe();

        let outcome = build_inventory(Some(home.path()), &request(Some("nope")), NOW, |_| None);

        assert_eq!(outcome.unwrap_err(), ConfigInventoryError::ProjectNotFound);
        assert_eq!(read_probe(), 0, "a rejected project id must read nothing");
    }

    #[test]
    fn fixed_sources_request_rejects_any_path_bearing_field() {
        for payload in [
            r#"{"projectId":"p","path":"/etc/passwd"}"#,
            r#"{"cwd":"/tmp"}"#,
            r#"{"env":{"HOME":"/tmp"}}"#,
            r#"{"home":"/tmp"}"#,
        ] {
            assert!(
                serde_json::from_str::<ConfigInventoryRequest>(payload).is_err(),
                "request must not accept {payload}"
            );
        }
        assert!(serde_json::from_str::<ConfigInventoryRequest>(r#"{"projectId":"p"}"#).is_ok());
    }

    #[test]
    fn fixed_sources_user_scope_records_carry_no_project_id() {
        let home = tempfile::tempdir().expect("temp home");
        let project = tempfile::tempdir().expect("temp project");
        write(&home.path().join(".codex").join("config.toml"), "model = \"gpt-5\"\n");

        let root = project.path().to_string_lossy().to_string();
        let inventory =
            build_inventory(Some(home.path()), &request(Some("project-1")), NOW, |_| Some(root.clone()))
                .expect("known project");

        for source in &inventory.sources {
            match source.scope {
                ConfigScope::User => assert_eq!(source.project_id, None),
                _ => assert_eq!(source.project_id.as_deref(), Some("project-1")),
            }
        }
    }

    // ── 1.2 Bounded parser and source isolation ──

    /// The two user sources under one temporary home, with no project scope.
    fn user_inventory(home: &Path) -> ConfigProfileInventoryDto {
        build_inventory(Some(home), &request(None), NOW, |_| None).expect("no project id")
    }

    fn source<'a>(
        inventory: &'a ConfigProfileInventoryDto,
        id: &str,
    ) -> &'a ConfigSourceDto {
        inventory.sources.iter().find(|s| s.id == id).expect("source is in the response")
    }

    const CODEX_USER: &str = "codex:user:config-toml";
    const CLAUDE_USER: &str = "claude_code:user:settings-json";

    fn toml_of_exact_size(size: usize) -> String {
        // `model = "…"` plus padding, so the file is valid TOML at an exact byte
        // count.
        let prefix = "model = \"";
        let suffix = "\"\n";
        let padding = size - prefix.len() - suffix.len();
        format!("{prefix}{}{suffix}", "a".repeat(padding))
    }

    #[test]
    fn source_isolation_missing_source_stays_non_fatal() {
        let home = tempfile::tempdir().expect("temp home");
        write(&home.path().join(".claude").join("settings.json"), r#"{"model":"opus"}"#);

        let inventory = user_inventory(home.path());

        assert_eq!(source(&inventory, CODEX_USER).status, ConfigSourceStatus::Missing);
        assert_eq!(source(&inventory, CLAUDE_USER).status, ConfigSourceStatus::Available);
        assert_eq!(inventory.settings.len(), 1);
    }

    #[test]
    fn source_isolation_one_mebibyte_is_still_read() {
        let home = tempfile::tempdir().expect("temp home");
        let content = toml_of_exact_size(MAX_SOURCE_BYTES as usize);
        assert_eq!(content.len() as u64, MAX_SOURCE_BYTES);
        write(&home.path().join(".codex").join("config.toml"), &content);

        let inventory = user_inventory(home.path());

        assert_eq!(source(&inventory, CODEX_USER).status, ConfigSourceStatus::Available);
    }

    #[test]
    fn source_isolation_above_one_mebibyte_is_not_parsed() {
        let home = tempfile::tempdir().expect("temp home");
        let content = toml_of_exact_size(MAX_SOURCE_BYTES as usize + 1);
        write(&home.path().join(".codex").join("config.toml"), &content);

        let inventory = user_inventory(home.path());

        let record = source(&inventory, CODEX_USER);
        assert_eq!(record.status, ConfigSourceStatus::TooLarge);
        assert_eq!(record.fingerprint, None);
        assert!(inventory.settings.is_empty(), "an oversized source yields no settings");
        assert_eq!(inventory.diagnostics[0].code, ConfigDiagnosticCode::TooLarge);
    }

    #[test]
    fn source_isolation_bounded_read_enforces_limit() {
        let dir = tempfile::tempdir().expect("temp dir");

        // Exactly at the limit: accepted with exact byte count.
        let at_limit = dir.path().join("at_limit.toml");
        let body = toml_of_exact_size(MAX_SOURCE_BYTES as usize);
        fs::write(&at_limit, &body).expect("write");
        match read_source(&at_limit) {
            SourceRead::Bytes(bytes) => assert_eq!(bytes.len() as u64, MAX_SOURCE_BYTES),
            _ => panic!("a file at exactly MAX_SOURCE_BYTES must be accepted"),
        }

        // One byte over: rejected by the bounded read, not only by metadata.
        let over = dir.path().join("over.toml");
        let body = toml_of_exact_size(MAX_SOURCE_BYTES as usize + 1);
        fs::write(&over, &body).expect("write");
        assert!(
            matches!(read_source(&over), SourceRead::TooLarge),
            "a file exceeding MAX_SOURCE_BYTES must be TooLarge"
        );

        // Well under the limit: content is preserved exactly.
        let small = dir.path().join("small.toml");
        let small_body = "model = \"gpt-5\"\n";
        fs::write(&small, small_body).expect("write");
        match read_source(&small) {
            SourceRead::Bytes(bytes) => assert_eq!(bytes, small_body.as_bytes()),
            _ => panic!("a small file must be accepted"),
        }
    }

    #[test]
    fn source_isolation_non_file_is_unreadable() {
        let home = tempfile::tempdir().expect("temp home");
        // A directory at the source path: deterministic on every platform, unlike
        // a permission bit that root would ignore.
        fs::create_dir_all(home.path().join(".codex").join("config.toml")).expect("create dir");

        let inventory = user_inventory(home.path());

        assert_eq!(source(&inventory, CODEX_USER).status, ConfigSourceStatus::Unreadable);
        assert_eq!(inventory.diagnostics[0].code, ConfigDiagnosticCode::Unreadable);
    }

    #[cfg(unix)]
    #[test]
    fn source_isolation_symlink_target_is_never_read() {
        let home = tempfile::tempdir().expect("temp home");
        let elsewhere = home.path().join("elsewhere.toml");
        write(&elsewhere, "model = \"leaked\"\n");
        let link = home.path().join(".codex").join("config.toml");
        fs::create_dir_all(link.parent().expect("parent")).expect("create parent");
        std::os::unix::fs::symlink(&elsewhere, &link).expect("create symlink");

        let inventory = user_inventory(home.path());

        let record = source(&inventory, CODEX_USER);
        assert_eq!(record.status, ConfigSourceStatus::UnsupportedSymlink);
        assert_eq!(record.fingerprint, None);
        let json = serde_json::to_string(&inventory).expect("serialize");
        assert!(!json.contains("leaked"), "the link target must not be read");
    }

    #[test]
    fn source_isolation_invalid_format_does_not_disable_other_sources() {
        let home = tempfile::tempdir().expect("temp home");
        write(&home.path().join(".codex").join("config.toml"), "model = = \"gpt-5\"\n");
        write(&home.path().join(".claude").join("settings.json"), r#"{"model":"opus"}"#);

        let inventory = user_inventory(home.path());

        assert_eq!(source(&inventory, CODEX_USER).status, ConfigSourceStatus::InvalidFormat);
        assert_eq!(source(&inventory, CLAUDE_USER).status, ConfigSourceStatus::Available);
        assert_eq!(inventory.settings.len(), 1);
        assert_eq!(inventory.diagnostics.len(), 1);
        assert_eq!(inventory.diagnostics[0].code, ConfigDiagnosticCode::InvalidFormat);
    }

    #[test]
    fn source_isolation_invalid_json_is_isolated_too() {
        let home = tempfile::tempdir().expect("temp home");
        write(&home.path().join(".codex").join("config.toml"), "model = \"gpt-5\"\n");
        write(&home.path().join(".claude").join("settings.json"), "{\"model\": }");

        let inventory = user_inventory(home.path());

        assert_eq!(source(&inventory, CLAUDE_USER).status, ConfigSourceStatus::InvalidFormat);
        assert_eq!(source(&inventory, CODEX_USER).status, ConfigSourceStatus::Available);
    }

    #[test]
    fn source_isolation_non_utf8_is_an_invalid_format() {
        let home = tempfile::tempdir().expect("temp home");
        let path = home.path().join(".claude").join("settings.json");
        fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
        fs::write(&path, [0xff, 0xfe, 0x00]).expect("write bytes");

        let inventory = user_inventory(home.path());

        assert_eq!(source(&inventory, CLAUDE_USER).status, ConfigSourceStatus::InvalidFormat);
    }

    #[test]
    fn source_isolation_never_executes_an_agent_cli() {
        // The needles are split so this assertion does not match itself.
        let module = include_str!("config_profile_inventory.rs");
        for needle in [
            concat!("std::", "process"),
            concat!("Comm", "and::new"),
            concat!("tauri_plugin_", "shell"),
        ] {
            assert!(!module.contains(needle), "this module must not reach for {needle}");
        }
    }

    // ── 1.3 Explicit scalar allowlist and DTO boundary ──

    fn setting<'a>(
        inventory: &'a ConfigProfileInventoryDto,
        agent: ConfigAgent,
        canonical: &str,
    ) -> Option<&'a ConfigSettingDto> {
        inventory
            .settings
            .iter()
            .find(|s| s.agent == agent && s.canonical_key == canonical)
    }

    #[test]
    fn dto_boundary_normalizes_each_allowed_scalar_type() {
        let home = tempfile::tempdir().expect("temp home");
        write(
            &home.path().join(".codex").join("config.toml"),
            "sandbox_mode = \"read-only\"\nmodel_reasoning_effort = \"high\"\n",
        );
        write(
            &home.path().join(".claude").join("settings.json"),
            r#"{"alwaysThinkingEnabled":true,"cleanupPeriodDays":20}"#,
        );

        let inventory = user_inventory(home.path());

        for (agent, canonical, native, expected) in [
            (
                ConfigAgent::Codex,
                "sandbox_mode",
                "sandbox_mode",
                ConfigValueDto::String("read-only".to_string()),
            ),
            (
                ConfigAgent::Codex,
                "model_reasoning_effort",
                "model_reasoning_effort",
                ConfigValueDto::String("high".to_string()),
            ),
            (
                ConfigAgent::ClaudeCode,
                "always_thinking_enabled",
                "alwaysThinkingEnabled",
                ConfigValueDto::Boolean(true),
            ),
            (
                ConfigAgent::ClaudeCode,
                "cleanup_period_days",
                "cleanupPeriodDays",
                ConfigValueDto::Integer(20),
            ),
        ] {
            let found = setting(&inventory, agent, canonical)
                .unwrap_or_else(|| panic!("{canonical} is normalized"));
            assert_eq!(found.value, expected);
            assert_eq!(found.native_key, native);
            assert_eq!(found.scope, ConfigScope::User);
            assert_eq!(found.source_id.is_empty(), false);
            assert_eq!(found.project_id, None);
        }
    }

    #[test]
    fn dto_boundary_reads_the_nested_claude_permission_mode() {
        let home = tempfile::tempdir().expect("temp home");
        write(
            &home.path().join(".claude").join("settings.json"),
            r#"{"permissions":{"defaultMode":"plan","allow":["Bash(rm:*)"]}}"#,
        );

        let inventory = user_inventory(home.path());

        let found = setting(&inventory, ConfigAgent::ClaudeCode, "permission_default_mode")
            .expect("nested allowlist entry is normalized");
        assert_eq!(found.value, ConfigValueDto::String("plan".to_string()));
        assert_eq!(found.native_key, "permissions.defaultMode");

        let json = serde_json::to_string(&inventory).expect("serialize");
        assert!(!json.contains("allow"), "a permission rule list must not cross the boundary");
        assert!(!json.contains("rm:*"));
    }

    #[test]
    fn dto_boundary_invalid_allowed_value_fails_closed() {
        let home = tempfile::tempdir().expect("temp home");
        write(
            &home.path().join(".claude").join("settings.json"),
            r#"{"model":"sonnet","cleanupPeriodDays":"thirty"}"#,
        );

        let inventory = user_inventory(home.path());

        assert!(setting(&inventory, ConfigAgent::ClaudeCode, "cleanup_period_days").is_none());
        assert!(setting(&inventory, ConfigAgent::ClaudeCode, "model").is_some());
        let diagnostic = &inventory.diagnostics[0];
        assert_eq!(diagnostic.code, ConfigDiagnosticCode::InvalidAllowedValue);
        assert_eq!(diagnostic.key, Some("cleanup_period_days"));
        let json = serde_json::to_string(&inventory).expect("serialize");
        assert!(!json.contains("thirty"), "the rejected value must not be echoed back");
    }

    #[test]
    fn dto_boundary_codex_approval_policy_accepts_only_the_string_form() {
        let home = tempfile::tempdir().expect("temp home");
        write(
            &home.path().join(".codex").join("config.toml"),
            "[approval_policy]\nmode = \"on-request\"\n",
        );

        let inventory = user_inventory(home.path());

        assert!(setting(&inventory, ConfigAgent::Codex, "approval_policy").is_none());
        assert_eq!(inventory.diagnostics[0].key, Some("approval_policy"));
        let json = serde_json::to_string(&inventory).expect("serialize");
        assert!(!json.contains("on-request"));
    }

    #[test]
    fn dto_boundary_secret_bearing_source_exposes_only_the_allowlisted_value() {
        let home = tempfile::tempdir().expect("temp home");
        write(
            &home.path().join(".claude").join("settings.json"),
            r#"{"model":"sonnet","env":{"ANTHROPIC_API_KEY":"secret-123"}}"#,
        );

        let inventory = user_inventory(home.path());
        let json = serde_json::to_string(&inventory).expect("serialize");

        assert!(json.contains("sonnet"));
        for forbidden in ["ANTHROPIC_API_KEY", "secret-123", "env"] {
            assert!(!json.contains(forbidden), "{forbidden} must not cross the boundary");
        }
        assert!(source(&inventory, CLAUDE_USER).has_unexposed_fields);
    }

    #[test]
    fn dto_boundary_unknown_keys_never_reach_the_response() {
        let home = tempfile::tempdir().expect("temp home");
        write(
            &home.path().join(".codex").join("config.toml"),
            "model = \"gpt-5\"\n[mcp_servers.local]\ncommand = \"/usr/bin/secret-helper\"\n",
        );

        let inventory = user_inventory(home.path());
        let json = serde_json::to_string(&inventory).expect("serialize");

        assert!(json.contains("gpt-5"));
        for forbidden in ["mcp_servers", "secret-helper", "command"] {
            assert!(!json.contains(forbidden), "{forbidden} must not cross the boundary");
        }
        assert!(source(&inventory, CODEX_USER).has_unexposed_fields);
    }

    #[test]
    fn dto_boundary_fully_exposed_source_reports_no_unexposed_fields() {
        let home = tempfile::tempdir().expect("temp home");
        write(&home.path().join(".claude").join("settings.json"), r#"{"model":"opus"}"#);

        let inventory = user_inventory(home.path());

        assert!(!source(&inventory, CLAUDE_USER).has_unexposed_fields);
    }

    #[test]
    fn dto_boundary_fingerprint_exists_only_for_available_sources() {
        let home = tempfile::tempdir().expect("temp home");
        let claude = home.path().join(".claude").join("settings.json");
        let body = r#"{"model":"opus"}"#;
        write(&claude, body);

        let inventory = user_inventory(home.path());

        let available = source(&inventory, CLAUDE_USER);
        assert_eq!(available.status, ConfigSourceStatus::Available);
        assert_eq!(available.fingerprint.as_deref(), Some(fingerprint(body.as_bytes()).as_str()));
        assert_eq!(source(&inventory, CODEX_USER).status, ConfigSourceStatus::Missing);
        assert_eq!(source(&inventory, CODEX_USER).fingerprint, None);
    }

    #[test]
    fn dto_boundary_response_carries_no_parser_or_os_error_text() {
        let home = tempfile::tempdir().expect("temp home");
        write(&home.path().join(".claude").join("settings.json"), "{oops");
        fs::create_dir_all(home.path().join(".codex").join("config.toml")).expect("create dir");

        let inventory = user_inventory(home.path());
        let json = serde_json::to_string(&inventory).expect("serialize");

        for forbidden in ["oops", "expected", "line ", "column ", "os error", "Permission"] {
            assert!(!json.contains(forbidden), "{forbidden} must not cross the boundary");
        }
    }

    // ── 1.4 Supported-source precedence and read-only diff ──

    /// A temporary home plus a registered project root, inspected together.
    struct Layered {
        _home: tempfile::TempDir,
        _project: tempfile::TempDir,
        inventory: ConfigProfileInventoryDto,
    }

    fn layered(
        codex_user: Option<&str>,
        codex_project: Option<&str>,
        claude_user: Option<&str>,
        claude_project: Option<&str>,
        claude_local: Option<&str>,
    ) -> Layered {
        let home = tempfile::tempdir().expect("temp home");
        let project = tempfile::tempdir().expect("temp project");
        for (body, path) in [
            (codex_user, home.path().join(".codex").join("config.toml")),
            (codex_project, project.path().join(".codex").join("config.toml")),
            (claude_user, home.path().join(".claude").join("settings.json")),
            (claude_project, project.path().join(".claude").join("settings.json")),
            (claude_local, project.path().join(".claude").join("settings.local.json")),
        ] {
            if let Some(body) = body {
                write(&path, body);
            }
        }
        let root = project.path().to_string_lossy().to_string();
        let inventory =
            build_inventory(Some(home.path()), &request(Some("project-1")), NOW, |_| Some(root.clone()))
                .expect("known project");
        Layered { _home: home, _project: project, inventory }
    }

    fn resolutions(
        inventory: &ConfigProfileInventoryDto,
        agent: ConfigAgent,
        canonical: &str,
    ) -> Vec<(ConfigScope, ConfigResolution)> {
        inventory
            .settings
            .iter()
            .filter(|s| s.agent == agent && s.canonical_key == canonical)
            .map(|s| (s.scope, s.resolution))
            .collect()
    }

    fn diff_status(
        inventory: &ConfigProfileInventoryDto,
        agent: ConfigAgent,
        canonical: &str,
        base: ConfigScope,
        compare: ConfigScope,
    ) -> ConfigDiffStatus {
        inventory
            .diffs
            .iter()
            .find(|d| {
                d.agent == agent
                    && d.canonical_key == canonical
                    && d.base_scope == base
                    && d.compare_scope == compare
            })
            .unwrap_or_else(|| panic!("{canonical} is compared between {base:?} and {compare:?}"))
            .status
    }

    #[test]
    fn supported_precedence_claude_local_overrides_project_and_user() {
        let layers = layered(
            None,
            None,
            Some(r#"{"model":"sonnet"}"#),
            Some(r#"{"model":"opus"}"#),
            Some(r#"{"model":"haiku"}"#),
        );

        let mut observed = resolutions(&layers.inventory, ConfigAgent::ClaudeCode, "model");
        observed.sort_by_key(|(scope, _)| *scope);

        assert_eq!(
            observed,
            vec![
                (ConfigScope::User, ConfigResolution::ObservedOverridden),
                (ConfigScope::Project, ConfigResolution::ObservedOverridden),
                (ConfigScope::ProjectLocal, ConfigResolution::ObservedActive),
            ]
        );
    }

    #[test]
    fn supported_precedence_user_value_is_active_without_a_project() {
        let home = tempfile::tempdir().expect("temp home");
        write(&home.path().join(".claude").join("settings.json"), r#"{"model":"sonnet"}"#);

        let inventory = user_inventory(home.path());

        assert_eq!(
            resolutions(&inventory, ConfigAgent::ClaudeCode, "model"),
            vec![(ConfigScope::User, ConfigResolution::ObservedActive)]
        );
    }

    #[test]
    fn supported_precedence_codex_project_value_stays_a_candidate() {
        let layers = layered(
            Some("sandbox_mode = \"workspace-write\"\n"),
            Some("sandbox_mode = \"read-only\"\n"),
            None,
            None,
            None,
        );

        let mut observed = resolutions(&layers.inventory, ConfigAgent::Codex, "sandbox_mode");
        observed.sort_by_key(|(scope, _)| *scope);

        assert_eq!(
            observed,
            vec![
                (ConfigScope::User, ConfigResolution::ObservedActive),
                (ConfigScope::Project, ConfigResolution::ProjectCandidate),
            ]
        );
    }

    #[test]
    fn supported_precedence_diff_reports_every_normalized_status() {
        let layers = layered(
            None,
            None,
            Some(r#"{"model":"sonnet","fastMode":true}"#),
            Some(r#"{"model":"opus","cleanupPeriodDays":20}"#),
            None,
        );
        let inventory = &layers.inventory;

        // model in both, different → changed; fastMode only in user → removed;
        // cleanupPeriodDays only in project → added.
        assert_eq!(
            diff_status(
                inventory,
                ConfigAgent::ClaudeCode,
                "model",
                ConfigScope::User,
                ConfigScope::Project
            ),
            ConfigDiffStatus::Changed
        );
        assert_eq!(
            diff_status(
                inventory,
                ConfigAgent::ClaudeCode,
                "fast_mode",
                ConfigScope::User,
                ConfigScope::Project
            ),
            ConfigDiffStatus::Removed
        );
        assert_eq!(
            diff_status(
                inventory,
                ConfigAgent::ClaudeCode,
                "cleanup_period_days",
                ConfigScope::User,
                ConfigScope::Project
            ),
            ConfigDiffStatus::Added
        );
    }

    #[test]
    fn supported_precedence_identical_layers_are_same() {
        let layers = layered(
            Some("model = \"gpt-5\"\n"),
            Some("model = \"gpt-5\"\n"),
            None,
            None,
            None,
        );

        assert_eq!(
            diff_status(
                &layers.inventory,
                ConfigAgent::Codex,
                "model",
                ConfigScope::User,
                ConfigScope::Project
            ),
            ConfigDiffStatus::Same
        );
    }

    #[test]
    fn supported_precedence_diff_carries_no_raw_document_text() {
        let layers = layered(
            None,
            None,
            Some(r#"{"model":"sonnet","env":{"TOKEN":"user-secret"}}"#),
            Some(r#"{"model":"opus","env":{"TOKEN":"project-secret"}}"#),
            None,
        );

        let json = serde_json::to_string(&layers.inventory.diffs).expect("serialize");

        assert!(json.contains("sonnet") && json.contains("opus"));
        for forbidden in ["TOKEN", "user-secret", "project-secret", "{\\\"model"] {
            assert!(!json.contains(forbidden), "{forbidden} must not appear in a diff");
        }
    }

    #[test]
    fn supported_precedence_uninspected_layer_is_never_compared() {
        // No Project is selected, so the project layer was never read. Reporting
        // the user settings as `removed` would claim a deletion that no file
        // states.
        let home = tempfile::tempdir().expect("temp home");
        write(&home.path().join(".codex").join("config.toml"), "model = \"gpt-5\"\n");
        write(&home.path().join(".claude").join("settings.json"), r#"{"model":"sonnet"}"#);

        let inventory = user_inventory(home.path());

        assert!(
            inventory.diffs.is_empty(),
            "a layer that was not inspected must not appear in the diff"
        );
    }

    #[test]
    fn supported_precedence_failed_layer_is_never_compared() {
        // The project source exists but cannot be parsed. Its status already
        // says so; a diff claiming every value was removed would say something
        // the file does not.
        let layers = layered(
            None,
            None,
            Some(r#"{"model":"sonnet"}"#),
            Some("{broken"),
            None,
        );

        assert!(
            !layers
                .inventory
                .diffs
                .iter()
                .any(|d| d.compare_scope == ConfigScope::Project),
            "an unparsable layer must not be compared"
        );
    }

    #[test]
    fn supported_precedence_agents_never_override_each_other() {
        let layers = layered(
            Some("model = \"gpt-5\"\n"),
            None,
            Some(r#"{"model":"sonnet"}"#),
            None,
            None,
        );

        assert_eq!(
            resolutions(&layers.inventory, ConfigAgent::Codex, "model"),
            vec![(ConfigScope::User, ConfigResolution::ObservedActive)]
        );
        assert_eq!(
            resolutions(&layers.inventory, ConfigAgent::ClaudeCode, "model"),
            vec![(ConfigScope::User, ConfigResolution::ObservedActive)]
        );
    }

    // ── 1.5 No persistent side effects ──

    /// Every file under `root`, as relative path plus contents.
    ///
    /// Contents rather than mtime: a repair or a reformat that happened to keep
    /// the timestamp would still show up here.
    fn snapshot(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut entries: Vec<(PathBuf, Vec<u8>)> = walkdir::WalkDir::new(root)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .map(|entry| {
                let relative = entry
                    .path()
                    .strip_prefix(root)
                    .expect("entry is under root")
                    .to_path_buf();
                (relative, fs::read(entry.path()).expect("read snapshot file"))
            })
            .collect();
        entries.sort();
        entries
    }

    #[test]
    fn no_persistent_side_effects_load_and_refresh_change_nothing() {
        let home = tempfile::tempdir().expect("temp home");
        let project = tempfile::tempdir().expect("temp project");
        let state = tempfile::tempdir().expect("temp application state");

        write(&home.path().join(".codex").join("config.toml"), "model = \"gpt-5\"\n");
        write(
            &home.path().join(".claude").join("settings.json"),
            r#"{"model":"sonnet","env":{"TOKEN":"secret"}}"#,
        );
        write(&project.path().join(".codex").join("config.toml"), "sandbox_mode = \"read-only\"\n");
        write(&project.path().join(".claude").join("settings.json"), r#"{"model":"opus"}"#);
        // A malformed source is the case most likely to tempt a repair.
        write(&project.path().join(".claude").join("settings.local.json"), "{broken");
        // Stand-ins for the Library, the SQLite database and Application Support
        // state: this capability is given no handle to any of them.
        write(&state.path().join("Library").join("skills").join("demo.md"), "# demo\n");
        write(&state.path().join("agentdeck.db"), "sqlite-bytes");
        write(&state.path().join("Application Support").join("settings.json"), "{}");

        let before: Vec<_> =
            [home.path(), project.path(), state.path()].map(snapshot).into_iter().collect();

        let root = project.path().to_string_lossy().to_string();
        for _ in 0..2 {
            build_inventory(Some(home.path()), &request(Some("project-1")), NOW, |_| Some(root.clone()))
                .expect("known project");
        }

        let after: Vec<_> =
            [home.path(), project.path(), state.path()].map(snapshot).into_iter().collect();
        assert_eq!(before, after, "inspection must leave every tree byte-for-byte unchanged");
    }

    #[test]
    fn no_persistent_side_effects_offline_library_is_never_created() {
        let home = tempfile::tempdir().expect("temp home");
        let offline = home.path().join("AgentDeck-Library");
        write(&home.path().join(".claude").join("settings.json"), r#"{"model":"sonnet"}"#);

        let inventory = user_inventory(home.path());

        assert_eq!(
            setting(&inventory, ConfigAgent::ClaudeCode, "model").map(|s| s.value.clone()),
            Some(ConfigValueDto::String("sonnet".to_string())),
            "an unavailable Library must not stop fixed-source inspection"
        );
        assert!(!offline.exists(), "no fallback write may create the Library path");
    }

    #[test]
    fn no_persistent_side_effects_missing_source_is_not_created() {
        let home = tempfile::tempdir().expect("temp home");

        user_inventory(home.path());

        assert!(!home.path().join(".codex").exists());
        assert!(!home.path().join(".claude").exists());
    }
}
