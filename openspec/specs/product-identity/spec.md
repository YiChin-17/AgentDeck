# product-identity Specification

## Purpose

TBD - created by archiving change 'establish-agentdeck-product-identity'. Update Purpose after archive.

## Requirements

### Requirement: User-facing desktop identity is AgentDeck

The desktop application and official distribution surfaces SHALL use `AgentDeck` as the current product name. This includes bundle metadata, the main window, application menu, tray menu, HTML title, Settings version and diagnostics content, App-owned locale text, the primary repository overview, release workflow labels, release titles, release notes, DMG filenames, checksum filenames, and hosted release assets. These surfaces MUST NOT present `Skills Manager` as the current product or official release name.

#### Scenario: User launches the desktop application

- **WHEN** the user launches a packaged AgentDeck build
- **THEN** the Dock, main window, application menu, and tray menu identify the App as `AgentDeck`
- **AND** no current-product label on those surfaces identifies it as `Skills Manager`

#### Scenario: User inspects Settings and diagnostics

- **WHEN** the user opens Settings and prepares diagnostic content
- **THEN** the version and diagnostic headings identify the App as `AgentDeck`
- **AND** operational instructions use the AgentDeck product name except where an external integration requires its actual legacy name

#### Scenario: User views an official release

- **WHEN** a user views or downloads a tagged official release
- **THEN** the workflow, release title, notes, DMG, checksum, and embedded application identify the product as `AgentDeck`
- **AND** `Skills Manager` appears only in explicit upstream attribution or a preserved external compatibility contract


<!-- @trace
source: establish-macos-distribution-trust
updated: 2026-08-16
code:
  - .github/workflows/release.yml
  - scripts/prepare-release.test.mjs
  - scripts/check-no-upstream-app-updater.test.mjs
  - README.md
  - .spectra.yaml
  - plan.md
  - docs/macos-distribution.md
  - scripts/check-macos-distribution.mjs
  - scripts/prepare-release.mjs
  - package.json
  - scripts/check-personal-installation.mjs
  - scripts/check-macos-distribution.test.mjs
  - .github/workflows/prepare-release.yml
  - scripts/check-personal-installation.test.mjs
  - scripts/check-no-upstream-app-updater.mjs
-->

---
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


<!-- @trace
source: establish-agentdeck-product-identity
updated: 2026-08-12
code:
  - src-tauri/icons/tray/tray-icon-32.png
  - src-tauri/capabilities/default.json
  - src-tauri/icons/icon.ico
  - src-tauri/icons/tray/tray-icon-source.png
  - src/lib/tauri.ts
  - scripts/product-identity-metadata.test.mjs
  - scripts/check-product-identity.test.mjs
  - src-tauri/icons/StoreLogo.png
  - src-tauri/icons/tray/tray-icon-24.png
  - src-tauri/icons/Square71x71Logo.png
  - src-tauri/icons/tray/tray-icon-color-16.png
  - src-tauri/icons/Square310x310Logo.png
  - src-tauri/icons/Square284x284Logo.png
  - src/i18n/en.json
  - scripts/build_small_icon.py
  - scripts/check-legacy-compatibility.test.mjs
  - src-tauri/src/commands/skills.rs
  - src-tauri/icons/Square30x30Logo.png
  - src-tauri/icons/icon-source-small.png
  - public/icons/32x32.png
  - src-tauri/icons/Square142x142Logo.png
  - src-tauri/icons/Square89x89Logo.png
  - src-tauri/icons/Square44x44Logo.png
  - src-tauri/icons/tray/tray-icon-color-20.png
  - src-tauri/icons/tray/tray-icon-color-24.png
  - src-tauri/icons/tray/tray-icon-color-32.png
  - src-tauri/icons/icon.icns
  - src-tauri/src/commands/git_backup.rs
  - src-tauri/src/commands/settings.rs
  - assets/icon.png
  - src-tauri/icons/tray/tray-icon-16.png
  - src-tauri/src/commands/tools.rs
  - src-tauri/src/core/app_state.rs
  - src/components/Sidebar.tsx
  - index.html
  - src/i18n/zh-TW.json
  - src-tauri/icons/64x64.png
  - src-tauri/icons/32x32.png
  - src-tauri/icons/128x128.png
  - scripts/build_tray_icons.py
  - src/context/AppContext.tsx
  - scripts/check-no-upstream-app-updater.mjs
  - src-tauri/src/core/sync_metadata.rs
  - scripts/check-no-upstream-app-updater.test.mjs
  - src-tauri/src/commands/scan.rs
  - src-tauri/icons/tray/tray-icon-20.png
  - src-tauri/src/core/git_backup.rs
  - scripts/product-identity-icon.test.mjs
  - scripts/check-product-identity.mjs
  - src-tauri/Cargo.toml
  - plan.md
  - src-tauri/tauri.conf.json
  - src-tauri/src/core/git_credentials.rs
  - src-tauri/icons/icon.png
  - scripts/product-identity-display.test.mjs
  - README.md
  - src-tauri/src/core/central_repo.rs
  - src-tauri/src/lib.rs
  - package.json
  - src-tauri/icons/128x128@2x.png
  - src-tauri/src/core/library_availability.rs
  - src-tauri/icons/icon-source.png
  - src-tauri/icons/Square107x107Logo.png
  - src-tauri/src/commands/presets.rs
  - src/views/Backup.tsx
  - src/views/Settings.tsx
  - scripts/build_macos_icon.py
  - src-tauri/icons/Square150x150Logo.png
