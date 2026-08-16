# app-update-policy Specification

## Purpose

TBD - created by archiving change 'remove-upstream-app-updater'. Update Purpose after archive.

## Requirements

### Requirement: AgentDeck does not check for application binary releases

AgentDeck SHALL NOT query an upstream or fork release service to compare application binary versions during startup or from Settings. AgentDeck MUST start and expose Settings without an application update notification or application release-check control.

#### Scenario: Application starts while upstream has a newer release

- **GIVEN** upstream Skills Manager publishes a version newer than the local AgentDeck version
- **WHEN** AgentDeck starts and remains open beyond the former delayed check interval
- **THEN** AgentDeck sends no application release request
- **AND** AgentDeck displays no application binary update notification

#### Scenario: User opens Settings

- **WHEN** the user opens Settings
- **THEN** Settings contains no control that checks for an AgentDeck or Skills Manager application release
- **AND** Settings remains usable when GitHub is unavailable


<!-- @trace
source: remove-upstream-app-updater
updated: 2026-08-11
code:
  - src/i18n/en.json
  - src-tauri/src/core/git_backup.rs
  - src-tauri/src/commands/tools.rs
  - src/lib/tauri.ts
  - src-tauri/src/commands/presets.rs
  - README.md
  - src/views/Settings.tsx
  - src-tauri/src/commands/scan.rs
  - plan.md
  - src-tauri/src/commands/skills.rs
  - scripts/check-no-upstream-app-updater.mjs
  - src/components/Sidebar.tsx
  - src-tauri/src/core/app_state.rs
  - src-tauri/src/commands/settings.rs
  - src-tauri/tauri.conf.json
  - package.json
  - src-tauri/capabilities/default.json
  - src/context/AppContext.tsx
  - src-tauri/src/core/sync_metadata.rs
  - src-tauri/src/lib.rs
  - src/views/Backup.tsx
  - src-tauri/src/commands/git_backup.rs
  - scripts/check-no-upstream-app-updater.test.mjs
  - src/i18n/zh-TW.json
  - src-tauri/src/core/library_availability.rs
  - src-tauri/Cargo.toml
-->

---
### Requirement: AgentDeck cannot download or install application updates

AgentDeck SHALL NOT expose runtime commands, frontend controls, Tauri permissions, plugin registrations, endpoints, public keys, or bundle artifacts that download, verify, install, or restart to apply an application binary update.

#### Scenario: Packaged application is inspected

- **WHEN** a maintainer inspects the Tauri configuration, capabilities, runtime plugin registrations, and dependency manifests
- **THEN** no application updater endpoint or signing public key is configured
- **AND** no Tauri updater permission or plugin dependency is present
- **AND** updater artifacts are not requested from the bundle build

#### Scenario: User views all Settings actions

- **WHEN** the user inspects the available Settings actions
- **THEN** no action downloads or installs an application binary
- **AND** no action offers to restart AgentDeck to apply an application update


<!-- @trace
source: remove-upstream-app-updater
updated: 2026-08-11
code:
  - src/i18n/en.json
  - src-tauri/src/core/git_backup.rs
  - src-tauri/src/commands/tools.rs
  - src/lib/tauri.ts
  - src-tauri/src/commands/presets.rs
  - README.md
  - src/views/Settings.tsx
  - src-tauri/src/commands/scan.rs
  - plan.md
  - src-tauri/src/commands/skills.rs
  - scripts/check-no-upstream-app-updater.mjs
  - src/components/Sidebar.tsx
  - src-tauri/src/core/app_state.rs
  - src-tauri/src/commands/settings.rs
  - src-tauri/tauri.conf.json
  - package.json
  - src-tauri/capabilities/default.json
  - src/context/AppContext.tsx
  - src-tauri/src/core/sync_metadata.rs
  - src-tauri/src/lib.rs
  - src/views/Backup.tsx
  - src-tauri/src/commands/git_backup.rs
  - scripts/check-no-upstream-app-updater.test.mjs
  - src/i18n/zh-TW.json
  - src-tauri/src/core/library_availability.rs
  - src-tauri/Cargo.toml
-->

---
### Requirement: Upstream provenance remains separate from runtime update trust

AgentDeck MUST preserve the documented upstream repository, fork baseline, retained MIT license, and Git-based maintainer workflow. AgentDeck MUST NOT treat an upstream release, tag, artifact, endpoint, or signing key as an application update trusted by the running App.

#### Scenario: Maintainer reviews upstream provenance

- **WHEN** a maintainer reads the baseline and project overview documents
- **THEN** the upstream Skills Manager repository and retained license remain identifiable
- **AND** the documents do not instruct the running App to consume upstream binary releases

#### Scenario: Maintainer synchronizes upstream code

- **WHEN** a maintainer fetches or selectively integrates upstream source changes
- **THEN** the Git workflow remains available
- **AND** application binary installation is not triggered by that workflow


