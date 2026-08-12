## ADDED Requirements

### Requirement: Artifact identity is typed and separate from subtype details

AgentDeck SHALL persist one typed Artifact identity for every managed item and SHALL keep subtype-specific fields in subtype detail storage. The persisted Artifact kind MUST be exactly one of `skill`, `plugin`, `hook`, or `config_profile`; an unknown kind MUST be rejected and MUST NOT be interpreted as `skill`.

#### Scenario: Existing Skill receives one stable Artifact identity

- **GIVEN** a schema v7 Skill with id `skill-1`
- **WHEN** the database is upgraded to the Artifact schema
- **THEN** exactly one Artifact with id `skill-1` and kind `skill` exists
- **AND** the Skill retains id `skill-1`, its original fields, Tags, Scenario memberships, and target associations

#### Scenario: New Skill creates its parent identity atomically

- **WHEN** a caller inserts or upserts a Skill through the existing store API
- **THEN** the operation persists a kind `skill` Artifact and its Skill detail in one transaction
- **AND** failure of either write leaves neither a partial Artifact nor a partial Skill detail

#### Scenario: Invalid subtype relationship is rejected

- **GIVEN** an Artifact whose kind is `plugin`
- **WHEN** a caller attempts to attach a Skill detail to that Artifact
- **THEN** the database rejects the write
- **AND** no existing Artifact or Skill row is changed

#### Scenario: Deleting a Skill removes only its owned identity and deployments

- **GIVEN** a Skill Artifact with Tags, Scenario memberships, and deployment rows
- **WHEN** the existing Skill delete operation succeeds
- **THEN** its Artifact identity, Skill detail, Tags, memberships, and deployments are removed by enforced relationships
- **AND** unrelated Artifacts and Skills remain unchanged

### Requirement: Deployment records represent scope and execution state explicitly

AgentDeck SHALL use a canonical Artifact deployment record containing Artifact id, global or project scope, Agent, enabled state, deployment mode, source path, target path, last synchronized hash and time, status, and last error. The uniqueness key MUST be Artifact id plus scope plus Agent.

#### Scenario: Legacy target maps to a global enabled deployment

- **GIVEN** a schema v7 `skill_targets` row with id `target-1`, Skill `skill-1`, tool `codex`, mode `symlink`, target path `/tmp/project/.agents/skills/demo`, source hash `abc`, and synchronized time `1000`
- **WHEN** the database is upgraded
- **THEN** one deployment retains id `target-1`
- **AND** it has Artifact `skill-1`, global scope, Agent `codex`, enabled `true`, mode `symlink`, the original target path, source hash `abc`, and synchronized time `1000`
- **AND** its source path equals the migrated Skill central path

#### Scenario: Scope invariants reject ambiguous records

- **WHEN** a caller writes global scope with a non-empty project id, project scope with an empty project id, or a scope outside `global` and `project`
- **THEN** the write is rejected
- **AND** no deployment row is created or replaced

#### Scenario: Supported deployment modes round-trip

- **WHEN** a caller stores and reads deployments using `symlink`, `copy`, and `cli-managed`
- **THEN** each mode round-trips without normalization or loss
- **AND** any other mode is rejected

#### Scenario: Existing Skill target API preserves its contract

- **GIVEN** canonical deployments include a global enabled row, a global disabled row, and a project-scoped row for the same Skill
- **WHEN** an existing caller requests `SkillTargetRecord` values
- **THEN** only the global enabled row is returned
- **AND** `tool`, `target_path`, `mode`, `status`, `synced_at`, `last_error`, and `source_hash` retain their existing meanings and serialized field names

### Requirement: Schema v7 upgrades are atomic, lossless, and retryable

AgentDeck MUST upgrade schema v7 databases to schema v8 in one SQLite transaction. It MUST verify row counts and foreign-key integrity before removing legacy target storage or committing user version 8. Any migration error MUST restore the complete pre-migration schema and data.

#### Scenario: Populated v7 database upgrades without data loss

- **GIVEN** a valid schema v7 database containing Skills, targets, Tags, Scenarios, Scenario-Agent toggles, Projects, settings, audit entries, and pending conflicts
- **WHEN** migrations run
- **THEN** user version becomes 8
- **AND** every pre-existing field and relationship has an equivalent v8 value
- **AND** every Skill and target has exactly one corresponding Artifact and deployment
- **AND** unrelated tables and rows are byte-for-byte or value-for-value unchanged

