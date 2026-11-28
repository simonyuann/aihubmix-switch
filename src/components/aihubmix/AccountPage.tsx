import { useCallback, useEffect, useRef, useState } from "react";
import { KeyRound, Plus } from "lucide-react";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { Button } from "@/components/ui/button";
import { useAccount } from "@/contexts/AccountContext";
import { dispatch, type KeyInfo } from "@/lib/api/aihubmix";
import { ApplyApiDialog } from "./ApplyApiDialog";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { friendlyError } from "./friendlyError";
export function AccountPage() {
  const { state } = useAccount();
  const [keys, setKeys] = useState<KeyInfo[]>([]);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [busy, setBusy] = useState(false);
  const [apply, setApply] = useState(false);
  const [deleting, setDeleting] = useState<KeyInfo | null>(null);
  const operation = useRef(false);
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
      if (rev === revision.current)
        setError(friendlyError(e, "加载 API 列表失败，请重试。"));
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
  async function copyKey(key: KeyInfo) {
    if (operation.current) return;
    operation.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    const rev = revision.current;
    try {
      const value = await dispatch<string>("get-key", { id: key.id });
      if (rev !== revision.current) return;
      await navigator.clipboard.writeText(value);
      setNotice("API Key 已复制");
    } catch (e) {
      if (rev === revision.current)
        setError(friendlyError(e, "复制失败，请重试。"));
    } finally {
      operation.current = false;
      if (rev === revision.current) setBusy(false);
    }
  }
  async function deleteKey() {
    if (!deleting || operation.current) return;
    operation.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    const rev = revision.current;
    try {
      await dispatch("delete-key", { id: deleting.id });
      if (rev !== revision.current) return;
      setKeys((current) => current.filter((key) => key.id !== deleting.id));
      setNotice(`已删除 API Key「${deleting.name}」`);
      setDeleting(null);
    } catch (e) {
      if (rev === revision.current) {
        setDeleting(null);
        setError(
          `${friendlyError(e, "删除未能确认")}。请刷新列表确认删除结果后再重试。`,
        );
      }
    } finally {
      operation.current = false;
      if (rev === revision.current) setBusy(false);
    }
  }
  return (
    <>
      <AppPageHeader
        title="API 管理"
        icon={<KeyRound className="h-5 w-5" />}
        actions={
          <Button disabled={busy} onClick={() => setApply(true)}>
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
                </div>
                <div className="flex gap-2">
                  <Button
                    variant="outline"
                    disabled={busy}
                    aria-label={`复制 ${k.name}`}
                    onClick={() => void copyKey(k)}
                  >
                    复制 Key
                  </Button>
                  <Button
                    variant="destructive"
                    disabled={busy}
                    aria-label={`删除 ${k.name}`}
                    onClick={() => setDeleting(k)}
                  >
                    删除
                  </Button>
                </div>
              </div>
            ))}
          </section>
          <p className="text-caption leading-6 text-fg-3">
            在首页选择工具并添加模型分组，AIHub 会自动绑定可用的 API
            Key。模型可在 Agent 中通过 /model 切换。
          </p>
        </div>
      </main>
      <ConfirmDialog
        isOpen={deleting !== null}
        title="删除 API Key"
        message={`确定删除「${deleting?.name ?? ""}」？删除后，使用此 Key 的工具将无法继续调用 API，此操作无法撤销。`}
        confirmText="确认删除"
        cancelText="取消"
        pending={busy}
        onConfirm={() => void deleteKey()}
        onCancel={() => setDeleting(null)}
      />
      {apply && (
        <ApplyApiDialog
          onClose={() => setApply(false)}
          onCreated={() => void load()}
        />
      )}
    </>
  );
}
