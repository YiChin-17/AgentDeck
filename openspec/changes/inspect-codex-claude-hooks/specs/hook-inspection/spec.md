## ADDED Requirements

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

### Requirement: Hooks page exposes filters, diagnostics, details, and compatibility without mutation controls

AgentDeck SHALL provide a `/hooks` route and Sidebar entry. The page SHALL expose Agent, scope, event, source status, and Project selection controls; source diagnostics; a Hook Inspector; bounded source comparison; and the compatibility matrix. It MUST NOT render create, edit, delete, enable, disable, apply, execute, backup, or restore controls in this change.

#### Scenario: User filters and inspects a Hook

- **GIVEN** inspection returns Codex and Claude Code entries from user and project scopes
- **WHEN** the user selects Agent `codex`, scope `project`, and an event, then opens one result
- **THEN** only matching entries remain in the list
- **AND** the Inspector displays that entry's source, event, matcher, handler type, handler fields, and known or unknown markers

#### Scenario: Missing and invalid sources remain understandable

- **GIVEN** one source is missing and one source is invalid
- **WHEN** the Hooks page loads
- **THEN** the missing source is shown as a normal empty state
- **AND** the invalid source shows its sanitized source-specific diagnostic
- **AND** available entries and the compatibility matrix remain interactive

#### Scenario: Read-only scope is visible in the interface

- **WHEN** the Hooks page renders
- **THEN** it labels the current capability as read-only
- **AND** no mutation or execution control is present
- **AND** English and Traditional Chinese locale files contain matching keys for every new user-visible string
