# pull-request-validation Specification

## Purpose

TBD - created by archiving change 'add-frontend-pr-validation'. Update Purpose after archive.

## Requirements

### Requirement: Pull requests trigger repository validation without a restrictive path filter

The Test workflow SHALL run for every pull request rather than limiting execution to Rust paths. A frontend-only, locale-only, package metadata, workflow, or repository contract change MUST enter the same pull-request validation workflow.

#### Scenario: Frontend-only pull request is opened

- **GIVEN** a pull request changes a file under `src/` and no file under `src-tauri/`
- **WHEN** GitHub evaluates workflow triggers
- **THEN** the Test workflow is triggered

#### Scenario: Repository contract pull request is opened

- **GIVEN** a pull request changes `package.json` or a file under `scripts/`
- **WHEN** GitHub evaluates workflow triggers
- **THEN** the Test workflow is triggered

#### Scenario: Workflow-only pull request is opened

- **GIVEN** a pull request changes `.github/workflows/test.yml`
- **WHEN** GitHub evaluates workflow triggers
- **THEN** the Test workflow is triggered


<!-- @trace
source: add-frontend-pr-validation
updated: 2026-08-17
code:
  - src-tauri/src/core/skillssh_api.rs
  - scripts/check-product-identity.mjs
  - plan.md
  - src-tauri/src/core/github_api.rs
  - src/views/Settings.tsx
  - scripts/check-pull-request-validation.test.mjs
  - scripts/check-product-identity.test.mjs
  - package.json
  - .github/workflows/test.yml
  - src-tauri/src/core/git_fetcher.rs
  - scripts/check-pull-request-validation.mjs
-->

---
### Requirement: Pull-request Node validation uses locked repeatable gates

The Test workflow MUST install Node dependencies with `npm ci` and MUST run the React and TypeScript production build, ESLint, locale integrity, and all committed `scripts/*.test.mjs` repository contracts. Each command MUST be a blocking step without failure suppression.

#### Scenario: All Node gates pass

- **WHEN** `npm ci`, `npm run build`, `npm run lint`, `npm run check:i18n`, and `node --test scripts/*.test.mjs` each exit with status 0
- **THEN** the Node validation job succeeds

#### Scenario: A Node gate fails

- **WHEN** any required Node validation command exits with a non-zero status
- **THEN** the Node validation job fails
- **AND** later success does not replace or suppress that failure


<!-- @trace
source: add-frontend-pr-validation
updated: 2026-08-17
code:
  - src-tauri/src/core/skillssh_api.rs
  - scripts/check-product-identity.mjs
  - plan.md
  - src-tauri/src/core/github_api.rs
  - src/views/Settings.tsx
  - scripts/check-pull-request-validation.test.mjs
  - scripts/check-product-identity.test.mjs
  - package.json
  - .github/workflows/test.yml
  - src-tauri/src/core/git_fetcher.rs
  - scripts/check-pull-request-validation.mjs
-->

---
### Requirement: Existing cross-platform Rust pull-request coverage remains available

The Test workflow MUST retain Rust tests on macOS and Windows and a Rust compile check on Linux while adding Node validation.

#### Scenario: Rust-only pull request is opened

- **GIVEN** a pull request changes a file under `src-tauri/`
- **WHEN** the Test workflow runs
- **THEN** macOS and Windows Rust test jobs run
- **AND** the Linux Rust check job runs


<!-- @trace
source: add-frontend-pr-validation
updated: 2026-08-17
code:
  - src-tauri/src/core/skillssh_api.rs
  - scripts/check-product-identity.mjs
  - plan.md
  - src-tauri/src/core/github_api.rs
  - src/views/Settings.tsx
  - scripts/check-pull-request-validation.test.mjs
  - scripts/check-product-identity.test.mjs
  - package.json
  - .github/workflows/test.yml
  - src-tauri/src/core/git_fetcher.rs
  - scripts/check-pull-request-validation.mjs
-->

---
### Requirement: Repository contract detects pull-request workflow drift

The repository SHALL expose a repeatable pull-request validation checker that fails when the unrestricted pull-request trigger, any required Node command, or existing Rust platform coverage is removed.

#### Scenario: Required PR command is removed

- **GIVEN** a workflow fixture omits `npm run check:i18n`
- **WHEN** the pull-request validation checker runs
- **THEN** it exits with a non-zero status
- **AND** the output identifies the missing-command rule and `.github/workflows/test.yml`

#### Scenario: Committed workflow satisfies the contract

- **WHEN** the pull-request validation checker evaluates the committed workflow and package script
- **THEN** it exits with status 0

<!-- @trace
source: add-frontend-pr-validation
updated: 2026-08-17
code:
  - src-tauri/src/core/skillssh_api.rs
  - scripts/check-product-identity.mjs
  - plan.md
  - src-tauri/src/core/github_api.rs
  - src/views/Settings.tsx
  - scripts/check-pull-request-validation.test.mjs
  - scripts/check-product-identity.test.mjs
  - package.json
  - .github/workflows/test.yml
  - src-tauri/src/core/git_fetcher.rs
  - scripts/check-pull-request-validation.mjs
-->