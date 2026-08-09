## ADDED Requirements

### Requirement: Agent workspace discovers configured global roots

AgentDeck SHALL list Skills from an agent's global primary root followed by its configured discovery-only additional roots. AgentDeck SHALL classify primary results as writable and additional-root results as read-only.

#### Scenario: Codex legacy-only Skill is listed

- **GIVEN** Codex primary root `~/.agents/skills` contains no `legacy-tool`
- **AND** a valid Skill exists at `~/.codex/skills/legacy-tool`
- **WHEN** the user opens the Codex Agent Skills view
- **THEN** `legacy-tool` is listed with path `~/.codex/skills/legacy-tool`
- **AND** the result is marked read-only

#### Scenario: Missing additional root is skipped

- **GIVEN** a configured discovery-only additional root does not exist or cannot be read
- **WHEN** AgentDeck refreshes the Agent Skills list
- **THEN** AgentDeck omits that root without creating or modifying any directory
- **AND** results from readable roots remain available

#### Scenario: Override makes the legacy directory primary

- **GIVEN** the Codex global override resolves to `~/.codex/skills`
- **WHEN** AgentDeck lists a Skill from that root
- **THEN** the result is classified as writable primary content
- **AND** the same root is not scanned again as an additional source

### Requirement: Equivalent results are deduplicated without hiding conflicts

AgentDeck MUST scan each canonical root once. Within one agent, AgentDeck SHALL retain the highest-precedence result for equal normalized name, enabled state, and content hash, and SHALL retain every same-name result whose content hash differs.

#### Scenario: Canonical root alias is scanned once

- **GIVEN** an additional root resolves to the same canonical directory as the primary root
- **WHEN** AgentDeck refreshes the Agent Skills list
- **THEN** the directory is traversed once
- **AND** its Skills are classified using primary precedence

#### Scenario: Identical modern and legacy copies prefer primary

- **GIVEN** `~/.agents/skills/shared-tool` and `~/.codex/skills/shared-tool` have the same content hash
- **WHEN** AgentDeck refreshes the Codex Agent Skills list
- **THEN** one `shared-tool` row is returned
- **AND** that row uses path `~/.agents/skills/shared-tool`
- **AND** that row is writable

#### Scenario: Conflicting same-name copies remain distinct

- **GIVEN** `~/.agents/skills/shared-tool` and `~/.codex/skills/shared-tool` have different content hashes
- **WHEN** AgentDeck refreshes the Codex Agent Skills list
- **THEN** two `shared-tool` rows are returned
- **AND** each row retains its own absolute path and root role

### Requirement: Actions use verified source identity

AgentDeck MUST identify an Agent Skills row by its server-returned absolute Skill path. Before every document, import, pull, or delete operation, AgentDeck MUST refresh the agent's allowed roots and match that exact path to a scanned result.

#### Scenario: Same relative path selects the requested source

- **GIVEN** modern and legacy roots each contain a different `shared-tool`
- **WHEN** the client requests the document for path `~/.codex/skills/shared-tool`
- **THEN** AgentDeck returns the legacy document
- **AND** AgentDeck does not read `~/.agents/skills/shared-tool`

#### Scenario: Unscanned path is rejected

- **GIVEN** `/tmp/untrusted/skill` was not returned by the agent's fresh root scan
- **WHEN** the client submits that path to an Agent Skills action
- **THEN** AgentDeck returns a not-found error
- **AND** AgentDeck performs no filesystem or Library mutation

#### Scenario: Source disappears before action

- **GIVEN** a Skill path was returned by the list command
- **AND** the Skill or its root is removed before the next action
- **WHEN** the client submits the stale path
- **THEN** AgentDeck returns a not-found error
- **AND** AgentDeck does not fall back to a same-name Skill in another root

### Requirement: Discovery-only sources remain read-only

AgentDeck SHALL allow document viewing and Library import from a read-only result. AgentDeck SHALL NOT pull center content into, delete, or remove a managed primary target through a read-only result.

#### Scenario: Legacy document can be viewed

- **GIVEN** a valid read-only legacy Skill contains `SKILL.md`
- **WHEN** the user opens its detail view
- **THEN** AgentDeck returns the document from the verified legacy path
- **AND** no source file is modified

#### Scenario: Legacy Skill imports without deployment

- **GIVEN** a valid read-only legacy Skill is not in the central Library
- **WHEN** the user imports it
- **THEN** AgentDeck creates the corresponding central Library Skill
- **AND** AgentDeck does not modify the legacy source
- **AND** AgentDeck does not create a global target or primary deployment

#### Scenario: Legacy pull is rejected

- **GIVEN** a read-only legacy Skill matches a center Skill with newer center content
- **WHEN** a client directly invokes the pull command for the legacy path
- **THEN** AgentDeck returns an invalid-input error
- **AND** the legacy source remains unchanged

#### Scenario: Legacy delete is rejected

- **GIVEN** a read-only legacy Skill exists
- **WHEN** a client directly invokes the delete command for the legacy path
- **THEN** AgentDeck returns an invalid-input error
- **AND** the legacy source remains unchanged

### Requirement: Primary source behavior remains unchanged

AgentDeck SHALL preserve existing document, import, target registration, pull, delete, and sync-status behavior for Skills resolved from the writable primary root.

#### Scenario: Primary import retains managed target behavior

- **GIVEN** a writable primary Skill is not in the central Library
- **WHEN** the user imports it from Agent Skills
- **THEN** AgentDeck imports the Skill using the existing primary workflow
- **AND** the resulting managed global target behavior remains unchanged

#### Scenario: Primary pull and delete remain available

- **GIVEN** a writable primary Skill satisfies the existing pull or delete preconditions
- **WHEN** the user invokes the corresponding action
- **THEN** AgentDeck performs the existing primary action
- **AND** read-only guards do not change its result

### Requirement: Source state is visible in UI

The Agent Skills UI SHALL use each Skill's absolute path as the row and action-state identity. The UI SHALL display a localized read-only source indicator and SHALL hide mutating source actions for read-only rows.

#### Scenario: Conflicting rows have independent UI state

- **GIVEN** modern and legacy roots contain different `shared-tool` Skills
- **WHEN** both rows are displayed
- **THEN** each row has a distinct UI key and action loading state derived from its absolute path
- **AND** opening either row displays that row's actual path and document

#### Scenario: Read-only row exposes safe actions only

- **GIVEN** a legacy Skill row is marked read-only
- **WHEN** the row and detail view are rendered
- **THEN** the UI displays the localized read-only indicator
- **AND** document viewing and import remain available
- **AND** pull, delete, and remove-managed actions are absent
