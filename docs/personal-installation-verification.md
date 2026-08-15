# Personal installation verification

Evidence for one personal-installation acceptance run: a local build of a single
commit, verified on one macOS machine. It is a record of that run, not a
standing guarantee — a later commit needs its own run.

Every path below is relative to the project root. This document must never
contain home directory paths, temporary directory paths, Library locations,
tokens, credentials, Keychain contents or user data.

## Environment

| Field | Value |
|-------|-------|
| Verified on | 2026-08-15 |
| Commit | `38f6c07` |
| macOS | 15.7.9 |
| Architecture | arm64 |
| Node.js | v26.4.0 |
| Cargo | 1.97.0 |
| Tauri CLI | 2.10.0 |
| Application version | 1.30.0 (`src-tauri/tauri.conf.json`) |

## Artifacts

| Artifact | Path | Result |
|----------|------|--------|
| Application | `src-tauri/target/release/bundle/macos/AgentDeck.app` | built, version 1.30.0 |
| macOS installer | `src-tauri/target/release/bundle/dmg/AgentDeck_1.30.0_aarch64.dmg` | built from the same run |

`npm run tauri:build` exited 0 and emitted no compiler warnings. Neither artifact is tracked in Git — `git status --short` lists nothing under the bundle directory.

## Automated checks

| Command | Exit | Result |
|---------|------|--------|
| `npm ci` | 0 | 370 packages installed from `package-lock.json` |
| `npm run build` | 0 | `tsc -b` clean, Vite bundle written to `dist/` (chunk-size advisory only) |
| `npm run lint` | 0 | no errors, no warnings |
| `npm run check:i18n` | 0 | passed |
| `node --test scripts/check-legacy-compatibility.test.mjs` | 0 | 59 pass, 0 fail |
| `node --test scripts/check-no-upstream-app-updater.test.mjs` | 0 | 16 pass, 0 fail |
| `node --test scripts/check-personal-installation.test.mjs` | 0 | 21 pass, 0 fail |
| `node --test scripts/check-product-identity.test.mjs` | 0 | 7 pass, 0 fail |
| `node --test scripts/check-ui-command-arguments.test.mjs` | 0 | 6 pass, 0 fail |
| `node --test scripts/product-identity-display.test.mjs` | 0 | 4 pass, 0 fail |
| `node --test scripts/product-identity-icon.test.mjs` | 0 | 4 pass, 0 fail |
| `node --test scripts/product-identity-metadata.test.mjs` | 0 | 2 pass, 0 fail |
| `npm run check:board` | 0 | passed |
| `npm run check:board-layout` | 0 | passed |
| `npm run check:config-profile-management` | 0 | passed |
| `npm run check:config-profiles-ui` | 0 | passed |
| `npm run check:hooks-ui` | 0 | passed |
| `npm run check:no-app-updater` | 0 | passed |
| `npm run check:plugin-mutations` | 0 | passed |
| `npm run check:plugins-ui` | 0 | passed |
| `npm run check:product-identity` | 0 | passed |
| `npm run check:skill-pack-ui` | 0 | passed |
| `cargo test --locked --manifest-path src-tauri/Cargo.toml` | 0 | 894 pass, 0 fail |
| `npm audit --omit=dev` | 0 | 0 vulnerabilities across 371 audited packages (npm 11 bundled auditor) |
| `cargo audit` | 0 | 0 vulnerabilities across 694 crate dependencies (cargo-audit 0.22.2, 1216 advisories) |
| `npm run check:personal-installation` | 0 | `app=AgentDeck.app identifier=io.github.yichin17.agentdeck version=1.30.0 updater=absent docs=complete` |

## Packaged smoke

The qualifying run used an isolated temporary home, Library, registered Codex
and Claude Projects, and fixed-output Plugin adapters. It resolved every checked
path below the isolated root before reading or writing it; no real Agent
configuration, Library, Keychain entry or Project was read or written. An
earlier invalid attempt is excluded from these results and disclosed under
Warnings.

