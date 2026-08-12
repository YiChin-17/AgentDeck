## 1. Legacy compatibility 安全網

- [x] 1.1 先為「Bundle identity migration preserves core data」新增 failing focused tests／path assertions，固定 internal Library、external configured Library、SQLite、central repo config、Git backup metadata 與 Keychain service 在 Bundle ID 切換前後解析不變，並覆蓋 external offline 不 fallback／不 mutation；以指定 Rust tests 的 pass count 與 temporary directory 前後 hash 驗證核心資料未搬移、建立或刪除。
- [x] 1.2 依「Legacy persistence 與 protocol identifiers 保持不變」及「Legacy compatibility identifiers remain unchanged」建立 source／integration assertions，固定 `.skills-manager`、`skills-manager.db`、`refs/skills-manager/*`、`Skills-Manager-*`、`skills-manager-git-backup`、既有 localStorage keys 與 `skills-manager-cli` contract；以 backup protocol tests、`npm run cli:build` 及 CLI JSON smoke test 驗證沒有平行 `.agentdeck` protocol 或命令破壞。

## 2. AgentDeck desktop 身份

- [x] 2.1 依「AgentDeck 成為唯一 display name 並使用穩定 Bundle ID」與「Desktop bundle identity is stable」將 Tauri Bundle ID 固定為 `io.github.yichin17.agentdeck`，將 npm／Cargo desktop package與 default binary 改為 `agentdeck`，同時保留 `app_lib` 與 explicit `skills-manager-cli`；以 metadata assertion、`npm run build`、`cargo check --manifest-path src-tauri/Cargo.toml --locked` 及 CLI build 驗證各 binary 可解析。
- [x] 2.2 依「User-facing desktop identity is AgentDeck」更新 main window、HTML title、App menu、Tray、Settings version／diagnostics、en／zh-TW App-owned translations與主要 README header，使一般產品 surfaces 只顯示 AgentDeck；以新增的 failing display-name assertions、`npm run check:i18n` 與人工 surfaces checklist 驗證沒有上游產品名稱殘留。
- [x] 2.3 依「OAuth 與 Skill CLI 保留為明確例外」及「Upstream and external integration names remain explicit exceptions」更新有限說明與 `plan.md` identity boundary，保留 attribution、MIT License、baseline、OAuth 真實撤銷名稱及 legacy Skill CLI；以 README／plan content review、`git remote -v` 與例外清單 assertion 驗證產品身份清楚但操作指引仍正確。

## 3. AgentDeck icon 資產

- [x] 3.1 依「AgentDeck icon 使用單一原始圖稿產生 desktop assets」及「AgentDeck uses independent desktop icon assets」建立至少 1024×1024、無文字、藍紫高對比、四張層疊 Artifact cards／deck 輪廓的 AgentDeck-owned 無損 master，記錄上游 master hash供回歸比較；以 dimensions／alpha inspection、hash inequality 與 16／32／128 px render review 驗證獨立性及小尺寸辨識度。
- [x] 3.2 從核准 master 以 Tauri 既有 icon tooling 產生通用 PNG、`.icns`、`.ico` 與列於 proposal 的 Windows Square assets，並更新 README icon，使所有 desktop targets 使用同一產品圖像；以檔案存在／非空／dimensions assertion、Tauri bundle metadata inspection 與 macOS Dock 人工檢查驗證產物。
- [x] 3.3 建立相同輪廓的單色透明 Tray source與 16／20／24／32 px outputs，在 macOS 啟用 template mode且保留其他平台對比；以 alpha／monochrome assertion及 macOS 深色、淺色選單列人工截圖確認圖標皆可辨識。

- [x] 3.4 依「Icons are reviewed at desktop sizes」補上 16 px 專用的簡化圖稿，讓層疊輪廓在縮到 16 px 後仍可辨識，並只套用在 `icon.icns` 的 `is32`／`s8mk`／`ic11` slot 上，保留已核准的 master、32 px 以上產物與同時作為側邊欄 logo 的 `32x32.png`；以 16／32／128 px render review、icns slot bytes 比對與 product identity check 驗證只有 16 px 改變。

## 4. Product identity 防回歸

- [x] 4.1 先為「Product identity check 使用明確 surfaces 與 legacy allowlist」及「Repository checks enforce product identity boundaries」建立錯誤 display name、錯誤 Bundle ID、錯誤 package、舊 icon hash、缺失 asset與合法 legacy exception fixtures，再實作 `scripts/check-product-identity.mjs` 及 package script；以每個違規 fixture 非零且指出檔案／規則、合法 allowlist fixture成功驗證檢查邊界。

## 5. 切換與驗收

- [x] 5.1 執行 `npm run build`、`npm run lint`、`npm run check:i18n`、product identity check、`npm run cli:build` 與 `cargo test --manifest-path src-tauri/Cargo.toml`，記錄 exit status、Rust test pass／fail count及 Tauri resolved metadata，確認 branding、bundle、CLI與 legacy contracts 同時成立。
- [x] 5.2 關閉舊 App 後，以既有 internal Library、external online Library及 external offline Library各啟動新 Bundle ID build，逐項檢查 Dock、window、App menu、Tray、Settings、資料數量、backup credential與 offline guard；保存人工結果並只提供「確認新 App 正常後自行移除舊 bundle」指引，不自動刪除 App 或使用者資料。
