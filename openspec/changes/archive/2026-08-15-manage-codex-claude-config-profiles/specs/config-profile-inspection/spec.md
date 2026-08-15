## ADDED Requirements

### Requirement: Config Profiles page separates inspection and management

AgentDeck SHALL preserve Agent, scope, and registered-Project inventory filters, refresh, source statuses, typed diagnostics, normalized settings, source resolution, and allowlisted diff on the Config Profiles page. The page SHALL present Config Profile management as an explicitly separate preview-first workflow and MUST NOT let management state change the authority, source set, or no-side-effect behavior of inventory requests.

#### Scenario: User inspects a valid registered Project

- **WHEN** the user opens Config Profiles and selects a registered Project
- **THEN** the inspection area displays available fixed sources and allowlisted settings for the selected filters
- **AND** identifies each setting's Agent, scope, source, and observed resolution

##### Example: Selected Claude Project

- **GIVEN** Project `Demo` has user model `sonnet` and local model `opus`
- **WHEN** the user selects `Demo`, Claude Code, and all scopes
- **THEN** both values are visible and `opus` is marked `observed_active`

#### Scenario: Source failure has a specific empty state

- **WHEN** a selected source is missing or has a failure status
- **THEN** the inspection area distinguishes that status from an inventory with no allowlisted settings
- **AND** valid sources remain inspectable

##### Example: Invalid Project beside valid user source

- **GIVEN** the user source is valid and the Project source has status `invalid_format`
- **WHEN** the page renders
- **THEN** the user settings remain visible and the Project source shows an invalid-format state rather than an empty inventory state

#### Scenario: Runtime limitation remains visible

- **WHEN** the inspection area displays supported-source resolution
- **THEN** it also displays that CLI flags, environment, managed policy, Project trust, and unscanned sources are outside the resolution

##### Example: Codex Project candidate

- **GIVEN** a Codex Project source contains `sandbox_mode = "read-only"`
- **WHEN** the inspection area displays that value
- **THEN** it labels the value `project_candidate` and displays the runtime-limitation notice

#### Scenario: Management selection does not mutate inspection

- **WHEN** the user selects or edits a Config Profile without confirming apply or restore
- **THEN** inventory requests continue to read only the fixed supported sources selected by the inventory filters
- **AND** the source files, Library, SQLite, Application Support, Git metadata, and system secret storage remain unchanged except for an explicitly saved profile or assignment transaction

## REMOVED Requirements

### Requirement: Config Profiles page is inspection-only

**Reason**: Phase 6 now adds an implemented, preview-first Config Profile management workflow beside the existing inspection area.

**Migration**: Preserve every inventory filter, diagnostic, setting, diff, empty state, runtime limitation, and no-side-effect inspection contract under `Config Profiles page separates inspection and management`; render mutation controls only inside the new management workflow.

#### Scenario: Existing inspection behavior migrates to the separated page

- **WHEN** the inspection-only requirement is replaced by the separated inspection and management requirement
- **THEN** every existing inventory filter, diagnostic, setting, diff, empty state, runtime limitation, and no-side-effect inspection scenario remains required
- **AND** only the absolute prohibition on implemented preview-first management controls is removed
