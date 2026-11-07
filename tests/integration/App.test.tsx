import { StrictMode, useMemo } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({
  otherProviders: false,
  dispatch: vi.fn(),
  setDecorations: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@/lib/api/aihubmix", () => ({ dispatch: mocks.dispatch }));
vi.mock("@/lib/modelsDevAutoSync", () => ({
  syncModelsDevPricingOnStartup: vi.fn().mockResolvedValue({ skipped: true }),
  MODELS_DEV_SYNC_CONFIG_QUERY_KEY: ["models-dev-sync-config"],
}));
vi.mock("@/lib/query", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/query")>()),
  useSettingsQuery: () => ({
    data: useMemo(
      () => ({
        visibleApps: { claude: true, codex: true, gemini: true },
        firstRunNoticeConfirmed: true,
        enableOtherProviders: mocks.otherProviders,
        newLayoutNoticeConfirmed: true,
        whatsNewSeenVersion: "99.0.0",
      }),
      [mocks.otherProviders],
    ),
  }),
}));
vi.mock("@/contexts/UpdateContext", () => ({
  useUpdate: () => ({ hasUpdate: false }),
}));
vi.mock("@/hooks/useSidebarStatus", () => ({
  useSidebarStatus: () => ({
    appStatus: () => ({ mode: "direct", alert: false }),
    todayCost: 0,
    todayTokens: 0,
    todayRequests: 0,
    todayHitRate: 0,
  }),
}));
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({ setDecorations: mocks.setDecorations }),
}));
vi.mock("@/components/theme-provider", () => ({
  useTheme: () => ({ theme: "system", setTheme: vi.fn() }),
}));
vi.mock("@/components/JsonEditor", () => ({
  default: ({ value }: { value: string }) => (
    <textarea readOnly value={value} />
  ),
}));
import App from "@/App";
const account = {
  relayBaseUrl: "https://csmy.fun:23457",
  account: { loggedIn: true, email: "a@example.test", userId: 1 },
  localKeys: [],
  legacyData: false,
  storageError: null,
};
const settings = {
  registrationEnabled: true,
  emailVerifyEnabled: true,
  passwordResetEnabled: true,
  invitationCodeEnabled: false,
  captchaRequired: false,
};
let loggedIn = false;
function response(command: string) {
  if (command === "session")
    return loggedIn
      ? Promise.resolve(account)
      : Promise.reject("AUTH_REQUIRED: 请先登录账号");
  if (command === "auth-settings") return Promise.resolve(settings);
  if (command === "login" || command === "register") {
    loggedIn = true;
    return Promise.resolve(account);
  }
  if (command === "logout") {
    loggedIn = false;
    return Promise.resolve({ ...account, account: { loggedIn: false } });
  }
  if (command === "payment-balance")
    return Promise.resolve({
      email: "a@example.test",
      balance: 12.34,
      currency: "USD",
    });
  if (command === "payment-overview")
    return Promise.resolve({
      enabled: true,
      balance: 12.34,
      currency: "USD",
      email: "a@example.test",
      methods: [],
      orders: [],
    });
  if (command === "groups")
    return Promise.resolve([{ id: 7, name: "平台服务", platform: "openai" }]);
  if (command === "tool-cards")
    return Promise.resolve({ cards: [], currentProviderId: null });
  return Promise.resolve([]);
}
function mount() {
  return render(
    <StrictMode>
      <QueryClientProvider
        client={
          new QueryClient({ defaultOptions: { queries: { retry: false } } })
        }
      >
        <App />
      </QueryClientProvider>
    </StrictMode>,
  );
}
async function login() {
  await screen.findByRole("heading", { name: "登录你的账户" });
  fireEvent.change(screen.getByLabelText("邮箱"), {
    target: { value: "a@example.test" },
  });
  fireEvent.change(screen.getByLabelText("密码", { selector: "input" }), {
    target: { value: "secret-pass" },
  });
  await waitFor(() =>
    expect(screen.getByRole("button", { name: "登录" })).toBeEnabled(),
  );
  fireEvent.click(screen.getByRole("button", { name: "登录" }));
  await screen.findByRole("navigation");
}
beforeEach(() => {
  vi.clearAllMocks();
  loggedIn = false;
  mocks.otherProviders = false;
  localStorage.clear();
  mocks.dispatch.mockImplementation(response);
});
describe("dedicated client account boundary", () => {
  it("restores the original CC provider workspace only after opting in", async () => {
    loggedIn = true;
    mocks.otherProviders = true;
    mount();
    await screen.findByRole("navigation");
    const add = await screen.findByRole("button", {
      name: /^(添加供应商|provider.addProvider)$/,
    });
    fireEvent.click(add);
    expect(
      await screen.findByRole("heading", {
        name: /^(添加供应商|provider.addNewProvider)$/,
      }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "平台分组" }),
    ).toBeInTheDocument();
  });

  it("does not mount tools or fetch wallet/keys before verifying login, including StrictMode", async () => {
    mount();
    await screen.findByRole("heading", { name: "登录你的账户" });
    expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
    expect(
      mocks.dispatch.mock.calls.some(([command]) =>
        ["keys", "payment-balance", "tool-cards"].includes(command),
      ),
    ).toBe(false);
  });
  it("keeps stored but unverified sessions behind the login screen", async () => {
    mocks.dispatch.mockImplementation((command) =>
      command === "session"
        ? Promise.reject("network offline")
        : response(command),
    );
    mount();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "network offline",
    );
    expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
  });
  it("shows a non-interactive header balance and places recharge and API management immediately above MCP", async () => {
    mount();
    await login();
    const balance = await screen.findByText("USD 12.34");
    const header = balance.closest("header");
    expect(header).not.toBeNull();
    expect(balance.closest("button, a, [role='button']")).toBeNull();
    const addGroup = screen.getByRole("button", { name: "添加分组" });
    expect(header).toContainElement(addGroup);
    expect(
      balance.compareDocumentPosition(addGroup) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    for (const name of ["MCP", "Skills", "nav.prompts"])
      expect(screen.getByRole("button", { name })).toBeInTheDocument();
    for (const name of ["授权中心", "添加供应商"])
      expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
    const nav = screen.getByRole("navigation");
    expect(nav).not.toContainElement(balance);
    expect(within(nav).queryByText("余额")).not.toBeInTheDocument();
    const labels = Array.from(nav.querySelectorAll("button")).map(
      (b) => b.getAttribute("aria-label") || b.textContent,
    );
    const mcp = labels.indexOf("MCP");
    expect(labels.slice(mcp - 2, mcp).map((s) => s?.split(" · ")[0])).toEqual([
      "充值",
      "API 管理",
    ]);
    expect(screen.getByRole("button", { name: "充值" })).toBeInTheDocument();
    expect(
      screen.getAllByRole("button", { name: "申请 API" }).length,
    ).toBeGreaterThan(0);
    expect(screen.queryByLabelText("服务地址")).not.toBeInTheDocument();
  });
  it("opens recharge and API management from the sidebar and keeps API application in the page header", async () => {
    loggedIn = true;
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "充值" }));
    expect(await screen.findByRole("dialog")).toHaveTextContent("账户充值");
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    const nav = screen.getByRole("navigation");
    fireEvent.click(
      Array.from(nav.querySelectorAll("button")).find(
        (b) => b.textContent === "API 管理",
      )!,
    );
    await screen.findByRole("heading", { name: "API 管理" });
    expect(await screen.findByText("USD 12.34")).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    fireEvent.click(screen.getAllByRole("button", { name: "申请 API" })[0]);
    expect(await screen.findByRole("dialog")).toHaveTextContent("申请 API");
    await screen.findByRole("option", { name: "平台服务 · openai" });
    fireEvent.click(screen.getByRole("button", { name: "取消" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(
      mocks.dispatch.mock.calls.some(([c]) =>
        ["payment-create", "create-key"].includes(c),
      ),
    ).toBe(false);
  });
  it("returns to login on expiry and clears the authenticated interface", async () => {
    loggedIn = true;
    mount();
    await screen.findByRole("navigation");
    act(() => window.dispatchEvent(new Event("aihubmix:session-expired")));
    await screen.findByRole("heading", { name: "登录你的账户" });
    expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
    expect(screen.getByRole("alert")).toHaveTextContent("登录已失效");
  });
  it("supports logout from settings instead of API management", async () => {
    loggedIn = true;
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "API 管理" }));
    await screen.findByRole("heading", { name: "API 管理" });
    expect(
      screen.queryByRole("button", { name: "退出登录" }),
    ).not.toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", {
        name: /^(设置|common.settings|nav.settings)$/,
      }),
    );
    expect(screen.queryByText("USD 12.34")).not.toBeInTheDocument();
    fireEvent.click(await screen.findByRole("button", { name: "退出登录" }));
    await screen.findByRole("heading", { name: "登录你的账户" });
    expect(mocks.dispatch).toHaveBeenCalledWith("logout");
  });
  it("sends a reset email explicitly and remains outside the application", async () => {
    mount();
    await screen.findByRole("heading", { name: "登录你的账户" });
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "忘记密码？" })).toBeEnabled(),
    );
    fireEvent.click(screen.getByRole("button", { name: "忘记密码？" }));
    fireEvent.change(screen.getByLabelText("邮箱"), {
      target: { value: "a@example.test" },
    });
    fireEvent.click(screen.getByRole("button", { name: "发送重置邮件" }));
    expect(await screen.findByRole("status")).toHaveTextContent("密码重置邮件");
    expect(mocks.dispatch).toHaveBeenCalledWith("forgot-password", {
      email: "a@example.test",
    });
    expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
  });
  it("registers with email verification and never submits mismatched passwords", async () => {
    mount();
    await screen.findByRole("heading", { name: "登录你的账户" });
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "立即注册" })).toBeEnabled(),
    );
    fireEvent.click(screen.getByRole("button", { name: "立即注册" }));
    fireEvent.change(screen.getByLabelText("邮箱"), {
      target: { value: "a@example.test" },
    });
    fireEvent.change(screen.getByLabelText("密码", { selector: "input" }), {
      target: { value: "secret-pass" },
    });
    fireEvent.change(screen.getByLabelText("确认密码"), {
      target: { value: "different" },
    });
    fireEvent.click(screen.getByRole("button", { name: "发送验证码" }));
    await screen.findByText("验证码已发送，请检查邮箱");
    expect(mocks.dispatch).toHaveBeenCalledWith("send-code", {
      email: "a@example.test",
    });
    fireEvent.change(screen.getByLabelText("邮箱验证码"), {
      target: { value: "123456" },
    });
    fireEvent.click(screen.getByRole("button", { name: "注册并登录" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("两次输入");
    expect(
      mocks.dispatch.mock.calls.some(([command]) => command === "register"),
    ).toBe(false);
    fireEvent.change(screen.getByLabelText("确认密码"), {
      target: { value: "secret-pass" },
    });
    fireEvent.click(screen.getByRole("button", { name: "注册并登录" }));
    await screen.findByRole("navigation");
    expect(mocks.dispatch).toHaveBeenCalledWith("register", {
      email: "a@example.test",
      password: "secret-pass",
      emailCode: "123456",
      invitationCode: "",
    });
  });
  it("does not restore a removed page from local navigation state", async () => {
    localStorage.setItem("cc-switch-last-view", "auth");
    loggedIn = true;
    mount();
    await screen.findByRole("navigation");
    expect(screen.queryByText("授权中心")).not.toBeInTheDocument();
  });
  it("can reconnect after public account settings fail to load", async () => {
    let attempts = 0;
    mocks.dispatch.mockImplementation((command) => {
      if (command === "auth-settings" && attempts++ < 2)
        return Promise.reject("offline");
      return response(command);
    });
    mount();
    const retry = await screen.findByRole("button", { name: "重新连接平台" });
    expect(screen.getByRole("button", { name: "登录" })).toBeDisabled();
    fireEvent.click(retry);
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "登录" })).toBeEnabled(),
    );
  });

  it("does not restore the application from a login response after session invalidation", async () => {
    let finishLogin!: (value: unknown) => void;
    mocks.dispatch.mockImplementation((command) =>
      command === "login"
        ? new Promise((resolve) => {
            finishLogin = resolve;
          })
        : response(command),
    );
    mount();
    await screen.findByRole("heading", { name: "登录你的账户" });
    fireEvent.change(screen.getByLabelText("邮箱"), {
      target: { value: "a@example.test" },
    });
    fireEvent.change(screen.getByLabelText("密码", { selector: "input" }), {
      target: { value: "secret-pass" },
    });
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "登录" })).toBeEnabled(),
    );
    fireEvent.click(screen.getByRole("button", { name: "登录" }));
    act(() => window.dispatchEvent(new Event("aihubmix:session-expired")));
    loggedIn = true;
    await act(async () => finishLogin(account));
    expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "登录你的账户" }),
    ).toBeInTheDocument();
  });
  it("lets the user clear a hidden old account when signing in as another user", async () => {
    mocks.dispatch.mockImplementation((command) =>
      command === "login"
        ? Promise.reject("请先退出当前账号，再登录其他账号")
        : response(command),
    );
    mount();
    await screen.findByRole("heading", { name: "登录你的账户" });
    fireEvent.change(screen.getByLabelText("邮箱"), {
      target: { value: "other@example.test" },
    });
    fireEvent.change(screen.getByLabelText("密码", { selector: "input" }), {
      target: { value: "fixture-password" },
    });
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "登录" })).toBeEnabled(),
    );
    fireEvent.click(screen.getByRole("button", { name: "登录" }));
    fireEvent.click(
      await screen.findByRole("button", { name: "退出本机旧账号" }),
    );
    await screen.findByText("已退出本机旧账号，请重新登录。");
    expect(mocks.dispatch).toHaveBeenCalledWith("logout");
  });
});
