import {
  QueryClient,
  QueryClientProvider,
  onlineManager,
} from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ dispatch: vi.fn(), imagegenGuide: vi.fn() }));
vi.mock("@/lib/api/aihubmix", () => ({ dispatch: mocks.dispatch }));
vi.mock("@tauri-apps/api/core", async (importOriginal) => {
  const original =
    await importOriginal<typeof import("@tauri-apps/api/core")>();
  return {
    ...original,
    invoke: (
      command: string,
      ...args: Parameters<typeof original.invoke> extends [
        string,
        ...infer Rest,
      ]
        ? Rest
        : never
    ) =>
      command === "codex_imagegen_guide"
        ? mocks.imagegenGuide()
        : original.invoke(command, ...args),
  };
});
import { ToolPage } from "@/components/aihubmix/ToolPage";
import { ApplyApiDialog } from "@/components/aihubmix/ApplyApiDialog";
const official = {
  provider: {
    id: "codex-official",
    name: "OpenAI Official",
    category: "official",
    settingsConfig: { auth: {}, config: "" },
  },
  configured: true,
  keyMasked: "",
};
const service = {
  provider: {
    id: "aihubmix-codex-account-1-service-7",
    name: "AIHubMix · OpenAI",
    category: "custom",
    websiteUrl: "https://csmy.fun:23457",
    settingsConfig: { auth: { OPENAI_API_KEY: "***" }, config: "" },
  },
  configured: false,
  keyMasked: "",
  groupId: 7,
  platform: "openai",
  selectedKeyId: null,
};
const keys = [
  {
    id: 10,
    name: "OpenAI Key",
    keyMasked: "sk-••••open",
    groupId: 7,
    status: "active",
    local: false,
  },
  {
    id: 20,
    name: "DeepSeek Key",
    keyMasked: "sk-••••deep",
    groupId: 8,
    status: "active",
    local: false,
  },
  {
    id: 21,
    name: "已禁用 Key",
    keyMasked: "sk-••••off",
    groupId: 7,
    status: "disabled",
    local: false,
  },
];
function wrap(node: React.ReactNode) {
  return render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      {node}
    </QueryClientProvider>,
  );
}
async function hoverImageGroup() {
  const disclosure = await screen.findByRole("button", {
    name: `${service.provider.name} 的生图模型`,
  });
  fireEvent.pointerEnter(disclosure.closest("article")!);
  return disclosure;
}
beforeEach(() => {
  vi.clearAllMocks();
  mocks.imagegenGuide.mockResolvedValue({ status: "unavailable" });
  mocks.dispatch.mockImplementation((command) =>
    Promise.resolve(
      command === "tool-cards"
        ? { cards: [official, service], currentProviderId: null }
        : command === "keys"
          ? keys
          : {},
    ),
  );
});
describe("dedicated service cards", () => {
  it("selects an image model from the active group on the home page and restores the gateway default", async () => {
    Element.prototype.scrollIntoView ??= vi.fn();
    let selected: string | null = null;
    const other = {
      ...service,
      provider: { ...service.provider, id: "another-group" },
      groupId: 8,
      imageModel: "gpt-image-1",
    };
    mocks.dispatch.mockImplementation(async (command, payload) => {
      if (command === "tool-cards")
        return {
          cards: [
            { ...service, configured: true, imageModel: selected },
            other,
          ],
          currentProviderId: service.provider.id,
        };
      if (command === "tool-image-models")
        return [{ id: "gpt-image-2.6-sunburst", name: "2.6 Sunburst" }];
      if (command === "set-tool-image-model") {
        expect(payload).toMatchObject({
          app: "codex",
          groupId: 7,
          providerId: service.provider.id,
        });
        selected = payload.model;
        return { model: selected, restartRequired: true };
      }
      return {};
    });
    wrap(<ToolPage app="codex" simple />);
    await hoverImageGroup();
    const picker = await screen.findByRole("combobox", { name: "生图模型" });
    expect(picker).toHaveTextContent("跟随网关");
    fireEvent.keyDown(picker, { key: "ArrowDown" });
    fireEvent.click(
      await screen.findByRole("option", { name: "2.6 Sunburst" }),
    );
    await waitFor(() => expect(picker).toHaveTextContent("2.6 Sunburst"));
    expect(
      await screen.findByText("toolPage.notice.imagegenSavedRestart"),
    ).toBeInTheDocument();
    await waitFor(() => expect(picker).toBeEnabled());
    fireEvent.keyDown(picker, { key: "ArrowDown" });
    fireEvent.click(await screen.findByRole("option", { name: /跟随网关/ }));
    await waitFor(() => expect(picker).toHaveTextContent("跟随网关"));
    expect(selected).toBeNull();
  });
  it("collapses the image row at rest and holds it open while choosing from the portalled menu", async () => {
    Element.prototype.scrollIntoView ??= vi.fn();
    mocks.dispatch.mockImplementation(async (command) => {
      if (command === "tool-cards")
        return {
          cards: [{ ...service, configured: true, imageModel: null }],
          currentProviderId: service.provider.id,
        };
      return [];
    });
    wrap(<ToolPage app="codex" simple />);
    const disclosure = await screen.findByRole("button", {
      name: `${service.provider.name} 的生图模型`,
    });
    const row = disclosure.closest("article")!;
    const panel = document.getElementById(
      disclosure.getAttribute("aria-controls")!,
    )!;
    expect(disclosure).toHaveAttribute("aria-expanded", "false");
    expect(panel).toHaveAttribute("inert");
    expect(screen.queryByRole("combobox", { name: "生图模型" })).toBeNull();

    fireEvent.pointerEnter(row);
    const picker = screen.getByRole("combobox", { name: "生图模型" });
    fireEvent.keyDown(picker, { key: "ArrowDown" });
    const option = await screen.findByRole("option", { name: /跟随网关/ });
    expect(row.contains(option)).toBe(false);
    fireEvent.pointerLeave(row);
    expect(disclosure).toHaveAttribute("aria-expanded", "true");
    fireEvent.click(option);
    await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());
    expect(disclosure).toHaveAttribute("aria-expanded", "false");

    // A tap/assistive activation also works without any hover event.
    fireEvent.click(disclosure);
    expect(panel).not.toHaveAttribute("inert");
    fireEvent.pointerDown(document.body);
    expect(panel).toHaveAttribute("inert");

    document.documentElement.dataset.keyboard = "";
    try {
      act(() => disclosure.focus());
      expect(disclosure).toHaveAttribute("aria-expanded", "true");
      fireEvent.click(disclosure);
      expect(picker).toHaveFocus();
      act(() =>
        screen
          .getByRole("button", { name: "toolPage.action.addGroup" })
          .focus(),
      );
      expect(panel).toHaveAttribute("inert");
    } finally {
      delete document.documentElement.dataset.keyboard;
    }
  });
  it("refreshes the active group's image models on every open and selects a newly available model", async () => {
    Element.prototype.scrollIntoView ??= vi.fn();
    const saved = { id: "gpt-image-2.6-sunburst", name: "2.6 Sunburst" };
    const retired = { id: "gpt-image-2.5-flare", name: "2.5 Flare" };
    const added = { id: "gpt-image-2.7", name: "gpt-image-2.7" };
    let models = Promise.resolve([saved, retired]);
    let selected = saved.id;
    mocks.dispatch.mockImplementation(async (command, payload) => {
      if (command === "tool-cards")
        return {
          cards: [{ ...service, configured: true, imageModel: selected }],
          currentProviderId: service.provider.id,
        };
      if (command === "tool-image-models") return models;
      if (command === "set-tool-image-model") {
        selected = payload.model;
        return { model: selected, restartRequired: false };
      }
      return {};
    });
    wrap(<ToolPage app="codex" simple />);
    await hoverImageGroup();
    const picker = await screen.findByRole("combobox", { name: "生图模型" });
    fireEvent.keyDown(picker, { key: "ArrowDown" });
    const initialOption = await screen.findByRole("option", {
      name: retired.name,
    });
    fireEvent.keyDown(initialOption, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("listbox")).toBeNull());

    let finishRefresh!: (value: (typeof saved)[]) => void;
    models = new Promise((resolve) => {
      finishRefresh = resolve;
    });
    fireEvent.keyDown(picker, { key: "ArrowDown" });
    expect(await screen.findByRole("listbox")).toBeVisible();
    expect(screen.queryByRole("option", { name: retired.name })).toBeNull();
    await act(async () => finishRefresh([saved, added]));
    const newOption = await screen.findByRole("option", { name: "2.7" });
    expect(screen.queryByRole("option", { name: retired.name })).toBeNull();
    fireEvent.click(newOption);
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("set-tool-image-model", {
        app: "codex",
        groupId: service.groupId,
        providerId: service.provider.id,
        model: added.id,
      }),
    );
    await waitFor(() => expect(picker).toHaveTextContent("2.7"));
  });
  it("identifies a retired saved model on a known image-capable card even without platform metadata", async () => {
    Element.prototype.scrollIntoView ??= vi.fn();
    mocks.dispatch.mockImplementation(async (command) => {
      if (command === "tool-cards") {
        return {
          cards: [
            {
              ...service,
              configured: true,
              imageModel: "gpt-image-2.5-sunburst",
              platform: undefined,
            },
          ],
          currentProviderId: service.provider.id,
        };
      }
      return [];
    });
    wrap(<ToolPage app="codex" simple />);
    await hoverImageGroup();
    expect(
      await screen.findByText("当前分组已不提供所选模型，请重新选择。"),
    ).toBeVisible();
    const picker = screen.getByRole("combobox", { name: "生图模型" });
    expect(picker).toHaveTextContent("2.5 Sunburst");
    fireEvent.keyDown(picker, { key: "ArrowDown" });
    expect(
      await screen.findByRole("option", { name: /跟随网关/ }),
    ).toBeVisible();
    expect(screen.queryByRole("option", { name: "2.5 Sunburst" })).toBeNull();
    expect(mocks.dispatch).not.toHaveBeenCalledWith(
      "set-tool-image-model",
      expect.anything(),
    );
  });
  it("requires a chosen replacement before removing the current group", async () => {
    mocks.dispatch.mockImplementation((command) =>
      Promise.resolve(
        command === "tool-cards"
          ? {
              cards: [official, { ...service, configured: true }],
              currentProviderId: service.provider.id,
            }
          : {},
      ),
    );
    wrap(<ToolPage app="codex" simple />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "toolPage.aria.remove",
      }),
    );
    const confirm = screen.getByRole("button", {
      name: "toolPage.action.switchAndRemove",
    });
    expect(confirm).toBeDisabled();
    fireEvent.change(
      screen.getByRole("combobox", {
        name: "toolPage.dialog.replacementGroup",
      }),
      {
        target: { value: official.provider.id },
      },
    );
    fireEvent.click(confirm);
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("remove-tool-card", {
        app: "codex",
        providerId: service.provider.id,
        groupId: 7,
        replacementId: official.provider.id,
      }),
    );
  });
  it("removes an unused group without activating another provider", async () => {
    wrap(<ToolPage app="codex" simple />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "toolPage.aria.remove",
      }),
    );
    expect(screen.queryByText("toolPage.dialog.replacementGroup")).toBeNull();
    fireEvent.click(
      screen.getByRole("button", { name: "toolPage.action.confirmRemove" }),
    );
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("remove-tool-card", {
        app: "codex",
        providerId: service.provider.id,
        groupId: 7,
      }),
    );
  });
  it("loads saved cards offline and refreshes them once the browser reconnects", async () => {
    const wasOnline = onlineManager.isOnline();
    onlineManager.setOnline(false);
    try {
      wrap(<ToolPage app="codex" simple />);
      expect(await screen.findByText(service.provider.name)).toBeVisible();
      expect(mocks.dispatch).toHaveBeenCalledWith("tool-cards", {
        app: "codex",
      });
      mocks.dispatch.mockClear();
      act(() => onlineManager.setOnline(true));
      await waitFor(() =>
        expect(mocks.dispatch).toHaveBeenCalledWith("tool-cards", {
          app: "codex",
        }),
      );
    } finally {
      onlineManager.setOnline(wasOnline);
    }
  });

  it("replaces raw API loading diagnostics with a concise retry message", async () => {
    mocks.dispatch.mockImplementation((command) =>
      command === "keys"
        ? Promise.reject(
            "连接失败 https://example.invalid/private?token=fixture",
          )
        : Promise.resolve({ cards: [service], currentProviderId: null }),
    );
    wrap(<ToolPage app="codex" />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "toolPage.aria.start",
      }),
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "toolPage.error.loadKeys",
    );
    expect(screen.queryByText(/example.invalid/)).not.toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "toolPage.action.retry" }),
    ).toBeEnabled();
  });

  it("keeps service warnings useful without exposing raw diagnostics", async () => {
    mocks.dispatch.mockResolvedValue({
      cards: [service],
      currentProviderId: null,
      warning:
        "模型服务暂不可用：https://example.invalid/private?token=fixture",
    });
    wrap(<ToolPage app="codex" simple />);
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "toolPage.error.groupsUnavailable",
    );
    expect(screen.queryByText(/example.invalid/)).not.toBeInTheDocument();
  });

  it("offers a single tool selector on the simple home", async () => {
    const onSelectApp = vi.fn();
    wrap(<ToolPage app="codex" simple onSelectApp={onSelectApp} />);
    fireEvent.click(
      await screen.findByRole("button", { name: "toolPage.aria.selectTool" }),
    );
    expect(
      screen.getByRole("button", { name: "DeepSeek Harness" }),
    ).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "Claude Code" }));
    expect(onSelectApp).toHaveBeenCalledWith("claude");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("offers onboarding when only the official account is present", async () => {
    mocks.dispatch.mockResolvedValue({
      cards: [official],
      groups: [],
      currentProviderId: null,
    });
    wrap(<ToolPage app="codex" simple />);
    expect(
      await screen.findByText("toolPage.empty.noGroupsTitle"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "toolPage.action.addGroup" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("OpenAI Official")).not.toBeInTheDocument();
  });
  it("offers a direct official restore on the Codex home without a group card", async () => {
    wrap(<ToolPage app="codex" simple />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "toolPage.action.restoreOfficial",
      }),
    );
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("start-tool-card", {
        app: "codex",
        providerId: "codex-official",
      }),
    );
    expect(
      await screen.findByText("toolPage.notice.switchedOfficialRestart"),
    ).toBeInTheDocument();
    expect(
      screen.queryByText("OpenAI Official", { selector: "h3" }),
    ).not.toBeInTheDocument();
  });
  it("shows when the official account is already active", async () => {
    mocks.dispatch.mockImplementation((command) =>
      Promise.resolve(
        command === "tool-cards"
          ? {
              cards: [official, service],
              groups: [],
              currentProviderId: "codex-official",
            }
          : {},
      ),
    );
    wrap(<ToolPage app="codex" simple />);
    expect(
      await screen.findByRole("button", {
        name: "toolPage.action.officialCurrent",
      }),
    ).toBeDisabled();
  });
  it("activates a legacy group from home without asking for a key", async () => {
    wrap(<ToolPage app="codex" simple />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "toolPage.aria.switchTo",
      }),
    );
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("start-tool-card", {
        app: "codex",
        providerId: service.provider.id,
      }),
    );
    expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument();
  });
  it("shows the active service and switches a configured group directly", async () => {
    mocks.dispatch.mockImplementation((command) =>
      Promise.resolve(
        command === "tool-cards"
          ? {
              cards: [official, { ...service, configured: true }],
              groups: [],
              currentProviderId: official.provider.id,
            }
          : {},
      ),
    );
    wrap(<ToolPage app="codex" simple />);
    await screen.findByText("AIHubMix · OpenAI");
    expect(screen.queryByText("OpenAI Official")).not.toBeInTheDocument();
    expect(screen.queryByText("toolPage.status.inUse")).not.toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "toolPage.aria.switchTo" }),
    );
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("start-tool-card", {
        app: "codex",
        providerId: service.provider.id,
      }),
    );
  });
  it.each([true, false])(
    "allows the active Codex group to be reapplied (simple=%s)",
    async (simple) => {
      mocks.dispatch.mockImplementation((command) =>
        Promise.resolve(
          command === "tool-cards"
            ? {
                cards: [official, { ...service, configured: true }],
                groups: [],
                currentProviderId: service.provider.id,
              }
            : {},
        ),
      );
      wrap(<ToolPage app="codex" simple={simple} />);
      const button = await screen.findByRole("button", {
        name: "toolPage.aria.reEnable",
      });
      expect(screen.getByText("toolPage.status.inUse")).toBeInTheDocument();
      fireEvent.click(button);
      await waitFor(() =>
        expect(mocks.dispatch).toHaveBeenCalledWith("start-tool-card", {
          app: "codex",
          providerId: service.provider.id,
        }),
      );
      const notice = await screen.findByText(/toolPage.notice.restartCodex/);
      expect(notice).toHaveAttribute("role", "status");
      expect(notice).not.toHaveTextContent("toolPage.summary.imagegenFallback");
      expect(
        screen
          .getByText("toolPage.summary.imagegenFallback")
          .closest("details"),
      ).not.toHaveAttribute("open");
    },
  );
  it.each(
    [true, false].flatMap((simple) =>
      [
        ["openai", true],
        ["openai_responses", true],
        ["OpenAI", true],
        ["openai_chat", false],
        ["anthropic", false],
        ["deepseek", false],
        [undefined, false],
      ].map(([platform, expected]) => ({ simple, platform, expected })),
    ),
  )(
    "shows reapply only for known direct Responses platforms (simple=$simple, platform=$platform)",
    async ({ simple, platform, expected }) => {
      mocks.dispatch.mockImplementation((command) =>
        Promise.resolve(
          command === "tool-cards"
            ? {
                cards: [
                  {
                    ...service,
                    configured: true,
                    platform,
                    legacyEndpoint: true,
                  },
                ],
                groups: [],
                currentProviderId: service.provider.id,
              }
            : {},
        ),
      );
      wrap(<ToolPage app="codex" simple={Boolean(simple)} />);
      await screen.findByText("toolPage.status.inUse");
      const button = screen.queryByRole("button", {
        name: "toolPage.aria.reEnable",
      });
      expect(screen.queryByText(/切换至标准 HTTP/)).toBeNull();
      if (expected) expect(button).toBeEnabled();
      else expect(button).not.toBeInTheDocument();
    },
  );
  it("does not offer Codex reapply to other tools", async () => {
    mocks.dispatch.mockImplementation((command) =>
      Promise.resolve(
        command === "tool-cards"
          ? {
              cards: [{ ...service, configured: true }],
              groups: [],
              currentProviderId: service.provider.id,
            }
          : {},
      ),
    );
    wrap(<ToolPage app="claude" simple />);
    await screen.findByText("toolPage.status.inUse");
    expect(
      screen.queryByRole("button", { name: "toolPage.aria.reEnable" }),
    ).not.toBeInTheDocument();
  });
  it("allows manual key selection in the advanced view before reapplying an unconfigured active group", async () => {
    mocks.dispatch.mockImplementation((command) =>
      Promise.resolve(
        command === "tool-cards"
          ? {
              cards: [service],
              groups: [],
              currentProviderId: service.provider.id,
            }
          : command === "keys"
            ? keys
            : {},
      ),
    );
    wrap(<ToolPage app="codex" />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "toolPage.aria.reEnable",
      }),
    );
    expect(await screen.findByLabelText("API Key")).toBeInTheDocument();
    expect(
      mocks.dispatch.mock.calls.some(
        ([command]) => command === "start-tool-card",
      ),
    ).toBe(false);
  });
  it("does not issue duplicate reapply requests while one is pending", async () => {
    let finish!: (value: object) => void;
    mocks.dispatch.mockImplementation((command) =>
      command === "tool-cards"
        ? Promise.resolve({
            cards: [{ ...service, configured: true }],
            groups: [],
            currentProviderId: service.provider.id,
          })
        : new Promise((resolve) => {
            finish = resolve;
          }),
    );
    wrap(<ToolPage app="codex" simple />);
    const button = await screen.findByRole("button", {
      name: "toolPage.aria.reEnable",
    });
    fireEvent.click(button);
    fireEvent.click(button);
    expect(button).toBeDisabled();
    expect(
      mocks.dispatch.mock.calls.filter(
        ([command]) => command === "start-tool-card",
      ),
    ).toHaveLength(1);
    await act(async () => finish({}));
  });
  it("labels successful CLI setup as a fallback and does not claim native images were verified", async () => {
    mocks.imagegenGuide.mockResolvedValue({ status: "written" });
    mocks.dispatch.mockImplementation((command) =>
      Promise.resolve(
        command === "tool-cards"
          ? {
              cards: [{ ...service, configured: true }],
              groups: [],
              currentProviderId: service.provider.id,
            }
          : {},
      ),
    );
    wrap(<ToolPage app="codex" simple />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "toolPage.aria.reEnable",
      }),
    );
    const notice = await screen.findByText(/toolPage.notice.restartCodex/);
    expect(notice).not.toHaveTextContent("toolPage.summary.imagegenFallback");
    expect(
      screen.getByText("toolPage.summary.imagegenFallback").closest("details"),
    ).not.toHaveAttribute("open");
    expect(notice).not.toHaveTextContent("toolPage.notice.imagegenSaved");
  });
  it("reuses CC cards with official restore, no model picker, and no arbitrary supplier actions", async () => {
    wrap(<ToolPage app="codex" />);
    await screen.findByText("AIHubMix · OpenAI");
    expect(screen.queryByLabelText("模型")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("服务地址")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "添加供应商" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /更多|More/ }),
    ).not.toBeInTheDocument();
    fireEvent.click(
      screen.getAllByRole("button", { name: "toolPage.aria.start" })[0],
    );
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("start-tool-card", {
        app: "codex",
        providerId: "codex-official",
      }),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "toolPage.notice.switchedOfficialRestart",
    );
  });
  it("selects only an active Key belonging to the card's exact group, without selecting a model", async () => {
    wrap(<ToolPage app="codex" />);
    // 两个 aria.start 按钮:已配置卡直启([0])与未配置卡走编辑对话框([1])。
    // 空资源下 t 返回 key 且插值不生效,同名按钮以卡片顺序区分。
    const startButtons = await screen.findAllByRole("button", {
      name: "toolPage.aria.start",
    });
    fireEvent.click(startButtons[1]);
    expect(
      mocks.dispatch.mock.calls.some(([c]) => c === "start-tool-card"),
    ).toBe(false);
    await screen.findByRole("option", { name: /OpenAI Key/ });
    expect(
      screen.queryByRole("option", { name: /DeepSeek Key|已禁用 Key/ }),
    ).not.toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("API Key"), {
      target: { value: "10" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "toolPage.action.saveAndStart" }),
    );
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("start-tool-card", {
        app: "codex",
        providerId: service.provider.id,
        keyId: 10,
      }),
    );
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
  });
  it("retains the Key editor on failure and prevents duplicate starts while activation is pending", async () => {
    let fail!: (error: unknown) => void;
    mocks.dispatch.mockImplementation((command) =>
      command === "tool-cards"
        ? Promise.resolve({ cards: [service], currentProviderId: null })
        : command === "keys"
          ? Promise.resolve(keys)
          : new Promise((_, reject) => {
              fail = reject;
            }),
    );
    wrap(<ToolPage app="codex" />);
    fireEvent.click(
      await screen.findByRole("button", { name: "toolPage.aria.start" }),
    );
    await screen.findByRole("option", { name: /OpenAI Key/ });
    fireEvent.change(screen.getByLabelText("API Key"), {
      target: { value: "10" },
    });
    const submit = screen.getByRole("button", {
      name: "toolPage.action.saveAndStart",
    });
    fireEvent.click(submit);
    fireEvent.click(submit);
    expect(
      mocks.dispatch.mock.calls.filter(([c]) => c === "start-tool-card"),
    ).toHaveLength(1);
    await act(async () => {
      fail(new Error("Key 无效"));
    });
    expect(await screen.findByRole("alert")).toHaveTextContent("Key 无效");
    expect(screen.getByLabelText("API Key")).toHaveValue("10");
  });
  it("adds a new platform group card and supports later groups without arbitrary provider settings", async () => {
    let created = false;
    let glmAvailable = false;
    const glm = {
      ...service,
      provider: {
        ...service.provider,
        id: "aihubmix-pi-account-1-service-9",
        name: "AIHubMix · GLM",
      },
      groupId: 9,
    };
    mocks.dispatch.mockImplementation((command) => {
      if (command === "create-tool-card") {
        created = true;
        return Promise.resolve({ providerId: glm.provider.id });
      }
      return Promise.resolve(
        command === "tool-cards"
          ? {
              cards: created ? [service, glm] : [service],
              groups: [
                { id: 7, name: "OpenAI", platform: "openai" },
                { id: 8, name: "DeepSeek", platform: "deepseek" },
                ...(glmAvailable
                  ? [{ id: 9, name: "GLM", platform: "glm" }]
                  : []),
              ],
              currentProviderId: null,
            }
          : [],
      );
    });
    wrap(<ToolPage app="pi" />);
    await screen.findByText("AIHubMix · OpenAI");
    fireEvent.click(
      screen.getByRole("button", { name: "toolPage.action.addGroup" }),
    );
    expect(
      screen.getByRole("option", {
        name: "OpenAItoolPage.dialog.alreadyAdded",
      }),
    ).toBeDisabled();
    expect(
      screen.getByRole("option", { name: "DeepSeek" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("option", { name: "GLM" }),
    ).not.toBeInTheDocument();
    glmAvailable = true;
    fireEvent.click(
      screen.getByRole("button", { name: "toolPage.action.refreshGroups" }),
    );
    await screen.findByRole("option", { name: "GLM" });
    fireEvent.change(screen.getByLabelText("toolPage.dialog.platformGroup"), {
      target: { value: "9" },
    });
    fireEvent.click(
      screen.getByRole("button", { name: "toolPage.aria.confirmAdd" }),
    );
    await screen.findByText("AIHubMix · GLM");
    expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument();
    expect(mocks.dispatch).not.toHaveBeenCalledWith(
      "start-tool-card",
      expect.anything(),
    );
    expect(mocks.dispatch).toHaveBeenCalledWith("create-tool-card", {
      app: "pi",
      groupId: 9,
    });
    expect(screen.queryByLabelText("服务地址")).not.toBeInTheDocument();
  });
  it("reports a successful API creation without exposing local storage diagnostics", async () => {
    mocks.dispatch.mockImplementation((command) =>
      Promise.resolve(
        command === "groups"
          ? [{ id: 7, name: "平台服务", platform: "openai" }]
          : {
              warning:
                "无法写入 /Users/example/private/vault-v2.bin：Permission denied",
            },
      ),
    );
    wrap(<ApplyApiDialog onClose={() => {}} />);
    await screen.findByRole("option", { name: "平台服务 · openai" });
    fireEvent.click(screen.getByRole("button", { name: "确认申请" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "API 已申请成功。本机凭据保存失败，请返回 API 列表查看。",
    );
    expect(screen.queryByText(/vault-v2/)).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "确认申请" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "返回查看 Key" })).toBeEnabled();
  });

  it("creates a platform Key only after explicit confirmation and stops duplicate or uncertain retries", async () => {
    let fail!: (error: unknown) => void;
    const pending = new Promise((_, reject) => {
      fail = reject;
    });
    mocks.dispatch.mockImplementation((command) =>
      command === "groups"
        ? Promise.resolve([{ id: 7, name: "平台服务", platform: "openai" }])
        : pending,
    );
    wrap(<ApplyApiDialog onClose={() => {}} />);
    await screen.findByRole("option", { name: "平台服务 · openai" });
    expect(
      mocks.dispatch.mock.calls.some(([command]) => command === "create-key"),
    ).toBe(false);
    const submit = screen.getByRole("button", { name: "确认申请" });
    fireEvent.click(submit);
    fireEvent.click(submit);
    expect(
      mocks.dispatch.mock.calls.filter(([command]) => command === "create-key"),
    ).toHaveLength(1);
    await act(async () => {
      fail(new Error("timeout"));
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "请先刷新 Key 列表确认结果",
    );
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "确认申请" })).toBeDisabled(),
    );
  });
});

