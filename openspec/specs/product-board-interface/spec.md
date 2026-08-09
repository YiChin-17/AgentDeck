# product-board-interface Specification

## Purpose

TBD - created by archiving change 'default-product-ui-to-traditional-chinese'. Update Purpose after archive.

## Requirements

### Requirement: Light theme is the product appearance default

AgentDeck SHALL use the light theme for the initial render when local storage contains no supported theme preference. AgentDeck MUST preserve supported `light`, `dark`, and `system` preferences and MUST allow the backend setting to override the initial local value when the backend returns a supported value.

#### Scenario: Fresh installation starts in light theme

- **GIVEN** local storage has no supported theme value
- **AND** the backend has no supported theme setting
- **WHEN** AgentDeck renders its first application screen
- **THEN** the light theme is active
- **AND** the first render does not apply the dark theme class

#### Scenario: Explicit dark preference is preserved

- **GIVEN** the persisted supported theme setting is `dark`
- **WHEN** AgentDeck loads the setting
- **THEN** the dark theme is active
- **AND** AgentDeck does not rewrite the preference to `light`

#### Scenario: System preference remains supported

- **GIVEN** the persisted supported theme setting is `system`
- **WHEN** AgentDeck resolves the appearance
- **THEN** the active appearance matches `prefers-color-scheme`
- **AND** a later system appearance change updates the active appearance


<!-- @trace
source: default-product-ui-to-traditional-chinese
updated: 2026-08-10
code:
  - src/components/Layout.tsx
  - src/i18n/zh.json
  - src/views/Settings.tsx
  - scripts/check-skill-pack-ui.mjs
  - package.json
  - scripts/check-board-lanes.ts
  - src/components/PresetBar.tsx
  - src/components/Sidebar.tsx
  - src/components/CommandPalette.tsx
  - scripts/check-board-layout.mjs
  - scripts/check-i18n-locales.mjs
  - src/hooks/useTheme.ts
  - src/i18n/index.ts
  - src/components/ArtifactBoard.tsx
  - src/components/DetailSheet.tsx
  - src/index.css
  - src/views/ProjectDetail.tsx
  - src/i18n/zh-TW.json
  - src/components/ArtifactInspector.tsx
  - src/i18n/en.json
  - src/components/boardLanes.ts
  - tailwind.config.js
  - src/App.tsx
  - src/views/MySkills.tsx
-->

---
### Requirement: Artifact management defaults to a four-lane Board

AgentDeck SHALL present Library and Project artifact management as a Board with exactly four canonical lanes. The central Library Board lanes SHALL be named Library, Codex, Claude, and Both; the Project Board lanes SHALL be named Undeployed, Codex, Claude, and Both. AgentDeck MUST derive one lane per Artifact from its Codex and Claude target membership and MUST render each Artifact identity exactly once in a Board context.

#### Scenario: Canonical targets derive one lane

- **WHEN** AgentDeck maps an Artifact to the Board
- **THEN** the lane follows the canonical target table below

##### Example: Canonical target table

| Codex target | Claude target | Expected lane |
| ------------ | ------------- | ------------- |
| false | false | Library in the central Board; Undeployed in a Project Board |
| true | false | Codex |
| false | true | Claude |
| true | true | Both |

#### Scenario: Artifact identity is not duplicated

- **GIVEN** an Artifact has both Codex and Claude targets
- **WHEN** the Board renders the Artifact
- **THEN** exactly one card with that Artifact identity appears in the Both lane
- **AND** no duplicate Library, Codex, or Claude data record is created

#### Scenario: Board remains usable in a narrow desktop window

- **GIVEN** the sidebar and Inspector reduce the available Board width
- **WHEN** all four lanes cannot fit in the central region
- **THEN** the Board provides horizontal scrolling with operable fixed-width lanes
- **AND** card controls remain visible without overlapping the sidebar or Inspector

#### Scenario: Project undeployed lane remains project-local

- **GIVEN** a Project Skill targets Codex, Claude, or Both
- **WHEN** the user drops the card in the Undeployed lane
- **THEN** AgentDeck disables the Codex and Claude project targets while retaining the Project Skill
- **AND** AgentDeck does not import, update, or overwrite any central Library Skill