<!-- @trace
source: remove-upstream-app-updater
updated: 2026-08-11
code:
  - src/i18n/en.json
  - src-tauri/src/core/git_backup.rs
  - src-tauri/src/commands/tools.rs
  - src/lib/tauri.ts
  - src-tauri/src/commands/presets.rs
  - README.md
  - src/views/Settings.tsx
  - src-tauri/src/commands/scan.rs
  - plan.md
  - src-tauri/src/commands/skills.rs
  - scripts/check-no-upstream-app-updater.mjs
  - src/components/Sidebar.tsx
  - src-tauri/src/core/app_state.rs
  - src-tauri/src/commands/settings.rs
  - src-tauri/tauri.conf.json
  - package.json
  - src-tauri/capabilities/default.json
  - src/context/AppContext.tsx
  - src-tauri/src/core/sync_metadata.rs
  - src-tauri/src/lib.rs
  - src/views/Backup.tsx
  - src-tauri/src/commands/git_backup.rs
  - scripts/check-no-upstream-app-updater.test.mjs
  - src/i18n/zh-TW.json
  - src-tauri/src/core/library_availability.rs
  - src-tauri/Cargo.toml
-->

---
### Requirement: Repository checks prevent application updater regression

The repository MUST provide a repeatable check that fails when application updater configuration, permission, registration, dependency, release query, or frontend installation flow is introduced into the defined runtime and build surfaces. The check MUST permit upstream references used only for attribution, licensing, baseline documentation, or Git maintenance.

#### Scenario: Updater runtime reference is introduced

- **GIVEN** a forbidden upstream release query or Tauri updater reference is present in a checked runtime or build surface
- **WHEN** the updater regression check runs
- **THEN** the command exits with a non-zero status
- **AND** the output identifies the affected file and forbidden pattern

#### Scenario: Only attribution references remain

- **GIVEN** upstream URLs exist only in README, baseline, license, or Spectra documentation
- **WHEN** the updater regression check runs
- **THEN** the command exits successfully
- **AND** the attribution references remain unchanged


<!-- @trace
source: remove-upstream-app-updater
updated: 2026-08-11
code:
  - src/i18n/en.json
  - src-tauri/src/core/git_backup.rs
  - src-tauri/src/commands/tools.rs
  - src/lib/tauri.ts
  - src-tauri/src/commands/presets.rs
  - README.md
  - src/views/Settings.tsx
  - src-tauri/src/commands/scan.rs
  - plan.md
  - src-tauri/src/commands/skills.rs
  - scripts/check-no-upstream-app-updater.mjs
  - src/components/Sidebar.tsx
  - src-tauri/src/core/app_state.rs
  - src-tauri/src/commands/settings.rs
  - src-tauri/tauri.conf.json
  - package.json
  - src-tauri/capabilities/default.json
  - src/context/AppContext.tsx
  - src-tauri/src/core/sync_metadata.rs
  - src-tauri/src/lib.rs
  - src/views/Backup.tsx
  - src-tauri/src/commands/git_backup.rs
  - scripts/check-no-upstream-app-updater.test.mjs
  - src/i18n/zh-TW.json
  - src-tauri/src/core/library_availability.rs
  - src-tauri/Cargo.toml
-->

---
### Requirement: Personal installation is the documented release policy

The project plan and personal installation documentation MUST define AgentDeck as a local personal-installation project without application auto-update or a currently active public release channel. The documentation MUST distinguish a locally generated bundle from an upstream or publicly distributed release and MUST NOT claim current public release hosting, distribution signing, notarization, binary update trust, or an application update service. A retained release workflow, distribution checker, or official-distribution draft document MUST be treated as inactive implementation material and MUST NOT authorize public distribution, a runtime release query, an update manifest, an update public key, a binary download, an installation flow, or an application auto-update service. Any future public distribution or application update trust MUST require a separate Spectra change before it becomes a current implementation, live acceptance, or documentation requirement.

#### Scenario: Maintainer reads the completed stabilization phase

- **WHEN** a maintainer reads Phase 7 of the project plan
- **THEN** personal local build, packaged smoke, backup, and uninstall verification remain recorded as completed without public trust claims
- **AND** those acceptance results remain independently reproducible without release credentials

#### Scenario: Maintainer reads the distribution phase

- **WHEN** a maintainer reads Phase 8 of the project plan
- **THEN** the completed fail-closed workflow and repository checks are recorded as inactive implementation material
- **AND** release credential configuration, tagged acceptance, and GitHub Release publication are not current completion criteria
- **AND** application auto-update, update manifests, runtime release queries, and updater signing keys remain out of scope

#### Scenario: User reads personal installation documentation

- **WHEN** a user reads the local build, installation, or troubleshooting instructions
- **THEN** the documentation identifies the artifact as a personal local build without application auto-update or inherited official trust
- **AND** it does not instruct the user to disable Gatekeeper or system security checks

#### Scenario: User reads official distribution documentation

- **WHEN** a user encounters retained macOS distribution documentation while AgentDeck remains personal-only
- **THEN** the documentation does not claim that a current public AgentDeck release exists
- **AND** it does not claim that the running application checks, downloads, or installs releases

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