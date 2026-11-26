import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ account: null as any }));
vi.mock("@/contexts/AccountContext", () => ({
  useOptionalAccount: () => mocks.account,
}));

import {
  BalanceDisplay,
  SidebarWalletCard,
} from "@/components/aihubmix/BalanceDisplay";
import {
  AppPageHeader,
  HeaderBalanceContext,
} from "@/components/shell/AppPageHeader";

beforeEach(() => {
  mocks.account = {
    state: { account: { loggedIn: true } },
    wallet: { currency: "CNY", balance: 88.8 },
    walletError: "",
    refreshWallet: vi.fn().mockResolvedValue(undefined),
  };
});

describe("wallet balance", () => {
  it("keeps raw balance errors out of the header tooltip", () => {
    mocks.account.walletError = "private server details";
    render(<BalanceDisplay />);
    expect(screen.getByRole("button", { name: "刷新余额" })).toHaveAttribute(
      "title",
      "余额刷新失败，点击重试",
    );
    expect(document.body.innerHTML).not.toContain("private server details");
  });

  it("shows the balance, refreshes once while pending, and opens recharge", async () => {
    let finish!: () => void;
    mocks.account.refreshWallet.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          finish = resolve;
        }),
    );
    const recharge = vi.fn();
    render(<SidebarWalletCard collapsed={false} onRecharge={recharge} />);

    expect(screen.getByText("¥88.80")).toBeInTheDocument();
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
    const view = render(
      <SidebarWalletCard collapsed={false} onRecharge={() => {}} />,
    );
    expect(screen.getByText("—")).toBeInTheDocument();
    mocks.account.walletError = "private server details";
    view.rerender(
      <SidebarWalletCard collapsed={false} onRecharge={() => {}} />,
    );
    expect(screen.getByText("余额暂不可用")).toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(
      "余额刷新失败，请重试",
    );
    expect(
      screen.queryByText(/private server details/),
    ).not.toBeInTheDocument();
  });

  it("keeps the amount accessible and opens recharge from the collapsed rail", () => {
    const recharge = vi.fn();
    render(<SidebarWalletCard collapsed onRecharge={recharge} />);
    const wallet = screen.getByRole("button", {
      name: "账户余额 ¥88.80，充值",
    });
    fireEvent.click(wallet);
    expect(recharge).toHaveBeenCalledOnce();
  });

  it("only shows the header balance when the workspace enables it", () => {
    const view = render(
      <HeaderBalanceContext.Provider value={false}>
        <AppPageHeader title="首页" />
      </HeaderBalanceContext.Provider>,
    );
    expect(
      screen.queryByRole("button", { name: "刷新余额" }),
    ).not.toBeInTheDocument();
    view.rerender(
      <HeaderBalanceContext.Provider value>
        <AppPageHeader title="其他页面" />
      </HeaderBalanceContext.Provider>,
    );
    expect(
      screen.getByRole("button", { name: "刷新余额" }),
    ).toBeInTheDocument();
  });

  it("does not show an account balance while logged out", () => {
    mocks.account.state.account.loggedIn = false;
    render(<SidebarWalletCard collapsed={false} onRecharge={() => {}} />);
    expect(
      screen.queryByRole("region", { name: "账户余额" }),
    ).not.toBeInTheDocument();
  });
});
