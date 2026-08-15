## MODIFIED Requirements

### Requirement: Existing specialized workflows remain available

AgentDeck MUST preserve Agent Skills discovery and read-only source behavior, supported non-canonical Agent targets, and existing modal dialogs outside the Library and Project Inspector flow. The sidebar MUST NOT expose a route for an Artifact workflow that has no implemented page. The Config Profiles route SHALL lead to its implemented inspection-only page and MUST NOT imply that profile mutation is available.

#### Scenario: Agent Skills workspace remains specialized

- **WHEN** the user opens an Agent Skills workspace
- **THEN** AgentDeck displays its existing discovery and read-only workflow rather than forcing the four-lane Board
- **AND** source identity and action restrictions remain unchanged

##### Example: Read-only Agent Skill source

- **GIVEN** an Agent Skill was discovered from an Agent-managed source
- **WHEN** the user opens its workspace
- **THEN** the existing read-only source view remains available and no Board deployment control replaces it

#### Scenario: Unimplemented navigation is absent

- **GIVEN** an Artifact workflow has no implemented page
- **WHEN** the sidebar renders
- **THEN** no enabled navigation item leads to an empty or non-functional management page

#### Scenario: Config Profiles navigation opens inspection

- **GIVEN** Config Profile inspection is implemented and profile mutation is not implemented
- **WHEN** the user selects Config Profiles in the sidebar
- **THEN** AgentDeck opens the Config Profiles inspection page
- **AND** the page exposes no create, edit, assign, apply, backup, or restore control
