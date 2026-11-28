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
      screen.getByRole("button", { name: "claudeDesktop.action.chineseStart" }),
    );
    await screen.findByRole("dialog");
    expect(
      screen.getByText("claudeDesktop.dialog.windowsNote"),
    ).toBeInTheDocument();
    expect(invoke).toHaveBeenCalledTimes(1);
    expect(invoke.mock.calls[0][1].consent).toBe(false);
    fireEvent.click(
      screen.getByRole("button", {
        name: "claudeDesktop.action.agreeAndStart",
      }),
    );
    await screen.findByText("claudeDesktop.message.chineseReady");
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
      screen.getByRole("button", { name: "claudeDesktop.action.chineseStart" }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: "claudeDesktop.action.cancel",
      }),
    );
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
      name: "claudeDesktop.action.chineseStart",
    });
    fireEvent.click(button);
    fireEvent.click(button);
    expect(invoke).toHaveBeenCalledTimes(1);
    resolve("localized_started");
    await screen.findByText("claudeDesktop.message.chineseReady");
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
      screen.getByRole("button", { name: "claudeDesktop.action.start" }),
    );
    await screen.findByText("claudeDesktop.message.launchRequested");
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
    fireEvent.click(
      screen.getByRole("button", { name: "claudeDesktop.action.restore" }),
    );
    expect(invoke).not.toHaveBeenCalled();
    fireEvent.click(
      screen
        .getAllByRole("button", { name: "claudeDesktop.action.restore" })
        .at(-1)!,
    );
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "claudeDesktop.action.start" }),
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
      screen.getByRole("button", { name: "claudeDesktop.action.chineseStart" }),
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
      screen.getByRole("button", { name: "claudeDesktop.action.chineseStart" }),
    );
    fireEvent.click(
      await screen.findByRole("button", {
        name: "claudeDesktop.action.agreeAndStart",
      }),
    );
    const channel = invoke.mock.calls[1][1].onProgress;
    expect(screen.getByRole("dialog")).toHaveClass("overflow-y-auto");
    act(() =>
      channel.onmessage({ phase: "authorizing", interruptible: false }),
    );
    expect(screen.getByRole("progressbar")).toHaveAccessibleName(
      "claudeDesktop.phase.authorizing",
    );
    expect(
      screen.getByText("claudeDesktop.progress.authorizeHint"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", {
        name: "claudeDesktop.action.cancelOperation",
      }),
    ).toBeInTheDocument();
    for (const [phase, label] of [
      ["checking_backup", "claudeDesktop.phase.checking_backup"],
      ["cleaning", "claudeDesktop.phase.cleaning"],
      ["closing", "claudeDesktop.phase.closing"],
      ["permissions", "claudeDesktop.phase.permissions"],
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
      "claudeDesktop.phase.restoring",
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
      "claudeDesktop.phase.translating",
    );
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "20",
    );
    expect(
      screen.getByText("claudeDesktop.progress.countedLine"),
    ).toBeInTheDocument();
    fireEvent.keyDown(screen.getByRole("dialog"), { key: "Escape" });
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    act(() => channel.onmessage({ phase: "verifying", interruptible: true }));
    expect(screen.getByRole("progressbar")).toHaveAccessibleName(
      "claudeDesktop.phase.verifying",
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
      screen.getByRole("button", { name: "claudeDesktop.action.chineseStart" }),
    );
    const args = invoke.mock.calls[0][1];
    act(() =>
      args.onProgress.onmessage({ phase: "replacing", interruptible: false }),
    );
    fireEvent.click(
      screen.getByRole("button", {
        name: "claudeDesktop.action.cancelOperation",
      }),
    );
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("cancel_claude_desktop_launch", {
        operationId: args.operationId,
      }),
    );
    expect(
      screen.getByRole("button", { name: "claudeDesktop.action.stopping" }),
    ).toBeDisabled();
    expect(
      screen.getByText("claudeDesktop.progress.safeStop"),
    ).toBeInTheDocument();
    await act(async () => {
      finish("stopped_after_install");
    });
    expect(
      screen.getByText("claudeDesktop.message.stoppedAfterInstall"),
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
      screen.getByRole("button", { name: "claudeDesktop.action.chineseStart" }),
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
    expect(
      screen.getByText(
        "claudeDesktop.status.failedPrefixclaudeDesktop.phase.signing",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "claudeDesktop.action.chineseStart" }),
    ).toBeEnabled();
    expect(
      screen.queryByRole("button", {
        name: "claudeDesktop.action.cancelOperation",
      }),
    ).not.toBeInTheDocument();
  });
});
