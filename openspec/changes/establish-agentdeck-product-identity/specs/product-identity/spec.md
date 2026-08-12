## ADDED Requirements

### Requirement: User-facing desktop identity is AgentDeck

The desktop application SHALL use `AgentDeck` as its product name in bundle metadata, the main window, application menu, tray menu, HTML title, Settings version and diagnostics content, App-owned locale text, and the primary repository overview. These surfaces MUST NOT present `Skills Manager` as the current product name.

#### Scenario: User launches the desktop application

- **WHEN** the user launches a packaged AgentDeck build
- **THEN** the Dock, main window, application menu, and tray menu identify the App as `AgentDeck`
- **AND** no current-product label on those surfaces identifies it as `Skills Manager`

#### Scenario: User inspects Settings and diagnostics

- **WHEN** the user opens Settings and prepares diagnostic content
- **THEN** the version and diagnostic headings identify the App as `AgentDeck`
- **AND** operational instructions use the AgentDeck product name except where an external integration requires its actual legacy name

### Requirement: Desktop bundle identity is stable

The AgentDeck desktop bundle MUST use `io.github.yichin17.agentdeck` as its identifier in development and production configuration. The npm package and Cargo desktop package/default binary SHALL use `agentdeck`, while the existing `skills-manager-cli` binary contract MUST remain available.

#### Scenario: Maintainer inspects build metadata

- **WHEN** a maintainer resolves the Tauri, npm, and Cargo build metadata
- **THEN** the Bundle ID is exactly `io.github.yichin17.agentdeck`
- **AND** the desktop package name is `agentdeck`
- **AND** `skills-manager-cli` remains an explicit runnable binary

#### Scenario: A later version is built

- **WHEN** the AgentDeck version changes after this identity migration
- **THEN** the Bundle ID remains `io.github.yichin17.agentdeck`
- **AND** the version is not embedded into the Bundle ID

### Requirement: Bundle identity migration preserves core data

Changing the Bundle ID MUST NOT move, delete, recreate, or replace the configured Library, SQLite state, central repository configuration, Git backup metadata, or Keychain service. AgentDeck MUST preserve external Library offline behavior and MUST NOT create an empty fallback Library when the configured Library is unavailable.

#### Scenario: Existing internal Library starts under the new bundle

- **GIVEN** an existing Library and SQLite state created by the previous bundle identity
- **WHEN** AgentDeck starts with `io.github.yichin17.agentdeck`
- **THEN** it opens the same Library and SQLite state
- **AND** no replacement Library or database is created

#### Scenario: Existing external Library is offline during first launch

- **GIVEN** an external Library configured by the previous bundle identity is unavailable
- **WHEN** AgentDeck first starts with the new Bundle ID
- **THEN** AgentDeck reports Library Offline for the same configured path
- **AND** AgentDeck does not create that path, switch to a default Library, or mutate deployment and backup state

#### Scenario: Legacy credential remains available

- **GIVEN** a Git backup credential exists under the legacy Keychain service
- **WHEN** AgentDeck starts with the new Bundle ID and accesses backup settings
- **THEN** AgentDeck uses the unchanged Keychain service identifier
- **AND** the migration does not copy the secret into a file, Library, or Git backup

### Requirement: AgentDeck uses independent desktop icon assets

AgentDeck SHALL use an AgentDeck-owned master icon that is square, at least 1024 by 1024 pixels, contains no text, depicts the approved layered Artifact-card deck concept, and is not identical to the upstream icon. Required macOS, Windows, and generic desktop icon assets MUST be generated from that master. The macOS tray icon MUST use a monochrome transparent asset in template mode.

#### Scenario: Desktop icon set is generated

- **WHEN** a maintainer generates icons from the approved AgentDeck master
- **THEN** required PNG, `.icns`, `.ico`, and Windows Square assets exist and are non-empty
- **AND** the product identity check confirms the master differs from the recorded upstream master hash

#### Scenario: Icons are reviewed at desktop sizes

