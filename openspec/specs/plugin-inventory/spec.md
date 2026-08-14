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

AgentDeck SHALL provide a `/plugins` route and Sidebar entry only when the functional Plugins page is present. The page SHALL display per-Agent availability, Plugin presence, opaque versions, scope, marketplace, enabled state, update state, localized diagnostics, read-only details, and filters for Agent, installed/available presence, scope, marketplace, and status. Unknown values SHALL remain visibly unknown. The inventory adapter boundary SHALL remain route-local, lossless and read-only: the page MUST NOT render or invoke arbitrary process execution, validation, details, or eval controls, and MUST NOT supply an executable, filesystem path, working directory, environment override, or CLI argument. The page SHALL render mutation controls only through the fixed user-scope contracts defined by the `Plugin mutations use an Agent-specific fixed capability matrix` and `Plugins page previews and confirms only supported mutations` requirements in this change.

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

#### Scenario: Mutation controls are limited to fixed user-scope operations

- **WHEN** the Plugins page renders an installed item and an available update
- **THEN** no validation, details, eval, or marketplace mutation control is rendered
- **AND** mutation controls are limited to the backend capability matrix for fixed user-scope install, update, remove, enable, and disable
- **AND** English and Traditional Chinese locale files contain matching keys for every visible Plugin string

##### Example: Read-only inventory with fixed mutation controls

- **GIVEN** the backend reports Codex mutations `install` and `remove`
- **AND** an available `reviewer@official` Codex record is not installed
- **WHEN** the Plugins page renders that record
- **THEN** install is enabled and remove is disabled
- **AND** no validation, details, eval, marketplace mutation, arbitrary executable, or caller-controlled argument control is rendered


<!-- @trace
source: manage-user-scoped-plugins
updated: 2026-08-14
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/en.json
  - plan.md
  - src-tauri/src/lib.rs
  - scripts/check-plugin-mutations.mjs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/mod.rs
  - src/i18n/zh-TW.json
  - src/lib/tauri.ts
  - src/views/Plugins.tsx
  - src-tauri/src/core/plugin_mutation.rs
  - src-tauri/src/core/plugin_inventory.rs
  - package.json
  - src-tauri/src/commands/plugin_mutation.rs
-->

---
### Requirement: Plugin mutations use an Agent-specific fixed capability matrix

AgentDeck SHALL expose the normalized operations `install`, `update`, `remove`, `enable`, and `disable` only through a backend-owned capability matrix. Codex SHALL support `install` through `codex plugin add --json` and `remove` through `codex plugin remove --json`. Claude Code SHALL support user-scope `install`, `update`, `remove`, `enable`, and `disable` through its corresponding official Plugin commands. AgentDeck MUST return `operation_unsupported` without starting a process for every matrix entry that is absent. The frontend MUST NOT supply an executable, working directory, environment override, scope, option, or CLI argument.

#### Scenario: Codex exposes only its documented mutations

- **GIVEN** Codex CLI 0.144.5 is available
- **WHEN** the Plugins page requests mutation capabilities
- **THEN** Codex reports `install` and `remove`
- **AND** Codex does not report `update`, `enable`, or `disable`

##### Example: Codex operation mapping

| Normalized operation | Fixed command prefix | Result |
| ----- | ----- | ----- |
| `install` | `codex plugin add --json --` | supported |
| `remove` | `codex plugin remove --json --` | supported |
| `update` | none | `operation_unsupported` |
| `enable` | none | `operation_unsupported` |
| `disable` | none | `operation_unsupported` |

#### Scenario: Claude Code commands fix the scope to user

- **WHEN** AgentDeck builds a supported Claude Code mutation
- **THEN** it passes the corresponding `claude plugin` subcommand
- **AND** it passes `--scope user` before the option terminator
- **AND** it does not pass `-y`, `--config`, `--keep-data`, `--prune`, or `--all`


<!-- @trace
source: manage-user-scoped-plugins
updated: 2026-08-14
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/en.json
  - plan.md
  - src-tauri/src/lib.rs
  - scripts/check-plugin-mutations.mjs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/mod.rs
  - src/i18n/zh-TW.json
  - src/lib/tauri.ts
  - src/views/Plugins.tsx
  - src-tauri/src/core/plugin_mutation.rs
  - src-tauri/src/core/plugin_inventory.rs
  - package.json
  - src-tauri/src/commands/plugin_mutation.rs
