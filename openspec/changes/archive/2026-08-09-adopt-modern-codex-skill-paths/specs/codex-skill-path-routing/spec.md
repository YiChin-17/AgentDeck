## ADDED Requirements

### Requirement: Modern Codex paths are the deployment defaults

AgentDeck SHALL use `.agents/skills` as the Codex deployment root at both global and project scope when no corresponding user override exists.

#### Scenario: Global Codex deployment uses the modern default

- **WHEN** no global Codex path override exists
- **THEN** the Codex global deployment root is `~/.agents/skills`

#### Scenario: Project Codex deployment uses the modern default

- **WHEN** no project-relative Codex path override exists for repository `/workspace/demo`
- **THEN** the Codex project deployment root is `/workspace/demo/.agents/skills`

### Requirement: Legacy Codex paths remain discovery-only sources

AgentDeck SHALL scan `.codex/skills` for existing Codex Skills through scanner-based global discovery and project workspace discovery, and SHALL NOT select that legacy root as a deployment target unless the user explicitly configures it as an override. The Agent Skills workspace listing and its local Skill actions are outside this requirement.

#### Scenario: Global legacy Skill remains visible in scanner discovery

- **GIVEN** a valid Skill exists only at `~/.codex/skills/legacy-tool`
- **WHEN** AgentDeck performs scanner-based global Skill discovery
- **THEN** the Skill is returned as a Codex discovery result with its legacy source path

#### Scenario: Project legacy Skill remains visible

- **GIVEN** a valid Skill exists only at `/workspace/demo/.codex/skills/legacy-tool`
- **WHEN** AgentDeck scans Codex Skills for `/workspace/demo`
- **THEN** the Skill is returned as a Codex project Skill with its legacy source path

#### Scenario: Discovery does not migrate legacy content

- **GIVEN** a valid Skill exists at `.codex/skills/legacy-tool`
- **WHEN** AgentDeck discovers that Skill
- **THEN** AgentDeck does not create, move, rewrite, or delete any Skill directory

### Requirement: Equivalent Codex discovery results are deduplicated

AgentDeck MUST scan each canonical Codex root at most once and SHALL present one visible Skill result when modern and legacy roots contain the same Skill name and content. AgentDeck SHALL preserve distinct results when the content differs.

#### Scenario: Symlinked roots are scanned once

- **GIVEN** the modern and legacy Codex roots resolve to the same canonical directory
- **WHEN** AgentDeck performs discovery
- **THEN** the directory is traversed once and each contained Skill produces one discovery result

#### Scenario: Identical copies produce one visible result

- **GIVEN** `shared-tool` has the same content hash in modern and legacy Codex roots
- **WHEN** AgentDeck performs discovery
- **THEN** one visible `shared-tool` result is produced
- **AND** global discovery retains both source locations in that result

#### Scenario: Conflicting copies remain visible

- **GIVEN** `shared-tool` has different content hashes in modern and legacy Codex roots
- **WHEN** AgentDeck performs discovery
- **THEN** both results remain available with their respective source paths

### Requirement: User overrides retain deployment precedence

AgentDeck MUST use the existing global absolute override and project-relative override as the Codex primary deployment roots. Clearing an override SHALL restore the corresponding `.agents/skills` default, and legacy discovery SHALL remain enabled.

#### Scenario: Global override controls deployment

- **GIVEN** `custom_tool_paths["codex"]` is `/custom/codex-skills`
- **WHEN** AgentDeck resolves the Codex global deployment root
- **THEN** the result is `/custom/codex-skills`
- **AND** the legacy global root remains discovery-only

#### Scenario: Project override controls deployment

- **GIVEN** `custom_tool_project_paths["codex"]` is `.custom/codex-skills`
- **WHEN** AgentDeck resolves the Codex project deployment root for `/workspace/demo`
- **THEN** the result is `/workspace/demo/.custom/codex-skills`
- **AND** the legacy project root remains discovery-only

#### Scenario: Override equal to legacy root is not scanned twice

- **GIVEN** the Codex override resolves to the same canonical directory as `.codex/skills`
- **WHEN** AgentDeck performs discovery
- **THEN** that root is traversed once

#### Scenario: Reset restores modern defaults

- **WHEN** the user clears both Codex path overrides
- **THEN** global deployment resolves to `~/.agents/skills`
- **AND** project deployment resolves to `<repo>/.agents/skills`

### Requirement: Other agent path behavior is unchanged

AgentDeck SHALL preserve the deployment and discovery roots of every non-Codex built-in and custom adapter.

#### Scenario: Non-Codex adapters retain configured paths

- **WHEN** the Codex path routing change is applied
- **THEN** tests for Claude Code and all existing adapter path contracts continue to pass without expected-value changes
