import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
const { open } = vi.hoisted(() => ({ open: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open }));
vi.mock("@/lib/platform", () => ({
  isWindows: () => true,
  isMac: () => false,
}));
import { DesktopProgramLocation } from "@/components/settings/DesktopProgramLocation";

describe("desktop program location", () => {
  beforeEach(() => open.mockReset());

  it.each(["Codex", "Claude"])("shows the detected %s path", async (name) => {
    const executablePath = `C:\\Programs\\${name}\\${name}.exe`;
    const inspect = vi
      .fn()
      .mockResolvedValue({ installed: true, executablePath });
    render(
      <DesktopProgramLocation
        name={name}
        executablePath=""
        inspect={inspect}
        onChange={vi.fn()}
      />,
    );
    expect(await screen.findByText("自动识别")).toBeInTheDocument();
    expect(screen.getByText(executablePath)).toBeInTheDocument();
  });

  it("asks users to select a program when none is detected", async () => {
    render(
      <DesktopProgramLocation
        name="Codex"
        executablePath=""
        inspect={vi.fn().mockResolvedValue({ installed: false })}
        onChange={vi.fn()}
      />,
    );
    expect(await screen.findByText("未找到，请手动选择")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "选择 Codex 程序" }),
    ).toBeEnabled();
  });

  it("validates a selected program before saving it", async () => {
    const path = "D:\\解压 软件\\ChatGPT.exe";
    const inspect = vi.fn().mockResolvedValue({ installed: false });
    const onChange = vi.fn();
    render(
      <DesktopProgramLocation
        name="Codex"
        executablePath=""
        inspect={inspect}
        onChange={onChange}
      />,
    );
    await screen.findByText("未找到，请手动选择");
    open.mockResolvedValue(path);
    fireEvent.click(screen.getByRole("button", { name: "选择 Codex 程序" }));
    await screen.findByRole("alert");
    expect(onChange).not.toHaveBeenCalled();
    inspect.mockResolvedValue({ installed: true, executablePath: path });
    fireEvent.click(screen.getByRole("button", { name: "选择 Codex 程序" }));
    await waitFor(() => expect(onChange).toHaveBeenCalledWith(path));
    expect(inspect).toHaveBeenCalledWith(path);
  });

  it("labels manual paths and lets users return to automatic detection", async () => {
    const path = "D:\\Claude\\Claude.exe";
    const onChange = vi.fn();
    render(
      <DesktopProgramLocation
        name="Claude"
        executablePath={path}
        inspect={vi
          .fn()
          .mockResolvedValue({ installed: true, executablePath: path })}
        onChange={onChange}
      />,
    );
    await screen.findByText("手动指定");
    fireEvent.click(screen.getByRole("button", { name: "恢复自动查找" }));
    expect(onChange).toHaveBeenCalledWith("");
  });

  it("handles inspection errors without displaying internal output", async () => {
    render(
      <DesktopProgramLocation
        name="Codex"
        executablePath=""
        inspect={() => {
          throw new Error("token=secret");
        }}
        onChange={vi.fn()}
      />,
    );
    await screen.findByText("检测失败，请重新检测或选择程序");
    expect(screen.queryByText(/token=secret/)).not.toBeInTheDocument();
  });
});
