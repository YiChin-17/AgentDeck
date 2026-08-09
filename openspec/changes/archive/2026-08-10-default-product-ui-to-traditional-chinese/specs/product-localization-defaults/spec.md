## ADDED Requirements

### Requirement: Traditional Chinese is the product default

AgentDeck SHALL initialize i18n with `zh-TW` when neither the backend setting nor local storage contains a supported language. AgentDeck MUST use `zh-TW` as the fallback resource for missing translation keys.

#### Scenario: Fresh installation starts in Traditional Chinese

- **GIVEN** the backend has no `language` setting
- **AND** local storage has no `language` value
- **WHEN** AgentDeck initializes i18n
- **THEN** the active language is `zh-TW`
- **AND** the first rendered application UI uses the `zh-TW` resource

#### Scenario: Invalid preferences fail closed to Traditional Chinese

- **GIVEN** the backend language value and local storage language value are absent or unsupported
- **WHEN** AgentDeck initializes i18n
- **THEN** the active language is `zh-TW`
- **AND** initialization completes without surfacing an application startup error

#### Scenario: Missing key falls back to Traditional Chinese

- **GIVEN** the active supported locale does not contain a requested translation key
- **AND** the `zh-TW` resource contains that key
- **WHEN** the UI requests the translation
- **THEN** AgentDeck renders the `zh-TW` value
- **AND** AgentDeck does not render the `zh` fallback value

### Requirement: Explicit supported language preferences are preserved

AgentDeck MUST resolve language preference in this order: a supported backend setting, a supported local storage value, then `zh-TW`. AgentDeck SHALL support only explicit `zh-TW` and `en` selections, SHALL NOT expose or load the Simplified Chinese resource, and MUST normalize a legacy `zh` preference from either persistence layer to `zh-TW`.

#### Scenario: Backend preference wins

- **GIVEN** the backend setting is `en`
- **AND** local storage contains `zh-TW`
- **WHEN** AgentDeck initializes i18n
- **THEN** the active language is `en`

#### Scenario: Local preference is used when backend preference is unavailable

- **GIVEN** the backend language setting is absent, unsupported, or cannot be read
- **AND** local storage contains `zh-TW`
- **WHEN** AgentDeck initializes i18n
- **THEN** the active language is `zh-TW`

#### Scenario: Legacy Simplified Chinese preference is normalized

- **GIVEN** the backend setting or local storage contains the legacy value `zh`
- **WHEN** AgentDeck initializes i18n
- **THEN** the active language is `zh-TW`
- **AND** no Simplified Chinese resource is loaded

#### Scenario: Settings exclude Simplified Chinese

- **WHEN** the user opens the language setting
- **THEN** the only available choices are Traditional Chinese and English
- **AND** no control can persist `zh`

#### Scenario: Manual selection survives restart

- **GIVEN** the user selects `zh-TW` or `en` in Settings
- **WHEN** the selection is persisted and AgentDeck restarts
- **THEN** AgentDeck restores that selected language
- **AND** the new `zh-TW` default does not override it

### Requirement: Traditional Chinese uses Taiwan product terminology

The `zh-TW` resource MUST use the approved Taiwan terminology for AgentDeck-owned user interface text while preserving technical names such as `Skill`, `Agent`, `Library`, Git, CLI, API, JSON, and TOML.

#### Scenario: Taiwan terminology is rendered

- **WHEN** the user views the `zh-TW` interface
- **THEN** AgentDeck uses 「本機」、「儲存庫」、「App」、「專案」、「設定」、「全域」、「唯讀」 and 「匯入／匯出」 for their corresponding product concepts
- **AND** AgentDeck-owned text does not use the prohibited equivalents defined by the locale integrity check

#### Scenario: User content is not rewritten

- **GIVEN** a Skill or Agent document contains terminology outside the approved product glossary
- **WHEN** AgentDeck renders that user-authored content
- **THEN** AgentDeck preserves the original content
- **AND** the product glossary is applied only to AgentDeck-owned translations

### Requirement: Locale resources preserve structural integrity

AgentDeck MUST provide a repeatable locale integrity command that verifies leaf translation keys, interpolation placeholders, and the approved Traditional Chinese terminology rules without adding a third-party runtime or test dependency.

#### Scenario: Locale resources are structurally aligned

- **WHEN** the locale integrity command checks the committed resources
- **THEN** `zh-TW` has the required leaf translation keys
- **AND** each corresponding value uses the same interpolation placeholder names
- **AND** the command exits successfully when all terminology rules pass

#### Scenario: Missing placeholder fails validation

- **GIVEN** a `zh-TW` value omits or renames a placeholder present in the corresponding baseline value
- **WHEN** the locale integrity command runs
- **THEN** the command exits with a non-zero status
- **AND** the output identifies the affected translation key and placeholder mismatch

#### Scenario: Prohibited product term fails validation

- **GIVEN** an AgentDeck-owned `zh-TW` translation contains a prohibited term from the approved glossary
- **WHEN** the locale integrity command runs
- **THEN** the command exits with a non-zero status
- **AND** the output identifies the affected translation key and prohibited term
