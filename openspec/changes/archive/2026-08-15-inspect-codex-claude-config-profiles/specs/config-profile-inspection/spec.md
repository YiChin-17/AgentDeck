## ADDED Requirements

### Requirement: Inventory reads only fixed supported sources

AgentDeck SHALL discover Codex and Claude Code configuration only from backend-composed fixed paths. Codex sources SHALL be the user `~/.codex/config.toml` and the selected registered project's `.codex/config.toml`; Claude Code sources SHALL be the user `~/.claude/settings.json` and the selected registered project's `.claude/settings.json` and `.claude/settings.local.json`. A caller MUST NOT supply a home directory, project root, source path, current working directory, environment, or additional filename.

#### Scenario: User-only inventory

- **WHEN** the caller requests inventory without a project ID
- **THEN** AgentDeck reads only the fixed Codex and Claude Code user sources
- **AND** no project source is probed

##### Example: Temporary user home

- **GIVEN** the backend test home is `/tmp/agentdeck-home`
- **WHEN** inventory is requested without a project ID
- **THEN** only `/tmp/agentdeck-home/.codex/config.toml` and `/tmp/agentdeck-home/.claude/settings.json` are inspected

#### Scenario: Registered project inventory

- **WHEN** the caller requests inventory for an existing AgentDeck project ID
- **THEN** AgentDeck obtains the project root from the stored project record
- **AND** reads only the fixed project and local source paths beneath that root

##### Example: Stored project root

- **GIVEN** project `project-1` is stored with root `/tmp/demo`
- **WHEN** inventory is requested for `project-1`
- **THEN** the project probes are exactly `/tmp/demo/.codex/config.toml`, `/tmp/demo/.claude/settings.json`, and `/tmp/demo/.claude/settings.local.json`

#### Scenario: Unknown project is rejected before reading

- **WHEN** the caller supplies an unknown project ID
- **THEN** AgentDeck returns `project_not_found`
- **AND** reads no user or project configuration source

### Requirement: Source reads are bounded and isolated

AgentDeck MUST inspect each fixed source independently without executing an Agent CLI or following a symbolic link. Each source read MUST be limited to 1 MiB. A source MUST report exactly one status from `missing`, `available`, `unreadable`, `too_large`, `unsupported_symlink`, or `invalid_format`, and failure of one source MUST NOT prevent other sources from being returned.

#### Scenario: Missing source remains non-fatal

- **WHEN** one fixed source does not exist
- **THEN** that source has status `missing`
- **AND** all other fixed sources are still inspected

#### Scenario: Oversized source is not parsed

- **WHEN** a fixed source exceeds 1 MiB
- **THEN** that source has status `too_large`
- **AND** AgentDeck does not parse or return settings from that source

#### Scenario: Symbolic link is not followed

- **WHEN** a fixed source path is a symbolic link
- **THEN** that source has status `unsupported_symlink`
- **AND** AgentDeck does not read the link target

#### Scenario: Invalid format is isolated

- **WHEN** a Codex TOML or Claude Code JSON source cannot be parsed
- **THEN** that source has status `invalid_format`
- **AND** settings from other valid sources remain available
- **AND** the response excludes parser input and raw parser error text

### Requirement: Only exact non-sensitive scalar settings cross the backend boundary

AgentDeck SHALL extract only exact allowlisted keys with exact scalar types. The Codex allowlist SHALL contain `model`, `model_reasoning_effort`, `model_verbosity`, string-form `approval_policy`, `sandbox_mode`, `web_search`, `service_tier`, and `personality`. The Claude Code allowlist SHALL contain `model`, `alwaysThinkingEnabled`, `autoUpdatesChannel`, `cleanupPeriodDays`, `fastMode`, and `permissions.defaultMode`. All other keys and values MUST NOT appear in a serializable DTO, diagnostic, or log.

#### Scenario: Allowlisted values are normalized

- **WHEN** a valid source contains allowlisted values with their required scalar types
- **THEN** each returned setting includes Agent, canonical key, native key, typed display value, scope, source ID, optional project ID, and resolution
- **AND** no generic JSON or TOML value is included

##### Example: Mixed scalar normalization

| Agent | Native input | Typed display value |
| ----- | ------------ | ------------------- |
| Codex | `sandbox_mode = "read-only"` | string `read-only` |
| Codex | `model_reasoning_effort = "high"` | string `high` |
| Claude Code | `"alwaysThinkingEnabled": true` | boolean `true` |
| Claude Code | `"cleanupPeriodDays": 20` | integer `20` |

#### Scenario: Sensitive and unknown content is excluded

- **WHEN** a valid source contains `env`, credentials, token-like values, API key helpers, commands, paths, Hooks, MCP configuration, Plugin configuration, permission rules, or unknown keys
- **THEN** none of their key names or values appears in the response
- **AND** the source can report only that unexposed fields exist

#### Scenario: Invalid allowlisted value fails closed

- **WHEN** an allowlisted key has a non-allowlisted shape or type
- **THEN** AgentDeck omits that key and returns `invalid_allowed_value` naming only the allowlisted key
- **AND** other valid allowlisted settings from the same source remain available

#### Scenario: Serialized response contains no raw source material

- **WHEN** the inventory response is serialized
- **THEN** it contains no raw document, raw bytes, unknown key name, unknown value, parser message, or operating-system error detail

##### Example: Secret-bearing source

