import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const backend = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: backend.invoke,
  Channel: class {
    onmessage: (value: unknown) => void = () => {};
  },
}));

import { CodexDesktopControls } from "@/components/aihubmix/CodexDesktopControls";
import { CodexDesktopSettings } from "@/components/settings/CodexDesktopSettings";
import { setCodexDesktopPreferences } from "@/lib/codexDesktop";

beforeEach(() => {
  localStorage.clear();
  backend.invoke.mockReset();
  backend.invoke.mockImplementation((command: string) => {
    if (command === "codex_desktop_status")
      return Promise.resolve({
        installed: true,
        running: false,
        platform: "macos",
      });
    throw new Error("Unexpected command");
  });
});

describe("Codex Desktop controls", () => {
  it("clears the loading state without showing a success message", async () => {
    setCodexDesktopPreferences({
      language: "system",
      plugins: true,
      executablePath: "",
    });
    backend.invoke.mockImplementation((command: string) =>
      command === "codex_desktop_status"
        ? Promise.resolve({
            installed: true,
            running: false,
            platform: "windows",
          })
        : Promise.resolve({
            status: "started",
            locale: "not_requested",
            plugins: "configured",
          }),
    );
    render(<CodexDesktopControls />);
    fireEvent.click(screen.getByRole("button", { name: "启动 Codex" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "启动 Codex" })).toBeEnabled(),
    );
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("clears the loading state while Windows applies the language in the background", async () => {
    setCodexDesktopPreferences({ language: "zh-CN", plugins: false });
    backend.invoke.mockImplementation((command: string) =>
      command === "codex_desktop_status"
        ? Promise.resolve({
            installed: true,
            running: false,
            platform: "windows",
          })
        : Promise.resolve({
            status: "started",
            locale: "scheduled",
            plugins: "not_requested",
          }),
    );
    render(<CodexDesktopControls />);
    fireEvent.click(screen.getByRole("button", { name: "启动 Codex" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "启动 Codex" })).toBeEnabled(),
    );
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("offers manual selection of a Linux desktop executable in settings", async () => {
    const ua = vi
      .spyOn(navigator, "userAgent", "get")
      .mockReturnValue("X11; Linux x86_64");
    try {
      backend.invoke.mockResolvedValue({
        installed: false,
        running: false,
        platform: "linux",
      });
      render(<CodexDesktopSettings />);
      expect(await screen.findByText("未找到，请手动选择")).toBeInTheDocument();
      expect(
        screen.getByRole("button", { name: "选择 Codex 程序" }),
      ).toBeEnabled();
    } finally {
      ua.mockRestore();
    }
  });

  it("shows the Linux graphical session requirement on launch", async () => {
    const message = "请在 Linux 图形桌面中启动 Codex。";
    backend.invoke.mockImplementation((command: string) =>
      command === "codex_desktop_status"
        ? Promise.resolve({
            installed: true,
            running: false,
            platform: "linux",
          })
        : Promise.reject(message),
    );
    render(<CodexDesktopControls />);
    fireEvent.click(screen.getByRole("button", { name: "启动 Codex" }));
    expect(await screen.findByRole("status")).toHaveTextContent(message);
  });

  it("allows retrying after plugin synchronization finishes", async () => {
    const message = "插件目录正在更新，请稍后再启动 Codex。";
    backend.invoke.mockImplementation((command: string) =>
      command === "codex_desktop_status"
        ? Promise.resolve({
            installed: true,
            running: false,
            platform: "windows",
          })
        : Promise.reject(message),
    );
    render(<CodexDesktopControls />);
    fireEvent.click(screen.getByRole("button", { name: "启动 Codex" }));
    expect(await screen.findByText(message)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "启动 Codex" })).toBeEnabled();
  });

  it("shows actual download progress and the verification phase", async () => {
    let report: (value: unknown) => void = () => {};
    let finish: (value: unknown) => void = () => {};
    backend.invoke.mockImplementation(
      (
        command: string,
        args?: { progress?: { onmessage: (value: unknown) => void } },
      ) => {
        if (command === "enable_codex_plugin_market") {
          report = (value) => args?.progress?.onmessage(value);
          return new Promise((resolve) => {
            finish = resolve;
          });
        }
        if (command === "codex_desktop_status")
          return Promise.resolve({
            installed: true,
            running: false,
            platform: "macos",
          });
        throw new Error("Unexpected command");
      },
    );
    render(<CodexDesktopSettings />);
    fireEvent.click(screen.getByRole("switch", { name: "社区插件市场" }));
    expect(
      await screen.findByText(/codexMarket.progress.preparing/),
    ).toBeInTheDocument();
    act(() =>
      report({
        phase: "downloading",
        downloaded: 25 * 1024 * 1024,
        total: 100 * 1024 * 1024,
      }),
    );
    expect(
      screen.getByText("codexMarket.progress.downloadingWithTotal"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("progressbar", {
        name: "codexMarket.aria.downloadProgress",
      }),
    ).toHaveValue(25 * 1024 * 1024);
    act(() => report({ phase: "verifying", downloaded: 100, total: 100 }));
    expect(
      screen.getByText(/codexMarket.progress.installing/),
    ).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
    await act(async () => finish({ revision: "test", restartRequired: true }));
    expect(
      screen.queryByText(/codexMarket.progress.installing/),
    ).not.toBeInTheDocument();
  });

  it("clears a failed download and allows retry before enabling the market", async () => {
    backend.invoke.mockImplementation((command: string) =>
      command === "enable_codex_plugin_market"
        ? Promise.reject("插件包下载中断")
        : Promise.resolve({
            installed: true,
            running: false,
            platform: "macos",
          }),
    );
    render(<CodexDesktopSettings />);
    fireEvent.click(screen.getByRole("switch", { name: "社区插件市场" }));
    expect(await screen.findByText("插件包下载中断")).toBeInTheDocument();
    expect(
      screen.queryByText(/codexMarket.progress.preparing/),
    ).not.toBeInTheDocument();
    expect(
      screen.getByRole("switch", { name: "社区插件市场" }),
    ).toHaveAttribute("data-state", "unchecked");
    backend.invoke.mockResolvedValue({
      revision: "retry",
      restartRequired: true,
    });
    fireEvent.click(screen.getByRole("switch", { name: "社区插件市场" }));
    await waitFor(() =>
      expect(
        screen.getByRole("switch", { name: "社区插件市场" }),
      ).toHaveAttribute("data-state", "checked"),
    );
    expect(screen.queryByText("插件包下载中断")).not.toBeInTheDocument();
    expect(
      screen.queryByText(/codexMarket.progress.preparing/),
    ).not.toBeInTheDocument();
  });
  it("launches with the requested preferences and omits unverified plugin status", async () => {
    setCodexDesktopPreferences({
      language: "zh-CN",
      plugins: true,
      executablePath: "",
    });
    backend.invoke.mockImplementation((command: string) => {
      if (command === "codex_desktop_status")
        return Promise.resolve({
          installed: true,
          running: false,
          platform: "macos",
        });
      if (command === "launch_codex_desktop")
        return Promise.resolve({
          status: "partial",
          locale: "applied",
          plugins: "not_confirmed",
        });
      throw new Error("Unexpected command");
    });
    render(<CodexDesktopControls />);
    fireEvent.click(screen.getByRole("button", { name: "启动 Codex" }));
    await waitFor(() =>
      expect(backend.invoke).toHaveBeenCalledWith("launch_codex_desktop", {
        language: "zh-CN",
        plugins: true,
        executablePath: "",
      }),
    );
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "启动 Codex" })).toBeEnabled(),
    );
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
  });

  it("explains when an automatic quit is unsuccessful", async () => {
    backend.invoke.mockImplementation((command: string) =>
      command === "codex_desktop_status"
        ? Promise.resolve({ installed: true, running: true, platform: "macos" })
        : Promise.resolve({
            status: "already_running",
            locale: "not_requested",
            plugins: "not_confirmed",
          }),
    );
    render(<CodexDesktopControls />);
    fireEvent.click(screen.getByRole("button", { name: "启动 Codex" }));
    expect(
      await screen.findByText(/未能自动退出 Codex，请保存工作并手动退出后重试/),
    ).toBeInTheDocument();
    expect(screen.queryByText(/已生效/)).not.toBeInTheDocument();
  });

  it("shows only known safe launch errors", async () => {
    backend.invoke.mockImplementation((command: string) =>
      command === "codex_desktop_status"
        ? Promise.resolve({
            installed: true,
            running: false,
            platform: "windows",
          })
        : Promise.reject("检查 Codex Desktop 超时"),
    );
    render(<CodexDesktopControls />);
    fireEvent.click(screen.getByRole("button", { name: "启动 Codex" }));
    expect(
      await screen.findByText("检查 Codex Desktop 超时"),
    ).toBeInTheDocument();
  });

  it("does not show unexpected launch errors containing sensitive information", async () => {
    backend.invoke.mockImplementation((command: string) =>
      command === "codex_desktop_status"
        ? Promise.resolve({
            installed: true,
            running: false,
            platform: "windows",
          })
        : Promise.reject("secret-api-key"),
    );
    render(<CodexDesktopControls />);
    fireEvent.click(screen.getByRole("button", { name: "启动 Codex" }));
    expect(await screen.findByText(/启动失败，请检查/)).toBeInTheDocument();
    expect(screen.queryByText(/secret-api-key/)).not.toBeInTheDocument();
  });

  it("only stores the plugin preference after the market is registered", async () => {
    backend.invoke.mockImplementation((command: string) => {
      if (command === "enable_codex_plugin_market")
        return Promise.resolve({
          revision: "fixed-revision",
          restartRequired: true,
        });
      throw new Error("Unexpected command");
    });
    render(<CodexDesktopSettings />);
    const plugins = screen.getByRole("switch", { name: "社区插件市场" });
    expect(plugins).not.toBeChecked();
    fireEvent.click(plugins);
    await waitFor(() => expect(plugins).toBeChecked());
    expect(backend.invoke).toHaveBeenCalledWith(
      "enable_codex_plugin_market",
      expect.objectContaining({ progress: expect.anything() }),
    );
    expect(
      localStorage.getItem("aihubmix:codex-desktop-preferences"),
    ).toContain('"plugins":true');
    expect(screen.getByRole("status")).toHaveTextContent("插件市场已开启");
    expect(screen.queryByText(/插件支持已生效/)).not.toBeInTheDocument();
  });

  it("leaves the plugin setting off when registration fails", async () => {
    backend.invoke.mockRejectedValue(new Error("private transport details"));
    render(<CodexDesktopSettings />);
    const plugins = screen.getByRole("switch", { name: "社区插件市场" });
    fireEvent.click(plugins);
    expect(await screen.findByRole("alert")).toHaveTextContent("操作未能确认");
    expect(plugins).not.toBeChecked();
    expect(
      screen.queryByText(/private transport details/),
    ).not.toBeInTheDocument();
  });

  it("shows a safe, helpful message for rate limits", async () => {
    backend.invoke.mockRejectedValue("CODEX_MARKET_RATE_LIMITED");
    render(<CodexDesktopSettings />);
    fireEvent.click(screen.getByRole("switch", { name: "社区插件市场" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "插件目录访问暂时达到次数限制，请稍后再试。",
    );
    expect(
      screen.queryByText(/CODEX_MARKET_RATE_LIMITED/),
    ).not.toBeInTheDocument();
  });

  it("turns off the market without uninstalling plugins", async () => {
    setCodexDesktopPreferences({ plugins: true });
    backend.invoke.mockImplementation((command: string) => {
      if (command === "disable_codex_plugin_market")
        return Promise.resolve({ restartRequired: true });
      throw new Error("Unexpected command");
    });
    render(<CodexDesktopSettings />);
    const plugins = screen.getByRole("switch", { name: "社区插件市场" });
    fireEvent.click(plugins);
    await waitFor(() => expect(plugins).not.toBeChecked());
    expect(screen.getByRole("status")).toHaveTextContent("插件市场已关闭");
    expect(backend.invoke).toHaveBeenCalledWith("disable_codex_plugin_market", {
      executablePath: "",
    });
    expect(backend.invoke).not.toHaveBeenCalledWith(
      "uninstall_codex_market_plugin",
      expect.anything(),
    );
    expect(
      localStorage.getItem("aihubmix:codex-desktop-preferences"),
    ).toContain('"plugins":false');
  });

  it("searches and installs a community plugin without claiming account authorization", async () => {
    backend.invoke.mockImplementation((command: string) => {
      if (command === "list_codex_market_plugins")
        return Promise.resolve({
          source: "社区公开插件缓存",
          revision: "fixed-revision",
          plugins: [
            {
              id: "one",
              name: "notes",
              displayName: "Notes",
              description: "笔记",
              author: "Example",
              tags: [],
              skills: 1,
              requiresAuth: false,
              installed: false,
            },
            {
              id: "two",
              name: "drive",
              displayName: "Drive",
              description: "云盘",
              author: "Example",
              tags: [],
              skills: 2,
              requiresAuth: true,
              installed: false,
            },
          ],
        });
      if (command === "install_codex_market_plugin")
        return Promise.resolve({ pluginId: "two", restartRequired: true });
      throw new Error("Unexpected command");
    });
    render(<CodexDesktopSettings />);
    fireEvent.click(screen.getByRole("button", { name: "管理插件" }));
    expect(await screen.findByText("Drive")).toBeInTheDocument();
    expect(screen.getByText("pluginMarket.warning")).toBeInTheDocument();
    fireEvent.change(
      screen.getByRole("textbox", { name: "pluginMarket.aria.search" }),
      { target: { value: "Drive" } },
    );
    expect(screen.queryByText("Notes")).not.toBeInTheDocument();
    expect(
      screen.getByText("Example · pluginMarket.auth.mayRequire"),
    ).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "pluginMarket.action.install" }),
    );
    await waitFor(() =>
      expect(backend.invoke).toHaveBeenCalledWith(
        "install_codex_market_plugin",
        { pluginId: "two", revision: "fixed-revision", executablePath: "" },
      ),
    );
    expect(
      await screen.findByRole("button", {
        name: "pluginMarket.action.uninstall",
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByText("pluginMarket.notice.installedConnectAuth"),
    ).toBeInTheDocument();
  });

  it("does not describe unknown plugin state as not installed", async () => {
    backend.invoke.mockImplementation((command: string) => {
      if (command === "list_codex_market_plugins")
        return Promise.resolve({
          source: "社区公开插件缓存",
          revision: "fixed-revision",
          plugins: [
            {
              id: "one",
              name: "notes",
              displayName: "Notes",
              description: "笔记",
              author: "Example",
              tags: [],
              skills: 1,
              requiresAuth: false,
              installed: null,
            },
          ],
        });
      throw new Error("Unexpected command");
    });
    render(<CodexDesktopSettings />);
    fireEvent.click(screen.getByRole("button", { name: "管理插件" }));
    expect(await screen.findByText("Notes")).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: "pluginMarket.action.viewAfterEnable",
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("button", { name: "pluginMarket.action.enableCatalog" }),
    ).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: "pluginMarket.action.install" }),
    ).not.toBeInTheDocument();
  });
});
