## 1. TDD fixtures 與契約

- [x] 1.1 先為「Hook discovery reads only fixed user and linked-project sources」與 design「固定來源描述器與 project id 邊界」在 `src-tauri/src/core/hook_inspection.rs` tests 建立 failing fixtures，固定無 Project 的 3 個 user descriptors、已關聯 Project 的 4 個 project descriptors、穩定 source ids及unknown project rejection；逐支執行 named tests，確認實作前至少一支因module或API尚不存在而 FAILED。
- [x] 1.2 先為「Each Hook source is parsed and diagnosed independently」與 design「來源隔離 parser 與 canonical Hook fragment」建立 Codex `hooks.json`／inline TOML、Claude Code三層JSON、missing、invalid JSON／TOML、invalid UTF-8、permission denied、multi-handler ordering與unknown values failing tests；以named tests固定單一來源錯誤不遮蔽其他來源及unknown fields逐值保留。
- [x] 1.3 先為「Inspection responses exclude non-Hook configuration and persistence」及「Source comparison is bounded and same-Agent only」與 design「限制讀取與 diff 成本」建立sentinel secret、1,048,576／1,048,577-byte、262,144／262,145-byte及4,000／4,001-line boundary tests；驗證非Hook siblings不進DTO／diagnostic且超限狀態與`diff_available`在實作前 FAILED。
- [x] 1.4 先為「Compatibility matrix is explicit and snapshot-based」與 design「文件快照驅動的 compatibility registry」建立2026-08-12官方event／handler fixtures，固定三態support、Agent-specific notes及`FutureEvent`不會升級成supported；以named registry tests確認實作前 constants尚不存在而 FAILED。

## 2. Core discovery、parser 與 registry

- [x] 2.1 依「固定來源描述器與 project id 邊界」在 `src-tauri/src/core/hook_inspection.rs`實作typed Agent／scope／format／source descriptors，從home與`SkillStore::get_project_by_id`產生固定paths且unknown project回傳`invalid_project`；以1.1 tests證明不接受frontend path、不fallback到cwd。
- [x] 2.2 依「來源隔離 parser 與 canonical Hook fragment」加入`toml_edit`並實作JSON／TOML Hook subtree extraction、deterministic source order、ordered `HookEntryDto` fields與per-source diagnostics，使invalid source不短路且unknown values不丟失；以1.2全部tests及`cargo check --manifest-path src-tauri/Cargo.toml --locked`驗證。
- [x] 2.3 依「限制讀取與 diff 成本」實作1 MiB read cap、canonical Hook-only text及256 KiB／4,000-line diff gates，確保有效超限來源仍列entries而missing／invalid／too-large不進line diff；以1.3 boundary tests驗證每個臨界值。
- [x] 2.4 依「文件快照驅動的 compatibility registry」實作typed event／handler rows、`supported`／`unsupported`／`unknown` cells與Agent-specific notes，未知discovery值只標記entry；以1.4 registry fixture完整比對Codex與Claude Code官方快照。

## 3. 唯讀 Tauri contract 與資料邊界

- [x] 3.1 依 design「唯讀 Tauri DTO 與 Hooks UI」在 `src-tauri/src/commands/hooks.rs`、`src-tauri/src/commands/mod.rs`、`src-tauri/src/core/mod.rs`及`src-tauri/src/lib.rs`註冊`get_hook_inspection(project_id)`，回傳sources／entries／compatibility／selected project／generated time；以command tests斷言固定serialized enum strings、typed `invalid_project`及所有來源狀態。
- [x] 3.2 強制「Inspection responses exclude non-Hook configuration and persistence」：command不得log或persist Hook content，完整serialized response與diagnostics排除含`sentinel-secret`的非Hook siblings，呼叫前後fixture files與SQLite row dump相同；以DTO serialization與read-only side-effect tests逐值比較驗證。

## 4. Hooks route、Inspector 與 comparison UI

- [x] 4.1 依 design「唯讀 Tauri DTO 與 Hooks UI」在 `src/lib/tauri.ts`定義精確DTO／`getHookInspection` wrapper，並在`src/App.tsx`、`src/components/Sidebar.tsx`、`src/views/Hooks.tsx`接上`/hooks`與Project selector；以`npm run build`確認route與IPC shape可編譯，且未新增全域`AppContext` state。
- [x] 4.2 完成「Hooks page exposes filters, diagnostics, details, and compatibility without mutation controls」的source cards、Agent／scope／event／status filters、missing／invalid／too-large states與latest-request-wins guard；以`check:hooks-ui` fixtures及manual rapid Project switch確認舊response不覆蓋新選擇、頁面沒有mutation／execution controls。
- [x] 4.3 在 `src/components/HookInspector.tsx`顯示source／event／matcher／handler fields與unknown markers，並讓`src/views/Hooks.tsx`只把不同且同Agent的`diff_available` pair送入`DocumentDiffViewer`，同頁呈現compatibility matrix；以static contract與manual fixtures驗證same-Agent diff、cross-Agent guard、limit reason及Agent-specific notes。
- [x] 4.4 依 design「靜態 UI 契約與雙語邊界」新增`scripts/check-hooks-ui.mjs`、`package.json`的`check:hooks-ui`及`src/i18n/en.json`／`src/i18n/zh-TW.json`完整文案，固定route、Sidebar、filters、Inspector、Compare guards與matrix wiring；執行`npm run check:hooks-ui`及`npm run check:i18n`要求exit 0。

## 5. 完整驗證與唯讀證據

- [x] 5.1 執行`cargo test --manifest-path src-tauri/Cargo.toml --locked`、`npm run build`、`npm run lint`、`npm run check:i18n`、`npm run check:hooks-ui`與`git diff --check`，要求Rust 0 failed且所有commands exit 0；以temporary HOME／linked Project的Codex JSON／TOML與Claude三層fixtures手動載入、篩選、inspect及diff，前後比較config bytes、SQLite dump、Library tree hash與Git status，確認沒有database migration、Hook執行、外部寫入、backup protocol或既有Skill功能變更。
