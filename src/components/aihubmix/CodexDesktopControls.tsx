import { useEffect, useState } from "react";
import { LoaderCircle, Play } from "lucide-react";
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
): string {
  if (result.status === "already_running") {
    return "未能自动退出 Codex，请保存工作并手动退出后重试。";
  }
  if (result.status === "not_installed")
    return "未找到受信任的 Codex Desktop，请在设置中选择程序路径。";
  if (result.status === "unsupported")
    return "当前系统或 Codex 版本暂不支持从这里启动。";

  const parts = ["Codex 已启动。"];
  if (preferences.language !== "system" && result.locale !== "applied")
    parts.push("界面语言未能确认，请在 Codex 中检查。");
  if (
    preferences.plugins &&
    result.plugins !== "applied" &&
    result.plugins !== "active"
  )
    parts.push("插件支持未能确认，请在 Codex 的插件页面检查。");
  if (result.status === "partial" && parts.length === 1)
    parts.push("部分功能未能确认，请在 Codex 中检查。");
  if (preferences.language !== "system") {
    parts.push(
      "增强启动期间，其他本机程序可能访问调试接口；退出 Codex 后关闭。",
    );
  }
  return parts.join("");
}

const safeLaunchErrors = new Set([
  "无法检查系统环境",
  "无法检查 Codex Desktop",
  "检查 Codex Desktop 超时",
  "无法分配本地调试端口",
  "无法分配独立的本地调试接口",
  "无法启动 Codex Desktop",
  "无法更新启动状态",
]);

/** Desktop launch is explicit; merely configuring a provider does not start Codex. */
export function CodexDesktopControls() {
  const preferences = useCodexDesktopPreferences();
  const [status, setStatus] = useState<CodexDesktopStatus | null>(null);
  const [message, setMessage] = useState("");
  const [launching, setLaunching] = useState(false);
  const [showPlugins, setShowPlugins] = useState(false);

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
      setMessage(startMessage(result, preferences));
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
          : "启动失败，请检查 Codex Desktop 是否已安装后重试。";
      setMessage(detail);
    } finally {
      setLaunching(false);
    }
  }

  const idle = status?.running
    ? "启动会先退出当前 Codex，正在运行的任务可能中断，请先保存工作。"
    : status?.installed === false
      ? "未检测到 Codex Desktop，可在设置中手动选择程序。"
      : "从这里启动，可应用设置中的语言和插件支持。";

  return (
    <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-2">
      <p
        role="status"
        aria-live="polite"
        className="min-w-0 flex-1 text-caption text-fg-3"
      >
        {message || idle}
      </p>
      <Button
        variant="outline"
        onClick={() => setShowPlugins(true)}
        disabled={launching}
        className="shrink-0"
      >
        管理插件
      </Button>
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
        {launching ? "正在启动" : "启动 Codex"}
      </Button>
      {showPlugins && (
        <CodexPluginMarketDialog onClose={() => setShowPlugins(false)} />
      )}
    </div>
  );
}
