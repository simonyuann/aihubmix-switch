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
import { Input } from "@/components/ui/input";
import { dispatch, type GroupInfo } from "@/lib/api/aihubmix";
export function ApplyApiDialog({
  onClose,
  onCreated,
}: {
  onClose: () => void;
  onCreated?: () => void;
}) {
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
          setError(friendlyError(e, "加载分组失败，请重试。"));
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
        setError(`API 已申请成功。${result.warning}`);
        setUncertain(true);
      } else onClose();
    } catch (e) {
      if (active.current) {
        setError(
          `${friendlyError(e, "申请结果未能确认")}。请先刷新 Key 列表确认结果，再决定是否重新申请。`,
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
          <DialogTitle>申请 API</DialogTitle>
          <DialogDescription>
            为当前账号申请平台 Key，随后在对应分组卡片中选择此 Key。
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
            <span>名称</span>
            <Input
              aria-label="API 名称"
              placeholder="例如：我的开发工具"
              value={name}
              onChange={(e) => setName(e.target.value)}
              disabled={busy}
            />
          </label>
          <label className="block space-y-2 text-body">
            <span>模型服务</span>
            <select
              aria-label="模型服务"
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
              当前暂无可申请的模型服务，请联系平台客服。
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
              {uncertain ? "返回查看 Key" : "取消"}
            </Button>
            <Button disabled={busy || loading || !group || uncertain}>
              {busy ? "正在申请…" : "确认申请"}
            </Button>
          </div>
        </form>
      </DialogContent>
    </Dialog>
  );
}