| Step | Result |
|------|--------|
| Application launches and main pages render | pass — Library, Dashboard, Install Skills, Hooks, Plugin, Config Profiles, Backup and Settings all rendered |
| Existing internal Library opens | pass — the second launch reused the library created by the first and skipped the first-run prompt |
| Unavailable external Library shows Library Offline, creates no fallback | pass — offline banner named the configured Library, the external root was not recreated, and the stored library id was unchanged |
| Library Retry reconnects the same configured Library | pass — Retry cleared the offline state without rewriting the configuration |
| Skill deployment and conflict handling | pass — an existing fixture Skill was imported without changing its source, deployed as a Codex copy, detected as locally changed after an external edit, left unchanged on cancel, and wrote back only to the confirmed central copy |
| Plugin preview against fixed adapters | pass — inventory came from the fixed adapters, and preview showed the exact argument vector before any confirm |
| Plugin cancel produces no mutation | pass — only the six read-only adapter calls ran; no install command was ever invoked |
| Hook preview, cancel, stale conflict, apply, restore | pass — apply stayed disabled until a current preview existed, preview wrote nothing, an external edit invalidated the token with no write, confirmed apply changed only the previewed command, and restore reproduced the complete pre-apply JSON bytes |
| Config Profile preview, cancel, stale conflict, apply, restore | pass — preview showed `model: gpt-4 -> gpt-5`, cancel was byte-identical, an external edit invalidated the token with no write, confirmed apply changed only `model` while preserving unknown keys, and restore reproduced the complete pre-apply TOML bytes |
| Only confirmed temporary targets changed after quit | pass — the clean run changed only the isolated app state, imported central Skill, confirmed Codex deployment copy and explicitly confirmed targets; cancel, stale and restored Hook or Config Profile operations left their targets at the expected prior bytes |

Isolation used for the qualifying run: a temporary home directory, a temporary
external Library on the same machine, one temporary registered Project holding
fixture Codex and Claude Code settings, and fixed-output stand-ins for the Codex
and Claude Code executables placed ahead of the real ones. Because the native
folder picker did not accept synthetic navigation, the registered Project was
inserted into the isolated application's fixture database before launch. Every
filesystem assertion rejected paths outside the isolated root.

## Data compatibility

| Check | Result |
|-------|--------|
| Schema 0 → latest migration keeps ids, rows and relationships | `cargo test … migration` — 43 pass, 0 fail |
| Legacy names, refs, trailers, Keychain service and preference keys unchanged | `check-legacy-compatibility.test.mjs` — 59 pass, 0 fail |
| External Library offline creates no fallback Library or mutation | `cargo test … offline` — 39 pass, 0 fail |
| Library availability contract | `cargo test … library` — 49 pass, 0 fail |
| Skill sync and conflict contract | `cargo test … sync` 50 pass, `… conflict` 15 pass, 0 fail |
| Plugin contract | `cargo test … plugin` — 110 pass, 0 fail |
| Hook contract | `cargo test … hook` — 98 pass, 0 fail |
| Config Profile contract | `cargo test … config_profile` — 132 pass, 0 fail |

## Warnings

Known, non-blocking observations from this run.

| Item | Detail |
|------|--------|
| Unsigned build | The application is neither signed with a Developer ID certificate nor notarized. macOS requires a one-time per-application approval on first launch. |
| No auto-update | The build carries no updater dependency, permission, endpoint, public key or install flow, and never checks for a release. |
| macOS only | This run verifies the macOS bundle. Windows and Linux installers are not verified here and are not claimed to be complete. |
| Invalid isolation attempt excluded | Before the qualifying run, verification assertions read metadata and directory names from the operator home, and one native-picker workaround briefly created then removed a fixture-only directory there. No pre-existing content was read, modified or deleted, but that attempt did not meet the isolation contract and none of its results are counted above. The qualifying run restarted from a clean fixture with a path-prefix guard that rejected every path outside its isolated root. |
| Native folder picker limitation | Synthetic navigation did not select the temporary Project reliably. The qualifying run therefore registered the Project directly in the isolated application's fixture database before launch; this did not alter production data or bypass any packaged mutation workflow being verified. |
| Advisory warnings | `cargo audit` reports 0 vulnerabilities and 26 allowed warnings: 17 unmaintained, 8 unsound, 1 yanked. None is an active vulnerability, and remediation would require breaking upgrades in the transitive graph, so the dependency graph is unchanged in this change. |