<!-- @trace
source: default-product-ui-to-traditional-chinese
updated: 2026-08-10
code:
  - src/components/Layout.tsx
  - src/i18n/zh.json
  - src/views/Settings.tsx
  - scripts/check-skill-pack-ui.mjs
  - package.json
  - scripts/check-board-lanes.ts
  - src/components/PresetBar.tsx
  - src/components/Sidebar.tsx
  - src/components/CommandPalette.tsx
  - scripts/check-board-layout.mjs
  - scripts/check-i18n-locales.mjs
  - src/hooks/useTheme.ts
  - src/i18n/index.ts
  - src/components/ArtifactBoard.tsx
  - src/components/DetailSheet.tsx
  - src/index.css
  - src/views/ProjectDetail.tsx
  - src/i18n/zh-TW.json
  - src/components/ArtifactInspector.tsx
  - src/i18n/en.json
  - src/components/boardLanes.ts
  - tailwind.config.js
  - src/App.tsx
  - src/views/MySkills.tsx
-->

---
### Requirement: Board target changes use drag and Inspector controls

AgentDeck MUST allow the user to change Codex and Claude targets by dragging a card to a canonical lane or by changing the Inspector target checkboxes. Both interaction paths SHALL apply the same lane mapping and MUST preserve every non-Codex and non-Claude target.

#### Scenario: Dragging to a canonical lane updates targets

- **GIVEN** an Artifact card is in any Board lane
- **WHEN** the user drops it in a canonical lane for the current Board context
- **THEN** AgentDeck persists the exact Codex and Claude target combination represented by the destination lane
- **AND** the confirmed card appears in that lane without creating a new Artifact

#### Scenario: Inspector checkboxes recompute the lane

- **GIVEN** the selected Artifact is in the Codex lane
- **WHEN** the user selects the Claude checkbox while leaving Codex selected
- **THEN** AgentDeck persists both canonical targets
- **AND** the selected card moves to the Both lane
- **AND** the Inspector remains open for the same Artifact identity

#### Scenario: Non-canonical targets survive a Board change

- **GIVEN** an Artifact targets Codex and another supported Agent
- **WHEN** the user drags the card from Codex to Claude
- **THEN** AgentDeck removes the Codex target and adds the Claude target
- **AND** the other Agent target remains unchanged

#### Scenario: Failed target update restores the confirmed state

- **GIVEN** a target mutation cannot be persisted
- **WHEN** a drag or checkbox update fails
- **THEN** AgentDeck restores the previous confirmed lane and checkbox state
- **AND** the selected Artifact remains selected
- **AND** AgentDeck displays a localized error message


<!-- @trace
source: default-product-ui-to-traditional-chinese
updated: 2026-08-10
code:
  - src/components/Layout.tsx
  - src/i18n/zh.json
  - src/views/Settings.tsx
  - scripts/check-skill-pack-ui.mjs
  - package.json
  - scripts/check-board-lanes.ts
  - src/components/PresetBar.tsx
  - src/components/Sidebar.tsx
  - src/components/CommandPalette.tsx
  - scripts/check-board-layout.mjs
  - scripts/check-i18n-locales.mjs
  - src/hooks/useTheme.ts
  - src/i18n/index.ts
  - src/components/ArtifactBoard.tsx
  - src/components/DetailSheet.tsx
  - src/index.css
  - src/views/ProjectDetail.tsx
  - src/i18n/zh-TW.json
  - src/components/ArtifactInspector.tsx
  - src/i18n/en.json
  - src/components/boardLanes.ts
  - tailwind.config.js
  - src/App.tsx
  - src/views/MySkills.tsx
-->

---
### Requirement: Cards remain concise and preserve source content

Board cards SHALL display the Artifact title, type, relevant status, target indicators, and a summary clamped to at most two visual lines. AgentDeck MUST NOT truncate, rewrite, or persist the card summary over the source description or Skill document.

#### Scenario: Long description does not expand a card

- **GIVEN** an Artifact has a description longer than two visual lines
- **WHEN** the Board card renders
- **THEN** the summary occupies at most two visual lines
- **AND** the complete description remains unchanged in persistent data

#### Scenario: Missing summary has a localized empty value

