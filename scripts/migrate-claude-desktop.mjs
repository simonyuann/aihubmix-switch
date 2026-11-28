// T22 ClaudeDesktopControls 批次:硬编码中文 → claudeDesktop.* keys
import fs from "node:fs";
const p = "src/components/aihubmix/ClaudeDesktopControls.tsx";
let s = fs.readFileSync(p, "utf8");

// phaseLabels:模块级映射改为 key 表(渲染处 t())
const phasePairs = [
  ["preparing", "检查应用与语言资源"],
  ["authorizing", "等待系统授权"],
  ["checking_backup", "检查旧补丁与备份"],
  ["cleaning", "清理旧补丁组件"],
  ["closing", "关闭 Claude 与后台服务"],
  ["permissions", "准备应用目录写入权限"],
  ["copying", "创建应用副本"],
  ["extracting", "解包界面资源"],
  ["translating", "汉化界面文字"],
  ["resources", "写入中文语言资源"],
  ["packing", "重新打包与更新完整性信息"],
  ["signing", "重新签名应用"],
  ["verifying", "校验应用文件"],
  ["replacing", "备份并安装应用"],
  ["restoring", "恢复原版文件"],
  ["launching", "启动 Claude Desktop"],
  ["completed", "补丁处理完成"],
  ["cancelled", "已取消"],
];
for (const [id, zh] of phasePairs) {
  s = s.replace(`${id}: "${zh}",`, `${id}: "claudeDesktop.phase.${id}",`);
}

