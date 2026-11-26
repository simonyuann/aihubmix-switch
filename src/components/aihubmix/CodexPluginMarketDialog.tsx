import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactElement,
} from "react";
import { settingsApi } from "@/lib/api/settings";
import { Search, X } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { CodexMarketProgress } from "@/components/aihubmix/CodexMarketProgress";
import {
  codexMarketError,
  codexPluginMarket,
  type CodexPluginCatalog,
  type CodexMarketPlugin,
  type CodexMarketProgress as MarketProgress,
} from "@/lib/codexPluginMarket";
import { setCodexDesktopPreferences } from "@/lib/codexDesktop";

function pluginMatches(plugin: CodexMarketPlugin, query: string) {
  const text = [
    plugin.displayName,
    plugin.name,
    plugin.description,
    plugin.author,
    ...(plugin.tags ?? []),
  ]
    .filter((part): part is string => typeof part === "string")
    .join(" ")
    .toLocaleLowerCase();
  return text.includes(query);
}

export function CodexPluginMarketDialog({
  children,
}: {
  children: ReactElement;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Dialog open={open} onOpenChange={setOpen}>
      <DialogTrigger asChild>{children}</DialogTrigger>
      {open && <PluginMarketContent />}
    </Dialog>
  );
}

function PluginMarketContent() {
  const searchInput = useRef<HTMLInputElement>(null);
  const [catalog, setCatalog] = useState<CodexPluginCatalog | null>(null);
  const [query, setQuery] = useState("");
  const [visibleCount, setVisibleCount] = useState(80);
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [marketProgress, setMarketProgress] = useState<MarketProgress | null>(
    null,
  );
  const gate = useRef(false);
  const mounted = useRef(true);

  const reload = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const data = await codexPluginMarket.list();
      if (mounted.current) setCatalog(data);
    } catch (cause) {
      if (mounted.current)
        setError(
          codexMarketError(cause, "无法加载插件列表，请检查网络后重试。"),
        );
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    void reload();
    return () => {
      mounted.current = false;
    };
  }, [reload]);

  const matches = useMemo(() => {
    const term = query.trim().toLocaleLowerCase();
    return (catalog?.plugins ?? []).filter(
      (plugin) => !term || pluginMatches(plugin, term),
    );
  }, [catalog, query]);

  async function enableMarket(prepareLocal = false) {
    if (gate.current) return;
    gate.current = true;
    setBusy("enable");
    setMarketProgress(null);
    setError("");
    setNotice("");
    try {
      const setup = await codexPluginMarket.enable((progress) => {
        if (mounted.current) setMarketProgress(progress);
      }, prepareLocal);
      if (!mounted.current) return;
      const saved = setCodexDesktopPreferences({ plugins: true });
      await reload();
      if (mounted.current) {
        if (saved)
          setNotice(
            setup?.officialReady === false
              ? "社区目录已启用。官方插件同步未完成，可点「同步官方插件」重试。"
              : prepareLocal
                ? "本机插件库已就绪。重新打开 Codex 后即可在社区插件页安装。"
                : "插件目录已启用。可在这里按需安装，或从 AIHub 启动桌面端后直接安装。",
          );
        else setError("插件目录已设置，但偏好保存失败；请重新检查设置。");
      }
    } catch (cause) {
      if (mounted.current)
        setError(
          codexMarketError(
            cause,
            "无法启用插件目录，请检查网络和 Codex 安装后重试。",
          ),
        );
    } finally {
      gate.current = false;
      if (mounted.current) setBusy(null);
      if (mounted.current) setMarketProgress(null);
    }
  }

  async function change(plugin: CodexMarketPlugin) {
    if (gate.current || !catalog || plugin.installed === null) return;
    gate.current = true;
    setBusy(plugin.id);
    setError("");
    setNotice("");
    try {
      if (plugin.installed) await codexPluginMarket.uninstall(plugin.id);
      else {
        await codexPluginMarket.install(plugin.id, catalog.revision);
        setCodexDesktopPreferences({ plugins: true });
      }
      if (!mounted.current) return;
      setCatalog(
        (old) =>
          old && {
            ...old,
            plugins: old.plugins.map((item) =>
              item.id === plugin.id
                ? { ...item, installed: !plugin.installed }
                : item,
            ),
          },
      );
      setNotice(
        !plugin.installed && plugin.requiresAuth
          ? "已安装。请在桌面端连接自己的服务账号，再新建会话使用。"
          : plugin.installed
            ? "已卸载。请新建会话或重启 Codex 以更新插件。"
            : "已安装。请新建会话或重启 Codex 以加载插件。",
      );
    } catch (cause) {
      if (mounted.current)
        setError(
          codexMarketError(cause, "操作未能确认，请刷新列表检查状态后再试。"),
        );
    } finally {
      gate.current = false;
      if (mounted.current) setBusy(null);
    }
  }

  return (
    <DialogContent
      className="w-[calc(100vw-2rem)] max-w-2xl overflow-hidden"
      onOpenAutoFocus={(event) => {
        event.preventDefault();
        searchInput.current?.focus();
      }}
      onEscapeKeyDown={(event) => {
        if (gate.current) event.preventDefault();
      }}
    >
      <DialogHeader>
        <div className="flex items-center justify-between gap-3">
          <DialogTitle>Codex 插件</DialogTitle>
          <DialogClose asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label="关闭插件管理"
              title="关闭插件管理"
              disabled={!!busy}
            >
              <X aria-hidden="true" className="h-4 w-4" />
            </Button>
          </DialogClose>
        </div>
        <DialogDescription>
          安装后新建会话或重启 Codex；部分插件需要单独登录。
        </DialogDescription>
        <p className="text-caption text-fg-3">
          社区插件可能读取本机文件或运行代码，安装前请确认来源。
        </p>
      </DialogHeader>
      <div className="min-h-0 space-y-3 overflow-y-auto px-6 pb-6">
        <label className="relative block">
          <Search
            aria-hidden="true"
            className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-3"
          />
          <Input
            ref={searchInput}
            aria-label="搜索插件"
            placeholder="搜索插件名称或用途"
            className="pl-9"
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
              setVisibleCount(80);
            }}
          />
        </label>
        {loading && (
          <p role="status" className="text-body text-fg-3">
            正在加载插件列表…
          </p>
        )}
        {busy === "enable" && <CodexMarketProgress progress={marketProgress} />}
        {error && (
          <div
            role="alert"
            className="flex flex-wrap items-center gap-2 text-caption text-destructive"
          >
            {error}
            <Button
              variant="outline"
              size="compact"
              onClick={() => void reload()}
              disabled={!!busy}
            >
              刷新列表
            </Button>
          </div>
        )}
        {notice && (
          <p role="status" className="text-caption text-fg-2">
            {notice}
          </p>
        )}
        {!loading && catalog && (
          <>
            <p className="text-caption text-fg-3">
              找到 {matches.length} 个插件
              {matches.length > visibleCount
                ? `，已显示 ${visibleCount} 个`
                : ""}
            </p>
            <div className="flex flex-wrap items-center justify-between gap-2 text-caption text-fg-2">
              <span>
                从 AIHub 启动桌面端即可按需安装。也可提前下载完整插件库
                {catalog.downloadBytes > 0 &&
                  `（约 ${Math.ceil(catalog.downloadBytes / 1048576)} MiB）`}
                。
              </span>
              <Button
                variant="outline"
                size="compact"
                disabled={!!busy}
                onClick={() => void enableMarket()}
              >
                同步官方插件
              </Button>
              <Button
                variant="outline"
                size="compact"
                disabled={!!busy}
                onClick={() => void enableMarket(true)}
              >
                准备 Codex 插件库
              </Button>
            </div>
            {catalog.plugins.some((plugin) => plugin.installed === null) && (
              <div className="flex flex-wrap items-center justify-between gap-2 rounded-control bg-subtle px-3 py-2 text-caption text-fg-2">
                <span>插件状态暂不可用；启用目录后可查看和管理。</span>
                <Button
                  variant="outline"
                  size="compact"
                  onClick={() => void enableMarket()}
                  disabled={!!busy}
                >
                  {busy === "enable" ? "正在启用…" : "启用插件目录"}
                </Button>
              </div>
            )}
            {!matches.length && (
              <p className="py-6 text-center text-body text-fg-3">
                没有找到匹配的插件。
              </p>
            )}
            <ul className="divide-y divide-border rounded-panel border border-border">
              {matches.slice(0, visibleCount).map((plugin) => (
                <li
                  key={plugin.id}
                  className="flex items-center gap-3 px-4 py-3"
                >
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-body font-medium text-fg-1">
                      {plugin.displayName || plugin.name}
                    </p>
                    {plugin.description && (
                      <p className="mt-1 line-clamp-2 text-caption text-fg-3">
                        {plugin.description}
                      </p>
                    )}
                    <p className="mt-1 text-caption text-fg-3">
                      {plugin.author ? `${plugin.author} · ` : ""}
                      {plugin.inApiCatalog === false
                        ? "官方入口需登录 ChatGPT"
                        : plugin.requiresAuth
                          ? "可能需要授权"
                          : "授权要求以插件说明为准"}
                    </p>
                  </div>
                  {plugin.officialUrl &&
                    (plugin.inApiCatalog === false || plugin.requiresAuth) && (
                      <Button
                        variant="outline"
                        size="compact"
                        disabled={!!busy}
                        onClick={() =>
                          void settingsApi
                            .openExternal(plugin.officialUrl!)
                            .catch(() =>
                              setError("无法打开 ChatGPT 插件目录。"),
                            )
                        }
                      >
                        {plugin.inApiCatalog === false
                          ? "前往 ChatGPT 连接"
                          : "官方入口"}
                      </Button>
                    )}
                  {(plugin.inApiCatalog !== false || plugin.installed) && (
                    <Button
                      variant="outline"
                      size="compact"
                      disabled={!!busy || plugin.installed === null}
                      onClick={() => void change(plugin)}
                    >
                      {busy === plugin.id
                        ? "处理中…"
                        : plugin.installed === null
                          ? "启用后查看"
                          : plugin.installed
                            ? "卸载"
                            : "安装"}
                    </Button>
                  )}
                </li>
              ))}
            </ul>
            {matches.length > visibleCount && (
              <Button
                variant="outline"
                className="w-full"
                onClick={() => setVisibleCount((count) => count + 80)}
              >
                加载更多插件
              </Button>
            )}
          </>
        )}
      </div>
    </DialogContent>
  );
}