-->

---
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


<!-- @trace
source: establish-agentdeck-product-identity
updated: 2026-08-12
code:
  - src-tauri/icons/tray/tray-icon-32.png
  - src-tauri/capabilities/default.json
  - src-tauri/icons/icon.ico
  - src-tauri/icons/tray/tray-icon-source.png
  - src/lib/tauri.ts
  - scripts/product-identity-metadata.test.mjs
  - scripts/check-product-identity.test.mjs
  - src-tauri/icons/StoreLogo.png
  - src-tauri/icons/tray/tray-icon-24.png
  - src-tauri/icons/Square71x71Logo.png
  - src-tauri/icons/tray/tray-icon-color-16.png
  - src-tauri/icons/Square310x310Logo.png
  - src-tauri/icons/Square284x284Logo.png
  - src/i18n/en.json
  - scripts/build_small_icon.py
  - scripts/check-legacy-compatibility.test.mjs
  - src-tauri/src/commands/skills.rs
  - src-tauri/icons/Square30x30Logo.png
  - src-tauri/icons/icon-source-small.png
  - public/icons/32x32.png
  - src-tauri/icons/Square142x142Logo.png
  - src-tauri/icons/Square89x89Logo.png
  - src-tauri/icons/Square44x44Logo.png
  - src-tauri/icons/tray/tray-icon-color-20.png
  - src-tauri/icons/tray/tray-icon-color-24.png
  - src-tauri/icons/tray/tray-icon-color-32.png
  - src-tauri/icons/icon.icns
  - src-tauri/src/commands/git_backup.rs
  - src-tauri/src/commands/settings.rs
  - assets/icon.png
  - src-tauri/icons/tray/tray-icon-16.png
  - src-tauri/src/commands/tools.rs
  - src-tauri/src/core/app_state.rs
  - src/components/Sidebar.tsx
  - index.html
  - src/i18n/zh-TW.json
  - src-tauri/icons/64x64.png
  - src-tauri/icons/32x32.png
  - src-tauri/icons/128x128.png
  - scripts/build_tray_icons.py
  - src/context/AppContext.tsx
  - scripts/check-no-upstream-app-updater.mjs
  - src-tauri/src/core/sync_metadata.rs
  - scripts/check-no-upstream-app-updater.test.mjs
  - src-tauri/src/commands/scan.rs
  - src-tauri/icons/tray/tray-icon-20.png
  - src-tauri/src/core/git_backup.rs
  - scripts/product-identity-icon.test.mjs
  - scripts/check-product-identity.mjs
  - src-tauri/Cargo.toml
  - plan.md
  - src-tauri/tauri.conf.json
  - src-tauri/src/core/git_credentials.rs
  - src-tauri/icons/icon.png
  - scripts/product-identity-display.test.mjs
  - README.md
  - src-tauri/src/core/central_repo.rs
  - src-tauri/src/lib.rs
  - package.json
  - src-tauri/icons/128x128@2x.png
  - src-tauri/src/core/library_availability.rs
  - src-tauri/icons/icon-source.png
  - src-tauri/icons/Square107x107Logo.png
  - src-tauri/src/commands/presets.rs
  - src/views/Backup.tsx
  - src/views/Settings.tsx
  - scripts/build_macos_icon.py
  - src-tauri/icons/Square150x150Logo.png
