## Context

Phase 5 starts after Hook management is complete. AgentDeck has no Plugin runtime modules or route, but schema v8 already reserves Artifact kind `plugin`. The local toolchain snapshot used for this proposal is Codex CLI 0.144.5 and Claude Code 2.1.229: both expose Plugin inventory as JSON, while their command sets and fields differ. Codex exposes Plugin and marketplace listing through `plugin list` and `plugin marketplace list`; Claude Code exposes installed／available listing plus marketplace listing.

The official CLIs own marketplace resolution, installed state, caches, authentication and future schema evolution. AgentDeck must consume their public JSON boundary without reading those internal files or letting the frontend construct a process invocation.

## Goals / Non-Goals

**Goals:**

- Invoke only four fixed read-only Plugin inventory commands plus fixed version probes, without a shell or caller-supplied executable／arguments.
- Bound process duration and captured output, then return stable sanitized diagnostics.
- Normalize Agent-specific JSON into an explicit DTO without inventing missing values.
- Isolate Codex and Claude Code availability／parse failures.
- Provide a complete read-only Plugins route with filters, details and bilingual diagnostics.
- Keep Plugin payload, CLI output, marketplace credentials and cache data out of persistence.

**Non-Goals:**

- No install、update、remove、enable、disable、marketplace mutation、validate、details or eval invocation.
- No direct Plugin cache／manifest／settings reads and no embedded Skill、Hook、MCP server、script or dependency scan.
- No Plugin Artifact、detail、deployment、Library copy、Git backup or database migration.
- No Project assignment、Board integration、cross-Agent conversion、semver ordering or inferred update checks.
- No changes to existing Skill、Hook、backup or offline Library workflows.

## Decisions

### Fixed Plugin CLI capability table

Backend code defines an enum-backed table for `codex --version`, `codex plugin list --available --json`, `codex plugin marketplace list --json`, `claude --version`, `claude plugin list --available --json` and `claude plugin marketplace list --json`. The Tauri command exposes no executable, path, working directory, environment override or argument field. Each process uses `tokio::process::Command` directly with stdin closed and never uses a shell.

The adapter probes version first and records supported read capabilities for that exact response. An unavailable executable produces an Agent-local diagnostic. A version probe that cannot establish the fixed CLI contract reports `unsupported_cli` and skips that Agent's inventory commands; a later inventory non-zero exit remains `non_zero_exit`, while a marketplace-list non-zero exit becomes `marketplace_unavailable`. AgentDeck does not inspect raw stderr or add help probes to guess why a subcommand failed, and it does not fall back to reading config or cache files.

Alternative rejected: accept arbitrary command arrays from the frontend. That would turn the IPC into a process execution surface and make read-only guarantees unenforceable.

### Bounded asynchronous process runner and sanitized failures

A shared runner uses existing Tokio support, a 10-second timeout per invocation, concurrent bounded reads of stdout and stderr, and a 1,048,576-byte limit for each stream. It terminates and reaps a timed-out or oversized child. Exactly 1,048,576 bytes is accepted; the next byte returns `output_too_large`.

The fixed diagnostic vocabulary is `cli_missing`, `unsupported_cli`, `timeout`, `non_zero_exit`, `invalid_json`, `output_too_large` and `marketplace_unavailable`. A diagnostic contains Agent, command capability, code and optional numeric exit status only. Raw stdout、stderr、filesystem paths、environment values and parser excerpts never enter IPC errors or logs.

Alternative rejected: use `Command::output`. It buffers unbounded output before the caller can enforce the response limit.

### Agent-specific JSON parsers and lossless unknown states

Codex and Claude Code keep separate fixture-driven parsers because equal field names do not prove equal semantics. Both produce `PluginInventoryItemDto` with a route-local id and these fields: Agent, plugin id, display name, installed state, availability, installed version, available version, scope, marketplace, enabled state and update state. Version values remain opaque strings.

The normalized states use explicit enums with `unknown`; absent or unrecognized CLI fields map to `unknown`, never to false、global、disabled or up-to-date. Additional JSON fields are ignored without rejecting the entire response. Records deduplicate only within the same Agent、marketplace and plugin id; an installed record wins presence while each known field is merged only from that same key. Sort order is Agent, display name, marketplace, plugin id.

Alternative rejected: one permissive deserializer shared by both Agents. It would silently assign one CLI's meaning to the other CLI's shape.

### Agent-isolated aggregation without persistence

