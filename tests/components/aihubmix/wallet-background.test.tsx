import { act, cleanup, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({
  event: undefined as undefined | ((value: unknown) => void),
  dispatch: vi.fn(),
}));
vi.mock("@/hooks/useTauriEvent", () => ({
  useTauriEvent: (_name: string, handler: (value: unknown) => void) => {
    mocks.event = handler;
  },
}));
vi.mock("@/lib/api/aihubmix", () => ({ dispatch: mocks.dispatch }));
import { AccountProvider, useAccount } from "@/contexts/AccountContext";

function Wallet() {
  const { wallet } = useAccount();
  return <span>{wallet?.balance ?? "no balance"}</span>;
}

afterEach(cleanup);

describe("background wallet updates", () => {
  it("ignores other accounts and updates the current account", async () => {
    mocks.dispatch.mockImplementation((command: string) =>
      Promise.resolve(
        command === "session"
          ? { account: { loggedIn: true, userId: 1 } }
          : { email: "test@example.test", balance: 10, currency: "CNY" },
      ),
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    render(
      <QueryClientProvider client={client}>
        <AccountProvider>
          <Wallet />
        </AccountProvider>
      </QueryClientProvider>,
    );
    expect(await screen.findByText("10")).toBeInTheDocument();
    act(() =>
      mocks.event?.({ userId: 2, wallet: { balance: 99, currency: "CNY" } }),
    );
    expect(screen.getByText("10")).toBeInTheDocument();
    act(() =>
      mocks.event?.({ userId: 1, wallet: { balance: 12, currency: "CNY" } }),
    );
    expect(screen.getByText("12")).toBeInTheDocument();
  });
});
