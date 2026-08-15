## ADDED Requirements

### Requirement: Profiles persist only exact typed non-sensitive settings

AgentDeck SHALL persist each Config Profile as one `config_profile` Artifact with a revisioned detail and zero or more Agent-specific entries. Every entry MUST use a canonical key from the Config Profile inspection allowlist and the exact string, boolean, or integer type assigned to that key. A profile, SQLite row, deployment row, frontend mutation DTO, diagnostic, log, and Git backup metadata MUST NOT contain a raw configuration document, an unknown key or value, a credential, a token, an API key, an environment value, a permission rule, a command, a path, Hook data, MCP data, or Plugin data.

#### Scenario: Valid mixed-Agent profile is stored as typed entries

- **WHEN** the user creates a profile containing valid Codex and Claude Code allowlisted scalar entries
- **THEN** AgentDeck creates one `config_profile` Artifact and stores each entry with its Agent, canonical key, exact scalar type, and scalar value
- **AND** the profile revision is `1`

##### Example: Development profile

| Agent | Canonical key | Typed value |
| ----- | ------------- | ----------- |
| Codex | `sandbox_mode` | string `read-only` |
| Codex | `model_reasoning_effort` | string `high` |
| Claude Code | `always_thinking_enabled` | boolean `true` |
| Claude Code | `cleanup_period_days` | integer `20` |

#### Scenario: Unknown or wrong-type entry is rejected atomically

- **WHEN** a create or update request contains an unknown key or a value whose scalar type differs from the allowlist
- **THEN** AgentDeck returns `invalid_profile_entry`
- **AND** creates or changes no Artifact, detail, entry, assignment, deployment, or recovery row

#### Scenario: Secret-shaped request cannot cross the mutation boundary

- **WHEN** a caller adds a raw document, path, environment, credential, token, command, nested object, or unknown field to a profile request
- **THEN** request deserialization or profile validation rejects the request
- **AND** no rejected key name or value appears in a response or log

### Requirement: Profile CRUD is revisioned and transactionally consistent

AgentDeck SHALL expose list, create, update, and delete operations for Config Profiles. Create and update MUST validate names and entries before opening the write transaction, update the Artifact, detail, and entry set in one SQLite transaction, and increment the revision exactly once for each successful entry-set or name change. Delete MUST reject a profile that has a canonical deployment assignment or recovery metadata.

#### Scenario: Successful update replaces the entry set once

- **GIVEN** profile `profile-1` is at revision `3`
- **WHEN** the user saves a valid replacement name and entry set against revision `3`
- **THEN** AgentDeck commits the name and complete entry set together
- **AND** returns revision `4`

#### Scenario: Stale profile editor is rejected

- **GIVEN** profile `profile-1` is at revision `4`
- **WHEN** an update request uses expected revision `3`
- **THEN** AgentDeck returns `stale_profile`
- **AND** the Artifact, detail, entries, assignments, and source files remain unchanged

#### Scenario: In-use profile cannot be deleted

- **GIVEN** a Config Profile has at least one Project and Agent assignment or a recovery point
- **WHEN** the user requests profile deletion
- **THEN** AgentDeck returns `profile_in_use`
- **AND** no Artifact, detail, entry, deployment, or recovery data is deleted

### Requirement: Assignments reuse canonical Project deployments

AgentDeck SHALL represent each Config Profile assignment with the existing canonical deployment identity for one Config Profile Artifact, one registered Project scope, and one Agent. An assignment MUST resolve the Project from its stored record and MUST NOT persist a caller-supplied root or target path. The same profile, Project, and Agent tuple MUST be unique. Assignment creation and removal MUST preserve referential integrity and MUST NOT write a configuration source.

#### Scenario: One profile is assigned to two Projects and both Agents

- **WHEN** the user assigns one profile to Project `alpha` and Project `beta` for Codex and Claude Code
- **THEN** AgentDeck stores exactly four canonical deployment identities
- **AND** each identity refers to the same Config Profile Artifact and its own Project and Agent tuple

##### Example: Canonical assignment tuples

| Profile | Project | Agent | Count |
| ------- | ------- | ----- | ----- |
| `profile-1` | `alpha` | Codex | 1 |
| `profile-1` | `alpha` | Claude Code | 1 |
| `profile-1` | `beta` | Codex | 1 |
| `profile-1` | `beta` | Claude Code | 1 |

#### Scenario: Unknown Project assignment is rejected

- **WHEN** the caller assigns a profile to a Project ID that is absent from AgentDeck
- **THEN** AgentDeck returns `project_not_found`
- **AND** creates no deployment or source file

#### Scenario: Removing an assignment does not mutate its source

- **WHEN** the user removes a Config Profile assignment
- **THEN** AgentDeck removes only the matching canonical deployment identity when no protected recovery state requires it
- **AND** leaves the Project configuration bytes unchanged

### Requirement: Mutation resolves only fixed Project sources

