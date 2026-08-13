//! Validated, previewable and recoverable Hook edits on fixed sources.
//!
//! Mutation never accepts a filesystem path: a request names an enum-backed
//! source id plus an optional linked Project id, and this module re-resolves the
//! same fixed descriptors the read-only inspector uses. See
//! `openspec/changes/edit-codex-claude-hooks`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use serde_json::{Map as JsonMap, Value as JsonValue};
use toml_edit::{
    Array as TomlArray, ArrayOfTables as TomlArrayOfTables, DocumentMut, InlineTable as TomlInlineTable,
    Item as TomlItem, Table as TomlTable, Value as TomlValue,
};

use crate::core::artifact::{ArtifactKind, ArtifactRecord, HookBackupKind, HookContext};
use crate::core::hook_inspection::{self, HookAgent, HookFieldShape, HookFormat, HookScope};
use crate::core::skill_store::{HookBackupRecord, HookDetailRecord, SkillStore};

/// Serializes every Hook mutation in this process.
static HOOK_WRITE_LOCK: Mutex<()> = Mutex::new(());

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

/// Every failure a Hook mutation can report. The frontend branches on the
/// snake_case string, so these variants are part of the IPC contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookWriteError {
    InvalidProject,
    InvalidSource,
    SourceOffline,
    UnsupportedSourceType,
    InvalidHookDraft,
    StaleDraft,
    SourceConflict,
    PreviewTooLarge,
    BackupFailed,
    AtomicReplaceUnsupported,
    WriteFailed,
    RestoreFailed,
    RecoveryRequired,
}

impl HookWriteError {
    pub fn as_str(&self) -> &'static str {
        match self {
            HookWriteError::InvalidProject => "invalid_project",
            HookWriteError::InvalidSource => "invalid_source",
            HookWriteError::SourceOffline => "source_offline",
            HookWriteError::UnsupportedSourceType => "unsupported_source_type",
            HookWriteError::InvalidHookDraft => "invalid_hook_draft",
            HookWriteError::StaleDraft => "stale_draft",
            HookWriteError::SourceConflict => "source_conflict",
            HookWriteError::PreviewTooLarge => "preview_too_large",
            HookWriteError::BackupFailed => "backup_failed",
            HookWriteError::AtomicReplaceUnsupported => "atomic_replace_unsupported",
            HookWriteError::WriteFailed => "write_failed",
            HookWriteError::RestoreFailed => "restore_failed",
            HookWriteError::RecoveryRequired => "recovery_required",
        }
    }
}

/// Whether the resolved target already exists. Both states are writable; every
/// other file type is refused before any mutation starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookTargetState {
    /// The fixed source is a regular file.
    Existing,
    /// The fixed source is absent under an existing root, so it can be created.
    Missing,
}

/// A resolved, writable Hook source. Only this module produces one, and only
/// from a fixed descriptor — never from caller-supplied text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookWriteTarget {
    pub source_id: String,
    pub agent: HookAgent,
    pub scope: HookScope,
    pub format: HookFormat,
    pub path: PathBuf,
    pub state: HookTargetState,
}

// ---------------------------------------------------------------------------
// Fixed source capability
// ---------------------------------------------------------------------------

/// Resolves a mutation request to a writable fixed source.
///
/// `home` is optional and `lookup` maps a linked Project id to its root, exactly
/// as on the read path: an unresolved home or Project must fail rather than fall
/// back to the process working directory, which would write to files the user
/// never linked.
///
/// Resolution is read-only. It creates no directory, no file and no database
/// row, so a refused request leaves the disk untouched.
pub fn resolve_write_target(
    home: Option<&Path>,
    project_id: Option<&str>,
    source_id: &str,
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<HookWriteTarget, HookWriteError> {
    let project_root = match project_id {
        None => None,
        Some(id) => Some(PathBuf::from(
            lookup(id).ok_or(HookWriteError::InvalidProject)?,
        )),
    };

    // An unresolved home still enumerates the user descriptors, so that a user
    // source resolves to `source_offline` below rather than to `invalid_source`:
    // the source id is real, its root is not.
    let home_base = home.unwrap_or_else(|| Path::new("~"));
    let descriptor = hook_inspection::source_descriptors(home_base, project_root.as_deref())
        .into_iter()
        .find(|descriptor| descriptor.id == source_id)
        .ok_or(HookWriteError::InvalidSource)?;

    let root: &Path = match descriptor.scope {
        HookScope::User => match home {
            None => return Err(HookWriteError::SourceOffline),
            Some(home) => home,
        },
        HookScope::Project | HookScope::ProjectLocal => project_root
            .as_deref()
            .ok_or(HookWriteError::InvalidSource)?,
    };
    if !root.is_dir() {
        return Err(HookWriteError::SourceOffline);
    }

    // `symlink_metadata` does not follow the link, so a symlinked source is
    // refused instead of being replaced by its target — which would write
    // outside the approved boundary.
    let state = match std::fs::symlink_metadata(&descriptor.path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => HookTargetState::Missing,
        Err(_) => return Err(HookWriteError::UnsupportedSourceType),
        Ok(metadata) if metadata.file_type().is_file() => HookTargetState::Existing,
        Ok(_) => return Err(HookWriteError::UnsupportedSourceType),
    };

    Ok(HookWriteTarget {
        source_id: descriptor.id.to_string(),
        agent: descriptor.agent,
        scope: descriptor.scope,
        format: descriptor.format,
        path: descriptor.path,
        state,
    })
}

// ---------------------------------------------------------------------------
// Edit operations
// ---------------------------------------------------------------------------

/// Where an existing handler lives in the source as it was read.
///
/// The indexes are the ones the inspector reported, so a source edited
/// externally between preview and apply no longer matches and is refused rather
/// than silently rewriting a different handler.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookHandlerLocatorDto {
    pub event: String,
    pub group_index: usize,
    pub handler_index: usize,
}

/// The handler the user wants after the edit.
///
/// `fields` carries the complete set of *known* fields: a known field the user
/// removed is absent here and is removed from the document, while unknown
/// fields are never expressible and therefore never touched.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookHandlerDraftDto {
    pub event: String,
    pub matcher: Option<String>,
    pub handler_type: String,
    pub fields: JsonMap<String, JsonValue>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HookEditOperationDto {
    CreateHandler {
        draft: HookHandlerDraftDto,
    },
    UpdateHandler {
        locator: HookHandlerLocatorDto,
        draft: HookHandlerDraftDto,
    },
    DeleteHandler {
        locator: HookHandlerLocatorDto,
    },
}

/// Why one draft value is not acceptable. The frontend localizes the code and
/// anchors it to `field`; the backend never ships prose.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookIssueCode {
    /// Not in the selected Agent's documented event list.
    UnknownEvent,
    /// Not in the selected Agent's documented handler types.
    UnknownHandlerType,
    /// Not in the selected Agent's documented fields — including a field that
    /// exists in the source but may not be added or changed through the API.
    UnknownField,
    /// Documented field, but the value is not a shape the Agent accepts.
    InvalidValueType,
    /// A value the handler type requires is absent or empty.
    MissingValue,
    /// Not a draft problem: the canonical diff input is past the size gate, so
    /// the preview cannot be shown and Apply stays disabled.
    PreviewTooLarge,
}

/// One field-anchored validation failure, addressed back to the operation that
/// produced it so the editor can highlight the right control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookValidationIssueDto {
    pub operation_index: usize,
    pub field: String,
    pub code: HookIssueCode,
}

/// A complete rewritten document plus the Hook subtree either side of the edit.
///
/// `document_text` is what gets written; only the canonical fields cross the IPC
/// boundary, so non-Hook siblings never reach the frontend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookTransform {
    pub document_text: String,
    pub before_canonical: String,
    pub after_canonical: String,
}

/// Why a transformation did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookTransformFailure {
    /// One or more draft values are not acceptable for this Agent.
    Invalid(Vec<HookValidationIssueDto>),
    /// A locator does not name exactly one handler in the current source.
    Stale,
    /// The source exists but is not parsable, or its Hook subtree is not the
    /// documented shape. Editing it would have to guess at the user's intent.
    Unparsable,
}

impl HookTransformFailure {
    /// The wire code this failure reports.
    pub fn code(&self) -> HookWriteError {
        match self {
            HookTransformFailure::Invalid(_) => HookWriteError::InvalidHookDraft,
            HookTransformFailure::Stale => HookWriteError::StaleDraft,
            HookTransformFailure::Unparsable => HookWriteError::UnsupportedSourceType,
        }
    }
}

/// Validates the operations against the Agent's registry and applies them to the
/// complete source document.
///
/// `source_text` is `None` for a fixed source that does not exist yet. The whole
/// document is rewritten rather than just its Hook subtree, because a JSON
/// sibling or a TOML comment outside `[hooks]` must survive the write untouched.
pub fn transform_source(
    agent: HookAgent,
    format: HookFormat,
    source_text: Option<&str>,
    operations: &[HookEditOperationDto],
) -> Result<HookTransform, HookTransformFailure> {
    let subtree = match source_text {
        None => None,
        Some(text) => Some(
            hook_inspection::parse_hook_subtree(format, text)
                .map_err(|_| HookTransformFailure::Unparsable)?,
        ),
    };
    let before = subtree
        .as_ref()
        .and_then(|subtree| subtree.value.clone())
        .unwrap_or_else(|| JsonValue::Object(JsonMap::new()));

    let issues = validate_operations(agent, operations);
    if !issues.is_empty() {
        return Err(HookTransformFailure::Invalid(issues));
    }
    let mut addressed: Vec<&str> = Vec::new();
    for operation in operations {
        if let Some(locator) = operation_locator(operation) {
            if find_handler(&before, locator).is_none() {
                return Err(HookTransformFailure::Stale);
            }
            // Indexes address the source as it was read, so two operations on
            // one event would make the second address a shifted handler. Rather
            // than reindex, refuse: picking the wrong handler is the failure
            // this whole path exists to prevent.
            if addressed.contains(&locator.event.as_str()) {
                return Err(HookTransformFailure::Stale);
            }
            addressed.push(&locator.event);
        }
    }

    let document_text = match format {
        HookFormat::Json => patch_json(agent, source_text, operations)?,
        HookFormat::Toml => patch_toml(agent, source_text, operations)?,
    };
    let after = hook_inspection::parse_hook_subtree(format, &document_text)
        .map_err(|_| HookTransformFailure::Unparsable)?;

    Ok(HookTransform {
        document_text,
        before_canonical: subtree
            .map(|subtree| subtree.canonical_text)
            .unwrap_or_default(),
        after_canonical: after.canonical_text,
    })
}

// ---------------------------------------------------------------------------
// JSON document patching
// ---------------------------------------------------------------------------

fn patch_json(
    agent: HookAgent,
    source_text: Option<&str>,
    operations: &[HookEditOperationDto],
) -> Result<String, HookTransformFailure> {
    let mut root: JsonValue = match source_text {
        None => JsonValue::Object(JsonMap::new()),
        Some(text) => serde_json::from_str(text).map_err(|_| HookTransformFailure::Unparsable)?,
    };
    let root_map = root
        .as_object_mut()
        .ok_or(HookTransformFailure::Unparsable)?;

    let mut hooks = match root_map.remove("hooks") {
        None => JsonMap::new(),
        Some(JsonValue::Object(map)) => map,
        Some(_) => return Err(HookTransformFailure::Unparsable),
    };
    for operation in operations {
        apply_json_operation(&mut hooks, agent, operation)?;
    }
    root_map.insert("hooks".to_string(), JsonValue::Object(hooks));

    serde_json::to_string_pretty(&root).map_err(|_| HookTransformFailure::Unparsable)
}

/// Builds the handler as it should be written.
///
/// The draft carries the complete set of known fields, so a known field the
/// user cleared disappears; unknown fields are copied over from `existing`
/// because the API has no way to express them.
fn build_json_handler(
    agent: HookAgent,
    draft: &HookHandlerDraftDto,
    existing: Option<&JsonValue>,
) -> JsonValue {
    let mut handler = JsonMap::new();
    handler.insert(
        "type".to_string(),
        JsonValue::String(draft.handler_type.clone()),
    );
    for (name, value) in &draft.fields {
        handler.insert(name.clone(), value.clone());
    }
    if let Some(existing) = existing.and_then(JsonValue::as_object) {
        for (name, value) in existing {
            if name == "type" || hook_inspection::known_fields(agent).contains(&name.as_str()) {
                continue;
            }
            handler.insert(name.clone(), value.clone());
        }
    }
    JsonValue::Object(handler)
}

fn json_group_matcher(group: &JsonValue) -> Option<&str> {
    group.get("matcher").and_then(JsonValue::as_str)
}

/// Appends a handler to the group that carries `matcher`, creating the event
/// and the group when they are not there yet.
fn insert_json_handler(
    hooks: &mut JsonMap<String, JsonValue>,
    draft: &HookHandlerDraftDto,
    handler: JsonValue,
) -> Result<(), HookTransformFailure> {
    let groups = hooks
        .entry(draft.event.clone())
        .or_insert_with(|| JsonValue::Array(Vec::new()))
        .as_array_mut()
        .ok_or(HookTransformFailure::Unparsable)?;

    let position = groups
        .iter()
        .position(|group| json_group_matcher(group) == draft.matcher.as_deref());
    let index = match position {
        Some(index) => index,
        None => {
            let mut group = JsonMap::new();
            if let Some(matcher) = &draft.matcher {
                group.insert("matcher".to_string(), JsonValue::String(matcher.clone()));
            }
            group.insert("hooks".to_string(), JsonValue::Array(Vec::new()));
            groups.push(JsonValue::Object(group));
            groups.len() - 1
        }
    };
    groups[index]
        .get_mut("hooks")
        .and_then(JsonValue::as_array_mut)
        .ok_or(HookTransformFailure::Unparsable)?
        .push(handler);
    Ok(())
}

/// Removes a handler and prunes the group and event it leaves empty.
fn remove_json_handler(
    hooks: &mut JsonMap<String, JsonValue>,
    locator: &HookHandlerLocatorDto,
) -> Result<JsonValue, HookTransformFailure> {
    let stale = || HookTransformFailure::Stale;
    let groups = hooks
        .get_mut(&locator.event)
        .and_then(JsonValue::as_array_mut)
        .ok_or_else(stale)?;
    let group = groups.get_mut(locator.group_index).ok_or_else(stale)?;
    let handlers = group
        .get_mut("hooks")
        .and_then(JsonValue::as_array_mut)
        .ok_or_else(stale)?;
    if locator.handler_index >= handlers.len() {
        return Err(stale());
    }
    let removed = handlers.remove(locator.handler_index);

    if handlers.is_empty() {
        groups.remove(locator.group_index);
    }
    if groups.is_empty() {
        hooks.remove(&locator.event);
    }
    Ok(removed)
}

