// T22 ToolPage 批次:硬编码中文 → i18n(映射表驱动)
import fs from "node:fs";
const p = "src/components/aihubmix/ToolPage.tsx";
let s = fs.readFileSync(p, "utf8");
const before = s;

// 精确 old→new 替换对(按文件中实际文本,顺序敏感:长串在前)
const pairs = [
  // ---- handlers:notice/error(quoted)----
  [`      setNotice("分组已添加。");`, `      setNotice(t("toolPage.notice.groupAdded"));`],
  [`friendlyError(e, "添加分组失败，请重试。")`, `friendlyError(e, t("toolPage.error.addGroup"))`],
  [`["分组已从此工具移除。", ...(result?.warnings ?? [])].join(" ")`, `[t("toolPage.notice.groupRemoved"), ...(result?.warnings ?? [])].join(" ")`],
  [`friendlyError(e, "移除分组失败，请检查当前配置后重试。")`, `friendlyError(e, t("toolPage.error.removeGroup"))`],
  [`          ? "生图模型已保存。请重启 Codex，使连接设置生效。"
          : "生图模型已保存。",`, `          ? t("toolPage.notice.imagegenSavedRestart")
          : t("toolPage.notice.imagegenSaved"),`],
  [`friendlyError(e, "保存生图模型失败，请重试。")`, `friendlyError(e, t("toolPage.error.saveImagegen"))`],
  [`              ? "如需使用命令行生图备用方式，还需在本机安装 Python 3.11+ 和 OpenAI SDK；WSL、远程主机和容器需分别配置。"
              : "命令行生图备用方式未能自动接入，这不代表原生生图不可用。请检查 Codex 的 shell_environment_policy 和 PYTHONPATH 设置。",`, `              ? t("toolPage.notice.imagegenManualRequired")
              : t("toolPage.notice.imagegenManualFailed"),`],
  [`            "命令行生图备用方式的状态不可用，这不代表原生生图不可用。",`, `            t("toolPage.notice.imagegenStatusUnavailable"),`],
  [`? \`已切换到 \${card.provider.name}。请重启 Codex；如果官方登录已失效，请在 Codex 中重新登录。\``, `? t("toolPage.notice.switchedOfficialRestart", { name: card.provider.name })`],
  [`? \`已启用 \${card.provider.name}。可以同时启用多个分组；请在 DeepSeek Harness 中选择模型。\``, `? t("toolPage.notice.dshMultiEnabled", { name: card.provider.name })`],
  [`              ? \`已切换到 \${card.provider.name}。\${result.selectedModel ? \`默认模型：\${result.selectedModel}。\` : ""}\${app === "codex" ? "请重启 Codex，使配置生效。" : \`在 \${APP_DISPLAY_NAME[app]} 中选择模型。\`}\`
          : "API Key 已保存。",`, `              ? t("toolPage.notice.switchedTo", { name: card.provider.name }) +
                (result.selectedModel
                  ? t("toolPage.notice.defaultModelSet", { model: result.selectedModel })
                  : "") +
                (app === "codex"
                  ? t("toolPage.notice.restartCodex")
                  : t("toolPage.notice.pickModelInApp", { app: APP_DISPLAY_NAME[app] }))
          : t("toolPage.notice.apiKeySaved"),`],
  [`friendlyError(warning, "部分配置未能确认，请刷新状态后重试。")`, `friendlyError(warning, t("toolPage.error.partialConfig"))`],
  [`friendlyError(e, "操作失败，请检查网络后重试。")`, `friendlyError(e, t("toolPage.error.operationFailed"))`],
  // ---- header / JSX ----
  [`        title={simple ? "首页" : APP_DISPLAY_NAME[app]}`, `        title={simple ? t("toolPage.title.home") : APP_DISPLAY_NAME[app]}`],
  [`              <Button variant="outline" onClick={onOpenProviders}>
                其他供应商
              </Button>`, `              <Button variant="outline" onClick={onOpenProviders}>
                {t("toolPage.action.openProviders")}
              </Button>`],
  [`                onClick={openAddGroup}
              >
                添加分组
              </Button>`, `                onClick={openAddGroup}
              >
                {t("toolPage.action.addGroup")}
              </Button>`],
  [`<Button onClick={() => setApply(true)}>申请 API</Button>`, `<Button onClick={() => setApply(true)}>{t("toolPage.action.applyApi")}</Button>`],
  [`            DeepSeek Harness 支持同时启用多个分组，请在 DeepSeek Harness
            中选择模型。`, `            {t("toolPage.hint.dshMulti")}`],
  [`<p className="text-caption font-medium text-fg-3">当前工具</p>`, `<p className="text-caption font-medium text-fg-3">{t("toolPage.label.currentTool")}</p>`],
  [`aria-label={\`选择 AI 工具，当前为 \${APP_DISPLAY_NAME[app]}\`}`, `aria-label={t("toolPage.aria.selectTool", { name: APP_DISPLAY_NAME[app] })}`],
  [`                  <ArrowLeftRight aria-hidden="true" className="h-3.5 w-3.5" />
                  切换工具`, `                  <ArrowLeftRight aria-hidden="true" className="h-3.5 w-3.5" />
                  {t("toolPage.action.switchTool")}`],
  [`<h2 className="text-title font-semibold">模型分组</h2>`, `<h2 className="text-title font-semibold">{t("toolPage.section.groups")}</h2>`],
  [`                  {officialCodex.provider.id === data?.currentProviderId
                    ? "当前为官方配置"
                    : "恢复官方登录"}`, `                  {officialCodex.provider.id === data?.currentProviderId
                    ? t("toolPage.action.officialCurrent")
                    : t("toolPage.action.restoreOfficial")}`],
  [`                <Plus aria-hidden="true" className="h-4 w-4" />
                添加分组`, `                <Plus aria-hidden="true" className="h-4 w-4" />
                {t("toolPage.action.addGroup")}`],
  [`              正在加载模型服务…`, `              {t("toolPage.status.loadingGroups")}`],
  [`{friendlyError(loadError, "加载模型服务失败，请重试。")}{" "}
              <Button variant="outline" onClick={() => void refetch()}>
                重试
              </Button>`, `{friendlyError(loadError, t("toolPage.error.loadGroups"))}{" "}
              <Button variant="outline" onClick={() => void refetch()}>
                {t("toolPage.action.retry")}
              </Button>`],
  [`{friendlyError(data.warning, "模型服务暂不可用，请重试。")}{" "}
            <Button variant="outline" onClick={() => void refetch()}>
              重试
            </Button>`, `{friendlyError(data.warning, t("toolPage.error.groupsUnavailable"))}{" "}
            <Button variant="outline" onClick={() => void refetch()}>
              {t("toolPage.action.retry")}
            </Button>`],
  [`                    activeLabel={
                      enabled ? "已启用" : current ? "使用中" : undefined
                    }`, `                    activeLabel={
                      enabled
                        ? t("toolPage.status.enabled")
                        : current
                          ? t("toolPage.status.inUse")
                          : undefined
                    }`],
  [`                    status={
                      card.verificationPending
                        ? "待联网验证"
                        : card.configured
                          ? "分组已就绪"
                          : "切换时自动配置"
                    }`, `                    status={
                      card.verificationPending
                        ? t("toolPage.status.verificationPending")
                        : card.configured
                          ? t("toolPage.status.ready")
                          : t("toolPage.status.autoConfig")
                    }`],
  [`aria-label={\`重新启用 \${card.provider.name}\`}
                          onClick={() => void operate(card, true)}
                        >
                          重新启用`, `aria-label={t("toolPage.aria.reEnable", { name: card.provider.name })}
                          onClick={() => void operate(card, true)}
                        >
                          {t("toolPage.action.reEnable")}`],
  [`aria-label={\`\${app === "dsh" ? "启用" : "切换到"} \${card.provider.name}\`}
                        onClick={() => void operate(card, true)}
                      >
                        {app === "dsh" ? "启用" : "切换"}`, `aria-label={t(
                          app === "dsh" ? "toolPage.aria.enable" : "toolPage.aria.switchTo",
                          { name: card.provider.name },
                        )}
                        onClick={() => void operate(card, true)}
                      >
                        {app === "dsh" ? t("toolPage.action.enable") : t("toolPage.action.switch")}`],
  [`aria-label={\`移除 \${card.provider.name}\`}
                        onClick={() => confirmRemoval(card)}
                      >
                        移除`, `aria-label={t("toolPage.aria.remove", { name: card.provider.name })}
                        onClick={() => confirmRemoval(card)}
                      >
                        {t("toolPage.action.remove")}`],
  [`<span className="text-caption text-fg-3">状态未知</span>`, `<span className="text-caption text-fg-3">{t("toolPage.status.unknown")}</span>`],
  [`                            label: enabled ? "已启用" : "使用中",`, `                            label: enabled
                              ? t("toolPage.status.enabled")
                              : t("toolPage.status.inUse"),`],
  [`                          ? { label: "状态未知", dot: "muted" }`, `                          ? { label: t("toolPage.status.unknown"), dot: "muted" }`],
  [`                              label: \`重新启用 \${card.provider.name}\`,`, `                              label: t("toolPage.aria.reEnable", { name: card.provider.name }),`],
  [`                              disabledReason: busy ? "正在切换配置" : undefined,
                              onClick: () =>
                                card.configured
                                  ? void operate(card, true)
                                  : edit(card),`, `                              disabledReason: busy
                                ? t("toolPage.status.switchingConfig")
                                : undefined,
                              onClick: () =>
                                card.configured
                                  ? void operate(card, true)
                                  : edit(card),`],
  [`                                label: \`\${app === "dsh" ? "启用" : "启动"} \${card.provider.name}\`,
                                disabledReason: busy
                                  ? "正在切换配置"
                                  : undefined,`, `                                label: t(
                                  app === "dsh" ? "toolPage.aria.enable" : "toolPage.aria.start",
                                  { name: card.provider.name },
                                ),
                                disabledReason: busy
                                  ? t("toolPage.status.switchingConfig")
                                  : undefined,`],
  [`                              key: "remove-group",
                              label: \`移除 \${card.provider.name}\`,`, `                              key: "remove-group",
                              label: t("toolPage.aria.remove", { name: card.provider.name }),`],
  [`                              disabledReason:
                                busy || dshUnknown
                                  ? "请等待配置状态确认"
                                  : undefined,`, `                              disabledReason:
                                busy || dshUnknown
                                  ? t("toolPage.error.waitStateConfirm")
                                  : undefined,`],
  [`                              label: card.verificationPending
                                ? "待联网验证"
                                : card.configured
                                  ? card.keyMasked || "已配置"
                                  : "待选择 API",`, `                              label: card.verificationPending
                                ? t("toolPage.status.verificationPending")
                                : card.configured
                                  ? card.keyMasked || t("toolPage.status.configured")
                                  : t("toolPage.status.keyNotSelected"),`],
  [`                    editDisabledReason: busy ? "正在保存配置" : undefined,`, `                    editDisabledReason: busy
                      ? t("toolPage.status.savingConfig")
                      : undefined,`],
  [`<p className="font-semibold">还没有添加模型分组</p>`, `<p className="font-semibold">{t("toolPage.empty.noGroupsTitle")}</p>`],
  [`                <p className="text-body text-fg-3">
                  点「添加分组」选择要使用的模型分组。
                </p>`, `                <p className="text-body text-fg-3">
                  {t("toolPage.empty.noGroupsHint")}
                </p>`],
  [`            <p className="text-body text-fg-3">
              当前暂无适用于此工具的模型服务。
            </p>`, `            <p className="text-body text-fg-3">
              {t("toolPage.empty.noServices")}
            </p>`],
  [`<summary className="cursor-pointer">命令行生图备用方式</summary>`, `<summary className="cursor-pointer">{t("toolPage.summary.imagegenFallback")}</summary>`],
  [`<DialogTitle>移除 {removing.provider.name}</DialogTitle>`, `<DialogTitle>{t("toolPage.dialog.removeTitle", { name: removing.provider.name })}</DialogTitle>`],
  [`                从此工具移除分组后，仍可重新添加，平台 API Key 会保留。`, `                {t("toolPage.dialog.removeDescription")}`],
  [`                替代分组
                <select`, `                {t("toolPage.dialog.replacementGroup")}
                <select`],
  [`                  <option value="">请选择替代分组</option>`, `                  <option value="">{t("toolPage.dialog.selectReplacement")}</option>`],
  [`                  <span>请先添加并配置另一个分组，再移除此分组。</span>`, `                  <span>{t("toolPage.dialog.replacementRequired")}</span>`],
  [`              <DialogTitle>选择工具</DialogTitle>
              <DialogDescription>
                选择后在首页展示该工具及其分组。
              </DialogDescription>`, `              <DialogTitle>{t("toolPage.dialog.selectToolTitle")}</DialogTitle>
              <DialogDescription>
                {t("toolPage.dialog.selectToolDescription")}
              </DialogDescription>`],
  [`                    <span className="text-caption text-action-text">当前</span>`, `                    <span className="text-caption text-action-text">
                      {t("toolPage.status.current")}
                    </span>`],
  [`              <DialogTitle>添加分组</DialogTitle>
              <DialogDescription>
                此处显示当前工具支持的模型分组。添加后可随时切换。
              </DialogDescription>`, `              <DialogTitle>{t("toolPage.action.addGroup")}</DialogTitle>
              <DialogDescription>
                {t("toolPage.dialog.addDescription")}
              </DialogDescription>`],
  [`                <span>平台分组</span>
                <select
                  aria-label="平台分组"`, `                <span>{t("toolPage.dialog.platformGroup")}</span>
                <select
                  aria-label={t("toolPage.dialog.platformGroup")}`],
  [`                  <option value="">请选择分组</option>`, `                  <option value="">{t("toolPage.dialog.selectGroup")}</option>`],
  [`                      {groupAdded(group.id) ? "（已添加）" : ""}`, `                      {groupAdded(group.id) ? t("toolPage.dialog.alreadyAdded") : ""}`],
  [`                刷新分组`, `                {t("toolPage.action.refreshGroups")}`],
  [`                <p className="text-body text-fg-3">
                  当前工具暂无可添加的分组。
                </p>`, `                <p className="text-body text-fg-3">
                  {t("toolPage.dialog.noAvailableGroups")}
                </p>`],
  [`                  aria-label="确认添加分组"`, `                  aria-label={t("toolPage.aria.confirmAdd")}`],
  [`                  {busy ? "正在添加…" : "添加分组"}`, `                  {busy ? t("toolPage.status.adding") : t("toolPage.action.addGroup")}`],
  [`                {app === "dsh"
                  ? "仅显示此分组下可用的 API Key。启用后请在 DeepSeek Harness 中选择模型。"
                  : "仅显示此分组下可用的 API Key。模型可在 Agent 中通过 /model 切换。"}`, `                {app === "dsh"
                  ? t("toolPage.dialog.keyDescriptionDsh")
                  : t("toolPage.dialog.keyDescription")}`],
  [`                  <option value="">请选择 API Key</option>`, `                  <option value="">{t("toolPage.dialog.selectApiKey")}</option>`],
  [`                  正在加载 API…`, `                  {t("toolPage.status.loadingKeys")}`],
  [`                {friendlyError(keyQuery.error, "加载 API 列表失败，请重试。")}{" "}
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => void keyQuery.refetch()}
                  >
                    重试`, `                {friendlyError(keyQuery.error, t("toolPage.error.loadKeys"))}{" "}
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => void keyQuery.refetch()}
                  >
                    {t("toolPage.action.retry")}`],
  [`                <p className="text-body text-fg-3">
                  此分组暂无可用 API，请通过右上角「申请 API」创建。
                </p>`, `                <p className="text-body text-fg-3">
                  {t("toolPage.dialog.noKeys")}
                </p>`],
  [`                  保存
                </Button>`, `                  {t("toolPage.action.save")}
                </Button>`],
  [`                  {busy
                    ? "正在处理…"
                    : app === "dsh"
                      ? "保存并启用"
                      : "保存并启动"}`, `                  {busy
                    ? t("toolPage.status.processing")
                    : app === "dsh"
                      ? t("toolPage.action.saveAndEnable")
                      : t("toolPage.action.saveAndStart")}`],
  // 取消 ×2(Button 变体不同,分别处理)
  [`                onClick={() => setRemoving(null)}
              >
                取消`, `                onClick={() => setRemoving(null)}
              >
                {t("toolPage.action.cancel")}`],
  [`                    onClick={() => {
                      setEditing(null);
                      setKeyId("");
                    }}
                  >
                    取消`, `                    onClick={() => {
                      setEditing(null);
                      setKeyId("");
                    }}
                  >
                    {t("toolPage.action.cancel")}`],
  // useTranslation hook
  [`  const client = useQueryClient();`, `  const client = useQueryClient();
  const { t } = useTranslation();`],
  [`import { useRef, useState } from "react";`, `import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";`],
];

let applied = 0;
const missed = [];
for (const [oldStr, newStr] of pairs) {
  if (s.includes(oldStr)) {
    s = s.split(oldStr).join(newStr);
    applied++;
  } else {
    missed.push(oldStr.slice(0, 60).replace(/\n/g, "\\n"));
  }
}
fs.writeFileSync(p, s);
console.log(`applied: ${applied}/${pairs.length}`);
if (missed.length) {
  console.log("MISSED:");
  for (const m of missed) console.log("  -", m);
}