- **GIVEN** an Artifact has no display summary or description
- **WHEN** the Board card renders
- **THEN** the card displays the localized unavailable value
- **AND** AgentDeck does not generate or persist replacement content


<!-- @trace
source: default-product-ui-to-traditional-chinese
updated: 2026-08-10
code:
  - src/components/Layout.tsx
  - src/i18n/zh.json
  - src/views/Settings.tsx
  - scripts/check-skill-pack-ui.mjs
  - package.json
  - scripts/check-board-lanes.ts
  - src/components/PresetBar.tsx
  - src/components/Sidebar.tsx
  - src/components/CommandPalette.tsx
  - scripts/check-board-layout.mjs
  - scripts/check-i18n-locales.mjs
  - src/hooks/useTheme.ts
  - src/i18n/index.ts
  - src/components/ArtifactBoard.tsx
  - src/components/DetailSheet.tsx
  - src/index.css
  - src/views/ProjectDetail.tsx
  - src/i18n/zh-TW.json
  - src/components/ArtifactInspector.tsx
  - src/i18n/en.json
  - src/components/boardLanes.ts
  - tailwind.config.js
  - src/App.tsx
  - src/views/MySkills.tsx
-->

---
### Requirement: Selected Artifact opens a docked Inspector

AgentDeck SHALL open the selected Artifact in a fixed right-side Inspector while leaving the sidebar and Board context visible. The Inspector MUST display every available value among the full description, when-to-use guidance, canonical and other targets, deployment mode, source path, synchronization state, and diff action.

#### Scenario: Card opens the Inspector without covering navigation

- **WHEN** the user clicks a card or focuses it and presses Enter
- **THEN** the Inspector opens for that Artifact on the right side
- **AND** the sidebar remains visible and operable
- **AND** the Board remains visible in the remaining central region

#### Scenario: Optional metadata is absent

- **GIVEN** the selected Artifact has no when-to-use value or diff
- **WHEN** the Inspector renders
- **THEN** the missing textual value is shown as localized unavailable content
- **AND** an unavailable diff action is not presented as executable
- **AND** AgentDeck does not infer missing content from the summary

#### Scenario: Inspector closes with keyboard

- **GIVEN** the Inspector is open
- **WHEN** the user presses Escape
- **THEN** the Inspector closes
- **AND** the Board retains its search, view, and scroll state

#### Scenario: Selected lane remains visible when Inspector opens

- **GIVEN** the selected Artifact is in a lane that would leave the visible Board region after the Inspector consumes its fixed width
- **WHEN** the Inspector opens or the selected Artifact moves to another lane
- **THEN** AgentDeck scrolls the Board horizontally until the selected Artifact lane is visible
- **AND** the Inspector does not overlay the Board or reset the Board to its first lane


<!-- @trace
source: default-product-ui-to-traditional-chinese
updated: 2026-08-10
code:
  - src/components/Layout.tsx
  - src/i18n/zh.json
  - src/views/Settings.tsx
  - scripts/check-skill-pack-ui.mjs
  - package.json
  - scripts/check-board-lanes.ts
  - src/components/PresetBar.tsx
  - src/components/Sidebar.tsx
  - src/components/CommandPalette.tsx
  - scripts/check-board-layout.mjs
  - scripts/check-i18n-locales.mjs
  - src/hooks/useTheme.ts
  - src/i18n/index.ts
  - src/components/ArtifactBoard.tsx
  - src/components/DetailSheet.tsx
  - src/index.css
  - src/views/ProjectDetail.tsx
  - src/i18n/zh-TW.json
  - src/components/ArtifactInspector.tsx
  - src/i18n/en.json
  - src/components/boardLanes.ts
  - tailwind.config.js
  - src/App.tsx
  - src/views/MySkills.tsx
-->

---
### Requirement: Skill Packs are reusable mixed-skill collections

AgentDeck SHALL call user-facing Presets "Skill Packs" in English and "Skill 包" in Traditional Chinese. A Skill Pack MUST allow central Skills from different sources, series, and purposes to coexist in one named membership collection. Editing membership MUST NOT copy Skill content or mutate Agent deployment targets; applying a Skill Pack to a workspace SHALL use the existing one-time batch deployment behavior.

