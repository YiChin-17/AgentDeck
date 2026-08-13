# hook-management Specification

## Purpose

TBD - created by archiving change 'edit-codex-claude-hooks'. Update Purpose after archive.

## Requirements

### Requirement: Hook mutation resolves only fixed writable sources

AgentDeck SHALL resolve every Hook preview, apply, and restore target from an enum-backed source id plus an optional linked Project id. It MUST NOT accept a filesystem path from the frontend. A writable target MUST be either a regular file or a missing fixed source beneath an existing home or linked Project root; symlinks, special files, and unavailable roots MUST be rejected before mutation.

#### Scenario: Known source id resolves inside a linked Project

- **GIVEN** linked Project `project-1` has existing root `/workspace/demo`
- **AND** source id `claude_code:project:settings-json` is selected
- **WHEN** a Hook change is previewed for `project-1`
- **THEN** the backend resolves `/workspace/demo/.claude/settings.json` from its fixed descriptor
- **AND** no path supplied by the frontend is read or written

#### Scenario: Unknown source and Project fail closed

- **WHEN** a mutation request contains unknown source id `claude_code:project:other-json` or unknown Project id `missing-project`
- **THEN** it returns `invalid_source` or `invalid_project` respectively
- **AND** no directory, backup, Artifact, or target file is created

#### Scenario: Offline root and symlink are not replaced

- **GIVEN** a linked Project root is absent, or its fixed Hook source is a symlink
- **WHEN** preview or apply is requested
- **THEN** it returns `source_offline` or `unsupported_source_type` respectively
- **AND** it does not create the missing root or replace the symlink


<!-- @trace
source: edit-codex-claude-hooks
updated: 2026-08-13
code:
  - src/i18n/en.json
  - src/lib/tauri.ts
  - src-tauri/src/core/hook_inspection.rs
  - src-tauri/src/core/skill_store.rs
  - src-tauri/src/core/mod.rs
  - src-tauri/src/lib.rs
  - scripts/check-hooks-ui.mjs
  - plan.md
  - src-tauri/src/commands/hooks.rs
  - src-tauri/src/core/hook_management.rs
  - src-tauri/src/core/migrations.rs
  - src/components/HookEditor.tsx
  - src/i18n/zh-TW.json
  - src-tauri/src/core/artifact.rs
  - src/views/Hooks.tsx
-->

---
### Requirement: Agent-specific operations validate before transformation

AgentDeck SHALL model Hook edits as `create_handler`, `update_handler`, or `delete_handler` operations located by source event, matcher group index, and handler index. Create and update values MUST satisfy the selected Agent's documented event, handler type, field name, and field value constraints. Existing unknown values MUST remain unchanged unless their containing handler is deleted, and the API MUST reject requests that add or modify unknown values.

#### Scenario: Valid Codex operation produces a transformed document

- **GIVEN** Codex `hooks.json` contains a `PreToolUse` command handler at group 0 and handler 0
- **WHEN** an update operation changes its matcher to `Shell` and its `timeout` to `30`
- **THEN** validation succeeds
- **AND** the transformed Hook subtree retains the same event and handler position with the new matcher and timeout

#### Scenario: Claude-only handler is rejected for Codex

- **WHEN** a create operation requests handler type `http` for a Codex source
- **THEN** validation returns `invalid_hook_draft` with a field-specific issue for `handlerType`
- **AND** preview has `canApply=false`
- **AND** no source, backup, or database row changes

#### Scenario: Existing unknown field survives an unrelated update

- **GIVEN** a valid handler contains unknown field `vendorOption` with value `keep-me`
- **WHEN** an update operation changes only the known `timeout` field
- **THEN** `vendorOption` remains present with value `keep-me` in the transformed source
- **AND** the request cannot replace or delete `vendorOption` directly

#### Scenario: Stale locator is rejected

- **GIVEN** an update operation identifies event `PreToolUse`, group 1, handler 2
- **WHEN** the current source has no unique handler at that locator
- **THEN** preview returns `stale_draft`
- **AND** no fallback handler is selected


<!-- @trace
source: edit-codex-claude-hooks
updated: 2026-08-13
code:
  - src/i18n/en.json
  - src/lib/tauri.ts
  - src-tauri/src/core/hook_inspection.rs
  - src-tauri/src/core/skill_store.rs
  - src-tauri/src/core/mod.rs
  - src-tauri/src/lib.rs
  - scripts/check-hooks-ui.mjs
  - plan.md
  - src-tauri/src/commands/hooks.rs
  - src-tauri/src/core/hook_management.rs
  - src-tauri/src/core/migrations.rs
  - src/components/HookEditor.tsx
  - src/i18n/zh-TW.json
  - src-tauri/src/core/artifact.rs
  - src/views/Hooks.tsx