-->

---
### Requirement: Mutation selectors are derived from fresh inventory

AgentDeck MUST build a Plugin selector only from an exact Agent, marketplace, and plugin id record in a newly collected inventory. It MUST reject an empty component, a component containing NUL or control characters, a component beginning with `-`, or a component longer than 512 bytes. It MUST place `--` before the single selector argument. Install preview SHALL require an available record that is not installed. Update, remove, enable, and disable preview SHALL require an installed record, and enable or disable SHALL require the opposite known enabled state. Claude Code update, remove, enable, and disable SHALL additionally require inventory scope `user`. Claude Code install SHALL accept an available-only record whose scope and installed state remain `unknown`, because the fixed command owns `--scope user`; it MUST reject an explicit project or local scope. Codex install and remove SHALL use the fixed user-scope CLI capability even when Codex inventory scope is `unknown`, and AgentDeck MUST NOT rewrite that inventory field to `user`.

#### Scenario: Claude available-only install uses the fixed user scope

- **GIVEN** Claude Code inventory reports `reviewer@official` as available with installed state and scope both `unknown`
- **WHEN** install preview is requested for that exact identity
- **THEN** AgentDeck permits the fixed Claude Code install capability
- **AND** preview scope is `user`
- **AND** update, remove, enable, and disable remain unavailable for that record

#### Scenario: Caller identity absent from fresh inventory is rejected

- **GIVEN** the frontend submits Agent `claude_code`, plugin id `reviewer`, and marketplace `team`
- **AND** fresh inventory contains only `claude_code:official:reviewer`
- **WHEN** mutation preview is requested
- **THEN** AgentDeck returns `identity_not_found`
- **AND** no Plugin mutation process starts

#### Scenario: Option-like selector cannot become a CLI flag

- **GIVEN** a Plugin identity component begins with `--config`
- **WHEN** mutation preview is requested
- **THEN** AgentDeck returns `precondition_failed`
- **AND** it does not construct or execute a mutation argv

#### Scenario: Codex remove keeps unknown inventory scope lossless

- **GIVEN** Codex inventory reports an installed `reviewer@official` record with scope `unknown`
- **WHEN** remove preview is requested for that exact identity
- **THEN** AgentDeck permits the fixed Codex user-scope remove capability
- **AND** preview scope is `user`
- **AND** the inventory record scope remains `unknown`


<!-- @trace
source: manage-user-scoped-plugins
updated: 2026-08-14
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/en.json
  - plan.md
  - src-tauri/src/lib.rs
  - scripts/check-plugin-mutations.mjs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/mod.rs
  - src/i18n/zh-TW.json
  - src/lib/tauri.ts
  - src/views/Plugins.tsx
  - src-tauri/src/core/plugin_mutation.rs
  - src-tauri/src/core/plugin_inventory.rs
  - package.json
  - src-tauri/src/commands/plugin_mutation.rs
-->

---
### Requirement: Mutation preview binds one intent to a transient token

AgentDeck SHALL provide `preview_plugin_mutation` with request fields limited to Agent, normalized operation, plugin id, and marketplace. It MUST collect fresh inventory, validate the fixed capability and preconditions, calculate a SHA-256 fingerprint over the selected item, Agent CLI version, read capabilities, and operation, and store the full intent under a UUID v4 token. The preview response SHALL contain the token, 120-second expiry, Agent, operation, identity, fixed user scope, non-sensitive argv display, destructive flag, and base fingerprint. Preview state MUST remain in backend memory, MUST contain at most 128 pending entries, and MUST NOT be written to logs, SQLite, Library files, Git backup, AppContext, or localStorage.

#### Scenario: Preview returns a reviewable fixed intent

- **GIVEN** `claude_code:official:reviewer` is available, not installed, and valid for user scope
- **WHEN** install preview is requested
- **THEN** the response identifies `claude_code`, `install`, `reviewer`, `official`, and scope `user`
- **AND** argv display is `claude plugin install --scope user -- reviewer@official`
- **AND** the response contains a UUID token and an expiry exactly 120 seconds after creation

