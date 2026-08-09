#!/usr/bin/env node
// Locale integrity check for src/i18n.
// Verifies leaf key parity, interpolation placeholder parity, and the approved
// Taiwan product glossary for zh-TW. Uses only the Node standard library so the
// check stays runnable without adding a test framework dependency.
import fs from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const localeDir = path.join(root, 'src', 'i18n');

// en is the source language: every other locale is measured against its key set.
const BASELINE = 'en';
const LOCALES = ['en', 'zh-TW'];

// Glossary locale: only AgentDeck-owned zh-TW translations are held to the
// Taiwan product terminology. User-authored Skill content is never checked here.
const GLOSSARY_LOCALE = 'zh-TW';
const REQUIRED_UI_KEYS = ['common.close', 'common.refresh'];
const SKILL_PACK_TERMS = {
  en: 'Skill Pack',
  'zh-TW': 'Skill 包',
};

// Only unambiguous replacements belong here. Terms with a legitimate Taiwan
// reading are deliberately excluded so the check does not flag correct text:
//   「項目」 means "item" in Taiwan usage (「沒有符合項目」), not only "project".
//   「文件」 means "document" in Taiwan usage (「檢視文件」), not only "file".
//   「支持」 and 「提交」 are ordinary Taiwan words in a product UI.
const PROHIBITED_TERMS = [
  { term: '本地', use: '本機', reason: 'local' },
  { term: '倉庫', use: '儲存庫', reason: 'repository' },
  // 「應用程式」 is the real macOS folder name and standard Taiwan wording, so it
  // stays allowed; bare 「應用」 as a noun for AgentDeck itself becomes 「App」.
  { term: '應用', use: 'App', reason: 'application', allowedIn: ['應用程式'] },
  { term: '設置', use: '設定', reason: 'settings' },
  { term: '全局', use: '全域', reason: 'global' },
  { term: '只讀', use: '唯讀', reason: 'read-only' },
  { term: '導入', use: '匯入', reason: 'import' },
  { term: '導出', use: '匯出', reason: 'export' },
  { term: '默認', use: '預設', reason: 'default' },
  { term: '缺省', use: '預設', reason: 'default' },
  { term: '用戶', use: '使用者', reason: 'user' },
  { term: '克隆', use: '複製', reason: 'clone' },
  { term: '網絡', use: '網路', reason: 'network' },
  { term: '軟件', use: '軟體', reason: 'software' },
  { term: '硬件', use: '硬體', reason: 'hardware' },
  { term: '信息', use: '訊息', reason: 'message' },
  { term: '刷新', use: '重新整理', reason: 'refresh' },
  { term: '菜單', use: '選單', reason: 'menu' },
  { term: '內存', use: '記憶體', reason: 'memory' },
  { term: '緩存', use: '快取', reason: 'cache' },
  { term: '屏幕', use: '螢幕', reason: 'screen' },
  { term: '視頻', use: '影片', reason: 'video' },
  { term: '質量', use: '品質', reason: 'quality' },
  { term: '選項卡', use: '分頁', reason: 'tab' },
  { term: '打印', use: '列印', reason: 'print' },
];

function readLocale(locale) {
  const filePath = path.join(localeDir, `${locale}.json`);
  return JSON.parse(fs.readFileSync(filePath, 'utf8'));
}

function flatten(value, prefix = '', out = new Map()) {
  for (const [key, child] of Object.entries(value)) {
    const fullKey = prefix ? `${prefix}.${key}` : key;
    if (child && typeof child === 'object' && !Array.isArray(child)) {
      flatten(child, fullKey, out);
    } else {
      out.set(fullKey, child);
    }
  }
  return out;
}

function placeholders(value) {
  if (typeof value !== 'string') return new Set();
  const found = value.matchAll(/\{\{\s*([^}]+?)\s*\}\}/g);
  return new Set(Array.from(found, (m) => m[1]));
}

function sorted(set) {
  return Array.from(set).sort();
}

// Counts occurrences of `term` that are not part of an allowed longer word.
function prohibitedHits(text, { term, allowedIn = [] }) {
  let hits = 0;
  let from = 0;
  for (;;) {
    const at = text.indexOf(term, from);
    if (at === -1) return hits;
    const covered = allowedIn.some((allowed) => {
      const offset = allowed.indexOf(term);
      if (offset === -1) return false;
      return text.startsWith(allowed, at - offset);
    });
    if (!covered) hits += 1;
    from = at + term.length;
  }
}

const errors = [];

