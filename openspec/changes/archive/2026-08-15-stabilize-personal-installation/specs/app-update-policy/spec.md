## MODIFIED Requirements

### Requirement: Personal installation is the documented release policy

The project plan and personal installation documentation MUST define AgentDeck as a local personal-installation project without application auto-update. The documentation MUST distinguish a locally generated bundle from an upstream or publicly distributed release and MUST NOT claim public release hosting, distribution signing, notarization, binary update trust, or an application update service. Any public distribution or application update trust MUST require a separate future change before it becomes an implementation or documentation requirement.

#### Scenario: Maintainer reads the stabilization phase

- **WHEN** a maintainer reads Phase 7 of the project plan
- **THEN** local build, packaged smoke, stabilization, backup, and uninstall verification remain in scope
- **AND** public distribution and application auto-update are not listed as current completion criteria

#### Scenario: User reads the personal installation documentation

- **WHEN** a user reads the local build, installation, or troubleshooting instructions
- **THEN** the documentation identifies the artifact as a personal local build without application auto-update
- **AND** it does not attribute upstream signing, notarization, hosting, or binary update trust to that artifact
- **AND** it does not instruct the user to disable Gatekeeper or system security checks
