// T22 CodexPluginMarketDialog 批次:硬编码中文 → pluginMarket.* keys
import fs from "node:fs";
const p = "src/components/aihubmix/CodexPluginMarketDialog.tsx";
let s = fs.readFileSync(p, "utf8");

const pairs = [
  [`codexMarketError(cause, "无法加载插件列表，请检查网络后重试。")`, `codexMarketError(cause, t("pluginMarket.error.loadFailed"))`],
  [`codexMarketError(cause, "操作未能确认，请刷新列表检查状态后再试。")`, `codexMarketError(cause, t("pluginMarket.error.unconfirmed"))`],
  [`codexMarketError(
            cause,
            "无法启用插件目录，请检查网络和 Codex 安装后重试。",
          )`, `codexMarketError(
            cause,
            t("pluginMarket.error.enableFailed"),
          )`],
  [`            setup?.officialReady === false
              ? "社区目录已启用。官方插件同步未完成，可点「同步官方插件」重试。"
              : prepareLocal
                ? "本机插件库已就绪。重新打开 Codex 后即可在社区插件页安装。"
                : "插件目录已启用。可在这里按需安装，或从 AIHub 启动桌面端后直接安装。",`, `            setup?.officialReady === false
              ? t("pluginMarket.notice.officialSyncIncomplete")
              : prepareLocal
                ? t("pluginMarket.notice.localReady")
                : t("pluginMarket.notice.enabled"),`],
  [`        else setError("插件目录已设置，但偏好保存失败；请重新检查设置。");`, `        else setError(t("pluginMarket.error.prefSaveFailed"));`],
  [`          ? "已安装。请在桌面端连接自己的服务账号，再新建会话使用。"
          : plugin.installed
            ? "已卸载。请新建会话或重启 Codex 以更新插件。"
            : "已安装。请新建会话或重启 Codex 以加载插件。"`, `          ? t("pluginMarket.notice.installedConnectAuth")
          : plugin.installed
            ? t("pluginMarket.notice.uninstalled")
            : t("pluginMarket.notice.installed")`],
  [`          <DialogTitle>Codex 插件</DialogTitle>`, `          <DialogTitle>{t("pluginMarket.title")}</DialogTitle>`],
  [`              aria-label="关闭插件管理"
              title="关闭插件管理"`, `              aria-label={t("pluginMarket.aria.close")}
              title={t("pluginMarket.aria.close")}`],
  [`          安装后新建会话或重启 Codex；部分插件需要单独登录。`, `          {t("pluginMarket.description")}`],
  [`          社区插件可能读取本机文件或运行代码，安装前请确认来源。`, `          {t("pluginMarket.warning")}`],
  [`            aria-label="搜索插件"
            placeholder="搜索插件名称或用途"`, `            aria-label={t("pluginMarket.aria.search")}
            placeholder={t("pluginMarket.placeholder.search")}`],
  [`            正在加载插件列表…`, `            {t("pluginMarket.status.loading")}`],
  [`              刷新列表`, `              {t("pluginMarket.action.refresh")}`],
  [`            <p className="text-caption text-fg-3">
              找到 {matches.length} 个插件
              {matches.length > visibleCount
                ? \`，已显示 \${visibleCount} 个\`
                : ""}
            </p>`, `            <p className="text-caption text-fg-3">
              {t("pluginMarket.status.found", { count: matches.length })}
              {matches.length > visibleCount
                ? t("pluginMarket.status.shownMore", { count: visibleCount })
                : ""}
            </p>`],
  [`              <span>
                从 AIHub 启动桌面端即可按需安装。也可提前下载完整插件库
                {catalog.downloadBytes > 0 &&
                  \`（约 \${Math.ceil(catalog.downloadBytes / 1048576)} MiB）\`}
                。
              </span>`, `              <span>
                {t("pluginMarket.hint.download", {
                  sizeInfo:
                    catalog.downloadBytes > 0
                      ? \`（约 \${Math.ceil(catalog.downloadBytes / 1048576)} MiB）\`
                      : "",
                })}
              </span>`],
  [`                同步官方插件`, `                {t("pluginMarket.action.syncOfficial")}`],
  [`                准备 Codex 插件库`, `                {t("pluginMarket.action.prepareLibrary")}`],
  [`                <span>插件状态暂不可用；启用目录后可查看和管理。</span>`, `                <span>{t("pluginMarket.label.statusUnavailable")}</span>`],
  [`                  {busy === "enable" ? "正在启用…" : "启用插件目录"}`, `                  {busy === "enable"
                    ? t("pluginMarket.status.enabling")
                    : t("pluginMarket.action.enableCatalog")}`],
  [`                没有找到匹配的插件。`, `                {t("pluginMarket.empty.noMatches")}`],
  [`                      {plugin.inApiCatalog === false
                        ? "官方入口需登录 ChatGPT"
                        : plugin.requiresAuth
                          ? "可能需要授权"
                          : "授权要求以插件说明为准"}`, `                      {plugin.inApiCatalog === false
                        ? t("pluginMarket.auth.loginRequired")
                        : plugin.requiresAuth
                          ? t("pluginMarket.auth.mayRequire")
                          : t("pluginMarket.auth.seeDescription")}`],
  [`                            .catch(() =>
                              setError("无法打开 ChatGPT 插件目录。"),
                            )`, `                            .catch(() =>
                              setError(t("pluginMarket.error.openCatalogFailed")),
                            )`],
  [`                        {plugin.inApiCatalog === false
                          ? "前往 ChatGPT 连接"
                          : "官方入口"}`, `                        {plugin.inApiCatalog === false
                          ? t("pluginMarket.action.gotoChatgpt")
                          : t("pluginMarket.action.officialEntry")}`],
  [`                      {busy === plugin.id
                        ? "处理中…"
                        : plugin.installed === null
                          ? "启用后查看"
                          : plugin.installed
                            ? "卸载"
                            : "安装"}`, `                      {busy === plugin.id
                        ? t("pluginMarket.status.working")
                        : plugin.installed === null
                          ? t("pluginMarket.action.viewAfterEnable")
                          : plugin.installed
                            ? t("pluginMarket.action.uninstall")
                            : t("pluginMarket.action.install")}`],
  [`                加载更多插件`, `                {t("pluginMarket.action.loadMore")}`],
  // useTranslation(PluginMarketContent 组件内)
  [`function PluginMarketContent() {
  const searchInput = useRef<HTMLInputElement>(null);`, `function PluginMarketContent() {
  const { t } = useTranslation();
  const searchInput = useRef<HTMLInputElement>(null);`],
  [`import {
  claudeDesktop,`, `import { useTranslation } from "react-i18next";
import {
  claudeDesktop,`],
];

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
