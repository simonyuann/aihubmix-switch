import { useMemo, useSyncExternalStore } from "react";
import { Channel, invoke } from "@tauri-apps/api/core";
import { readPreference, writePreference } from "@/lib/userPreferences";

export type ClaudeLaunchPhase =
  | "preparing"
  | "authorizing"
  | "checking_backup"
  | "cleaning"
  | "closing"
  | "permissions"
  | "copying"
  | "extracting"
  | "translating"
  | "resources"
  | "packing"
  | "signing"
  | "verifying"
  | "replacing"
  | "restoring"
  | "launching"
  | "completed"
  | "cancelled";
export interface ClaudeLaunchProgress {
  phase: ClaudeLaunchPhase;
  completed?: number | null;
  total?: number | null;
  interruptible: boolean;
}

export type ClaudeDesktopLanguage = "zh-CN" | "system";
export interface ClaudeDesktopPreferences {
  language: ClaudeDesktopLanguage;
  executablePath: string;
}
export interface ClaudeLaunchStatus {
  installed: boolean;
  patched: boolean;
  canRestore: boolean;
  version: string;
  platform: string;
  executablePath?: string;
}
const KEY = "aihub:claude-desktop-preferences";
const EVENT = "aihub:claude-desktop-preferences-changed";
const DEFAULT = '{"language":"zh-CN","executablePath":""}';
function snapshot() {
  try {
    return readPreference(KEY) ?? DEFAULT;
  } catch {
    return DEFAULT;
  }
}
function decode(raw: string): ClaudeDesktopPreferences {
  try {
    const value = JSON.parse(raw);
    return {
      language: value?.language === "system" ? "system" : "zh-CN",
      executablePath:
        typeof value?.executablePath === "string" ? value.executablePath : "",
    };
  } catch {
    return { language: "zh-CN", executablePath: "" };
  }
}
function subscribe(listener: () => void) {
  window.addEventListener(EVENT, listener);
  window.addEventListener("storage", listener);
  return () => {
    window.removeEventListener(EVENT, listener);
    window.removeEventListener("storage", listener);
  };
}
export function useClaudeDesktopPreferences() {
  const raw = useSyncExternalStore(subscribe, snapshot, () => DEFAULT);
  return useMemo(() => decode(raw), [raw]);
}
export function setClaudeDesktopPreferences(
  updates: Partial<ClaudeDesktopPreferences>,
) {
  try {
    writePreference(KEY, JSON.stringify({ ...decode(snapshot()), ...updates }));
    window.dispatchEvent(new Event(EVENT));
    return true;
  } catch {
    return false;
  }
}
export const claudeDesktop = {
  cancel: (operationId: string) =>
    invoke<void>("cancel_claude_desktop_launch", { operationId }),
  status: (executablePath = "") =>
    invoke<ClaudeLaunchStatus>("claude_desktop_launch_status", {
      executablePath,
    }),
  run: (
    executablePath: string,
    action: "chinese" | "launch" | "restore",
    consent = false,
    onProgress?: (progress: ClaudeLaunchProgress) => void,
    operationId = crypto.randomUUID(),
  ) => {
    const progress = new Channel<ClaudeLaunchProgress>();
    progress.onmessage = (phase) => onProgress?.(phase);
    return invoke<string>("launch_claude_desktop", {
      executablePath,
      action,
      consent,
      onProgress: progress,
      operationId,
    });
  },
};
export function claudeDesktopError(error: unknown): string {
  const messages: Record<string, string> = {
    not_installed: "未找到 Claude Desktop，请先安装，或在设置中选择程序位置。",
    unsupported_layout:
      "当前 Claude Desktop 安装结构暂不支持汉化，请使用官方安装版。",
    invalid_installation: "所选位置不是可用的 Claude Desktop，请重新选择。",
    cancelled: "本次操作已取消。",
    python_missing: "汉化需要 Python 3.9 或更新版本，请先安装 Python。",
    elevation_unavailable: "无法打开系统授权窗口，请确认已安装 PolicyKit。",
    no_matching_backup:
      "没有与当前版本匹配的备份。请使用官方安装程序恢复，避免降级。",
    busy: "Claude Desktop 正在处理上一项操作，请稍候。",
    launch_failed: "启动未完成，请手动打开 Claude Desktop 后重试。",
    state_write_failed: "无法保存汉化状态，请检查磁盘空间和应用目录权限。",
    patch_failed:
      "汉化未完成。请先关闭 Claude Desktop 后重试；若仍失败，可在设置中恢复原版。商店安装版可能需要使用官方独立安装版。",
  };
  return typeof error === "string" &&
    Object.prototype.hasOwnProperty.call(messages, error)
    ? messages[error]
    : "操作未完成，请检查 Claude Desktop 安装状态后重试。";
}