AgentDeck SHALL derive every apply and restore target from a registered Project record and the requested Agent. The Codex target MUST be `<registered-project>/.codex/config.toml`; the Claude Code target MUST be `<registered-project>/.claude/settings.json`. Management requests MUST NOT identify user scope, Claude Code project-local scope, a home directory, a project root, a source path, a current working directory, an environment, an additional filename, or a command.

#### Scenario: Codex and Claude assignments resolve distinct fixed targets

- **GIVEN** Project `project-1` has stored root `/tmp/demo`
- **WHEN** AgentDeck previews Codex and Claude Code applies for that Project
- **THEN** the targets are exactly `/tmp/demo/.codex/config.toml` and `/tmp/demo/.claude/settings.json`
- **AND** no other source is probed

#### Scenario: Symlink or special target is rejected before preview

- **WHEN** a fixed Project target is a symbolic link or a non-regular special file
- **THEN** AgentDeck returns `unsupported_symlink` or `source_invalid`
- **AND** issues no preview token and reads no link target

#### Scenario: Missing fixed target is a create candidate

- **WHEN** the parent Project root exists and the fixed target is missing
- **THEN** AgentDeck can return a create preview whose base revision is `absent`
- **AND** does not create a directory or file before confirmed apply

#### Scenario: Invalid or oversized target is not repaired

- **WHEN** a fixed target is invalid TOML or JSON or exceeds 1 MiB
- **THEN** AgentDeck returns `source_invalid` or `too_large`
- **AND** issues no preview token, backup, staged file, or source write

### Requirement: Apply requires an exact single-use typed preview

AgentDeck SHALL expose a typed apply preview before every source mutation. The preview MUST bind a single-use expiring token to profile ID and revision, registered Project ID, Agent, fixed source ID, base fingerprint or absent marker, exact transformed output hash, and allowlisted typed diff. The diff vocabulary SHALL be `same`, `added`, `changed`, and `removed`; an apply preview MUST NOT infer removal from a profile entry that is absent. Apply MUST accept only the token and MUST re-read and revalidate every bound input under the Config Profile write lock.

#### Scenario: Preview shows only explicit allowlisted changes

- **GIVEN** a target contains allowlisted `model = "gpt-5"`, unknown content, and a profile entry setting `model` to `gpt-5.1`
- **WHEN** the user requests an apply preview
- **THEN** the preview contains one typed `changed` entry from `gpt-5` to `gpt-5.1`
- **AND** contains no raw document, unknown key, unknown value, path, or backup bytes

#### Scenario: Profile omission preserves the existing setting

- **GIVEN** the target contains allowlisted key `sandbox_mode` and the selected profile has no `sandbox_mode` entry
- **WHEN** AgentDeck creates and applies the preview
- **THEN** `sandbox_mode` is absent from the diff
- **AND** its source value remains unchanged

#### Scenario: External source change invalidates preview

- **GIVEN** an apply preview was issued for source fingerprint `A`
- **WHEN** the source fingerprint becomes `B` before apply
- **THEN** apply returns `stale_preview`
- **AND** creates no recovery point, staged file, source mutation, or deployment update

#### Scenario: Profile revision change invalidates preview

- **GIVEN** an apply preview is bound to profile revision `7`
- **WHEN** the profile changes to revision `8` before apply
- **THEN** apply returns `stale_preview`
- **AND** the source and deployment state remain unchanged

#### Scenario: Token is expired or replayed

- **WHEN** apply receives an expired token or a token already consumed by a prior apply attempt
- **THEN** AgentDeck returns `preview_expired` or `stale_preview`
- **AND** starts no source mutation

### Requirement: Agent-specific transformation preserves unselected content

AgentDeck MUST transform only the profile's exact allowlisted entries. Codex transformation SHALL use TOML editing that preserves unknown keys, unknown tables, comments, and ordering. Claude Code transformation SHALL preserve every unselected top-level key and every unselected sibling beneath `permissions` while changing only exact allowlisted leaves. A transformed document MUST parse successfully and reproduce every selected typed entry before it becomes writable.

#### Scenario: Codex comments and unknown tables survive apply

- **GIVEN** a Codex target contains comments, provider configuration, an MCP table, and `model = "gpt-5"`
- **WHEN** a confirmed profile changes only `model` to `gpt-5.1`
- **THEN** the resulting TOML retains the comments, provider configuration, MCP table, and their ordering
- **AND** changes only the selected allowlisted value

#### Scenario: Claude nested permission siblings survive apply

- **GIVEN** a Claude Code target has `permissions.defaultMode = "default"`, an `allow` array, a `deny` array, and an `env` object
- **WHEN** a confirmed profile changes only `permissions.defaultMode` to `plan`
- **THEN** the resulting JSON retains the `allow`, `deny`, and `env` values
- **AND** changes only `permissions.defaultMode`

#### Scenario: Transformed output fails closed

- **WHEN** the post-transform document fails to parse or does not reproduce an exact selected typed entry
- **THEN** AgentDeck returns `write_failed`
- **AND** does not create a staged target or replace the original source

### Requirement: Apply is atomic, recoverable, and state-consistent

