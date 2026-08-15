<p align="center">
  <img src="assets/icon.png" width="80" />
</p>

<h1 align="center">AgentDeck</h1>

<p align="center">
  One app to manage AI agent skills across all your coding tools.
</p>

## AgentDeck fork direction

AgentDeck is a fork of [xingkongliang/skills-manager](https://github.com/xingkongliang/skills-manager) retained under the MIT License. The upstream baseline and verification evidence are recorded in [`BASELINE.md`](BASELINE.md).

AgentDeck extends the upstream skill-management foundation to manage Codex and Claude Code Skills, Plugins, Hooks, and Config Profiles from one desktop app. macOS is the first target. Existing upstream cross-platform behavior remains protected unless a later specification explicitly changes that compatibility boundary.

Desktop builds use `AgentDeck` as the product name and `io.github.yichin17.agentdeck` as the stable Bundle ID. This product identity change intentionally keeps the existing `.skills-manager` storage, `skills-manager.db`, backup protocol, `skills-manager-git-backup` Keychain service, local preference keys, and `skills-manager-cli` command contract so existing data and automation continue to work.

The retained GitHub OAuth integration may still appear as `skills-manager` on GitHub's authorization page; use that actual external name when revoking access. On macOS, the Bundle ID change can leave the previous and new apps installed together. Close the old app before starting AgentDeck. After confirming AgentDeck can open the existing Library and backup settings, manually remove `Skills Manager.app` if desired. AgentDeck does not delete the old app or user data.

<p align="center">
  🎬 <a href="https://www.youtube.com/watch?v=wfbCrfNASVU">Video intro (YouTube)</a>
  &nbsp;·&nbsp;
  <a href="https://www.bilibili.com/video/BV1845F6REUu/">视频介绍 (Bilibili)</a>
</p>

<p align="center">
  <a href="./README.zh-CN.md">中文说明</a>
  &nbsp;·&nbsp;
  <a href="https://x.com/JayTL00">@JayTL00 on X</a>
  &nbsp;·&nbsp;
  <a href="https://buymeacoffee.com/jaytl">Buy me a coffee</a>
</p>

<p align="center">
  <a href="https://trendshift.io/repositories/23290?utm_source=repository-badge&amp;utm_medium=badge&amp;utm_campaign=badge-repository-23290" target="_blank" rel="noopener noreferrer"><img src="https://trendshift.io/api/badge/repositories/23290" alt="xingkongliang%2Fskills-manager | Trendshift" width="250" height="55"/></a>
</p>

<p align="center">
  <img src="assets/demo/library.png" width="800" alt="AgentDeck Library" />
</p>

<p align="center"><strong>Install Skills — Marketplace</strong></p>
<p align="center"><img src="assets/demo/install-skills.png" width="800" alt="Install Skills Marketplace" /></p>

<p align="center"><strong>Global Workspace</strong></p>
<p align="center"><img src="assets/demo/global-workspace.png" width="800" alt="Global Workspace" /></p>

<p align="center"><strong>Agent Workspace</strong></p>
<p align="center"><img src="assets/demo/agent-workspace.png" width="800" alt="Agent Workspace" /></p>

<p align="center"><strong>Project Workspace</strong></p>
<p align="center"><img src="assets/demo/project-workspace.png" width="800" alt="Project Workspace" /></p>

<p align="center"><strong>Backup & Multi-Device Sync</strong></p>
<p align="center"><img src="assets/demo/backup.png" width="800" alt="Backup and multi-device sync" /></p>

<p align="center"><strong>Settings</strong></p>
<p align="center"><img src="assets/demo/settings.png" width="800" alt="Settings" /></p>

## Features

- **Unified skill library** — Install skills from Git repos, local folders, `.zip` / `.skill` archives, or the [skills.sh](https://skills.sh) marketplace. Everything goes into one central repo, which defaults to `~/.skills-manager` and can be customized in **Settings**.
- **Marketplace + AI search** — Browse popular skills from the marketplace, run keyword search, or enable SkillsMP AI search with your API key.
- **Presets** — Group skills into named presets. In any workspace, click a preset pill to instantly activate or deactivate all its skills for the current agent scope. The sidebar lists all presets for quick access.
- **Global Workspace** — Each agent gets its own page listing every skill in its global folder — including ones installed outside AgentDeck — so the view always reflects what the agent actually sees. Add or remove skills per agent, or use the All Agents overview to manage every installed agent at once.
- **Project Workspaces** — View and manage project-local skill folders for supported agents, compare them with your central library, and sync changes in either direction. Supports nested skill directories and per-agent assignment when exporting.
- **Linked Workspaces** — Point to any directory as a skills root — useful for skills that live outside the default agent paths. Managed as a standalone workspace without participating in global preset sync.
- **Multi-tool sync** — Sync skills to any supported tool via symlink or copy with a single click. Every skill card shows an agent icon badge per enabled agent — click a badge to install or remove that skill for that agent right from the card, with the badge reflecting live sync state.
- **Add from Library sheet** — In any workspace, click **+ Add Skills** to open a unified picker: search your central library, toggle target agents with always-visible chips (with select-all/clear), and batch-add multiple skills in one click.
- **Batch operations** — Multi-select skills for bulk enable/disable, export, or delete. Project Workspaces also support bulk enable/disable for project-local skills.
- **Skill tagging and filters** — Tag skills, use tags to group similar skills, and filter by source or tag — including an **Untagged** pill to quickly find skills missing labels.
- **Update tracking** — Check for upstream updates on Git-based skills; re-import local ones.
- **Skill preview and source inspection** — Read `SKILL.md` / `README.md`, inspect source metadata, and compare local content with the upstream version inside the app.
- **Custom tools** — Add your own agents/tools with custom skills directories, or override the default path for any built-in tool.
- **Backup & multi-device sync** — Connect a private GitHub repository with one sign-in (or any Git remote), and the app backs your library up automatically and keeps all connected devices in sync. Merges are skill-aware — a rename on one machine combines cleanly with an edit on another — and true conflicts never block: your local version stays put until you choose keep mine / use remote / keep both. Snapshot versions are restorable at any time.
- **Activity log & Export Logs** — Install / remove / update / sync operations are recorded locally. Use **Settings → Export Logs** to bundle recent logs and activity history into a single zip for easier issue reports.
- **Flexible app settings** — Configure repo path, sync mode, theme, text size, language, tray behavior, proxy, Git remote, Skill update preferences, and the order agents appear throughout the app — all in one place.

## Core Concepts

<p align="center">
  <img src="assets/diagram-concept-map.png" width="640" alt="Concept map: Library, Preset, Global Workspace, Project Workspace, Agent" />
</p>

- **Presets are reusable skill groups** — A preset is a named collection of skills. Activate a preset in any workspace to add all its skills to the selected agents; deactivate to remove them. Applying a preset is a one-time copy — not a live sync.
- **Global Workspace manages per-agent global skills** — Each installed agent has its own global skills folder (e.g. `~/.claude/skills/` for Claude Code). Each agent page lists everything in that folder — even skills installed without AgentDeck — so you can add, remove, or adopt them; the All Agents overview manages every agent at once.
- **Project Workspaces are project-local skill sets** — A project workspace manages the skills that live inside a specific project (e.g. `<project>/.claude/skills/`). Skills added here only apply to that project.
- **Tags are for grouping and filtering** — Use tags to label similar skills, then filter by tag to find the subset you want quickly.
- **Batch control works everywhere** — Multi-select skills in any workspace for bulk operations.

## Quick Start

1. Install skills from local folders, Git repositories, archives, or the marketplace. If you have a SkillsMP API key, you can also turn on AI search.
2. Open **Global Workspace** from the sidebar and pick an agent (e.g. Claude Code).
3. Click a **Preset** pill to activate its skills for that agent, or use **+ Add Skills** to pick from your library and toggle target agents inline. Active presets show a ✓; partial installs show a count badge.
4. To manage project-local skills, open a **Project Workspace** and use the same preset pills or the **+ Add Skills** picker with its multi-agent target selector.
5. Configure agent paths, custom tools, theme, language, proxy, and Git preferences in **Settings**.
6. If you want history or multi-machine sync, open **Backup** in the sidebar and click **Sign in with GitHub** — backup and cross-device sync run automatically from then on.

## Backup & Multi-Device Sync

The **Backup** page (sidebar) keeps your skill library versioned in a Git repository. One device gets versioned backup with restorable snapshots; several devices connected to the same repository stay in sync with each other automatically. The remote stays a plain Git repository — you can `git clone` it anywhere, no lock-in.

### Connect

- **Sign in with GitHub** (recommended): an 8-digit device-flow sign-in creates a private `skills-manager-backup` repository for you. The token is stored in the OS keychain — never in files or the repo config.
- **Advanced**: paste any Git URL (HTTPS + PAT, SSH, self-hosted) under **Settings → Git Sync Configuration**.
- On a new machine with an empty library, the first launch asks: **start fresh, or restore from a backup?**

### How syncing works

- **Automatic**: local changes are committed and pushed in the background a couple of minutes after you stop editing; updates pushed by your other devices are merged in and pushed back automatically. **Back Up Now** is always available for an immediate run, and every backup in the history shows which device made it.
- **Skill-aware merging**: changes are merged per skill, not per text line — renaming a skill on one machine combines cleanly with editing its content on another.
- **Conflicts never block or overwrite**: if the same skill was edited on two devices at once, everything else syncs normally while that skill keeps your local version and appears under **Needs attention** (also badged on its card in the Library). Pick **keep mine / use remote / keep both** — a safety snapshot is taken before any choice is applied, so every decision is undoable.
- **Snapshots & restore**: manual backups create snapshot versions; open the Backup page history to restore any of them. A restore first saves the current state as its own snapshot.

### What's included

Skills, tags, presets, and per-agent skill toggles are backed up. Secrets (API keys, tokens, proxy settings) and machine-specific wiring never leave the machine. Skills over 100 MB stay local and are excluded from backup automatically (labeled on the Backup page). The SQLite database is not in Git — it stores metadata that is rebuilt from the skill files.

### Disconnecting

The Backup page offers three levels: **disconnect this machine** (other devices and remote data untouched), **revoke the GitHub authorization**, or **delete the remote backup** entirely (routed through GitHub's own type-the-name confirmation).

## Supported Tools

52 agents are supported out of the box, including:

Claude Code · Codex · Cursor · GitHub Copilot · Gemini CLI · OpenCode · OpenClaw · Hermes Agent · OpenHands · Cline · Goose · Windsurf · Continue · Grok · Antigravity · Qwen Code · Crush · Kilo Code · Roo Code · Amp · Kiro CLI · Droid · TRAE IDE · Warp · Qoder · CodeBuddy

**Settings** lists them all, leading with the ones detected on your machine. You can also add custom tools there and manage their skills the same way.

## In-App Help

The **Help** button in **Settings** mirrors the current product flow: recommended workflows, presets, skill installation, the Library (with the Untagged filter and per-card delete), the Global Workspace and the **+ Add Skills** sheet, Project Workspaces with the multi-agent target picker, backup & multi-device sync, and environment-level settings (including Export Logs for issue reports). It is intended as the in-app version of this quick-start guide.

## Tech Stack

| Layer | Tech |
|-------|------|
| Frontend | React 19, TypeScript, Vite, Tailwind CSS |
| Desktop | Tauri 2 |
| Backend | Rust |
| Storage | SQLite (`rusqlite`) |
| i18n | react-i18next |

## Getting Started

### Prerequisites

- Node.js 18+
- Rust toolchain
- [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your OS

### Development

```bash
npm install
npm run tauri:dev
```

### CLI

The repository includes an agent-friendly CLI built on the same Rust shared core used by the desktop app. Both the CLI and the desktop app go through the same SQLite database, central library, and sync engine.

```bash
# Repository / library overview
npm run cli -- repo status
npm run cli -- skills list
npm run cli -- skills show db

# Install skills (default: enter library only — does NOT sync to agents)
npm run cli -- skills install ./my-skill                       # local path
npm run cli -- skills install https://github.com/foo/bar.git   # git URL
npm run cli -- skills install vercel-labs/agent-skills@react-best-practices  # skills.sh
npm run cli -- skills install foo/bar --sync                   # add to active preset + sync to agents

# Update / check from upstream (git skills re-clone, local skills re-import source)
npm run cli -- skills update --all
npm run cli -- skills check --all

# Search the skills.sh marketplace (no API key needed)
npm run cli -- skills search react --limit 5

# Remove (--yes required; --dry-run available)
npm run cli -- skills remove <ref> --dry-run
npm run cli -- skills remove <ref> --yes

# Enable / disable skills by changing preset membership
npm run cli -- presets add-skill <preset> <ref>
npm run cli -- presets remove-skill <preset> <ref>

# Sync the active preset out to enabled agents
npm run cli -- skills sync --dry-run
npm run cli -- skills sync --tool claude_code

# Adopt skills that already exist in an agent directory (e.g. ~/.claude/skills/)
npm run cli -- skills adopt ~/.claude/skills --dry-run
npm run cli -- skills adopt ~/.claude/skills

# Tag
npm run cli -- skills tag add <ref> web frontend
npm run cli -- skills tag list

# Presets
npm run cli -- presets list
npm run cli -- presets preview Default
npm run cli -- presets apply Default
npm run cli -- presets add-skill <preset> <skill>
npm run cli -- presets remove-skill <preset> <skill>

# Export one skill to an arbitrary directory (one-shot copy, not managed)
npm run cli -- skills export db --dest ~/.claude/skills/db

# Git-backed skills repo
npm run cli -- git status
npm run cli -- git pull
npm run cli -- git commit -m "chore: update skills"
```

Available command groups:
- `repo` — inspect or change the configured base directory
- `tools` — list detected tool targets and paths
- `skills` — manage skills in the central library (`list / show / install / update / check / remove / enable / disable / sync / search / adopt / tag / export`)
- `presets` — list presets, preview / apply, add or remove skills from a preset
- `git` — operate on the git-backed `skills/` repository (`clone`, `pull`, `push`, `commit`, `versions`, `restore`)

Extra flags:
- `--skills-root <path>` — operate on a cloned/exported skills repo directly instead of the local app default. The manager's state (DB, presets, cache, logs) lives in `~/.skills-manager/external/<name>-<hash>/`, namespaced by the canonical path of the skills root, so the external checkout itself stays clean.
- `--json` — machine-readable output for scripts/agents

```bash
npm run -s cli -- --skills-root /path/to/my-skills --json skills list
```

#### Install the binary on PATH

Agents and scripts that invoke `skills-manager-cli` directly (without `npm run`) need the binary on PATH. Install it with:

```bash
npm run cli:install
# equivalent to:
# cargo install --path src-tauri --bin skills-manager-cli --locked --force
```

This drops the binary at `~/.cargo/bin/skills-manager-cli`. Re-run after pulling updates to refresh it.

#### Concurrent use with the desktop app

The CLI and desktop app share the same SQLite database. SQLite serializes writes safely, but the running app does not auto-refresh its in-memory caches when the CLI mutates state — restart or trigger a manual refresh in the app after `presets apply`, `git pull`, or other CLI write operations.

### Build

```bash
npm run tauri:build
npm run cli:build
```

## Personal installation (macOS)

AgentDeck is installed by building it yourself. There is no published download: this project ships **no application auto-update**, no public release hosting, no Developer ID signing and **no notarization guarantee**. What you install is a personal local build of the commit you checked out, and every statement below applies to that build only.

### 1. Build from the committed lockfiles

```bash
npm ci
npm run tauri:build
```

`npm ci` installs the exact dependency versions in `package-lock.json`, and the Rust side builds against `src-tauri/Cargo.lock`, so the same commit produces the same application. The build writes:

- `src-tauri/target/release/bundle/macos/AgentDeck.app` — the application
- `src-tauri/target/release/bundle/dmg/AgentDeck_<version>_<arch>.dmg` — the macOS installer for that same build

Neither is tracked in Git. Verify what you just built with:

```bash
npm run check:personal-installation
```

It confirms the bundle name, the `io.github.yichin17.agentdeck` Bundle ID, the version, the executable, the installer and the absence of any application updater surface, and prints one summary line.

### 2. Install the application

Open the `.dmg` and drag `AgentDeck.app` into `/Applications`, or copy the `.app` there yourself. A personal `~/Applications` folder works the same way.

Because this build is not signed by a Developer ID certificate, macOS asks you to approve it once. Approve the app itself — right-click it in Finder and choose **Open**, or open **System Settings → Privacy & Security** and click **Open Anyway** after the first blocked launch. That is a per-application approval; do not turn off Gatekeeper or any other system security check to run this build.

### 3. First launch and existing data

On first launch AgentDeck opens the data that is already on the machine. It reuses the existing `.skills-manager` storage directory, the `skills-manager.db` SQLite database, presets, registered Projects, deployment records, Git backup metadata, the `skills-manager-git-backup` Keychain entry and the local preference keys, all under their existing names. The schema migration runs in place and is retryable; nothing is renamed, moved, duplicated or deleted.

If you also used **Skills Manager.app** before, close it before starting AgentDeck — the two are separate applications sharing the same data.

### 4. Library location and reconnecting an offline Library

An internal Library lives inside the application's own data directory. An external Library stays wherever you configured it, including a removable or network volume.

When a configured external Library is unreachable, AgentDeck shows **Library Offline** for that Library and changes nothing: it does not create a replacement Library, does not repoint deployments and does not record deletions. Reconnect the volume and use the **Retry** action to bring the same Library back.

### 5. Back up and restore

Use the **Backup** page to connect a Git remote, then **Back Up Now** for an immediate versioned backup. To restore, open the backup history and pick a snapshot — the current state is saved as its own snapshot first, so a restore is undoable. On a machine with an empty library, the first launch offers to restore from an existing backup instead of starting fresh. See [Backup & Multi-Device Sync](#backup--multi-device-sync) for the full behavior.

### 6. Uninstall

Removing `AgentDeck.app` from `/Applications` removes the application only. **It does not remove your data.** Your library, database, backup metadata and stored credential stay where they are, so reinstalling a later build picks them up again.

If you also want the data gone, remove these individually, and only the ones you actually want to lose:

- the `.skills-manager` storage directory in your home folder — library content, presets and deployment records
- an external Library directory, if you configured one outside that storage directory
- the `skills-manager-git-backup` entry in **Keychain Access** — the Git backup credential
- the Git backup remote itself, if you no longer want the versioned history

The `skills-manager-cli` binary, if you installed it, lives at `~/.cargo/bin/skills-manager-cli` and is removed separately.

## Troubleshooting

### macOS asks for the `skills-manager-git-backup` keychain entry again

A personal build's code signature changes whenever you rebuild it, and macOS ties keychain access to that signature. After installing a new local build, the first Git backup may ask for permission to read the `skills-manager-git-backup` entry. Click **Always Allow** for the new build.

## Star History

<p align="center">
  <a href="https://github.com/xingkongliang/star-history-svg">
    <img src="assets/star-history.svg" width="800" alt="Star History chart for xingkongliang/skills-manager" />
  </a>
</p>

## License

MIT
