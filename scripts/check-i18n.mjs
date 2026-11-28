#!/usr/bin/env node
// T10 防回归:所有 locale 的 key 集合必须与 en 完全一致(含孤立 key 检查)。
// 运行:pnpm check:i18n
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const dir = path.join(path.dirname(fileURLToPath(import.meta.url)), "../src/i18n/locales");
const files = fs.readdirSync(dir).filter((f) => f.endsWith(".json"));
function flatKeys(obj, prefix = "") {
  const keys = [];
  for (const [k, v] of Object.entries(obj)) {
    const p = prefix ? `${prefix}.${k}` : k;
    if (v && typeof v === "object" && !Array.isArray(v)) keys.push(...flatKeys(v, p));
    else keys.push(p);
  }
  return keys;
}
const locales = {};
for (const f of files) locales[f.replace(".json", "")] = flatKeys(JSON.parse(fs.readFileSync(path.join(dir, f), "utf8")));
const reference = locales.en;
if (!reference) {
  console.error("check-i18n: 缺少基准 locale en.json");
  process.exit(1);
}
const refSet = new Set(reference);
let failed = false;
for (const [name, keys] of Object.entries(locales)) {
  if (name === "en") continue;
  const set = new Set(keys);
  const missing = reference.filter((k) => !set.has(k));
  const extra = keys.filter((k) => !refSet.has(k));
  if (missing.length || extra.length) {
    failed = true;
    console.error(`[check-i18n] ${name} 与 en 不一致: 缺 ${missing.length} 个, 多 ${extra.length} 个`);
    if (missing.length) console.error(`  缺失: ${missing.join(", ")}`);
    if (extra.length) console.error(`  多余(孤立 key,请删除或补到 en): ${extra.join(", ")}`);
  }
}
if (failed) process.exit(1);
console.log(`check-i18n: ${Object.keys(locales).length} 个 locale 全部一致 (${reference.length} keys)`);
