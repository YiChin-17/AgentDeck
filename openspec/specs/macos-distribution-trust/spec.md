# macos-distribution-trust Specification

## Purpose

TBD - created by archiving change 'establish-macos-distribution-trust'. Update Purpose after archive.

## Requirements

### Requirement: Tagged macOS releases have one traceable AgentDeck identity

An official macOS release SHALL originate from a previously unused `v<semver>` tag whose commit is contained in the protected main branch history. The tag version MUST equal the versions in `package.json`, `src-tauri/tauri.conf.json`, every DMG filename, and every embedded application's `CFBundleShortVersionString`. Every embedded application MUST be named `AgentDeck.app` and use Bundle ID `io.github.yichin17.agentdeck`.

#### Scenario: Release identity is consistent

- **GIVEN** tag `v1.31.0` points to a commit in protected main history
- **WHEN** the release workflow validates source and bundle metadata
- **THEN** both committed versions, both architecture DMG names, and both embedded application versions equal `1.31.0`
- **AND** both applications are named `AgentDeck.app` with Bundle ID `io.github.yichin17.agentdeck`

#### Scenario: Tag or metadata is inconsistent

- **WHEN** the tag is reused, is outside protected main history, lacks the `v<semver>` shape, or differs from committed or bundled metadata
- **THEN** the workflow fails before signing
- **AND** it creates no draft or public release

##### Example: identity failures

| Input | Expected finding |
| ----- | ---------------- |
| tag `v1.31.0`, package version `1.30.0` | `tag_version_mismatch` |
| DMG `Skills_Manager_1.31.0_aarch64.dmg` | `identity_mismatch` |
| embedded Bundle ID `com.example.agentdeck` | `identity_mismatch` |


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
### Requirement: Release credentials are ephemeral and fail closed

The release workflow MUST obtain Apple signing and notarization credentials only from a protected GitHub Environment named `macos-release`. Build jobs MUST use an ephemeral Keychain and owner-only private key file, MUST remove both in an always-running cleanup step, and MUST NOT place credential values in repository files, workflow artifacts, caches, logs, release notes, or checksums. A missing, invalid, or mismatched credential MUST stop the build without ad-hoc signing fallback.

#### Scenario: Protected credentials are available

- **GIVEN** the `macos-release` Environment supplies the documented Apple secrets and expected TeamIdentifier
- **WHEN** either macOS architecture job begins
- **THEN** it imports credentials into runner-local temporary storage
- **AND** only that job can access the credentials
- **AND** cleanup removes the temporary Keychain and private key file after success, failure, or cancellation

#### Scenario: A credential is missing or invalid

- **WHEN** a required secret, expected TeamIdentifier, certificate payload, or private key is missing or invalid
- **THEN** the job exits before producing a release-ready artifact
- **AND** the output identifies only the missing or invalid field name
- **AND** no credential value or ad-hoc signed artifact is emitted


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
### Requirement: Every distributed application is signed, notarized, stapled, and Gatekeeper-approved

Each arm64 and x86_64 release job MUST verify the built AgentDeck application and the unique AgentDeck application mounted read-only from its DMG. Each application MUST pass strict Developer ID Application signature verification, expected TeamIdentifier matching, secure timestamp and hardened runtime checks, notarization ticket validation, and Gatekeeper execution assessment. The DMG MUST carry a valid stapled notarization ticket before it is eligible for publication.

#### Scenario: Both architecture artifacts pass trust verification

- **WHEN** the arm64 and x86_64 jobs finish notarization
- **THEN** each built application and each DMG-contained application passes identity, signature, TeamIdentifier, timestamp, hardened runtime, stapler, and Gatekeeper checks
- **AND** each DMG passes stapler validation
- **AND** the jobs upload only the verified DMGs and their release metadata to the final publication job

#### Scenario: Apple trust verification fails

- **WHEN** signing uses another team, notarization is rejected or times out, a ticket is absent, the DMG contains zero or multiple applications, or Gatekeeper rejects an application
- **THEN** the affected job fails
- **AND** no artifact from that job is eligible for publication
- **AND** no other architecture can cause a public release by itself


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
### Requirement: Publication is staged, complete, and checksum-verifiable

