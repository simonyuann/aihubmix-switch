import { useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { isWindows } from "@/lib/platform";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { CodexPluginMarketDialog } from "@/components/aihubmix/CodexPluginMarketDialog";
import { CodexMarketProgress } from "@/components/aihubmix/CodexMarketProgress";
import { codexMarketError, codexPluginMarket, type CodexMarketProgress as MarketProgress } from "@/lib/codexPluginMarket";
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
  const [marketProgress, setMarketProgress] = useState<MarketProgress | null>(null);
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
            ? "插件目录已设置，请新建会话或重启 Codex，然后在「插件 → 个人」中查看 AIHubMix 社区插件。"
            : "插件市场已关闭，但已安装的插件不会卸载或停用；如需移除，请重新启用市场后逐个卸载。",
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
        {isWindows() && (
          <SettingsRow
            label="Codex 程序位置（Windows）"
            description={
              preferences.executablePath ||
              "自动查找；解压版可手动选择 ChatGPT.exe 或 Codex.exe。仅接受有效签名的官方程序。"
            }
            control={
              <>
                <Button
                  variant="outline"
                  size="compact"
                  onClick={() =>
                    void (async () => {
                      try {
                        const path = await open({
                          multiple: false,
                          directory: false,
                          filters: [
                            { name: "Codex 桌面程序", extensions: ["exe"] },
                          ],
                        });
                        if (typeof path === "string") {
                          const status = await codexDesktop.status(path);
                          if (!status.installed) {
                            setError(
                              "无法验证该程序。请选择带有效官方签名的 ChatGPT.exe 或 Codex.exe。",
                            );
                            return;
                          }
                          save({ executablePath: path });
                        }
                      } catch {
                        setError("无法打开文件选择器，请重试。");
                      }
                    })()
                  }
                >
                  选择程序
                </Button>
                {preferences.executablePath && (
                  <Button
                    variant="outline"
                    size="compact"
                    onClick={() => save({ executablePath: "" })}
                  >
                    恢复自动查找
                  </Button>
                )}
              </>
            }
          />
        )}
        <SettingsRow
          label="Codex 界面语言"
          description="从首页启动 Codex 时应用，不改变本软件的语言。请保持 Switch 在后台运行，以便继续应用界面增强。"
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
          description="首次开启时从 AIHub 下载站准备插件库（约 375 MiB，解压后约 509 MiB），无需安装 Git。插件需单独安装，不会全部自动开启；旧版用户请关闭后重新开启，关闭市场不会卸载已安装的插件。"
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