-->

---
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


<!-- @trace
source: establish-agentdeck-product-identity
updated: 2026-08-12
code:
  - src-tauri/icons/tray/tray-icon-32.png
  - src-tauri/capabilities/default.json
  - src-tauri/icons/icon.ico
  - src-tauri/icons/tray/tray-icon-source.png
  - src/lib/tauri.ts
  - scripts/product-identity-metadata.test.mjs
  - scripts/check-product-identity.test.mjs
  - src-tauri/icons/StoreLogo.png
  - src-tauri/icons/tray/tray-icon-24.png
  - src-tauri/icons/Square71x71Logo.png
  - src-tauri/icons/tray/tray-icon-color-16.png
  - src-tauri/icons/Square310x310Logo.png
  - src-tauri/icons/Square284x284Logo.png
  - src/i18n/en.json
  - scripts/build_small_icon.py
  - scripts/check-legacy-compatibility.test.mjs
  - src-tauri/src/commands/skills.rs
  - src-tauri/icons/Square30x30Logo.png
  - src-tauri/icons/icon-source-small.png
  - public/icons/32x32.png
  - src-tauri/icons/Square142x142Logo.png
  - src-tauri/icons/Square89x89Logo.png
  - src-tauri/icons/Square44x44Logo.png
  - src-tauri/icons/tray/tray-icon-color-20.png
  - src-tauri/icons/tray/tray-icon-color-24.png
  - src-tauri/icons/tray/tray-icon-color-32.png
  - src-tauri/icons/icon.icns
  - src-tauri/src/commands/git_backup.rs
  - src-tauri/src/commands/settings.rs
  - assets/icon.png
  - src-tauri/icons/tray/tray-icon-16.png
  - src-tauri/src/commands/tools.rs
  - src-tauri/src/core/app_state.rs
  - src/components/Sidebar.tsx
  - index.html
  - src/i18n/zh-TW.json
  - src-tauri/icons/64x64.png
  - src-tauri/icons/32x32.png
  - src-tauri/icons/128x128.png
  - scripts/build_tray_icons.py
  - src/context/AppContext.tsx
  - scripts/check-no-upstream-app-updater.mjs
  - src-tauri/src/core/sync_metadata.rs
  - scripts/check-no-upstream-app-updater.test.mjs
  - src-tauri/src/commands/scan.rs
  - src-tauri/icons/tray/tray-icon-20.png
  - src-tauri/src/core/git_backup.rs
  - scripts/product-identity-icon.test.mjs
  - scripts/check-product-identity.mjs
  - src-tauri/Cargo.toml
  - plan.md
  - src-tauri/tauri.conf.json
  - src-tauri/src/core/git_credentials.rs
  - src-tauri/icons/icon.png
  - scripts/product-identity-display.test.mjs
  - README.md
  - src-tauri/src/core/central_repo.rs
  - src-tauri/src/lib.rs
  - package.json
  - src-tauri/icons/128x128@2x.png
  - src-tauri/src/core/library_availability.rs
  - src-tauri/icons/icon-source.png
  - src-tauri/icons/Square107x107Logo.png
  - src-tauri/src/commands/presets.rs
  - src/views/Backup.tsx
  - src/views/Settings.tsx
  - scripts/build_macos_icon.py
  - src-tauri/icons/Square150x150Logo.png
