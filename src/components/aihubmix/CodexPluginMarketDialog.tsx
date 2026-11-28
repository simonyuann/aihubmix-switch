import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactElement,
} from "react";
import { useTranslation } from "react-i18next";
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
  const { t } = useTranslation();
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
        setError(codexMarketError(cause, t("pluginMarket.error.loadFailed")));
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
              ? t("pluginMarket.notice.officialSyncIncomplete")
              : prepareLocal
                ? t("pluginMarket.notice.localReady")
                : t("pluginMarket.notice.enabled"),
          );
        else setError(t("pluginMarket.error.prefSaveFailed"));
      }
    } catch (cause) {
      if (mounted.current)
        setError(codexMarketError(cause, t("pluginMarket.error.enableFailed")));
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
          ? t("pluginMarket.notice.installedConnectAuth")
          : plugin.installed
            ? t("pluginMarket.notice.uninstalled")
            : t("pluginMarket.notice.installed"),
      );
    } catch (cause) {
      if (mounted.current)
        setError(codexMarketError(cause, t("pluginMarket.error.unconfirmed")));
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
          <DialogTitle>{t("pluginMarket.title")}</DialogTitle>
          <DialogClose asChild>
            <Button
              type="button"
              variant="ghost"
              size="icon"
              aria-label={t("pluginMarket.aria.close")}
              title={t("pluginMarket.aria.close")}
              disabled={!!busy}
            >
              <X aria-hidden="true" className="h-4 w-4" />
            </Button>
          </DialogClose>
        </div>
        <DialogDescription>{t("pluginMarket.description")}</DialogDescription>
        <p className="text-caption text-fg-3">{t("pluginMarket.warning")}</p>
      </DialogHeader>
      <div className="min-h-0 space-y-3 overflow-y-auto px-6 pb-6">
        <label className="relative block">
          <Search
            aria-hidden="true"
            className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-3"
          />
          <Input
            ref={searchInput}
            aria-label={t("pluginMarket.aria.search")}
            placeholder={t("pluginMarket.placeholder.search")}
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
            {t("pluginMarket.status.loading")}
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
              {t("pluginMarket.action.refresh")}
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
              {t("pluginMarket.status.found", { count: matches.length })}
              {matches.length > visibleCount
                ? t("pluginMarket.status.shownMore", { count: visibleCount })
                : ""}
            </p>
            <div className="flex flex-wrap items-center justify-between gap-2 text-caption text-fg-2">
              <span>
                {t("pluginMarket.hint.download", {
                  sizeInfo:
                    catalog.downloadBytes > 0
                      ? `（约 ${Math.ceil(catalog.downloadBytes / 1048576)} MiB）`
                      : "",
                })}
              </span>
              <Button
                variant="outline"
                size="compact"
                disabled={!!busy}
                onClick={() => void enableMarket()}
              >
                {t("pluginMarket.action.syncOfficial")}
              </Button>
              <Button
                variant="outline"
                size="compact"
                disabled={!!busy}
                onClick={() => void enableMarket(true)}
              >
                {t("pluginMarket.action.prepareLibrary")}
              </Button>
            </div>
            {catalog.plugins.some((plugin) => plugin.installed === null) && (
              <div className="flex flex-wrap items-center justify-between gap-2 rounded-control bg-subtle px-3 py-2 text-caption text-fg-2">
                <span>{t("pluginMarket.label.statusUnavailable")}</span>
                <Button
                  variant="outline"
                  size="compact"
                  onClick={() => void enableMarket()}
                  disabled={!!busy}
                >
                  {busy === "enable"
                    ? t("pluginMarket.status.enabling")
                    : t("pluginMarket.action.enableCatalog")}
                </Button>
              </div>
            )}
            {!matches.length && (
              <p className="py-6 text-center text-body text-fg-3">
                {t("pluginMarket.empty.noMatches")}
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
                        ? t("pluginMarket.auth.loginRequired")
                        : plugin.requiresAuth
                          ? t("pluginMarket.auth.mayRequire")
                          : t("pluginMarket.auth.seeDescription")}
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
                              setError(
                                t("pluginMarket.error.openCatalogFailed"),
                              ),
                            )
                        }
                      >
                        {plugin.inApiCatalog === false
                          ? t("pluginMarket.action.gotoChatgpt")
                          : t("pluginMarket.action.officialEntry")}
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
                        ? t("pluginMarket.status.working")
                        : plugin.installed === null
                          ? t("pluginMarket.action.viewAfterEnable")
                          : plugin.installed
                            ? t("pluginMarket.action.uninstall")
                            : t("pluginMarket.action.install")}
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
                {t("pluginMarket.action.loadMore")}
              </Button>
            )}
          </>
        )}
      </div>
    </DialogContent>
  );
}
