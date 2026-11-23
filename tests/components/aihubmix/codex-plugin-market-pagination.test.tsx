import { fireEvent, render, screen } from "@testing-library/react";
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
import { CodexPluginMarketDialog } from "@/components/aihubmix/CodexPluginMarketDialog";

describe("Codex plugin catalog pagination", () => {
  it("prepares the full library only on request", async () => {
    render(<CodexPluginMarketDialog onClose={() => {}} />);
    await screen.findByText(/375 MiB/);
    expect(codexPluginMarket.enable).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "准备 Codex 插件库" }));
    expect(codexPluginMarket.enable).toHaveBeenCalledWith(
      expect.any(Function),
      true,
    );
    await screen.findByText(/本机插件库已就绪/);
  });
  it("lets users browse past the first 80 and searches the entire catalog", async () => {
    render(<CodexPluginMarketDialog onClose={() => {}} />);
    expect(
      await screen.findByText("找到 165 个插件，已显示 80 个"),
    ).toBeInTheDocument();
    expect(screen.queryByText("Plugin 160")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "加载更多插件" }));
    expect(screen.getByText("Plugin 159")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "加载更多插件" }));
    expect(screen.getByText("Plugin 164")).toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "加载更多插件" }),
    ).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("搜索插件"), {
      target: { value: "plugin-164" },
    });
    expect(screen.getByText("Plugin 164")).toBeInTheDocument();
    expect(screen.getByText("找到 1 个插件")).toBeInTheDocument();
  });
});