- **GIVEN** a Claude source contains `{"model":"sonnet","env":{"ANTHROPIC_API_KEY":"secret-123"}}`
- **WHEN** the inventory response is serialized
- **THEN** it contains `sonnet` but contains neither `ANTHROPIC_API_KEY` nor `secret-123`

### Requirement: Inventory exposes source identity without source content

AgentDeck SHALL return one source record per fixed source with an opaque source ID, Agent, scope, optional project ID, fixed display path, status, optional SHA-256 fingerprint, and `has_unexposed_fields`. A fingerprint MUST be present only for a successfully parsed `available` source and MUST be calculated from that source snapshot.

#### Scenario: Available source has stable snapshot identity

- **WHEN** a fixed source parses successfully
- **THEN** its source record has status `available`
- **AND** includes the SHA-256 fingerprint of the bytes parsed for that response

#### Scenario: Failed source exposes no fingerprint

- **WHEN** a source is missing, unreadable, too large, a symbolic link, or invalid
- **THEN** its source record has no fingerprint

##### Example: Fingerprint boundary by status

| Source status | Fingerprint |
| ------------- | ----------- |
| `available` | SHA-256 string |
| `missing` | absent |
| `too_large` | absent |
| `unsupported_symlink` | absent |
| `invalid_format` | absent |

### Requirement: Precedence and diff are limited to supported sources

AgentDeck SHALL calculate resolution and normalized diff only from the allowlisted settings in the supported sources returned by this capability. Codex ordering SHALL be user then project; Claude Code ordering SHALL be user then project then local. The UI MUST state that CLI flags, environment, managed policy, project trust, and unscanned sources can change actual runtime values. Codex project settings MUST be labeled `project_candidate` rather than asserted as runtime-active.

#### Scenario: Higher supported Claude scope overrides a lower scope

- **GIVEN** the same Claude Code allowlisted key is present in user, project, and local sources
- **WHEN** AgentDeck resolves the supported-source inventory
- **THEN** the local setting is `observed_active`
- **AND** the project and user settings are `observed_overridden`

##### Example: Claude model precedence

- **GIVEN** user `model` is `sonnet`, project `model` is `opus`, and local `model` is `haiku`
- **WHEN** inventory is normalized for that registered project
- **THEN** `haiku` is the observed active value and the two lower-scope values remain visible as overridden

#### Scenario: Codex project value remains conditional

- **GIVEN** a Codex allowlisted key exists in both user and project sources
- **WHEN** AgentDeck resolves the supported-source inventory
- **THEN** the project value is labeled `project_candidate`
- **AND** the UI does not claim that Codex loaded it for the current runtime session

#### Scenario: Normalized diff never includes raw document text

- **WHEN** two supported source layers differ
- **THEN** AgentDeck reports only `same`, `added`, `changed`, or `removed` for allowlisted typed values
- **AND** returns no raw TOML or JSON diff

### Requirement: Config Profiles page is inspection-only

AgentDeck SHALL provide a Config Profiles page with Agent, scope, and registered-project filters, refresh, source statuses, typed diagnostics, normalized settings, source resolution, and allowlisted diff. The page MUST NOT render create, edit, assign, apply, backup, restore, or secret-storage controls.

#### Scenario: User inspects a valid registered project

- **WHEN** the user opens Config Profiles and selects a registered project
- **THEN** the page displays available fixed sources and allowlisted settings for the selected filters
- **AND** identifies each setting's Agent, scope, source, and observed resolution

##### Example: Selected Claude project

- **GIVEN** project `Demo` has user model `sonnet` and local model `opus`
- **WHEN** the user selects `Demo`, Claude Code, and all scopes
- **THEN** both values are visible and `opus` is marked `observed_active`

#### Scenario: Source failure has a specific empty state

- **WHEN** a selected source is missing or has a failure status
- **THEN** the page distinguishes that status from an inventory with no allowlisted settings
- **AND** valid sources remain inspectable

##### Example: Invalid project beside valid user source

- **GIVEN** the user source is valid and the project source has status `invalid_format`
- **WHEN** the page renders
- **THEN** the user settings remain visible and the project source shows an invalid-format state rather than an empty inventory state

#### Scenario: Runtime limitation is visible

- **WHEN** the page displays supported-source resolution
- **THEN** it also displays that CLI flags, environment, managed policy, project trust, and unscanned sources are outside the resolution

##### Example: Codex project candidate

- **GIVEN** a Codex project source contains `sandbox_mode = "read-only"`
- **WHEN** the page displays that value
- **THEN** it labels the value `project_candidate` and displays the runtime-limitation notice

### Requirement: Inspection produces no persistent side effects

Config Profile inspection MUST NOT write, repair, format, rename, or delete a source file. It MUST NOT write the Library, SQLite database, Application Support state, logs, Git backup metadata, or system secret storage, and MUST NOT trigger Library synchronization or deletion.

#### Scenario: Refresh leaves all storage unchanged

- **GIVEN** snapshots of all fixed sources, the Library, SQLite database, and Application Support state
- **WHEN** the user loads and refreshes Config Profiles
- **THEN** every snapshot remains byte-for-byte unchanged

#### Scenario: Offline Library does not change inspection authority

- **WHEN** the configured external Library is offline
- **THEN** AgentDeck can still inspect the fixed configuration sources
- **AND** performs no Library synchronization, deletion, or fallback write

##### Example: Unmounted external Library

- **GIVEN** the configured Library path `/Volumes/AgentDeck-Library` is unavailable and the fixed user settings exist
- **WHEN** Config Profiles inventory is refreshed
- **THEN** the user settings are returned without creating or modifying any Library path