-->

---
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


<!-- @trace
source: establish-agentdeck-product-identity
updated: 2026-08-12
code:
  - src-tauri/icons/tray/tray-icon-32.png
  - src-tauri/capabilities/default.json
  - src-tauri/icons/icon.ico
  - src-tauri/icons/tray/tray-icon-source.png
  - src/lib/tauri.ts
  - scripts/product-identity-metadata.test.mjs
  - scripts/check-product-identity.test.mjs
  - src-tauri/icons/StoreLogo.png
  - src-tauri/icons/tray/tray-icon-24.png
  - src-tauri/icons/Square71x71Logo.png
  - src-tauri/icons/tray/tray-icon-color-16.png
  - src-tauri/icons/Square310x310Logo.png
  - src-tauri/icons/Square284x284Logo.png
  - src/i18n/en.json
  - scripts/build_small_icon.py
  - scripts/check-legacy-compatibility.test.mjs
  - src-tauri/src/commands/skills.rs
  - src-tauri/icons/Square30x30Logo.png
  - src-tauri/icons/icon-source-small.png
  - public/icons/32x32.png
  - src-tauri/icons/Square142x142Logo.png
  - src-tauri/icons/Square89x89Logo.png
  - src-tauri/icons/Square44x44Logo.png
  - src-tauri/icons/tray/tray-icon-color-20.png
  - src-tauri/icons/tray/tray-icon-color-24.png
  - src-tauri/icons/tray/tray-icon-color-32.png
  - src-tauri/icons/icon.icns
  - src-tauri/src/commands/git_backup.rs
  - src-tauri/src/commands/settings.rs
  - assets/icon.png
  - src-tauri/icons/tray/tray-icon-16.png
  - src-tauri/src/commands/tools.rs
  - src-tauri/src/core/app_state.rs
  - src/components/Sidebar.tsx
  - index.html
  - src/i18n/zh-TW.json
  - src-tauri/icons/64x64.png
  - src-tauri/icons/32x32.png
  - src-tauri/icons/128x128.png
  - scripts/build_tray_icons.py
  - src/context/AppContext.tsx
  - scripts/check-no-upstream-app-updater.mjs
  - src-tauri/src/core/sync_metadata.rs
  - scripts/check-no-upstream-app-updater.test.mjs
  - src-tauri/src/commands/scan.rs
  - src-tauri/icons/tray/tray-icon-20.png
  - src-tauri/src/core/git_backup.rs
  - scripts/product-identity-icon.test.mjs
  - scripts/check-product-identity.mjs
  - src-tauri/Cargo.toml
  - plan.md
  - src-tauri/tauri.conf.json
  - src-tauri/src/core/git_credentials.rs
  - src-tauri/icons/icon.png
  - scripts/product-identity-display.test.mjs
  - README.md
  - src-tauri/src/core/central_repo.rs
  - src-tauri/src/lib.rs
  - package.json
  - src-tauri/icons/128x128@2x.png
  - src-tauri/src/core/library_availability.rs
  - src-tauri/icons/icon-source.png
  - src-tauri/icons/Square107x107Logo.png
  - src-tauri/src/commands/presets.rs
  - src/views/Backup.tsx
  - src/views/Settings.tsx
  - scripts/build_macos_icon.py
  - src-tauri/icons/Square150x150Logo.png
-->

---
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


