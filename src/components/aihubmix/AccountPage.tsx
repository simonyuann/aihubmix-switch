import { useCallback, useEffect, useRef, useState } from "react";
import { KeyRound, Plus } from "lucide-react";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { Button } from "@/components/ui/button";
import { useAccount } from "@/contexts/AccountContext";
import { dispatch, type KeyInfo } from "@/lib/api/aihubmix";
import { useTranslation } from "react-i18next";
import { ApplyApiDialog } from "./ApplyApiDialog";
import { ConfirmDialog } from "@/components/ConfirmDialog";
import { friendlyError } from "./friendlyError";
export function AccountPage() {
  const { t } = useTranslation();
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
        setError(friendlyError(e, t("accountPage.error.loadKeys")));
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
      setNotice(t("accountPage.notice.keyCopied"));
    } catch (e) {
      if (rev === revision.current)
        setError(friendlyError(e, t("accountPage.error.copyFailed")));
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
      setNotice(t("accountPage.notice.keyDeleted", { name: deleting.name }));
      setDeleting(null);
    } catch (e) {
      if (rev === revision.current) {
        setDeleting(null);
        setError(
          t("accountPage.error.deleteUnconfirmedDetail", {
            detail: friendlyError(e, t("accountPage.error.deleteUnconfirmed")),
          }),
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
        title={t("accountPage.title")}
        icon={<KeyRound className="h-5 w-5" />}
        actions={
          <Button disabled={busy} onClick={() => setApply(true)}>
            <Plus className="me-2 h-4 w-4" />
            {t("applyApiDialog.action.open")}
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
              <p className="text-caption text-fg-3">
                {t("accountPage.label.currentAccount")}
              </p>
              <p className="mt-1 text-strong font-medium">
                {state?.account.email}
              </p>
            </div>
          </section>
          <section className="rounded-panel border border-border">
            <div className="flex items-center justify-between border-b border-border p-4">
              <h2 className="font-semibold">
                {t("accountPage.section.myKeys")}
              </h2>
              <Button
                variant="ghost"
                disabled={busy}
                onClick={() => void load()}
              >
                {t("accountPage.action.refresh")}
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
                  {busy
                    ? t("accountPage.status.loading")
                    : t("accountPage.empty.noKeys")}
                </p>
                {!busy && (
                  <Button className="mt-4" onClick={() => setApply(true)}>
                    {t("applyApiDialog.action.open")}
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
                    aria-label={t("accountPage.aria.copyKey", { name: k.name })}
                    onClick={() => void copyKey(k)}
                  >
                    {t("accountPage.action.copyKey")}
                  </Button>
                  <Button
                    variant="destructive"
                    disabled={busy}
                    aria-label={t("accountPage.aria.deleteKey", {
                      name: k.name,
                    })}
                    onClick={() => setDeleting(k)}
                  >
                    {t("accountPage.action.deleteKey")}
                  </Button>
                </div>
              </div>
            ))}
          </section>
          <p className="text-caption leading-6 text-fg-3">
            {t("accountPage.empty.bindHint")}
          </p>
        </div>
      </main>
      <ConfirmDialog
        isOpen={deleting !== null}
        title={t("accountPage.dialog.deleteTitle")}
        message={t("accountPage.dialog.deleteBody", {
          name: deleting?.name ?? "",
        })}
        confirmText={t("accountPage.dialog.confirmDelete")}
        cancelText={t("accountPage.action.cancel")}
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
