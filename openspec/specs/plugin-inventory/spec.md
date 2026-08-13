# plugin-inventory Specification

## Purpose

TBD - created by archiving change 'inspect-codex-claude-plugins'. Update Purpose after archive.

## Requirements

### Requirement: Plugin inventory invokes only fixed read-only CLI capabilities

AgentDeck SHALL collect Plugin data only through fixed Codex and Claude Code executable names and enum-backed read-only argument lists. The frontend MUST NOT supply an executable, filesystem path, working directory, environment override, or CLI argument. The backend MUST invoke each process without a shell and with stdin closed.

#### Scenario: Codex inventory uses the fixed JSON command

- **WHEN** AgentDeck collects the Codex Plugin inventory
- **THEN** it invokes `codex plugin list --available --json`
- **AND** it invokes `codex plugin marketplace list --json`
- **AND** no caller-controlled argument is appended

#### Scenario: Claude Code inventory uses the fixed JSON command

- **WHEN** AgentDeck collects the Claude Code Plugin inventory
- **THEN** it invokes `claude plugin list --available --json`
- **AND** it invokes `claude plugin marketplace list --json`
- **AND** it does not invoke install, update, uninstall, remove, enable, disable, validate, details, eval, or marketplace mutation

#### Scenario: Missing CLI is isolated

- **GIVEN** the Codex executable cannot be resolved and the Claude Code executable is available
- **WHEN** Plugin inventory is requested
- **THEN** the Codex result contains diagnostic `cli_missing`
- **AND** the Claude Code commands still run and their usable inventory remains in the response


<!-- @trace
source: inspect-codex-claude-plugins
updated: 2026-08-13
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/zh-TW.json
  - src/components/Sidebar.tsx
  - package.json
  - plan.md
  - src-tauri/src/core/mod.rs
  - src/views/Plugins.tsx
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/plugin_inventory.rs
  - src/App.tsx
  - src-tauri/src/commands/plugins.rs
  - src-tauri/src/lib.rs
  - src/i18n/en.json
  - src/lib/tauri.ts
-->

---
### Requirement: CLI execution is bounded and failures are sanitized

AgentDeck MUST impose a 10-second deadline and a 1,048,576-byte limit separately on stdout and stderr for every fixed Plugin CLI invocation. It MUST terminate and reap a timed-out or oversized child. Diagnostics MUST contain only Agent, fixed capability, a code from `cli_missing`, `unsupported_cli`, `timeout`, `non_zero_exit`, `invalid_json`, `output_too_large`, or `marketplace_unavailable`, and an optional numeric exit status. Raw output, parser excerpts, environment values, credentials, and filesystem paths MUST NOT appear in diagnostics or logs.

#### Scenario: Exact output boundary succeeds

- **GIVEN** a fake fixed Plugin command writes 1,048,576 bytes to stdout and valid JSON is present within that bounded result
- **WHEN** the runner collects the command
- **THEN** it accepts the stream without an `output_too_large` diagnostic
- **AND** the child exits and is reaped

#### Scenario: One byte above the output boundary fails closed

- **GIVEN** a fake fixed Plugin command writes 1,048,577 bytes to stdout or stderr
- **WHEN** the runner collects the command
- **THEN** it terminates and reaps the child
- **AND** it returns `output_too_large`
- **AND** no captured bytes appear in the serialized diagnostic

##### Example: Output boundaries

| stdout bytes | stderr bytes | Expected |
| ----- | ----- | ----- |
| 1048576 | 0 | accepted |
| 1048577 | 0 | `output_too_large` |
| 0 | 1048577 | `output_too_large` |

#### Scenario: Timeout and non-zero exit remain typed

- **GIVEN** one fake command exceeds 10 seconds and another exits with status 23 while writing `sentinel-secret` to stderr
- **WHEN** each command is collected
- **THEN** the first returns `timeout` after the child is terminated and reaped
- **AND** the second returns `non_zero_exit` with exit status 23
- **AND** neither serialized result nor logs contain `sentinel-secret`


<!-- @trace
source: inspect-codex-claude-plugins
updated: 2026-08-13
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/zh-TW.json
  - src/components/Sidebar.tsx
  - package.json
  - plan.md
  - src-tauri/src/core/mod.rs
  - src/views/Plugins.tsx
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/plugin_inventory.rs
  - src/App.tsx
  - src-tauri/src/commands/plugins.rs
  - src-tauri/src/lib.rs
  - src/i18n/en.json
  - src/lib/tauri.ts
-->

---
### Requirement: Agent-specific JSON is normalized without invented values

