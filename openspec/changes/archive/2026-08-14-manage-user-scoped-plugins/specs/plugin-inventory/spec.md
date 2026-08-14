## MODIFIED Requirements

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

## ADDED Requirements

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
