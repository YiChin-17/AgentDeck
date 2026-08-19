# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

AgentDeck's version numbers start at 1.0.0 and are independent of the upstream
Skills Manager sequence. The upstream history up to v1.30.0 is kept in
[`CHANGELOG-legacy.md`](CHANGELOG-legacy.md).

## [1.0.0] - 2026-08-19

### Release Overview
- First release under AgentDeck's own version sequence. The app now carries its own product identity, its own repository, and no dependency on upstream release infrastructure.
- Beyond managing Skills, AgentDeck now inspects and edits Codex and Claude Code Hooks, Plugins, and Config Profiles from one desktop app.

### User-facing
- **Codex and Claude Code Hooks** — Inspect which Hook configuration each agent actually loads, with its source, scope and format errors shown before anything is written. Editing happens inside the app with schema validation, a real diff, external-modification conflict detection, and a restore path.
- **Plugins** — A read-only inventory of what Codex and Claude Code have installed, plus install and state changes for user-scoped plugins, without touching either CLI's own cache.
- **Config Profiles** — View which scope currently supplies each Codex TOML and Claude Code JSON setting, then apply one set of non-sensitive settings across registered projects with preview, conflict protection and restore.
- **Traditional Chinese by default** — The product UI defaults to `zh-TW`, and an existing `zh` preference no longer starts the app in Simplified Chinese.
- **Modern Codex skill paths** — `.agents/skills` is the deployment default. Skills already living in `~/.codex/skills` still appear in the Agent Workspace instead of silently disappearing.
- **External Library protection** — When a Library on an unmounted external disk is unreachable, the app no longer creates an empty directory at the mount point and treat it as a new Library. It reports the Library as offline and changes nothing.
- **No application auto-update** — The app no longer queries upstream Skills Manager releases and cannot install a binary signed by an upstream key. Installing AgentDeck means building it yourself.
- **Settings links point here** — The GitHub and bug-report entries in Settings now lead to AgentDeck's own repository rather than upstream. (Issue #3)
- **ssh:// Git sources keep working** — A valid `ssh://` skill source is no longer rewritten into an invalid GitHub HTTPS shorthand during normalization. (Issue #2)
- **A misconfigured proxy fails closed** — When a configured proxy cannot build an HTTP client, the backend now reports the failure instead of silently connecting without the proxy. (Issue #5)

### Developer & Governance
- **Own product identity** — Bundle, window, menus, locales, package metadata and icon use AgentDeck and the `io.github.yichin17.agentdeck` Bundle ID. Existing `.skills-manager` storage, `skills-manager.db`, backup protocol, Keychain service and CLI contract are deliberately unchanged, so existing data keeps working.
- **Standalone repository** — AgentDeck now lives in its own repository rather than a fork of upstream, with its own version sequence starting here.
- **Artifact foundation** — Backend identity, deployment and backup metadata are no longer rooted solely in Skill, so Hooks, Plugins and Config Profiles fit without polluting `SkillRecord`.
- **Personal installation is the supported path** — Build from the committed lockfiles and verify the result with `npm run check:personal-installation`. macOS distribution material is retained but dormant; no public release is offered.
- **Security advisories resolved** — The `quick-xml` and `rkyv` advisories found in the production dependency audit are cleared.
- **Pull request validation widened** — Frontend, locale and repository contract checks now run on every pull request, not only on Rust path changes. (Issue #4)
