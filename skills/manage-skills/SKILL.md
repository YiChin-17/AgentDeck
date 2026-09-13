---
name: manage-skills
description: Manage skills through the available skills-manager-cli; distinguish search, library installation, and deployment to user-selected agents or projects.
---

## Before doing anything

1. Confirm the CLI is available: `command -v skills-manager-cli`. If it's not on PATH, this skill doesn't apply — fall back to find-skills (or tell the user to install skills-manager).
2. **Always pass `--json` when you parse output yourself.** Pretty-printed output is for the user; JSON is for you. Errors come back as `{"ok": false, "error": "..."}` on stderr with a non-zero exit code.

```bash
skills-manager-cli --json skills list
```

## Mental model

There's **one central library** at `~/.skills-manager/skills/` that all agents share. Each skill in the library has metadata in a SQLite DB (source URL, preset membership, tags, enabled flag). A **preset** is a named group of skills. The active preset gets **synced** out to every enabled agent's skill directory (`~/.claude/skills/`, `~/.cursor/skills/`, etc.) by symlink or copy.

So the lifecycle is: **install → (in library) → add to preset → sync → (visible to agent)**. `install --sync` is the shortcut that does all three.

Internally, presets are still stored as scenarios for backward-compatible Git Backup. The CLI and UI call them presets.

## Install

```bash
# From skills.sh marketplace
skills-manager-cli skills install vercel-labs/agent-skills@react-best-practices

# Any git URL (use /tree/branch/subpath form when the skill lives in a sub-directory)
skills-manager-cli skills install https://github.com/anthropics/skills.git
skills-manager-cli skills install https://github.com/foo/bar/tree/main/skills/baz

# Local folder
skills-manager-cli skills install ./my-skill

# Force a source type when the ref is ambiguous
skills-manager-cli skills install foo/bar --skillssh
skills-manager-cli skills install ./looks-like/owner-repo --local
```

**Default is library-only** — the skill enters the DB but doesn't appear in any agent yet. To make it visible:
- `--sync` → add to the current active preset + sync to every enabled agent; use only when the user authorized that deployment scope. Otherwise keep the installation library-only.
- `--sync-preset <name>` → add to a specific preset + sync
- Or later: `presets add-skill <preset> <skill>` followed by `skills sync`

**Ref resolution** is deterministic, no path-existence guessing:
1. Starts with `./`, `../`, `/`, or `~/` → local path
2. Contains `://`, ends in `.git`, or starts with `git@` → git URL
3. Matches `owner/repo`, `owner/repo/skill`, or `owner/repo@skill` → skillssh
4. Otherwise → error; pass `--local` / `--git` / `--skillssh` to disambiguate

**Always verify after install** with `skills list` or `skills show <name>` so you can confirm the skill landed and report the preset / sync state back to the user.

## Search

```bash
skills-manager-cli --json skills search "react performance" --limit 5
```

Each result has `install_ref` (paste straight into `skills install`), `installs` (popularity proxy), and `skills_sh_url`. Show relevant candidates with source and scope. Install counts indicate popularity, not validation or trust; inspect suitability and source regardless of popularity.

## Update / Check

```bash
# Re-fetch one skill (git/skillssh re-clones, local/import re-imports source dir)
skills-manager-cli skills update <skill-name-or-id>

# Re-fetch all eligible skills
skills-manager-cli skills update --all

# Just probe remote revisions, don't touch files
skills-manager-cli skills check --all
```

`check` is the dry-run partner of `update`. Local-only skills (no git source) are reported as `skipped: true`.

## Remove

```bash
# Always preview first when removing more than one
skills-manager-cli skills remove <skill> --dry-run

# --yes is required for the actual delete; --json mode does NOT auto-confirm
skills-manager-cli skills remove <skill> --yes
```

Remove deletes the central-library copy, all synced targets across agents, and the DB row. It's not reversible without re-installing.

## Enable / Disable

```bash
skills-manager-cli skills disable <skill>   # skipped by future syncs
skills-manager-cli skills enable <skill>
```

Disable is a "soft remove" — it stops the skill from being written into agent directories on future syncs, but **does not** purge already-synced copies. If the user wants the skill gone from agents *now*, follow up with `skills remove` or re-sync (the disabled skill will be cleaned up on the next sync of its preset).

## Sync

```bash
# Sync current active preset to all enabled agents
skills-manager-cli skills sync

# Preview the target list — safe, no writes
skills-manager-cli skills sync --dry-run

# Switch active preset, then sync
skills-manager-cli skills sync --preset "Web Dev"

# Only sync to a single agent (useful when one agent's directory got out of sync)
skills-manager-cli skills sync --tool claude_code
```

