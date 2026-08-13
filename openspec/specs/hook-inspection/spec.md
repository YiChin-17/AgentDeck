# hook-inspection Specification

## Purpose

TBD - created by archiving change 'inspect-codex-claude-hooks'. Update Purpose after archive.

## Requirements

### Requirement: Hook discovery reads only fixed user and linked-project sources

AgentDeck SHALL discover Hook configuration from fixed Codex and Claude Code source descriptors without accepting an arbitrary filesystem path from the frontend. Without a selected Project it SHALL read only user sources. With a selected linked Project it SHALL additionally read that Project's sources and MUST reject an unknown Project id instead of falling back to the process working directory.

#### Scenario: User sources are enumerated without a Project

- **WHEN** `get_hook_inspection` is called with a null Project id
- **THEN** the result includes Codex `.codex/hooks.json`, Codex `.codex/config.toml`, and Claude Code `.claude/settings.json` user source descriptors
- **AND** it includes no project or project-local source descriptor

#### Scenario: Linked Project adds fixed project sources

- **GIVEN** linked Project `project-1` has root `/workspace/demo`
- **WHEN** `get_hook_inspection` is called with Project id `project-1`
- **THEN** the result additionally includes `/workspace/demo/.codex/hooks.json`, `/workspace/demo/.codex/config.toml`, `/workspace/demo/.claude/settings.json`, and `/workspace/demo/.claude/settings.local.json`
- **AND** no path outside the user and `/workspace/demo` descriptors is read

#### Scenario: Unknown Project fails closed

- **WHEN** `get_hook_inspection` is called with Project id `missing-project`
- **THEN** the command returns an `invalid_project` error
- **AND** it does not inspect Hook files relative to the current process directory


<!-- @trace
source: inspect-codex-claude-hooks
updated: 2026-08-13
code:
  - src/lib/tauri.ts
  - src/i18n/en.json
  - src-tauri/src/core/mod.rs
  - src/i18n/zh-TW.json
  - src/components/HookInspector.tsx
  - src/views/Hooks.tsx
  - scripts/check-hooks-ui.mjs
  - src-tauri/src/commands/mod.rs
  - package.json
  - .agents/skills/spectra-verify/SKILL.md
  - .agents/skills/spectra-analyze/SKILL.md
  - src-tauri/Cargo.toml
  - src-tauri/src/lib.rs
  - plan.md
  - src/App.tsx
  - src-tauri/src/core/hook_inspection.rs
  - src/components/Sidebar.tsx
  - src-tauri/src/commands/hooks.rs
-->

---
### Requirement: Each Hook source is parsed and diagnosed independently

AgentDeck SHALL parse the Hook subtree from Codex JSON, Codex inline TOML, and Claude Code JSON while preserving source, Agent, scope, event, matcher group order, handler order, handler type, and unknown Hook fields. Each source MUST produce exactly one of `missing`, `valid`, `invalid`, or `too_large`. A failure in one source MUST NOT suppress entries or diagnostics from another source.

#### Scenario: Codex JSON and inline TOML are both retained

- **GIVEN** a Codex `hooks.json` contains one `PreToolUse` command handler and the same layer's `config.toml` contains one `SessionStart` command handler
- **WHEN** Hook inspection runs
- **THEN** both sources have status `valid`
- **AND** each entry retains its own source id, event, matcher, handler index, handler type, and display fields
- **AND** neither source replaces the other

#### Scenario: Claude Code source layers remain distinct

- **GIVEN** Claude Code user, project, and project-local settings each contain one matching `PostToolUse` handler
- **WHEN** Hook inspection runs for that Project
- **THEN** three entries are returned with scopes `user`, `project`, and `project_local`
- **AND** their source order and handler order are deterministic

#### Scenario: Invalid source is isolated

- **GIVEN** Codex `hooks.json` contains invalid JSON and Claude Code user settings contain a valid `Notification` handler
- **WHEN** Hook inspection runs
- **THEN** the Codex source has status `invalid` with a sanitized JSON diagnostic
- **AND** the Claude Code source and its entry remain available
- **AND** no parser panic or whole-page error occurs

#### Scenario: Unknown Hook values remain visible

- **GIVEN** a valid source contains event `FutureEvent`, handler type `future_handler`, and field `vendorOption`
- **WHEN** Hook inspection runs
- **THEN** the entry retains all three original names and the display value of `vendorOption`
- **AND** the event, handler type, and field are marked `unknown`
- **AND** none is normalized to a known Codex or Claude Code capability

#### Scenario: Oversized source fails before parsing

- **GIVEN** a Hook config file contains 1,048,577 bytes
- **WHEN** Hook inspection runs
- **THEN** that source has status `too_large`
- **AND** no entry or canonical Hook text is produced for that source
- **AND** other sources remain available