-->

---
### Requirement: Round trips preserve configuration outside edited fields

AgentDeck SHALL transform the complete source document while changing only fields named by validated Hook operations. JSON writes MUST preserve all non-Hook sibling keys and unmodified unknown Hook values. TOML writes MUST additionally preserve unmodified comments, key order, tables, and formatting outside the edited nodes.

#### Scenario: JSON sibling secret survives without entering the preview

- **GIVEN** Claude Code settings contain `apiToken: sentinel-secret` beside `hooks`
- **WHEN** a handler command is updated and the resulting file is written
- **THEN** the file still contains the exact `apiToken` value
- **AND** the preview DTO, validation issues, errors, database rows, and logs do not contain `sentinel-secret`

#### Scenario: TOML comments and unknown tables survive a Hook update

- **GIVEN** Codex `config.toml` contains ordered non-Hook tables, comments inside and outside `[hooks]`, and an unknown Hook field
- **WHEN** one documented handler field is updated
- **THEN** all unmodified tables, comments, key order, and the unknown Hook field remain present in the written TOML
- **AND** only the selected field differs in a structural round-trip comparison


<!-- @trace
source: edit-codex-claude-hooks
updated: 2026-08-13
code:
  - src/i18n/en.json
  - src/lib/tauri.ts
  - src-tauri/src/core/hook_inspection.rs
  - src-tauri/src/core/skill_store.rs
  - src-tauri/src/core/mod.rs
  - src-tauri/src/lib.rs
  - scripts/check-hooks-ui.mjs
  - plan.md
  - src-tauri/src/commands/hooks.rs
  - src-tauri/src/core/hook_management.rs
  - src-tauri/src/core/migrations.rs
  - src/components/HookEditor.tsx
  - src/i18n/zh-TW.json
  - src-tauri/src/core/artifact.rs
  - src/views/Hooks.tsx
-->

---
### Requirement: Preview binds an exact source revision to validated operations

AgentDeck SHALL return a `HookWritePreviewDto` containing the source id, full-source SHA-256 `baseRevision`, before and after canonical Hook text, validation issues, `canApply`, and `wouldCreateFile`. A missing source MUST use the fixed revision value `missing`. Preview MUST NOT mutate filesystem or database state, and it MUST set `canApply=false` when canonical diff input exceeds 262,144 bytes or 4,000 lines.

#### Scenario: Preview returns exact before and after Hook fragments

- **GIVEN** a valid source has full-source SHA-256 `abc123` and one `PreToolUse` handler
- **WHEN** a valid operation adds one `PostToolUse` handler
- **THEN** preview returns `baseRevision=abc123`
- **AND** before canonical text contains only the original Hook subtree
- **AND** after canonical text contains both Hook events
- **AND** non-Hook sibling content is absent
- **AND** `canApply=true`

#### Scenario: Missing fixed source previews file creation

- **GIVEN** the fixed source is missing but its home or linked Project root exists
- **WHEN** a valid create operation is previewed
- **THEN** preview returns `baseRevision=missing` and `wouldCreateFile=true`
- **AND** no source parent, file, backup, or Artifact is created

#### Scenario: Preview refuses oversized diff input

- **GIVEN** the transformed canonical Hook text is 262,145 bytes or 4,001 lines
- **WHEN** preview is requested
- **THEN** it returns `preview_too_large` and `canApply=false`
- **AND** it does not send the oversized text to the line-diff component

##### Example: Preview size boundaries

| Canonical bytes | Canonical lines | Expected |
| ----- | ----- | ----- |
| 262144 | 4000 | `canApply=true` |
| 262145 | 1 | `preview_too_large` |
| 1 | 4001 | `preview_too_large` |


<!-- @trace
source: edit-codex-claude-hooks
updated: 2026-08-13
code:
  - src/i18n/en.json
  - src/lib/tauri.ts
  - src-tauri/src/core/hook_inspection.rs
  - src-tauri/src/core/skill_store.rs
  - src-tauri/src/core/mod.rs
  - src-tauri/src/lib.rs
  - scripts/check-hooks-ui.mjs
  - plan.md
  - src-tauri/src/commands/hooks.rs
  - src-tauri/src/core/hook_management.rs
  - src-tauri/src/core/migrations.rs
  - src/components/HookEditor.tsx
  - src/i18n/zh-TW.json
  - src-tauri/src/core/artifact.rs
  - src/views/Hooks.tsx
-->

---
### Requirement: Apply is conflict-safe, recoverable, and atomic

