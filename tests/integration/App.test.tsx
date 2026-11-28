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
    localStorage.setItem("aihubmix:advanced-interface", "true");
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
      "连接暂时不可用，请检查网络后重试。",
    );
    expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
  });
  it("keeps one wallet above help and settings in simple navigation", async () => {
    mount();
    await login();
    const balance = await screen.findByText("USD 12.34");
    const nav = screen.getByRole("navigation");
    const wallet = within(nav).getByRole("region", { name: "账户余额" });
    const help = within(nav).getByRole("button", { name: "帮助与答疑" });
    expect(wallet).toContainElement(balance);
    for (const item of [
      help,
      within(nav).getByRole("button", { name: "设置" }),
    ]) {
      expect(
        wallet.compareDocumentPosition(item) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    }
    expect(screen.getAllByRole("button", { name: "刷新余额" })).toHaveLength(1);
    expect(balance.closest("button, a, [role='button']")).toBeNull();
    expect(
      within(wallet).getByRole("button", { name: "充值" }),
    ).toBeInTheDocument();
    for (const name of ["授权中心", "添加供应商"])
      expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
    expect(
      within(nav).queryByRole("button", { name: "API 管理" }),
    ).not.toBeInTheDocument();
    expect(
      screen.getAllByRole("button", { name: "toolPage.action.addGroup" })
        .length,
    ).toBeGreaterThan(0);
    expect(screen.queryByLabelText("服务地址")).not.toBeInTheDocument();

    fireEvent.click(help);
    await screen.findByRole("heading", { name: "帮助与答疑" });
    expect(within(wallet).getByRole("button", { name: "充值" })).toBeEnabled();
    expect(screen.getAllByRole("button", { name: "刷新余额" })).toHaveLength(1);
  });
  it("opens recharge from home and keeps API management accessible from account settings", async () => {
    loggedIn = true;
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "充值" }));
    expect(await screen.findByRole("dialog")).toHaveTextContent("账户充值");
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    fireEvent.click(
      within(screen.getByRole("navigation")).getByRole("button", {
        name: "设置",
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "管理 API Key" }),
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
  it("opens the announcement list from the sidebar and reads an item on its own page", async () => {
    loggedIn = true;
    mocks.dispatch.mockImplementation((command: string) =>
      command === "announcements"
        ? Promise.resolve([
            {
              id: 18,
              title: "维护通知",
              content: "今晚维护服务。",
              notifyMode: "popup",
              createdAt: "2026-11-24T08:00:00Z",
              readAt: null,
            },
            {
              id: 17,
              title: "版本说明",
              content: "上一条公告。",
              notifyMode: "silent",
              createdAt: "2026-11-23T08:00:00Z",
              readAt: "2026-11-23T08:30:00Z",
            },
          ])
        : response(command),
    );
    mount();
    expect(
      await screen.findByRole("button", { name: /新公告.*维护通知/ }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    fireEvent.click(
      within(screen.getByRole("navigation")).getByRole("button", {
        name: "公告",
      }),
    );
    expect(
      await screen.findByRole("heading", { name: "公告" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: /版本说明/ }),
    ).toBeInTheDocument();
    expect(screen.queryByText("今晚维护服务。")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /维护通知/ }));
    expect(await screen.findByText("今晚维护服务。")).toBeInTheDocument();
    expect(mocks.dispatch).toHaveBeenCalledWith("announcement-read", {
      id: 18,
    });
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
    fireEvent.click(
      within(await screen.findByRole("navigation")).getByRole("button", {
        name: "设置",
      }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "管理 API Key" }),
    );
    await screen.findByRole("heading", { name: "API 管理" });
    expect(
      screen.queryByRole("button", { name: "退出登录" }),
    ).not.toBeInTheDocument();
    fireEvent.click(
      screen.getByRole("button", {
        name: /^(设置|common.settings|nav.settings)$/,
      }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "退出登录" }));
    await screen.findByRole("heading", { name: "登录你的账户" });
    expect(mocks.dispatch).toHaveBeenCalledWith("logout");
    expect(screen.queryByText(/请关闭并重新打开/)).not.toBeInTheDocument();
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
  it("keeps the login form compact even with agreements configured and a failed request", async () => {
    mocks.dispatch.mockImplementation((command) => {
      if (command === "auth-settings")
        return Promise.resolve({
          ...settings,
          agreements: [{ title: "服务条款", content: "测试" }],
        });
      if (command === "login") return Promise.reject("连接服务超时");
      return response(command);
    });
    mount();
    await screen.findByRole("heading", { name: "登录你的账户" });
    expect(screen.getByLabelText("记住我")).not.toBeChecked();
    fireEvent.change(screen.getByLabelText("邮箱"), {
      target: { value: "a@example.test" },
    });
    fireEvent.change(screen.getByLabelText("密码"), {
      target: { value: "test-password" },
    });
    const submit = screen.getByRole("button", { name: "登录" });
    await waitFor(() => expect(submit).toBeEnabled());
    fireEvent.click(submit);
    expect(await screen.findByRole("alert")).toHaveTextContent("连接服务超时");
    for (const label of ["服务条款", "网络诊断", "手动下载安装包"])
      expect(screen.queryByText(label)).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "立即注册" }));
    expect(await screen.findByText("服务条款")).toBeInTheDocument();
  });
  it("uses the saved password only for its email and only after clicking login", async () => {
    mocks.dispatch.mockImplementation((command) =>
      command === "remembered-login"
        ? Promise.resolve("a@example.test")
        : response(command),
    );
    mount();
    const email = await screen.findByDisplayValue("a@example.test");
    const pwd = screen.getByLabelText("密码");
    expect(pwd).toHaveValue("");
    expect(screen.getByLabelText("记住我")).toBeChecked();
    fireEvent.change(email, { target: { value: "b@example.test" } });
    expect(pwd).toBeRequired();
    fireEvent.change(email, { target: { value: "a@example.test" } });
    expect(pwd).not.toBeRequired();
    expect(mocks.dispatch.mock.calls.some(([cmd]) => cmd === "login")).toBe(
      false,
    );
    fireEvent.click(screen.getByRole("button", { name: "登录" }));
    await screen.findByRole("navigation");
    expect(mocks.dispatch).toHaveBeenCalledWith(
      "login",
      expect.objectContaining({
        password: "",
        remember: true,
        useSavedPassword: true,
      }),
    );
  });
  it("forgets saved credentials immediately when unchecked", async () => {
    mocks.dispatch.mockImplementation((command) =>
      command === "remembered-login"
        ? Promise.resolve("a@example.test")
        : response(command),
    );
    mount();
    await screen.findByDisplayValue("a@example.test");
    fireEvent.click(screen.getByLabelText("记住我"));
    await waitFor(() =>
      expect(screen.getByLabelText("记住我")).not.toBeChecked(),
    );
    expect(mocks.dispatch).toHaveBeenCalledWith("forget-remembered-login");
    expect(screen.getByLabelText("密码")).toBeRequired();
  });
  it("can reconnect after public account settings fail to load", async () => {
    let attempts = 0;
    mocks.dispatch.mockImplementation((command) => {
      if (command === "auth-settings" && attempts++ === 0)
        return Promise.reject("offline");
      return response(command);
    });
    mount();
    const retry = await screen.findByRole("button", { name: "重新连接平台" });
    expect(screen.getByRole("button", { name: "登录" })).toBeDisabled();
    expect(attempts).toBe(1);
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