// 精确 old→new 对
const pairs = [
  [`      setMessage("取消请求未能送达，请重试。");`, `      setMessage(t("claudeDesktop.message.cancelFailed"));`],
  [`          aria-label="处理步骤"`, `          aria-label={t("claudeDesktop.aria.steps")}`],
  [`              {phaseLabels[step]}`, `              {t(phaseLabels[step])}`],
  [`          {failed ? "未完成 · " : ""}
          {phaseLabel}`, `          {failed ? t("claudeDesktop.status.failedPrefix") : ""}
          {t(phaseLabel)}`],
  [`        <span className="shrink-0 tabular-nums text-fg-3">
          已用 {Math.floor(elapsed / 60)}分{elapsed % 60}秒
        </span>`, `        <span className="shrink-0 tabular-nums text-fg-3">
          {t("claudeDesktop.progress.elapsed", {
            minutes: Math.floor(elapsed / 60),
            seconds: elapsed % 60,
          })}
        </span>`],
  [`        aria-label={phaseLabel}`, `        aria-label={t(phaseLabel)}`],
  [`          {phase === "signing" ? "已签名" : "已处理"} {status.completed} /{" "}
          {status.total} 项 · 当前阶段 {percent}%
        </p>`, `          {t("claudeDesktop.progress.countedLine", {
            label: t(phase === "signing" ? "claudeDesktop.progress.signed" : "claudeDesktop.progress.processed"),
            completed: status.completed,
            total: status.total,
            percent,
          })}
        </p>`],
  [`          {stopping
          ? phase === "authorizing"
            ? "取消请求已记录，请关闭系统授权弹窗。"
            : status.interruptible
              ? "正在取消并清理临时文件…"
              : "正在安全停止。当前写入完成后结束，并停止自动启动。"
          : phase === "authorizing"
            ? "请在系统弹窗中允许此操作。也可以在系统弹窗中取消授权。"
            : failed
              ? "可重试此操作，或在设置中恢复原版。"
              : "处理完成后自动进入下一步。"}`, `          {stopping
          ? phase === "authorizing"
            ? t("claudeDesktop.progress.cancelRecordedAuth")
            : status.interruptible
              ? t("claudeDesktop.progress.cancellingCleanup")
              : t("claudeDesktop.progress.safeStop")
          : phase === "authorizing"
            ? t("claudeDesktop.progress.authorizeHint")
            : failed
              ? t("claudeDesktop.progress.retryHint")
              : t("claudeDesktop.progress.autoNext")}`],
  [`          任务运行中 · 距上次进展 {idleSeconds} 秒`, `          {t("claudeDesktop.progress.idle", { seconds: idleSeconds })}`],
  [`          {stopping ? "正在安全停止…" : "取消操作"}`, `          {stopping
            ? t("claudeDesktop.action.stopping")
            : t("claudeDesktop.action.cancelOperation")}`],
  [`        setMessage("补丁处理已完成，已按要求停止自动启动。");`, `        setMessage(t("claudeDesktop.message.stoppedAfterInstall"));`],
  [`          saved
            ? "已恢复原版，之后将保留 Claude 自身的语言设置。"
            : "已恢复原版，但语言偏好未能保存，请重新设置。",`, `          saved
            ? t("claudeDesktop.message.restoredKept")
            : t("claudeDesktop.message.restoredNotSaved"),`],
  [`          action === "chinese"
            ? "中文补丁已就绪，已发送启动请求。"
            : "已发送启动请求。",`, `          action === "chinese"
            ? t("claudeDesktop.message.chineseReady")
            : t("claudeDesktop.message.launchRequested"),`],
  [`            : preferences.language === "zh-CN"
              ? "中文启动 Claude Desktop"
              : "启动 Claude Desktop"}`, `            : preferences.language === "zh-CN"
              ? t("claudeDesktop.action.chineseStart")
              : t("claudeDesktop.action.start")}`],
  [`            <RotateCcw className="mr-2 h-4 w-4" />
            恢复原版`, `            <RotateCcw className="mr-2 h-4 w-4" />
            {t("claudeDesktop.action.restore")}`],
  [`              {pending === "restore"
                ? "恢复 Claude Desktop 原版"
                : "安装 Claude Desktop 中文补丁"}`, `              {pending === "restore"
                ? t("claudeDesktop.dialog.restoreTitle")
                : t("claudeDesktop.dialog.chineseTitle")}`],
  [`              {busy
                ? "正在准备中文界面，可查看当前步骤或取消操作。"
                : pending === "restore"
                  ? "将关闭 Claude Desktop，并恢复与当前版本匹配的应用备份。账号和会话不会删除。"
                  : "使用开源项目 claude-desktop-zh-cn 的离线汉化资源。首次安装可能需要几分钟，安装前备份应用文件，完成后自动启动；可在设置中恢复原版。"}`, `              {busy
                ? t("claudeDesktop.dialog.preparing")
                : pending === "restore"
                  ? t("claudeDesktop.dialog.restoreDescription")
                  : t("claudeDesktop.dialog.chineseDescription")}`],
  [`                {isWindows()
                  ? "Windows 新版补丁会修改 Claude.exe 的完整性哈希，使原程序签名失效；同时修改 Cowork 服务，降低对客户端签名的校验。"
                  : isMac()
                    ? "macOS 补丁会修改应用资源并重新进行本地签名，替换原有开发者签名。"
                    : "Linux 补丁会修改 Claude Desktop 的应用资源，需要系统管理员授权。目前支持官方 deb 安装版。"}`, `                {isWindows()
                  ? t("claudeDesktop.dialog.windowsNote")
                  : isMac()
                    ? t("claudeDesktop.dialog.macNote")
                    : t("claudeDesktop.dialog.linuxNote")}`],
  [`              <p>
                Claude
                更新后可能需要重新汉化。官方账号界面已有原生中文；本补丁主要用于第三方
                API 模式。
              </p>`, `              <p>{t("claudeDesktop.dialog.relocalizeNote")}</p>`],
  [`              >
                取消
              </Button>`, `              >
                {t("claudeDesktop.action.cancel")}
              </Button>`],
  [`                {busy
                  ? "正在处理…"
                  : pending === "restore"
                    ? "恢复原版"
                    : "同意并中文启动"}`, `                {busy
                  ? t("claudeDesktop.status.processing")
                  : pending === "restore"
                    ? t("claudeDesktop.action.restore")
                    : t("claudeDesktop.action.agreeAndStart")}`],
  // useTranslation
  [`import { isWindows, isMac } from "@/lib/platform";`, `import { isWindows, isMac } from "@/lib/platform";
import { useTranslation } from "react-i18next";`],
  [`  const preferences = useClaudeDesktopPreferences();`, `  const preferences = useClaudeDesktopPreferences();
  const { t } = useTranslation();`],
];

let applied = 0;
const missed = [];
for (const [oldStr, newStr] of pairs) {
  if (s.includes(oldStr)) {
    s = s.split(oldStr).join(newStr);
    applied++;
  } else missed.push(oldStr.slice(0, 56).replace(/\n/g, "\\n"));
}
fs.writeFileSync(p, s);
console.log(`applied: ${applied}/${pairs.length}`);
if (missed.length) {
  console.log("MISSED:");
  for (const m of missed) console.log("  -", m);
}