AgentDeck SHALL parse Codex and Claude Code Plugin JSON with separate adapters and SHALL normalize each record into Agent, plugin id, display name, installed state, availability, installed version, available version, scope, marketplace, enabled state, and update state. Version values MUST remain opaque strings. Every absent or unrecognized status-like field MUST become `unknown`; it MUST NOT be inferred from another Agent or from a field with a different meaning. Additive unknown JSON fields MUST NOT invalidate an otherwise supported response.

#### Scenario: Missing fields stay unknown

- **GIVEN** a Codex item identifies plugin `reviewer` and marketplace `official` but omits scope, enabled, available version, and update state
- **WHEN** the Codex adapter normalizes the item
- **THEN** plugin id and marketplace retain their exact values
- **AND** scope, enabled state, available version, and update state are `unknown`
- **AND** the adapter does not copy defaults from Claude Code

#### Scenario: Versions remain opaque

- **GIVEN** one CLI reports version `release-2026.08+vendor`
- **WHEN** the item is normalized
- **THEN** the DTO retains `release-2026.08+vendor` exactly
- **AND** AgentDeck does not classify it as newer, older, or invalid

#### Scenario: Additive fields do not break a supported response

- **GIVEN** a valid Plugin record contains an unrecognized sibling field `futureMetadata`
- **WHEN** its Agent-specific adapter parses the response
- **THEN** the known Plugin fields remain available
- **AND** `futureMetadata` is not exposed as a supported normalized field
- **AND** the response has no `invalid_json` diagnostic


<!-- @trace
source: inspect-codex-claude-plugins
updated: 2026-08-13
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/zh-TW.json
  - src/components/Sidebar.tsx
  - package.json
  - plan.md
  - src-tauri/src/core/mod.rs
  - src/views/Plugins.tsx
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/plugin_inventory.rs
  - src/App.tsx
  - src-tauri/src/commands/plugins.rs
  - src-tauri/src/lib.rs
  - src/i18n/en.json
  - src/lib/tauri.ts
-->

---
### Requirement: Inventory identity, merge, and ordering are deterministic

AgentDeck SHALL identify a route-local Plugin record by Agent, marketplace, and plugin id. It SHALL merge installed and available facts only for the same key, SHALL preserve separate records for equal plugin ids from different Agents or marketplaces, and SHALL sort records by Agent, display name, marketplace, then plugin id. An installed fact SHALL win only the installed-presence field; every other field SHALL merge only from known values on the same key.

#### Scenario: Same plugin id in two marketplaces remains distinct

- **GIVEN** Codex marketplaces `official` and `team` each list plugin id `reviewer`
- **WHEN** inventory is normalized
- **THEN** two Codex records are returned
- **AND** each route-local id contains its own marketplace identity
- **AND** neither marketplace's version or state overwrites the other

#### Scenario: Installed and available facts merge within one key

- **GIVEN** an installed record for `codex:official:reviewer` has installed version `1.0`
- **AND** an available record for the same key has available version `1.1`
- **WHEN** inventory is normalized
- **THEN** one record is returned with installed state `installed`, installed version `1.0`, and available version `1.1`
- **AND** update state remains `unknown` unless the Codex JSON reports it explicitly

#### Scenario: Ordering is stable

- **GIVEN** Plugin records arrive in different CLI array orders across two refreshes
- **WHEN** both responses are normalized
- **THEN** their final order is identical by Agent, display name, marketplace, then plugin id


<!-- @trace
source: inspect-codex-claude-plugins
updated: 2026-08-13
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/zh-TW.json
  - src/components/Sidebar.tsx
  - package.json
  - plan.md
  - src-tauri/src/core/mod.rs
  - src/views/Plugins.tsx
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/plugin_inventory.rs
  - src/App.tsx
  - src-tauri/src/commands/plugins.rs
  - src-tauri/src/lib.rs
  - src/i18n/en.json
  - src/lib/tauri.ts
-->

---
### Requirement: Agent and marketplace failures are isolated

AgentDeck SHALL return one result per Agent containing CLI version, supported read capabilities, marketplaces, Plugin items, and diagnostics. A failed Agent or marketplace command MUST NOT remove successful results from another Agent or from another capability of the same Agent. Invalid top-level JSON MUST invalidate only the command that produced it.

#### Scenario: Marketplace listing fails while installed items remain visible

- **GIVEN** Claude Code Plugin listing succeeds and its marketplace listing returns a recognized offline failure
- **WHEN** inventory is requested
- **THEN** Claude Code installed items remain present
- **AND** its marketplace capability contains `marketplace_unavailable`
- **AND** Codex collection is unaffected

