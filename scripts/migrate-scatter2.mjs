// T22 收尾子批:Announcements/CodexDesktopControls/AccountPage 余量/HomeImageModelSelect 提示
// 注:CodexDesktopControls 的 safeLaunchErrors(14 条)为后端消息内容匹配,非 i18n,保留。
import fs from "node:fs";
function migrate(p, pairs, extras = {}) {
  let s = fs.readFileSync(p, "utf8");
  let ok = 0;
  const missed = [];
  for (const [o, n] of pairs) {
    if (s.includes(o)) { s = s.split(o).join(n); ok++; }
    else missed.push(o.slice(0, 64).replace(/\n/g, "\\n"));
  }
  if (extras.importAfter && !s.includes('from "react-i18next"')) {
    s = s.replace(extras.importAfter, `$&\n${extras.importLine}`);
  }
  if (extras.hookAfter) {
    s = s.replace(extras.hookAfter, `$&\n${extras.hookLine}`);
  }
  fs.writeFileSync(p, s);
  console.log(`${p}: ${ok}/${pairs.length}${missed.length ? " MISSED:\n  " + missed.join("\n  ") : ""}`);
}

// 1. HomeImageModelSelect 提示 2 处(hook 已有)
migrate("src/components/aihubmix/HomeImageModelSelect.tsx", [
  [`生图和改图使用此型号。请保持 AIHub 运行。`, `{t("imageModelSelect.hint.keepRunning")}`],
  [`当前分组仅提供网关默认选项。`, `{t("imageModelSelect.hint.gatewayOnly")}`],
]);

// 2. AccountPage 余量 11 处(hook 已有)
migrate("src/components/aihubmix/AccountPage.tsx", [
  ["setNotice(`已删除 API Key「${deleting.name}」`);", 'setNotice(t("accountPage.notice.keyDeleted", { name: deleting.name }));'],
  ["`${friendlyError(e, t(\"accountPage.error.deleteUnconfirmed\"))}。请刷新列表确认删除结果后再重试。`,", 't("accountPage.error.deleteUnconfirmedDetail", {\n            detail: friendlyError(e, t("accountPage.error.deleteUnconfirmed")),\n          }),'],
  ['<p className="text-caption text-fg-3">当前账号</p>', '<p className="text-caption text-fg-3">{t("accountPage.label.currentAccount")}</p>'],
  ['<h2 className="font-semibold">我的 API Keys</h2>', '<h2 className="font-semibold">{t("accountPage.section.myKeys")}</h2>'],
  [`                刷新
              </Button>`, `                {t("accountPage.action.refresh")}
              </Button>`],
  ["aria-label={`复制 ${k.name}`}", 'aria-label={t("accountPage.aria.copyKey", { name: k.name })}'],
  [`                    复制 Key
                  </Button>`, `                    {t("accountPage.action.copyKey")}
                  </Button>`],
  ["aria-label={`删除 ${k.name}`}", 'aria-label={t("accountPage.aria.deleteKey", { name: k.name })}'],
  [`                    删除
                  </Button>`, `                    {t("accountPage.action.deleteKey")}
                  </Button>`],
  [`            在首页选择工具并添加模型分组，AIHub 会自动绑定可用的 API
            Key。模型可在 Agent 中通过 /model 切换。`, `            {t("accountPage.empty.bindHint")}`],
  ["message={`确定删除「${deleting?.name ?? \"\"}」？删除后，使用此 Key 的工具将无法继续调用 API，此操作无法撤销。`}", 'message={t("accountPage.dialog.deleteBody", {\n          name: deleting?.name ?? "",\n        })}'],
]);

