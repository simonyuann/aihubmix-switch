import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { ArrowLeft, Bell, ChevronRight, Megaphone, X } from "lucide-react";
import { useAccount } from "@/contexts/AccountContext";
import { dispatch, type Announcement } from "@/lib/api/aihubmix";
import { Button } from "@/components/ui/button";
import { useSettingsQuery } from "@/lib/query";
import { SessionMarkdown } from "@/components/sessions/reader/SessionMarkdown";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";

const Context = createContext<{
  unread: number;
  open: () => void;
  latestUnread: Announcement | null;
  openUnread: () => void;
} | null>(null);

export function useAnnouncements() {
  return useContext(Context);
}

export function AnnouncementBanner() {
  const announcements = useAnnouncements();
  if (!announcements?.latestUnread) return null;
  return (
    <button
      type="button"
      onClick={announcements.openUnread}
      className="aihub-page-enter mx-6 mt-3 flex items-center gap-3 rounded-panel bg-subtle px-4 py-3 text-start text-body text-fg-1 transition-colors hover:bg-selected motion-reduce:transition-none"
    >
      <Megaphone
        className="h-4 w-4 shrink-0 text-action-text"
        aria-hidden="true"
      />
      <span className="min-w-0 flex-1 truncate">
        <strong>平台公告：</strong>
        {announcements.latestUnread.title}
      </span>
      {announcements.unread > 1 && (
        <span className="shrink-0 text-caption text-fg-3">
          {announcements.unread} 条未读
        </span>
      )}
      <ChevronRight className="h-4 w-4 shrink-0" aria-hidden="true" />
    </button>
  );
}

export function AnnouncementBell() {
  const announcements = useContext(Context);
  if (!announcements) return null;
  return (
    <button
      type="button"
      aria-label={
        announcements.unread
          ? `平台公告，${announcements.unread} 条未读`
          : "平台公告"
      }
      title="平台公告"
      className="relative flex h-8 w-8 shrink-0 items-center justify-center rounded text-fg-2 hover:bg-subtle hover:text-fg-1"
      style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
      onClick={announcements.open}
    >
      <Bell className="h-4 w-4" aria-hidden="true" />
      {announcements.unread > 0 && (
        <span className="absolute right-1 top-1 h-2 w-2 rounded-full bg-action" />
      )}
    </button>
  );
}

