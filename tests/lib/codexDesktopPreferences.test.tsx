import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke }));
import {
  codexDesktop,
  setCodexDesktopPreferences,
  useCodexDesktopPreferences,
} from "@/lib/codexDesktop";

describe("Codex Desktop preferences", () => {
  beforeEach(() => {
    localStorage.clear();
    invoke.mockReset();
  });
  it("preserves older preferences and lets users clear a selected program", () => {
    localStorage.setItem(
      "aihubmix:codex-desktop-preferences",
      JSON.stringify({ language: "en-US", plugins: true }),
    );
    const { result } = renderHook(() => useCodexDesktopPreferences());
    expect(result.current).toEqual({
      language: "en-US",
      plugins: true,
      executablePath: "",
    });
    act(() => {
      setCodexDesktopPreferences({
        executablePath: "C:\\Portable\\ChatGPT.exe",
      });
    });
    expect(result.current.executablePath).toBe("C:\\Portable\\ChatGPT.exe");
    act(() => {
      setCodexDesktopPreferences({ executablePath: "" });
    });
    expect(result.current).toEqual({
      language: "en-US",
      plugins: true,
      executablePath: "",
    });
  });
  it("passes the selected executable to both status and launch", async () => {
    const preferences = {
      language: "system" as const,
      plugins: false,
      executablePath: "C:\\Portable\\ChatGPT.exe",
    };
    await codexDesktop.status(preferences.executablePath);
    await codexDesktop.launch(preferences);
    expect(invoke).toHaveBeenNthCalledWith(1, "codex_desktop_status", {
      executablePath: preferences.executablePath,
    });
    expect(invoke).toHaveBeenNthCalledWith(
      2,
      "launch_codex_desktop",
      preferences,
    );
  });
});
