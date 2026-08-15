# personal-installation-readiness Specification

## Purpose

TBD - created by archiving change 'stabilize-personal-installation'. Update Purpose after archive.

## Requirements

### Requirement: Locked regression gates establish install readiness

The project SHALL verify personal-installation readiness from committed lockfiles before inspecting a packaged application. The gate MUST run the React and TypeScript production build, lint, locale integrity, repository Node contracts, complete locked Rust tests, and production dependency audits. Every result MUST record the command, exit status, and emitted pass count or build result without suppressing failures or changing dependency graphs first.

#### Scenario: All repository gates pass

- **GIVEN** frontend and Rust dependencies match `package-lock.json` and `src-tauri/Cargo.lock`
- **WHEN** the Phase 7 regression gate runs
- **THEN** every required command exits with status 0
- **AND** the verification evidence records each emitted count or build result

##### Example: Phase 6 starting baseline

- **GIVEN** the Rust suite starts from 894 passing tests and zero failures
- **WHEN** the Phase 7 regression gate is first recorded
- **THEN** the evidence names the new emitted test count instead of retaining an assumed count
- **AND** any count decrease requires an explicit explanation and no removed regression test

#### Scenario: A repository gate fails

- **WHEN** any required build, test, contract, or production audit exits non-zero
- **THEN** personal-installation readiness remains incomplete
- **AND** the exact command, exit status, and concise failure evidence are retained
- **AND** no test, audit, or lockfile is changed solely to hide the failure


<!-- @trace
source: stabilize-personal-installation
updated: 2026-08-15
code:
  - scripts/check-hooks-ui.mjs
  - scripts/check-personal-installation.mjs
  - scripts/check-personal-installation.test.mjs
  - scripts/check-plugins-ui.mjs
  - docs/personal-installation-verification.md
  - plan.md
  - scripts/check-no-upstream-app-updater.test.mjs
  - scripts/check-ui-command-arguments.test.mjs
  - src-tauri/src/core/config_profile_inventory.rs
  - package.json
  - README.md
  - scripts/frontend-argument-surface.mjs
-->

---
### Requirement: Packaged artifacts have fixed identity and no updater authority

The repository SHALL provide `npm run check:personal-installation` to inspect the locally generated macOS application and installer under `src-tauri/target/release/bundle`. The checker MUST verify application name `AgentDeck`, Bundle ID `io.github.yichin17.agentdeck`, version consistency with committed Tauri configuration, an executable main binary, and absence of application updater configuration, permission, dependency, endpoint, public key, release query, and installation flow. The checker MUST NOT modify the bundle or query a network service.

#### Scenario: Local AgentDeck bundle is valid

- **GIVEN** `npm run tauri:build` generated `src-tauri/target/release/bundle/macos/AgentDeck.app` and a macOS installer for the same build
- **WHEN** the personal-installation checker runs on macOS
- **THEN** it exits with status 0
- **AND** it reports `AgentDeck.app`, `io.github.yichin17.agentdeck`, the committed version, `updater=absent`, and `docs=complete`

#### Scenario: Bundle metadata is inconsistent

- **WHEN** the application or installer is missing, the Bundle ID or version differs, or the main executable is absent
- **THEN** the checker exits non-zero with `bundle_missing`, `installer_missing`, `identity_mismatch`, `version_mismatch`, or `executable_missing`
- **AND** it does not search a home directory or unrelated build root as a fallback

#### Scenario: Updater authority appears in a packaged surface

- **WHEN** an updater dependency, permission, endpoint, public key, release query, or frontend installation flow is present
- **THEN** the checker exits non-zero with `updater_surface_present`
- **AND** no packaged artifact is accepted as personal-installation ready

#### Scenario: Packaged inspection runs on another host

- **WHEN** packaged artifact inspection runs on a non-macOS host
- **THEN** the checker exits non-zero with `unsupported_host`
- **AND** repository build and contract tests remain independently runnable


<!-- @trace
source: stabilize-personal-installation
updated: 2026-08-15
code:
  - scripts/check-hooks-ui.mjs
  - scripts/check-personal-installation.mjs
  - scripts/check-personal-installation.test.mjs
  - scripts/check-plugins-ui.mjs
  - docs/personal-installation-verification.md
  - plan.md
  - scripts/check-no-upstream-app-updater.test.mjs
  - scripts/check-ui-command-arguments.test.mjs
  - src-tauri/src/core/config_profile_inventory.rs
  - package.json
  - README.md
  - scripts/frontend-argument-surface.mjs
-->

---
### Requirement: Existing data survives first launch of the packaged application

A packaged AgentDeck build MUST open existing `.skills-manager` Library content, SQLite state, repository configuration, Git backup metadata, Keychain service identity, local preference keys, and `skills-manager-cli` contract without moving, recreating, renaming, or deleting them. Schema migration MUST remain atomic and retryable. An unavailable configured external Library MUST remain offline without an empty fallback Library or persistent mutation.

#### Scenario: Existing internal data opens under the packaged app

- **GIVEN** an isolated fixture contains a pre-current SQLite schema, Library content, presets, Projects, deployments, conflicts, and backup metadata
- **WHEN** the packaged AgentDeck starts with that fixture
- **THEN** migration completes with all identities, rows, relationships, files, and backup metadata preserved
- **AND** no parallel AgentDeck-named Library, database, Keychain service, local preference namespace, or CLI binary is created

#### Scenario: Existing external Library is unavailable at first launch