#### Scenario: Mixed Skills belong to one Skill Pack

- **GIVEN** the central Library contains `spectra-apply`, a Git-installed Skill, and a locally imported Skill
- **WHEN** the user adds all three Skills to one Skill Pack
- **THEN** the Skill Pack records all three memberships without duplicating their central content

#### Scenario: Browsing a Skill Pack does not deploy it

- **WHEN** the user selects a Skill Pack in the sidebar to view or edit its members
- **THEN** AgentDeck does not add or remove any Agent target
- **AND** deployment occurs only through an explicit workspace Skill Pack action

#### Scenario: Internal Preset compatibility is preserved

- **GIVEN** existing Preset records, frontend types, IPC commands, and CLI commands
- **WHEN** the user-facing Skill Pack terminology is introduced
- **THEN** AgentDeck reads and writes the existing records without migration
- **AND** existing internal Preset identifiers remain compatible


<!-- @trace
source: default-product-ui-to-traditional-chinese
updated: 2026-08-10
code:
  - src/components/Layout.tsx
  - src/i18n/zh.json
  - src/views/Settings.tsx
  - scripts/check-skill-pack-ui.mjs
  - package.json
  - scripts/check-board-lanes.ts
  - src/components/PresetBar.tsx
  - src/components/Sidebar.tsx
  - src/components/CommandPalette.tsx
  - scripts/check-board-layout.mjs
  - scripts/check-i18n-locales.mjs
  - src/hooks/useTheme.ts
  - src/i18n/index.ts
  - src/components/ArtifactBoard.tsx
  - src/components/DetailSheet.tsx
  - src/index.css
  - src/views/ProjectDetail.tsx
  - src/i18n/zh-TW.json
  - src/components/ArtifactInspector.tsx
  - src/i18n/en.json
  - src/components/boardLanes.ts
  - tailwind.config.js
  - src/App.tsx
  - src/views/MySkills.tsx
-->

---
### Requirement: Skill Pack selection is scoped to the Library

AgentDeck SHALL show a Skill Pack as selected in the sidebar only while the central Library route is displaying that pack's membership context. Project, Agent, Settings, and other routes MUST display only their own active navigation or workspace item while preserving the most recently viewed Skill Pack for later return.

#### Scenario: Project navigation has one selected context

- **GIVEN** the user previously viewed the Default Skill Pack
- **WHEN** the user opens a Project workspace
- **THEN** the Project item is selected in the sidebar
- **AND** the Default Skill Pack does not retain selected styling
- **AND** returning to the central Library restores the previously viewed Skill Pack


<!-- @trace
source: default-product-ui-to-traditional-chinese
updated: 2026-08-10
code:
  - src/components/Layout.tsx
  - src/i18n/zh.json
  - src/views/Settings.tsx
  - scripts/check-skill-pack-ui.mjs
  - package.json
  - scripts/check-board-lanes.ts
  - src/components/PresetBar.tsx
  - src/components/Sidebar.tsx
  - src/components/CommandPalette.tsx
  - scripts/check-board-layout.mjs
  - scripts/check-i18n-locales.mjs
  - src/hooks/useTheme.ts
  - src/i18n/index.ts
  - src/components/ArtifactBoard.tsx
  - src/components/DetailSheet.tsx
  - src/index.css
  - src/views/ProjectDetail.tsx
  - src/i18n/zh-TW.json
  - src/components/ArtifactInspector.tsx
  - src/i18n/en.json
  - src/components/boardLanes.ts
  - tailwind.config.js
  - src/App.tsx
  - src/views/MySkills.tsx
-->

---
### Requirement: Skill Pack deployment actions are explicit and safe

AgentDeck SHALL expose separately labeled add and remove actions for each non-empty Skill Pack in a workspace. The Skill Pack label itself MUST NOT silently toggle between adding and removing deployments. Adding MUST create only missing Skill-Agent deployments in the current workspace scope. Removing MUST target only matching Skill-Agent deployments in the current workspace scope and MUST require confirmation that identifies the Skill Pack and exact matching item count. Both actions MUST NOT delete central Skills or change Skill Pack membership.

#### Scenario: Add action skips existing deployments

