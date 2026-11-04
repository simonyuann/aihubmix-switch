import {
  fireEvent,
  render,
  screen,
  waitFor,
  cleanup,
} from "@testing-library/react";
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
    const historical = { ...pending, id: 2, amount: 20, status: "FAILED" };
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
    expect(
      screen.queryByRole("button", { name: "创建充值订单" }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: /CNY 20/ }),
    ).not.toBeInTheDocument();
    const history = screen.getByRole("button", { name: /^充值记录/ });
    expect(history).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(history);
    expect(screen.queryByText(/#\d+/)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /CNY 20/ }));
    await waitFor(() =>
      expect(screen.queryByAltText("订单付款二维码")).not.toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: /CNY 50/ }));
    expect(await screen.findByAltText("订单付款二维码")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "打开支付页面" }),
    ).toBeInTheDocument();
  });
  it("returns to recharge after a cancelled payment without creating another order", async () => {
    const pending = {
      id: 3,
      amount: 50,
      payAmount: 50,
      currency: "CNY",
      status: "PENDING",
      expiresAt: new Date(Date.now() + 600000).toISOString(),
      qrCode: "checkout-c",
    };
    const cancelled = { ...pending, status: "CANCELLED" };
    mocks.dispatch.mockImplementation((command: string) => {
      if (command === "payment-overview") return Promise.resolve(overview);
      if (command === "payment-cancel") return Promise.resolve(cancelled);
      return Promise.resolve(pending);
    });
    render(<RechargeDialog open onClose={() => {}} onPaid={() => {}} />);
    const create = await screen.findByRole("button", { name: "创建充值订单" });
    await waitFor(() => expect(create).toBeEnabled());
    fireEvent.click(create);
    await screen.findByAltText("订单付款二维码");
    fireEvent.click(screen.getByRole("button", { name: "取消订单" }));
    fireEvent.click(await screen.findByRole("button", { name: "返回充值" }));
    expect(screen.getByRole("button", { name: "创建充值订单" })).toBeEnabled();
    expect(screen.queryByAltText("订单付款二维码")).not.toBeInTheDocument();
    expect(
      mocks.dispatch.mock.calls.filter(
        ([command]) => command === "payment-create",
      ),
    ).toHaveLength(1);
  });
  it("uses service wallet denomination separately from checkout currency", async () => {
    mocks.dispatch.mockResolvedValue({
      ...overview,
      currency: "USD",
      balance: 12.34,
    });
    render(<RechargeDialog open onClose={() => {}} onPaid={() => {}} />);
    expect(await screen.findByText(/钱包余额：USD 12.34/)).toBeInTheDocument();
  });
});
