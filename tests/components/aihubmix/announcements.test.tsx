import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useState } from "react";
import { Dialog, DialogContent, DialogDescription, DialogTitle } from "@/components/ui/dialog";

const mocks = vi.hoisted(() => ({ dispatch: vi.fn(), settings: { data: { firstRunNoticeConfirmed: true }, isError: false } }));
vi.mock("@/lib/api/aihubmix", () => ({ dispatch: mocks.dispatch }));
vi.mock("@/lib/query", () => ({ useSettingsQuery: () => mocks.settings }));
vi.mock("@/contexts/AccountContext", () => ({
  useAccount: () => ({ state: { account: { userId: 1, loggedIn: true } } }),
}));
import {
  AnnouncementBell,
  AnnouncementBanner,
  AnnouncementProvider,
} from "@/components/aihubmix/Announcements";

const notice = {
  id: 2,
  title: "维护通知",
  content: "今晚服务维护",
  notifyMode: "popup",
  readAt: null,
  createdAt: "2026-11-04T12:00:00Z",
};

beforeEach(() => {
  vi.clearAllMocks();
  mocks.settings = { data: { firstRunNoticeConfirmed: true }, isError: false };
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("platform announcements", () => {
  it("keeps a confirmed read state when an older refresh finishes later", async () => {
    let finishRefresh!: (items: unknown) => void;
    let loads = 0;
    mocks.dispatch.mockImplementation((command: string) => {
      if (command !== "announcements") return Promise.resolve(null);
      if (++loads === 1)
        return Promise.resolve([{ ...notice, notifyMode: "silent" }]);
      return new Promise((resolve) => {
        finishRefresh = resolve;
      });
    });
    render(
      <AnnouncementProvider>
        <AnnouncementBell />
        <AnnouncementBanner />
      </AnnouncementProvider>,
    );
    fireEvent.click(
      await screen.findByRole("button", { name: "平台公告，1 条未读" }),
    );
    fireEvent.click(screen.getByRole("button", { name: /维护通知/ }));
    await waitFor(() =>
      expect(screen.getByRole("button", {
        name: "平台公告", hidden: true,
      })).toBeInTheDocument(),
    );
    await act(async () => {
      finishRefresh([{ ...notice, notifyMode: "silent" }]);
    });
    expect(screen.queryByRole("button", {
      name: /1 条未读/, hidden: true,
    })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", {
      name: /平台公告：/, hidden: true,
    })).not.toBeInTheDocument();
  });

  it("does not reopen a manually viewed popup while its read request is pending", async () => {
    const interval = vi.spyOn(window, "setInterval");
    const second = { ...notice, id: 3, title: "新版说明", content: "新功能介绍" };
    mocks.dispatch.mockImplementation(
      (command: string, payload?: { id: number }) => {
        if (command === "announcements") return Promise.resolve([notice, second]);
        if (payload?.id === second.id) return new Promise(() => {});
        return Promise.resolve(null);
      },
    );
    render(
      <AnnouncementProvider><AnnouncementBell /></AnnouncementProvider>,
    );
    expect(await screen.findByText(notice.content)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "返回公告列表" }));
    fireEvent.click(screen.getByRole("button", { name: /新版说明/ }));
    expect(screen.getByText(second.content)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "关闭公告" }));
    await waitFor(() =>
      expect(screen.queryByRole("dialog")).not.toBeInTheDocument(),
    );
    expect(interval).toHaveBeenCalledWith(expect.any(Function), 60_000);
    const refresh = interval.mock.calls[0][0] as () => void;
    await act(async () => { refresh(); });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("waits for the first-run dialog to close before opening the announcement", async () => {
    mocks.dispatch.mockImplementation((command: string) => Promise.resolve(command === "announcements" ? [notice] : null));
    function FirstRun() {
      const [pending, setPending] = useState(true);
      mocks.settings = { data: { firstRunNoticeConfirmed: !pending }, isError: false };
      return <AnnouncementProvider>
        <Dialog open={pending}>
          <DialogContent zIndex="top"><DialogTitle>欢迎使用</DialogTitle><DialogDescription>首次使用说明</DialogDescription><button onClick={() => setPending(false)}>知道了</button></DialogContent>
        </Dialog>
      </AnnouncementProvider>;
    }
    render(<FirstRun />);
    expect(await screen.findByRole("dialog", { name: "欢迎使用" })).toBeInTheDocument();
    await waitFor(() => expect(mocks.dispatch).toHaveBeenCalledWith("announcements"));
    expect(screen.queryByText("今晚服务维护")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "知道了" }));
    expect(await screen.findByText("今晚服务维护")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "关闭公告" }));
    await waitFor(() => expect(screen.queryByText("今晚服务维护")).not.toBeInTheDocument());
  });

  it("waits until another dialog has closed, then shows the popup", async () => {
    mocks.dispatch.mockImplementation((command: string) => Promise.resolve(command === "announcements" ? [notice] : null));
    function ExistingDialog() {
      const [open, setOpen] = useState(true);
      return <AnnouncementProvider><Dialog open={open}>
        <DialogContent><DialogTitle>其他提示</DialogTitle><DialogDescription>请先处理</DialogDescription><button onClick={() => setOpen(false)}>完成</button></DialogContent>
      </Dialog></AnnouncementProvider>;
    }
    render(<ExistingDialog />);
    await waitFor(() => expect(mocks.dispatch).toHaveBeenCalledWith("announcements"));
    expect(screen.queryByText("今晚服务维护")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "完成" }));
    expect(await screen.findByText("今晚服务维护")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    await waitFor(() => expect(screen.queryByText("今晚服务维护")).not.toBeInTheDocument());
  });

  it("renders markdown safely in the announcement popup", async () => {
    const content = "## 使用指南\n\n**重要**：请查看 [帮助](https://example.com/help)。\n\n- 第一步\n- 第二步\n\n`code`\n\n![示意图](https://example.com/image.png)\n\n<img src=x onerror=alert(1)>\n\n[危险](javascript:alert(1))";
    mocks.dispatch.mockImplementation((command: string) => Promise.resolve(command === "announcements" ? [{ ...notice, content }] : null));
    render(<AnnouncementProvider><AnnouncementBell /></AnnouncementProvider>);
    expect(await screen.findByRole("heading", { name: "使用指南" })).toBeInTheDocument();
    expect(screen.getByText("重要").tagName).toBe("STRONG");
    expect(screen.getByRole("link", { name: "帮助" })).toHaveAttribute("href", "https://example.com/help");
    expect(screen.getByRole("list")).toHaveTextContent("第一步");
    expect(screen.getByText("code").tagName).toBe("CODE");
    expect(screen.getByRole("img", { name: "示意图" })).toHaveAttribute("src", "https://example.com/image.png");
    expect(document.querySelector('img[src="x"]')).toBeNull();
    expect(document.querySelector('a[href^="javascript:"]')).toBeNull();
  });
  it("shows only unread announcements on the home banner and retains history after reading", async () => {
    mocks.dispatch.mockImplementation((command: string) =>
      Promise.resolve(
        command === "announcements"
          ? [{ ...notice, notifyMode: "silent" }]
          : null,
      ),
    );
    render(
      <AnnouncementProvider>
        <AnnouncementBanner />
        <AnnouncementBell />
      </AnnouncementProvider>,
    );
    fireEvent.click(
      await screen.findByRole("button", { name: /平台公告：\s*维护通知/ }),
    );
    expect(await screen.findByText("今晚服务维护")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: /平台公告：\s*维护通知/ }),
      ).not.toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "平台公告" }));
    expect(await screen.findByText("维护通知")).toBeInTheDocument();
  });
  it("shows an unread popup and marks it read without adding a backend entry", async () => {
    mocks.dispatch.mockImplementation((command: string) =>
      Promise.resolve(command === "announcements" ? [notice] : null),
    );
    render(
      <AnnouncementProvider>
        <AnnouncementBell />
      </AnnouncementProvider>,
    );
    expect(await screen.findByText("今晚服务维护")).toBeInTheDocument();
    expect(mocks.dispatch).not.toHaveBeenCalledWith("announcement-read", {
      id: 2,
    });
    expect(
      screen.queryByRole("button", { name: "管理公告" }),
    ).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("announcement-read", {
        id: 2,
      }),
    );
    await waitFor(() =>
      expect(screen.queryByText("今晚服务维护")).not.toBeInTheDocument(),
    );
    fireEvent.click(screen.getByRole("button", { name: "平台公告" }));
    expect(await screen.findByText("维护通知")).toBeInTheDocument();
  });

  it("keeps silent announcements in the list and shows an unread indicator", async () => {
    mocks.dispatch.mockResolvedValue([{ ...notice, notifyMode: "silent" }]);
    render(
      <AnnouncementProvider>
        <AnnouncementBell />
      </AnnouncementProvider>,
    );
    const bell = await screen.findByRole("button", {
      name: "平台公告，1 条未读",
    });
    expect(screen.queryByText("今晚服务维护")).not.toBeInTheDocument();
    fireEvent.click(bell);
    fireEvent.click(await screen.findByRole("button", { name: /维护通知/ }));
    expect(screen.getByText("今晚服务维护")).toBeInTheDocument();
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("announcement-read", {
        id: 2,
      }),
    );
  });
});
