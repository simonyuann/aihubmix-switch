import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const mocks = vi.hoisted(() => ({ dispatch: vi.fn() }));
vi.mock("@/lib/api/aihubmix", () => ({ dispatch: mocks.dispatch }));
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

beforeEach(() => vi.clearAllMocks());
afterEach(cleanup);

describe("platform announcements", () => {
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