- **GIVEN** an isolated fixture points to an unavailable external Library with a known Library identity
- **WHEN** the packaged AgentDeck starts
- **THEN** the application displays Library Offline for the configured Library
- **AND** it creates no fallback Library, deployment target, backup state, or deletion record
- **AND** internal application state remains available for inspection

#### Scenario: Migration verification fails

- **WHEN** the fixture violates a migration invariant
- **THEN** startup reports the migration failure without committing a partial schema
- **AND** the original schema and data remain retryable
- **AND** no Library filesystem mutation is used as a recovery fallback


<!-- @trace
source: stabilize-personal-installation
updated: 2026-08-15
code:
  - scripts/check-hooks-ui.mjs
  - scripts/check-personal-installation.mjs
  - scripts/check-personal-installation.test.mjs
  - scripts/check-plugins-ui.mjs
  - docs/personal-installation-verification.md
  - plan.md
  - scripts/check-no-upstream-app-updater.test.mjs
  - scripts/check-ui-command-arguments.test.mjs
  - src-tauri/src/core/config_profile_inventory.rs
  - package.json
  - README.md
  - scripts/frontend-argument-surface.mjs
-->

---
### Requirement: Packaged smoke preserves established workflow safety

Packaged smoke verification SHALL use isolated temporary Library, home, registered Codex and Claude Projects, and fake fixed-output Plugin CLI adapters. It MUST verify existing Skill deployment and conflict handling, Plugin preview authority, Hook preview and recovery, Config Profile preview and recovery, and Library Online and Offline presentation. The smoke MUST NOT read or mutate the operator's real Agent configuration, Library, credential store, or Projects.

#### Scenario: Temporary Projects complete primary workflows

- **GIVEN** isolated Codex and Claude Projects contain known Skill targets, Hook files, and Config Profile sources
- **WHEN** the smoke checklist performs preview, cancel, confirmed apply, external-change conflict, and restore operations
- **THEN** only the confirmed fixed temporary targets change
- **AND** cancel and stale operations produce no mutation
- **AND** recovery restores the exact prior bytes or absence

#### Scenario: Plugin smoke uses fixed adapters

- **GIVEN** fake Codex and Claude Plugin adapters return bounded known inventory
- **WHEN** preview and confirm smoke operations run
- **THEN** each confirm uses the exact reviewed token and fixed argument vector
- **AND** no real Plugin executable, marketplace cache, login state, working directory, or environment is accessed

#### Scenario: Isolation cannot be established

- **WHEN** a smoke step cannot prove its Library, home, Project, or CLI adapter is isolated
- **THEN** that step is not executed
- **AND** personal-installation readiness remains incomplete with the isolation blocker recorded


<!-- @trace
source: stabilize-personal-installation
updated: 2026-08-15
code:
  - scripts/check-hooks-ui.mjs
  - scripts/check-personal-installation.mjs
  - scripts/check-personal-installation.test.mjs
  - scripts/check-plugins-ui.mjs
  - docs/personal-installation-verification.md
  - plan.md
  - scripts/check-no-upstream-app-updater.test.mjs
  - scripts/check-ui-command-arguments.test.mjs
  - src-tauri/src/core/config_profile_inventory.rs
  - package.json
  - README.md
  - scripts/frontend-argument-surface.mjs
-->

---
### Requirement: Installation documentation and evidence are complete and non-sensitive

`README.md` SHALL document local locked-dependency build, application installation, first launch, legacy data reuse, internal and external Library locations, Library Offline recovery, Git backup and restore, and uninstall behavior. It MUST state that removing the application bundle does not remove user data by default and MUST identify data locations individually before any optional cleanup. It MUST state that this personal build has no application auto-update or public distribution, signing, notarization, or hosting guarantee. Verification evidence MUST contain only project-relative artifact paths and MUST NOT contain tokens, credentials, Keychain contents, source documents, home paths, temporary absolute paths, or user data.

#### Scenario: User follows the personal installation guide

- **WHEN** a user reads the installation section
- **THEN** the user can build and install `AgentDeck.app`, understand first-launch reuse and Library Offline behavior, use existing backup and restore, and remove the app separately from retained data
- **AND** the guide does not instruct the user to disable Gatekeeper or system security checks

#### Scenario: Required documentation is incomplete

- **WHEN** any required install, data reuse, offline, backup, restore, uninstall, or no-auto-update topic is absent
- **THEN** the checker exits non-zero with `documentation_incomplete`
- **AND** the packaged build is not recorded as personal-installation ready

#### Scenario: Verification evidence is reviewed

- **WHEN** `docs/personal-installation-verification.md` is prepared for commit
- **THEN** it contains Environment, Artifacts, Automated checks, Packaged smoke, Data compatibility, and Warnings sections
- **AND** every artifact path is relative to the project root
- **AND** no sensitive or machine-specific value is present

<!-- @trace
source: stabilize-personal-installation
updated: 2026-08-15
code:
  - scripts/check-hooks-ui.mjs
  - scripts/check-personal-installation.mjs
  - scripts/check-personal-installation.test.mjs
  - scripts/check-plugins-ui.mjs
  - docs/personal-installation-verification.md
  - plan.md
  - scripts/check-no-upstream-app-updater.test.mjs
  - scripts/check-ui-command-arguments.test.mjs
  - src-tauri/src/core/config_profile_inventory.rs
  - package.json
  - README.md
  - scripts/frontend-argument-surface.mjs
-->