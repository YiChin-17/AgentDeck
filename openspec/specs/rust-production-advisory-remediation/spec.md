# rust-production-advisory-remediation Specification

## Purpose

TBD - created by archiving change 'resolve-rust-production-advisories'. Update Purpose after archive.

## Requirements

### Requirement: Targeted Rust production advisories are eliminated

The committed Rust dependency graph MUST contain versions that are not affected by `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`, or `RUSTSEC-2026-0235`. Remediation MUST update the owning dependency paths instead of suppressing audit findings or forcing versions outside a parent crate's declared compatibility range.

#### Scenario: The remediated lockfile is audited

- **WHEN** a maintainer runs `cargo audit --file src-tauri/Cargo.lock` against the remediated checkout
- **THEN** the audit reports zero findings for `RUSTSEC-2026-0194`, `RUSTSEC-2026-0195`, and `RUSTSEC-2026-0235`
- **THEN** no cargo-audit ignore or allowlist entry hides any of those IDs

##### Example: vulnerable versions are absent

- **GIVEN** the baseline contained `quick-xml 0.38.4`, `quick-xml 0.39.4`, and `rkyv 0.7.46`
- **WHEN** `cargo tree --manifest-path src-tauri/Cargo.toml --target all` is inspected after remediation
- **THEN** none of those three package-version pairs appears in the resolved tree


<!-- @trace
source: resolve-rust-production-advisories
updated: 2026-08-09
code:
  - BASELINE.md
  - src-tauri/Cargo.toml
  - README.md
  - plan.md
-->

---
### Requirement: Cross-platform dependency support is preserved

The remediation SHALL retain the Tauri 2 dependency paths required by macOS, Windows, and Linux. It MUST NOT remove a platform dependency, disable an existing feature, or change AgentDeck runtime behavior solely to eliminate a lockfile finding.

#### Scenario: All platform dependency paths are resolved

- **WHEN** a maintainer runs `cargo tree --manifest-path src-tauri/Cargo.toml --target all`
- **THEN** Cargo resolves the complete dependency graph successfully
- **THEN** the existing Tauri, Wayland, and logging functionality remains represented by supported dependency paths

#### Scenario: A dependency upgrade requires behavior change

- **WHEN** the only available remediation requires leaving Tauri 2, removing a supported platform path, or changing observable application behavior
- **THEN** implementation stops with the resolver or compiler evidence recorded
- **THEN** the change artifacts are updated before the scope expands


<!-- @trace
source: resolve-rust-production-advisories
updated: 2026-08-09
code:
  - BASELINE.md
  - src-tauri/Cargo.toml
  - README.md
  - plan.md
-->

---
### Requirement: Remediation evidence is reproducible

The repository MUST record the changed dependency versions, exact verification commands, tool versions, exit statuses, Rust test counts, frontend production build result, targeted audit result, and any unexecuted platform compile checks.

#### Scenario: Compatible remediation passes verification

- **WHEN** the target dependencies have been updated
- **THEN** `cargo test --manifest-path src-tauri/Cargo.toml`, `cargo audit --file src-tauri/Cargo.lock`, `npm ci`, and `npm run build` exit successfully
- **THEN** the tracked evidence contains the command results and the lockfile diff contains no unrelated dependency families

#### Scenario: A required verification fails

- **WHEN** dependency resolution, compile, test, build, or audit exits unsuccessfully
- **THEN** the evidence records the non-zero exit status and concise error output
- **THEN** the remediation remains incomplete without disabling the failed check

<!-- @trace
source: resolve-rust-production-advisories
updated: 2026-08-09
code:
  - BASELINE.md
  - src-tauri/Cargo.toml
  - README.md
  - plan.md
-->