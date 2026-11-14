import {
  fireEvent,
  render,
  screen,
  waitFor,
  cleanup,
  act,
} from "@testing-library/react";
import { beforeEach, afterEach, describe, expect, it, vi } from "vitest";
import "@testing-library/jest-dom";
import { ClaudeDesktopControls } from "@/components/aihubmix/ClaudeDesktopControls";
import {
  claudeDesktopError,
  setClaudeDesktopPreferences,
} from "@/lib/claudeDesktop";

const { invoke } = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke,
  Channel: class {
    onmessage = (_phase: string) => {};
  },
}));
vi.mock("@/lib/platform", () => ({
  isWindows: () => true,
  isMac: () => false,
}));
beforeEach(() => {
  localStorage.clear();
  invoke.mockReset();
});
afterEach(cleanup);

describe("Claude Chinese launch", () => {
  it("requires explicit acknowledgment of executable and Cowork changes", async () => {
    invoke
      .mockRejectedValueOnce("consent_required")
      .mockResolvedValueOnce("localized_started");
    render(<ClaudeDesktopControls />);
    fireEvent.click(
      screen.getByRole("button", { name: "中文启动 Claude Desktop" }),
    );
    await screen.findByRole("dialog");
    expect(screen.getByText(/降低对客户端签名的校验/)).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke.mock.calls[0][1].consent).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "同意并中文启动" }));
    await screen.findByText("中文补丁已就绪，已发送启动请求。");
    expect(invoke.mock.calls[1][1]).toEqual({
      executablePath: "",
      action: "chinese",
      consent: true,
      onProgress: expect.any(Object),
      operationId: expect.any(String),
    });
  });
  it("cancel does not apply the patch", async () => {
    invoke.mockRejectedValue("consent_required");
    render(<ClaudeDesktopControls />);
    fireEvent.click(
      screen.getByRole("button", { name: "中文启动 Claude Desktop" }),
    );
    fireEvent.click(await screen.findByRole("button", { name: "取消" }));
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("keeps already-patched launches prompt-free and suppresses duplicate clicks", async () => {
    let resolve!: (value: string) => void;
    invoke.mockImplementation(
      () =>
        new Promise<string>((done) => {
          resolve = done;
        }),
    );
    render(<ClaudeDesktopControls />);
    const button = screen.getByRole("button", {
      name: "中文启动 Claude Desktop",
    });
    fireEvent.click(button);
    fireEvent.click(button);
    expect(invoke).toHaveBeenCalledTimes(1);
    resolve("localized_started");
    await screen.findByText("中文补丁已就绪，已发送启动请求。");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
  it("launches selected unpacked app without patch when preserve is selected", async () => {
    setClaudeDesktopPreferences({
      language: "system",
      executablePath: "C:\\Apps\\Claude.exe",
    });
    invoke.mockResolvedValue("started");
    render(<ClaudeDesktopControls />);
    fireEvent.click(
      screen.getByRole("button", { name: "启动 Claude Desktop" }),
    );
    await screen.findByText("已发送启动请求。");
    expect(invoke).toHaveBeenCalledWith("launch_claude_desktop", {
      executablePath: "C:\\Apps\\Claude.exe",
      action: "launch",
      consent: false,
      onProgress: expect.any(Object),
      operationId: expect.any(String),
    });
  });
  it("restores only after confirmation and switches the next launch to preserve", async () => {
    invoke.mockResolvedValue("restored");
    render(<ClaudeDesktopControls showRestore />);
    fireEvent.click(screen.getByRole("button", { name: "恢复原版" }));
    expect(invoke).not.toHaveBeenCalled();
    fireEvent.click(
      screen.getAllByRole("button", { name: "恢复原版" }).at(-1)!,
    );
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "启动 Claude Desktop" }),
      ).toBeInTheDocument(),
    );
    expect(invoke.mock.calls[0][1]).toMatchObject({
      action: "restore",
      consent: true,
    });
  });
  it("never exposes raw errors, paths, or secrets", async () => {
    invoke.mockRejectedValue("failed path /Users/private auth=sk-test-secret");
    render(<ClaudeDesktopControls />);
    fireEvent.click(
      screen.getByRole("button", { name: "中文启动 Claude Desktop" }),
    );
    await screen.findByText(
      "操作未完成，请检查 Claude Desktop 安装状态后重试。",
    );
    expect(document.body.textContent).not.toContain("sk-test-secret");
    expect(claudeDesktopError("toString")).toContain("操作未完成");
    expect(claudeDesktopError(new Error("secret"))).not.toContain("secret");
  });
  it("shows native installation phases inside the confirmation dialog", async () => {
    let finish!: (value: string) => void;
    invoke.mockRejectedValueOnce("consent_required").mockImplementationOnce(
      () =>
        new Promise<string>((resolve) => {
          finish = resolve;
        }),
    );
    render(<ClaudeDesktopControls />);
    fireEvent.click(
      screen.getByRole("button", { name: "中文启动 Claude Desktop" }),
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "同意并中文启动" }),
    );
    const channel = invoke.mock.calls[1][1].onProgress;
    expect(screen.getByRole("dialog")).toHaveClass("overflow-y-auto");
    act(() =>
      channel.onmessage({ phase: "authorizing", interruptible: false }),
    );
    expect(screen.getByRole("progressbar")).toHaveAccessibleName(
      "等待系统授权",
    );
    expect(screen.getByText(/请在系统弹窗中允许/)).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "取消操作" }),
    ).toBeInTheDocument();
    for (const [phase, label] of [
      ["checking_backup", "检查旧补丁与备份"],
      ["cleaning", "清理旧补丁组件"],
      ["closing", "关闭 Claude 与后台服务"],
      ["permissions", "准备应用目录写入权限"],
    ]) {
      act(() => channel.onmessage({ phase, interruptible: true }));
      expect(screen.getByRole("progressbar")).toHaveAccessibleName(label);
    }
    act(() =>
      channel.onmessage({
        phase: "restoring",
        completed: 2,
        total: 5,
        interruptible: false,
      }),
    );
    expect(screen.getByRole("progressbar")).toHaveAccessibleName(
      "恢复原版文件",
    );
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "40",
    );
    act(() =>
      channel.onmessage({
        phase: "translating",
        completed: 20,
        total: 100,
        interruptible: true,
      }),
    );
    expect(screen.getByRole("progressbar")).toHaveAccessibleName(
      "汉化界面文字",
    );
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "20",
    );
    expect(screen.getByText(/已处理 20 \/ 100 项/)).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    act(() => channel.onmessage({ phase: "verifying", interruptible: true }));
    expect(screen.getByRole("progressbar")).toHaveAccessibleName(
      "校验应用文件",
    );
    await act(async () => {
      finish("localized_started");
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  });
  it("sends cancellation only for the active operation and waits for safe completion", async () => {
    let finish!: (value: string) => void;
    invoke.mockImplementation((command) =>
      command === "cancel_claude_desktop_launch"
        ? Promise.resolve()
        : new Promise<string>((resolve) => {
            finish = resolve;
          }),
    );
    render(<ClaudeDesktopControls />);
    fireEvent.click(
      screen.getByRole("button", { name: "中文启动 Claude Desktop" }),
    );
    const args = invoke.mock.calls[0][1];
    act(() =>
      args.onProgress.onmessage({ phase: "replacing", interruptible: false }),
    );
    fireEvent.click(screen.getByRole("button", { name: "取消操作" }));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("cancel_claude_desktop_launch", {
        operationId: args.operationId,
      }),
    );
    expect(
      screen.getByRole("button", { name: "正在安全停止…" }),
    ).toBeDisabled();
    expect(screen.getByText(/当前写入完成后结束/)).toBeInTheDocument();
    await act(async () => {
      finish("stopped_after_install");
    });
    expect(
      screen.getByText("补丁处理已完成，已按要求停止自动启动。"),
    ).toBeInTheDocument();
    expect(screen.queryByRole("progressbar")).not.toBeInTheDocument();
  });

  it("retains the failed stage and offers a retry without showing script output", async () => {
    let reject!: (error: string) => void;
    invoke.mockImplementation(
      () =>
        new Promise((_resolve, fail) => {
          reject = fail;
        }),
    );
    render(<ClaudeDesktopControls />);
    fireEvent.click(
      screen.getByRole("button", { name: "中文启动 Claude Desktop" }),
    );
    act(() =>
      invoke.mock.calls[0][1].onProgress.onmessage({
        phase: "signing",
        completed: 2,
        total: 8,
        interruptible: true,
      }),
    );
    await act(async () => reject("patch_failed"));
    expect(screen.getByText("未完成 · 重新签名应用")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "中文启动 Claude Desktop" }),
    ).toBeEnabled();
    expect(
      screen.queryByRole("button", { name: "取消操作" }),
    ).not.toBeInTheDocument();
  });
});
