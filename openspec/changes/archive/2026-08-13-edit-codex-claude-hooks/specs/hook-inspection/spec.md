## RENAMED Requirements

- FROM: `### Requirement: Hooks page exposes filters, diagnostics, details, and compatibility without mutation controls`
- TO: `### Requirement: Hooks page exposes gated Hook editing without execution controls`

## MODIFIED Requirements

### Requirement: Hooks page exposes filters, diagnostics, details, and compatibility without mutation controls

AgentDeck SHALL provide a `/hooks` route and Sidebar entry. The page SHALL retain Agent, scope, event, source status, and Project filters; source diagnostics; Hook Inspector; bounded source comparison; and compatibility matrix. For writable fixed sources it SHALL additionally expose Edit, Delete, Preview, Apply, and Restore controls governed by backend validation and exact source revisions. It MUST NOT render or invoke any Hook execution, test-run, enable, or disable action.

#### Scenario: User filters and inspects a Hook

- **GIVEN** inspection returns Codex and Claude Code entries from user and project scopes
- **WHEN** the user selects Agent `codex`, scope `project`, and an event, then opens one result
- **THEN** only matching entries remain in the list
- **AND** the Inspector displays that entry's source, event, matcher, handler type, handler fields, and known or unknown markers

#### Scenario: Missing and invalid sources remain understandable

- **GIVEN** one writable fixed source is missing and one source is invalid
- **WHEN** the Hooks page loads
- **THEN** the missing source is shown as an empty state with an option to create its first handler
- **AND** the invalid source shows its sanitized source-specific diagnostic without mutation controls
- **AND** available entries and the compatibility matrix remain interactive

#### Scenario: Edit controls are limited by source capability

- **GIVEN** inspection includes a valid regular source, a symlink source, an offline source, and a too-large source
- **WHEN** the Hooks page renders
- **THEN** Edit and Delete are available only for handlers in the valid regular source
- **AND** the other sources show a localized reason that mutation is unavailable
- **AND** English and Traditional Chinese locale files contain matching keys for every new user-visible string

#### Scenario: Apply requires a current successful preview

- **GIVEN** a user changed a Hook draft
- **WHEN** backend validation has not produced a current preview with `canApply=true`
- **THEN** Apply is disabled
- **AND** clicking Preview never writes a source, backup, Artifact, or localStorage entry

#### Scenario: Draft changes invalidate an earlier preview

- **GIVEN** preview succeeded for draft revision `draft-1`
- **WHEN** the user changes any event, matcher, handler type, or editable field
- **THEN** the earlier diff and base revision are marked stale
- **AND** Apply remains disabled until preview succeeds for the new draft

#### Scenario: Project switch clears sensitive route-local state

- **GIVEN** the editor holds a draft and preview for Project `project-1`
- **WHEN** the user selects Project `project-2` while a request is pending
- **THEN** the draft, preview, selected handler, and recovery selection for `project-1` are cleared
- **AND** a late response for `project-1` does not replace `project-2` state
- **AND** no Hook content is stored in `AppContext` or localStorage

#### Scenario: Restore is previewed and never executes a Hook

- **GIVEN** a latest recovery point exists for the selected source
- **WHEN** the user requests Restore
- **THEN** the page displays the backend restore diff before enabling Restore Apply
- **AND** restore requires the current base revision
- **AND** no Hook command, prompt, HTTP endpoint, MCP tool, or agent handler is invoked
