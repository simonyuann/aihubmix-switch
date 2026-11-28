import { useEffect, useRef, useState } from "react";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { friendlyError } from "./friendlyError";
import { useTranslation } from "react-i18next";
import { Input } from "@/components/ui/input";
import { dispatch, type GroupInfo } from "@/lib/api/aihubmix";
export function ApplyApiDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated?: () => void;
}) {
  const { t } = useTranslation();
  const [groups, setGroups] = useState<GroupInfo[]>([]);
  const [group, setGroup] = useState("");
  const [name, setName] = useState("");
  const [loading, setLoading] = useState(true);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [uncertain, setUncertain] = useState(false);
  const gate = useRef(false);
  const active = useRef(true);
  useEffect(() => {
    active.current = true;
    dispatch<GroupInfo[]>("groups")
      .then((value) => {
        if (active.current) {
          setGroups(value);
          setGroup(String(value[0]?.id ?? ""));
        }
      })
      .catch((e) => {
        if (active.current)
          setError(friendlyError(e, t("applyApiDialog.error.loadGroups")));
      })
      .finally(() => {
        if (active.current) setLoading(false);
      });
    return () => {
      active.current = false;
    };
  }, []);
  async function create() {
    if (gate.current || !group || uncertain) return;
    gate.current = true;
    setBusy(true);
    setError("");
    try {
      const result = await dispatch<{ warning?: string }>("create-key", {
        name: name.trim(),
        groupId: Number(group),
      });
      if (!active.current) return;
      onCreated?.();
      if (result.warning) {
        const warning = friendlyError(
          result.warning,
          t("applyApiDialog.error.credentialSaveFailed"),
        );
        setError(`${t("applyApiDialog.success.applied")}${warning}`);
        setUncertain(true);
      } else onClose();
    } catch (e) {
      if (active.current) {
        setError(
          `${friendlyError(e, t("applyApiDialog.error.unconfirmed"))}。${t("applyApiDialog.error.refreshKeysFirst")}`,
        );
        setUncertain(true);
      }
    } finally {
      gate.current = false;
      if (active.current) setBusy(false);
    }
  }
  return (
    <Dialog
      open
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
    >
      <DialogContent className="w-[calc(100vw-2rem)] overflow-hidden">
        <DialogHeader>
          <DialogTitle>{t("applyApiDialog.title")}</DialogTitle>
          <DialogDescription>
            {t("applyApiDialog.description")}
          </DialogDescription>
        </DialogHeader>
        <form
          className="min-h-0 space-y-4 overflow-y-auto px-6 pb-6"
          onSubmit={(e) => {
            e.preventDefault();
            void create();
          }}
        >
          <label className="block space-y-2 text-body">
            <span>{t("applyApiDialog.label.name")}</span>
            <Input
              aria-label={t("applyApiDialog.label.apiName")}
              placeholder={t("applyApiDialog.placeholder.nameExample")}
              value={name}
              onChange={(e) => setName(e.target.value)}
              disabled={busy}
            />
          </label>
          <label className="block space-y-2 text-body">
            <span>{t("applyApiDialog.label.modelService")}</span>
            <select
              aria-label={t("applyApiDialog.label.modelService")}
              className="h-10 w-full rounded-control border border-border bg-surface px-3"
              value={group}
              onChange={(e) => setGroup(e.target.value)}
              disabled={busy || loading}
            >
              {groups.map((g) => (
                <option key={g.id} value={g.id}>
                  {g.name} · {g.platform}
                </option>
              ))}
            </select>
          </label>
          {!loading && !groups.length && (
            <p className="text-body text-fg-3">
              {t("applyApiDialog.empty.noServices")}
            </p>
          )}
          {error && (
            <p role="alert" className="text-body text-destructive">
              {error}
            </p>
          )}
          <div className="flex justify-end gap-2">
            <Button
              type="button"
              variant="outline"
              onClick={onClose}
              disabled={busy}
            >
              {uncertain
                ? t("applyApiDialog.action.backToKeys")
                : t("applyApiDialog.action.cancel")}
            </Button>
            <Button disabled={busy || loading || !group || uncertain}>
              {busy
                ? t("applyApiDialog.status.applying")
                : t("applyApiDialog.action.confirm")}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
