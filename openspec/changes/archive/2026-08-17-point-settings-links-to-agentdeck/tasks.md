## 1. Product identity regression contract

- [x] 1.1 依「Product identity checker 驗證受管 Settings destinations」在 `scripts/check-product-identity.test.mjs` 新增先失敗的 fixtures，交付「AgentDeck-owned Settings links target the AgentDeck repository」對正確 repository、upstream regression 與合法 attribution 的判定，並以 `node --test scripts/check-product-identity.test.mjs` 驗證 finding file 與 rule。
- [x] 1.2 在 `scripts/check-product-identity.mjs` 實作受管 Settings repository destination rule，確保缺少正確 URL、回到 upstream 或 action 未使用受管 constant 都以非零狀態回報，並以 1.1 fixtures 與 `npm run check:product-identity` 驗證。

## 2. Settings destinations

- [x] 2.1 依「Settings actions 共用固定 AgentDeck repository constant」將 `src/views/Settings.tsx` 的 repository base 固定為 `https://github.com/YiChin-17/AgentDeck`，讓 GitHub 與 report-issue actions 分別開啟 repo 與既有 bug report template URL，並以 product identity fixtures、`npm run build` 與 `npm run lint` 驗證 action wiring 與編譯。
- [x] 2.2 執行 `node --test scripts/*.test.mjs` 與 `npm run check:product-identity`，確認完整 repository contracts 通過，且 upstream attribution 與 legacy compatibility exceptions 未被誤判。