<!-- @trace
source: establish-agentdeck-product-identity
updated: 2026-08-12
code:
  - src-tauri/icons/tray/tray-icon-32.png
  - src-tauri/capabilities/default.json
  - src-tauri/icons/icon.ico
  - src-tauri/icons/tray/tray-icon-source.png
  - src/lib/tauri.ts
  - scripts/product-identity-metadata.test.mjs
  - scripts/check-product-identity.test.mjs
  - src-tauri/icons/StoreLogo.png
  - src-tauri/icons/tray/tray-icon-24.png
  - src-tauri/icons/Square71x71Logo.png
  - src-tauri/icons/tray/tray-icon-color-16.png
  - src-tauri/icons/Square310x310Logo.png
  - src-tauri/icons/Square284x284Logo.png
  - src/i18n/en.json
  - scripts/build_small_icon.py
  - scripts/check-legacy-compatibility.test.mjs
  - src-tauri/src/commands/skills.rs
  - src-tauri/icons/Square30x30Logo.png
  - src-tauri/icons/icon-source-small.png
  - public/icons/32x32.png
  - src-tauri/icons/Square142x142Logo.png
  - src-tauri/icons/Square89x89Logo.png
  - src-tauri/icons/Square44x44Logo.png
  - src-tauri/icons/tray/tray-icon-color-20.png
  - src-tauri/icons/tray/tray-icon-color-24.png
  - src-tauri/icons/tray/tray-icon-color-32.png
  - src-tauri/icons/icon.icns
  - src-tauri/src/commands/git_backup.rs
  - src-tauri/src/commands/settings.rs
  - assets/icon.png
  - src-tauri/icons/tray/tray-icon-16.png
  - src-tauri/src/commands/tools.rs
  - src-tauri/src/core/app_state.rs
  - src/components/Sidebar.tsx
  - index.html
  - src/i18n/zh-TW.json
  - src-tauri/icons/64x64.png
  - src-tauri/icons/32x32.png
  - src-tauri/icons/128x128.png
  - scripts/build_tray_icons.py
  - src/context/AppContext.tsx
  - scripts/check-no-upstream-app-updater.mjs
  - src-tauri/src/core/sync_metadata.rs
  - scripts/check-no-upstream-app-updater.test.mjs
  - src-tauri/src/commands/scan.rs
  - src-tauri/icons/tray/tray-icon-20.png
  - src-tauri/src/core/git_backup.rs
  - scripts/product-identity-icon.test.mjs
  - scripts/check-product-identity.mjs
  - src-tauri/Cargo.toml
  - plan.md
  - src-tauri/tauri.conf.json
  - src-tauri/src/core/git_credentials.rs
  - src-tauri/icons/icon.png
  - scripts/product-identity-display.test.mjs
  - README.md
  - src-tauri/src/core/central_repo.rs
  - src-tauri/src/lib.rs
  - package.json
  - src-tauri/icons/128x128@2x.png
  - src-tauri/src/core/library_availability.rs
  - src-tauri/icons/icon-source.png
  - src-tauri/icons/Square107x107Logo.png
  - src-tauri/src/commands/presets.rs
  - src/views/Backup.tsx
  - src/views/Settings.tsx
  - scripts/build_macos_icon.py
  - src-tauri/icons/Square150x150Logo.png
-->

---
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

