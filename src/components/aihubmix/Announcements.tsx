import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { ArrowLeft, ChevronRight, Megaphone } from "lucide-react";
import { useAccount } from "@/contexts/AccountContext";
import { dispatch, type Announcement } from "@/lib/api/aihubmix";
import { SessionMarkdown } from "@/components/sessions/reader/SessionMarkdown";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { Button } from "@/components/ui/button";
import { friendlyError } from "./friendlyError";

const Context = createContext<{
  items: Announcement[];
  error: string;
  loading: boolean;
  refresh: () => Promise<void>;
  markRead: (item: Announcement) => void;
} | null>(null);

export function AnnouncementBanner({ onOpen }: { onOpen: () => void }) {
  const unread = useContext(Context)?.items.filter((item) => !item.readAt);
  if (!unread?.length) return null;
  return (
    <button
      type="button"
      onClick={onOpen}
      className="aihub-page-enter mx-6 mt-3 flex items-center gap-3 rounded-panel bg-subtle px-4 py-3 text-start text-body text-fg-1 transition-colors hover:bg-selected motion-reduce:transition-none"
    >
      <Megaphone
        className="h-4 w-4 shrink-0 text-action-text"
        aria-hidden="true"
      />
      <span className="min-w-0 flex-1 truncate">
        <strong>新公告</strong>
        <span className="ms-2">{unread[0].title}</span>
      </span>
      {unread.length > 1 && (
        <span className="shrink-0 text-caption text-fg-3">
          {unread.length} 条未读
        </span>
      )}
      <ChevronRight className="h-4 w-4 shrink-0" aria-hidden="true" />
    </button>
  );
}

export function AnnouncementProvider({ children }: { children: ReactNode }) {
  const { state } = useAccount();
  const [items, setItems] = useState<Announcement[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const generation = useRef(0);
  const latestRefresh = useRef(0);
  const pendingReads = useRef(new Set<number>());

  const refresh = useCallback(async () => {
    const current = generation.current;
    const request = ++latestRefresh.current;
    const isCurrent = () =>
      generation.current === current && latestRefresh.current === request;
    try {
      const next = await dispatch<Announcement[]>("announcements");
      if (!isCurrent()) return;
      if (!Array.isArray(next)) throw new Error("公告数据格式异常");
      // An earlier read acknowledgement must survive a stale server response.
      setItems((previous) => {
        const readAt = new Map(previous.map((item) => [item.id, item.readAt]));
        return next
          .map((item) => ({
            ...item,
            readAt: item.readAt ?? readAt.get(item.id) ?? null,
          }))
          .sort((a, b) => (b.createdAt ?? "").localeCompare(a.createdAt ?? ""));
      });
      setError("");
    } catch (e) {
      if (isCurrent()) setError(friendlyError(e, "公告加载失败，请重试。"));
    } finally {
      if (isCurrent()) setLoading(false);
    }
  }, []);

  useEffect(() => {
    generation.current++;
    pendingReads.current = new Set();
    setItems([]);
    setLoading(true);
    setError("");
    void refresh();
    const timer = window.setInterval(() => void refresh(), 60_000);
    return () => {
      generation.current++;
      window.clearInterval(timer);
    };
  }, [state?.account.userId, state?.relayBaseUrl, refresh]);

  const markRead = useCallback((item: Announcement) => {
    const pending = pendingReads.current;
    if (item.readAt || pending.has(item.id)) return;
    pending.add(item.id);
    const current = generation.current;
    void dispatch("announcement-read", { id: item.id })
      .then(() => {
        if (generation.current !== current) return;
        const readAt = new Date().toISOString();
        setItems((previous) =>
          previous.map((entry) =>
            entry.id === item.id ? { ...entry, readAt } : entry,
          ),
        );
      })
      .catch((e) => {
        if (generation.current === current)
          setError(friendlyError(e, "公告状态保存失败，请重试。"));
      })
      .finally(() => pending.delete(item.id));
  }, []);

  return (
    <Context.Provider value={{ items, error, loading, refresh, markRead }}>
      {children}
    </Context.Provider>
  );
}

export function AnnouncementPage() {
  const announcements = useContext(Context);
  const refresh = announcements?.refresh;
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const selected = announcements?.items.find((item) => item.id === selectedId);
  const scrollArea = useRef<HTMLElement>(null);
  const articleTitle = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    void refresh?.();
  }, [refresh]);

  useEffect(() => {
    scrollArea.current?.scrollTo?.(0, 0);
    if (selectedId != null) articleTitle.current?.focus();
  }, [selectedId]);

  return (
    <>
      <AppPageHeader
        title="公告"
        icon={<Megaphone className="h-5 w-5" />}
        actions={
          selected && (
            <Button variant="outline" onClick={() => setSelectedId(null)}>
              <ArrowLeft className="h-4 w-4" aria-hidden="true" />
              返回公告列表
            </Button>
          )
        }
      />
      <main
        id="main-content"
        ref={scrollArea}
        className="min-h-0 flex-1 overflow-y-auto scroll-stable px-6 py-5"
      >
        <div className="max-w-3xl space-y-4">
          {announcements?.error && (
            <div className="flex flex-wrap items-center gap-3">
              <p role="alert" className="text-body text-destructive">
                {announcements.error}
              </p>
              <Button
                variant="outline"
                size="compact"
                onClick={() => void refresh?.()}
              >
                刷新公告
              </Button>
            </div>
          )}
          {selected ? (
            <article className="min-w-0 rounded-panel border border-border p-6">
              <h2
                ref={articleTitle}
                tabIndex={-1}
                className="mb-2 break-words text-title font-semibold"
              >
                {selected.title}
              </h2>
              {selected.createdAt && (
                <time
                  dateTime={selected.createdAt}
                  className="text-caption text-fg-3"
                >
                  {selected.createdAt.slice(0, 10)}
                </time>
              )}
              <SessionMarkdown
                content={selected.content}
                autoLoadRemoteImages
                className="mt-5 whitespace-pre-line break-words text-body leading-6"
              />
            </article>
          ) : announcements?.items.length ? (
            <ul className="divide-y divide-border overflow-hidden rounded-panel border border-border">
              {announcements.items.map((item) => (
                <li key={item.id}>
                  <button
                    type="button"
                    onClick={() => {
                      setSelectedId(item.id);
                      announcements.markRead(item);
                    }}
                    className="flex w-full items-center gap-3 px-4 py-3.5 text-start hover:bg-subtle focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring"
                  >
                    <span
                      aria-hidden="true"
                      className={`h-2 w-2 shrink-0 rounded-full ${item.readAt ? "bg-transparent" : "bg-action"}`}
                    />
                    <span className="min-w-0 flex-1 break-words font-medium">
                      {item.title}
                      {!item.readAt && (
                        <span className="sr-only">（未读）</span>
                      )}
                    </span>
                    {item.createdAt && (
                      <time
                        dateTime={item.createdAt}
                        className="shrink-0 text-caption text-fg-3"
                      >
                        {item.createdAt.slice(0, 10)}
                      </time>
                    )}
                    <ChevronRight
                      className="h-4 w-4 shrink-0 text-fg-3"
                      aria-hidden="true"
                    />
                  </button>
                </li>
              ))}
            </ul>
          ) : announcements?.loading ? (
            <p role="status" className="py-4 text-body text-fg-3">
              正在加载公告…
            </p>
          ) : (
            !announcements?.error && (
              <p className="py-4 text-body text-fg-3">暂无公告</p>
            )
          )}
        </div>
      </main>
    </>
  );
}
