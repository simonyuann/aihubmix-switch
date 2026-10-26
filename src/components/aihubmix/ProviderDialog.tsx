import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { providersApi } from "@/lib/api/providers";
import type { AppId } from "@/lib/api";
import { aihubmixApi, dispatch, type KeyInfo } from "@/lib/api/aihubmix";
import { APP_DISPLAY_NAME } from "@/components/shell/AppGlyph";
export function ProviderDialog({
  item,
  onClose,
}: {
  item: KeyInfo;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const active = useRef(true);
  const gate = useRef(false);
  useEffect(() => {
    active.current = true;
    return () => {
      active.current = false;
    };
  }, []);
  const [app, setApp] = useState<AppId>("codex");
  const [models, setModels] = useState<{ id: string; name: string }[]>([]);
  const [model, setModel] = useState("");
  const [effort, setEffort] = useState("auto");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let active = true;
    dispatch<typeof models>("models", { id: item.id })
      .then((value) => {
        if (active) {
          setModels(value);
          setModel(value[0]?.id ?? "");
        }
      })
      .catch((e) => {
        if (active) setError(String(e));
      });
    return () => {
      active = false;
    };
  }, [item.id]);
  async function save() {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    setError("");
    try {
      const draft = await aihubmixApi.draft({
        keyId: item.id,
        app,
        model,
        ...(effort === "auto" ? {} : { effort }),
      });
      if (!active.current) return;
      const saved = await providersApi.add(draft, app, false);
      if (!saved) throw new Error("供应商保存失败");
      await queryClient.invalidateQueries({ queryKey: ["providers", app] });
      if (active.current) onClose();
    } catch (e) {
      if (active.current) setError(String(e));
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
      <DialogContent>
        <DialogHeader>
          <DialogTitle>添加 CC 供应商</DialogTitle>
          <DialogDescription>
            保存后，在 CC 供应商页显式切换或应用配置。
          </DialogDescription>
        </DialogHeader>
        <label>
          目标应用
          <select
            className="w-full rounded-control border border-border bg-surface p-2"
            value={app}
            onChange={(e) => setApp(e.target.value as AppId)}
          >
            {Object.entries(APP_DISPLAY_NAME).map(([id, name]) => (
              <option key={id} value={id}>
                {name}
              </option>
            ))}
          </select>
        </label>
        <label>
          模型
          <Input
            list="aihubmix-models"
            value={model}
            onChange={(e) => setModel(e.target.value)}
          />
          <datalist id="aihubmix-models">
            {models.map((m) => (
              <option key={m.id} value={m.id}>
                {m.name}
              </option>
            ))}
          </datalist>
        </label>
        {app === "codex" && (
          <label>
            推理强度
            <select
              className="w-full rounded-control border border-border bg-surface p-2"
              value={effort}
              onChange={(e) => setEffort(e.target.value)}
            >
              {["auto", "minimal", "low", "medium", "high", "xhigh"].map(
                (v) => (
                  <option key={v}>{v}</option>
                ),
              )}
            </select>
          </label>
        )}
        {error && (
          <p role="alert" className="text-destructive">
            {error}
          </p>
        )}
        <Button disabled={busy || !model.trim()} onClick={() => void save()}>
          保存供应商
        </Button>
      </DialogContent>
    </Dialog>
  );
}