export function AnnouncementProvider({ children }: { children: ReactNode }) {
  const account = useAccount();
  const { data: settings, isError: settingsError } = useSettingsQuery();
  const [items, setItems] = useState<Announcement[]>([]);
  const [open, setOpen] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [error, setError] = useState("");
  const [anotherDialogOpen, setAnotherDialogOpen] = useState(false);
  const seenPopups = useRef(new Set<number>());
  const generation = useRef(0);

  useEffect(() => {
    if (open || !items.some((item) => !item.readAt && item.notifyMode === "popup" && !seenPopups.current.has(item.id))) {
      setAnotherDialogOpen(false);
      return;
    }
    const check = () => setAnotherDialogOpen(!!document.querySelector('[role="dialog"]'));
    check();
    const observer = new MutationObserver(check);
    observer.observe(document.body, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [items, open]);

  const refresh = useCallback(async () => {
    const current = generation.current;
    try {
      const next = await dispatch<Announcement[]>("announcements");
      if (generation.current !== current) return;
      if (!Array.isArray(next)) throw new Error("公告数据格式异常");
      setItems(next);
      setError("");
    } catch (e) {
      if (generation.current === current) setError(String(e));
    }
  }, []);

  useEffect(() => {
    generation.current++;
    seenPopups.current.clear();
    setItems([]);
    setOpen(false);
    setSelectedId(null);
    void refresh();
    const timer = window.setInterval(() => void refresh(), 60_000);
    return () => {
      generation.current++;
      window.clearInterval(timer);
    };
  }, [account.state?.account.userId, refresh]);

  const markRead = useCallback((item: Announcement) => {
    if (!item.readAt) {
      const current = generation.current;
      void dispatch("announcement-read", { id: item.id })
        .then(() => {
          if (generation.current !== current) return;
          setItems((previous) =>
            previous.map((entry) =>
              entry.id === item.id
                ? { ...entry, readAt: new Date().toISOString() }
                : entry,
            ),
          );
        })
        .catch((e) => {
          if (generation.current === current) setError(String(e));
        });
    }
  }, []);

  const view = useCallback(
    (item: Announcement, mark = true) => {
      setSelectedId(item.id);
      setOpen(true);
      if (mark) markRead(item);
    },
    [markRead],
  );

  useEffect(() => {
    // First-run UI has a higher layer and its own focus trap. Wait until it
    // has been dismissed and any other open dialog has actually unmounted.
    if (open || (!settings && !settingsError) || (settings && settings.firstRunNoticeConfirmed !== true) || anotherDialogOpen || document.querySelector('[role="dialog"]')) return;
    const popup = items.find(
      (item) =>
        !item.readAt &&
        item.notifyMode === "popup" &&
        !seenPopups.current.has(item.id),
    );
    if (popup) {
      seenPopups.current.add(popup.id);
      view(popup, false);
    }
  }, [items, open, view, settings, settingsError, anotherDialogOpen]);

  const selected = items.find((item) => item.id === selectedId);
  const close = () => {
    if (selected) markRead(selected);
    setOpen(false);
  };
  const unread = items.filter((item) => !item.readAt).length;
  const latestUnread =
    items
      .filter((item) => !item.readAt)
      .sort((a, b) =>
        (b.createdAt ?? "").localeCompare(a.createdAt ?? ""),
      )[0] ?? null;
  return (
    <Context.Provider
      value={{
        unread,
        latestUnread,
        openUnread: () => {
          if (latestUnread) view(latestUnread, false);
        },
        open: () => {
          setSelectedId(null);
          setOpen(true);
          void refresh();
        },
      }}
    >
      {children}
      <Dialog
        open={open}
        onOpenChange={(next) => (next ? setOpen(true) : close())}
      >
        <DialogContent className="max-w-lg">
          <DialogHeader>
            <Button variant="ghost" size="icon-compact" aria-label="关闭公告" className="absolute right-4 top-4" onClick={close}>
              <X className="h-4 w-4" />
            </Button>
            <DialogTitle className="flex items-center gap-2 pr-10">
              {selected && (
                <Button
                  variant="ghost"
                  size="icon-compact"
                  aria-label="返回公告列表"
                  onClick={() => {
                    if (selected) markRead(selected);
                    setSelectedId(null);
                  }}
                >
                  <ArrowLeft className="h-4 w-4" />
                </Button>
              )}
              {selected ? selected.title : "平台公告"}
            </DialogTitle>
            <DialogDescription>
              {selected ? "平台公告" : "查看平台最新消息"}
            </DialogDescription>
          </DialogHeader>
          <div className="min-h-24 min-w-0 flex-1 overflow-y-auto px-6 py-2">
            {error && (
              <p role="alert" className="mb-3 break-words text-destructive">
                {error}
              </p>
            )}
            {selected ? (
              <SessionMarkdown content={selected.content} autoLoadRemoteImages className="whitespace-pre-line text-body leading-6" />
            ) : items.length ? (
              <div className="divide-y divide-border">
                {items.map((item) => (
                  <button
                    key={item.id}
                    type="button"
                    onClick={() => view(item)}
                    className="flex w-full items-start gap-3 py-3 text-start hover:text-action-text"
                  >
                    <span
                      className={`mt-1.5 h-2 w-2 shrink-0 rounded-full ${item.readAt ? "bg-transparent" : "bg-action"}`}
                    />
                    <span className="min-w-0 flex-1 break-words font-medium">
                      {item.title}
                    </span>
                    <span className="shrink-0 text-caption text-fg-3">
                      {item.createdAt ? item.createdAt.slice(0, 10) : ""}
                    </span>
                  </button>
                ))}
              </div>
            ) : (
              <p className="py-4 text-fg-3">暂无公告</p>
            )}
          </div>
          <DialogFooter>
            <Button onClick={close}>关闭</Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </Context.Provider>
  );
}
