// T22 散点批次:HomeImageModelSelect/CodexMarketProgress/DiagnosticReport 三文件迁移
import fs from "node:fs";
function migrate(p, pairs, hookAnchor, hookLine) {
  let s = fs.readFileSync(p, "utf8");
  let ok = 0;
  for (const [o, n] of pairs) {
    if (s.includes(o)) { s = s.split(o).join(n); ok++; }
    else console.log(`SKIP ${p}:`, o.slice(0, 56).replace(/\n/g, "\\n"));
  }
  if (!s.includes("useTranslation")) {
    s = s.replace(hookAnchor, `${hookAnchor}\n${hookLine}`);
    console.log(`hook added to ${p}`);
  }
  fs.writeFileSync(p, s);
  console.log(`${p}: ${ok}/${pairs.length} fixed`);
}

// 1. HomeImageModelSelect
migrate(
  "src/components/aihubmix/HomeImageModelSelect.tsx",
  [
    [`          生图模型`, `          {t("imageModelSelect.label")}`],
    [`{model ? modelName(model, selected?.name) : "跟随网关（默认）"}`, `{model ? modelName(model, selected?.name) : t("imageModelSelect.gatewayDefault")}`],
    [`<SelectItem value={gateway}>跟随网关（默认）</SelectItem>`, `<SelectItem value={gateway}>{t("imageModelSelect.gatewayDefault")}</SelectItem>`],
    [`<span className="text-caption text-fg-3">正在读取模型…</span>`, `<span className="text-caption text-fg-3">{t("imageModelSelect.status.loadingModels")}</span>`],
    [`<span role="alert">读取分组生图模型失败。</span>`, `<span role="alert">{t("imageModelSelect.error.loadFailed")}</span>`],
    [`            重试
          </Button>`, `            {t("imageModelSelect.action.retry")}
          </Button>`],
    [`          当前分组已不提供所选模型，请重新选择。`, `          {t("imageModelSelect.error.modelUnavailable")}`],
  ],
  `import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";`,
  `import { useTranslation } from "react-i18next";`,
);

// 2. CodexMarketProgress
migrate(
  "src/components/aihubmix/CodexMarketProgress.tsx",
  [
    [`  const label = !progress
    ? "正在准备插件库…"
    : progress.phase === "checking"
      ? "正在准备插件库…"
      : progress.phase === "downloading"
        ? !progress.total
          ? \`正在下载插件库，已下载 \${mib(progress.downloaded)} MiB\`
          : \`正在下载插件库：\${mib(progress.downloaded)} / \${mib(progress.total)} MiB（\${percent}%）\`
        : progress.phase === "verifying"
          ? "正在安装插件库，请稍候…"
          : "正在安装插件库，请稍候…";`, `  const label = !progress
    ? t("codexMarket.progress.preparing")
    : progress.phase === "checking"
      ? t("codexMarket.progress.preparing")
      : progress.phase === "downloading"
        ? !progress.total
          ? t("codexMarket.progress.downloadingNoTotal", { downloaded: mib(progress.downloaded) })
          : t("codexMarket.progress.downloadingWithTotal", {
              downloaded: mib(progress.downloaded),
              total: mib(progress.total),
              percent,
            })
        : progress.phase === "verifying"
          ? t("codexMarket.progress.installing")
          : t("codexMarket.progress.installing");`],
    [`      aria-label="插件库下载进度"`, `      aria-label={t("codexMarket.aria.downloadProgress")}`],
    [`import type { CodexMarketProgress as Progress } from "@/lib/codexPluginMarket";`, `import { useTranslation } from "react-i18next";
import type { CodexMarketProgress as Progress } from "@/lib/codexPluginMarket";`],
    [`  const mib = (bytes: number) => (bytes / 1024 / 1024).toFixed(1);`, `  const { t } = useTranslation();
  const mib = (bytes: number) => (bytes / 1024 / 1024).toFixed(1);`],
  ],
  `import type { CodexMarketProgress as Progress } from "@/lib/codexPluginMarket";`,
  `import { useTranslation } from "react-i18next";`,
);

// 3. DiagnosticReport
migrate(
  "src/components/aihubmix/DiagnosticReport.tsx",
  [
    [`        aria-label="网络诊断结果"`, `        aria-label={t("diagnosticReport.aria.report")}`],
    [`            () => onNotice("已复制诊断结果"),`, `            () => onNotice(t("diagnosticReport.toast.copied")),`],
    [`            () => onNotice("请选中诊断结果并复制。"),`, `            () => onNotice(t("diagnosticReport.toast.copyFailed")),`],
    [`        复制诊断结果`, `        {t("diagnosticReport.action.copy")}`],
  ],
  `import { Button } from "@/components/ui/button";`,
  `import { useTranslation } from "react-i18next";`,
);
console.log("scatter batch done");