AgentDeck SHALL serialize Hook mutations under a Hook write lock. Apply MUST re-read the complete source, require an exact `baseRevision` match, re-run validation and transformation, create an owner-private recovery backup, sync a staged target in the target directory, and atomically replace the target. An unsupported atomic replacement platform or any failed step MUST leave the original target bytes or absence unchanged and MUST NOT leave committed Hook metadata for an unapplied change.

#### Scenario: External modification blocks apply

- **GIVEN** preview returned base revision `abc123`
- **AND** an external process changed the source to revision `def456`
- **WHEN** apply submits revision `abc123` and the previewed operations
- **THEN** it returns `source_conflict`
- **AND** the external bytes remain unchanged
- **AND** no recovery backup or database row is created

#### Scenario: Successful apply creates backup before replacement

- **GIVEN** preview is valid and the source revision is unchanged
- **WHEN** apply succeeds
- **THEN** an owner-private latest recovery point contains the exact pre-apply bytes or an absence marker
- **AND** the target is atomically replaced by the validated document
- **AND** a new inspection returns the edited Hook values

#### Scenario: Injected write failure restores the original state

- **GIVEN** a test injects failure at backup promotion, staged target sync, atomic replacement, or SQLite commit
- **WHEN** apply runs
- **THEN** it returns a sanitized `backup_failed` or `write_failed` error
- **AND** the target has its exact original bytes or remains absent
- **AND** Artifact, Hook detail, and active backup metadata remain consistent with the target

#### Scenario: Unsupported atomic replacement fails before mutation

- **GIVEN** the runtime cannot provide atomic replacement for the target
- **WHEN** apply is requested
- **THEN** it returns `atomic_replace_unsupported`
- **AND** target, backup, operation journal, and database state remain unchanged


<!-- @trace
source: edit-codex-claude-hooks
updated: 2026-08-13
code:
  - src/i18n/en.json
  - src/lib/tauri.ts
  - src-tauri/src/core/hook_inspection.rs
  - src-tauri/src/core/skill_store.rs
  - src-tauri/src/core/mod.rs
  - src-tauri/src/lib.rs
  - scripts/check-hooks-ui.mjs
  - plan.md
  - src-tauri/src/commands/hooks.rs
  - src-tauri/src/core/hook_management.rs
  - src-tauri/src/core/migrations.rs
  - src/components/HookEditor.tsx
  - src/i18n/zh-TW.json
  - src-tauri/src/core/artifact.rs
  - src/views/Hooks.tsx
-->

---
### Requirement: Hook identity and backup metadata exclude Hook payload

AgentDeck SHALL create one kind `hook` Artifact per successfully managed source id and context key, where context is `global` or `project:<project-id>`. Schema v9 SHALL persist Hook detail and recovery metadata containing ids, Agent, scope, format, hashes, state-relative backup locator, timestamps, and backup kind only. SQLite, the central Library, localStorage, logs, and `.skills-manager` Git backup MUST NOT contain Hook command, prompt, URL, headers, environment, or complete Hook source payload.

#### Scenario: First successful apply creates one identity

- **GIVEN** no managed Hook Artifact exists for `codex:user:hooks-json` in context `global`
- **WHEN** the first apply succeeds
- **THEN** exactly one kind `hook` Artifact and one Hook detail row exist for that source and context
- **AND** a later successful apply reuses the same Artifact id

#### Scenario: Preview and failed apply create no identity

- **WHEN** preview succeeds but apply is not called, or apply fails before replacement
- **THEN** no new Artifact, Hook detail, or backup metadata row is committed

##### Example: Preview and conflict leave schema v9 empty

- **GIVEN** `artifacts`, `hook_details`, and `hook_backups` contain zero rows for `codex:user:hooks-json` in context `global`
- **WHEN** preview succeeds and a later apply returns `source_conflict`
- **THEN** all three row counts for that source and context remain zero

#### Scenario: Sensitive payload exists only in authorized transient and recovery locations

- **GIVEN** a Hook command contains `sentinel-secret`
- **WHEN** apply and inspection complete
- **THEN** `sentinel-secret` exists only in the source file, authorized in-memory IPC content, and owner-private recovery payload
- **AND** SQLite dumps, Library tree content, Git backup changes, logs, localStorage contract, and serialized errors exclude it

#### Scenario: Schema v8 upgrades atomically to v9

- **GIVEN** a populated schema v8 database
- **WHEN** migrations run
- **THEN** user version becomes 9
- **AND** existing rows and relationships remain unchanged
- **AND** Hook detail and backup tables contain zero seed rows
- **AND** migration failure rolls back to user version 8 with neither new table visible


