import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useState } from "react";
import {
  AnnouncementBanner,
  AnnouncementPage,
  AnnouncementProvider,
} from "@/components/aihubmix/Announcements";

const mocks = vi.hoisted(() => ({ dispatch: vi.fn() }));
vi.mock("@/lib/api/aihubmix", () => ({ dispatch: mocks.dispatch }));
vi.mock("@/contexts/AccountContext", () => ({
  useAccount: () => ({ state: { account: { userId: 1, loggedIn: true } } }),
}));

const notice = {
  id: 2,
  title: "维护通知",
  content: "今晚服务维护",
  notifyMode: "popup",
  readAt: null,
  createdAt: "2026-11-04T12:00:00Z",
};
const older = {
  ...notice,
  id: 1,
  title: "版本说明",
  content: "已有功能说明",
  notifyMode: "silent",
  readAt: "2026-11-03T12:10:00Z",
  createdAt: "2026-11-03T12:00:00Z",
};

function HomeAndAnnouncements() {
  const [showPage, setShowPage] = useState(false);
  return (
    <AnnouncementProvider>
      <button onClick={() => setShowPage(false)}>首页</button>
      <button onClick={() => setShowPage(true)}>公告</button>
      {showPage ? (
        <AnnouncementPage />
      ) : (
        <AnnouncementBanner onOpen={() => setShowPage(true)} />
      )}
    </AnnouncementProvider>
  );
}

beforeEach(() => {
  mocks.dispatch.mockReset();
  mocks.dispatch.mockImplementation((command: string) =>
    Promise.resolve(command === "announcements" ? [older, notice] : null),
  );
});
afterEach(() => vi.restoreAllMocks());

describe("platform announcements", () => {
  it("uses the home reminder to open a list and marks only the article the user opens", async () => {
    render(<HomeAndAnnouncements />);
    const reminder = await screen.findByRole("button", {
      name: /announcements\.banner\.new.*维护通知/,
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByText(notice.content)).not.toBeInTheDocument();
    fireEvent.click(reminder);

    const rows = screen.getAllByRole("listitem");
    expect(rows[0]).toHaveTextContent(notice.title);
    expect(rows[1]).toHaveTextContent(older.title);
    expect(screen.queryByText(notice.content)).not.toBeInTheDocument();
    expect(mocks.dispatch).not.toHaveBeenCalledWith(
      "announcement-read",
      expect.anything(),
    );

    fireEvent.click(
      screen.getByRole("button", {
        name: /维护通知.*announcements\.aria\.unread/,
      }),
    );
    expect(screen.getByText(notice.content)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: notice.title })).toHaveFocus();
    await waitFor(() =>
      expect(
        mocks.dispatch.mock.calls.filter(
          ([command]) => command === "announcement-read",
        ),
      ).toEqual([["announcement-read", { id: notice.id }]]),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "announcements.action.backToList" }),
    );
    expect(
      screen.getByRole("button", { name: /版本说明/ }),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "首页" }));
    await waitFor(() =>
      expect(
        screen.queryByRole("button", { name: /announcements\.banner\.new/ }),
      ).not.toBeInTheDocument(),
    );
  });

  it("updates the reminder when polling discovers a new popup-mode announcement", async () => {
    const interval = vi.spyOn(window, "setInterval");
    mocks.dispatch.mockResolvedValue([older]);
    render(<HomeAndAnnouncements />);
    await waitFor(() =>
      expect(mocks.dispatch).toHaveBeenCalledWith("announcements"),
    );
    expect(
      screen.queryByRole("button", { name: /announcements\.banner\.new/ }),
    ).not.toBeInTheDocument();
    mocks.dispatch.mockResolvedValue([older, notice]);
    const poll = interval.mock.calls.find(
      ([, delay]) => delay === 60_000,
    )?.[0] as () => void;
    await act(async () => poll());
    expect(
      screen.getByRole("button", {
        name: /announcements\.banner\.new.*维护通知/,
      }),
    ).toBeInTheDocument();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByText(notice.content)).not.toBeInTheDocument();
  });

  it("retains a confirmed read when a stale refresh finishes later", async () => {
    render(<HomeAndAnnouncements />);
    await screen.findByRole("button", { name: /announcements\.banner\.new/ });
    let finishRefresh!: (items: unknown) => void;
    mocks.dispatch.mockImplementation((command: string) =>
      command === "announcements"
        ? new Promise((resolve) => {
            finishRefresh = resolve;
          })
        : Promise.resolve(null),
    );
    fireEvent.click(screen.getByRole("button", { name: "公告" }));
    fireEvent.click(
      screen.getByRole("button", {
        name: /维护通知.*announcements\.aria\.unread/,
      }),
    );
    fireEvent.click(
      screen.getByRole("button", { name: "announcements.action.backToList" }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("button", {
          name: /维护通知.*announcements\.aria\.unread/,
        }),
      ).not.toBeInTheDocument(),
    );
    await act(async () => finishRefresh([notice]));
    expect(
      screen.getByRole("button", { name: /维护通知/ }),
    ).toBeInTheDocument();
    expect(
      screen.queryByRole("button", {
        name: /维护通知.*announcements\.aria\.unread/,
      }),
    ).not.toBeInTheDocument();
  });

  it("does not send another read request when the user reopens an article while marking it read", async () => {
    mocks.dispatch.mockImplementation((command: string) =>
      command === "announcements"
        ? Promise.resolve([notice])
        : new Promise(() => {}),
    );
    render(<HomeAndAnnouncements />);
    fireEvent.click(screen.getByRole("button", { name: "公告" }));
    fireEvent.click(await screen.findByRole("button", { name: /维护通知/ }));
    fireEvent.click(
      screen.getByRole("button", { name: "announcements.action.backToList" }),
    );
    fireEvent.click(screen.getByRole("button", { name: /维护通知/ }));
    expect(
      mocks.dispatch.mock.calls.filter(
        ([command]) => command === "announcement-read",
      ),
    ).toEqual([["announcement-read", { id: notice.id }]]);
  });

  it("renders markdown safely after opening the announcement", async () => {
    const content =
      "## 使用指南\n\n**重要**：请查看 [帮助](https://example.com/help)。\n\n- 第一步\n- 第二步\n\n`code`\n\n![示意图](https://example.com/image.png)\n\n<img src=x onerror=alert(1)>\n\n[危险](javascript:alert(1))";
    mocks.dispatch.mockImplementation((command: string) =>
      Promise.resolve(
        command === "announcements" ? [{ ...notice, content }] : null,
      ),
    );
    render(<HomeAndAnnouncements />);
    fireEvent.click(screen.getByRole("button", { name: "公告" }));
    fireEvent.click(await screen.findByRole("button", { name: /维护通知/ }));
    expect(
      screen.getByRole("heading", { name: "使用指南" }),
    ).toBeInTheDocument();
    expect(screen.getByText("重要").tagName).toBe("STRONG");
    expect(screen.getByRole("link", { name: "帮助" })).toHaveAttribute(
      "href",
      "https://example.com/help",
    );
    expect(screen.getByRole("list")).toHaveTextContent("第一步");
    expect(screen.getByText("code").tagName).toBe("CODE");
    expect(screen.getByRole("img", { name: "示意图" })).toHaveAttribute(
      "src",
      "https://example.com/image.png",
    );
    expect(document.querySelector('img[src="x"]')).toBeNull();
    expect(document.querySelector('a[href^="javascript:"]')).toBeNull();
  });
});