#### Scenario: Pending preview capacity is bounded

- **GIVEN** 128 unexpired preview tokens already exist
- **WHEN** one more valid preview is created
- **THEN** AgentDeck evicts the token with the earliest expiry
- **AND** the pending token count remains 128


<!-- @trace
source: manage-user-scoped-plugins
updated: 2026-08-14
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/en.json
  - plan.md
  - src-tauri/src/lib.rs
  - scripts/check-plugin-mutations.mjs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/mod.rs
  - src/i18n/zh-TW.json
  - src/lib/tauri.ts
  - src/views/Plugins.tsx
  - src-tauri/src/core/plugin_mutation.rs
  - src-tauri/src/core/plugin_inventory.rs
  - package.json
  - src-tauri/src/commands/plugin_mutation.rs
-->

---
### Requirement: Apply consumes the token and rejects stale state

AgentDeck SHALL provide `apply_plugin_mutation` with a token as its only request field. It MUST atomically consume the token before starting a CLI process, reject missing, expired, or replayed tokens without starting a process, recollect inventory under a global mutation gate, and compare the current fingerprint with the preview fingerprint. A mismatch in identity, scope, CLI version, capability, presence, versions, or enabled state MUST return `stale_preview`. All Plugin mutations MUST execute one at a time across both Agents.

#### Scenario: Replayed token cannot repeat a mutation

- **GIVEN** a valid token has already reached apply
- **WHEN** the same token is submitted again
- **THEN** AgentDeck rejects the request as an expired or missing preview
- **AND** no second CLI process starts

#### Scenario: External state change invalidates preview

- **GIVEN** preview records installed version `1.0` for `reviewer@official`
- **AND** the official CLI state changes that installed version to `1.1` before apply
- **WHEN** apply recollects inventory
- **THEN** AgentDeck returns `stale_preview`
- **AND** it does not start the mutation command


<!-- @trace
source: manage-user-scoped-plugins
updated: 2026-08-14
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/en.json
  - plan.md
  - src-tauri/src/lib.rs
  - scripts/check-plugin-mutations.mjs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/mod.rs
  - src/i18n/zh-TW.json
  - src/lib/tauri.ts
  - src/views/Plugins.tsx
  - src-tauri/src/core/plugin_mutation.rs
  - src-tauri/src/core/plugin_inventory.rs
  - package.json
  - src-tauri/src/commands/plugin_mutation.rs
-->

---
### Requirement: Mutation execution remains bounded and sanitized

AgentDeck MUST invoke a mutation without a shell and with stdin closed. It MUST impose a 10-second deadline and a 1,048,576-byte limit separately on stdout and stderr, and it MUST terminate and reap a timed-out or oversized child. It MUST NOT return or log raw output, parser excerpts, paths, credentials, environment values, or Plugin payload. For Claude install or update, AgentDeck SHALL map only fixture-pinned official non-TTY confirmation phrases in bounded stderr to `interactive_confirmation_required`; every other non-zero exit SHALL remain `non_zero_exit` with optional numeric status.

#### Scenario: Interactive marketplace command is not auto-accepted

- **GIVEN** Claude Code install requires confirmation for a marketplace-declared command in a non-TTY process
- **WHEN** AgentDeck executes the fixed install command with stdin closed
- **THEN** AgentDeck does not pass `-y`
- **AND** it returns `interactive_confirmation_required`
- **AND** the command text and captured stderr are absent from the serialized diagnostic and logs

#### Scenario: Oversized mutation output fails closed

- **GIVEN** a fake mutation writes 1,048,577 bytes to stdout or stderr
- **WHEN** AgentDeck executes the mutation
- **THEN** it terminates and reaps the child
- **AND** it returns `output_too_large`
- **AND** no captured bytes appear in the diagnostic


<!-- @trace
source: manage-user-scoped-plugins
updated: 2026-08-14
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/en.json
  - plan.md
  - src-tauri/src/lib.rs
  - scripts/check-plugin-mutations.mjs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/mod.rs
  - src/i18n/zh-TW.json
  - src/lib/tauri.ts
  - src/views/Plugins.tsx
  - src-tauri/src/core/plugin_mutation.rs
  - src-tauri/src/core/plugin_inventory.rs
  - package.json
  - src-tauri/src/commands/plugin_mutation.rs