fn apply_json_operation(
    hooks: &mut JsonMap<String, JsonValue>,
    agent: HookAgent,
    operation: &HookEditOperationDto,
) -> Result<(), HookTransformFailure> {
    match operation {
        HookEditOperationDto::CreateHandler { draft } => {
            let handler = build_json_handler(agent, draft, None);
            insert_json_handler(hooks, draft, handler)
        }
        HookEditOperationDto::DeleteHandler { locator } => {
            remove_json_handler(hooks, locator).map(|_| ())
        }
        HookEditOperationDto::UpdateHandler { locator, draft } => {
            let current = JsonValue::Object(hooks.clone());
            let existing = find_handler(&current, locator)
                .ok_or(HookTransformFailure::Stale)?
                .clone();
            let handler = build_json_handler(agent, draft, Some(&existing));

            let group_matcher = current
                .get(&locator.event)
                .and_then(JsonValue::as_array)
                .and_then(|groups| groups.get(locator.group_index))
                .and_then(json_group_matcher);
            let in_place =
                locator.event == draft.event && group_matcher == draft.matcher.as_deref();

            if in_place {
                // Replacing the element keeps the handler where the user sees
                // it; removing and re-appending would move it to the end.
                let handlers = hooks
                    .get_mut(&locator.event)
                    .and_then(JsonValue::as_array_mut)
                    .and_then(|groups| groups.get_mut(locator.group_index))
                    .and_then(|group| group.get_mut("hooks"))
                    .and_then(JsonValue::as_array_mut)
                    .ok_or(HookTransformFailure::Stale)?;
                handlers[locator.handler_index] = handler;
                Ok(())
            } else {
                remove_json_handler(hooks, locator)?;
                insert_json_handler(hooks, draft, handler)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// TOML document patching
// ---------------------------------------------------------------------------

fn patch_toml(
    agent: HookAgent,
    source_text: Option<&str>,
    operations: &[HookEditOperationDto],
) -> Result<String, HookTransformFailure> {
    let mut document: DocumentMut = match source_text {
        None => DocumentMut::new(),
        Some(text) => text.parse().map_err(|_| HookTransformFailure::Unparsable)?,
    };
    for operation in operations {
        apply_toml_operation(&mut document, agent, operation)?;
    }
    Ok(document.to_string())
}

fn json_to_toml_value(value: &JsonValue) -> Option<TomlValue> {
    match value {
        JsonValue::Null => None,
        JsonValue::Bool(flag) => Some(TomlValue::from(*flag)),
        JsonValue::Number(number) => number
            .as_i64()
            .map(TomlValue::from)
            .or_else(|| number.as_f64().map(TomlValue::from)),
        JsonValue::String(text) => Some(TomlValue::from(text.clone())),
        JsonValue::Array(items) => {
            let mut array = TomlArray::new();
            for item in items {
                array.push(json_to_toml_value(item)?);
            }
            Some(TomlValue::Array(array))
        }
        JsonValue::Object(map) => {
            let mut table = TomlInlineTable::new();
            for (key, item) in map {
                table.insert(key, json_to_toml_value(item)?);
            }
            Some(TomlValue::InlineTable(table))
        }
    }
}

/// Writes a value only when it actually differs.
///
/// Rewriting an unchanged key would replace its formatting, which is exactly
/// what a TOML round trip has to leave alone.
fn set_toml_field(table: &mut TomlTable, key: &str, value: &JsonValue) {
    if let Some(existing) = table.get(key) {
        if &hook_inspection::toml_item_to_json(existing) == value {
            return;
        }
    }
    match json_to_toml_value(value) {
        Some(value) => {
            table.insert(key, TomlItem::Value(value));
        }
        None => {
            table.remove(key);
        }
    }
}

fn toml_handler_table(draft: &HookHandlerDraftDto) -> TomlTable {
    let mut table = TomlTable::new();
    set_toml_field(
        &mut table,
        "type",
        &JsonValue::String(draft.handler_type.clone()),
    );
    for (name, value) in &draft.fields {
        set_toml_field(&mut table, name, value);
    }
    table
}

fn toml_hooks_table(document: &mut DocumentMut) -> Result<&mut TomlTable, HookTransformFailure> {
    let item = document
        .as_table_mut()
        .entry("hooks")
        .or_insert_with(|| {
            let mut table = TomlTable::new();
            // Implicit so a new event renders as `[[hooks.Event]]` without an
            // empty `[hooks]` header appearing above it.
            table.set_implicit(true);
            TomlItem::Table(table)
        });
    item.as_table_mut().ok_or(HookTransformFailure::Unparsable)
}

fn toml_event_groups<'a>(
    hooks: &'a mut TomlTable,
    event: &str,
) -> Result<&'a mut TomlArrayOfTables, HookTransformFailure> {
    let item = hooks
        .entry(event)
        .or_insert_with(|| TomlItem::ArrayOfTables(TomlArrayOfTables::new()));
    item.as_array_of_tables_mut()
        .ok_or(HookTransformFailure::Unparsable)
}

fn toml_group_matcher(group: &TomlTable) -> Option<String> {
    group
        .get("matcher")
        .and_then(|item| item.as_str())
        .map(str::to_string)
}

fn insert_toml_handler(
    document: &mut DocumentMut,
    draft: &HookHandlerDraftDto,
) -> Result<(), HookTransformFailure> {
    let handler = toml_handler_table(draft);
    let hooks = toml_hooks_table(document)?;
    let groups = toml_event_groups(hooks, &draft.event)?;

    let position = groups
        .iter()
        .position(|group| toml_group_matcher(group) == draft.matcher);
    let index = match position {
        Some(index) => index,
        None => {
            let mut group = TomlTable::new();
            if let Some(matcher) = &draft.matcher {
                group.insert(
                    "matcher",
                    TomlItem::Value(TomlValue::from(matcher.clone())),
                );
            }
            groups.push(group);
            groups.len() - 1
        }
    };

    let group = groups
        .get_mut(index)
        .ok_or(HookTransformFailure::Unparsable)?;
    group
        .entry("hooks")
        .or_insert_with(|| TomlItem::ArrayOfTables(TomlArrayOfTables::new()))
        .as_array_of_tables_mut()
        .ok_or(HookTransformFailure::Unparsable)?
        .push(handler);
    Ok(())
}

fn remove_toml_handler(
    document: &mut DocumentMut,
    locator: &HookHandlerLocatorDto,
) -> Result<(), HookTransformFailure> {
    let stale = || HookTransformFailure::Stale;
    let hooks = toml_hooks_table(document)?;
    let groups = toml_event_groups(hooks, &locator.event)?;
    let group = groups.get_mut(locator.group_index).ok_or_else(stale)?;
    let handlers = group
        .get_mut("hooks")
        .and_then(TomlItem::as_array_of_tables_mut)
        .ok_or_else(stale)?;
    if locator.handler_index >= handlers.len() {
        return Err(stale());
    }
    handlers.remove(locator.handler_index);

    if handlers.is_empty() {
        group.remove("hooks");
        groups.remove(locator.group_index);
    }
    if groups.is_empty() {
        hooks.remove(&locator.event);
    }
    Ok(())
}

fn apply_toml_operation(
    document: &mut DocumentMut,
    agent: HookAgent,
    operation: &HookEditOperationDto,
) -> Result<(), HookTransformFailure> {
    match operation {
        HookEditOperationDto::CreateHandler { draft } => insert_toml_handler(document, draft),
        HookEditOperationDto::DeleteHandler { locator } => remove_toml_handler(document, locator),
        HookEditOperationDto::UpdateHandler { locator, draft } => {
            let stale = || HookTransformFailure::Stale;
            let hooks = toml_hooks_table(document)?;
            let groups = toml_event_groups(hooks, &locator.event)?;
            let group = groups.get_mut(locator.group_index).ok_or_else(stale)?;
            let current_matcher = toml_group_matcher(group);
            let in_place = locator.event == draft.event && current_matcher == draft.matcher;

            if !in_place {
                remove_toml_handler(document, locator)?;
                return insert_toml_handler(document, draft);
            }

            let handler = group
                .get_mut("hooks")
                .and_then(TomlItem::as_array_of_tables_mut)
                .and_then(|handlers| handlers.get_mut(locator.handler_index))
                .ok_or_else(stale)?;

            set_toml_field(
                handler,
                "type",
                &JsonValue::String(draft.handler_type.clone()),
            );
            for (name, value) in &draft.fields {
                set_toml_field(handler, name, value);
            }
            // A known field the draft no longer carries was cleared by the user.
            let cleared: Vec<String> = handler
                .iter()
                .map(|(key, _)| key.to_string())
                .filter(|key| {
                    key != "type"
                        && hook_inspection::known_fields(agent).contains(&key.as_str())
                        && !draft.fields.contains_key(key)
                })
                .collect();
            for key in cleared {
                handler.remove(&key);
            }
            Ok(())
        }
    }
}

/// The locator an operation addresses, if it addresses one at all.
fn operation_locator(operation: &HookEditOperationDto) -> Option<&HookHandlerLocatorDto> {
    match operation {
        HookEditOperationDto::CreateHandler { .. } => None,
        HookEditOperationDto::UpdateHandler { locator, .. } => Some(locator),
        HookEditOperationDto::DeleteHandler { locator } => Some(locator),
    }
}

/// Resolves a locator against the Hook subtree as it was actually read.
///
/// A locator that does not name exactly one handler is not repaired by
/// searching nearby: the next handler along could be an unrelated command.
fn find_handler<'a>(hooks: &'a JsonValue, locator: &HookHandlerLocatorDto) -> Option<&'a JsonValue> {
    hooks
        .get(&locator.event)?
        .as_array()?
        .get(locator.group_index)?
        .get("hooks")?
        .as_array()?
        .get(locator.handler_index)
        .filter(|handler| handler.is_object())
}

/// Checks a draft value against the shape its field name implies.
fn shape_matches(shape: HookFieldShape, value: &JsonValue) -> bool {
    match shape {
        HookFieldShape::Text => value.as_str().is_some_and(|text| !text.is_empty()),
        HookFieldShape::Integer => value.as_i64().is_some_and(|number| number >= 0),
        HookFieldShape::Bool => value.is_boolean(),
        HookFieldShape::TextList => value
            .as_array()
            .is_some_and(|items| items.iter().all(JsonValue::is_string)),
        HookFieldShape::Table => value.is_object(),
        HookFieldShape::Any => !value.is_null(),
    }
}

/// The field a handler type cannot work without. Derived from the handler type
/// itself, not from a documented required-field list.
fn required_fields(handler_type: &str) -> &'static [&'static str] {
    match handler_type {
        "command" => &["command"],
        "http" => &["url"],
        "mcp_tool" => &["server", "tool"],
        "prompt" => &["prompt"],
        _ => &[],
    }
}

/// Checks every draft against the Agent's pinned registry.
///
/// All issues are collected rather than returned one at a time, so the editor
/// can mark every offending control in one pass.
fn validate_operations(
    agent: HookAgent,
    operations: &[HookEditOperationDto],
) -> Vec<HookValidationIssueDto> {
    let mut issues = Vec::new();
    for (operation_index, operation) in operations.iter().enumerate() {
        let draft = match operation {
            HookEditOperationDto::DeleteHandler { .. } => continue,
            HookEditOperationDto::CreateHandler { draft } => draft,
            HookEditOperationDto::UpdateHandler { draft, .. } => draft,
        };
        let mut issue = |field: &str, code: HookIssueCode| {
            issues.push(HookValidationIssueDto {
                operation_index,
                field: field.to_string(),
                code,
            });
        };

        if !hook_inspection::known_events(agent).contains(&draft.event.as_str()) {
            issue("event", HookIssueCode::UnknownEvent);
        }
        if !hook_inspection::known_handlers(agent).contains(&draft.handler_type.as_str()) {
            issue("handlerType", HookIssueCode::UnknownHandlerType);
        }
        for (name, value) in &draft.fields {
            if !hook_inspection::known_fields(agent).contains(&name.as_str()) {
                issue(name, HookIssueCode::UnknownField);
                continue;
            }
            if !shape_matches(hook_inspection::field_shape(name), value) {
                issue(name, HookIssueCode::InvalidValueType);
            }
        }
        for name in required_fields(&draft.handler_type) {
            if !draft.fields.contains_key(*name) {
                issue(name, HookIssueCode::MissingValue);
            }
        }
    }
    issues
}

// ---------------------------------------------------------------------------
// Preview
// ---------------------------------------------------------------------------

/// The revision reported for a fixed source that does not exist yet.
///
/// A literal keeps the absent state out of the hash space, so no file content
/// can ever collide with "there was no file".
pub const MISSING_REVISION: &str = "missing";

/// Hashes the exact bytes a mutation was based on.
///
/// The hash covers the whole document, not just the Hook subtree: an external
/// edit to a sibling key changes the file we are about to replace, so it must
/// invalidate the preview too.
pub fn revision_of(bytes: Option<&[u8]>) -> String {
    match bytes {
        None => MISSING_REVISION.to_string(),
        Some(bytes) => {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(bytes);
            format!("{:x}", hasher.finalize())
        }
    }
}

/// What the user is asked to confirm before a write.
///
/// Only the Hook subtree crosses this boundary. `baseRevision` binds the
/// confirmation to the exact bytes it was computed from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookWritePreviewDto {
    pub source_id: String,
    pub base_revision: String,
    pub before_canonical_text: String,
    pub after_canonical_text: String,
    pub validation_issues: Vec<HookValidationIssueDto>,
    pub can_apply: bool,
    pub would_create_file: bool,
}

/// Builds the preview for a validated set of operations.
///
/// This function has no database handle and no backup root by construction, so
/// a preview cannot persist anything even if a later change tries to.
pub fn preview_change(
    home: Option<&Path>,
    project_id: Option<&str>,
    source_id: &str,
    operations: &[HookEditOperationDto],
    lookup: impl Fn(&str) -> Option<String>,
) -> Result<HookWritePreviewDto, HookWriteError> {
    let target = resolve_write_target(home, project_id, source_id, lookup)?;
    let source_text = read_source_text(&target)?;
    let base_revision = revision_of(source_text.as_deref().map(str::as_bytes));
    let before_canonical = match &source_text {
        None => String::new(),
        Some(text) => hook_inspection::parse_hook_subtree(target.format, text)
            .map_err(|_| HookWriteError::UnsupportedSourceType)?
            .canonical_text,
    };

    let (after_canonical, mut issues) = match transform_source(
        target.agent,
        target.format,
        source_text.as_deref(),
        operations,
    ) {
        Ok(transform) => (transform.after_canonical, Vec::new()),
        Err(HookTransformFailure::Invalid(issues)) => (String::new(), issues),
        Err(failure) => return Err(failure.code()),
    };

    let oversized = |text: &str| {
        text.len() > hook_inspection::MAX_DIFF_BYTES
            || text.lines().count() > hook_inspection::MAX_DIFF_LINES
    };
    let too_large = oversized(&before_canonical) || oversized(&after_canonical);
    if too_large {
        issues.push(HookValidationIssueDto {
            operation_index: 0,
            field: "preview".to_string(),
            code: HookIssueCode::PreviewTooLarge,
        });
    }

    Ok(HookWritePreviewDto {
        source_id: target.source_id,
        base_revision,
        // Oversized text is dropped here rather than at the component: the O(n²)
        // line diff must never receive it in the first place.
        before_canonical_text: if too_large {
            String::new()
        } else {
            before_canonical
        },
        after_canonical_text: if too_large {
            String::new()
        } else {
            after_canonical
        },
        can_apply: issues.is_empty(),
        would_create_file: target.state == HookTargetState::Missing,
        validation_issues: issues,
    })
}

/// Reads the whole source, or reports that it is not there.
///
/// The same read limit as inspection applies: a config past it is refused
/// rather than pulled into memory.
fn read_source_text(target: &HookWriteTarget) -> Result<Option<String>, HookWriteError> {
    if target.state == HookTargetState::Missing {
        return Ok(None);
    }
    let metadata =
        fs::metadata(&target.path).map_err(|_| HookWriteError::UnsupportedSourceType)?;
    if metadata.len() > hook_inspection::MAX_SOURCE_BYTES {
        return Err(HookWriteError::PreviewTooLarge);
    }
    let bytes = fs::read(&target.path).map_err(|_| HookWriteError::UnsupportedSourceType)?;
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| HookWriteError::UnsupportedSourceType)
}

// ---------------------------------------------------------------------------
// Apply
// ---------------------------------------------------------------------------

/// The steps of the apply transaction a test can force to fail.
///
/// Recovery is only trustworthy if every step has been observed failing, and
/// the failures cannot be produced by ordinary filesystem manipulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookFaultPoint {
    /// Promoting the staged recovery payload to `latest`.
    BackupPromote,
    /// Writing and syncing the staged replacement next to the target.
    StagedTargetSync,
    /// The atomic replacement itself.
    AtomicReplace,
    /// Committing Artifact, detail and backup metadata.
    SqliteCommit,
    /// A platform that cannot promise atomic replacement at all.
    AtomicReplaceUnsupported,
}

/// Everything a mutation needs beyond its request.
///
/// `backup_root` is application state — `central_repo::base_dir()/hook-backups`
/// in production — deliberately outside the Library Git tree, because a
/// recovery payload holds the user's raw Hook configuration.
pub struct HookWriteEnv<'a> {
    pub home: Option<&'a Path>,
    pub backup_root: &'a Path,
    pub store: &'a SkillStore,
    /// Test-only seam; production callers pass `None`.
    pub fault: Option<HookFaultPoint>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookApplyOutcome {
    pub artifact_id: String,
    pub backup_id: String,
    /// Revision of the source after the write.
    pub revision: String,
    pub created_file: bool,
}

