import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
const mocks = vi.hoisted(() => ({ dispatch: vi.fn() }));
vi.mock("@/lib/api/aihubmix", () => ({ dispatch: mocks.dispatch }));
import { ToolPage } from "@/components/aihubmix/ToolPage";
import { ApplyApiDialog } from "@/components/aihubmix/ApplyApiDialog";
const key = {
  id: 1,
  name: "开发",
  keyMasked: "sk-***",
  groupId: 1,
  groupName: "默认",
  status: "active",
  quotaUsed: 0,
  quota: 100,
  createdAt: "",
  local: false,
};
function wrap(node: React.ReactNode) {
  return render(
    <QueryClientProvider
      client={
        new QueryClient({ defaultOptions: { queries: { retry: false } } })
      }
    >
      {node}
    </QueryClientProvider>,
  );
}
beforeEach(() => {
  vi.clearAllMocks();
  mocks.dispatch.mockImplementation((command) =>
    Promise.resolve(
      command === "keys"
        ? [key]
        : command === "models"
          ? [{ id: "gpt-test", name: "测试模型" }]
          : command === "tool-status"
            ? { configured: false, model: "", keyId: null }
            : [],
    ),
  );
});
describe("dedicated API configuration", () => {
  it("only applies the chosen account Key/model and exposes no arbitrary endpoint or provider editor", async () => {
    wrap(<ToolPage app="codex" />);
    await screen.findByRole("option", { name: "测试模型" });
    expect(screen.queryByLabelText("服务地址")).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "添加供应商" }),
    ).not.toBeInTheDocument();
    expect(
      mocks.dispatch.mock.calls.some(
        ([command]) => command === "configure-tool",
      ),
    ).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "应用配置" }));
    expect(await screen.findByRole("status")).toHaveTextContent("配置已应用");
    expect(mocks.dispatch).toHaveBeenCalledWith("configure-tool", {
      app: "codex",
      keyId: 1,
      model: "gpt-test",
    });
  });
  it("ignores previous-Key model results and disallows applying during model loading", async () => {
    let resolveA!: (value: unknown) => void;
    const pending = new Promise((resolve) => {
      resolveA = resolve;
    });
    mocks.dispatch.mockImplementation((command, payload) =>
      Promise.resolve(
        command === "keys"
          ? [key, { ...key, id: 2, name: "另一个Key" }]
          : command === "tool-status"
            ? { configured: false, model: "", keyId: null }
            : command === "models"
              ? payload.id === 1
                ? pending
                : [{ id: "b-model", name: "B模型" }]
              : [],
      ),
    );
    wrap(<ToolPage app="claude" />);
    await screen.findByRole("option", { name: /另一个Key/ });
    expect(screen.getByRole("button", { name: "应用配置" })).toBeDisabled();
    fireEvent.change(screen.getByLabelText("API Key"), {
      target: { value: "2" },
    });
    await screen.findByRole("option", { name: "B模型" });
    await act(async () => {
      resolveA([{ id: "a-model", name: "A模型" }]);
    });
    expect(
      screen.queryByRole("option", { name: "A模型" }),
    ).not.toBeInTheDocument();
    expect(screen.getByLabelText("模型")).toHaveValue("b-model");
  });
  it("excludes imported local keys and does not auto-apply or create keys for empty accounts", async () => {
    mocks.dispatch.mockImplementation((command) =>
      Promise.resolve(
        command === "keys"
          ? [{ ...key, id: -1, local: true }]
          : command === "tool-status"
            ? { configured: false, model: "", keyId: null }
            : [],
      ),
    );
    wrap(<ToolPage app="codex" />);
    await screen.findByText("先申请一个 API Key，即可连接此工具。");
    expect(screen.queryByLabelText("API Key")).not.toBeInTheDocument();
    expect(
      mocks.dispatch.mock.calls.some(([command]) =>
        ["models", "configure-tool", "create-key"].includes(command),
      ),
    ).toBe(false);
  });
  it("creates a platform Key only after explicit confirmation and stops duplicate or uncertain retries", async () => {
    let fail!: (error: unknown) => void;
    const pending = new Promise((_, reject) => {
      fail = reject;
    });
    mocks.dispatch.mockImplementation((command) =>
      command === "groups"
        ? Promise.resolve([{ id: 7, name: "平台服务", platform: "openai" }])
        : pending,
    );
    wrap(<ApplyApiDialog onClose={() => {}} />);
    await screen.findByRole("option", { name: "平台服务 · openai" });
    expect(
      mocks.dispatch.mock.calls.some(([command]) => command === "create-key"),
    ).toBe(false);
    const submit = screen.getByRole("button", { name: "确认申请" });
    fireEvent.click(submit);
    fireEvent.click(submit);
    expect(
      mocks.dispatch.mock.calls.filter(([command]) => command === "create-key"),
    ).toHaveLength(1);
    await act(async () => {
      fail(new Error("timeout"));
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "请先刷新 Key 列表确认结果",
    );
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "确认申请" })).toBeDisabled(),
    );
  });
});