<!-- @trace
source: establish-agentdeck-product-identity
updated: 2026-08-12
code:
  - src-tauri/icons/tray/tray-icon-32.png
  - src-tauri/capabilities/default.json
  - src-tauri/icons/icon.ico
  - src-tauri/icons/tray/tray-icon-source.png
  - src/lib/tauri.ts
  - scripts/product-identity-metadata.test.mjs
  - scripts/check-product-identity.test.mjs
  - src-tauri/icons/StoreLogo.png
  - src-tauri/icons/tray/tray-icon-24.png
  - src-tauri/icons/Square71x71Logo.png
  - src-tauri/icons/tray/tray-icon-color-16.png
  - src-tauri/icons/Square310x310Logo.png
  - src-tauri/icons/Square284x284Logo.png
  - src/i18n/en.json
  - scripts/build_small_icon.py
  - scripts/check-legacy-compatibility.test.mjs
  - src-tauri/src/commands/skills.rs
  - src-tauri/icons/Square30x30Logo.png
  - src-tauri/icons/icon-source-small.png
  - public/icons/32x32.png
  - src-tauri/icons/Square142x142Logo.png
  - src-tauri/icons/Square89x89Logo.png
  - src-tauri/icons/Square44x44Logo.png
  - src-tauri/icons/tray/tray-icon-color-20.png
  - src-tauri/icons/tray/tray-icon-color-24.png
  - src-tauri/icons/tray/tray-icon-color-32.png
  - src-tauri/icons/icon.icns
  - src-tauri/src/commands/git_backup.rs
  - src-tauri/src/commands/settings.rs
  - assets/icon.png
  - src-tauri/icons/tray/tray-icon-16.png
  - src-tauri/src/commands/tools.rs
  - src-tauri/src/core/app_state.rs
  - src/components/Sidebar.tsx
  - index.html
  - src/i18n/zh-TW.json
  - src-tauri/icons/64x64.png
  - src-tauri/icons/32x32.png
  - src-tauri/icons/128x128.png
  - scripts/build_tray_icons.py
  - src/context/AppContext.tsx
  - scripts/check-no-upstream-app-updater.mjs
  - src-tauri/src/core/sync_metadata.rs
  - scripts/check-no-upstream-app-updater.test.mjs
  - src-tauri/src/commands/scan.rs
  - src-tauri/icons/tray/tray-icon-20.png
  - src-tauri/src/core/git_backup.rs
  - scripts/product-identity-icon.test.mjs
  - scripts/check-product-identity.mjs
  - src-tauri/Cargo.toml
  - plan.md
  - src-tauri/tauri.conf.json
  - src-tauri/src/core/git_credentials.rs
  - src-tauri/icons/icon.png
  - scripts/product-identity-display.test.mjs
  - README.md
  - src-tauri/src/core/central_repo.rs
  - src-tauri/src/lib.rs
  - package.json
  - src-tauri/icons/128x128@2x.png
  - src-tauri/src/core/library_availability.rs
  - src-tauri/icons/icon-source.png
  - src-tauri/icons/Square107x107Logo.png
  - src-tauri/src/commands/presets.rs
  - src/views/Backup.tsx
  - src/views/Settings.tsx
  - scripts/build_macos_icon.py
  - src-tauri/icons/Square150x150Logo.png
-->

---
### Requirement: AgentDeck-owned Settings links target the AgentDeck repository

The Settings repository action SHALL open `https://github.com/YiChin-17/AgentDeck`, and the Settings issue-report action SHALL open the bug report creation flow under that same repository. The repository product-identity check MUST reject an upstream Skills Manager destination on either AgentDeck-owned Settings action while preserving upstream URLs used for explicit attribution or provenance.

#### Scenario: User opens the project repository

- **WHEN** the user activates the GitHub repository action in Settings
- **THEN** AgentDeck opens `https://github.com/YiChin-17/AgentDeck`
- **AND** the action does not open the upstream Skills Manager repository

#### Scenario: User reports an issue

- **WHEN** the user completes the Settings report-issue action
- **THEN** AgentDeck opens `https://github.com/YiChin-17/AgentDeck/issues/new?template=bug_report.md`
- **AND** the existing diagnostic copy flow remains available

#### Scenario: Settings destination regresses to upstream

- **GIVEN** either AgentDeck-owned Settings action resolves to `https://github.com/xingkongliang/skills-manager`
- **WHEN** the product-identity check runs
- **THEN** the check exits with a non-zero status
- **AND** the finding identifies `src/views/Settings.tsx` and the repository-destination rule

#### Scenario: Upstream attribution remains present

- **GIVEN** the upstream repository URL appears only in an approved attribution or provenance surface
- **WHEN** the product-identity check runs
- **THEN** that occurrence does not violate the Settings repository-destination rule

<!-- @trace
source: point-settings-links-to-agentdeck
updated: 2026-08-17
code:
  - scripts/check-product-identity.mjs
  - src/views/Settings.tsx
  - scripts/check-product-identity.test.mjs
-->