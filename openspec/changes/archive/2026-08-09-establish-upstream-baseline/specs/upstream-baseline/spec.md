## ADDED Requirements

### Requirement: Upstream provenance is recorded

The repository MUST identify `https://github.com/xingkongliang/skills-manager.git` as the upstream source, `https://github.com/YiChin-17/AgentDeck.git` as the fork origin, commit `ab2a6947062c49640b751d4c2a9d8be816347dc1` and tag `v1.30.0` as the baseline point, and the retained MIT license.

#### Scenario: A maintainer inspects the baseline

- **WHEN** a maintainer reads the tracked baseline and project overview documents
- **THEN** the maintainer can identify both repository URLs, the full baseline commit, the baseline tag, and the retained license without relying on local Git configuration

### Requirement: Baseline verification uses locked dependencies

The baseline process SHALL install frontend dependencies from `package-lock.json`, preserve `src-tauri/Cargo.lock`, and run the repository-owned React/TypeScript production build and Rust workspace tests without changing dependency manifests first.

#### Scenario: The unmodified fork baseline is verified

- **WHEN** Phase 0 baseline verification begins at the recorded baseline commit plus AgentDeck planning artifacts
- **THEN** dependency installation, the production build, and Rust tests run against the committed lockfiles before any advisory remediation is applied

### Requirement: Verification evidence is reproducible

The repository MUST record the verification date, relevant tool versions, exact commands, exit status, test pass/fail counts when emitted by the test runner, and a concise result for every required baseline check.

#### Scenario: A required check passes

- **WHEN** a baseline command exits successfully
- **THEN** the tracked baseline evidence records the command, execution context, successful exit status, and emitted pass count or build result

#### Scenario: A required check fails

- **WHEN** dependency installation, build, test, or audit exits unsuccessfully
- **THEN** the tracked baseline evidence records the command, non-zero exit status, concise error evidence, and Phase 0 remains incomplete

### Requirement: Production dependency advisories are evaluated safely

The baseline process SHALL run production dependency audits for the committed JavaScript and Rust dependency graphs. It MUST NOT suppress an audit failure, remove a test, or change application behavior to report a passing baseline.

#### Scenario: No production advisory is reported

- **WHEN** both production dependency audits complete without a production advisory
- **THEN** the baseline evidence records the clean result and no dependency manifest or lockfile is changed for remediation

#### Scenario: A compatible remediation is available

- **WHEN** an audit reports a production advisory that can be resolved without a breaking dependency upgrade or application behavior change
- **THEN** only the affected dependency manifest and lockfile are updated, and the affected install, build, tests, and audit are rerun and recorded

#### Scenario: Remediation requires expanded scope

- **WHEN** resolving a production advisory requires a breaking upgrade or application behavior change
- **THEN** the advisory remains documented and remediation is deferred to a separate Spectra change

### Requirement: Project direction and upstream compatibility are explicit

The project overview SHALL distinguish AgentDeck's product direction from the upstream Skills Manager, preserve required upstream attribution, and state that macOS is the first target while existing cross-platform behavior remains protected unless a later specification explicitly changes it.

#### Scenario: A repository visitor reads the project overview

- **WHEN** a visitor opens `README.md`
- **THEN** the visitor can identify the upstream project, AgentDeck's management scope, the macOS-first target, and the cross-platform compatibility constraint

### Requirement: Baseline artifacts exclude local and sensitive data

New baseline evidence and content added by this change MUST use project-relative paths and MUST NOT contain tokens, login information, newly recorded machine-specific absolute paths, dependency caches, generated build output, or Spectra's local database.

#### Scenario: Baseline changes are reviewed before commit

- **WHEN** the Phase 0 diff is inspected
- **THEN** additions contain only tracked documentation, Spectra artifacts, and dependency files required by a recorded compatible advisory remediation, with no local or sensitive data
