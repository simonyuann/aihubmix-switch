import {
  QueryClient,
  QueryClientProvider,
  useQuery,
} from "@tanstack/react-query";
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { AccountProvider, useAccount } from "@/contexts/AccountContext";
import { AccountBoundary } from "@/App";

const mocks = vi.hoisted(() => ({ dispatch: vi.fn() }));
vi.mock("@/lib/api/aihubmix", () => ({ dispatch: mocks.dispatch }));
vi.mock("@/hooks/useTauriEvent", () => ({ useTauriEvent: vi.fn() }));
vi.mock("@/components/aihubmix/LoginPage", () => ({
  LoginPage: () => <span>登录页</span>,
}));
vi.mock("@/CcWorkspace", () => ({ default: () => null }));

const account = (id: number) => ({
  relayBaseUrl: "https://example.test",
  account: { loggedIn: true, email: `user${id}@example.test`, userId: id },
  localKeys: [],
  legacyData: false,
  storageError: null,
});
let current = 1;
function Controls() {
  const { refresh, logout, authenticate } = useAccount();
  return (
    <>
      <button onClick={() => void refresh()}>刷新</button>
      <button onClick={() => void logout()}>退出</button>
      <button onClick={() => void authenticate("login", {})}>登录</button>
    </>
  );
}
function Content() {
  const { state } = useAccount();
  const { data } = useQuery({
    queryKey: ["tool-cards", "codex"],
    queryFn: () => mocks.dispatch("tool-cards"),
    staleTime: Infinity,
  });
  return (
    <>
      <span>{state?.account.email}</span>
      <span>{data as string}</span>
      <input aria-label="临时编辑" defaultValue="" />
    </>
  );
}
function mount(client: QueryClient) {
  render(
    <QueryClientProvider client={client}>
      <AccountProvider>
        <Controls />
        <AccountBoundary>
          <Content />
        </AccountBoundary>
      </AccountProvider>
    </QueryClientProvider>,
  );
}
beforeEach(() => {
  current = 1;
  mocks.dispatch.mockReset();
  mocks.dispatch.mockImplementation((command) => {
    if (command === "session") return Promise.resolve(account(current));
    if (command === "tool-cards") return Promise.resolve(`cards-${current}`);
    return Promise.resolve(null);
  });
});
describe("account cache isolation", () => {
  it("clears cached platform data and local page state if session refresh changes account", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    mount(client);
    await screen.findByText("cards-1");
    client.setQueryData(["platform-keys"], ["old-secret"]);
    fireEvent.change(screen.getByLabelText("临时编辑"), {
      target: { value: "old-account" },
    });
    current = 2;
    fireEvent.click(screen.getByText("刷新"));
    await screen.findByText("user2@example.test");
    await screen.findByText("cards-2");
    expect(screen.queryByText("cards-1")).not.toBeInTheDocument();
    expect(client.getQueryData(["platform-keys"])).toBeUndefined();
    expect(screen.getByLabelText("临时编辑")).toHaveValue("");
  });
  it("hides the previous account immediately while logout is pending", async () => {
    let finish!: () => void;
    mocks.dispatch.mockImplementation((command) =>
      command === "session"
        ? Promise.resolve(account(1))
        : command === "tool-cards"
          ? Promise.resolve("cards-1")
          : command === "logout"
            ? new Promise<void>((resolve) => {
                finish = resolve;
              })
            : Promise.resolve(null),
    );
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    mount(client);
    await screen.findByText("cards-1");
    fireEvent.click(screen.getByText("退出"));
    expect(screen.queryByText("cards-1")).not.toBeInTheDocument();
    expect(screen.getByText("app.status.verifying")).toBeInTheDocument();
    const sessionCalls = mocks.dispatch.mock.calls.filter(
      ([command]) => command === "session",
    ).length;
    fireEvent(window, new Event("online"));
    expect(
      mocks.dispatch.mock.calls.filter(([command]) => command === "session"),
    ).toHaveLength(sessionCalls);
    finish();
    await screen.findByText("登录页");
    expect(client.getQueryData(["tool-cards", "codex"])).toBeUndefined();
  });
  it("does not restore an old pending response after the account changes", async () => {
    let resolveOld!: (value: string) => void;
    mocks.dispatch.mockImplementation((command) => {
      if (command === "session") return Promise.resolve(account(current));
      if (command === "tool-cards")
        return current === 1
          ? new Promise<string>((resolve) => {
              resolveOld = resolve;
            })
          : Promise.resolve("cards-2");
      return Promise.resolve(null);
    });
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    mount(client);
    await screen.findByText("user1@example.test");
    await waitFor(() => expect(resolveOld).toBeDefined());
    current = 2;
    fireEvent.click(screen.getByText("刷新"));
    await screen.findByText("cards-2");
    resolveOld("cards-1");
    await waitFor(() =>
      expect(client.getQueryData(["tool-cards", "codex"])).toBe("cards-2"),
    );
    expect(screen.queryByText("cards-1")).not.toBeInTheDocument();
  });
  it("clears cached data on session expiry", async () => {
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    mount(client);
    await screen.findByText("cards-1");
    fireEvent(window, new Event("aihubmix:session-expired"));
    await screen.findByText("登录页");
    await waitFor(() =>
      expect(client.getQueryData(["tool-cards", "codex"])).toBeUndefined(),
    );
  });
  it("starts a fresh cache when another user logs in after logout", async () => {
    mocks.dispatch.mockImplementation((command) => {
      if (command === "login") {
        current = 2;
        return Promise.resolve(account(current));
      }
      if (command === "session") return Promise.resolve(account(current));
      if (command === "tool-cards") return Promise.resolve(`cards-${current}`);
      return Promise.resolve(null);
    });
    const client = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    mount(client);
    await screen.findByText("cards-1");
    client.setQueryData(["platform-keys"], ["old-secret"]);
    fireEvent.click(screen.getByText("退出"));
    await screen.findByText("登录页");
    fireEvent.click(screen.getByText("登录"));
    await screen.findByText("cards-2");
    expect(client.getQueryData(["platform-keys"])).toBeUndefined();
    expect(screen.queryByText("cards-1")).not.toBeInTheDocument();
  });
});