/// Re-validates, backs up and atomically replaces the target.
///
/// `base_revision` is the value the user confirmed. Apply never accepts the
/// previewed text: it re-derives the document from the operations so an edited
/// payload cannot bypass validation.
pub fn apply_change(
    env: &HookWriteEnv<'_>,
    project_id: Option<&str>,
    source_id: &str,
    base_revision: &str,
    operations: &[HookEditOperationDto],
) -> Result<HookApplyOutcome, HookWriteError> {
    // One mutation at a time, process-wide: two applies to the same source
    // would otherwise interleave their backup and replacement steps.
    let _guard = HOOK_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    ensure_mutations_allowed(env.backup_root)?;
    ensure_atomic_replacement(env)?;
    let target = resolve_write_target(env.home, project_id, source_id, |id| {
        env.store
            .get_project_by_id(id)
            .ok()
            .flatten()
            .map(|record| record.path)
    })?;

    // Re-read rather than trust the preview: the confirmed revision is only
    // meaningful against the bytes that are on disk right now.
    let original = read_source_text(&target)?;
    if revision_of(original.as_deref().map(str::as_bytes)) != base_revision {
        return Err(HookWriteError::SourceConflict);
    }

    let transform = transform_source(
        target.agent,
        target.format,
        original.as_deref(),
        operations,
    )
    .map_err(|failure| failure.code())?;
    if transform.after_canonical.len() > hook_inspection::MAX_DIFF_BYTES
        || transform.after_canonical.lines().count() > hook_inspection::MAX_DIFF_LINES
    {
        return Err(HookWriteError::PreviewTooLarge);
    }

    let context = write_context(&target, project_id);
    let existing = env
        .store
        .get_hook_detail(&target.source_id, &context.key())
        .map_err(|_| HookWriteError::WriteFailed)?;
    let artifact_id = existing
        .as_ref()
        .map(|detail| detail.artifact_id.clone())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let created_file = target.state == HookTargetState::Missing;
    let backup_id = perform_write(
        env,
        &target,
        &artifact_id,
        &context,
        existing.is_none(),
        original.as_deref(),
        &transform.document_text,
    )?;

    Ok(HookApplyOutcome {
        artifact_id,
        backup_id,
        revision: revision_of(Some(transform.document_text.as_bytes())),
        created_file,
    })
}

/// A user-scope source is the same file whichever Project is selected, so its
/// identity is global even when a Project is in view.
fn write_context(target: &HookWriteTarget, project_id: Option<&str>) -> HookContext {
    match target.scope {
        HookScope::User => HookContext::Global,
        HookScope::Project | HookScope::ProjectLocal => match project_id {
            Some(id) => HookContext::Project(id.to_string()),
            None => HookContext::Global,
        },
    }
}

fn injected(env: &HookWriteEnv<'_>, point: HookFaultPoint) -> bool {
    env.fault == Some(point)
}

/// Refuses before anything is touched on a runtime that cannot promise an
/// atomic replacement. Falling back to delete-then-rename would leave a window
/// where the Agent reads a missing or half-written config.
fn ensure_atomic_replacement(env: &HookWriteEnv<'_>) -> Result<(), HookWriteError> {
    if injected(env, HookFaultPoint::AtomicReplaceUnsupported) || !cfg!(unix) {
        return Err(HookWriteError::AtomicReplaceUnsupported);
    }
    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> std::io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn mode_of(path: &Path) -> Option<u32> {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).ok().map(|m| m.permissions().mode())
}

#[cfg(not(unix))]
fn mode_of(_path: &Path) -> Option<u32> {
    None
}

/// Writes a file and forces it to disk before anyone can depend on it.
fn write_synced(path: &Path, bytes: &[u8], mode: u32) -> std::io::Result<()> {
    use std::io::Write;
    let mut file = fs::File::create(path)?;
    set_mode(path, mode)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// A rename is only durable once the directory entry itself is on disk.
fn sync_dir(path: &Path) -> std::io::Result<()> {
    fs::File::open(path)?.sync_all()
}

/// Recovery payloads hold the user's raw configuration, so the directory is
/// owner-only from the moment it exists.
fn create_private_dir(path: &Path) -> Result<(), HookWriteError> {
    fs::create_dir_all(path).map_err(|_| HookWriteError::BackupFailed)?;
    set_mode(path, 0o700).map_err(|_| HookWriteError::BackupFailed)?;
    Ok(())
}

fn staged_target_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| "source".to_string());
    // Same directory, so the rename stays on one filesystem and is atomic.
    target.with_file_name(format!(".{name}.agentdeck-{}", uuid::Uuid::new_v4()))
}

/// The backup, staged write, replacement and metadata commit, in the one order
/// that leaves a recoverable state at every step.
#[allow(clippy::too_many_arguments)]
fn perform_write(
    env: &HookWriteEnv<'_>,
    target: &HookWriteTarget,
    artifact_id: &str,
    context: &HookContext,
    is_new_identity: bool,
    original: Option<&str>,
    document_text: &str,
) -> Result<String, HookWriteError> {
    let backup_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let backup_dir = env.backup_root.join(artifact_id);
    let pending = backup_dir.join("latest.pending");
    let staged_backup = backup_dir.join("latest.tmp");
    create_private_dir(&backup_dir)?;

    if let Some(original) = original {
        write_synced(&staged_backup, original.as_bytes(), 0o600)
            .map_err(|_| HookWriteError::BackupFailed)?;
    }
    if injected(env, HookFaultPoint::BackupPromote) {
        let _ = fs::remove_file(&staged_backup);
        return Err(HookWriteError::BackupFailed);
    }
    // Stage the new recovery payload without overwriting the existing one:
    // an earlier recovery point stays intact until the metadata commit
    // confirms this operation, so a mid-transaction failure cannot orphan
    // the previous payload from its metadata.
    if original.is_some() {
        fs::rename(&staged_backup, &pending).map_err(|_| HookWriteError::BackupFailed)?;
    }
    let _ = sync_dir(&backup_dir);

    // Written before the target changes, so a crash mid-replacement leaves
    // enough to tell which state the target should be brought back to.
    let journal = HookOperationJournal {
        operation_id: uuid::Uuid::new_v4().to_string(),
        artifact_id: artifact_id.to_string(),
        source_id: target.source_id.clone(),
        context_key: context.key(),
        backup_id: backup_id.to_string(),
        backup_kind: match original {
            Some(_) => "bytes".to_string(),
            None => "absent".to_string(),
        },
        backup_locator: match original {
            Some(_) => format!("{artifact_id}/latest.pending"),
            None => String::new(),
        },
        base_revision: revision_of(original.map(str::as_bytes)),
        target_revision: revision_of(Some(document_text.as_bytes())),
    };
    write_journal(env.backup_root, &journal)?;

    let parent = target
        .path
        .parent()
        .ok_or(HookWriteError::WriteFailed)?
        .to_path_buf();
    fs::create_dir_all(&parent).map_err(|_| HookWriteError::WriteFailed)?;
    let staged_target = staged_target_path(&target.path);
    // A file AgentDeck creates may hold a secret, so it starts owner-only; an
    // existing file keeps whatever mode the user chose.
    let mode = mode_of(&target.path).unwrap_or(0o600);

    let replaced = (|| -> Result<(), HookWriteError> {
        write_synced(&staged_target, document_text.as_bytes(), mode)
            .map_err(|_| HookWriteError::WriteFailed)?;
        if injected(env, HookFaultPoint::StagedTargetSync) {
            return Err(HookWriteError::WriteFailed);
        }
        if injected(env, HookFaultPoint::AtomicReplace) {
            return Err(HookWriteError::WriteFailed);
        }
        fs::rename(&staged_target, &target.path).map_err(|_| HookWriteError::WriteFailed)?;
        let _ = sync_dir(&parent);
        Ok(())
    })();
    if let Err(error) = replaced {
        let _ = fs::remove_file(&staged_target);
        let _ = fs::remove_file(&pending);
        let _ = fs::remove_file(journal_path(env.backup_root, &journal.operation_id));
        return Err(error);
    }

    let committed = commit_hook_metadata(
        env,
        target,
        artifact_id,
        context,
        is_new_identity,
        original,
        document_text,
        &backup_id,
        now,
    );
    if let Err(error) = committed {
        restore_from(original, &target.path)?;
        let _ = fs::remove_file(&pending);
        let _ = fs::remove_file(journal_path(env.backup_root, &journal.operation_id));
        return Err(error);
    }

    // Metadata now names this operation, but recovery is not usable until the
    // staged payload is promoted. Keep the journal on any error so startup can
    // finish this idempotently instead of leaving metadata pointed at stale
    // bytes.
    promote_pending_backup(env.backup_root, &journal)?;
    remove_journal(env.backup_root, &journal.operation_id)?;
    Ok(backup_id)
}

