import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ account: null as any }));
vi.mock("@/contexts/AccountContext", () => ({
  useOptionalAccount: () => mocks.account,
}));

import { HomeWalletBalance } from "@/components/aihubmix/BalanceDisplay";
import { AppPageHeader, HeaderBalanceContext } from "@/components/shell/AppPageHeader";

beforeEach(() => {
  mocks.account = {
    state: { account: { loggedIn: true } },
    wallet: { currency: "CNY", balance: 88.8 },
    walletError: "",
    refreshWallet: vi.fn().mockResolvedValue(undefined),
  };
});

describe("simple home balance", () => {
  it("shows the balance, refreshes once while pending, and opens recharge", async () => {
    let finish!: () => void;
    mocks.account.refreshWallet.mockImplementation(
      () => new Promise<void>((resolve) => { finish = resolve; }),
    );
    const recharge = vi.fn();
    render(<HomeWalletBalance onRecharge={recharge} />);

    expect(screen.getByText("CNY 88.80")).toBeInTheDocument();
    const refresh = screen.getByRole("button", { name: "刷新余额" });
    fireEvent.click(refresh);
    expect(refresh).toBeDisabled();
    expect(screen.getByText("刷新中")).toBeInTheDocument();
    fireEvent.click(refresh);
    expect(mocks.account.refreshWallet).toHaveBeenCalledTimes(1);
    await act(async () => finish());
    expect(refresh).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "充值" }));
    expect(recharge).toHaveBeenCalledTimes(1);
  });

  it("handles unavailable or empty balances without exposing error details", () => {
    mocks.account.wallet = null;
    const view = render(<HomeWalletBalance onRecharge={() => {}} />);
    expect(screen.getByText("—")).toBeInTheDocument();
    mocks.account.walletError = "private server details";
    view.rerender(<HomeWalletBalance onRecharge={() => {}} />);
    expect(screen.getByText("余额暂不可用")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent("余额刷新失败，请重试");
    expect(screen.queryByText(/private server details/)).not.toBeInTheDocument();
  });

  it("only hides the header balance when explicitly requested", () => {
    const view = render(
      <HeaderBalanceContext.Provider value>
        <AppPageHeader title="首页" hideBalance />
      </HeaderBalanceContext.Provider>,
    );
    expect(screen.queryByRole("button", { name: "刷新余额" })).not.toBeInTheDocument();
    view.rerender(
      <HeaderBalanceContext.Provider value>
        <AppPageHeader title="其他页面" />
      </HeaderBalanceContext.Provider>,
    );
    expect(screen.getByRole("button", { name: "刷新余额" })).toBeInTheDocument();
  });

  it("does not show an account balance while logged out", () => {
    mocks.account.state.account.loggedIn = false;
    render(<HomeWalletBalance onRecharge={() => {}} />);
    expect(screen.queryByRole("region", { name: "账户余额" })).not.toBeInTheDocument();
  });
});
