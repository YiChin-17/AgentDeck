# git-source-url-normalization Specification

## Purpose

TBD - created by archiving change 'preserve-ssh-git-url-normalization'. Update Purpose after archive.

## Requirements

### Requirement: Supported complete Git URLs retain their transport form

AgentDeck SHALL pass a validated `ssh://` Git source through normalization without rewriting its scheme, authority, repository path, or `.git` suffix. AgentDeck MUST preserve the existing passthrough behavior for non-tree HTTP, HTTPS, and SCP-style `git@` sources.

#### Scenario: Valid ssh URL is normalized

- **GIVEN** the source is `ssh://git@github.com/acme/skills.git`
- **WHEN** AgentDeck validates and parses the Git source
- **THEN** the clone URL is exactly `ssh://git@github.com/acme/skills.git`
- **AND** branch and subpath are unset

#### Scenario: Existing complete URL forms remain unchanged

- **WHEN** AgentDeck parses a non-tree HTTP, HTTPS, or SCP-style `git@` source
- **THEN** normalization preserves the source as the clone URL
- **AND** normalization does not convert it to GitHub shorthand


<!-- @trace
source: preserve-ssh-git-url-normalization
updated: 2026-08-17
code:
  - src-tauri/src/core/git_fetcher.rs
  - scripts/check-product-identity.test.mjs
  - src/views/Settings.tsx
  - plan.md
  - scripts/check-product-identity.mjs
-->

---
### Requirement: Existing shorthand and GitHub tree normalization remains stable

AgentDeck MUST continue converting `owner/repository` shorthand into the existing GitHub HTTPS clone form and MUST continue extracting branch and subpath data from supported GitHub tree URLs.

#### Scenario: GitHub shorthand is parsed

- **GIVEN** the source is `acme/skills`
- **WHEN** AgentDeck parses the Git source
- **THEN** the clone URL is `https://github.com/acme/skills.git`
- **AND** branch and subpath are unset

#### Scenario: GitHub tree URL is parsed

- **GIVEN** the source is `https://github.com/acme/skills/tree/main/tools/example`
- **WHEN** AgentDeck parses the Git source
- **THEN** the clone URL is `https://github.com/acme/skills.git`
- **AND** the branch is `main`
- **AND** the subpath is `tools/example`

<!-- @trace
source: preserve-ssh-git-url-normalization
updated: 2026-08-17
code:
  - src-tauri/src/core/git_fetcher.rs
  - scripts/check-product-identity.test.mjs
  - src/views/Settings.tsx
  - plan.md
  - scripts/check-product-identity.mjs
-->