Build and verification jobs SHALL have read-only repository permissions and MUST NOT create or modify a GitHub Release. A final job that depends on all architecture and regression gates SHALL be the only job with `contents: write`. It MUST stage one arm64 DMG, one x86_64 DMG, and one SHA-256 file for each DMG in a non-public draft, verify the exact asset set and digests, and only then publish that same draft. The workflow MUST NOT publish `latest.json`, `.sig`, `.app.tar.gz`, or another application updater artifact.

#### Scenario: Complete draft becomes public

- **GIVEN** both architecture jobs and every required regression gate succeeded for one commit
- **WHEN** the publication job creates the release draft
- **THEN** the draft contains exactly two AgentDeck DMGs and their two SHA-256 files
- **AND** each checksum line contains 64 lowercase hexadecimal characters, two spaces, and the corresponding DMG basename
- **AND** authenticated draft verification succeeds before the release becomes public

#### Scenario: Asset or gate is incomplete

- **WHEN** a job fails, an architecture or checksum is missing, a digest differs, an asset is duplicated, an updater artifact appears, or the draft refers to another commit
- **THEN** publication fails
- **AND** the release remains absent or non-public
- **AND** the workflow does not overwrite an existing tag, release, or asset


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
### Requirement: Repository checks enforce the distribution contract without live credentials

The repository SHALL provide `npm run check:macos-distribution` using only the Node.js standard library. The checker MUST validate committed release workflow identity, version gates, permissions, protected Environment use, secret boundaries, updater artifact absence, Apple verification gates, checksum generation, publication dependencies, and distribution documentation without querying GitHub, Apple, the network, the operator Keychain, or environment secret values.

#### Scenario: Committed distribution contract is valid

- **WHEN** the repository checker runs against the committed tree
- **THEN** it exits with status 0
- **AND** it prints `macOS distribution contract passed: product=AgentDeck targets=arm64,x86_64 updater=absent publish=staged`

#### Scenario: A workflow regression is introduced

- **WHEN** a fixture introduces a legacy product name, legacy bundle path, broad release authority, missing Environment, secret exposure, updater artifact, missing trust gate, missing checksum, or publication before all dependencies
- **THEN** the checker exits non-zero with the corresponding stable finding
- **AND** the finding names a project-relative file and rule without including a secret value


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
### Requirement: Users can distinguish personal and official trust channels

Personal build documentation MUST continue to state that a locally generated bundle has no inherited signing, notarization, hosting, or application update guarantee. Official distribution documentation SHALL identify the exact GitHub release, tag, architecture-specific AgentDeck DMG, SHA-256 verification process, Developer ID and notarization expectations, Gatekeeper behavior, and withdrawal procedure. Neither channel MUST instruct users to disable Gatekeeper or another system security check.

#### Scenario: User verifies an official download

- **WHEN** a user follows the official macOS distribution guide
- **THEN** the user can select the correct architecture DMG, recompute its SHA-256 digest, compare it with the published checksum, and confirm Gatekeeper acceptance
- **AND** the guide identifies the hosted artifact as an official AgentDeck release rather than an upstream Skills Manager release

#### Scenario: User builds locally

- **WHEN** a user follows the personal installation guide
- **THEN** the guide does not claim that the local artifact inherited the official release signature or notarization
- **AND** the personal checker remains independently runnable without release credentials or network access


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
### Requirement: Personal-use scope keeps live distribution inactive

While AgentDeck is maintained only for the owner's personal use and has no external recipients, maintainers MUST NOT configure release credentials solely to complete this change, push an acceptance tag, run the live signing and notarization workflow, or create a draft or public GitHub Release. The retained release workflow, checker, fixtures, and draft distribution documentation SHALL remain inactive implementation material and MUST NOT be treated as a current release channel. A future decision to distribute AgentDeck outside the owner's devices MUST be authorized by a new Spectra change and MUST complete live release acceptance before any artifact becomes public.

#### Scenario: Current change closes without a live release

- **GIVEN** the owner confirms that AgentDeck remains personal-only with no external recipients
- **WHEN** maintainers assess completion of the distribution-trust change
- **THEN** they do not configure the `macos-release` Environment, push an acceptance tag, or create a GitHub Release
- **AND** the retained workflow and static checks do not establish a current public distribution channel

#### Scenario: Distribution is requested in the future

- **WHEN** the owner decides to provide AgentDeck to another person or publish a downloadable artifact
- **THEN** maintainers create a new Spectra change for the then-current distribution requirements
- **AND** no artifact becomes public until that change's live signing, notarization, checksum, and Gatekeeper acceptance passes

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