import { StrictMode } from "react";
import {
  fireEvent,
  render,
  screen,
  waitFor,
  cleanup,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({
  dispatch: vi.fn(),
  add: vi.fn(),
  draft: vi.fn(),
}));
vi.mock("@/lib/api/aihubmix", () => ({
  dispatch: mocks.dispatch,
  aihubmixApi: { draft: mocks.draft },
}));
vi.mock("@/lib/api/providers", () => ({ providersApi: { add: mocks.add } }));
vi.mock("@/components/shell/AppGlyph", () => ({
  APP_DISPLAY_NAME: { codex: "Codex", claude: "Claude Code" },
}));
vi.mock("qrcode", () => ({
  default: {
    toDataURL: vi.fn().mockResolvedValue("data:image/png;base64,abc"),
  },
}));
import {
  RechargeDialog,
  validAmount,
} from "@/components/aihubmix/RechargeDialog";
import { ProviderDialog } from "@/components/aihubmix/ProviderDialog";
import { AccountPage } from "@/components/aihubmix/AccountPage";
const overview = {
  enabled: true,
  email: "a@example.test",
  balance: 10,
  currency: "USD",
  minAmount: 1,
  maxAmount: 100,
  methods: [
    { id: "alipay", name: "支付宝", minAmount: 1, maxAmount: 100, feeRate: 0 },
  ],
  orders: [],
};
const item = {
  id: 1,
  name: "Test",
  keyMasked: "sk-***",
  groupId: 1,
  groupName: "Default",
  status: "active",
  quotaUsed: 0,
  quota: 100,
  createdAt: "",
  local: false,
};
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
});
afterEach(cleanup);
describe("AIHubMix account integration", () => {
  it("requires exact decimal amounts", () => {
    expect(validAmount("1.01", 1, 100)).toBe(true);
    for (const value of ["1.001", "1e2", "0.99", "101", "-2"])
      expect(validAmount(value, 1, 100)).toBe(false);
  });
  it("does not create orders on open and blocks uncertain retries after failed refresh", async () => {
    mocks.dispatch.mockImplementation((command: string) =>
      command === "payment-overview"
        ? Promise.resolve(overview)
        : Promise.reject(new Error("network timeout")),
    );
    const view = render(
      <RechargeDialog open onClose={() => {}} onPaid={() => {}} />,
    );
    const create = await screen.findByRole("button", { name: "创建充值订单" });
    await waitFor(() => expect(create).toBeEnabled());
    expect(mocks.dispatch).not.toHaveBeenCalledWith(
      "payment-create",
      expect.anything(),
    );
    fireEvent.click(create);
    await screen.findByText(/提交结果不确定/);
    expect(create).toBeDisabled();
    mocks.dispatch.mockRejectedValue(new Error("refresh failed"));
    fireEvent.click(screen.getByRole("button", { name: "刷新余额与充值记录" }));
    await screen.findByText(/refresh failed/);
    expect(create).toBeDisabled();
    view.rerender(
      <RechargeDialog open={false} onClose={() => {}} onPaid={() => {}} />,
    );
    view.rerender(<RechargeDialog open onClose={() => {}} onPaid={() => {}} />);
    await screen.findByText(/提交结果不确定/);
  });
  it("saves a draft through CC without writing live configuration", async () => {
    mocks.dispatch.mockResolvedValue([{ id: "gpt-5", name: "GPT 5" }]);
    const draft = { id: "p1", name: "AIHubMix", settingsConfig: {} };
    mocks.draft.mockResolvedValue(draft);
    mocks.add.mockResolvedValue(true);
    const close = vi.fn();
    wrap(<ProviderDialog item={item} onClose={close} />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "保存供应商" })).toBeEnabled(),
    );
    expect(mocks.add).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "保存供应商" }));
    await waitFor(() =>
      expect(mocks.add).toHaveBeenCalledWith(draft, "codex", false),
    );
    expect(close).toHaveBeenCalled();
  });
  it("surfaces CC save errors and retains the editable dialog", async () => {
    mocks.dispatch.mockResolvedValue([{ id: "gpt-5", name: "GPT 5" }]);
    mocks.draft.mockResolvedValue({ id: "p1" });
    mocks.add.mockRejectedValue(new Error("database unavailable"));
    const close = vi.fn();
    wrap(<ProviderDialog item={item} onClose={close} />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "保存供应商" })).toBeEnabled(),
    );
    fireEvent.click(screen.getByRole("button", { name: "保存供应商" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "database unavailable",
    );
    expect(close).not.toHaveBeenCalled();
  });
  it("does not save a stale account draft after unmount", async () => {
    mocks.dispatch.mockResolvedValue([{ id: "gpt-5", name: "GPT 5" }]);
    let resolve!: (value: unknown) => void;
    mocks.draft.mockImplementation(
      () =>
        new Promise((r) => {
          resolve = r;
        }),
    );
    const view = wrap(<ProviderDialog item={item} onClose={() => {}} />);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "保存供应商" })).toBeEnabled(),
    );
    fireEvent.click(screen.getByRole("button", { name: "保存供应商" }));
    view.unmount();
    resolve({ id: "stale" });
    await Promise.resolve();
    expect(mocks.add).not.toHaveBeenCalled();
  });
  it("restores a pending checkout after viewing another order", async () => {
    const pending = {
      id: 1,
      amount: 50,
      payAmount: 50,
      bonusAmount: 0,
      currency: "CNY",
      method: "alipay",
      status: "PENDING",
      createdAt: "",
      expiresAt: new Date(Date.now() + 600000).toISOString(),
      tradeNo: "a",
    };
    const historical = { ...pending, id: 2, status: "FAILED" };
    mocks.dispatch.mockImplementation(
      (command: string, payload?: { id?: number }) => {
        if (command === "payment-overview")
          return Promise.resolve({ ...overview, orders: [historical] });
        if (command === "payment-create")
          return Promise.resolve({
            ...pending,
            qrCode: "checkout-a",
            payUrl: "https://pay.example.test/a",
          });
        return Promise.resolve(payload?.id === 2 ? historical : pending);
      },
    );
    render(<RechargeDialog open onClose={() => {}} onPaid={() => {}} />);
    const create = await screen.findByRole("button", { name: "创建充值订单" });
    await waitFor(() => expect(create).toBeEnabled());
    fireEvent.click(create);
    await screen.findByAltText("订单付款二维码");
    fireEvent.click(screen.getByRole("button", { name: /#2 ·/ }));
    await waitFor(() =>
      expect(screen.queryByAltText("订单付款二维码")).not.toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /#1 ·/ }));
    expect(await screen.findByAltText("订单付款二维码")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "打开支付页面" }),
    ).toBeInTheDocument();
  });
  it("uses service wallet denomination separately from checkout currency", async () => {
    mocks.dispatch.mockResolvedValue({
      ...overview,
      currency: "USD",
      balance: 12.34,
    });
    render(<RechargeDialog open onClose={() => {}} onPaid={() => {}} />);
    expect(await screen.findByText(/钱包余额：USD 12.34/)).toBeInTheDocument();
    cleanup();
    mocks.dispatch.mockImplementation((command: string) =>
      Promise.resolve(
        command === "state"
          ? {
              relayBaseUrl: "https://example.test",
              account: { loggedIn: true, email: "a@test" },
              localKeys: [],
              legacyData: false,
              storageError: null,
            }
          : command === "payment-balance"
            ? { email: "a@test", balance: 12.34, currency: "USD" }
            : [],
      ),
    );
    wrap(<AccountPage />);
    expect(await screen.findByText(/钱包余额：USD 12.34/)).toBeInTheDocument();
  });
  it("reviews imported mappings and prefills an explicitly saved provider", async () => {
    mocks.dispatch.mockImplementation((command: string) =>
      Promise.resolve(
        command === "state"
          ? {
              relayBaseUrl: "https://example.test",
              account: { loggedIn: false },
              localKeys: [item],
              legacyData: false,
              legacyMappings: [
                {
                  app: "claude",
                  appLabel: "Claude Code",
                  model: "legacy-model",
                },
              ],
              storageError: null,
            }
          : command === "models"
            ? [{ id: "other-model", name: "Other" }]
            : [],
      ),
    );
    mocks.draft.mockResolvedValue({ id: "legacy-provider" });
    mocks.add.mockResolvedValue(true);
    wrap(<AccountPage />);
    await screen.findByText("已导入的工具与模型映射");
    fireEvent.click(
      screen.getByRole("button", { name: "预填 Claude Code 供应商" }),
    );
    expect(screen.getByRole("status")).toHaveTextContent("legacy-model");
    expect(mocks.add).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "添加供应商" }));
    await waitFor(() =>
      expect(screen.getByLabelText("模型")).toHaveValue("legacy-model"),
    );
    expect(screen.getByLabelText("目标应用")).toHaveValue("claude");
    fireEvent.click(screen.getByRole("button", { name: "保存供应商" }));
    await waitFor(() =>
      expect(mocks.draft).toHaveBeenCalledWith({
        keyId: 1,
        app: "claude",
        model: "legacy-model",
      }),
    );
    expect(mocks.add).toHaveBeenCalledWith(
      { id: "legacy-provider" },
      "claude",
      false,
    );
  });
  it("loads the account inside CC StrictMode", async () => {
    mocks.dispatch.mockResolvedValue({
      relayBaseUrl: "https://example.test",
      account: { loggedIn: false },
      localKeys: [],
      legacyData: false,
      storageError: null,
    });
    wrap(
      <StrictMode>
        <AccountPage />
      </StrictMode>,
    );
    await waitFor(() =>
      expect(screen.getByLabelText("服务地址")).toHaveValue(
        "https://example.test",
      ),
    );
    expect(screen.getByRole("button", { name: "登录" })).toBeEnabled();
  });
  it("discards stale account details after navigating away", async () => {
    let resolve!: (value: unknown) => void;
    mocks.dispatch.mockImplementation((command: string) => {
      if (command === "state")
        return Promise.resolve({
          relayBaseUrl: "https://example.test",
          account: { loggedIn: true, email: "a@test", userId: 1 },
          localKeys: [],
          legacyData: false,
          storageError: null,
        });
      if (command === "payment-balance")
        return new Promise((r) => {
          resolve = r;
        });
      return Promise.resolve([]);
    });
    const view = wrap(<AccountPage />);
    await screen.findByText("a@test");
    view.unmount();
    resolve({ email: "a@test", balance: 999, currency: "USD" });
    await Promise.resolve();
    expect(screen.queryByText(/999/)).not.toBeInTheDocument();
  });
});