- **GIVEN** a Skill Pack contains three Skills for two selected Agents and two of the six Skill-Agent deployments already exist
- **WHEN** the user chooses Add this Skill Pack
- **THEN** AgentDeck adds the four missing deployments
- **AND** AgentDeck leaves the two existing deployments unchanged

#### Scenario: Remove action requires scoped confirmation

- **GIVEN** five deployed Skill-Agent items in the current workspace match the selected Skill Pack
- **WHEN** the user chooses Remove this Skill Pack
- **THEN** AgentDeck presents a confirmation containing the Skill Pack name and the exact count `5`
- **AND** the confirmation states that central Skills and Skill Pack membership remain unchanged
- **AND** no removal mutation occurs before confirmation

#### Scenario: Canceling removal changes nothing

- **GIVEN** the Skill Pack removal confirmation is open
- **WHEN** the user cancels the confirmation
- **THEN** AgentDeck sends no removal mutation
- **AND** workspace deployments, central Skills, Skill Pack membership, and visible status remain unchanged

#### Scenario: Confirmed removal affects only matching workspace deployments

- **GIVEN** the same central Skills are deployed in the current Project and another workspace
- **WHEN** the user confirms removal of the Skill Pack from the current Project
- **THEN** AgentDeck removes only matching deployments from the current Project Agent scope
- **AND** deployments in the other workspace remain unchanged
- **AND** central Skills and Skill Pack membership remain unchanged


<!-- @trace
source: default-product-ui-to-traditional-chinese
updated: 2026-08-10
code:
  - src/components/Layout.tsx
  - src/i18n/zh.json
  - src/views/Settings.tsx
  - scripts/check-skill-pack-ui.mjs
  - package.json
  - scripts/check-board-lanes.ts
  - src/components/PresetBar.tsx
  - src/components/Sidebar.tsx
  - src/components/CommandPalette.tsx
  - scripts/check-board-layout.mjs
  - scripts/check-i18n-locales.mjs
  - src/hooks/useTheme.ts
  - src/i18n/index.ts
  - src/components/ArtifactBoard.tsx
  - src/components/DetailSheet.tsx
  - src/index.css
  - src/views/ProjectDetail.tsx
  - src/i18n/zh-TW.json
  - src/components/ArtifactInspector.tsx
  - src/i18n/en.json
  - src/components/boardLanes.ts
  - tailwind.config.js
  - src/App.tsx
  - src/views/MySkills.tsx
-->

---
### Requirement: Board and List share state and operations

AgentDeck SHALL retain a List view that uses the same Artifact identities, search query, target mutations, selection, and Inspector as the Board. Switching between Board and List MUST NOT mutate Artifact or target data.

#### Scenario: View switch preserves state

- **GIVEN** a search query and selected Artifact are active in Board view
- **WHEN** the user switches to List view
- **THEN** the same filtered Artifact set is displayed
- **AND** the same Artifact remains selected in the Inspector
- **AND** no target mutation is issued


<!-- @trace
source: default-product-ui-to-traditional-chinese
updated: 2026-08-10
code:
  - src/components/Layout.tsx
  - src/i18n/zh.json
  - src/views/Settings.tsx
  - scripts/check-skill-pack-ui.mjs
  - package.json
  - scripts/check-board-lanes.ts
  - src/components/PresetBar.tsx
  - src/components/Sidebar.tsx
  - src/components/CommandPalette.tsx
  - scripts/check-board-layout.mjs
  - scripts/check-i18n-locales.mjs
  - src/hooks/useTheme.ts
  - src/i18n/index.ts
  - src/components/ArtifactBoard.tsx
  - src/components/DetailSheet.tsx
  - src/index.css
  - src/views/ProjectDetail.tsx
  - src/i18n/zh-TW.json
  - src/components/ArtifactInspector.tsx
  - src/i18n/en.json
  - src/components/boardLanes.ts
  - tailwind.config.js
  - src/App.tsx
  - src/views/MySkills.tsx
-->

---
### Requirement: App shell uses the light Board visual hierarchy

AgentDeck SHALL use a fixed left sidebar, a central content region, and an optional fixed right Inspector. The light theme MUST use neutral light surfaces, subtle borders and shadows, a blue primary action color, and distinct blue, orange, and purple cues for Codex, Claude, and Both while retaining readable equivalent states in the dark theme.

