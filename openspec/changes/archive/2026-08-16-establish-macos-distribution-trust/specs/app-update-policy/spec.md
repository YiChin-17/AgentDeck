## MODIFIED Requirements

### Requirement: Personal installation is the documented release policy

The project plan and personal installation documentation MUST define AgentDeck as a local personal-installation project without application auto-update or a currently active public release channel. The documentation MUST distinguish a locally generated bundle from an upstream or publicly distributed release and MUST NOT claim current public release hosting, distribution signing, notarization, binary update trust, or an application update service. A retained release workflow, distribution checker, or official-distribution draft document MUST be treated as inactive implementation material and MUST NOT authorize public distribution, a runtime release query, an update manifest, an update public key, a binary download, an installation flow, or an application auto-update service. Any future public distribution or application update trust MUST require a separate Spectra change before it becomes a current implementation, live acceptance, or documentation requirement.

#### Scenario: Maintainer reads the completed stabilization phase

- **WHEN** a maintainer reads Phase 7 of the project plan
- **THEN** personal local build, packaged smoke, backup, and uninstall verification remain recorded as completed without public trust claims
- **AND** those acceptance results remain independently reproducible without release credentials

#### Scenario: Maintainer reads the distribution phase

- **WHEN** a maintainer reads Phase 8 of the project plan
- **THEN** the completed fail-closed workflow and repository checks are recorded as inactive implementation material
- **AND** release credential configuration, tagged acceptance, and GitHub Release publication are not current completion criteria
- **AND** application auto-update, update manifests, runtime release queries, and updater signing keys remain out of scope

#### Scenario: User reads personal installation documentation

- **WHEN** a user reads the local build, installation, or troubleshooting instructions
- **THEN** the documentation identifies the artifact as a personal local build without application auto-update or inherited official trust
- **AND** it does not instruct the user to disable Gatekeeper or system security checks

#### Scenario: User reads official distribution documentation

- **WHEN** a user encounters retained macOS distribution documentation while AgentDeck remains personal-only
- **THEN** the documentation does not claim that a current public AgentDeck release exists
- **AND** it does not claim that the running application checks, downloads, or installs releases