describe("DeepSeek Harness multi-provider experience", () => {
  const card = (id: number, live: boolean | null, configured = true) => ({
    ...service,
    provider: {
      ...service.provider,
      id: `aihubmix-dsh-account-1-service-${id}`,
      name: `分组 ${id}`,
    },
    groupId: id,
    selectedKeyId: configured ? 10 : null,
    configured,
    inLive: live,
  });
  it("offers DSH as a selectable tool rather than an unavailable placeholder", async () => {
    const select = vi.fn();
    wrap(<ToolPage app="codex" simple onSelectApp={select} />);
    fireEvent.click(
      await screen.findByRole("button", { name: "toolPage.aria.selectTool" }),
    );
    const choice = screen.getByRole("button", { name: /DeepSeek Harness/ });
    expect(choice).toBeEnabled();
    fireEvent.click(choice);
    expect(select).toHaveBeenCalledWith("dsh");
    expect(screen.queryByText("待接入")).not.toBeInTheDocument();
  });
  it("shows multiple enabled groups without implying a single current provider", async () => {
    mocks.dispatch.mockImplementation((command) =>
      Promise.resolve(
        command === "tool-cards"
          ? {
              cards: [card(7, true), card(8, true), card(9, null)],
              groups: [],
              currentProviderId: "aihubmix-dsh-account-1-service-7",
            }
          : [],
      ),
    );
    wrap(<ToolPage app="dsh" simple />);
    await screen.findByText("分组 7");
    expect(screen.getAllByText("toolPage.status.enabled")).toHaveLength(2);
    expect(screen.queryByText("toolPage.status.inUse")).not.toBeInTheDocument();
    expect(screen.getByText("toolPage.status.unknown")).toBeInTheDocument();
    expect(screen.getByText("toolPage.hint.dshMulti")).toBeInTheDocument();
    expect(
      screen.getAllByRole("button", { name: "toolPage.aria.enable" })[0],
    ).toBeEnabled();
  });
  it("enables a configured group and tells users to select a model in DSH", async () => {
    mocks.dispatch.mockImplementation((command) =>
      Promise.resolve(
        command === "tool-cards"
          ? {
              cards: [card(7, false)],
              groups: [],
              currentProviderId: null,
            }
          : {},
      ),
    );
    wrap(<ToolPage app="dsh" simple />);
    fireEvent.click(
      await screen
        .findAllByRole("button", { name: "toolPage.aria.enable" })
        .then((b) => b[0]),
    );
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("start-tool-card", {
        app: "dsh",
        providerId: "aihubmix-dsh-account-1-service-7",
      }),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "toolPage.notice.dshMultiEnabled",
    );
  });
});
