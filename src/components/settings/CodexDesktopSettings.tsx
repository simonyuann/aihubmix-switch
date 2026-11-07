import { useRef, useState } from "react";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { CodexPluginMarketDialog } from "@/components/aihubmix/CodexPluginMarketDialog";
import { codexMarketError, codexPluginMarket } from "@/lib/codexPluginMarket";
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
  setCodexDesktopPreferences,
  useCodexDesktopPreferences,
  type CodexDesktopLanguage,
} from "@/lib/codexDesktop";

export function CodexDesktopSettings() {
  const preferences = useCodexDesktopPreferences();
  const [error, setError] = useState("");
  const [showPlugins, setShowPlugins] = useState(false);
  const [pluginsBusy, setPluginsBusy] = useState(false);
  const [notice, setNotice] = useState("");
  const gate = useRef(false);

  function save(updates: Parameters<typeof setCodexDesktopPreferences>[0]) {
    setError(setCodexDesktopPreferences(updates) ? "" : "偏好保存失败，请重试。");
  }

  async function togglePlugins(plugins: boolean) {
    if (gate.current) return;
    gate.current = true;
    setPluginsBusy(true);
    setError("");
    setNotice("");
    try {
      if (plugins) await codexPluginMarket.enable();
      else await codexPluginMarket.disable();
      if (!setCodexDesktopPreferences({ plugins })) {
        setError("偏好保存失败，请重试。");
      } else {
        setNotice(plugins ? "插件目录已设置，请新建会话或重启 Codex。" : "插件市场已关闭，但已安装的插件不会卸载或停用；如需移除，请重新启用市场后逐个卸载。" );
      }
    } catch (cause) {
      setError(codexMarketError(cause, "操作未能确认，请检查网络和 Codex 安装状态后重试。"));
    } finally {
      gate.current = false;
      setPluginsBusy(false);
    }
  }

  return (
    <SettingsBlock title="Codex Desktop">
      <SettingsCard>
        <SettingsRow
          label="Codex 界面语言"
          description="从首页启动 Codex 时应用，不改变本软件的语言。请保持 Switch 在后台运行，以便继续应用界面增强。"
          control={
            <Select
              value={preferences.language}
              onValueChange={(value) => save({ language: value as CodexDesktopLanguage })}
            >
              <SelectTrigger className="h-8 w-[160px]" aria-label="Codex 界面语言">
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
          description="关闭市场不会卸载或停用已安装的插件。部分插件连接服务时仍需单独授权。"
          control={
            <>
              <Button variant="outline" size="compact" onClick={() => setShowPlugins(true)} disabled={pluginsBusy}>管理插件</Button>
              <Switch aria-label="社区插件市场" checked={preferences.plugins} onCheckedChange={(plugins) => void togglePlugins(plugins)} disabled={pluginsBusy} />
            </>
          }
        />
      </SettingsCard>
      {error && <p role="alert" className="text-caption text-destructive">{error}</p>}
      {notice && <p role="status" className="text-caption text-fg-3">{notice}</p>}
      {showPlugins && <CodexPluginMarketDialog onClose={() => setShowPlugins(false)} />}
    </SettingsBlock>
  );
}
