import { useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { APP_DISPLAY_NAME, AppGlyph } from "@/components/shell/AppGlyph";
import { ProviderCard } from "@/components/providers/ProviderCard";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { settingsApi } from "@/lib/api/settings";
import { dispatch, type GroupInfo, type KeyInfo } from "@/lib/api/aihubmix";
import type { AppId } from "@/lib/api";
import type { Provider } from "@/types";
import { ApplyApiDialog } from "./ApplyApiDialog";
interface ToolCard {
  provider: Provider;
  configured: boolean;
  keyMasked: string;
  platform?: string;
  groupId?: number;
  selectedKeyId?: number | null;
}
interface ToolCards {
  cards: ToolCard[];
  groups: GroupInfo[];
  currentProviderId: string | null;
  warning?: string;
}
export function ToolPage({
  app,
  onOpenProviders,
}: {
  app: AppId;
  onOpenProviders?: () => void;
}) {
  const client = useQueryClient();
  const {
    data,
    isPending,
    error: loadError,
    refetch,
  } = useQuery({
    queryKey: ["tool-cards", app],
    queryFn: () => dispatch<ToolCards>("tool-cards", { app }),
  });
  const [editing, setEditing] = useState<ToolCard | null>(null);
  const [keyId, setKeyId] = useState("");
  const [adding, setAdding] = useState(false);
  const [groupId, setGroupId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [apply, setApply] = useState(false);
  const gate = useRef(false);
  const keyQuery = useQuery({
    queryKey: ["platform-keys"],
    queryFn: () => dispatch<KeyInfo[]>("keys"),
    enabled: editing !== null,
  });
  const keys = (keyQuery.data ?? []).filter(
    (key) =>
      !key.local &&
      key.id > 0 &&
      key.status === "active" &&
      key.groupId === editing?.groupId,
  );
  const groups = data?.groups ?? [];
  const availableGroups = groups.filter(
    (group) => !data?.cards.some((card) => card.groupId === group.id),
  );
  const groupAdded = (id: number) =>
    data?.cards.some((card) => card.groupId === id);
  function edit(card: ToolCard) {
    setKeyId(card.selectedKeyId ? String(card.selectedKeyId) : "");
    setError("");
    setEditing(card);
  }
  async function createCard() {
    if (gate.current || !groupId) return;
    gate.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      await dispatch("create-tool-card", { app, groupId: Number(groupId) });
      await refetch();
      setAdding(false);
      setNotice("分组已添加，请选择此分组的 API Key。");
    } catch (e) {
      setError(String(e));
    } finally {
      gate.current = false;
      setBusy(false);
    }
  }
  async function operate(card: ToolCard, start: boolean, selectedKey?: string) {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await dispatch<{ warnings?: string[] }>(
        start ? "start-tool-card" : "save-tool-key",
        {
          app,
          providerId: card.provider.id,
          ...(selectedKey ? { keyId: Number(selectedKey) } : {}),
        },
      );
      setEditing(null);
      setKeyId("");
      setNotice(
        start
          ? `已切换到 ${card.provider.name}。在 ${APP_DISPLAY_NAME[app]} 中通过 /model 切换模型。`
          : "API Key 已保存。",
      );
      if (result?.warnings?.length)
        setNotice((notice) => `${notice} ${result.warnings!.join("；")}`);
      await Promise.all([
        refetch(),
        client.invalidateQueries({ queryKey: ["providers", app] }),
        client.invalidateQueries({ queryKey: ["proxy"] }),
      ]);
    } catch (e) {
      setError(String(e));
    } finally {
      gate.current = false;
      setBusy(false);
    }
  }
  return (
    <>
      <AppPageHeader
        title={APP_DISPLAY_NAME[app]}
        variant="app"
        icon={<AppGlyph app={app} size={22} />}
        actions={
          <>
            {onOpenProviders && (
              <Button variant="outline" onClick={onOpenProviders}>
                其他供应商
              </Button>
            )}
            <Button
              variant="outline"
              disabled={busy || isPending || !!loadError}
              onClick={() => {
                setError("");
                setGroupId(String(availableGroups[0]?.id ?? ""));
                setAdding(true);
                void refetch();
              }}
            >
              添加分组
            </Button>
            <Button variant="outline" onClick={() => setApply(true)}>
              申请 API
            </Button>
          </>
        }
      />
      <main
        id="main-content"
        className="min-h-0 flex-1 overflow-y-auto scroll-stable px-6 py-6"
      >
        <div className="space-y-3">
          {isPending && (
            <p role="status" className="text-body text-fg-3">
              正在加载模型服务…
            </p>
          )}
          {loadError && (
            <div role="alert" className="text-body text-destructive">
              {String(loadError)}{" "}
              <Button variant="outline" onClick={() => void refetch()}>
                重试
              </Button>
            </div>
          )}
          {data?.warning && (
            <p role="alert" className="text-body text-fg-3">
              {data.warning}{" "}
              <Button variant="outline" onClick={() => void refetch()}>
                重试
              </Button>
            </p>
          )}
          {data?.cards.map((card) => {
            const current = card.provider.id === data.currentProviderId;
            return (
              <ProviderCard
                key={card.provider.id}
                provider={card.provider}
                appId={app}
                fixedService
                isCurrent={current}
                presentation={{
                  tone: current ? "direct" : undefined,
                  status: current
                    ? { label: "使用中", dot: "direct" }
                    : undefined,
                  buttons: current
                    ? []
                    : [
                        {
                          key: "switch",
                          label: `启动 ${card.provider.name}`,
                          disabledReason: busy ? "正在切换配置" : undefined,
                          onClick: () =>
                            card.configured
                              ? void operate(card, true)
                              : edit(card),
                        },
                      ],
                  chips:
                    card.provider.category === "official"
                      ? []
                      : [
                          {
                            key: "key",
                            label: card.configured
                              ? card.keyMasked || "已配置"
                              : "待选择 API",
                            tone: "outline",
                          },
                        ],
                  editDisabledReason: busy ? "正在保存配置" : undefined,
                }}
                onEdit={() => edit(card)}
                onDelete={() => {}}
                onConfigureUsage={() => {}}
                onOpenWebsite={(url) => void settingsApi.openExternal(url)}
              />
            );
          })}
          {data && !data.cards.length && (
            <p className="text-body text-fg-3">
              当前暂无适用于此工具的模型服务。
            </p>
          )}
          {error && !editing && !adding && (
            <p role="alert" className="break-words text-body text-destructive">
              {error}
            </p>
          )}
          {notice && (
            <p role="status" className="text-body text-fg-2">
              {notice}
            </p>
          )}
        </div>
      </main>
      {adding && (
        <Dialog
          open
          onOpenChange={(open) => {
            if (!open && !busy) setAdding(false);
          }}
        >
          <DialogContent className="w-[calc(100vw-2rem)]">
            <DialogHeader>
              <DialogTitle>添加分组</DialogTitle>
              <DialogDescription>
                选择已持有可用 API 的分组，卡片中只会显示此分组的 API Key。
              </DialogDescription>
            </DialogHeader>
            <form
              className="space-y-4 px-6 pb-6"
              onSubmit={(e) => {
                e.preventDefault();
                void createCard();
              }}
            >
              <label className="block space-y-2 text-body">
                <span>平台分组</span>
                <select
                  aria-label="平台分组"
                  className="h-10 w-full rounded-control border border-border bg-surface px-3"
                  value={groupId}
                  onChange={(e) => setGroupId(e.target.value)}
                  disabled={busy || !groups.length}
                >
                  <option value="">请选择分组</option>
                  {groups.map((group) => (
                    <option
                      key={group.id}
                      value={group.id}
                      disabled={groupAdded(group.id)}
                    >
                      {group.name}
                      {groupAdded(group.id) ? "（已添加）" : ""}
                    </option>
                  ))}
                </select>
              </label>
              <Button
                type="button"
                variant="outline"
                disabled={busy || isPending}
                onClick={() => void refetch()}
              >
                刷新分组
              </Button>
              {!availableGroups.length && (
                <p className="text-body text-fg-3">
                  暂无可添加的分组。请先申请对应分组的 API；已添加的分组已标记。
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
                  disabled={busy}
                  onClick={() => setAdding(false)}
                >
                  取消
                </Button>
                <Button
                  aria-label="确认添加分组"
                  disabled={
                    busy ||
                    !availableGroups.some(
                      (group) => String(group.id) === groupId,
                    )
                  }
                >
                  {busy ? "正在添加…" : "添加分组"}
                </Button>
              </div>
            </form>
          </DialogContent>
        </Dialog>
      )}
      {editing && (
        <Dialog
          open
          onOpenChange={(open) => {
            if (!open && !busy) {
              setEditing(null);
              setKeyId("");
            }
          }}
        >
          <DialogContent className="w-[calc(100vw-2rem)]">
            <DialogHeader>
              <DialogTitle>{editing.provider.name}</DialogTitle>
              <DialogDescription>
                仅显示此分组下可用的 API Key。模型可在 Agent 中通过 /model
                切换。
              </DialogDescription>
            </DialogHeader>
            <form
              className="space-y-4 px-6 pb-6"
              onSubmit={(e) => {
                e.preventDefault();
                void operate(editing, false, keyId);
              }}
            >
              <label className="block space-y-2 text-body">
                <span>API Key</span>
                <select
                  autoFocus
                  aria-label="API Key"
                  className="h-10 w-full rounded-control border border-border bg-surface px-3"
                  value={keyId}
                  onChange={(e) => setKeyId(e.target.value)}
                  disabled={busy || keyQuery.isPending}
                >
                  <option value="">请选择 API Key</option>
                  {keys.map((key) => (
                    <option key={key.id} value={key.id}>
                      {key.name} · {key.keyMasked}
                    </option>
                  ))}
                </select>
              </label>
              {keyQuery.isPending && (
                <p role="status" className="text-body text-fg-3">
                  正在加载 API…
                </p>
              )}
              {keyQuery.error && (
                <p role="alert" className="text-body text-destructive">
                  {String(keyQuery.error)}{" "}
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => void keyQuery.refetch()}
                  >
                    重试
                  </Button>
                </p>
              )}
              {!keyQuery.isPending && !keyQuery.error && !keys.length && (
                <p className="text-body text-fg-3">
                  此分组暂无可用 API，请通过右上角「申请 API」创建。
                </p>
              )}
              {error && (
                <p
                  role="alert"
                  className="break-words text-body text-destructive"
                >
                  {error}
                </p>
              )}
              <div className="flex flex-wrap justify-end gap-2">
                <Button
                  type="button"
                  variant="outline"
                  disabled={busy}
                  onClick={() => {
                    setEditing(null);
                    setKeyId("");
                  }}
                >
                  取消
                </Button>
                <Button
                  variant="outline"
                  disabled={
                    busy || !keys.some((key) => String(key.id) === keyId)
                  }
                >
                  保存
                </Button>
                <Button
                  type="button"
                  disabled={
                    busy || !keys.some((key) => String(key.id) === keyId)
                  }
                  onClick={() => void operate(editing, true, keyId)}
                >
                  {busy ? "正在处理…" : "保存并启动"}
                </Button>
              </div>
            </form>
          </DialogContent>
        </Dialog>
      )}
      {apply && (
        <ApplyApiDialog
          onClose={() => setApply(false)}
          onCreated={() => {
            void refetch();
            void client.invalidateQueries({ queryKey: ["platform-keys"] });
          }}
        />
      )}
    </>
  );
}
