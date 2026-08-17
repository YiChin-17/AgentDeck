# http-client-proxy-construction Specification

## Purpose

TBD - created by archiving change 'fail-closed-invalid-proxy'. Update Purpose after archive.

## Requirements

### Requirement: Configured proxy failures stop HTTP client construction

When a non-empty proxy is configured, AgentDeck MUST return an explicit error if the proxy cannot be parsed or the blocking HTTP client cannot be built. AgentDeck MUST NOT replace the failed configured client with a default client and MUST NOT send the same operation through a direct connection.

#### Scenario: Configured proxy is malformed

- **GIVEN** a non-empty proxy value that `reqwest::Proxy::all` rejects
- **WHEN** a skills.sh or GitHub HTTP operation constructs its client
- **THEN** the operation returns an error before sending a request
- **AND** no direct client is constructed as fallback

#### Scenario: Configured client build fails

- **GIVEN** proxy parsing succeeds and the HTTP client builder returns an error
- **WHEN** a skills.sh or GitHub HTTP operation constructs its client
- **THEN** the operation returns the client construction error before sending a request
- **AND** no default client replaces the failed client

#### Scenario: Proxy error is surfaced safely

- **WHEN** proxy client construction returns an error
- **THEN** the error identifies the proxy configuration or HTTP client construction stage
- **AND** the error context does not include the configured proxy URL or embedded credentials


<!-- @trace
source: fail-closed-invalid-proxy
updated: 2026-08-17
code:
  - src-tauri/src/core/skillssh_api.rs
  - plan.md
  - src/views/Settings.tsx
  - src-tauri/src/core/git_fetcher.rs
  - src-tauri/src/core/github_api.rs
  - scripts/check-product-identity.test.mjs
  - .github/workflows/test.yml
  - scripts/check-product-identity.mjs
-->

---
### Requirement: Empty and absent proxy values preserve direct client behavior

AgentDeck SHALL treat `None` and an empty proxy string as no configured proxy and SHALL construct the normal blocking HTTP client with the requested timeout and user agent.

#### Scenario: Proxy setting is absent

- **GIVEN** the proxy value is `None`
- **WHEN** AgentDeck constructs the blocking HTTP client
- **THEN** client construction succeeds without a proxy

#### Scenario: Proxy setting is empty

- **GIVEN** the proxy value is an empty string
- **WHEN** AgentDeck constructs the blocking HTTP client
- **THEN** client construction succeeds without a proxy


<!-- @trace
source: fail-closed-invalid-proxy
updated: 2026-08-17
code:
  - src-tauri/src/core/skillssh_api.rs
  - plan.md
  - src/views/Settings.tsx
  - src-tauri/src/core/git_fetcher.rs
  - src-tauri/src/core/github_api.rs
  - scripts/check-product-identity.test.mjs
  - .github/workflows/test.yml
  - scripts/check-product-identity.mjs
-->

---
### Requirement: Supported configured proxy schemes remain usable

AgentDeck SHALL continue accepting proxy URLs parsed by reqwest for HTTP, HTTPS, and SOCKS5 proxy schemes and SHALL apply the resulting proxy to the blocking HTTP client.

#### Scenario: Supported proxy URL is configured

- **WHEN** AgentDeck constructs clients with valid HTTP, HTTPS, and SOCKS5 proxy URLs
- **THEN** construction succeeds for each proxy scheme
- **AND** AgentDeck does not downgrade any configured proxy to a direct client

<!-- @trace
source: fail-closed-invalid-proxy
updated: 2026-08-17
code:
  - src-tauri/src/core/skillssh_api.rs
  - plan.md
  - src/views/Settings.tsx
  - src-tauri/src/core/git_fetcher.rs
  - src-tauri/src/core/github_api.rs
  - scripts/check-product-identity.test.mjs
  - .github/workflows/test.yml
  - scripts/check-product-identity.mjs
-->