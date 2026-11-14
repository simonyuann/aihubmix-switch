import { beforeEach, describe, expect, it, vi } from "vitest";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke,
  Channel: class {
    onmessage = () => {};
  },
}));

import { setCodexDesktopPreferences } from "@/lib/codexDesktop";
import { codexPluginMarket, codexMarketError } from "@/lib/codexPluginMarket";

describe("plugin management uses the selected Desktop installation", () => {
  beforeEach(() => {
    localStorage.clear();
    invoke.mockReset();
  });

  it("passes a portable installation to every plugin operation", async () => {
    const executablePath = "D:\\应用 程序\\ChatGPT\\ChatGPT.exe";
    setCodexDesktopPreferences({ executablePath });
    await codexPluginMarket.list();
    await codexPluginMarket.enable();
    await codexPluginMarket.disable();
    await codexPluginMarket.install("demo", "revision");
    await codexPluginMarket.uninstall("demo");
    expect(invoke.mock.calls.map(([command]) => command)).toEqual([
      "list_codex_market_plugins",
      "enable_codex_plugin_market",
      "disable_codex_plugin_market",
      "install_codex_market_plugin",
      "uninstall_codex_market_plugin",
    ]);
    for (const [, args] of invoke.mock.calls) {
      expect(args).toMatchObject({ executablePath });
    }
  });

  it("reads path changes and clearing on the next operation", async () => {
    setCodexDesktopPreferences({ executablePath: "D:\\old\\ChatGPT.exe" });
    await codexPluginMarket.list();
    setCodexDesktopPreferences({ executablePath: "E:\\new\\Codex.exe" });
    await codexPluginMarket.install("demo", "revision");
    setCodexDesktopPreferences({ executablePath: "" });
    await codexPluginMarket.disable();
    expect(invoke.mock.calls.map(([, args]) => args.executablePath)).toEqual([
      "D:\\old\\ChatGPT.exe",
      "E:\\new\\Codex.exe",
      "",
    ]);
  });

  it("keeps automatic discovery for existing preferences", async () => {
    localStorage.setItem(
      "aihubmix:codex-desktop-preferences",
      '{"plugins":true}',
    );
    await codexPluginMarket.list();
    expect(invoke).toHaveBeenCalledWith("list_codex_market_plugins", {
      executablePath: "",
    });
  });

  it("shows safe path guidance while filtering process output", () => {
    const message = "所选 Codex 路径不可用，请在设置中重新选择程序";
    expect(codexMarketError(message, "fallback")).toBe(message);
    expect(codexMarketError("C:\\private token=secret", "fallback")).toBe(
      "fallback",
    );
  });
});
