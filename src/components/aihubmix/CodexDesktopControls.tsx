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

function startMessage(result: CodexDesktopLaunchResult, preferences: CodexDesktopPreferences): string {
  if (result.status === "already_running") {
    return "未能自动退出 Codex，请保存工作并手动退出后重试。";
  }
  if (result.status === "not_installed") return "未找到 Codex Desktop，请先安装。";
  if (result.status === "unsupported") return "当前系统或 Codex 版本暂不支持从这里启动。";

  const parts = [result.status === "started" ? "Codex 已启动。" : "Codex 已启动，部分功能未能确认。"];
  if (preferences.language !== "system") {
    parts.push(result.locale === "applied" ? "界面语言已生效。" : "界面语言未能确认，请在 Codex 中检查。" );
  }
  if (preferences.plugins) {
    parts.push(
      result.plugins === "applied" || result.plugins === "active"
        ? "插件支持已生效。"
        : "插件支持未能确认；请在 Codex 的 Plugins → Personal 页面查看社区插件。",
    );
  }
  if (preferences.language !== "system") {
    parts.push("增强启动期间，本机其他进程可能访问调试接口；退出 Codex 后关闭。");
  }
  return parts.join("");
}

/** Desktop launch is explicit; merely configuring a provider does not start Codex. */
export function CodexDesktopControls() {
  const preferences = useCodexDesktopPreferences();
  const [status, setStatus] = useState<CodexDesktopStatus | null>(null);
  const [message, setMessage] = useState("");
  const [launching, setLaunching] = useState(false);
  const [showPlugins, setShowPlugins] = useState(false);

  useEffect(() => {
    let active = true;
    void codexDesktop.status().then(
      (value) => { if (active) setStatus(value); },
      () => { if (active) setStatus(null); },
    );
    return () => { active = false; };
  }, []);

  async function launch() {
    if (launching) return;
    setLaunching(true);
    setMessage("");
    try {
      const result = await codexDesktop.launch(preferences);
      setMessage(startMessage(result, preferences));
      if (result.status === "started" || result.status === "partial" || result.status === "already_running") {
        setStatus((old) => ({ installed: true, running: true, platform: old?.platform ?? "" }));
      }
    } catch {
      setMessage("启动失败，请检查 Codex Desktop 是否已安装后重试。");
    } finally {
      setLaunching(false);
    }
  }

  const idle = status?.running
    ? "Codex 正在运行；启动时会先正常退出，再应用设置重新启动。"
    : status?.installed === false
      ? "未检测到 Codex Desktop。"
      : "从这里启动，可应用设置中的语言和插件支持。";

  return (
    <div className="mt-3 flex flex-wrap items-center gap-x-4 gap-y-2">
      <p role="status" aria-live="polite" className="min-w-0 flex-1 text-caption text-fg-3">
        {message || idle}
      </p>
      <Button variant="outline" onClick={() => setShowPlugins(true)} disabled={launching} className="shrink-0">管理插件</Button>
      <Button onClick={() => void launch()} disabled={launching} className="shrink-0">
        {launching ? <LoaderCircle aria-hidden="true" className="h-4 w-4 animate-spin motion-reduce:animate-none" /> : <Play aria-hidden="true" className="h-4 w-4" />}
        {launching ? "正在启动" : "启动 Codex"}
      </Button>
      {showPlugins && <CodexPluginMarketDialog onClose={() => setShowPlugins(false)} />}
    </div>
  );
}
