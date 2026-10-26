import { useEffect, useRef, useState } from "react";
import { CheckCircle2, Loader2, KeyRound } from "lucide-react";
import { useQueryClient } from "@tanstack/react-query";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { APP_DISPLAY_NAME, AppGlyph } from "@/components/shell/AppGlyph";
import { Button } from "@/components/ui/button";
import { dispatch, type KeyInfo } from "@/lib/api/aihubmix";
import type { AppId } from "@/lib/api";
import { ApplyApiDialog } from "./ApplyApiDialog";
interface ToolStatus {
  configured: boolean;
  model: string;
  keyId: number | null;
}
export function ToolPage({ app }: { app: AppId }) {
  const client = useQueryClient();
  const [keys, setKeys] = useState<KeyInfo[]>([]);
  const [keyId, setKey] = useState("");
  const [models, setModels] = useState<{ id: string; name: string }[]>([]);
  const [model, setModel] = useState("");
  const [effort, setEffort] = useState("auto");
  const [loading, setLoading] = useState(true);
  const [modelsLoading, setModelsLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [configured, setConfigured] = useState(false);
  const [notice, setNotice] = useState("");
  const [error, setError] = useState("");
  const [apply, setApply] = useState(false);
  const [reload, setReload] = useState(0);
  const gate = useRef(false);
  const revision = useRef(0);
  useEffect(() => {
    const rev = ++revision.current;
    setLoading(true);
    setKeys([]);
    setModels([]);
    setKey("");
    setModel("");
    setError("");
    setNotice("");
    setConfigured(false);
    Promise.all([
      dispatch<KeyInfo[]>("keys"),
      dispatch<ToolStatus>("tool-status", { app }),
    ])
      .then(([all, status]) => {
        if (rev !== revision.current) return;
        const own = all.filter((k) => k.id > 0 && !k.local);
        setKeys(own);
        setKey(
          String(
            own.find((k) => k.id === status.keyId)?.id ?? own[0]?.id ?? "",
          ),
        );
        setModel(status.model ?? "");
        setConfigured(status.configured);
      })
      .catch((e) => {
        if (rev === revision.current) setError(String(e));
      })
      .finally(() => {
        if (rev === revision.current) setLoading(false);
      });
    return () => {
      revision.current++;
    };
  }, [app, reload]);
  useEffect(() => {
    if (!keyId) return;
    let active = true;
    setModelsLoading(true);
    setModels([]);
    setError("");
    dispatch<typeof models>("models", { id: Number(keyId) })
      .then((list) => {
        if (active) {
          setModels(list);
          setModel((current) =>
            list.some((m) => m.id === current) ? current : (list[0]?.id ?? ""),
          );
        }
      })
      .catch((e) => {
        if (active) setError(String(e));
      })
      .finally(() => {
        if (active) setModelsLoading(false);
      });
    return () => {
      active = false;
    };
  }, [keyId]);
  async function save() {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    const rev = revision.current;
    try {
      await dispatch("configure-tool", {
        app,
        keyId: Number(keyId),
        model,
        ...(app === "codex" && effort !== "auto" ? { effort } : {}),
      });
      if (rev !== revision.current) return;
      setConfigured(true);
      setNotice("平台 API 配置已应用。重新打开工具即可使用。");
      await client.invalidateQueries({ queryKey: ["providers", app] });
    } catch (e) {
      if (rev === revision.current) setError(String(e));
    } finally {
      gate.current = false;
      if (rev === revision.current) setBusy(false);
    }
  }
  return (
    <>
      <AppPageHeader
        title={APP_DISPLAY_NAME[app]}
        variant="app"
        icon={<AppGlyph app={app} size={22} />}
        actions={
          <Button variant="outline" onClick={() => setApply(true)}>
            申请 API
          </Button>
        }
      />
      <main
        id="main-content"
        className="min-h-0 flex-1 overflow-y-auto scroll-stable px-6 py-6"
      >
        <div className="max-w-3xl space-y-6">
          <section className="rounded-panel border border-border bg-surface">
            <div className="flex items-center justify-between gap-3 border-b border-border p-5">
              <div className="flex items-center gap-3">
                <div className="flex h-10 w-10 items-center justify-center rounded-control bg-action/10 text-action">
                  <KeyRound className="h-5 w-5" />
                </div>
                <div>
                  <h2 className="font-semibold">AIHubMix API</h2>
                  <p className="mt-1 text-caption text-fg-3">默认平台服务</p>
                </div>
              </div>
              <span className="flex items-center gap-1.5 text-caption text-fg-3">
                {configured && <CheckCircle2 className="h-4 w-4 text-action" />}
                {configured ? "已配置" : "待配置"}
              </span>
            </div>
            <form
              className="space-y-5 p-5"
              onSubmit={(e) => {
                e.preventDefault();
                void save();
              }}
            >
              {loading ? (
                <p role="status" className="text-body text-fg-3">
                  正在加载账号配置…
                </p>
              ) : !keys.length ? (
                <div className="py-5 text-center">
                  <p className="mb-4 text-body text-fg-3">
                    先申请一个 API Key，即可连接此工具。
                  </p>
                  <Button type="button" onClick={() => setApply(true)}>
                    申请 API
                  </Button>
                </div>
              ) : (
                <>
                  <label className="block space-y-2 text-body">
                    <span>API Key</span>
                    <select
                      aria-label="API Key"
                      className="h-10 w-full rounded-control border border-border bg-surface px-3"
                      value={keyId}
                      onChange={(e) => {
                        setKey(e.target.value);
                        setModel("");
                        setNotice("");
                      }}
                      disabled={busy}
                    >
                      {keys.map((k) => (
                        <option key={k.id} value={k.id}>
                          {k.name} · {k.keyMasked}
                        </option>
                      ))}
                    </select>
                  </label>
                  <label className="block space-y-2 text-body">
                    <span>模型</span>
                    <select
                      aria-label="模型"
                      className="h-10 w-full rounded-control border border-border bg-surface px-3"
                      value={model}
                      onChange={(e) => setModel(e.target.value)}
                      disabled={busy || modelsLoading}
                    >
                      {modelsLoading ? (
                        <option value="">正在加载模型…</option>
                      ) : models.length ? (
                        models.map((m) => (
                          <option key={m.id} value={m.id}>
                            {m.name || m.id}
                          </option>
                        ))
                      ) : (
                        <option value="">暂无可用模型</option>
                      )}
                    </select>
                  </label>
                  {app === "codex" && (
                    <label className="block space-y-2 text-body">
                      <span>推理强度</span>
                      <select
                        aria-label="推理强度"
                        className="h-10 w-full rounded-control border border-border bg-surface px-3"
                        value={effort}
                        onChange={(e) => setEffort(e.target.value)}
                        disabled={busy}
                      >
                        {[
                          "auto",
                          "minimal",
                          "low",
                          "medium",
                          "high",
                          "xhigh",
                        ].map((v) => (
                          <option key={v} value={v}>
                            {v === "auto" ? "自动" : v}
                          </option>
                        ))}
                      </select>
                    </label>
                  )}
                  <p className="text-caption leading-6 text-fg-3">
                    应用后会更新 {APP_DISPLAY_NAME[app]} 的本机连接配置。
                  </p>
                  <div className="flex justify-end">
                    <Button disabled={busy || modelsLoading || !model}>
                      {busy && (
                        <Loader2 className="me-2 h-4 w-4 animate-spin" />
                      )}
                      {busy ? "正在应用…" : "应用配置"}
                    </Button>
                  </div>
                </>
              )}
              {error && (
                <p role="alert" className="text-body text-destructive">
                  {error}
                </p>
              )}
              {notice && (
                <p role="status" className="text-body text-fg-2">
                  {notice}
                </p>
              )}
            </form>
          </section>
        </div>
      </main>
      {apply && (
        <ApplyApiDialog
          onClose={() => setApply(false)}
          onCreated={() => setReload((v) => v + 1)}
        />
      )}
    </>
  );
}
