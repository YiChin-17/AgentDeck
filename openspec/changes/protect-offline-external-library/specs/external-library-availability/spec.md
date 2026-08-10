## ADDED Requirements

### Requirement: Application state remains available independently of an external Library

AgentDeck MUST keep the Tauri application's SQLite database, scenarios, cache, and logs on internal application storage when the Library content root is configured on an external volume. AgentDeck SHALL preserve the existing internal default Library behavior when no external Library is configured.

#### Scenario: External Library is offline at startup

- **GIVEN** an external Library is configured
- **AND** its volume is not mounted when AgentDeck starts
- **WHEN** AgentDeck initializes application state
- **THEN** AgentDeck opens its internal application state
- **AND** AgentDeck reports the configured Library as offline
- **AND** AgentDeck does not create the configured external path or a replacement Library

#### Scenario: Default internal Library remains unchanged

- **GIVEN** no external Library is configured
- **WHEN** AgentDeck starts
- **THEN** AgentDeck uses the existing internal Library root
- **AND** existing install, sync, delete, watcher, and backup behavior remains available

### Requirement: Legacy external repository configuration migrates without data loss

AgentDeck MUST migrate a legacy configured repository layout through a versioned, retryable copy-and-verify process. AgentDeck MUST NOT delete the legacy source, blind-merge conflicting state, or mark migration complete before the internal application state and external Library identity are verified.

#### Scenario: Online legacy repository migrates safely

- **GIVEN** a legacy `repo_path` contains a valid database, scenarios, and `skills` Library
- **AND** the internal migration target is safe to initialize
- **WHEN** AgentDeck upgrades the configuration
- **THEN** AgentDeck copies and verifies the required application state on internal storage
- **AND** AgentDeck configures the legacy `skills` directory as the external Library root
- **AND** AgentDeck retains the legacy source files for rollback

#### Scenario: Offline legacy repository defers migration

- **GIVEN** a legacy `repo_path` is configured
- **AND** its volume is offline during the first upgraded startup
- **WHEN** AgentDeck evaluates migration
- **THEN** AgentDeck records a retryable offline migration state
- **AND** AgentDeck does not create the legacy path or an empty Library
- **AND** AgentDeck does not mark migration complete

#### Scenario: Conflicting state blocks migration

- **GIVEN** both legacy and internal state locations contain non-equivalent application state
- **WHEN** AgentDeck evaluates migration
- **THEN** AgentDeck reports a stable migration-blocked reason
- **AND** AgentDeck does not merge, overwrite, or delete either location

### Requirement: Configured Library availability is verified without side effects

AgentDeck MUST determine availability from the configured root's readability, writability, and persistent Library identity. Availability probes MUST NOT create directories, files, databases, metadata, or deployment targets.

#### Scenario: Missing mountpoint is offline

- **GIVEN** a configured external Library path does not exist
- **WHEN** AgentDeck probes Library availability
- **THEN** AgentDeck reports `offline` with a missing-path reason
- **AND** the configured path remains absent

#### Scenario: Same path resolves to a different Library

- **GIVEN** the configured path exists
- **AND** its Library identity does not match the configured identity
- **WHEN** AgentDeck probes Library availability
- **THEN** AgentDeck reports `offline` with an identity-mismatch reason
- **AND** AgentDeck does not adopt or modify that directory

#### Scenario: Library is not writable

- **GIVEN** the configured Library exists and has the expected identity
- **AND** AgentDeck cannot safely write to it
- **WHEN** AgentDeck probes Library availability
- **THEN** AgentDeck reports `offline` with a not-writable reason
- **AND** no mutation command becomes available

### Requirement: Offline state fails closed across Library operations

AgentDeck MUST reject install, import, reimport, update, delete, deployment sync, scenario or Preset sync, metadata reindex or write, and Git backup operations while the Library is offline. Rejection MUST NOT mutate Library files, database records, metadata, Git state, or Agent and Project targets.

#### Scenario: Direct mutation call is rejected

- **GIVEN** the Library is offline
- **WHEN** a client directly invokes a protected mutation command
- **THEN** AgentDeck returns a `library_offline` error
- **AND** Library files, database rows, metadata, Git state, and deployment targets remain unchanged

#### Scenario: Startup work is suspended

- **GIVEN** the Library is offline during startup
- **WHEN** AgentDeck completes application initialization
- **THEN** AgentDeck skips Library metadata reindex, startup scenario application, file watching, and automatic backup
- **AND** missing Library content is not interpreted as deletion

#### Scenario: Runtime disconnect stops subsequent work

- **GIVEN** the Library was online
- **AND** its volume becomes unavailable before a protected mutation
- **WHEN** AgentDeck performs the pre-mutation availability check
- **THEN** AgentDeck transitions to offline
- **AND** the mutation returns `library_offline`
- **AND** no later step in that operation executes

### Requirement: Offline state is visible throughout the product

AgentDeck SHALL expose a Library availability DTO containing state, stable reason, configured path, and nullable Library identity. While offline, AgentDeck SHALL display a global localized `Library Offline` banner and SHALL disable Library, deployment, Preset sync, and backup actions while keeping Settings and diagnostics accessible.

#### Scenario: User sees offline status

- **GIVEN** the Library is offline
- **WHEN** the user opens any primary AgentDeck page
- **THEN** a localized `Library Offline` banner is visible
- **AND** the banner identifies the configured path and offers Retry

#### Scenario: Unsafe actions are unavailable

- **GIVEN** the Library is offline
- **WHEN** the user views Library, Agent Skills, Project, Preset, or Backup controls
- **THEN** controls that would mutate Library or deployment state are disabled
- **AND** Settings and diagnostics remain accessible

#### Scenario: Cached inventory does not imply file availability

- **GIVEN** internal application state contains last-known Skill records
- **AND** the Library is offline
- **WHEN** the user views cached inventory
- **THEN** AgentDeck marks it as offline last-known state
- **AND** attempts to open unavailable Library documents return the localized offline error

### Requirement: Reconnect restores service only after full verification

AgentDeck SHALL provide an explicit Retry operation that restores online state only after the configured root and Library identity are verified and required metadata refresh and watcher startup succeed. A failed Retry MUST leave AgentDeck offline without replaying queued mutations.

#### Scenario: Original Library reconnects successfully

- **GIVEN** AgentDeck is offline
- **AND** the original external Library with the expected identity becomes readable and writable
- **WHEN** the user invokes Retry
- **THEN** AgentDeck refreshes Library metadata and restarts required watchers
- **AND** AgentDeck transitions to online only after those steps succeed
- **AND** protected actions become available again

#### Scenario: Retry finds the wrong Library

- **GIVEN** AgentDeck is offline
- **AND** the configured path now contains a different Library identity
- **WHEN** the user invokes Retry
- **THEN** AgentDeck remains offline with an identity-mismatch reason
- **AND** AgentDeck does not refresh, adopt, or modify that directory

#### Scenario: Retry partially fails

- **GIVEN** the Library probe succeeds
- **AND** metadata refresh or watcher startup fails
- **WHEN** the user invokes Retry
- **THEN** AgentDeck remains offline
- **AND** the failure is reported without enabling protected actions
- **AND** no queued offline mutation is executed
