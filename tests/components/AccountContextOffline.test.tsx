import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ dispatch: vi.fn() }));
vi.mock("@/lib/api/aihubmix", () => ({ dispatch: mocks.dispatch }));
vi.mock("@/hooks/useTauriEvent", () => ({ useTauriEvent: vi.fn() }));
import { AccountProvider, useAccount } from "@/contexts/AccountContext";

const verified = {
  relayBaseUrl: "https://example.test",
  account: { loggedIn: true, email: "user@example.test", userId: 1 },
  localKeys: [],
  legacyData: false,
  storageError: null,
};
function View() {
  const { state, checking, error, refresh, logout, authenticate } =
    useAccount();
  return (
    <>
      <span data-testid="account">
        {checking ? "checking" : (state?.account.email ?? "logged-out")}
      </span>
      <span role="status">{error}</span>
      <button onClick={() => void refresh()}>刷新会话</button>
      <button onClick={() => void logout()}>退出</button>
      <button
        onClick={() =>
          void authenticate("login", {
            email: "user@example.test",
            password: "test",
          })
        }
      >
        登录
      </button>
    </>
  );
}
function mount() {
  return render(
    <QueryClientProvider client={new QueryClient()}>
      <AccountProvider>
        <View />
      </AccountProvider>
    </QueryClientProvider>,
  );
}
beforeEach(() => {
  mocks.dispatch.mockReset();
  mocks.dispatch.mockImplementation((command) =>
    command === "session" ? Promise.resolve(verified) : Promise.resolve(null),
  );
});
describe("account session connectivity", () => {
  it("keeps a verified account during transient errors and retries on reconnection", async () => {
    mount();
    await screen.findByText("user@example.test");
    mocks.dispatch.mockImplementation((command) =>
      command === "session"
        ? Promise.reject("connection timeout")
        : Promise.resolve(null),
    );
    fireEvent.click(screen.getByText("刷新会话"));
    await screen.findByText("accountContext.error.connectionUnavailable");
    expect(screen.getByTestId("account")).toHaveTextContent(
      "user@example.test",
    );
    mocks.dispatch.mockImplementation((command) =>
      command === "session" ? Promise.resolve(verified) : Promise.resolve(null),
    );
    fireEvent(window, new Event("online"));
    await waitFor(() =>
      expect(screen.getByRole("status")).toBeEmptyDOMElement(),
    );
  });
  it("does not trust unverified cold-start sessions when offline", async () => {
    mocks.dispatch.mockRejectedValue("connection timeout");
    mount();
    await screen.findByText("accountContext.error.connectionUnavailable");
    expect(screen.getByTestId("account")).toHaveTextContent("logged-out");
  });
  it("restores a backend-validated local session and revalidates on reconnect", async () => {
    mocks.dispatch.mockImplementation((command) =>
      command === "session"
        ? Promise.resolve({ ...verified, sessionOffline: true })
        : Promise.resolve(null),
    );
    mount();
    await screen.findByText("user@example.test");
    expect(screen.getByRole("status")).toHaveTextContent(
      "accountContext.status.offlineUsingSaved",
    );
    mocks.dispatch.mockImplementation((command) =>
      command === "session" ? Promise.resolve(verified) : Promise.resolve(null),
    );
    fireEvent(window, new Event("online"));
    await waitFor(() =>
      expect(screen.getByRole("status")).toBeEmptyDOMElement(),
    );
    expect(screen.getByTestId("account")).toHaveTextContent(
      "user@example.test",
    );
  });
  it("enters the account immediately after successful login when session verification was unavailable", async () => {
    mocks.dispatch.mockImplementation((command) =>
      command === "session"
        ? Promise.reject("session unavailable")
        : command === "login"
          ? Promise.resolve(verified)
          : Promise.resolve(null),
    );
    mount();
    await screen.findByText("accountContext.error.connectionUnavailable");
    fireEvent.click(screen.getByText("登录"));
    await screen.findByText("user@example.test");
    expect(
      mocks.dispatch.mock.calls.filter(([command]) => command === "session"),
    ).toHaveLength(1);
  });
  it("clears the verified session on authentication failure", async () => {
    mount();
    await screen.findByText("user@example.test");
    mocks.dispatch.mockImplementation((command) =>
      command === "session"
        ? Promise.reject("AUTH_REQUIRED: expired")
        : Promise.resolve(null),
    );
    fireEvent.click(screen.getByText("刷新会话"));
    await waitFor(() =>
      expect(screen.getByTestId("account")).toHaveTextContent("logged-out"),
    );
  });
  it("clears the account and shows a server revocation warning after logout", async () => {
    mocks.dispatch.mockImplementation((command) =>
      command === "session"
        ? Promise.resolve(verified)
        : command === "logout"
          ? Promise.resolve({ revocationWarning: "无法确认服务端会话已撤销" })
          : Promise.resolve(null),
    );
    mount();
    await screen.findByText("user@example.test");
    fireEvent.click(screen.getByText("退出"));
    await screen.findByText(/无法确认服务端会话已撤销/);
    expect(screen.getByRole("status").textContent).toBe(
      "无法确认服务端会话已撤销",
    );
    expect(screen.getByTestId("account")).toHaveTextContent("logged-out");
  });
});