const simplifiedLocalePath = path.join(localeDir, 'zh.json');
if (fs.existsSync(simplifiedLocalePath)) {
  errors.push('[policy] Simplified Chinese locale must not exist: src/i18n/zh.json');
}

const i18nSource = fs.readFileSync(path.join(localeDir, 'index.ts'), 'utf8');
if (/from\s+['"]\.\/zh\.json['"]/.test(i18nSource)) {
  errors.push('[policy] src/i18n/index.ts must not import the Simplified Chinese locale');
}
if (/\bzh\s*:\s*\{\s*translation\s*:/.test(i18nSource)) {
  errors.push('[policy] src/i18n/index.ts must not register a zh resource');
}
if (!/lang\s*===\s*['"]zh['"]\s*\?\s*['"]zh-TW['"]/.test(i18nSource)) {
  errors.push('[policy] legacy zh preferences must normalize to zh-TW');
}

const settingsSource = fs.readFileSync(
  path.join(root, 'src', 'views', 'Settings.tsx'),
  'utf8',
);
if (/value\s*:\s*['"]zh['"]/.test(settingsSource)) {
  errors.push('[policy] Settings must not expose a Simplified Chinese option');
}

const tauriSource = fs.readFileSync(path.join(root, 'src', 'lib', 'tauri.ts'), 'utf8');
if (!/export interface Preset\s*\{/.test(tauriSource) || !/getPresets/.test(tauriSource)) {
  errors.push('[compatibility] internal Preset types and APIs must remain available');
}

const flatLocales = new Map(
  LOCALES.map((locale) => [locale, flatten(readLocale(locale))]),
);
const baseline = flatLocales.get(BASELINE);

for (const [locale, values] of flatLocales) {
  for (const key of REQUIRED_UI_KEYS) {
    if (!values.has(key)) {
      errors.push(`[${locale}] missing required product UI key: ${key}`);
    }
  }

  const expectedTerm = SKILL_PACK_TERMS[locale];
  if (!Array.from(values.values()).some((value) =>
    typeof value === 'string' && value.includes(expectedTerm)
  )) {
    errors.push(`[${locale}] user-facing Skill Pack terminology is missing: ${expectedTerm}`);
  }

  for (const [key, value] of values) {
    const visibleValue = typeof value === 'string'
      ? value.replace(/\{\{\s*[^}]+\s*\}\}/g, '')
      : value;
    if (typeof visibleValue === 'string' && /\bpreset(s)?\b/i.test(visibleValue)) {
      errors.push(`[${locale}] legacy user-facing Preset terminology at ${key}: ${value}`);
    }
  }
}

for (const locale of LOCALES) {
  if (locale === BASELINE) continue;
  const target = flatLocales.get(locale);

  for (const key of baseline.keys()) {
    if (!target.has(key)) {
      errors.push(`[${locale}] missing key: ${key}`);
    }
  }
  for (const key of target.keys()) {
    if (!baseline.has(key)) {
      errors.push(`[${locale}] extra key not present in ${BASELINE}: ${key}`);
    }
  }

  for (const [key, value] of target) {
    if (!baseline.has(key)) continue;
    const expected = placeholders(baseline.get(key));
    const actual = placeholders(value);
    const missing = sorted(expected).filter((name) => !actual.has(name));
    const unexpected = sorted(actual).filter((name) => !expected.has(name));
    if (missing.length || unexpected.length) {
      const parts = [];
      if (missing.length) parts.push(`missing {{${missing.join('}}, {{')}}}`);
      if (unexpected.length) {
        parts.push(`unexpected {{${unexpected.join('}}, {{')}}}`);
      }
      errors.push(`[${locale}] placeholder mismatch at ${key}: ${parts.join('; ')}`);
    }
  }
}

for (const [key, value] of flatLocales.get(GLOSSARY_LOCALE)) {
  if (typeof value !== 'string') continue;
  for (const rule of PROHIBITED_TERMS) {
    if (prohibitedHits(value, rule) > 0) {
      errors.push(
        `[${GLOSSARY_LOCALE}] prohibited term 「${rule.term}」 at ${key}: ` +
          `use 「${rule.use}」 for ${rule.reason} — ${value}`,
      );
    }
  }
}

if (errors.length) {
  console.error(`Locale integrity check failed with ${errors.length} error(s):`);
  for (const error of errors) console.error(`  ${error}`);
  process.exit(1);
}

console.log(`Locale integrity check passed for ${LOCALES.join(', ')}.`);
