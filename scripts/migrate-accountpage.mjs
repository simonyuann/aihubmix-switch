// T22 AccountPage 批次:硬编码中文 → accountPage.* keys
import fs from "node:fs";
const p = "src/components/aihubmix/AccountPage.tsx";
let s = fs.readFileSync(p, "utf8");

const pairs = [
  [`setError(friendlyError(e, "加载 API 列表失败，请重试。"));`, `setError(friendlyError(e, t("accountPage.error.loadKeys")));`],
  [`      setNotice("API Key 已复制");`, `      setNotice(t("accountPage.notice.keyCopied"));`],
  [`setError(friendlyError(e, "复制失败，请重试。"));`, `setError(friendlyError(e, t("accountPage.error.copyFailed")));`],
  [`\`\${friendlyError(e, "删除未能确认")}。请刷新列表确认删除结果后再重试。\``, `\`\${friendlyError(e, t("accountPage.error.deleteUnconfirmed"))}。请刷新列表确认删除结果后再重试。\``],
  [`        title="API 管理"`, `        title={t("accountPage.title")}`],
  [`                  {busy ? "正在加载…" : "还没有 API Key"}`, `                  {busy
                    ? t("accountPage.status.loading")
                    : t("accountPage.empty.noKeys")}`],
  [`        title="删除 API Key"`, `        title={t("accountPage.dialog.deleteTitle")}`],
  [`        confirmText="确认删除"`, `        confirmText={t("accountPage.dialog.confirmDelete")}`],
  [`        cancelText="取消"`, `        cancelText={t("accountPage.action.cancel")}`],
  // useTranslation
  [`import { useQuery, useQueryClient } from "@tanstack/react-query";`, `import { useQuery, useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";`],
];

// hook 注入:找组件函数体
const hookMatch = s.match(/export function AccountPage\(\) \{\n/);
if (hookMatch) {
  s = s.replace(`export function AccountPage() {
`, `export function AccountPage() {
  const { t } = useTranslation();
`);
}

let applied = 0;
const missed = [];
for (const [oldStr, newStr] of pairs) {
  if (s.includes(oldStr)) {
    s = s.split(oldStr).join(newStr);
    applied++;
  } else missed.push(oldStr.slice(0, 60).replace(/\n/g, "\\n"));
}
fs.writeFileSync(p, s);
console.log(`applied: ${applied}/${pairs.length}`);
if (missed.length) {
  console.log("MISSED:");
  for (const m of missed) console.log("  -", m);
}
