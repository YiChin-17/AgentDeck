## MODIFIED Requirements

### Requirement: User-facing desktop identity is AgentDeck

The desktop application and official distribution surfaces SHALL use `AgentDeck` as the current product name. This includes bundle metadata, the main window, application menu, tray menu, HTML title, Settings version and diagnostics content, App-owned locale text, the primary repository overview, release workflow labels, release titles, release notes, DMG filenames, checksum filenames, and hosted release assets. These surfaces MUST NOT present `Skills Manager` as the current product or official release name.

#### Scenario: User launches the desktop application

- **WHEN** the user launches a packaged AgentDeck build
- **THEN** the Dock, main window, application menu, and tray menu identify the App as `AgentDeck`
- **AND** no current-product label on those surfaces identifies it as `Skills Manager`

#### Scenario: User inspects Settings and diagnostics

- **WHEN** the user opens Settings and prepares diagnostic content
- **THEN** the version and diagnostic headings identify the App as `AgentDeck`
- **AND** operational instructions use the AgentDeck product name except where an external integration requires its actual legacy name

#### Scenario: User views an official release

- **WHEN** a user views or downloads a tagged official release
- **THEN** the workflow, release title, notes, DMG, checksum, and embedded application identify the product as `AgentDeck`
- **AND** `Skills Manager` appears only in explicit upstream attribution or a preserved external compatibility contract
