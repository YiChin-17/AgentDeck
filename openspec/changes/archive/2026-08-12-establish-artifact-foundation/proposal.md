## Why

AgentDeck 的 Board 已用 Artifact 語意呈現 Skills，但 backend identity、deployment 與 backup metadata 仍以 Skill 為唯一根型別，無法在不污染 `SkillRecord` 的前提下承接後續 Hooks、Plugins 與 Config Profiles。Phase 3 必須先建立可無損升級的通用資料基礎，同時維持現有 Skill 行為與 legacy backup contract。

## What Changes

- 新增明確的 `ArtifactKind`、Artifact identity record 與個別 detail table 關係；既有 Skill 使用原 id 成為 `skill` Artifact，不複製 identity。
- 新增可表達 global／project scope、Agent、enabled、deployment mode、source／target path、最後同步資訊與錯誤狀態的通用 deployment record。
- 建立 SQLite versioned migration，將所有既有 Skills 與 `skill_targets` 無損轉換到 Artifact 基礎模型；migration 失敗時整筆 rollback，且重跑保持 idempotent。
- 保留現有 `SkillRecord`、Scenarios／Skill Packs、Tags、commands、CLI JSON 與 UI 行為；既有 Skill callers 透過相容 API 使用新的 canonical identity 與 deployment storage。
- 固定 legacy Git backup 相容策略：Phase 3 不改 `.skills-manager` metadata layout、schema／merge protocol、refs 或 trailers；後續新增非 Skill Artifact backup 時必須另行升版並提供舊 client 邊界。
- 新增 fresh database、v7 upgrade、rollback、foreign-key integrity、Skill CRUD／deployment regression 與 legacy backup round-trip tests。

## Non-Goals

- 不新增 Hook、Plugin 或 Config Profile 的 detail schema、scanner、editor、CLI adapter、部署流程或 UI 入口。
- 不改變 Codex／Claude 路徑、symlink／copy 行為、Board lane 定義、Inspector 版面或 Library offline contract。
- 不在本 change 升級 `.skills-manager/schema.json`、merge protocol、Git refs／trailers或修改官方 Plugin cache。
- 不重新命名 `SkillStore`、`skills-manager-cli` 或任何 legacy persistence identifier。

## Capabilities

### New Capabilities

- `artifact-foundation`: 定義通用 Artifact identity、typed detail boundary、deployment record、舊 Skill 資料無損 migration 與 legacy backup 相容契約。

### Modified Capabilities

（無）

## Impact

- Plan phase: `plan.md` Phase 3「Artifact 基礎模型」。
- Affected specs: `artifact-foundation`；保留 `product-board-interface`、`external-library-availability` 與 `product-identity` 的既有 requirements。
- Intentional upstream divergence: AgentDeck 新增上層 Artifact 與通用 deployment schema；既有 upstream Skill behavior 與跨平台能力保持不變。
- Affected code:
  - New: `src-tauri/src/core/artifact.rs`
  - Modified: `src-tauri/src/core/mod.rs`
  - Modified: `src-tauri/src/core/migrations.rs`
  - Modified: `src-tauri/src/core/skill_store.rs`
  - Modified: `src-tauri/src/core/sync_metadata.rs`
  - Modified: `src-tauri/src/core/merge/integration_tests.rs`
- Dependencies: 不新增 crate 或 npm dependency。
