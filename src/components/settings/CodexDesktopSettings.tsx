import { useRef, useState } from "react";
import { DesktopProgramLocation } from "@/components/settings/DesktopProgramLocation";
import { isWindows } from "@/lib/platform";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { CodexPluginMarketDialog } from "@/components/aihubmix/CodexPluginMarketDialog";
import { CodexMarketProgress } from "@/components/aihubmix/CodexMarketProgress";
import {
  codexMarketError,
  codexPluginMarket,
  type CodexMarketProgress as MarketProgress,
} from "@/lib/codexPluginMarket";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import {
  SettingsBlock,
  SettingsCard,
  SettingsRow,
} from "@/components/settings/SettingsLayout";
import {
  codexDesktop,
  setCodexDesktopPreferences,
  useCodexDesktopPreferences,
  type CodexDesktopLanguage,
} from "@/lib/codexDesktop";

export function CodexDesktopSettings() {
  const preferences = useCodexDesktopPreferences();
  const [error, setError] = useState("");
  const [showPlugins, setShowPlugins] = useState(false);
  const [pluginsBusy, setPluginsBusy] = useState(false);
  const [enablingPlugins, setEnablingPlugins] = useState(false);
  const [marketProgress, setMarketProgress] = useState<MarketProgress | null>(
    null,
  );
  const [notice, setNotice] = useState("");
  const gate = useRef(false);

  function save(updates: Parameters<typeof setCodexDesktopPreferences>[0]) {
    setError(
      setCodexDesktopPreferences(updates) ? "" : "偏好保存失败，请重试。",
    );
  }

  async function togglePlugins(plugins: boolean) {
    if (gate.current) return;
    gate.current = true;
    setPluginsBusy(true);
    setEnablingPlugins(plugins);
    setMarketProgress(null);
    setError("");
    setNotice("");
    try {
      if (plugins) await codexPluginMarket.enable(setMarketProgress);
      else await codexPluginMarket.disable();
      if (!setCodexDesktopPreferences({ plugins })) {
        setError("偏好保存失败，请重试。");
      } else {
        setNotice(
          plugins
            ? "插件市场已开启。从 AIHub 启动桌面端后，可在「插件 → 个人」按需安装。"
            : "插件市场已关闭。",
        );
      }
    } catch (cause) {
      setError(
        codexMarketError(
          cause,
          "操作未能确认，请检查网络和 Codex 安装状态后重试。",
        ),
      );
    } finally {
      gate.current = false;
      setPluginsBusy(false);
      setEnablingPlugins(false);
      setMarketProgress(null);
    }
  }

  return (
    <SettingsBlock title="Codex Desktop">
      <SettingsCard>
        <DesktopProgramLocation
          name="Codex"
          executablePath={preferences.executablePath}
          inspect={codexDesktop.status}
          onChange={(executablePath) => save({ executablePath })}
          canChoose={isWindows()}
        />
        <SettingsRow
          label="Codex 界面语言"
          control={
            <Select
              value={preferences.language}
              onValueChange={(value) =>
                save({ language: value as CodexDesktopLanguage })
              }
            >
              <SelectTrigger
                className="h-8 w-[160px]"
                aria-label="Codex 界面语言"
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="zh-CN">简体中文</SelectItem>
                <SelectItem value="en-US">English</SelectItem>
                <SelectItem value="system">保留原设置</SelectItem>
              </SelectContent>
            </Select>
          }
        />
        <SettingsRow
          label="社区插件市场"
          control={
            <>
              <Button
                variant="outline"
                size="compact"
                onClick={() => setShowPlugins(true)}
                disabled={pluginsBusy}
              >
                管理插件
              </Button>
              <Switch
                aria-label="社区插件市场"
                checked={preferences.plugins}
                onCheckedChange={(plugins) => void togglePlugins(plugins)}
                disabled={pluginsBusy}
              />
            </>
          }
        />
      </SettingsCard>
      {enablingPlugins && <CodexMarketProgress progress={marketProgress} />}
      {error && (
        <p role="alert" className="text-caption text-destructive">
          {error}
        </p>
      )}
      {notice && (
        <p role="status" className="text-caption text-fg-3">
          {notice}
        </p>
      )}
      {showPlugins && (
        <CodexPluginMarketDialog onClose={() => setShowPlugins(false)} />
      )}
    </SettingsBlock>
  );
}
