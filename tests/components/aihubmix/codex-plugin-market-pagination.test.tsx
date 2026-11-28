import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";

vi.mock("@/lib/codexPluginMarket", () => ({
  codexMarketError: (_error: unknown, fallback: string) => fallback,
  codexPluginMarket: {
    list: vi.fn().mockResolvedValue({
      source: "community",
      revision: "fixture",
      downloadBytes: 393081888,
      plugins: Array.from({ length: 165 }, (_, index) => ({
        id: `plugin-${index}`,
        name: `plugin-${index}`,
        displayName: `Plugin ${index}`,
        description: "",
        author: "",
        tags: [],
        requiresAuth: false,
        installed: false,
      })),
    }),
    enable: vi.fn(),
    install: vi.fn(),
    uninstall: vi.fn(),
  },
}));

import { codexPluginMarket } from "@/lib/codexPluginMarket";
import {
  getCodexDesktopPreferences,
  setCodexDesktopPreferences,
} from "@/lib/codexDesktop";
import { settingsApi } from "@/lib/api/settings";
import { CodexPluginMarketDialog } from "@/components/aihubmix/CodexPluginMarketDialog";

function openMarket() {
  render(
    <CodexPluginMarketDialog>
      <button>管理插件</button>
    </CodexPluginMarketDialog>,
  );
  const trigger = screen.getByRole("button", { name: "管理插件" });
  fireEvent.click(trigger);
  return trigger;
}

describe("Codex plugin catalog pagination", () => {
  it("closes from the header or Escape and returns focus to the opener", async () => {
    const trigger = openMarket();
    const search = screen.getByRole("textbox", {
      name: "pluginMarket.aria.search",
    });
    expect(search).toHaveFocus();
    const close = screen.getByRole("button", {
      name: "pluginMarket.aria.close",
    });
    expect(search.closest(".overflow-y-auto")).not.toContainElement(close);
    fireEvent.click(close);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await waitFor(() => expect(trigger).toHaveFocus());

    fireEvent.click(trigger);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    await waitFor(() => expect(trigger).toHaveFocus());
  });

  it("opens the official account entry without claiming a local installation", async () => {
    const open = vi.spyOn(settingsApi, "openExternal").mockResolvedValue();
    vi.mocked(codexPluginMarket.list).mockResolvedValueOnce({
      source: "community",
      revision: "fixture",
      downloadBytes: 0,
      plugins: [
        {
          id: "adobe",
          name: "adobe",
          displayName: "Adobe",
          description: "",
          version: "",
          author: "",
          license: "",
          requiresAuth: true,
          installed: null,
          skills: 0,
          tags: [],
          inApiCatalog: false,
          officialUrl: "https://chatgpt.com/plugins",
        },
      ],
    });
    openMarket();
    fireEvent.click(
      await screen.findByRole("button", {
        name: "pluginMarket.action.gotoChatgpt",
      }),
    );
    expect(open).toHaveBeenCalledWith("https://chatgpt.com/plugins");
    expect(codexPluginMarket.install).not.toHaveBeenCalled();
    open.mockRestore();
  });

  it("prepares the full library only on request", async () => {
    openMarket();
    await screen.findByText(/pluginMarket.hint.download/);
    expect(codexPluginMarket.enable).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getByRole("button", {
        name: "pluginMarket.action.prepareLibrary",
      }),
    );
    expect(codexPluginMarket.enable).toHaveBeenCalledWith(
      expect.any(Function),
      true,
    );
    await screen.findByText("pluginMarket.notice.localReady");
  });
  it("lets users browse past the first 80 and searches the entire catalog", async () => {
    openMarket();
    expect(
      await screen.findByText(/pluginMarket.status.found/),
    ).toBeInTheDocument();
    expect(screen.queryByText("Plugin 160")).not.toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "pluginMarket.action.loadMore" }),
    );
    expect(screen.getByText("Plugin 159")).toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "pluginMarket.action.loadMore" }),
    );
    expect(screen.getByText("Plugin 164")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "pluginMarket.action.loadMore" }),
    ).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("pluginMarket.aria.search"), {
      target: { value: "plugin-164" },
    });
    expect(screen.getByText("Plugin 164")).toBeInTheDocument();
    expect(screen.getByText(/pluginMarket.status.found/)).toBeInTheDocument();
  });

  it("keeps an in-flight market change open until the preference is saved", async () => {
    setCodexDesktopPreferences({ plugins: false });
    let complete!: () => void;
    vi.mocked(codexPluginMarket.enable).mockReturnValueOnce(
      new Promise((resolve) => {
        complete = () => resolve({ restartRequired: true });
      }),
    );
    const trigger = openMarket();
    fireEvent.click(
      await screen.findByRole("button", {
        name: "pluginMarket.action.syncOfficial",
      }),
    );
    const close = screen.getByRole("button", {
      name: "pluginMarket.aria.close",
    });
    expect(close).toBeDisabled();
    fireEvent.click(close);
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.getByRole("dialog")).toBeVisible();
    expect(getCodexDesktopPreferences().plugins).toBe(false);

    await act(async () => complete());
    expect(getCodexDesktopPreferences().plugins).toBe(true);
    expect(close).toBeEnabled();
    fireEvent.click(close);
    await waitFor(() => expect(trigger).toHaveFocus());
  });
});
