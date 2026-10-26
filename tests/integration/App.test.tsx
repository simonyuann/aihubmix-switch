import { StrictMode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({
  dispatch: vi.fn(),
  setDecorations: vi.fn().mockResolvedValue(undefined),
}));
vi.mock("@/lib/api/aihubmix", () => ({ dispatch: mocks.dispatch }));
vi.mock("@/lib/modelsDevAutoSync", () => ({
  syncModelsDevPricingOnStartup: vi.fn().mockResolvedValue({ skipped: true }),
  MODELS_DEV_SYNC_CONFIG_QUERY_KEY: ["models-dev-sync-config"],
}));
vi.mock("@/lib/query", () => ({
  useSettingsQuery: () => ({
    data: { visibleApps: { claude: true, codex: true, gemini: true } },
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
  if (command === "tool-status")
    return Promise.resolve({ configured: false, model: "", keyId: null });
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
  localStorage.clear();
  mocks.dispatch.mockImplementation(response);
});
describe("dedicated client account boundary", () => {
  it("does not mount tools or fetch wallet/keys before verifying login, including StrictMode", async () => {
    mount();
    await screen.findByRole("heading", { name: "登录你的账户" });
    expect(screen.queryByRole("navigation")).not.toBeInTheDocument();
    expect(
      mocks.dispatch.mock.calls.some(([command]) =>
        ["keys", "payment-balance", "tool-status"].includes(command),
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
  it("enters CC only after verified login and replaces removed navigation with wallet and API actions", async () => {
    mount();
    await login();
    expect(await screen.findByText("12.34")).toBeInTheDocument();
    expect(screen.getByText("USD")).toBeInTheDocument();
    for (const name of ["MCP", "Skills", "提示词", "授权中心", "添加供应商"])
      expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "充值" })).toBeInTheDocument();
    expect(
      screen.getAllByRole("button", { name: "申请 API" }).length,
    ).toBeGreaterThan(0);
    expect(screen.queryByLabelText("服务地址")).not.toBeInTheDocument();
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
  it("supports logout from the account page", async () => {
    loggedIn = true;
    mount();
    fireEvent.click(await screen.findByRole("button", { name: "我的账户" }));
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
    await screen.findByText("AIHubMix API");
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
});
