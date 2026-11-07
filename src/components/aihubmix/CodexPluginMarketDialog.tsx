import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Search } from "lucide-react";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { codexMarketError, codexPluginMarket, type CodexPluginCatalog, type CodexMarketPlugin } from "@/lib/codexPluginMarket";
import { setCodexDesktopPreferences } from "@/lib/codexDesktop";

function pluginMatches(plugin: CodexMarketPlugin, query: string) {
  const text = [plugin.displayName, plugin.name, plugin.description, plugin.author, ...(plugin.tags ?? [])]
    .filter((part): part is string => typeof part === "string")
    .join(" ")
    .toLocaleLowerCase();
  return text.includes(query);
}

export function CodexPluginMarketDialog({ onClose }: { onClose: () => void }) {
  const [catalog, setCatalog] = useState<CodexPluginCatalog | null>(null);
  const [query, setQuery] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const gate = useRef(false);
  const mounted = useRef(true);

  const reload = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const data = await codexPluginMarket.list();
      if (mounted.current) setCatalog(data);
    } catch (cause) {
      if (mounted.current) setError(codexMarketError(cause, "无法加载插件列表，请检查网络后重试。"));
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    void reload();
    return () => { mounted.current = false; };
  }, [reload]);

  const matches = useMemo(() => {
    const term = query.trim().toLocaleLowerCase();
    return (catalog?.plugins ?? []).filter((plugin) => !term || pluginMatches(plugin, term));
  }, [catalog, query]);

  async function enableMarket() {
    if (gate.current) return;
    gate.current = true;
    setBusy("enable");
    setError("");
    setNotice("");
    try {
      await codexPluginMarket.enable();
      if (!mounted.current) return;
      const saved = setCodexDesktopPreferences({ plugins: true });
      await reload();
      if (mounted.current) {
        if (saved) setNotice("插件目录已设置，请新建会话或重启 Codex。");
        else setError("插件目录已设置，但偏好保存失败；请重新检查设置。");
      }
    } catch (cause) {
      if (mounted.current) setError(codexMarketError(cause, "无法启用插件目录，请检查网络和 Codex 安装后重试。"));
    } finally {
      gate.current = false;
      if (mounted.current) setBusy(null);
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
      setCatalog((old) => old && ({
        ...old,
        plugins: old.plugins.map((item) => item.id === plugin.id ? { ...item, installed: !plugin.installed } : item),
      }));
      setNotice(plugin.installed ? "已卸载。请新建会话或重启 Codex 以更新插件。" : "已安装。请新建会话或重启 Codex 以加载插件。");
    } catch (cause) {
      if (mounted.current) setError(codexMarketError(cause, "操作未能确认，请刷新列表检查状态后再试。"));
    } finally {
      gate.current = false;
      if (mounted.current) setBusy(null);
    }
  }

  return (
    <Dialog open onOpenChange={(open) => { if (!open && !busy) onClose(); }}>
      <DialogContent className="w-[calc(100vw-2rem)] max-w-2xl overflow-hidden">
        <DialogHeader>
          <DialogTitle>Codex 插件</DialogTitle>
          <DialogDescription>
            来自社区公开插件缓存。插件连接第三方服务时可能需要单独登录或授权；安装后请新建会话。
          </DialogDescription>
          <p className="text-caption text-fg-3">社区插件可能读取本机文件或运行代码，安装前请确认来源。</p>
        </DialogHeader>
        <div className="min-h-0 space-y-3 overflow-y-auto px-6 pb-6">
          <label className="relative block">
            <Search aria-hidden="true" className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-fg-3" />
            <Input aria-label="搜索插件" placeholder="搜索插件名称或用途" className="pl-9" value={query} onChange={(event) => setQuery(event.target.value)} />
          </label>
          {loading && <p role="status" className="text-body text-fg-3">正在加载插件列表…</p>}
          {error && <div role="alert" className="flex flex-wrap items-center gap-2 text-caption text-destructive">{error}<Button variant="outline" size="compact" onClick={() => void reload()} disabled={!!busy}>刷新列表</Button></div>}
          {notice && <p role="status" className="text-caption text-fg-2">{notice}</p>}
          {!loading && catalog && (
            <>
              <p className="text-caption text-fg-3">找到 {matches.length} 个插件{matches.length > 80 ? "，请搜索以缩小范围" : ""}</p>
              {catalog.plugins.some((plugin) => plugin.installed === null) && (
                <div className="flex flex-wrap items-center justify-between gap-2 rounded-control bg-subtle px-3 py-2 text-caption text-fg-2">
                  <span>插件状态暂不可用；启用目录后可查看和管理。</span>
                  <Button variant="outline" size="compact" onClick={() => void enableMarket()} disabled={!!busy}>
                    {busy === "enable" ? "正在启用…" : "启用插件目录"}
                  </Button>
                </div>
              )}
              {!matches.length && <p className="py-6 text-center text-body text-fg-3">没有找到匹配的插件。</p>}
              <ul className="divide-y divide-border rounded-panel border border-border">
                {matches.slice(0, 80).map((plugin) => (
                  <li key={plugin.id} className="flex items-center gap-3 px-4 py-3">
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-body font-medium text-fg-1">{plugin.displayName || plugin.name}</p>
                      {plugin.description && <p className="mt-1 line-clamp-2 text-caption text-fg-3">{plugin.description}</p>}
                      <p className="mt-1 text-caption text-fg-3">{plugin.author ? `${plugin.author} · ` : ""}{plugin.requiresAuth ? "可能需要授权" : "授权要求以插件说明为准"}</p>
                    </div>
                    <Button variant="outline" size="compact" disabled={!!busy || plugin.installed === null} onClick={() => void change(plugin)}>
                      {busy === plugin.id ? "处理中…" : plugin.installed === null ? "启用后查看" : plugin.installed ? "卸载" : "安装"}
                    </Button>
                  </li>
                ))}
              </ul>
            </>
          )}
          <div className="flex justify-end pt-1"><Button variant="outline" disabled={!!busy} onClick={onClose}>完成</Button></div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
