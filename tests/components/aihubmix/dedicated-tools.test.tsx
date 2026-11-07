import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ dispatch: vi.fn() }));
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
        ? Promise.resolve({ status: "unavailable" })
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
beforeEach(() => {
  vi.clearAllMocks();
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
  it("offers a single tool selector on the simple home", async () => {
    const onSelectApp = vi.fn();
    wrap(<ToolPage app="codex" simple onSelectApp={onSelectApp} />);
    fireEvent.click(
      await screen.findByRole("button", { name: /选择 AI 工具，当前为 Codex/ }),
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
    expect(await screen.findByText("还没有添加模型分组")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "申请 API" }),
    ).toBeInTheDocument();
    expect(screen.queryByText("OpenAI Official")).not.toBeInTheDocument();
  });
  it("explains how to activate a simple-home service and only opens key selection when needed", async () => {
    wrap(<ToolPage app="codex" simple />);
    fireEvent.click(
      await screen.findByRole("button", {
        name: "选择 API 并使用 AIHubMix · OpenAI",
      }),
    );
    expect(await screen.findByLabelText("API Key")).toBeInTheDocument();
    expect(mocks.dispatch).not.toHaveBeenCalledWith(
      "start-tool-card",
      expect.anything(),
    );
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
    expect(screen.queryByText("使用中")).not.toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", { name: "切换到 AIHubMix · OpenAI" }),
    );
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("start-tool-card", {
        app: "codex",
        providerId: service.provider.id,
      }),
    );
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
      screen.getByRole("button", { name: "启动 OpenAI Official" }),
    );
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("start-tool-card", {
        app: "codex",
        providerId: "codex-official",
      }),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "已切换到 OpenAI Official",
    );
  });
  it("selects only an active Key belonging to the card's exact group, without selecting a model", async () => {
    wrap(<ToolPage app="codex" />);
    fireEvent.click(
      await screen.findByRole("button", { name: "启动 AIHubMix · OpenAI" }),
    );
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
    fireEvent.click(screen.getByRole("button", { name: "保存并启动" }));
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
      await screen.findByRole("button", { name: "启动 AIHubMix · OpenAI" }),
    );
    await screen.findByRole("option", { name: /OpenAI Key/ });
    fireEvent.change(screen.getByLabelText("API Key"), {
      target: { value: "10" },
    });
    const submit = screen.getByRole("button", { name: "保存并启动" });
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
    fireEvent.click(screen.getByRole("button", { name: "添加分组" }));
    expect(
      screen.getByRole("option", { name: "OpenAI（已添加）" }),
    ).toBeDisabled();
    expect(
      screen.getByRole("option", { name: "DeepSeek" }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("option", { name: "GLM" }),
    ).not.toBeInTheDocument();
    glmAvailable = true;
    fireEvent.click(screen.getByRole("button", { name: "刷新分组" }));
    await screen.findByRole("option", { name: "GLM" });
    fireEvent.change(screen.getByLabelText("平台分组"), {
      target: { value: "9" },
    });
    fireEvent.click(screen.getByRole("button", { name: "确认添加分组" }));
    await screen.findByText("AIHubMix · GLM");
    expect(mocks.dispatch).toHaveBeenCalledWith("create-tool-card", {
      app: "pi",
      groupId: 9,
    });
    expect(screen.queryByLabelText("服务地址")).not.toBeInTheDocument();
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
      await screen.findByRole("button", { name: /选择 AI 工具/ }),
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
    expect(screen.getAllByText("已启用")).toHaveLength(2);
    expect(screen.queryByText("使用中")).not.toBeInTheDocument();
    expect(screen.getByText("状态未知")).toBeInTheDocument();
    expect(screen.getByText(/支持同时启用多个分组/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "启用 分组 9" })).toBeEnabled();
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
    fireEvent.click(await screen.findByRole("button", { name: "启用 分组 7" }));
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("start-tool-card", {
        app: "dsh",
        providerId: "aihubmix-dsh-account-1-service-7",
      }),
    );
    expect(await screen.findByRole("status")).toHaveTextContent(
      "请在 DeepSeek Harness 中选择模型",
    );
  });
});
