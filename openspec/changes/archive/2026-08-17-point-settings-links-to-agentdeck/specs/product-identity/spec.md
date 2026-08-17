## ADDED Requirements

### Requirement: AgentDeck-owned Settings links target the AgentDeck repository

The Settings repository action SHALL open `https://github.com/YiChin-17/AgentDeck`, and the Settings issue-report action SHALL open the bug report creation flow under that same repository. The repository product-identity check MUST reject an upstream Skills Manager destination on either AgentDeck-owned Settings action while preserving upstream URLs used for explicit attribution or provenance.

#### Scenario: User opens the project repository

- **WHEN** the user activates the GitHub repository action in Settings
- **THEN** AgentDeck opens `https://github.com/YiChin-17/AgentDeck`
- **AND** the action does not open the upstream Skills Manager repository

#### Scenario: User reports an issue

- **WHEN** the user completes the Settings report-issue action
- **THEN** AgentDeck opens `https://github.com/YiChin-17/AgentDeck/issues/new?template=bug_report.md`
- **AND** the existing diagnostic copy flow remains available

#### Scenario: Settings destination regresses to upstream

- **GIVEN** either AgentDeck-owned Settings action resolves to `https://github.com/xingkongliang/skills-manager`
- **WHEN** the product-identity check runs
- **THEN** the check exits with a non-zero status
- **AND** the finding identifies `src/views/Settings.tsx` and the repository-destination rule

#### Scenario: Upstream attribution remains present

- **GIVEN** the upstream repository URL appears only in an approved attribution or provenance surface
- **WHEN** the product-identity check runs
- **THEN** that occurrence does not violate the Settings repository-destination rule