`get_plugin_inventory` runs Codex and Claude Code collection independently and returns both `PluginAgentInventoryDto` results in a `PluginInventoryDto`. Each Agent result carries version, capabilities, marketplaces, items and diagnostics. A failed inventory or marketplace command affects only that Agent and capability; usable records from the other Agent remain visible. A marketplace connectivity failure becomes `marketplace_unavailable` only when the CLI identifies that fixed listing capability as unavailable.

The collector receives no `SkillStore` or Library path and performs no write. The response exists only in backend memory, the current IPC response and route-local React state.

Alternative rejected: cache inventory in SQLite. The first Phase 5 change has no lifecycle or freshness contract for persistent Plugin data.

### Read-only Plugins route and route-local state

`/plugins` becomes available only with the implemented page and Sidebar entry. The page shows Agent status cards, installed／available rows, version、scope、marketplace、enabled／update values, localized diagnostics and filters for Agent、presence、scope、marketplace and status. Unknown values stay visible as unknown. Refresh replaces the route-local response using a latest-request-wins request id.

The page renders no mutation, validation, marketplace management, details or eval controls. It stores no inventory or CLI output in `AppContext` or localStorage.

Alternative rejected: reuse the Skill Board. A Plugin inventory record is CLI-owned read state and is not yet an Artifact deployment.

## Implementation Contract

- **Backend interface:** add one Tauri command named `get_plugin_inventory` with no request fields. It returns `PluginInventoryDto { agents, generatedAt }`; each Agent result contains `agent`, `cliVersion`, `capabilities`, `marketplaces`, `items` and `diagnostics`.
- **Allowed processes:** only the six fixed probes in the Fixed Plugin CLI capability table. Tests inject a fake runner behind an internal trait or function seam; production never accepts an executable or arguments over IPC.
- **Bounds:** each process has a 10-second deadline and accepts at most 1,048,576 bytes separately on stdout and stderr. Timeout／overflow kills and reaps the child before returning.
- **Normalization:** all status-like DTO fields are closed enums containing `unknown`. Version strings are not parsed or compared. Identity and deduplication are scoped to Agent plus marketplace plus plugin id.
- **Failure behavior:** diagnostics expose only fixed codes and optional exit status. One Agent or one marketplace failure does not remove successful items from another Agent. Invalid top-level JSON invalidates only that command result.
- **Persistence boundary:** command and UI code must contain no Plugin database write, Library access, official cache access, localStorage write or logging of captured output.
- **UI behavior:** `/plugins` and its Sidebar link render inventory, filters, details, empty states and diagnostics; they expose no command that mutates either CLI.
- **Acceptance:** Rust tests cover exact argv, no-shell execution seam, both JSON parsers, unknown fields, deduplication, deterministic sort, 1,048,576／1,048,577-byte boundaries, timeout, non-zero exit, invalid JSON, missing CLI, Agent isolation and absence of persistence calls. `npm run build`, `npm run lint`, `npm run check:i18n`, `npm run check:plugins-ui`, the full locked Rust suite and `git diff --check` exit 0.
- **In scope:** fixed global CLI inventory and marketplace listing for Codex and Claude Code.
- **Out of scope:** every Plugin／marketplace mutation, Project assignment, direct cache read, Plugin component scan and persistent Plugin model.

## Risks / Trade-offs

- [CLI JSON schemas change between releases] → Keep Agent-specific fixtures, ignore additive fields, expose `unsupported_cli`／`invalid_json` instead of guessing, and record the CLI version in each response.
- [A child blocks or emits unlimited data] → Enforce concurrent bounded reads, deadline, kill and reap before returning.
- [stderr or parser errors contain credentials or local paths] → Return fixed diagnostic codes only and never log captured streams.
- [Installed and available feeds disagree] → Merge only an identical Agent／marketplace／plugin key and preserve unknown values; do not infer semver or updates.
- [One CLI is absent or offline] → Keep its diagnostic beside the other Agent's usable inventory.

## Migration Plan

1. Add the backend runner、Agent adapters、parsers and Tauri command behind tests.
2. Add frontend DTOs、route、Sidebar link、Plugins page、locales and static contract.
3. Run all acceptance commands before exposing the route in a release.
4. No schema or user-data migration runs because inventory is transient.
5. Rollback removes the route、command and new modules; official CLI state and AgentDeck persistence remain unchanged.

## Open Questions

None. Mutation capabilities and persistent Plugin identity remain intentionally deferred to the next Phase 5 change.