#### Scenario: Invalid Codex JSON does not suppress Claude Code

- **GIVEN** Codex Plugin listing emits syntactically invalid JSON
- **AND** Claude Code listing emits valid JSON with one installed plugin
- **WHEN** inventory is requested
- **THEN** Codex Plugin listing contains `invalid_json`
- **AND** the Claude Code plugin remains present
- **AND** no Codex parser text appears in the response


<!-- @trace
source: inspect-codex-claude-plugins
updated: 2026-08-13
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/zh-TW.json
  - src/components/Sidebar.tsx
  - package.json
  - plan.md
  - src-tauri/src/core/mod.rs
  - src/views/Plugins.tsx
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/plugin_inventory.rs
  - src/App.tsx
  - src-tauri/src/commands/plugins.rs
  - src-tauri/src/lib.rs
  - src/i18n/en.json
  - src/lib/tauri.ts
-->

---
### Requirement: Plugin inventory is transient and excludes sensitive payload

AgentDeck SHALL keep Plugin inventory only in backend memory, the current IPC response, and route-local UI state. It MUST NOT create Plugin Artifacts, detail rows, deployments, Library files, Git backup changes, localStorage entries, or direct official cache reads. Captured stdout and stderr MUST NOT be persisted or logged.

#### Scenario: Refresh has no persistence side effects

- **GIVEN** SQLite rows, Library tree hashes, Git status, localStorage keys, and official Plugin cache bytes are recorded before refresh
- **WHEN** the user loads and refreshes the Plugins page
- **THEN** every recorded persistent value remains unchanged
- **AND** no kind `plugin` Artifact or deployment row is created

#### Scenario: Sensitive CLI text is not retained

- **GIVEN** an unknown CLI field or stderr contains `sentinel-secret`
- **WHEN** collection succeeds or fails
- **THEN** `sentinel-secret` is absent from diagnostics, logs, SQLite, Library files, Git changes, and localStorage
- **AND** raw captured streams are released after normalization or failure handling


<!-- @trace
source: inspect-codex-claude-plugins
updated: 2026-08-13
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/zh-TW.json
  - src/components/Sidebar.tsx
  - package.json
  - plan.md
  - src-tauri/src/core/mod.rs
  - src/views/Plugins.tsx
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/plugin_inventory.rs
  - src/App.tsx
  - src-tauri/src/commands/plugins.rs
  - src-tauri/src/lib.rs
  - src/i18n/en.json
  - src/lib/tauri.ts
-->

---
### Requirement: Plugins page exposes complete read-only inventory

AgentDeck SHALL provide a `/plugins` route and Sidebar entry only when the functional Plugins page is present. The page SHALL display per-Agent availability, Plugin presence, opaque versions, scope, marketplace, enabled state, update state, localized diagnostics, read-only details, and filters for Agent, installed／available presence, scope, marketplace, and status. Unknown values SHALL remain visibly unknown. The page MUST NOT render or invoke Plugin or marketplace mutation, validation, details, or eval controls.

#### Scenario: User filters Plugin inventory

- **GIVEN** the response contains installed and available records from Codex and Claude Code across two marketplaces
- **WHEN** the user selects Agent `claude_code`, presence `installed`, and one marketplace
- **THEN** only matching records remain visible
- **AND** clearing filters restores every usable record
- **AND** diagnostics remain visible independently of item filters

#### Scenario: Refresh uses latest-request-wins state

- **GIVEN** one Plugin refresh is pending
- **WHEN** the user starts a newer refresh and the older request finishes last
- **THEN** the older response does not replace the newer route-local state
- **AND** no inventory is copied into `AppContext` or localStorage

#### Scenario: Mutation controls are absent

- **WHEN** the Plugins page renders an installed item and an available update
- **THEN** no install, update, remove, uninstall, enable, disable, validate, details, eval, or marketplace mutation control is rendered
- **AND** English and Traditional Chinese locale files contain matching keys for every visible Plugin string

<!-- @trace
source: inspect-codex-claude-plugins
updated: 2026-08-13
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/zh-TW.json
  - src/components/Sidebar.tsx
  - package.json
  - plan.md
  - src-tauri/src/core/mod.rs
  - src/views/Plugins.tsx
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/plugin_inventory.rs
  - src/App.tsx
  - src-tauri/src/commands/plugins.rs
  - src-tauri/src/lib.rs
  - src/i18n/en.json
  - src/lib/tauri.ts
-->