<!-- @trace
source: inspect-codex-claude-hooks
updated: 2026-08-13
code:
  - src/lib/tauri.ts
  - src/i18n/en.json
  - src-tauri/src/core/mod.rs
  - src/i18n/zh-TW.json
  - src/components/HookInspector.tsx
  - src/views/Hooks.tsx
  - scripts/check-hooks-ui.mjs
  - src-tauri/src/commands/mod.rs
  - package.json
  - .agents/skills/spectra-verify/SKILL.md
  - .agents/skills/spectra-analyze/SKILL.md
  - src-tauri/Cargo.toml
  - src-tauri/src/lib.rs
  - plan.md
  - src/App.tsx
  - src-tauri/src/core/hook_inspection.rs
  - src/components/Sidebar.tsx
  - src-tauri/src/commands/hooks.rs
-->

---
### Requirement: Inspection responses exclude non-Hook configuration and persistence

AgentDeck SHALL return only Hook subtree data required for local inspection. It MUST NOT return non-Hook sibling keys from settings or config files and MUST NOT persist Hook content in SQLite, the Library, Git backup metadata, logs, or localStorage. Inspection MUST NOT execute a Hook or modify any source file.

#### Scenario: Non-Hook secret sibling is excluded

- **GIVEN** Claude Code settings contain a `hooks` object and a non-Hook sibling value `apiToken: sentinel-secret`
- **WHEN** the inspection DTO and diagnostic strings are serialized
- **THEN** the Hook entries and canonical Hook text are present
- **AND** `sentinel-secret` is absent from the complete serialized response and diagnostics

#### Scenario: Read-only inspection has no side effects

- **GIVEN** fixed source file bytes, SQLite rows, Library tree hashes, and Git status before inspection
- **WHEN** the user loads, filters, inspects, and compares Hook sources
- **THEN** all source file bytes, SQLite rows, Library tree hashes, and Git status remain unchanged
- **AND** no configured Hook command, prompt, HTTP endpoint, MCP tool, or agent handler is invoked


<!-- @trace
source: inspect-codex-claude-hooks
updated: 2026-08-13
code:
  - src/lib/tauri.ts
  - src/i18n/en.json
  - src-tauri/src/core/mod.rs
  - src/i18n/zh-TW.json
  - src/components/HookInspector.tsx
  - src/views/Hooks.tsx
  - scripts/check-hooks-ui.mjs
  - src-tauri/src/commands/mod.rs
  - package.json
  - .agents/skills/spectra-verify/SKILL.md
  - .agents/skills/spectra-analyze/SKILL.md
  - src-tauri/Cargo.toml
  - src-tauri/src/lib.rs
  - plan.md
  - src/App.tsx
  - src-tauri/src/core/hook_inspection.rs
  - src/components/Sidebar.tsx
  - src-tauri/src/commands/hooks.rs
-->

---
### Requirement: Compatibility matrix is explicit and snapshot-based

AgentDeck SHALL generate a compatibility matrix from a typed registry pinned to the 2026-08-12 official Codex and Claude Code Hook documentation snapshot. Every matrix cell MUST be `supported`, `unsupported`, or `unknown`. Equal event names MUST NOT imply equal runtime semantics, and discovery of an unregistered value MUST NOT promote that value to `supported`.

#### Scenario: Handler support remains Agent-specific

- **WHEN** the compatibility matrix is generated
- **THEN** Codex marks `command` as `supported`
- **AND** Claude Code marks `command`, `http`, `mcp_tool`, `prompt`, and `agent` as `supported`
- **AND** Codex does not mark `http`, `mcp_tool`, `prompt`, or `agent` as `supported`

#### Scenario: Shared event name keeps separate notes

- **WHEN** the matrix includes `PreToolUse`
- **THEN** both Agent cells report their registry support state
- **AND** the row retains Agent-specific notes instead of claiming a shared input or output contract

#### Scenario: Future value remains unknown

- **GIVEN** discovery returns event `FutureEvent` that is absent from the registry
- **WHEN** the inspection response is assembled
- **THEN** the entry is marked `unknown`
- **AND** no compatibility cell is changed to `supported`


<!-- @trace
source: inspect-codex-claude-hooks
updated: 2026-08-13
code:
  - src/lib/tauri.ts
  - src/i18n/en.json
  - src-tauri/src/core/mod.rs
  - src/i18n/zh-TW.json
  - src/components/HookInspector.tsx
  - src/views/Hooks.tsx
  - scripts/check-hooks-ui.mjs
  - src-tauri/src/commands/mod.rs
  - package.json
  - .agents/skills/spectra-verify/SKILL.md
  - .agents/skills/spectra-analyze/SKILL.md
  - src-tauri/Cargo.toml
  - src-tauri/src/lib.rs
  - plan.md
  - src/App.tsx
  - src-tauri/src/core/hook_inspection.rs
  - src/components/Sidebar.tsx
  - src-tauri/src/commands/hooks.rs
-->

---
### Requirement: Source comparison is bounded and same-Agent only

AgentDeck SHALL provide deterministic canonical Hook subtree text for valid sources and SHALL allow comparison only between two different `diff_available` sources belonging to the same Agent. A source MUST have `diff_available=false` when its canonical Hook fragment exceeds 262,144 bytes or 4,000 lines. Cross-Agent, identical-source, invalid, missing, and too-large pairs MUST NOT enter the line-diff algorithm.