-->

---
### Requirement: Mutation success requires post-operation inventory proof

AgentDeck MUST recollect Plugin inventory after a mutation process exits successfully. Install SHALL succeed only when the target is installed; remove SHALL succeed only when the target is absent or explicitly not installed; enable and disable SHALL succeed only when enabled state equals the requested state; update SHALL succeed only when installed version changes and, when preview supplied an available version, the new installed version equals it. Refresh failure, unknown target state, or an unmet postcondition MUST return `verification_failed` and the latest sanitized inventory, and MUST NOT report success optimistically.

#### Scenario: Exit zero without observable state change is not success

- **GIVEN** update preview records installed version `1.0` and available version `1.1`
- **AND** the update process exits with status 0
- **AND** refreshed inventory still reports installed version `1.0`
- **WHEN** AgentDeck verifies the mutation
- **THEN** it returns `verification_failed`
- **AND** the Plugins page does not display an update success state

##### Example: Operation postconditions

| Operation | Preview state | Required refreshed state |
| ----- | ----- | ----- |
| `install` | available, not installed | installed |
| `remove` | installed | absent or not installed |
| `enable` | disabled | enabled |
| `disable` | enabled | disabled |
| `update` | installed `1.0`, available `1.1` | installed `1.1` |


<!-- @trace
source: manage-user-scoped-plugins
updated: 2026-08-14
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/en.json
  - plan.md
  - src-tauri/src/lib.rs
  - scripts/check-plugin-mutations.mjs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/mod.rs
  - src/i18n/zh-TW.json
  - src/lib/tauri.ts
  - src/views/Plugins.tsx
  - src-tauri/src/core/plugin_mutation.rs
  - src-tauri/src/core/plugin_inventory.rs
  - package.json
  - src-tauri/src/commands/plugin_mutation.rs
-->

---
### Requirement: Plugins page previews and confirms only supported mutations

The Plugins page SHALL render mutation controls from the backend capability matrix and current item state. It MUST disable operations for unsupported or unmet-precondition records and display a localized fixed reason. It MUST also disable Claude Code mutation for non-user or unknown-scope records. It SHALL allow backend-declared Codex install and remove for an exact record whose inventory scope remains `unknown`, because the fixed Codex capability defines user scope. Every operation MUST request preview before apply. Remove and uninstall MUST display a destructive confirmation containing the Agent, plugin id, marketplace, scope, and argv display from that same preview. Apply MUST submit only the preview token. Verified success MUST replace route-local data with the returned fresh inventory; failure MUST display a localized fixed diagnostic and trigger a fresh inventory request.

#### Scenario: Unsupported Codex update is unavailable

- **GIVEN** a user-scope Codex Plugin is installed and has a newer available version
- **WHEN** the Plugins page renders its actions
- **THEN** update is unavailable because the Codex capability matrix omits it
- **AND** clicking or invoking the unavailable operation does not call preview or apply

#### Scenario: Destructive confirmation remains bound to preview

- **GIVEN** remove preview identifies Agent `codex`, plugin `reviewer`, marketplace `official`, and scope `user`
- **WHEN** the user opens the destructive confirmation
- **THEN** the dialog displays those exact preview values and its fixed argv display
- **AND** confirmation sends only that preview token to apply
- **AND** cancel sends no mutation request

<!-- @trace
source: manage-user-scoped-plugins
updated: 2026-08-14
code:
  - scripts/check-plugins-ui.mjs
  - src/i18n/en.json
  - plan.md
  - src-tauri/src/lib.rs
  - scripts/check-plugin-mutations.mjs
  - src-tauri/src/commands/mod.rs
  - src-tauri/src/core/mod.rs
  - src/i18n/zh-TW.json
  - src/lib/tauri.ts
  - src/views/Plugins.tsx
  - src-tauri/src/core/plugin_mutation.rs
  - src-tauri/src/core/plugin_inventory.rs
  - package.json
  - src-tauri/src/commands/plugin_mutation.rs
-->