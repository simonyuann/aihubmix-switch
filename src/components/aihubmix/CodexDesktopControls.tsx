import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { LoaderCircle, Play, Puzzle } from "lucide-react";
import { codexMarketError } from "@/lib/codexPluginMarket";
import { isLinux } from "@/lib/platform";
import { Button } from "@/components/ui/button";
import { CodexPluginMarketDialog } from "@/components/aihubmix/CodexPluginMarketDialog";
import {
  codexDesktop,
  useCodexDesktopPreferences,
  type CodexDesktopLaunchResult,
  type CodexDesktopPreferences,
  type CodexDesktopStatus,
} from "@/lib/codexDesktop";

function startMessage(
  result: CodexDesktopLaunchResult,
  preferences: CodexDesktopPreferences,
  platform: string | undefined,
  t: (key: string) => string,
): string {
  if (result.status === "already_running") {
    if (platform === "linux" || (!platform && isLinux()))
      return t("codexDesktop.launch.restartAfterQuit");
    return t("codexDesktop.launch.manualQuitRetry");
  }
  if (result.status === "not_installed")
    return t("codexDesktop.launch.notFoundTrusted");
  if (result.status === "unsupported")
    return t("codexDesktop.launch.unsupported");

  if (
    preferences.language !== "system" &&
    result.locale !== "applied" &&
    result.locale !== "scheduled"
  )
    return t("codexDesktop.launch.localeUnconfirmed");
  return "";
}

const safeLaunchErrors = new Set([
  "Codex 正在启动。",
  "插件目录正在更新，请稍后再启动 Codex。",
  "无法检查系统环境",
  "无法检查 Codex Desktop",
  "检查 Codex Desktop 超时",
  "无法分配本地调试端口",
  "无法分配独立的本地调试接口",
  "无法启动 Codex Desktop",
  "请在 Linux 图形桌面中启动 Codex。",
  "无法检查 Codex 运行状态",
  "当前桌面版缺少内置 Node，请更新桌面版或从 AIHub 管理插件",
  "插件安装组件未能准备好，请检查可用磁盘空间后重试",
  "无法为桌面程序准备插件工具路径",
  "无法更新启动状态",
]);

/** Desktop launch is explicit; merely configuring a provider does not start Codex. */
export function CodexDesktopControls() {
  const preferences = useCodexDesktopPreferences();
  const { t } = useTranslation();
  const [status, setStatus] = useState<CodexDesktopStatus | null>(null);
  const [message, setMessage] = useState("");
  const [launching, setLaunching] = useState(false);

  useEffect(() => {
    let active = true;
    void codexDesktop.status(preferences.executablePath).then(
      (value) => {
        if (active) setStatus(value);
      },
      () => {
        if (active) setStatus(null);
      },
    );
    return () => {
      active = false;
    };
  }, [preferences.executablePath]);

  async function launch() {
    if (launching) return;
    setLaunching(true);
    setMessage("");
    try {
      const result = await codexDesktop.launch(preferences);
      setMessage(startMessage(result, preferences, status?.platform, t));
      if (
        result.status === "started" ||
        result.status === "partial" ||
        result.status === "already_running"
      ) {
        setStatus((old) => ({
          installed: true,
          running: true,
          platform: old?.platform ?? "",
        }));
      }
    } catch (error) {
      const detail =
        typeof error === "string" && safeLaunchErrors.has(error)
          ? error
          : codexMarketError(error, t("codexDesktop.launch.failedFallback"));
      setMessage(detail);
    } finally {
      setLaunching(false);
    }
  }

  const idle =
    status?.installed === false ? t("codexDesktop.idle.notInstalled") : "";

  return (
    <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-2">
      {(message || idle) && (
        <p
          role="status"
          aria-live="polite"
          className="min-w-0 flex-1 text-caption text-fg-3"
        >
          {message || idle}
        </p>
      )}
      <Button
        onClick={() => void launch()}
        disabled={launching}
        className="shrink-0"
      >
        {launching ? (
          <LoaderCircle
            aria-hidden="true"
            className="h-4 w-4 animate-spin motion-reduce:animate-none"
          />
        ) : (
          <Play aria-hidden="true" className="h-4 w-4" />
        )}
        {launching
          ? t("codexDesktop.action.launching")
          : t("codexDesktop.action.launch")}
      </Button>
      <CodexPluginMarketDialog>
        <Button variant="outline" disabled={launching} className="shrink-0">
          <Puzzle className="h-4 w-4" aria-hidden="true" />
          {t("codexDesktop.action.managePlugins")}
        </Button>
      </CodexPluginMarketDialog>
    </div>
  );
}