#### Scenario: Fresh Library Board matches the required hierarchy

- **GIVEN** AgentDeck starts with no saved theme preference
- **WHEN** the Library Board renders
- **THEN** the fixed sidebar, top toolbar, four Board lanes, and light surfaces are visible
- **AND** primary actions use the blue action token
- **AND** Codex, Claude, and Both lane cues are visually distinct

#### Scenario: Secondary filters remain visible below the Library toolbar

- **GIVEN** Library source, tag, or Preset filters are available
- **WHEN** the Library Board renders without scrolling
- **THEN** every filter row is fully visible below the context toolbar
- **AND** no filter control is clipped by or overlaps the sticky toolbar boundary

#### Scenario: Board content does not paint through the sticky header

- **GIVEN** a Library or Project Board is longer than the available window height
- **WHEN** the user scrolls vertically
- **THEN** the context header and toolbar remain on an opaque top layer
- **AND** lane headings and cards remain hidden until they pass below the sticky layer
- **AND** no Board content appears above or through the sticky header

#### Scenario: Dark theme remains functional

- **GIVEN** the user explicitly selects the dark theme
- **WHEN** the Board and Inspector render
- **THEN** text, focus, active, disabled, status, and lane cue states remain distinguishable
- **AND** all Board and Inspector operations remain available


<!-- @trace
source: default-product-ui-to-traditional-chinese
updated: 2026-08-10
code:
  - src/components/Layout.tsx
  - src/i18n/zh.json
  - src/views/Settings.tsx
  - scripts/check-skill-pack-ui.mjs
  - package.json
  - scripts/check-board-lanes.ts
  - src/components/PresetBar.tsx
  - src/components/Sidebar.tsx
  - src/components/CommandPalette.tsx
  - scripts/check-board-layout.mjs
  - scripts/check-i18n-locales.mjs
  - src/hooks/useTheme.ts
  - src/i18n/index.ts
  - src/components/ArtifactBoard.tsx
  - src/components/DetailSheet.tsx
  - src/index.css
  - src/views/ProjectDetail.tsx
  - src/i18n/zh-TW.json
  - src/components/ArtifactInspector.tsx
  - src/i18n/en.json
  - src/components/boardLanes.ts
  - tailwind.config.js
  - src/App.tsx
  - src/views/MySkills.tsx
-->

---
### Requirement: Existing specialized workflows remain available

AgentDeck MUST preserve Agent Skills discovery and read-only source behavior, supported non-canonical Agent targets, and existing modal dialogs outside the Library and Project Inspector flow. The sidebar MUST NOT expose routes for Artifact types that are not implemented.

#### Scenario: Agent Skills workspace remains specialized

- **WHEN** the user opens an Agent Skills workspace
- **THEN** AgentDeck displays its existing discovery and read-only workflow rather than forcing the four-lane Board
- **AND** source identity and action restrictions remain unchanged

#### Scenario: Unimplemented navigation is absent

- **GIVEN** Plugin, Hook, or Config Profile management is not implemented
- **WHEN** the sidebar renders
- **THEN** no enabled navigation item leads to an empty or non-functional management page

<!-- @trace
source: default-product-ui-to-traditional-chinese
updated: 2026-08-10
code:
  - src/components/Layout.tsx
  - src/i18n/zh.json
  - src/views/Settings.tsx
  - scripts/check-skill-pack-ui.mjs
  - package.json
  - scripts/check-board-lanes.ts
  - src/components/PresetBar.tsx
  - src/components/Sidebar.tsx
  - src/components/CommandPalette.tsx
  - scripts/check-board-layout.mjs
  - scripts/check-i18n-locales.mjs
  - src/hooks/useTheme.ts
  - src/i18n/index.ts
  - src/components/ArtifactBoard.tsx
  - src/components/DetailSheet.tsx
  - src/index.css
  - src/views/ProjectDetail.tsx
  - src/i18n/zh-TW.json
  - src/components/ArtifactInspector.tsx
  - src/i18n/en.json
  - src/components/boardLanes.ts
  - tailwind.config.js
  - src/App.tsx
  - src/views/MySkills.tsx
-->