// 3. Announcements 11 处(三个组件各接 hook)
migrate(
  "src/components/aihubmix/Announcements.tsx",
  [
    [`<strong>新公告</strong>`, `<strong>{t("announcements.banner.new")}</strong>`],
    [`{unread.length} 条未读`, `{t("announcements.banner.unreadCount", { count: unread.length })}`],
    [`throw new Error("公告数据格式异常");`, `throw new Error(t("announcements.error.badPayload"));`],
    [`friendlyError(e, "公告加载失败，请重试。")`, `friendlyError(e, t("announcements.error.loadFailed"))`],
    [`friendlyError(e, "公告状态保存失败，请重试。")`, `friendlyError(e, t("announcements.error.saveStateFailed"))`],
    [`title="公告"`, `title={t("announcements.title")}`],
  [`              返回公告列表
            </Button>`, `              {t("announcements.action.backToList")}
            </Button>`],
  [`                刷新公告
              </Button>`, `                {t("announcements.action.refresh")}
              </Button>`],
    [`<span className="sr-only">（未读）</span>`, `<span className="sr-only">{t("announcements.aria.unread")}</span>`],
    [`            正在加载公告…
          </p>`, `            {t("announcements.status.loading")}
          </p>`],
    [`<p className="py-4 text-body text-fg-3">暂无公告</p>`, `<p className="py-4 text-body text-fg-3">{t("announcements.empty.none")}</p>`],
  ],
  {
    importAfter: `import { friendlyError } from "./friendlyError";`,
    importLine: `import { useTranslation } from "react-i18next";`,
    hookAfter: `export function AnnouncementBanner({ onOpen }: { onOpen: () => void }) {`,
    hookLine: `  const { t } = useTranslation();`,
  },
);
// Provider 与 Page 各自补 hook
{
  const p = "src/components/aihubmix/Announcements.tsx";
  let s = fs.readFileSync(p, "utf8");
  s = s.replace(
    `  const { state } = useAccount();`,
    `  const { state } = useAccount();\n  const { t } = useTranslation();`,
  );
  s = s.replace(
    `export function AnnouncementPage() {\n  const announcements = useContext(Context);`,
    `export function AnnouncementPage() {\n  const { t } = useTranslation();\n  const announcements = useContext(Context);`,
  );
  fs.writeFileSync(p, s);
  console.log("Announcements provider/page hooks injected");
}

// 4. CodexDesktopControls 10 处 i18n(safeLaunchErrors 14 处保留为后端匹配)
migrate(
  "src/components/aihubmix/CodexDesktopControls.tsx",
  [
    [`      return "请保存工作并完全退出 Codex（包括托盘），再重新启动。";`, `      return t("codexDesktop.launch.restartAfterQuit");`],
    [`    return "未能自动退出 Codex，请保存工作并手动退出后重试。";`, `    return t("codexDesktop.launch.manualQuitRetry");`],
    [`    return "未找到受信任的 Codex Desktop，请在设置中选择程序路径。";`, `    return t("codexDesktop.launch.notFoundTrusted");`],
    [`    return "当前系统或 Codex 版本暂不支持从这里启动。";`, `    return t("codexDesktop.launch.unsupported");`],
    [`    return "界面语言未能确认，请在 Codex 中检查。";`, `    return t("codexDesktop.launch.localeUnconfirmed");`],
    [
      `function startMessage(
  result: CodexDesktopLaunchResult,
  preferences: CodexDesktopPreferences,
  platform?: string,
): string {`,
      `function startMessage(
  result: CodexDesktopLaunchResult,
  preferences: CodexDesktopPreferences,
  platform: string | undefined,
  t: (key: string) => string,
): string {`,
    ],
    [`      setMessage(startMessage(result, preferences, status?.platform));`, `      setMessage(startMessage(result, preferences, status?.platform, t));`],
    [`              "启动失败，请检查 Codex Desktop 是否已安装后重试。",`, `              t("codexDesktop.launch.failedFallback"),`],
    [`    status?.installed === false ? "未找到 Codex，请在设置中选择程序。" : "";`, `    status?.installed === false ? t("codexDesktop.idle.notInstalled") : "";`],
    [`{launching ? "正在启动" : "启动 Codex"}`, `{launching ? t("codexDesktop.action.launching") : t("codexDesktop.action.launch")}`],
    [`          <Puzzle className="h-4 w-4" aria-hidden="true" />
          管理插件
        </Button>`, `          <Puzzle className="h-4 w-4" aria-hidden="true" />
          {t("codexDesktop.action.managePlugins")}
        </Button>`],
  ],
  {
    importAfter: `import { useEffect, useState } from "react";`,
    importLine: `import { useTranslation } from "react-i18next";`,
    hookAfter: `  const preferences = useCodexDesktopPreferences();`,
    hookLine: `  const { t } = useTranslation();`,
  },
);
console.log("scatter2 done");
