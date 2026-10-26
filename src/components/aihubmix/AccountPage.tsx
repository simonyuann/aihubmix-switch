import { useCallback, useEffect, useRef, useState } from "react";
import { Globe, Plus, LogOut } from "lucide-react";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { Button } from "@/components/ui/button";
import { useAccount } from "@/contexts/AccountContext";
import { dispatch, type KeyInfo } from "@/lib/api/aihubmix";
import { ApplyApiDialog } from "./ApplyApiDialog";
export function AccountPage() {
  const { state, logout } = useAccount();
  const [keys, setKeys] = useState<KeyInfo[]>([]);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [apply, setApply] = useState(false);
  const revision = useRef(0);
  const load = useCallback(async () => {
    const rev = ++revision.current;
    setBusy(true);
    setError("");
    try {
      const value = await dispatch<KeyInfo[]>("keys");
      if (rev === revision.current)
        setKeys(value.filter((k) => k.id > 0 && !k.local));
    } catch (e) {
      if (rev === revision.current) setError(String(e));
    } finally {
      if (rev === revision.current) setBusy(false);
    }
  }, []);
  useEffect(() => {
    void load();
    return () => {
      revision.current++;
    };
  }, [load]);
  return (
    <>
      <AppPageHeader
        title="我的账户"
        icon={<Globe className="h-5 w-5" />}
        actions={
          <Button onClick={() => setApply(true)}>
            <Plus className="me-2 h-4 w-4" />
            申请 API
          </Button>
        }
      />
      <main
        id="main-content"
        className="min-h-0 flex-1 overflow-y-auto scroll-stable px-6 py-5"
      >
        <div className="max-w-4xl space-y-6">
          <section className="flex items-center justify-between gap-4 rounded-panel border border-border p-5">
            <div>
              <p className="text-caption text-fg-3">当前账号</p>
              <p className="mt-1 text-strong font-medium">
                {state?.account.email}
              </p>
            </div>
            <Button
              variant="outline"
              disabled={busy}
              onClick={() => {
                setBusy(true);
                void logout().catch((e) => {
                  setError(String(e));
                  setBusy(false);
                });
              }}
            >
              <LogOut className="me-2 h-4 w-4" />
              退出登录
            </Button>
          </section>
          <section className="rounded-panel border border-border">
            <div className="flex items-center justify-between border-b border-border p-4">
              <h2 className="font-semibold">我的 API Keys</h2>
              <Button
                variant="ghost"
                disabled={busy}
                onClick={() => void load()}
              >
                刷新
              </Button>
            </div>
            {error && (
              <p role="alert" className="p-4 text-body text-destructive">
                {error}
              </p>
            )}
            {notice && (
              <p role="status" className="p-4 text-body text-fg-2">
                {notice}
              </p>
            )}
            {!keys.length && (
              <div className="px-6 py-12 text-center">
                <p className="text-body text-fg-3">
                  {busy ? "正在加载…" : "还没有 API Key"}
                </p>
                {!busy && (
                  <Button className="mt-4" onClick={() => setApply(true)}>
                    申请 API
                  </Button>
                )}
              </div>
            )}
            {keys.map((k) => (
              <div
                key={k.id}
                className="flex flex-wrap items-center justify-between gap-4 border-b border-border p-4 last:border-b-0"
              >
                <div>
                  <p className="font-medium">
                    {k.name}
                    <span className="ms-2 text-caption text-fg-3">
                      {k.groupName} · {k.status}
                    </span>
                  </p>
                  <code className="mt-1 block text-caption text-fg-3">
                    {k.keyMasked}
                  </code>
                  <p className="mt-1 text-caption text-fg-3">
                    Key 配额：{k.quotaUsed ?? "—"} / {k.quota ?? "—"}
                  </p>
                </div>
                <Button
                  variant="outline"
                  onClick={() => {
                    setError("");
                    void dispatch<string>("get-key", { id: k.id })
                      .then((key) => navigator.clipboard.writeText(key))
                      .then(() => setNotice("API Key 已复制"))
                      .catch((e) => setError(String(e)));
                  }}
                >
                  复制 Key
                </Button>
              </div>
            ))}
          </section>
          <p className="text-caption leading-6 text-fg-3">
            在左侧选择工具，再选择当前账号的 Key 与模型，即可应用平台 API 配置。
          </p>
        </div>
      </main>
      {apply && (
        <ApplyApiDialog
          onClose={() => setApply(false)}
          onCreated={() => void load()}
        />
      )}
    </>
  );
}