#### Scenario: Same-Agent sources produce a canonical diff

- **GIVEN** Codex user `hooks.json` canonical text contains a `PreToolUse` command and Codex project `hooks.json` canonical text contains the same command plus a `PostToolUse` command
- **WHEN** the user compares those two source ids
- **THEN** the existing side-by-side document diff shows the added `PostToolUse` subtree
- **AND** formatting outside each extracted Hook subtree is absent from the comparison

#### Scenario: Cross-Agent comparison is refused

- **GIVEN** one Codex source and one Claude Code source are selected
- **WHEN** the user requests comparison
- **THEN** Compare remains disabled with a reason that cross-Agent source grammars are not text-compared
- **AND** the line-diff algorithm is not called

#### Scenario: Canonical fragment reaches the diff limit

- **GIVEN** one valid source produces canonical Hook text of 262,145 bytes or 4,001 lines
- **WHEN** inspection returns the source
- **THEN** its parsed Hook entries remain available
- **AND** `diff_available` is `false`
- **AND** the UI states that the source exceeds the comparison limit


<!-- @trace
source: inspect-codex-claude-hooks
updated: 2026-08-13
code:
  - src/lib/tauri.ts
  - src/i18n/en.json
  - src-tauri/src/core/mod.rs
  - src/i18n/zh-TW.json
  - src/components/HookInspector.tsx
  - src/views/Hooks.tsx
  - scripts/check-hooks-ui.mjs
  - src-tauri/src/commands/mod.rs
  - package.json
  - .agents/skills/spectra-verify/SKILL.md
  - .agents/skills/spectra-analyze/SKILL.md
  - src-tauri/Cargo.toml
  - src-tauri/src/lib.rs
  - plan.md
  - src/App.tsx
  - src-tauri/src/core/hook_inspection.rs
  - src/components/Sidebar.tsx
  - src-tauri/src/commands/hooks.rs
-->

---
### Requirement: Hooks page exposes gated Hook editing without execution controls

AgentDeck SHALL provide a `/hooks` route and Sidebar entry. The page SHALL retain Agent, scope, event, source status, and Project filters; source diagnostics; Hook Inspector; bounded source comparison; and compatibility matrix. For writable fixed sources it SHALL additionally expose Edit, Delete, Preview, Apply, and Restore controls governed by backend validation and exact source revisions. It MUST NOT render or invoke any Hook execution, test-run, enable, or disable action.

#### Scenario: User filters and inspects a Hook

- **GIVEN** inspection returns Codex and Claude Code entries from user and project scopes
- **WHEN** the user selects Agent `codex`, scope `project`, and an event, then opens one result
- **THEN** only matching entries remain in the list
- **AND** the Inspector displays that entry's source, event, matcher, handler type, handler fields, and known or unknown markers

#### Scenario: Missing and invalid sources remain understandable

- **GIVEN** one writable fixed source is missing and one source is invalid
- **WHEN** the Hooks page loads
- **THEN** the missing source is shown as an empty state with an option to create its first handler
- **AND** the invalid source shows its sanitized source-specific diagnostic without mutation controls
- **AND** available entries and the compatibility matrix remain interactive

#### Scenario: Edit controls are limited by source capability

- **GIVEN** inspection includes a valid regular source, a symlink source, an offline source, and a too-large source
- **WHEN** the Hooks page renders
- **THEN** Edit and Delete are available only for handlers in the valid regular source
- **AND** the other sources show a localized reason that mutation is unavailable
- **AND** English and Traditional Chinese locale files contain matching keys for every new user-visible string

#### Scenario: Apply requires a current successful preview

- **GIVEN** a user changed a Hook draft
- **WHEN** backend validation has not produced a current preview with `canApply=true`
- **THEN** Apply is disabled
- **AND** clicking Preview never writes a source, backup, Artifact, or localStorage entry

#### Scenario: Draft changes invalidate an earlier preview

- **GIVEN** preview succeeded for draft revision `draft-1`
- **WHEN** the user changes any event, matcher, handler type, or editable field
- **THEN** the earlier diff and base revision are marked stale
- **AND** Apply remains disabled until preview succeeds for the new draft

#### Scenario: Project switch clears sensitive route-local state

- **GIVEN** the editor holds a draft and preview for Project `project-1`
- **WHEN** the user selects Project `project-2` while a request is pending
- **THEN** the draft, preview, selected handler, and recovery selection for `project-1` are cleared
- **AND** a late response for `project-1` does not replace `project-2` state
- **AND** no Hook content is stored in `AppContext` or localStorage

#### Scenario: Restore is previewed and never executes a Hook

- **GIVEN** a latest recovery point exists for the selected source
- **WHEN** the user requests Restore
- **THEN** the page displays the backend restore diff before enabling Restore Apply
- **AND** restore requires the current base revision
- **AND** no Hook command, prompt, HTTP endpoint, MCP tool, or agent handler is invoked

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
