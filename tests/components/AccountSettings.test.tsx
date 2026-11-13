import { act, fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AccountSettings } from "@/components/settings/AccountSettings";

const account = vi.hoisted(() => ({
  state: { account: { loggedIn: true, email: "account@example.test" } },
  logout: vi.fn(),
}));
vi.mock("@/contexts/AccountContext", () => ({
  useOptionalAccount: () => account,
}));

describe("settings account logout", () => {
  beforeEach(() => vi.clearAllMocks());

  it("shows the account and prevents duplicate logout while pending", async () => {
    let complete!: () => void;
    account.logout.mockImplementation(
      () =>
        new Promise<void>((resolve) => {
          complete = resolve;
        }),
    );
    render(<AccountSettings />);
    expect(screen.getByText(/account@example\.test/)).toBeInTheDocument();
    const button = screen.getByRole("button", { name: "退出登录" });
    fireEvent.click(button);
    fireEvent.click(button);
    expect(account.logout).toHaveBeenCalledTimes(1);
    expect(button).toBeDisabled();
    await act(async () => complete());
  });

  it("keeps logout available after a failure and displays the error", async () => {
    account.logout.mockRejectedValue(new Error("network timeout"));
    render(<AccountSettings />);
    fireEvent.click(screen.getByRole("button", { name: "退出登录" }));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "退出失败，请重试。",
    );
    expect(screen.getByRole("button", { name: "退出登录" })).toBeEnabled();
  });
});