- **WHEN** the App icon is rendered at 16, 32, 128, and macOS Dock sizes
- **THEN** the layered deck silhouette remains recognizable
- **AND** no text or upstream icon artwork appears

#### Scenario: Tray icon follows macOS appearance

- **WHEN** the tray icon is displayed in macOS light and dark menu bar appearances
- **THEN** the system template rendering maintains visible contrast in both appearances
- **AND** the tray silhouette remains recognizable at 16 and 32 pixels

### Requirement: Legacy compatibility identifiers remain unchanged

AgentDeck MUST preserve existing `.skills-manager` storage and backup paths, `skills-manager.db`, `refs/skills-manager/*`, `Skills-Manager-*` Git trailers, the `skills-manager-git-backup` Keychain service, existing localStorage keys, and the `skills-manager-cli` command contract. The identity migration MUST NOT globally replace `skills-manager` strings.

#### Scenario: Existing backup repository is opened

- **GIVEN** a backup contains `.skills-manager` metadata and `Skills-Manager-*` trailers
- **WHEN** AgentDeck reads or updates that backup after the identity migration
- **THEN** it uses the existing protocol identifiers
- **AND** it does not create a parallel `.agentdeck` protocol tree or ref namespace

#### Scenario: Existing CLI automation runs

- **GIVEN** automation invokes `skills-manager-cli` with the existing arguments
- **WHEN** the identity-migrated repository builds and runs the CLI
- **THEN** the command remains available with the existing argument and JSON contracts

#### Scenario: Existing local preference is read in the same container

- **GIVEN** a supported preference is stored under an existing `skills-manager:*` localStorage key
- **WHEN** AgentDeck runs in a container where that storage remains available
- **THEN** AgentDeck reads the existing key
- **AND** it does not require a duplicate `agentdeck:*` key

### Requirement: Upstream and external integration names remain explicit exceptions

AgentDeck MUST preserve upstream attribution, the retained MIT license, baseline provenance, historical changelogs, the actual GitHub OAuth App name required for revocation instructions, and the legacy Skill CLI name. These exceptions MUST NOT authorize `Skills Manager` as the general user-facing AgentDeck product name.

#### Scenario: Repository identity is reviewed

- **WHEN** a maintainer reads the primary README and baseline documents
- **THEN** the primary product is identified as AgentDeck
- **AND** the upstream Skills Manager source and retained license remain identifiable

#### Scenario: User revokes a legacy OAuth authorization

- **GIVEN** GitHub displays the connected OAuth App under its existing external name
- **WHEN** AgentDeck tells the user how to revoke that authorization
- **THEN** the instruction uses GitHub's actual displayed name
- **AND** the surrounding UI still identifies the desktop product as AgentDeck

### Requirement: Repository checks enforce product identity boundaries

The repository MUST provide a repeatable check that validates the AgentDeck display name, exact Bundle ID, desktop package name, independent icon master, required icon outputs, and the explicit legacy allowlist. The check MUST fail with the affected file and identity rule when a checked surface regresses.

#### Scenario: Upstream display name returns to a checked surface

- **GIVEN** `Skills Manager` is introduced as the product label in a checked window, menu, locale, metadata, or primary README surface
- **WHEN** the product identity check runs
- **THEN** the command exits with a non-zero status
- **AND** the output identifies the affected file and display-name rule

#### Scenario: Bundle metadata drifts

- **GIVEN** the Bundle ID differs from `io.github.yichin17.agentdeck` or the desktop package differs from `agentdeck`
- **WHEN** the product identity check runs
- **THEN** the command exits with a non-zero status
- **AND** the output identifies the incorrect metadata field

#### Scenario: Only approved legacy identifiers remain

- **GIVEN** every `skills-manager` occurrence in checked files belongs to the explicit storage, protocol, CLI, OAuth, historical, or attribution allowlist
- **WHEN** the product identity check runs
- **THEN** the command exits successfully
- **AND** no legacy data identifier is rewritten