#### Scenario: Fresh database reaches the same final schema

- **WHEN** migrations run against an empty database
- **THEN** user version becomes 8
- **AND** the Artifact, Skill detail, and deployment constraints match those of an upgraded populated database
- **AND** zero seed Artifact or deployment rows are created

#### Scenario: Migration verification failure rolls back

- **GIVEN** a schema v7 fixture that causes an Artifact count, target mapping, or foreign-key integrity check to fail
- **WHEN** migration is attempted
- **THEN** migration returns an error naming the failed invariant
- **AND** user version remains 7
- **AND** the legacy `skills` and `skill_targets` schema and rows remain intact
- **AND** no partial Artifact or deployment table is visible after rollback

#### Scenario: Completed migration is idempotent

- **GIVEN** a valid schema v8 database
- **WHEN** migrations run again
- **THEN** the schema and all row values remain unchanged
- **AND** no duplicate Artifact or deployment is created

#### Scenario: Older binary fails closed on schema v8

- **GIVEN** a binary whose latest supported schema version is 7
- **WHEN** it opens a schema v8 database
- **THEN** it rejects the database as newer than supported
- **AND** it performs no downgrade or data mutation

### Requirement: Existing Skill behavior and offline safety remain compatible

AgentDeck SHALL preserve the observable behavior and serialized contracts of existing Skills, Skill Packs, Tags, Board targets, sync operations, commands, and `skills-manager-cli`. Database migration MUST remain independent of external Library availability, while every filesystem mutation MUST continue to use the existing Library offline guard.

#### Scenario: Existing callers observe unchanged Skill data

- **GIVEN** the same pre-upgrade Skill and target dataset
- **WHEN** Library, Board, Agent Skills, Project, Skill Pack, and CLI queries run after migration
- **THEN** they return the same Skill ids, counts, target states, Tags, memberships, and existing JSON fields as before migration
- **AND** they do not expose a new required frontend or CLI field

#### Scenario: Offline external Library permits internal migration only

- **GIVEN** the configured external Library is offline and the internal database is schema v7
- **WHEN** AgentDeck starts
- **THEN** the internal database can upgrade to schema v8 without creating or modifying the configured Library path
- **AND** startup remains in the existing `library_offline` state
- **AND** any Library or deployment filesystem mutation remains blocked with no target or Library side effect

#### Scenario: Deployment status does not persist secrets

- **WHEN** AgentDeck stores an Artifact or deployment record
- **THEN** it stores only identity, type, scope, Agent, paths, mode, synchronization metadata, status, and sanitized error data
- **AND** it MUST NOT store tokens, credentials, login payloads, or command environments in Artifact or deployment columns

### Requirement: Legacy Git backup format remains unchanged in Phase 3

AgentDeck MUST preserve the existing `.skills-manager` Skill metadata layout, schema marker, merge protocol version, refs, trailers, Keychain service, and restore behavior. Phase 3 Artifact and deployment rows MUST NOT introduce non-Skill metadata files or change canonical Skill metadata bytes.

#### Scenario: Skill metadata round-trip is byte compatible

- **GIVEN** a fixed schema v7 Skill, Tags, Scenario, and membership dataset
- **WHEN** metadata is written before and after the schema v8 migration
- **THEN** canonical `.skills-manager` Skill, Scenario, membership, schema, and protocol files are byte-identical
- **AND** no Artifact or deployment metadata directory is added

#### Scenario: Existing backup restores through the Artifact store

- **GIVEN** a backup created by the schema v7 application with protocol 2 metadata
- **WHEN** the schema v8 application restores and reindexes it
- **THEN** each restored Skill receives exactly one kind `skill` Artifact identity
- **AND** existing Skill ids, paths, Tags, Scenarios, memberships, refs, and trailers remain unchanged

#### Scenario: Existing object merge behavior is preserved

- **WHEN** current protocol 2 merge, conflict, legacy-client, and pre-protocol restore fixtures run against the schema v8 store
- **THEN** their selected trees, conflict decisions, safety refs, and commit trailers remain unchanged
- **AND** Phase 3 does not increment the merge protocol version