<!-- @trace
source: edit-codex-claude-hooks
updated: 2026-08-13
code:
  - src/i18n/en.json
  - src/lib/tauri.ts
  - src-tauri/src/core/hook_inspection.rs
  - src-tauri/src/core/skill_store.rs
  - src-tauri/src/core/mod.rs
  - src-tauri/src/lib.rs
  - scripts/check-hooks-ui.mjs
  - plan.md
  - src-tauri/src/commands/hooks.rs
  - src-tauri/src/core/hook_management.rs
  - src-tauri/src/core/migrations.rs
  - src/components/HookEditor.tsx
  - src/i18n/zh-TW.json
  - src-tauri/src/core/artifact.rs
  - src/views/Hooks.tsx
-->

---
### Requirement: Restore requires preview and preserves a reverse recovery point

AgentDeck SHALL expose only the latest valid recovery point for a managed Hook source. Restore MUST provide a canonical Hook diff preview and MUST require an exact current-source base revision before apply. A successful restore MUST first capture the current bytes or absence as the new recovery point, then atomically restore the previous bytes or remove a file whose backup kind is `absent`.

#### Scenario: Restore reverts the latest apply

- **GIVEN** apply changed source revision `before` to `after` and stored the `before` bytes
- **WHEN** the user previews restore and applies it while the source still has revision `after`
- **THEN** the source returns to the exact `before` bytes
- **AND** the new latest recovery point contains the exact `after` bytes
- **AND** inspection reflects the restored Hook subtree

#### Scenario: Restore of an absence marker removes only an unchanged created file

- **GIVEN** apply created a previously missing fixed source and stored an absence marker
- **WHEN** restore is previewed and applied without an intervening source change
- **THEN** the created regular file is removed atomically
- **AND** its parent directories and every other source remain unchanged

#### Scenario: Modified source or corrupt backup blocks restore

- **GIVEN** the current source changed after restore preview, or the recovery payload hash does not match metadata
- **WHEN** restore apply runs
- **THEN** it returns `source_conflict` or `restore_failed`
- **AND** the current source remains unchanged
- **AND** no recovery point is replaced


<!-- @trace
source: edit-codex-claude-hooks
updated: 2026-08-13
code:
  - src/i18n/en.json
  - src/lib/tauri.ts
  - src-tauri/src/core/hook_inspection.rs
  - src-tauri/src/core/skill_store.rs
  - src-tauri/src/core/mod.rs
  - src-tauri/src/lib.rs
  - scripts/check-hooks-ui.mjs
  - plan.md
  - src-tauri/src/commands/hooks.rs
  - src-tauri/src/core/hook_management.rs
  - src-tauri/src/core/migrations.rs
  - src/components/HookEditor.tsx
  - src/i18n/zh-TW.json
  - src-tauri/src/core/artifact.rs
  - src/views/Hooks.tsx
-->

---
### Requirement: Interrupted Hook writes recover before new mutations

AgentDeck SHALL record a payload-free operation journal before target replacement and SHALL reconcile every unfinished journal before accepting a new Hook mutation. Recovery MUST either restore the pre-operation target state or confirm the committed metadata state. If reconciliation fails, inspection SHALL remain available while all Hook mutation commands return `recovery_required`.

#### Scenario: Startup restores an interrupted uncommitted write

- **GIVEN** a crash occurred after target replacement but before Hook metadata commit
- **WHEN** AgentDeck starts and reads the operation journal
- **THEN** it restores the target from the recorded recovery point
- **AND** it removes uncommitted Hook metadata
- **AND** it clears the journal only after target and database agree

#### Scenario: Failed reconciliation blocks mutation only

- **GIVEN** an unfinished journal references an unreadable recovery payload
- **WHEN** startup recovery runs
- **THEN** Hook inspection remains available
- **AND** preview, apply, and restore commands return `recovery_required`
- **AND** the current source is not overwritten

<!-- @trace
source: edit-codex-claude-hooks
updated: 2026-08-13
code:
  - src/i18n/en.json
  - src/lib/tauri.ts
  - src-tauri/src/core/hook_inspection.rs
  - src-tauri/src/core/skill_store.rs
  - src-tauri/src/core/mod.rs
  - src-tauri/src/lib.rs
  - scripts/check-hooks-ui.mjs
  - plan.md
  - src-tauri/src/commands/hooks.rs
  - src-tauri/src/core/hook_management.rs
  - src-tauri/src/core/migrations.rs
  - src/components/HookEditor.tsx
  - src/i18n/zh-TW.json
  - src-tauri/src/core/artifact.rs
  - src/views/Hooks.tsx
-->