## Adopt skills installed elsewhere

When skills already live in an agent's directory (e.g. installed via `npx skills add` or manual `git clone`) but aren't in the central library, pull them in:

```bash
# Dry-run scan first — lists candidates without writing
skills-manager-cli skills adopt ~/.claude/skills --dry-run

# Adopt everything found — each becomes source_type=local (can't auto-update from git)
skills-manager-cli skills adopt ~/.claude/skills

# Adopt a single skill and pin it to a git source so `update` works later
skills-manager-cli skills adopt ~/.claude/skills/react-best-practices \
  --git-url https://github.com/vercel-labs/agent-skills/tree/main/react-best-practices

# Or pass --git-subpath explicitly when the URL is just the repo root
skills-manager-cli skills adopt ~/.claude/skills/react-best-practices \
  --git-url https://github.com/vercel-labs/agent-skills \
  --git-subpath react-best-practices

# Skill lives at the repo root? Pass an empty subpath
skills-manager-cli skills adopt ~/.claude/skills/my-skill \
  --git-url https://github.com/me/my-skill --git-subpath ""
```

`adopt` auto-excludes anything already in the DB or already a sync target, so it's safe to re-run. `--git-url` requires either a URL with a subpath (`/tree/branch/path`) or an explicit `--git-subpath` — without that, future `update` would re-clone the wrong directory, so the CLI refuses to guess.

## Tag

```bash
skills-manager-cli skills tag add <skill> web frontend
skills-manager-cli skills tag remove <skill> frontend
skills-manager-cli skills tag list <skill>   # tags on one skill
skills-manager-cli skills tag list           # all distinct tags
```

## Presets

```bash
skills-manager-cli presets list
skills-manager-cli presets current

skills-manager-cli presets add-skill <preset> <skill>...
skills-manager-cli presets remove-skill <preset> <skill>...

skills-manager-cli presets apply <preset>   # makes it active + syncs
```

Use `presets add-skill` when you want to put an already-installed skill into a *different* preset without re-installing it, or to share a skill across multiple presets.

## Health check

When sync misbehaves or a command errors in a confusing way:

```bash
skills-manager-cli --json repo status   # base dir, skill / preset counts, active preset
skills-manager-cli --json tools list    # detected agents and their target paths
```

These two are read-only and great for diagnosing "why isn't this skill showing up in Cursor" type questions.

## Typical workflows

### "Find me a skill for X"

1. `skills search "X" --limit 5` — show the top 1–3 hits with install counts and source.
2. Report the candidates and their tradeoffs; a search-only request ends here.

### "Install a skill that does X"

1. Resolve the requested skill; ask only if the selection materially remains ambiguous.
2. Install into the library by default. Deploy only to the agents/projects or preset authorized by the user; do not assume all enabled agents.
3. Use `skills list` or `skills show <name>` to verify library and actual deployment state separately.

### "What skills do I have?"

```bash
skills-manager-cli --json skills list
```

The `enabled`, `presets`, and `source_type` fields are usually the most informative to summarize back.

### "Pull in the skills already installed in my agent directories"

1. `skills adopt ~/.claude/skills --dry-run` (and any other agent dirs the user mentions) — show the candidate list.
2. After user confirms: `skills adopt ~/.claude/skills`.
3. For any adopted skill where the user knows the original repo, follow up with `skills adopt ... --git-url ... --git-subpath ...` to restore the update link.

### "Update everything"

```bash
skills-manager-cli skills check --all     # see what has upstream changes
skills-manager-cli skills update --all    # apply
```

Report which skills actually refreshed (`refreshed: true` in the JSON) vs which were already up-to-date.

## Pitfalls

- **No active preset** → `skills sync` (without `--preset`) fails. Show the user `presets list` and pick one with them, or use `sync --preset <name>`.
- **Install succeeded but skill doesn't appear in the agent** → install defaults to library-only. Re-run with `--sync`, or add it to the active preset and sync.
- **Adopted skills can't be `update`d from git** → `npx skills add` and manual `git clone` don't leave source metadata, so adopt has to treat them as `local`. Fix per-skill with `adopt ... --git-url ... --git-subpath ...`, or just `skills remove` + `skills install <git-ref>` to start clean with a real source.
- **`--dry-run` only exists on `remove`, `sync`, `adopt`.** For `install` / `update` / `check`, the preview is a different command (`search` before install, `check` before update).
