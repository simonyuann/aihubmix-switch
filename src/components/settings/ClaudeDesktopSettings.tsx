import { useState } from "react";
import { DesktopProgramLocation } from "@/components/settings/DesktopProgramLocation";
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
import { ClaudeDesktopControls } from "@/components/aihubmix/ClaudeDesktopControls";
import {
  claudeDesktop,
  setClaudeDesktopPreferences,
  useClaudeDesktopPreferences,
  type ClaudeDesktopLanguage,
} from "@/lib/claudeDesktop";

export function ClaudeDesktopSettings() {
  const preferences = useClaudeDesktopPreferences();
  const [error, setError] = useState("");
  function save(updates: Parameters<typeof setClaudeDesktopPreferences>[0]) {
    setError(
      setClaudeDesktopPreferences(updates) ? "" : "偏好保存失败，请重试。",
    );
  }
  return (
    <SettingsBlock title="Claude Desktop">
      <SettingsCard>
        <SettingsRow
          label="Claude 界面语言"
          control={
            <Select
              value={preferences.language}
              onValueChange={(language) =>
                save({ language: language as ClaudeDesktopLanguage })
              }
            >
              <SelectTrigger
                className="h-8 w-[160px]"
                aria-label="Claude 界面语言"
              >
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="zh-CN">简体中文</SelectItem>
                <SelectItem value="system">保留原设置</SelectItem>
              </SelectContent>
            </Select>
          }
        />
        <DesktopProgramLocation
          name="Claude"
          executablePath={preferences.executablePath}
          inspect={claudeDesktop.status}
          onChange={(executablePath) => save({ executablePath })}
        />
        <div className="px-4 pb-4">
          <ClaudeDesktopControls showRestore />
          {error && (
            <p role="alert" className="mt-2 text-caption text-danger-text">
              {error}
            </p>
          )}
        </div>
      </SettingsCard>
    </SettingsBlock>
  );
}