/// Puts the target back the way the recovery point says it was.
fn restore_from(original: Option<&str>, path: &Path) -> Result<(), HookWriteError> {
    match original {
        Some(bytes) => {
            let staged = staged_target_path(path);
            write_synced(&staged, bytes.as_bytes(), mode_of(path).unwrap_or(0o600))
                .map_err(|_| HookWriteError::WriteFailed)?;
            fs::rename(&staged, path).map_err(|_| HookWriteError::WriteFailed)?;
        }
        None => match fs::remove_file(path) {
            Ok(()) => {}
            // Already absent is the state we were asked to produce.
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(HookWriteError::WriteFailed),
        },
    }
    if let Some(parent) = path.parent() {
        let _ = sync_dir(parent);
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn commit_hook_metadata(
    env: &HookWriteEnv<'_>,
    target: &HookWriteTarget,
    artifact_id: &str,
    context: &HookContext,
    is_new_identity: bool,
    original: Option<&str>,
    document_text: &str,
    backup_id: &str,
    now: i64,
) -> Result<(), HookWriteError> {
    if injected(env, HookFaultPoint::SqliteCommit) {
        return Err(HookWriteError::WriteFailed);
    }

    let artifact = ArtifactRecord {
        id: artifact_id.to_string(),
        kind: ArtifactKind::Hook,
    };
    env.store
        .commit_hook_write(
            is_new_identity.then_some(&artifact),
            &HookDetailRecord {
                artifact_id: artifact_id.to_string(),
                source_id: target.source_id.clone(),
                context: context.clone(),
                agent: target.agent.as_str().to_string(),
                scope: target.scope.as_str().to_string(),
                format: match target.format {
                    HookFormat::Json => "json".to_string(),
                    HookFormat::Toml => "toml".to_string(),
                },
                created_at: now,
                updated_at: now,
            },
            &HookBackupRecord {
                id: backup_id.to_string(),
                artifact_id: artifact_id.to_string(),
                kind: match original {
                    Some(_) => HookBackupKind::Bytes,
                    None => HookBackupKind::Absent,
                },
                before_hash: revision_of(original.map(str::as_bytes)),
                after_hash: revision_of(Some(document_text.as_bytes())),
                locator: match original {
                    Some(_) => format!("{artifact_id}/latest"),
                    None => String::new(),
                },
                created_at: now,
                restored_at: None,
            },
        )
        .map_err(|_| HookWriteError::WriteFailed)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Operation journal and startup reconciliation
// ---------------------------------------------------------------------------

/// Unfinished operations live under this directory of the backup root.
pub const JOURNAL_DIR: &str = "journal";

/// What an interrupted operation left behind.
///
/// The journal lives on the filesystem rather than in SQLite because it has to
/// survive a crash between the target replacement and the metadata commit —
/// exactly the window a database transaction cannot cover. It carries ids,
/// hashes and a locator only: never Hook content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HookOperationJournal {
    pub operation_id: String,
    pub artifact_id: String,
    pub source_id: String,
    pub context_key: String,
    pub backup_id: String,
    /// `bytes` or `absent`, matching the recovery point.
    pub backup_kind: String,
    /// Backup-root-relative payload location; empty for an absence marker.
    pub backup_locator: String,
    pub base_revision: String,
    pub target_revision: String,
}

pub fn journal_path(backup_root: &Path, operation_id: &str) -> PathBuf {
    backup_root
        .join(JOURNAL_DIR)
        .join(format!("{operation_id}.json"))
}

/// Refuses every mutation while an unfinished operation journal is present.
///
/// Inspection stays available: a user whose recovery is stuck still needs to
/// see what is on disk.
pub fn ensure_mutations_allowed(backup_root: &Path) -> Result<(), HookWriteError> {
    if pending_journals(backup_root).is_empty() {
        Ok(())
    } else {
        Err(HookWriteError::RecoveryRequired)
    }
}

/// Every journal file still on disk, in a stable order.
fn pending_journals(backup_root: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = match fs::read_dir(backup_root.join(JOURNAL_DIR)) {
        Err(_) => return Vec::new(),
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .collect(),
    };
    paths.sort();
    paths
}

fn write_journal(
    backup_root: &Path,
    journal: &HookOperationJournal,
) -> Result<(), HookWriteError> {
    let path = journal_path(backup_root, &journal.operation_id);
    let dir = path.parent().ok_or(HookWriteError::WriteFailed)?;
    create_private_dir(dir)?;
    let body = serde_json::to_vec(journal).map_err(|_| HookWriteError::WriteFailed)?;
    write_synced(&path, &body, 0o600).map_err(|_| HookWriteError::WriteFailed)?;
    let _ = sync_dir(dir);
    Ok(())
}

/// Brings every unfinished operation back to a consistent state at startup.
///
/// Either the target is restored from its recovery point, or the recorded
/// metadata is confirmed; the journal is only cleared once target and database
/// agree.
pub fn reconcile_pending_operations(env: &HookWriteEnv<'_>) -> Result<(), HookWriteError> {
    let _guard = HOOK_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    for path in pending_journals(env.backup_root) {
        let journal: HookOperationJournal = fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .ok_or(HookWriteError::RecoveryRequired)?;
        reconcile_one(env, &journal)?;
        remove_journal(env.backup_root, &journal.operation_id)?;
    }
    Ok(())
}

fn reconcile_one(
    env: &HookWriteEnv<'_>,
    journal: &HookOperationJournal,
) -> Result<(), HookWriteError> {
    // The metadata commit is the last step, so a recovery point recorded under
    // this operation's id means the whole write landed.
    let committed = env
        .store
        .get_hook_backup(&journal.artifact_id)
        .map_err(|_| HookWriteError::RecoveryRequired)?
        .is_some_and(|backup| backup.id == journal.backup_id);
    if committed {
        promote_pending_backup(env.backup_root, journal)?;
        return Ok(());
    }

    let project_id = match HookContext::parse(&journal.context_key) {
        Ok(HookContext::Global) => None,
        Ok(HookContext::Project(id)) => Some(id),
        Err(_) => return Err(HookWriteError::RecoveryRequired),
    };
    let target = resolve_write_target(
        env.home,
        project_id.as_deref(),
        &journal.source_id,
        |id| {
            env.store
                .get_project_by_id(id)
                .ok()
                .flatten()
                .map(|record| record.path)
        },
    )
    .map_err(|_| HookWriteError::RecoveryRequired)?;

    let original = read_recovery_payload(env.backup_root, journal)?;
    restore_from(original.as_deref(), &target.path)
        .map_err(|_| HookWriteError::RecoveryRequired)?;
    if journal.backup_locator.ends_with("/latest.pending") {
        remove_if_present(&env.backup_root.join(&journal.backup_locator))
            .map_err(|_| HookWriteError::RecoveryRequired)?;
        sync_dir(
            env.backup_root
                .join(&journal.artifact_id)
                .as_path(),
        )
        .map_err(|_| HookWriteError::RecoveryRequired)?;
    }
    Ok(())
}

fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn remove_journal(backup_root: &Path, operation_id: &str) -> Result<(), HookWriteError> {
    let path = journal_path(backup_root, operation_id);
    remove_if_present(&path).map_err(|_| HookWriteError::RecoveryRequired)?;
    let dir = path.parent().ok_or(HookWriteError::RecoveryRequired)?;
    sync_dir(dir).map_err(|_| HookWriteError::RecoveryRequired)
}

fn promote_pending_backup(
    backup_root: &Path,
    journal: &HookOperationJournal,
) -> Result<(), HookWriteError> {
    let backup_dir = backup_root.join(&journal.artifact_id);
    let pending = backup_dir.join("latest.pending");
    let latest = backup_dir.join("latest");
    if journal.backup_kind == "absent" {
        remove_if_present(&latest).map_err(|_| HookWriteError::RecoveryRequired)?;
    } else if pending.exists() {
        let pending_bytes = fs::read(&pending).map_err(|_| HookWriteError::RecoveryRequired)?;
        if revision_of(Some(&pending_bytes)) != journal.base_revision {
            return Err(HookWriteError::RecoveryRequired);
        }
        fs::rename(&pending, &latest).map_err(|_| HookWriteError::RecoveryRequired)?;
    } else {
        // Journal removal may have failed after an earlier successful
        // promotion. Accept that already-promoted state only when the exact
        // payload hash still matches this operation.
        let latest_bytes = fs::read(&latest).map_err(|_| HookWriteError::RecoveryRequired)?;
        if revision_of(Some(&latest_bytes)) != journal.base_revision {
            return Err(HookWriteError::RecoveryRequired);
        }
    }
    sync_dir(&backup_dir).map_err(|_| HookWriteError::RecoveryRequired)
}

/// Reads the payload a journal or backup record points at, refusing anything
/// whose bytes do not hash to the revision the metadata recorded.
fn read_recovery_payload(
    backup_root: &Path,
    journal: &HookOperationJournal,
) -> Result<Option<String>, HookWriteError> {
    if journal.backup_kind == "absent" {
        return Ok(None);
    }
    let bytes = fs::read(backup_root.join(&journal.backup_locator))
        .map_err(|_| HookWriteError::RecoveryRequired)?;
    if revision_of(Some(&bytes)) != journal.base_revision {
        return Err(HookWriteError::RecoveryRequired);
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| HookWriteError::RecoveryRequired)
}

// ---------------------------------------------------------------------------
// Recovery and restore
// ---------------------------------------------------------------------------

/// The single recovery point a managed source can be restored to.
///
/// The payload itself is never returned — only whether it is still usable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HookRecoveryDto {
    pub artifact_id: String,
    pub backup_id: String,
    /// `bytes` or `absent`.
    pub kind: &'static str,
    pub created_at: i64,
    pub restored_at: Option<i64>,
    /// The current source is still exactly what the apply produced.
    pub can_restore: bool,
    /// Revision of the source as it is right now.
    pub base_revision: String,
}

/// Returns the latest recovery point for a managed source, if there is one.
pub fn get_recovery(
    env: &HookWriteEnv<'_>,
    project_id: Option<&str>,
    source_id: &str,
) -> Result<Option<HookRecoveryDto>, HookWriteError> {
    let (target, _, backup) = match resolve_recovery(env, project_id, source_id)? {
        None => return Ok(None),
        Some(found) => found,
    };
    let current = revision_of(read_source_text(&target)?.as_deref().map(str::as_bytes));

    Ok(Some(HookRecoveryDto {
        artifact_id: backup.artifact_id,
        backup_id: backup.id,
        kind: backup.kind.as_str(),
        created_at: backup.created_at,
        restored_at: backup.restored_at,
        // Restoring a source that has moved on since the apply would discard an
        // external edit the user never saw.
        can_restore: current == backup.after_hash,
        base_revision: current,
    }))
}

/// Looks up the managed identity and its single recovery point.
fn resolve_recovery(
    env: &HookWriteEnv<'_>,
    project_id: Option<&str>,
    source_id: &str,
) -> Result<Option<(HookWriteTarget, HookDetailRecord, HookBackupRecord)>, HookWriteError> {
    let target = resolve_write_target(env.home, project_id, source_id, |id| {
        env.store
            .get_project_by_id(id)
            .ok()
            .flatten()
            .map(|record| record.path)
    })?;
    let context = write_context(&target, project_id);
    let detail = match env
        .store
        .get_hook_detail(&target.source_id, &context.key())
        .map_err(|_| HookWriteError::RestoreFailed)?
    {
        None => return Ok(None),
        Some(detail) => detail,
    };
    let backup = match env
        .store
        .get_hook_backup(&detail.artifact_id)
        .map_err(|_| HookWriteError::RestoreFailed)?
    {
        None => return Ok(None),
        Some(backup) => backup,
    };
    Ok(Some((target, detail, backup)))
}

/// Reads and verifies the payload a recovery point names.
///
/// A payload whose bytes do not hash to the recorded revision is treated as
/// unusable rather than written over the user's current configuration.
fn recovery_payload(
    env: &HookWriteEnv<'_>,
    backup: &HookBackupRecord,
) -> Result<Option<String>, HookWriteError> {
    if backup.kind == HookBackupKind::Absent {
        return Ok(None);
    }
    let bytes = fs::read(env.backup_root.join(&backup.locator))
        .map_err(|_| HookWriteError::RestoreFailed)?;
    if revision_of(Some(&bytes)) != backup.before_hash {
        return Err(HookWriteError::RestoreFailed);
    }
    String::from_utf8(bytes)
        .map(Some)
        .map_err(|_| HookWriteError::RestoreFailed)
}

/// Diffs the current Hook subtree against the one held in the recovery point.
pub fn preview_restore(
    env: &HookWriteEnv<'_>,
    project_id: Option<&str>,
    source_id: &str,
    backup_id: &str,
) -> Result<HookWritePreviewDto, HookWriteError> {
    let (target, _, backup) = resolve_recovery(env, project_id, source_id)?
        .ok_or(HookWriteError::RestoreFailed)?;
    if backup.id != backup_id {
        return Err(HookWriteError::RestoreFailed);
    }

    let current = read_source_text(&target)?;
    let payload = recovery_payload(env, &backup)?;
    let canonical = |text: Option<&str>| -> Result<String, HookWriteError> {
        match text {
            None => Ok(String::new()),
            Some(text) => Ok(hook_inspection::parse_hook_subtree(target.format, text)
                .map_err(|_| HookWriteError::UnsupportedSourceType)?
                .canonical_text),
        }
    };
    let before_canonical = canonical(current.as_deref())?;
    let after_canonical = canonical(payload.as_deref())?;

    let oversized = |text: &str| {
        text.len() > hook_inspection::MAX_DIFF_BYTES
            || text.lines().count() > hook_inspection::MAX_DIFF_LINES
    };
    let too_large = oversized(&before_canonical) || oversized(&after_canonical);
    let base_revision = revision_of(current.as_deref().map(str::as_bytes));

    Ok(HookWritePreviewDto {
        source_id: target.source_id,
        can_apply: !too_large && base_revision == backup.after_hash,
        would_create_file: current.is_none() && payload.is_some(),
        before_canonical_text: if too_large {
            String::new()
        } else {
            before_canonical
        },
        after_canonical_text: if too_large {
            String::new()
        } else {
            after_canonical
        },
        validation_issues: if too_large {
            vec![HookValidationIssueDto {
                operation_index: 0,
                field: "preview".to_string(),
                code: HookIssueCode::PreviewTooLarge,
            }]
        } else {
            Vec::new()
        },
        base_revision,
    })
}

/// Restores the recovery point, after capturing the current state as the new
/// one so the restore itself can be undone.
pub fn apply_restore(
    env: &HookWriteEnv<'_>,
    project_id: Option<&str>,
    source_id: &str,
    backup_id: &str,
    base_revision: &str,
) -> Result<HookApplyOutcome, HookWriteError> {
    let _guard = HOOK_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    ensure_mutations_allowed(env.backup_root)?;
    ensure_atomic_replacement(env)?;

    let (target, detail, backup) = resolve_recovery(env, project_id, source_id)?
        .ok_or(HookWriteError::RestoreFailed)?;
    if backup.id != backup_id {
        return Err(HookWriteError::RestoreFailed);
    }

    let current = read_source_text(&target)?;
    let current_revision = revision_of(current.as_deref().map(str::as_bytes));
    if current_revision != base_revision || current_revision != backup.after_hash {
        return Err(HookWriteError::SourceConflict);
    }
    // Verified before anything is touched: an unusable payload must not cost
    // the user their current configuration.
    let payload = recovery_payload(env, &backup)?;
    let current = current.ok_or(HookWriteError::RestoreFailed)?;

    perform_restore(env, &target, &detail, &current, payload.as_deref())
}

/// The restore transaction: capture, journal, replace, commit.
///
/// It mirrors apply so that a crash at any point leaves the same recoverable
/// state, and it always produces a reverse recovery point — the state before
/// the restore — so one more restore undoes it.
fn perform_restore(
    env: &HookWriteEnv<'_>,
    target: &HookWriteTarget,
    detail: &HookDetailRecord,
    current: &str,
    payload: Option<&str>,
) -> Result<HookApplyOutcome, HookWriteError> {
    let artifact_id = detail.artifact_id.clone();
    let backup_id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp();
    let backup_dir = env.backup_root.join(&artifact_id);
    let pending = backup_dir.join("latest.pending");
    let staged_backup = backup_dir.join("latest.tmp");

    create_private_dir(&backup_dir)?;
    write_synced(&staged_backup, current.as_bytes(), 0o600)
        .map_err(|_| HookWriteError::BackupFailed)?;
    if injected(env, HookFaultPoint::BackupPromote) {
        let _ = fs::remove_file(&staged_backup);
        return Err(HookWriteError::BackupFailed);
    }
    fs::rename(&staged_backup, &pending).map_err(|_| HookWriteError::BackupFailed)?;
    let _ = sync_dir(&backup_dir);

    let journal = HookOperationJournal {
        operation_id: uuid::Uuid::new_v4().to_string(),
        artifact_id: artifact_id.clone(),
        source_id: target.source_id.clone(),
        context_key: detail.context.key(),
        backup_id: backup_id.clone(),
        backup_kind: "bytes".to_string(),
        backup_locator: format!("{artifact_id}/latest.pending"),
        base_revision: revision_of(Some(current.as_bytes())),
        target_revision: revision_of(payload.map(str::as_bytes)),
    };
    write_journal(env.backup_root, &journal)?;

    let restored = restore_from(payload, &target.path);
    if let Err(error) = restored {
        let _ = fs::remove_file(&pending);
        let _ = fs::remove_file(journal_path(env.backup_root, &journal.operation_id));
        return Err(error);
    }

    let committed = (|| -> Result<(), HookWriteError> {
        if injected(env, HookFaultPoint::SqliteCommit) {
            return Err(HookWriteError::WriteFailed);
        }
        env.store
            .commit_hook_write(
                None,
                &HookDetailRecord {
                    updated_at: now,
                    ..detail.clone()
                },
                &HookBackupRecord {
                    id: backup_id.clone(),
                    artifact_id: artifact_id.clone(),
                    kind: HookBackupKind::Bytes,
                    before_hash: revision_of(Some(current.as_bytes())),
                    after_hash: revision_of(payload.map(str::as_bytes)),
                    locator: format!("{artifact_id}/latest"),
                    created_at: now,
                    // This point exists because a restore ran, which is what
                    // distinguishes it from one an apply left behind.
                    restored_at: Some(now),
                },
            )
            .map_err(|_| HookWriteError::WriteFailed)
    })();
    if let Err(error) = committed {
        restore_from(Some(current), &target.path)?;
        let _ = fs::remove_file(&pending);
        let _ = fs::remove_file(journal_path(env.backup_root, &journal.operation_id));
        return Err(error);
    }
    promote_pending_backup(env.backup_root, &journal)?;
    remove_journal(env.backup_root, &journal.operation_id)?;

    Ok(HookApplyOutcome {
        artifact_id,
        backup_id,
        revision: revision_of(payload.map(str::as_bytes)),
        created_file: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    /// Lookup that resolves exactly one Project id, like the store does.
    fn lookup_project(id: &str, root: &Path) -> impl Fn(&str) -> Option<String> {
        let id = id.to_string();
        let root = root.to_string_lossy().to_string();
        move |requested: &str| {
            if requested == id {
                Some(root.clone())
            } else {
                None
            }
        }
    }

    fn no_project(_: &str) -> Option<String> {
        None
    }

    fn write_fixture(path: &Path, content: &str) {
        fs::create_dir_all(path.parent().expect("fixture parent")).expect("create fixture dir");
        fs::write(path, content).expect("write fixture");
    }

    /// Whole-tree snapshot: catches a file or directory that appears during a
    /// refused resolution, which a per-path check would miss.
    fn snapshot(root: &Path) -> Vec<String> {
        let mut items: Vec<String> = walkdir::WalkDir::new(root)
            .min_depth(1)
            .into_iter()
            .filter_map(Result::ok)
            .map(|entry| {
                entry
                    .path()
                    .strip_prefix(root)
                    .expect("relative path")
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        items.sort();
        items
    }

    // Requirement: Hook mutation resolves only fixed writable sources
    // Scenario: Known source id resolves inside a linked Project
    #[test]
    fn fixed_source_known_id_resolves_inside_a_linked_project() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        let settings = project.path().join(".claude").join("settings.json");
        write_fixture(&settings, r#"{"hooks":{}}"#);

        let target = resolve_write_target(
            Some(home.path()),
            Some("project-1"),
            "claude_code:project:settings-json",
            lookup_project("project-1", project.path()),
        )
        .expect("known source resolves");

        assert_eq!(target.source_id, "claude_code:project:settings-json");
        assert_eq!(target.path, settings);
        assert_eq!(target.agent, HookAgent::ClaudeCode);
        assert_eq!(target.scope, HookScope::Project);
        assert_eq!(target.format, HookFormat::Json);
        assert_eq!(target.state, HookTargetState::Existing);
    }

    #[test]
    fn fixed_source_regular_user_file_is_writable() {
        let home = tempfile::tempdir().expect("home");
        let config = home.path().join(".codex").join("config.toml");
        write_fixture(&config, "model = \"gpt-5-codex\"\n");

        let target = resolve_write_target(
            Some(home.path()),
            None,
            "codex:user:config-toml",
            no_project,
        )
        .expect("user source resolves");

        assert_eq!(target.path, config);
        assert_eq!(target.format, HookFormat::Toml);
        assert_eq!(target.scope, HookScope::User);
        assert_eq!(target.state, HookTargetState::Existing);
    }

    // Scenario: Unknown source and Project fail closed
    #[test]
    fn fixed_source_unknown_id_fails_closed() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");

        let error = resolve_write_target(
            Some(home.path()),
            Some("project-1"),
            "claude_code:project:other-json",
            lookup_project("project-1", project.path()),
        )
        .expect_err("unknown source id must fail");

        assert_eq!(error, HookWriteError::InvalidSource);
        assert_eq!(error.as_str(), "invalid_source");
    }

    #[test]
    fn fixed_source_unknown_project_fails_closed() {
        let home = tempfile::tempdir().expect("home");

        let error = resolve_write_target(
            Some(home.path()),
            Some("missing-project"),
            "claude_code:project:settings-json",
            no_project,
        )
        .expect_err("unknown project must fail");

        assert_eq!(error, HookWriteError::InvalidProject);
        assert_eq!(error.as_str(), "invalid_project");
    }

    /// A project-scoped source without a selected Project has no descriptor, so
    /// it is refused rather than resolved against home.
    #[test]
    fn fixed_source_project_scope_without_a_project_is_unknown() {
        let home = tempfile::tempdir().expect("home");

        let error = resolve_write_target(
            Some(home.path()),
            None,
            "claude_code:project:settings-json",
            no_project,
        )
        .expect_err("project source without a project must fail");

        assert_eq!(error, HookWriteError::InvalidSource);
    }

    // Scenario: Offline root and symlink are not replaced
    #[test]
    fn fixed_source_offline_root_is_reported_and_not_created() {
        let home = tempfile::tempdir().expect("home");
        let parent = tempfile::tempdir().expect("parent");
        let absent_root = parent.path().join("never-created");

        let before = snapshot(parent.path());
        let error = resolve_write_target(
            Some(home.path()),
            Some("project-1"),
            "codex:project:hooks-json",
            lookup_project("project-1", &absent_root),
        )
        .expect_err("absent project root must fail");

        assert_eq!(error, HookWriteError::SourceOffline);
        assert_eq!(error.as_str(), "source_offline");
        assert!(!absent_root.exists(), "the missing root must not be created");
        assert_eq!(before, snapshot(parent.path()));
    }

    #[test]
    fn fixed_source_unresolved_home_is_offline() {
        let error = resolve_write_target(None, None, "codex:user:hooks-json", no_project)
            .expect_err("unresolved home must fail");

        assert_eq!(error, HookWriteError::SourceOffline);
    }

    #[cfg(unix)]
    #[test]
    fn fixed_source_symlink_is_rejected() {
        use std::os::unix::fs::symlink;

        let home = tempfile::tempdir().expect("home");
        let elsewhere = home.path().join("elsewhere.json");
        fs::write(&elsewhere, r#"{"hooks":{}}"#).expect("write link target");
        let source = home.path().join(".codex").join("hooks.json");
        fs::create_dir_all(source.parent().expect("parent")).expect("create dir");
        symlink(&elsewhere, &source).expect("create symlink");

        let error =
            resolve_write_target(Some(home.path()), None, "codex:user:hooks-json", no_project)
                .expect_err("symlink source must fail");

        assert_eq!(error, HookWriteError::UnsupportedSourceType);
        assert_eq!(error.as_str(), "unsupported_source_type");
        // The link itself still points at the same file: nothing was replaced.
        assert!(fs::symlink_metadata(&source)
            .expect("symlink metadata")
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read_to_string(&elsewhere).expect("read link target"),
            r#"{"hooks":{}}"#
        );
    }

    /// A directory standing where the fixed source belongs is the portable
    /// stand-in for any non-regular file: the check is `is_file`, so a FIFO or
    /// device node takes the same branch.
    #[test]
    fn fixed_source_special_file_is_rejected() {
        let home = tempfile::tempdir().expect("home");
        let source = home.path().join(".codex").join("hooks.json");
        fs::create_dir_all(&source).expect("create directory in place of the source");

        let error =
            resolve_write_target(Some(home.path()), None, "codex:user:hooks-json", no_project)
                .expect_err("non-regular source must fail");

        assert_eq!(error, HookWriteError::UnsupportedSourceType);
        assert!(source.is_dir(), "the directory must be left in place");
    }

    #[test]
    fn fixed_source_missing_file_under_an_existing_root_is_writable() {
        let home = tempfile::tempdir().expect("home");

        let before = snapshot(home.path());
        let target =
            resolve_write_target(Some(home.path()), None, "codex:user:hooks-json", no_project)
                .expect("missing source under an existing root resolves");

        assert_eq!(target.state, HookTargetState::Missing);
        assert_eq!(target.path, home.path().join(".codex").join("hooks.json"));
        assert_eq!(
            before,
            snapshot(home.path()),
            "resolution must not create the parent or the file"
        );
    }

    /// Resolution is the gate every mutation passes through, so it must stay
    /// side-effect free even on the success path.
    #[test]
    fn fixed_source_resolution_creates_nothing() {
        let home = tempfile::tempdir().expect("home");
        let project = tempfile::tempdir().expect("project");
        write_fixture(
            &project.path().join(".claude").join("settings.json"),
            r#"{"hooks":{}}"#,
        );

        let home_before = snapshot(home.path());
        let project_before = snapshot(project.path());

        for source_id in [
            "codex:user:hooks-json",
            "codex:user:config-toml",
            "claude_code:user:settings-json",
            "codex:project:hooks-json",
            "codex:project:config-toml",
            "claude_code:project:settings-json",
            "claude_code:project_local:settings-local-json",
        ] {
            let _ = resolve_write_target(
                Some(home.path()),
                Some("project-1"),
                source_id,
                lookup_project("project-1", project.path()),
            );
        }

        assert_eq!(home_before, snapshot(home.path()));
        assert_eq!(project_before, snapshot(project.path()));
    }

    // -----------------------------------------------------------------------
    // Operations and round trips
    // -----------------------------------------------------------------------

    const CODEX_HOOKS_JSON: &str = r#"{
  "description": "workspace hooks",
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Bash",
        "hooks": [
          { "type": "command", "command": "echo pre", "timeout": 10, "vendorOption": "keep-me" }
        ]
      }
    ]
  }
}"#;

    const CLAUDE_SETTINGS_JSON: &str = r#"{
  "apiToken": "sentinel-secret",
  "hooks": {
    "PostToolUse": [
      {
        "matcher": "Write",
        "hooks": [
          { "type": "command", "command": "echo post" },
          { "type": "vendor_future", "vendorField": "keep-me-too" }
        ]
      }
    ]
  }
}"#;

    const CODEX_CONFIG_TOML: &str = r#"# top of file
model = "gpt-5-codex"

[profiles.work]
# work profile stays put
approval = "on-request"

# everything below configures hooks
[[hooks.SessionStart]]
matcher = "startup|resume"

[[hooks.SessionStart.hooks]]
type = "command"
# the handler comment
command = "echo start"
additionalContextLimit = 5000
vendorOption = "keep-me"

[telemetry]
enabled = false
"#;

    fn field_map(pairs: &[(&str, JsonValue)]) -> JsonMap<String, JsonValue> {
        pairs
            .iter()
            .map(|(key, value)| ((*key).to_string(), value.clone()))
            .collect()
    }

    fn draft(
        event: &str,
        matcher: Option<&str>,
        handler_type: &str,
        pairs: &[(&str, JsonValue)],
    ) -> HookHandlerDraftDto {
        HookHandlerDraftDto {
            event: event.to_string(),
            matcher: matcher.map(str::to_string),
            handler_type: handler_type.to_string(),
            fields: field_map(pairs),
        }
    }

    fn locator(event: &str, group_index: usize, handler_index: usize) -> HookHandlerLocatorDto {
        HookHandlerLocatorDto {
            event: event.to_string(),
            group_index,
            handler_index,
        }
    }

    fn parsed(text: &str) -> JsonValue {
        serde_json::from_str(text).expect("transformed document is valid JSON")
    }

    fn handler_at<'a>(
        document: &'a JsonValue,
        event: &str,
        group: usize,
        handler: usize,
    ) -> &'a JsonValue {
        &document["hooks"][event][group]["hooks"][handler]
    }

    fn issues_of(result: Result<HookTransform, HookTransformFailure>) -> Vec<HookValidationIssueDto> {
        match result {
            Err(HookTransformFailure::Invalid(issues)) => issues,
            other => panic!("expected an invalid draft, got {other:?}"),
        }
    }

    // Requirement: Agent-specific operations validate before transformation
    // Scenario: Valid Codex operation produces a transformed document
    #[test]
    fn operation_codex_update_changes_matcher_and_timeout() {
        let transform = transform_source(
            HookAgent::Codex,
            HookFormat::Json,
            Some(CODEX_HOOKS_JSON),
            &[HookEditOperationDto::UpdateHandler {
                locator: locator("PreToolUse", 0, 0),
                draft: draft(
                    "PreToolUse",
                    Some("Shell"),
                    "command",
                    &[
                        ("command", json!("echo pre")),
                        ("timeout", json!(30)),
                    ],
                ),
            }],
        )
        .expect("a documented Codex update is valid");

        let document = parsed(&transform.document_text);
        assert_eq!(document["hooks"]["PreToolUse"][0]["matcher"], "Shell");
        let handler = handler_at(&document, "PreToolUse", 0, 0);
        assert_eq!(handler["type"], "command");
        assert_eq!(handler["timeout"], 30);
        // Same event, same position: an update never moves a handler.
        assert_eq!(document["hooks"]["PreToolUse"].as_array().expect("groups").len(), 1);
        assert_eq!(
            document["hooks"]["PreToolUse"][0]["hooks"]
                .as_array()
                .expect("handlers")
                .len(),
            1
        );
        assert!(transform.after_canonical.contains("Shell"));
        assert!(transform.before_canonical.contains("Bash"));
    }

    // Scenario: Claude-only handler is rejected for Codex
    #[test]
    fn operation_claude_only_handler_type_is_rejected_for_codex() {
        let issues = issues_of(transform_source(
            HookAgent::Codex,
            HookFormat::Json,
            Some(CODEX_HOOKS_JSON),
            &[HookEditOperationDto::CreateHandler {
                draft: draft(
                    "PreToolUse",
                    Some("Bash"),
                    "http",
                    &[("url", json!("https://example.test/hook"))],
                ),
            }],
        ));

        let handler_type_issue = issues
            .iter()
            .find(|issue| issue.field == "handlerType")
            .expect("a handlerType issue");
        assert_eq!(handler_type_issue.operation_index, 0);
        assert_eq!(handler_type_issue.code, HookIssueCode::UnknownHandlerType);
    }

    #[test]
    fn operation_unknown_event_is_rejected() {
        let issues = issues_of(transform_source(
            HookAgent::Codex,
            HookFormat::Json,
            Some(CODEX_HOOKS_JSON),
            &[HookEditOperationDto::CreateHandler {
                draft: draft(
                    "NotAnEvent",
                    None,
                    "command",
                    &[("command", json!("echo hi"))],
                ),
            }],
        ));

        assert!(issues
            .iter()
            .any(|issue| issue.field == "event" && issue.code == HookIssueCode::UnknownEvent));
    }

    /// The API surface has no way to write an unknown value, so a request that
    /// names one is refused rather than quietly ignored.
    #[test]
    fn operation_unknown_field_cannot_be_set_by_a_request() {
        let issues = issues_of(transform_source(
            HookAgent::Codex,
            HookFormat::Json,
            Some(CODEX_HOOKS_JSON),
            &[HookEditOperationDto::UpdateHandler {
                locator: locator("PreToolUse", 0, 0),
                draft: draft(
                    "PreToolUse",
                    Some("Bash"),
                    "command",
                    &[
                        ("command", json!("echo pre")),
                        ("vendorOption", json!("overwritten")),
                    ],
                ),
            }],
        ));

        assert!(issues
            .iter()
            .any(|issue| issue.field == "vendorOption" && issue.code == HookIssueCode::UnknownField));
    }

    // Scenario: Existing unknown field survives an unrelated update
    #[test]
    fn operation_unknown_field_survives_an_unrelated_update() {
        let transform = transform_source(
            HookAgent::Codex,
            HookFormat::Json,
            Some(CODEX_HOOKS_JSON),
            &[HookEditOperationDto::UpdateHandler {
                locator: locator("PreToolUse", 0, 0),
                draft: draft(
                    "PreToolUse",
                    Some("Bash"),
                    "command",
                    &[
                        ("command", json!("echo pre")),
                        ("timeout", json!(45)),
                    ],
                ),
            }],
        )
        .expect("updating a known field is valid");

        let document = parsed(&transform.document_text);
        let handler = handler_at(&document, "PreToolUse", 0, 0);
        assert_eq!(handler["vendorOption"], "keep-me");
        assert_eq!(handler["timeout"], 45);
    }

    // Scenario: Stale locator is rejected
    #[test]
    fn operation_stale_locator_is_rejected() {
        let failure = transform_source(
            HookAgent::Codex,
            HookFormat::Json,
            Some(CODEX_HOOKS_JSON),
            &[HookEditOperationDto::UpdateHandler {
                locator: locator("PreToolUse", 1, 2),
                draft: draft(
                    "PreToolUse",
                    Some("Bash"),
                    "command",
                    &[("command", json!("echo pre"))],
                ),
            }],
        )
        .expect_err("a locator with no unique handler must fail");

        assert_eq!(failure, HookTransformFailure::Stale);
        assert_eq!(failure.code(), HookWriteError::StaleDraft);
    }

    #[test]
    fn operation_claude_create_handler_adds_a_new_event() {
        let transform = transform_source(
            HookAgent::ClaudeCode,
            HookFormat::Json,
            Some(CLAUDE_SETTINGS_JSON),
            &[HookEditOperationDto::CreateHandler {
                draft: draft(
                    "PreToolUse",
                    Some("Bash"),
                    "command",
                    &[("command", json!("echo pre"))],
                ),
            }],
        )
        .expect("a documented Claude Code create is valid");

        let document = parsed(&transform.document_text);
        assert_eq!(
            handler_at(&document, "PreToolUse", 0, 0)["command"],
            "echo pre"
        );
        // The event that was already there is untouched.
        assert_eq!(
            handler_at(&document, "PostToolUse", 0, 0)["command"],
            "echo post"
        );
    }

    #[test]
    fn operation_delete_handler_removes_only_the_located_handler() {
        let transform = transform_source(
            HookAgent::ClaudeCode,
            HookFormat::Json,
            Some(CLAUDE_SETTINGS_JSON),
            &[HookEditOperationDto::DeleteHandler {
                locator: locator("PostToolUse", 0, 0),
            }],
        )
        .expect("deleting a located handler is valid");

        let document = parsed(&transform.document_text);
        let handlers = document["hooks"]["PostToolUse"][0]["hooks"]
            .as_array()
            .expect("handlers");
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0]["type"], "vendor_future");
    }

    /// An unknown handler cannot be edited field by field, but it can be removed
    /// whole — otherwise a source with one unknown value would be uneditable.
    #[test]
    fn operation_delete_removes_a_handler_with_unknown_values() {
        let transform = transform_source(
            HookAgent::ClaudeCode,
            HookFormat::Json,
            Some(CLAUDE_SETTINGS_JSON),
            &[HookEditOperationDto::DeleteHandler {
                locator: locator("PostToolUse", 0, 1),
            }],
        )
        .expect("deleting an unknown handler is valid");

        let document = parsed(&transform.document_text);
        let handlers = document["hooks"]["PostToolUse"][0]["hooks"]
            .as_array()
            .expect("handlers");
        assert_eq!(handlers.len(), 1);
        assert_eq!(handlers[0]["type"], "command");
        assert!(!transform.after_canonical.contains("vendor_future"));
    }

    #[test]
    fn operation_create_into_a_missing_source_produces_a_complete_document() {
        let transform = transform_source(
            HookAgent::Codex,
            HookFormat::Json,
            None,
            &[HookEditOperationDto::CreateHandler {
                draft: draft(
                    "SessionStart",
                    Some("startup"),
                    "command",
                    &[("command", json!("echo start"))],
                ),
            }],
        )
        .expect("creating the first handler of a missing source is valid");

        assert_eq!(transform.before_canonical, "");
        let document = parsed(&transform.document_text);
        assert_eq!(
            handler_at(&document, "SessionStart", 0, 0)["command"],
            "echo start"
        );
    }

    #[test]
    fn operation_unparsable_source_is_not_edited() {
        let failure = transform_source(
            HookAgent::Codex,
            HookFormat::Json,
            Some("{ broken"),
            &[HookEditOperationDto::CreateHandler {
                draft: draft(
                    "SessionStart",
                    None,
                    "command",
                    &[("command", json!("echo start"))],
                ),
            }],
        )
        .expect_err("an unparsable source must not be rewritten");

        assert_eq!(failure, HookTransformFailure::Unparsable);
    }

    // Requirement: Round trips preserve configuration outside edited fields
    // Scenario: JSON sibling secret survives without entering the preview
    #[test]
    fn round_trip_json_sibling_secret_survives_and_stays_out_of_the_preview() {
        let transform = transform_source(
            HookAgent::ClaudeCode,
            HookFormat::Json,
            Some(CLAUDE_SETTINGS_JSON),
            &[HookEditOperationDto::UpdateHandler {
                locator: locator("PostToolUse", 0, 0),
                draft: draft(
                    "PostToolUse",
                    Some("Write"),
                    "command",
                    &[("command", json!("echo edited"))],
                ),
            }],
        )
        .expect("updating a Claude Code command is valid");

        let document = parsed(&transform.document_text);
        assert_eq!(document["apiToken"], "sentinel-secret");
        assert_eq!(
            handler_at(&document, "PostToolUse", 0, 0)["command"],
            "echo edited"
        );

        // The sibling never crosses the IPC boundary.
        for canonical in [&transform.before_canonical, &transform.after_canonical] {
            assert!(!canonical.contains("apiToken"));
            assert!(!canonical.contains("sentinel-secret"));
        }
    }

    // Scenario: TOML comments and unknown tables survive a Hook update
    #[test]
    fn round_trip_toml_comments_and_unknown_tables_survive() {
        let transform = transform_source(
            HookAgent::Codex,
            HookFormat::Toml,
            Some(CODEX_CONFIG_TOML),
            &[HookEditOperationDto::UpdateHandler {
                locator: locator("SessionStart", 0, 0),
                draft: draft(
                    "SessionStart",
                    Some("startup|resume"),
                    "command",
                    &[
                        ("command", json!("echo start")),
                        ("additionalContextLimit", json!(9000)),
                    ],
                ),
            }],
        )
        .expect("updating a documented Codex field is valid");

        let written = &transform.document_text;
        for preserved in [
            "# top of file",
            "[profiles.work]",
            "# work profile stays put",
            "approval = \"on-request\"",
            "# everything below configures hooks",
            "# the handler comment",
            "vendorOption = \"keep-me\"",
            "[telemetry]",
            "enabled = false",
        ] {
            assert!(written.contains(preserved), "{preserved} must survive");
        }

        let position = |needle: &str| written.find(needle).expect("needle present");
        assert!(position("[profiles.work]") < position("[[hooks.SessionStart]]"));
        assert!(position("[[hooks.SessionStart]]") < position("[telemetry]"));

        let before: Vec<&str> = CODEX_CONFIG_TOML.lines().collect();
        let after: Vec<&str> = written.lines().collect();
        assert_eq!(before.len(), after.len(), "no line may be added or removed");
        let changed: Vec<(usize, &str, &str)> = before
            .iter()
            .zip(after.iter())
            .enumerate()
            .filter(|(_, (b, a))| b != a)
            .map(|(index, (b, a))| (index, *b, *a))
            .collect();
        assert_eq!(changed.len(), 1, "exactly one line may differ: {changed:?}");
        assert!(changed[0].1.contains("5000"));
        assert!(changed[0].2.contains("additionalContextLimit"));
        assert!(changed[0].2.contains("9000"));
    }

    // -----------------------------------------------------------------------
    // Preview
    // -----------------------------------------------------------------------

    /// A Codex Hook subtree whose canonical text is exactly `bytes` long.
    fn codex_source_of_canonical_bytes(bytes: usize) -> String {
        let subtree = |pad: usize| {
            json!({
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{ "type": "command", "command": "x".repeat(pad), "timeout": 10 }]
                }]
            })
        };
        let base = serde_json::to_string_pretty(&subtree(0))
            .expect("canonical")
            .len();
        assert!(bytes >= base, "requested size is below the smallest subtree");
        let padded = subtree(bytes - base);
        assert_eq!(
            serde_json::to_string_pretty(&padded).expect("canonical").len(),
            bytes
        );
        format!(
            r#"{{"hooks":{}}}"#,
            serde_json::to_string(&padded).expect("compact")
        )
    }

    /// A Claude Code Hook subtree whose canonical text has exactly `lines`
    /// lines. Each `args` element is one pretty-printed line, so the count is
    /// adjustable by one.
    fn claude_source_of_canonical_lines(lines: usize) -> (String, Vec<JsonValue>) {
        let subtree = |args: usize| {
            json!({
                "PostToolUse": [{
                    "matcher": "Write",
                    "hooks": [{
                        "type": "command",
                        "command": "echo",
                        "timeout": 10,
                        "args": vec!["a"; args]
                    }]
                }]
            })
        };
        let one = serde_json::to_string_pretty(&subtree(1))
            .expect("canonical")
            .lines()
            .count();
        let args = lines + 1 - one;
        let padded = subtree(args);
        assert_eq!(
            serde_json::to_string_pretty(&padded)
                .expect("canonical")
                .lines()
                .count(),
            lines
        );
        (
            format!(
                r#"{{"hooks":{}}}"#,
                serde_json::to_string(&padded).expect("compact")
            ),
            vec![json!("a"); args],
        )
    }

    /// Changes `timeout` from 10 to 30: two digits either way, so the canonical
    /// text keeps the exact size the boundary tests are pinned to.
    fn size_neutral_codex_update(pad: usize) -> HookEditOperationDto {
        HookEditOperationDto::UpdateHandler {
            locator: locator("PreToolUse", 0, 0),
            draft: draft(
                "PreToolUse",
                Some("Bash"),
                "command",
                &[
                    ("command", json!("x".repeat(pad))),
                    ("timeout", json!(30)),
                ],
            ),
        }
    }

    fn size_neutral_claude_update(args: Vec<JsonValue>) -> HookEditOperationDto {
        HookEditOperationDto::UpdateHandler {
            locator: locator("PostToolUse", 0, 0),
            draft: draft(
                "PostToolUse",
                Some("Write"),
                "command",
                &[
                    ("command", json!("echo")),
                    ("timeout", json!(30)),
                    ("args", JsonValue::Array(args)),
                ],
            ),
        }
    }

    fn too_large_issue(preview: &HookWritePreviewDto) -> bool {
        preview
            .validation_issues
            .iter()
            .any(|issue| issue.code == HookIssueCode::PreviewTooLarge)
    }

    // Requirement: Preview binds an exact source revision to validated operations
    // Scenario: Preview returns exact before and after Hook fragments
    #[test]
    fn preview_returns_exact_before_and_after_fragments() {
        let home = tempfile::tempdir().expect("home");
        let source = home.path().join(".claude").join("settings.json");
        write_fixture(&source, CLAUDE_SETTINGS_JSON);

        let preview = preview_change(
            Some(home.path()),
            None,
            "claude_code:user:settings-json",
            &[HookEditOperationDto::CreateHandler {
                draft: draft(
                    "PreToolUse",
                    Some("Bash"),
                    "command",
                    &[("command", json!("echo pre"))],
                ),
            }],
            no_project,
        )
        .expect("preview succeeds");

        assert_eq!(preview.source_id, "claude_code:user:settings-json");
        assert_eq!(
            preview.base_revision,
            revision_of(Some(CLAUDE_SETTINGS_JSON.as_bytes()))
        );
        assert!(preview.before_canonical_text.contains("PostToolUse"));
        assert!(!preview.before_canonical_text.contains("PreToolUse"));
        assert!(preview.after_canonical_text.contains("PostToolUse"));
        assert!(preview.after_canonical_text.contains("PreToolUse"));
        // The sibling key never reaches the frontend.
        assert!(!preview.after_canonical_text.contains("apiToken"));
        assert!(!preview.after_canonical_text.contains("sentinel-secret"));
        assert!(preview.can_apply);
        assert!(!preview.would_create_file);
        assert!(preview.validation_issues.is_empty());
    }

    // Scenario: Missing fixed source previews file creation
    #[test]
    fn preview_missing_source_reports_creation_and_writes_nothing() {
        let home = tempfile::tempdir().expect("home");
        let before = snapshot(home.path());

        let preview = preview_change(
            Some(home.path()),
            None,
            "codex:user:hooks-json",
            &[HookEditOperationDto::CreateHandler {
                draft: draft(
                    "SessionStart",
                    Some("startup"),
                    "command",
                    &[("command", json!("echo start"))],
                ),
            }],
            no_project,
        )
        .expect("preview of a missing source succeeds");

        assert_eq!(preview.base_revision, MISSING_REVISION);
        assert!(preview.would_create_file);
        assert!(preview.can_apply);
        assert_eq!(preview.before_canonical_text, "");
        assert_eq!(
            before,
            snapshot(home.path()),
            "preview must not create the parent or the file"
        );
    }

    #[test]
    fn preview_invalid_draft_reports_issues_and_blocks_apply() {
        let home = tempfile::tempdir().expect("home");
        write_fixture(
            &home.path().join(".codex").join("hooks.json"),
            CODEX_HOOKS_JSON,
        );

        let preview = preview_change(
            Some(home.path()),
            None,
            "codex:user:hooks-json",
            &[HookEditOperationDto::CreateHandler {
                draft: draft(
                    "PreToolUse",
                    Some("Bash"),
                    "http",
                    &[("url", json!("https://example.test/hook"))],
                ),
            }],
            no_project,
        )
        .expect("an invalid draft still returns a preview");

        assert!(!preview.can_apply);
        assert!(preview
            .validation_issues
            .iter()
            .any(|issue| issue.field == "handlerType"));
    }

    #[test]
    fn preview_stale_locator_fails() {
        let home = tempfile::tempdir().expect("home");
        write_fixture(
            &home.path().join(".codex").join("hooks.json"),
            CODEX_HOOKS_JSON,
        );

        let error = preview_change(
            Some(home.path()),
            None,
            "codex:user:hooks-json",
            &[HookEditOperationDto::DeleteHandler {
                locator: locator("PreToolUse", 1, 2),
            }],
            no_project,
        )
        .expect_err("a stale locator must fail");

        assert_eq!(error, HookWriteError::StaleDraft);
    }

    // Scenario: Preview refuses oversized diff input
    // Example: Preview size boundaries
    #[test]
    fn preview_accepts_the_exact_byte_boundary() {
        let home = tempfile::tempdir().expect("home");
        let pad = 262_144
            - serde_json::to_string_pretty(&json!({
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{ "type": "command", "command": "", "timeout": 10 }]
                }]
            }))
            .expect("canonical")
            .len();
        write_fixture(
            &home.path().join(".codex").join("hooks.json"),
            &codex_source_of_canonical_bytes(262_144),
        );

        let preview = preview_change(
            Some(home.path()),
            None,
            "codex:user:hooks-json",
            &[size_neutral_codex_update(pad)],
            no_project,
        )
        .expect("preview at the boundary succeeds");

        assert_eq!(preview.before_canonical_text.len(), 262_144);
        assert_eq!(preview.after_canonical_text.len(), 262_144);
        assert!(preview.can_apply);
        assert!(!too_large_issue(&preview));
    }

    #[test]
    fn preview_refuses_one_byte_over_the_boundary() {
        let home = tempfile::tempdir().expect("home");
        let pad = 262_145
            - serde_json::to_string_pretty(&json!({
                "PreToolUse": [{
                    "matcher": "Bash",
                    "hooks": [{ "type": "command", "command": "", "timeout": 10 }]
                }]
            }))
            .expect("canonical")
            .len();
        write_fixture(
            &home.path().join(".codex").join("hooks.json"),
            &codex_source_of_canonical_bytes(262_145),
        );

        let preview = preview_change(
            Some(home.path()),
            None,
            "codex:user:hooks-json",
            &[size_neutral_codex_update(pad)],
            no_project,
        )
        .expect("an oversized preview still answers");

        assert!(!preview.can_apply);
        assert!(too_large_issue(&preview));
        // The oversized text must not be handed to the line diff.
        assert_eq!(preview.before_canonical_text, "");
        assert_eq!(preview.after_canonical_text, "");
    }

    #[test]
    fn preview_accepts_the_exact_line_boundary() {
        let home = tempfile::tempdir().expect("home");
        let (source, args) = claude_source_of_canonical_lines(4_000);
        write_fixture(&home.path().join(".claude").join("settings.json"), &source);

        let preview = preview_change(
            Some(home.path()),
            None,
            "claude_code:user:settings-json",
            &[size_neutral_claude_update(args)],
            no_project,
        )
        .expect("preview at the line boundary succeeds");

        assert_eq!(preview.after_canonical_text.lines().count(), 4_000);
        assert!(preview.can_apply);
    }

    #[test]
    fn preview_refuses_one_line_over_the_boundary() {
        let home = tempfile::tempdir().expect("home");
        let (source, args) = claude_source_of_canonical_lines(4_001);
        write_fixture(&home.path().join(".claude").join("settings.json"), &source);

        let preview = preview_change(
            Some(home.path()),
            None,
            "claude_code:user:settings-json",
            &[size_neutral_claude_update(args)],
            no_project,
        )
        .expect("an oversized preview still answers");

        assert!(!preview.can_apply);
        assert!(too_large_issue(&preview));
        assert_eq!(preview.after_canonical_text, "");
    }

    #[test]
    fn preview_leaves_the_source_untouched() {
        let home = tempfile::tempdir().expect("home");
        let source = home.path().join(".codex").join("config.toml");
        write_fixture(&source, CODEX_CONFIG_TOML);
        let before = snapshot(home.path());

        let _ = preview_change(
            Some(home.path()),
            None,
            "codex:user:config-toml",
            &[HookEditOperationDto::DeleteHandler {
                locator: locator("SessionStart", 0, 0),
            }],
            no_project,
        )
        .expect("preview succeeds");

        assert_eq!(fs::read_to_string(&source).expect("read"), CODEX_CONFIG_TOML);
        assert_eq!(before, snapshot(home.path()));
    }

    // -----------------------------------------------------------------------
    // Apply
    // -----------------------------------------------------------------------

    fn store_in(dir: &Path) -> SkillStore {
        SkillStore::new(&dir.join("hooks-test.db")).expect("store")
    }

    fn codex_command_update(command: &str) -> HookEditOperationDto {
        HookEditOperationDto::UpdateHandler {
            locator: locator("PreToolUse", 0, 0),
            draft: draft(
                "PreToolUse",
                Some("Bash"),
                "command",
                &[("command", json!(command)), ("timeout", json!(10))],
            ),
        }
    }

    fn codex_create(command: &str) -> HookEditOperationDto {
        HookEditOperationDto::CreateHandler {
            draft: draft(
                "SessionStart",
                Some("startup"),
                "command",
                &[("command", json!(command))],
            ),
        }
    }

    /// All four failure points of the apply transaction, with the code each one
    /// must report.
    const FAULT_POINTS: &[(HookFaultPoint, HookWriteError)] = &[
        (HookFaultPoint::BackupPromote, HookWriteError::BackupFailed),
        (HookFaultPoint::StagedTargetSync, HookWriteError::WriteFailed),
        (HookFaultPoint::AtomicReplace, HookWriteError::WriteFailed),
        (HookFaultPoint::SqliteCommit, HookWriteError::WriteFailed),
    ];

    // Requirement: Apply is conflict-safe, recoverable, and atomic
    // Scenario: External modification blocks apply
    #[test]
    fn apply_rejects_a_stale_revision() {
        let home = tempfile::tempdir().expect("home");
        let backups = tempfile::tempdir().expect("backups");
        let db = tempfile::tempdir().expect("db");
        let store = store_in(db.path());
        let source = home.path().join(".codex").join("hooks.json");
        write_fixture(&source, CODEX_HOOKS_JSON);

        // The revision the user confirmed, before an external process wrote.
        let confirmed = revision_of(Some(CODEX_HOOKS_JSON.as_bytes()));
        let external = CODEX_HOOKS_JSON.replace("echo pre", "echo external");
        fs::write(&source, &external).expect("external write");

        let env = HookWriteEnv {
            home: Some(home.path()),
            backup_root: backups.path(),
            store: &store,
            fault: None,
        };
        let error = apply_change(
            &env,
            None,
            "codex:user:hooks-json",
            &confirmed,
            &[codex_command_update("echo edited")],
        )
        .expect_err("a stale revision must be refused");

        assert_eq!(error, HookWriteError::SourceConflict);
        assert_eq!(fs::read_to_string(&source).expect("read"), external);
        assert!(
            snapshot(backups.path()).is_empty(),
            "a refused apply must not create a recovery point"
        );
    }

    // Scenario: Successful apply creates backup before replacement
    #[test]
    fn apply_creates_a_recovery_point_before_replacement() {
        let home = tempfile::tempdir().expect("home");
        let backups = tempfile::tempdir().expect("backups");
        let db = tempfile::tempdir().expect("db");
        let store = store_in(db.path());
        let source = home.path().join(".codex").join("hooks.json");
        write_fixture(&source, CODEX_HOOKS_JSON);

        let env = HookWriteEnv {
            home: Some(home.path()),
            backup_root: backups.path(),
            store: &store,
            fault: None,
        };
        let outcome = apply_change(
            &env,
            None,
            "codex:user:hooks-json",
            &revision_of(Some(CODEX_HOOKS_JSON.as_bytes())),
            &[codex_command_update("echo edited")],
        )
        .expect("apply succeeds");

        let written = fs::read_to_string(&source).expect("read target");
        assert!(written.contains("echo edited"));
        assert_eq!(outcome.revision, revision_of(Some(written.as_bytes())));
        assert!(!outcome.created_file);

        let payload = backups
            .path()
            .join(&outcome.artifact_id)
            .join("latest");
        assert_eq!(
            fs::read_to_string(&payload).expect("recovery payload"),
            CODEX_HOOKS_JSON,
            "the recovery point holds the exact pre-apply bytes"
        );

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let file_mode = fs::metadata(&payload).expect("payload metadata").permissions().mode() & 0o777;
            let dir_mode = fs::metadata(payload.parent().expect("payload dir"))
                .expect("dir metadata")
                .permissions()
                .mode()
                & 0o777;
            assert_eq!(file_mode, 0o600, "recovery payloads are owner-private");
            assert_eq!(dir_mode, 0o700, "recovery directories are owner-private");
        }
    }

    #[test]
    fn apply_creates_a_missing_source_and_records_an_absence_marker() {
        let home = tempfile::tempdir().expect("home");
        let backups = tempfile::tempdir().expect("backups");
        let db = tempfile::tempdir().expect("db");
        let store = store_in(db.path());
        let source = home.path().join(".codex").join("hooks.json");

        let env = HookWriteEnv {
            home: Some(home.path()),
            backup_root: backups.path(),
            store: &store,
            fault: None,
        };
        let outcome = apply_change(
            &env,
            None,
            "codex:user:hooks-json",
            MISSING_REVISION,
            &[codex_create("echo start")],
        )
        .expect("apply creates the missing source");

        assert!(outcome.created_file);
        assert!(fs::read_to_string(&source)
            .expect("created target")
            .contains("echo start"));
        // An absence marker is metadata, not an empty payload file.
        assert!(
            !backups.path().join(&outcome.artifact_id).join("latest").exists(),
            "an absent source must not get a payload file"
        );
    }

    // Scenario: Injected write failure restores the original state
    #[test]
    fn apply_fault_injection_reports_a_sanitized_code_and_restores_bytes() {
        for (point, expected) in FAULT_POINTS {
            let home = tempfile::tempdir().expect("home");
            let backups = tempfile::tempdir().expect("backups");
            let db = tempfile::tempdir().expect("db");
            let store = store_in(db.path());
            let source = home.path().join(".codex").join("hooks.json");
            write_fixture(&source, CODEX_HOOKS_JSON);
            let tree_before = snapshot(home.path());

            let env = HookWriteEnv {
                home: Some(home.path()),
                backup_root: backups.path(),
                store: &store,
                fault: Some(*point),
            };
            let error = apply_change(
                &env,
                None,
                "codex:user:hooks-json",
                &revision_of(Some(CODEX_HOOKS_JSON.as_bytes())),
                &[codex_command_update("echo edited")],
            )
            .expect_err("an injected fault must fail the apply");

            assert_eq!(&error, expected, "unexpected code for {point:?}");
            assert_eq!(
                fs::read_to_string(&source).expect("read target"),
                CODEX_HOOKS_JSON,
                "{point:?} must leave the exact original bytes"
            );
            assert_eq!(
                tree_before,
                snapshot(home.path()),
                "{point:?} must not leave a staged file behind"
            );
        }
    }

    #[test]
    fn apply_fault_injection_leaves_a_created_source_absent() {
        for (point, expected) in FAULT_POINTS {
            let home = tempfile::tempdir().expect("home");
            let backups = tempfile::tempdir().expect("backups");
            let db = tempfile::tempdir().expect("db");
            let store = store_in(db.path());
            let source = home.path().join(".codex").join("hooks.json");

            let env = HookWriteEnv {
                home: Some(home.path()),
                backup_root: backups.path(),
                store: &store,
                fault: Some(*point),
            };
            let error = apply_change(
                &env,
                None,
                "codex:user:hooks-json",
                MISSING_REVISION,
                &[codex_create("echo start")],
            )
            .expect_err("an injected fault must fail the apply");

            assert_eq!(&error, expected, "unexpected code for {point:?}");
            assert!(
                !source.exists(),
                "{point:?} must leave the created source absent"
            );
        }
    }

    // Scenario: Unsupported atomic replacement fails before mutation
    #[test]
    fn apply_unsupported_atomic_replacement_fails_before_mutation() {
        let home = tempfile::tempdir().expect("home");
        let backups = tempfile::tempdir().expect("backups");
        let db = tempfile::tempdir().expect("db");
        let store = store_in(db.path());
        let source = home.path().join(".codex").join("hooks.json");
        write_fixture(&source, CODEX_HOOKS_JSON);
        let tree_before = snapshot(home.path());

        let env = HookWriteEnv {
            home: Some(home.path()),
            backup_root: backups.path(),
            store: &store,
            fault: Some(HookFaultPoint::AtomicReplaceUnsupported),
        };
        let error = apply_change(
            &env,
            None,
            "codex:user:hooks-json",
            &revision_of(Some(CODEX_HOOKS_JSON.as_bytes())),
            &[codex_command_update("echo edited")],
        )
        .expect_err("an unsupported platform must refuse before writing");

        assert_eq!(error, HookWriteError::AtomicReplaceUnsupported);
        assert_eq!(
            fs::read_to_string(&source).expect("read target"),
            CODEX_HOOKS_JSON
        );
        assert_eq!(tree_before, snapshot(home.path()));
        assert!(
            snapshot(backups.path()).is_empty(),
            "nothing may be backed up before the platform check"
        );
    }

    // -----------------------------------------------------------------------
    // Recovery and restore
    // -----------------------------------------------------------------------

    /// A home, a private backup root and a store, wired the way apply expects.
    struct Workspace {
        home: tempfile::TempDir,
        backups: tempfile::TempDir,
        db: tempfile::TempDir,
        store: SkillStore,
    }

    fn workspace() -> Workspace {
        let db = tempfile::tempdir().expect("db");
        let store = store_in(db.path());
        Workspace {
            home: tempfile::tempdir().expect("home"),
            backups: tempfile::tempdir().expect("backups"),
            db,
            store,
        }
    }

    impl Workspace {
        fn env(&self, fault: Option<HookFaultPoint>) -> HookWriteEnv<'_> {
            HookWriteEnv {
                home: Some(self.home.path()),
                backup_root: self.backups.path(),
                store: &self.store,
                fault,
            }
        }

        fn source(&self) -> PathBuf {
            self.home.path().join(".codex").join("hooks.json")
        }

        /// Every byte of the database directory, so a value cannot hide in a
        /// write-ahead log the main file has not absorbed yet.
        fn database_bytes(&self) -> Vec<u8> {
            let mut bytes = Vec::new();
            for entry in walkdir::WalkDir::new(self.db.path())
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
            {
                bytes.extend(fs::read(entry.path()).expect("read database file"));
            }
            bytes
        }
    }

    // Requirement: Restore requires preview and preserves a reverse recovery point
    // Scenario: Restore reverts the latest apply
    #[test]
    fn restore_reverts_the_latest_apply() {
        let workspace = workspace();
        write_fixture(&workspace.source(), CODEX_HOOKS_JSON);

        let applied = apply_change(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &revision_of(Some(CODEX_HOOKS_JSON.as_bytes())),
            &[codex_command_update("echo edited")],
        )
        .expect("apply succeeds");
        let after_apply = fs::read_to_string(workspace.source()).expect("read");

        let recovery = get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
            .expect("recovery query")
            .expect("a recovery point exists");
        assert!(recovery.can_restore);
        assert_eq!(recovery.kind, "bytes");
        assert_eq!(recovery.artifact_id, applied.artifact_id);

        let preview = preview_restore(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &recovery.backup_id,
        )
        .expect("restore preview");
        assert!(preview.can_apply);
        assert!(preview.before_canonical_text.contains("echo edited"));
        assert!(preview.after_canonical_text.contains("echo pre"));

        apply_restore(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &recovery.backup_id,
            &preview.base_revision,
        )
        .expect("restore succeeds");

        assert_eq!(
            fs::read_to_string(workspace.source()).expect("read"),
            CODEX_HOOKS_JSON,
            "the source returns to the exact pre-apply bytes"
        );

        // The restore itself left a reverse recovery point.
        let reverse = get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
            .expect("recovery query")
            .expect("a reverse recovery point exists");
        assert_ne!(reverse.backup_id, recovery.backup_id);
        let payload = workspace
            .backups
            .path()
            .join(&applied.artifact_id)
            .join("latest");
        assert_eq!(
            fs::read_to_string(&payload).expect("reverse payload"),
            after_apply
        );
    }

    #[test]
    fn restore_can_be_undone_by_restoring_again() {
        let workspace = workspace();
        write_fixture(&workspace.source(), CODEX_HOOKS_JSON);

        apply_change(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &revision_of(Some(CODEX_HOOKS_JSON.as_bytes())),
            &[codex_command_update("echo edited")],
        )
        .expect("apply");
        let after_apply = fs::read_to_string(workspace.source()).expect("read");

        for _ in 0..2 {
            let recovery = get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
                .expect("recovery query")
                .expect("recovery point");
            apply_restore(
                &workspace.env(None),
                None,
                "codex:user:hooks-json",
                &recovery.backup_id,
                &recovery.base_revision,
            )
            .expect("restore");
        }

        assert_eq!(
            fs::read_to_string(workspace.source()).expect("read"),
            after_apply,
            "restoring twice returns to the applied state"
        );
    }

    // Scenario: Restore of an absence marker removes only an unchanged created file
    #[test]
    fn restore_of_an_absence_marker_removes_the_created_file() {
        let workspace = workspace();
        let other = workspace.home.path().join(".claude").join("settings.json");
        write_fixture(&other, CLAUDE_SETTINGS_JSON);

        apply_change(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            MISSING_REVISION,
            &[codex_create("echo start")],
        )
        .expect("apply creates the source");
        assert!(workspace.source().exists());

        let recovery = get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
            .expect("recovery query")
            .expect("recovery point");
        assert_eq!(recovery.kind, "absent");

        let preview = preview_restore(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &recovery.backup_id,
        )
        .expect("restore preview");
        apply_restore(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &recovery.backup_id,
            &preview.base_revision,
        )
        .expect("restore removes the created file");

        assert!(!workspace.source().exists());
        assert!(
            workspace.source().parent().expect("parent").exists(),
            "parent directories stay in place"
        );
        assert_eq!(
            fs::read_to_string(&other).expect("read"),
            CLAUDE_SETTINGS_JSON,
            "every other source is untouched"
        );
    }

    // Scenario: Modified source or corrupt backup blocks restore
    #[test]
    fn restore_refuses_a_source_changed_after_preview() {
        let workspace = workspace();
        write_fixture(&workspace.source(), CODEX_HOOKS_JSON);
        apply_change(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &revision_of(Some(CODEX_HOOKS_JSON.as_bytes())),
            &[codex_command_update("echo edited")],
        )
        .expect("apply");

        let recovery = get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
            .expect("recovery query")
            .expect("recovery point");
        let confirmed = recovery.base_revision.clone();

        let external = CODEX_HOOKS_JSON.replace("echo pre", "echo external");
        fs::write(workspace.source(), &external).expect("external write");

        let error = apply_restore(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &recovery.backup_id,
            &confirmed,
        )
        .expect_err("a changed source must block restore");

        assert_eq!(error, HookWriteError::SourceConflict);
        assert_eq!(fs::read_to_string(workspace.source()).expect("read"), external);
        let still = get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
            .expect("recovery query")
            .expect("recovery point");
        assert_eq!(
            still.backup_id, recovery.backup_id,
            "no recovery point may be replaced"
        );
    }

    #[test]
    fn restore_refuses_a_corrupt_recovery_payload() {
        let workspace = workspace();
        write_fixture(&workspace.source(), CODEX_HOOKS_JSON);
        let applied = apply_change(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &revision_of(Some(CODEX_HOOKS_JSON.as_bytes())),
            &[codex_command_update("echo edited")],
        )
        .expect("apply");
        let after_apply = fs::read_to_string(workspace.source()).expect("read");

        let recovery = get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
            .expect("recovery query")
            .expect("recovery point");
        fs::write(
            workspace
                .backups
                .path()
                .join(&applied.artifact_id)
                .join("latest"),
            "corrupted",
        )
        .expect("corrupt the payload");

        let error = apply_restore(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &recovery.backup_id,
            &recovery.base_revision,
        )
        .expect_err("a payload that does not match its hash must fail closed");

        assert_eq!(error, HookWriteError::RestoreFailed);
        assert_eq!(
            fs::read_to_string(workspace.source()).expect("read"),
            after_apply,
            "the current source must not be deleted or rewritten"
        );
    }

    #[test]
    fn recovery_reports_only_the_latest_point() {
        let workspace = workspace();
        write_fixture(&workspace.source(), CODEX_HOOKS_JSON);

        let first = apply_change(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &revision_of(Some(CODEX_HOOKS_JSON.as_bytes())),
            &[codex_command_update("echo first")],
        )
        .expect("first apply");
        let after_first = fs::read_to_string(workspace.source()).expect("read");
        let second = apply_change(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &revision_of(Some(after_first.as_bytes())),
            &[codex_command_update("echo second")],
        )
        .expect("second apply");

        assert_eq!(
            first.artifact_id, second.artifact_id,
            "a later apply reuses the same identity"
        );
        let recovery = get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
            .expect("recovery query")
            .expect("recovery point");
        assert_eq!(recovery.backup_id, second.backup_id);
        assert_eq!(
            fs::read_to_string(
                workspace
                    .backups
                    .path()
                    .join(&second.artifact_id)
                    .join("latest")
            )
            .expect("payload"),
            after_first,
            "only the state before the last apply is retained"
        );
    }

    #[test]
    fn recovery_is_absent_before_the_first_apply() {
        let workspace = workspace();
        write_fixture(&workspace.source(), CODEX_HOOKS_JSON);

        let recovery = get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
            .expect("recovery query");

        assert!(recovery.is_none());
    }

    // Scenario: Sensitive payload exists only in authorized transient and recovery locations
    #[test]
    fn recovery_metadata_excludes_hook_payload() {
        let workspace = workspace();
        write_fixture(&workspace.source(), CODEX_HOOKS_JSON);

        let outcome = apply_change(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &revision_of(Some(CODEX_HOOKS_JSON.as_bytes())),
            &[codex_command_update("echo sentinel-secret")],
        )
        .expect("apply");

        // Authorized locations.
        assert!(fs::read_to_string(workspace.source())
            .expect("read")
            .contains("sentinel-secret"));

        // Everything persisted about the write is free of Hook content.
        let database = workspace.database_bytes();
        assert!(
            !String::from_utf8_lossy(&database).contains("sentinel-secret"),
            "no Hook payload may reach SQLite"
        );

        let recovery = get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
            .expect("recovery query")
            .expect("recovery point");
        let serialized = serde_json::to_string(&recovery).expect("serialize");
        assert!(!serialized.contains("sentinel-secret"));
        assert!(!serialized.contains("echo"));
        assert_eq!(recovery.artifact_id, outcome.artifact_id);
    }

    // Requirement: Interrupted Hook writes recover before new mutations
    /// Builds the state a crash between replacement and commit leaves behind.
    fn interrupted_journal(workspace: &Workspace, artifact_id: &str, original: &str) -> String {
        let payload_dir = workspace.backups.path().join(artifact_id);
        fs::create_dir_all(&payload_dir).expect("payload dir");
        fs::write(payload_dir.join("latest"), original).expect("payload");

        let journal = HookOperationJournal {
            operation_id: "operation-1".to_string(),
            artifact_id: artifact_id.to_string(),
            source_id: "codex:user:hooks-json".to_string(),
            context_key: "global".to_string(),
            backup_id: "backup-1".to_string(),
            backup_kind: "bytes".to_string(),
            backup_locator: format!("{artifact_id}/latest"),
            base_revision: revision_of(Some(original.as_bytes())),
            target_revision: revision_of(Some(
                fs::read(workspace.source()).expect("target").as_slice(),
            )),
        };
        let path = journal_path(workspace.backups.path(), &journal.operation_id);
        fs::create_dir_all(path.parent().expect("journal dir")).expect("journal dir");
        fs::write(
            &path,
            serde_json::to_string(&journal).expect("serialize journal"),
        )
        .expect("write journal");
        journal.operation_id
    }

    // Scenario: Startup restores an interrupted uncommitted write
    #[test]
    fn recovery_startup_restores_an_interrupted_write() {
        let workspace = workspace();
        // The target already carries the replacement; the metadata never landed.
        write_fixture(
            &workspace.source(),
            &CODEX_HOOKS_JSON.replace("echo pre", "echo interrupted"),
        );
        let operation = interrupted_journal(&workspace, "artifact-1", CODEX_HOOKS_JSON);

        assert_eq!(
            ensure_mutations_allowed(workspace.backups.path()),
            Err(HookWriteError::RecoveryRequired),
            "an unfinished journal blocks mutation"
        );

        reconcile_pending_operations(&workspace.env(None)).expect("reconciliation succeeds");

        assert_eq!(
            fs::read_to_string(workspace.source()).expect("read"),
            CODEX_HOOKS_JSON,
            "the target is restored from its recovery point"
        );
        assert!(
            !journal_path(workspace.backups.path(), &operation).exists(),
            "the journal is cleared once target and database agree"
        );
        assert!(ensure_mutations_allowed(workspace.backups.path()).is_ok());
        assert!(
            workspace
                .store
                .get_hook_detail("codex:user:hooks-json", "global")
                .expect("query")
                .is_none(),
            "uncommitted Hook metadata is removed"
        );
    }

    // Scenario: Failed reconciliation blocks mutation only
    #[test]
    fn recovery_failed_reconciliation_keeps_blocking_mutations() {
        let workspace = workspace();
        let interrupted = CODEX_HOOKS_JSON.replace("echo pre", "echo interrupted");
        write_fixture(&workspace.source(), &interrupted);
        let operation = interrupted_journal(&workspace, "artifact-1", CODEX_HOOKS_JSON);
        // The recovery payload the journal points at is gone.
        fs::remove_file(workspace.backups.path().join("artifact-1").join("latest"))
            .expect("remove payload");

        reconcile_pending_operations(&workspace.env(None))
            .expect_err("an unreadable recovery payload cannot be reconciled");

        assert_eq!(
            ensure_mutations_allowed(workspace.backups.path()),
            Err(HookWriteError::RecoveryRequired)
        );
        assert_eq!(
            apply_change(
                &workspace.env(None),
                None,
                "codex:user:hooks-json",
                &revision_of(Some(interrupted.as_bytes())),
                &[codex_command_update("echo edited")],
            ),
            Err(HookWriteError::RecoveryRequired),
            "mutation stays blocked while recovery is pending"
        );
        assert_eq!(
            fs::read_to_string(workspace.source()).expect("read"),
            interrupted,
            "the current source is not overwritten"
        );
        assert!(journal_path(workspace.backups.path(), &operation).exists());
    }

    // -----------------------------------------------------------------------
    // End to end, against a temporary HOME and a linked Project
    // -----------------------------------------------------------------------

    /// The whole flow on both formats at once: preview, external-edit conflict,
    /// apply, restore and crash recovery, with a sentinel value planted in every
    /// place it is allowed to be so the places it is not can be asserted empty.
    #[test]
    fn end_to_end_preview_apply_restore_and_recovery_keep_payload_contained() {
        use crate::core::skill_store::ProjectRecord;

        let db = tempfile::tempdir().expect("db");
        let store = store_in(db.path());
        let home = tempfile::tempdir().expect("home");
        let backups = tempfile::tempdir().expect("backups");
        let project = tempfile::tempdir().expect("project");
        store
            .insert_project(&ProjectRecord {
                id: "project-1".to_string(),
                name: "Demo".to_string(),
                path: project.path().to_string_lossy().to_string(),
                workspace_type: "project".to_string(),
                linked_agent_key: None,
                linked_agent_name: None,
                disabled_path: None,
                sort_order: 0,
                created_at: 1,
                updated_at: 1,
            })
            .expect("link the project");

        let env = || HookWriteEnv {
            home: Some(home.path()),
            backup_root: backups.path(),
            store: &store,
            fault: None,
        };
        let lookup = |id: &str| {
            store
                .get_project_by_id(id)
                .ok()
                .flatten()
                .map(|record| record.path)
        };

        // A JSON source whose non-Hook sibling holds the sentinel, and a TOML
        // source with comments and an unknown Hook value.
        let json_source = project.path().join(".claude").join("settings.json");
        write_fixture(&json_source, CLAUDE_SETTINGS_JSON);
        let toml_source = home.path().join(".codex").join("config.toml");
        write_fixture(&toml_source, CODEX_CONFIG_TOML);

        // ── TOML preview and apply, comments intact ──
        let toml_operations = [HookEditOperationDto::UpdateHandler {
            locator: locator("SessionStart", 0, 0),
            draft: draft(
                "SessionStart",
                Some("startup|resume"),
                "command",
                &[
                    ("command", json!("echo start")),
                    ("additionalContextLimit", json!(9000)),
                ],
            ),
        }];
        let toml_preview = preview_change(
            Some(home.path()),
            None,
            "codex:user:config-toml",
            &toml_operations,
            lookup,
        )
        .expect("TOML preview");
        assert!(toml_preview.can_apply);
        apply_change(
            &env(),
            None,
            "codex:user:config-toml",
            &toml_preview.base_revision,
            &toml_operations,
        )
        .expect("TOML apply");
        let written_toml = fs::read_to_string(&toml_source).expect("read TOML");
        assert!(written_toml.contains("# the handler comment"));
        assert!(written_toml.contains("vendorOption = \"keep-me\""));
        assert!(written_toml.contains("9000"));

        // ── JSON preview, external edit, conflict, then apply ──
        let json_operations = [HookEditOperationDto::UpdateHandler {
            locator: locator("PostToolUse", 0, 0),
            draft: draft(
                "PostToolUse",
                Some("Write"),
                "command",
                &[("command", json!("echo sentinel-secret"))],
            ),
        }];
        let stale = preview_change(
            Some(home.path()),
            Some("project-1"),
            "claude_code:project:settings-json",
            &json_operations,
            lookup,
        )
        .expect("JSON preview");
        assert!(!stale.after_canonical_text.contains("apiToken"));

        let externally_edited = CLAUDE_SETTINGS_JSON.replace("echo post", "echo external");
        fs::write(&json_source, &externally_edited).expect("external edit");
        assert_eq!(
            apply_change(
                &env(),
                Some("project-1"),
                "claude_code:project:settings-json",
                &stale.base_revision,
                &json_operations,
            ),
            Err(HookWriteError::SourceConflict),
            "an external edit must invalidate the confirmed revision"
        );
        assert_eq!(
            fs::read_to_string(&json_source).expect("read"),
            externally_edited
        );

        let fresh = preview_change(
            Some(home.path()),
            Some("project-1"),
            "claude_code:project:settings-json",
            &json_operations,
            lookup,
        )
        .expect("second JSON preview");
        let applied = apply_change(
            &env(),
            Some("project-1"),
            "claude_code:project:settings-json",
            &fresh.base_revision,
            &json_operations,
        )
        .expect("JSON apply");
        let after_apply = fs::read_to_string(&json_source).expect("read");
        assert!(after_apply.contains("echo sentinel-secret"));
        assert!(
            after_apply.contains("\"apiToken\": \"sentinel-secret\""),
            "the non-Hook sibling survives the write"
        );

        // ── Restore, and restore again to undo it ──
        let recovery = get_recovery(&env(), Some("project-1"), "claude_code:project:settings-json")
            .expect("recovery")
            .expect("recovery point");
        let restore_preview = preview_restore(
            &env(),
            Some("project-1"),
            "claude_code:project:settings-json",
            &recovery.backup_id,
        )
        .expect("restore preview");
        apply_restore(
            &env(),
            Some("project-1"),
            "claude_code:project:settings-json",
            &recovery.backup_id,
            &restore_preview.base_revision,
        )
        .expect("restore");
        assert_eq!(
            fs::read_to_string(&json_source).expect("read"),
            externally_edited,
            "restore returns the externally edited bytes"
        );

        // ── A crash-time journal blocks mutation until it is reconciled ──
        let journal = HookOperationJournal {
            operation_id: "crashed".to_string(),
            artifact_id: applied.artifact_id.clone(),
            source_id: "claude_code:project:settings-json".to_string(),
            context_key: "project:project-1".to_string(),
            backup_id: "never-committed".to_string(),
            backup_kind: "bytes".to_string(),
            backup_locator: format!("{}/latest", applied.artifact_id),
            base_revision: revision_of(Some(after_apply.as_bytes())),
            target_revision: revision_of(Some(externally_edited.as_bytes())),
        };
        write_journal(backups.path(), &journal).expect("write journal");
        assert_eq!(
            ensure_mutations_allowed(backups.path()),
            Err(HookWriteError::RecoveryRequired)
        );
        reconcile_pending_operations(&env()).expect("reconcile");
        assert_eq!(
            fs::read_to_string(&json_source).expect("read"),
            after_apply,
            "reconciliation restores the state the journal recorded"
        );
        assert!(ensure_mutations_allowed(backups.path()).is_ok());

        // ── Where the sentinel may and may not be ──
        let mut leaked: Vec<String> = Vec::new();
        for root in [home.path(), project.path(), db.path()] {
            for entry in walkdir::WalkDir::new(root)
                .min_depth(1)
                .into_iter()
                .filter_map(Result::ok)
                .filter(|entry| entry.file_type().is_file())
            {
                let bytes = fs::read(entry.path()).expect("read file");
                if String::from_utf8_lossy(&bytes).contains("sentinel-secret")
                    && entry.path() != json_source
                {
                    leaked.push(entry.path().display().to_string());
                }
            }
        }
        assert!(
            leaked.is_empty(),
            "only the source file itself may hold Hook payload, found: {leaked:?}"
        );

        // The private recovery payload is the one other authorized location.
        let payload = backups.path().join(&applied.artifact_id).join("latest");
        assert!(payload.exists());
        let payload_holders: Vec<String> = walkdir::WalkDir::new(backups.path())
            .min_depth(1)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|entry| entry.file_type().is_file())
            .filter(|entry| {
                String::from_utf8_lossy(&fs::read(entry.path()).expect("read")).contains("sentinel-secret")
            })
            .map(|entry| entry.path().display().to_string())
            .collect();
        assert_eq!(
            payload_holders,
            vec![payload.display().to_string()],
            "only the latest recovery payload may hold Hook payload"
        );
    }

    // -----------------------------------------------------------------------
    // Second-operation recovery consistency
    // -----------------------------------------------------------------------

    #[test]
    fn second_apply_fault_preserves_existing_recovery_point() {
        for (point, expected) in FAULT_POINTS {
            let workspace = workspace();
            write_fixture(&workspace.source(), CODEX_HOOKS_JSON);

            let first = apply_change(
                &workspace.env(None),
                None,
                "codex:user:hooks-json",
                &revision_of(Some(CODEX_HOOKS_JSON.as_bytes())),
                &[codex_command_update("echo first")],
            )
            .expect("first apply");
            let after_first = fs::read_to_string(workspace.source()).expect("read");
            let payload_path = workspace
                .backups
                .path()
                .join(&first.artifact_id)
                .join("latest");
            let first_payload = fs::read_to_string(&payload_path).expect("payload");
            let first_recovery =
                get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
                    .expect("query")
                    .expect("recovery");

            let error = apply_change(
                &workspace.env(Some(*point)),
                None,
                "codex:user:hooks-json",
                &revision_of(Some(after_first.as_bytes())),
                &[codex_command_update("echo second")],
            )
            .expect_err("fault must fail");
            assert_eq!(&error, expected, "error code for {point:?}");

            assert_eq!(
                fs::read_to_string(workspace.source()).expect("read"),
                after_first,
                "{point:?}: target must return to second-apply input"
            );

            let recovery =
                get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
                    .expect("query")
                    .expect("recovery");
            assert_eq!(
                recovery.backup_id, first_recovery.backup_id,
                "{point:?}: metadata must not change"
            );

            assert_eq!(
                fs::read_to_string(&payload_path).expect("payload"),
                first_payload,
                "{point:?}: payload bytes must not change"
            );

            assert!(recovery.can_restore, "{point:?}: recovery must be restorable");
            let preview = preview_restore(
                &workspace.env(None),
                None,
                "codex:user:hooks-json",
                &recovery.backup_id,
            )
            .expect("preview");
            assert!(preview.can_apply, "{point:?}: restore preview must be applicable");
            apply_restore(
                &workspace.env(None),
                None,
                "codex:user:hooks-json",
                &recovery.backup_id,
                &preview.base_revision,
            )
            .expect("restore must succeed");
            assert_eq!(
                fs::read_to_string(workspace.source()).expect("read"),
                CODEX_HOOKS_JSON,
                "{point:?}: restore must return to original content"
            );
        }
    }

    #[test]
    fn restore_sqlite_failure_preserves_existing_recovery_point() {
        let workspace = workspace();
        write_fixture(&workspace.source(), CODEX_HOOKS_JSON);

        let applied = apply_change(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &revision_of(Some(CODEX_HOOKS_JSON.as_bytes())),
            &[codex_command_update("echo edited")],
        )
        .expect("apply");
        let after_apply = fs::read_to_string(workspace.source()).expect("read");
        let payload_path = workspace
            .backups
            .path()
            .join(&applied.artifact_id)
            .join("latest");
        let payload_before = fs::read_to_string(&payload_path).expect("payload");
        let recovery_before =
            get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
                .expect("query")
                .expect("recovery");

        let error = apply_restore(
            &workspace.env(Some(HookFaultPoint::SqliteCommit)),
            None,
            "codex:user:hooks-json",
            &recovery_before.backup_id,
            &recovery_before.base_revision,
        )
        .expect_err("sqlite fault must fail");
        assert_eq!(error, HookWriteError::WriteFailed);

        assert_eq!(
            fs::read_to_string(workspace.source()).expect("read"),
            after_apply,
            "target must not change"
        );

        let recovery =
            get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
                .expect("query")
                .expect("recovery");
        assert_eq!(
            recovery.backup_id, recovery_before.backup_id,
            "metadata must not change"
        );

        assert_eq!(
            fs::read_to_string(&payload_path).expect("payload"),
            payload_before,
            "payload must not change"
        );

        assert!(recovery.can_restore, "recovery must remain restorable");
    }

    #[test]
    fn consecutive_apply_restore_no_stale_payloads() {
        let workspace = workspace();
        write_fixture(&workspace.source(), CODEX_HOOKS_JSON);

        let _first = apply_change(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &revision_of(Some(CODEX_HOOKS_JSON.as_bytes())),
            &[codex_command_update("echo first")],
        )
        .expect("first");
        let after_first = fs::read_to_string(workspace.source()).expect("read");

        let second = apply_change(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &revision_of(Some(after_first.as_bytes())),
            &[codex_command_update("echo second")],
        )
        .expect("second");

        let recovery =
            get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
                .expect("query")
                .expect("recovery");
        assert_eq!(recovery.backup_id, second.backup_id);

        let backup_dir = workspace.backups.path().join(&second.artifact_id);
        assert!(
            !backup_dir.join("latest.pending").exists(),
            "no stale pending file"
        );
        assert!(
            !backup_dir.join("latest.tmp").exists(),
            "no stale staging file"
        );

        let payload = fs::read_to_string(backup_dir.join("latest")).expect("payload");
        assert_eq!(payload, after_first, "payload is the state before the last apply");

        let preview = preview_restore(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &recovery.backup_id,
        )
        .expect("preview");
        assert!(preview.can_apply);
        apply_restore(
            &workspace.env(None),
            None,
            "codex:user:hooks-json",
            &recovery.backup_id,
            &preview.base_revision,
        )
        .expect("restore");

        let reverse =
            get_recovery(&workspace.env(None), None, "codex:user:hooks-json")
                .expect("query")
                .expect("recovery");
        assert_ne!(reverse.backup_id, recovery.backup_id);
        assert!(
            !backup_dir.join("latest.pending").exists(),
            "no stale pending after restore"
        );
    }
}
