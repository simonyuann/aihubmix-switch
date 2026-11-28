import { useMemo, useSyncExternalStore } from "react";
import { invoke } from "@tauri-apps/api/core";
import { readPreference, writePreference } from "@/lib/userPreferences";

export type CodexDesktopLanguage = "zh-CN" | "en-US" | "system";

export interface CodexDesktopPreferences {
  language: CodexDesktopLanguage;
  plugins: boolean;
  executablePath: string;
}

export interface CodexDesktopStatus {
  installed: boolean;
  running: boolean;
  platform: string;
  message?: string;
  executablePath?: string;
}

export interface CodexDesktopLaunchResult {
  status:
    | "started"
    | "already_running"
    | "not_installed"
    | "unsupported"
    | "partial";
  locale:
    | "applied"
    | "scheduled"
    | "partial"
    | "not_confirmed"
    | "not_requested"
    | "unsupported"
    | "failed";
  plugins: string;
  message?: string;
}

const KEY = "aihubmix:codex-desktop-preferences";
const CHANGE = "aihubmix:codex-desktop-preferences-changed";
const DEFAULT = '{"language":"zh-CN","plugins":false}';

function snapshot(): string {
  try {
    return readPreference(KEY) ?? DEFAULT;
  } catch {
    return DEFAULT;
  }
}

function subscribe(listener: () => void) {
  window.addEventListener(CHANGE, listener);
  window.addEventListener("storage", listener);
  return () => {
    window.removeEventListener(CHANGE, listener);
    window.removeEventListener("storage", listener);
  };
}

function decode(raw: string): CodexDesktopPreferences {
  try {
    const value = JSON.parse(raw) as Partial<CodexDesktopPreferences>;
    return {
      language:
        value.language === "en-US" || value.language === "system"
          ? value.language
          : "zh-CN",
      plugins: value.plugins === true,
      executablePath:
        typeof value.executablePath === "string" ? value.executablePath : "",
    };
  } catch {
    return { language: "zh-CN", plugins: false, executablePath: "" };
  }
}

export function useCodexDesktopPreferences(): CodexDesktopPreferences {
  const raw = useSyncExternalStore(subscribe, snapshot, () => DEFAULT);
  return useMemo(() => decode(raw), [raw]);
}

export function getCodexDesktopPreferences(): CodexDesktopPreferences {
  return decode(snapshot());
}

export function setCodexDesktopPreferences(
  updates: Partial<CodexDesktopPreferences>,
): boolean {
  try {
    const next = { ...decode(snapshot()), ...updates };
    writePreference(KEY, JSON.stringify(next));
    window.dispatchEvent(new Event(CHANGE));
    return true;
  } catch {
    return false;
  }
}

export const codexDesktop = {
  status: (executablePath = "") =>
    invoke<CodexDesktopStatus>("codex_desktop_status", { executablePath }),
  launch: (preferences: CodexDesktopPreferences) =>
    invoke<CodexDesktopLaunchResult>("launch_codex_desktop", {
      language: preferences.language,
      plugins: preferences.plugins,
      executablePath: preferences.executablePath,
    }),
};