AgentDeck SHALL serialize Config Profile mutations under one write lock. A confirmed apply MUST create an owner-private recovery point for the original bytes or absence, write and sync a staged file in the target directory, atomically replace the target, sync the parent directory, verify the resulting fingerprint and selected entries, and commit recovery metadata plus canonical deployment state. Any failed step MUST restore the original bytes or absence and MUST NOT leave successful deployment state for an unapplied change.

#### Scenario: Successful apply records recovery and deployment state

- **WHEN** every revalidation, backup, staged write, sync, atomic replacement, verification, and SQLite commit succeeds
- **THEN** the fixed target contains the transformed document
- **AND** the latest recovery point contains the prior bytes or absent marker with owner-private permissions
- **AND** the canonical deployment stores the new fingerprint, timestamp, and clean status

#### Scenario: Fault before or after replacement rolls back

- **GIVEN** a test injects failure at recovery promotion, staged-file sync, atomic replacement, post-write verification, or SQLite commit
- **WHEN** apply runs
- **THEN** the fixed target is restored to its exact prior bytes or absence
- **AND** no successful deployment state is committed
- **AND** no staged file remains

#### Scenario: Rollback failure remains recoverable

- **WHEN** apply fails after replacement and restoring the original target also fails
- **THEN** AgentDeck returns `rollback_failed`
- **AND** retains the owner-private recovery point
- **AND** marks no deployment as successfully applied

#### Scenario: Unsupported atomic replacement fails before mutation

- **WHEN** the runtime cannot guarantee same-filesystem atomic replacement
- **THEN** AgentDeck returns `atomic_replace_unsupported`
- **AND** creates no recovery point, staged target, source mutation, or deployment update

#### Scenario: Offline Library blocks persistent management but not inspection

- **WHEN** the configured external Library is offline
- **THEN** profile CRUD, assignment, apply, and restore return `library_offline` before persistent mutation
- **AND** the existing fixed-source inventory remains available without Library synchronization or fallback write

### Requirement: Restore is previewed and conflict-safe

AgentDeck SHALL expose only the latest valid recovery point for each Config Profile, Project, and Agent deployment. Restore MUST provide an allowlisted typed preview, bind a single-use token to the current fingerprint and recovery revision, revalidate both under the Config Profile write lock, save the current bytes or absence as the next recovery point, and atomically restore the previous bytes. Raw recovery bytes MUST NOT cross the backend boundary.

#### Scenario: Existing source is restored after exact preview

- **GIVEN** a deployment has a latest recovery point and the current source matches the restore preview fingerprint
- **WHEN** the user confirms the restore token
- **THEN** AgentDeck saves the current bytes as the next recovery point
- **AND** atomically restores the previous bytes
- **AND** updates deployment fingerprint and status

#### Scenario: Created source is removed by absent recovery

- **GIVEN** the latest recovery kind is `absent` and the current target is the regular file created by the matching apply
- **WHEN** the user confirms restore
- **THEN** AgentDeck saves the current bytes as the next recovery point
- **AND** removes the created target without following a link or deleting a special file

#### Scenario: Current source change invalidates restore

- **GIVEN** a restore preview is bound to current fingerprint `C`
- **WHEN** the target changes to fingerprint `D` before restore
- **THEN** restore returns `stale_preview`
- **AND** leaves the target, recovery pointer, and deployment state unchanged

#### Scenario: Missing recovery is explicit

- **WHEN** the user requests restore for an assignment without a valid recovery point
- **THEN** AgentDeck returns `recovery_not_found`
- **AND** does not mutate the source or deployment

### Requirement: Config Profiles management is explicit and cancelable

AgentDeck SHALL provide profile list, typed editor, Project and Agent assignment controls, apply preview and confirmation, and restore preview and confirmation alongside the existing Config Profiles inventory. Management controls MUST expose only allowlisted typed fields and registered Projects. The page MUST NOT render a user-scope target, a project-local target, a raw document editor, a secret field, an arbitrary path control, a background auto-apply control, or a cross-Project batch apply action.

#### Scenario: User creates and assigns without writing a source

- **WHEN** the user saves a valid profile and assigns it to a registered Project and Agent
- **THEN** the page shows the profile revision and assignment
- **AND** no source changes until the user previews and confirms apply

#### Scenario: Apply dialog names the exact operation

- **WHEN** the user requests apply for one assignment
- **THEN** the dialog identifies the profile, Project, Agent, fixed source ID, and typed diff
- **AND** confirm submits only the preview token

#### Scenario: Cancel sends no mutation

- **WHEN** the user cancels an apply or restore preview
- **THEN** the page sends no apply or restore command
- **AND** source, profile, assignment, deployment, and recovery state remain unchanged

#### Scenario: Stale preview requires a fresh review

- **WHEN** apply or restore returns `stale_preview`
- **THEN** the page preserves the selected profile and assignment
- **AND** refreshes the inventory and requires a new preview before confirm is enabled

#### Scenario: Successful mutation refreshes all visible state

- **WHEN** apply or restore succeeds
- **THEN** the page reloads profiles, assignments, recovery availability, source status, settings, fingerprints, and typed diff
- **AND** displays no raw source or recovery content
