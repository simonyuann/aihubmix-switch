import { useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { Layers, Plus } from "lucide-react";
import { APP_IDS } from "@/config/appConfig";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { APP_DISPLAY_NAME, AppGlyph } from "@/components/shell/AppGlyph";
import { ProviderCard } from "@/components/providers/ProviderCard";
import { ProviderIconBox } from "@/components/ProviderIconBox";
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
import { groupBrandIcon } from "./groupBrandIcon";
import { AnnouncementBanner } from "./Announcements";
import { useWalletActions } from "./WalletPanel";
import { HomeWalletBalance } from "./BalanceDisplay";
import { CodexDesktopControls } from "./CodexDesktopControls";
interface ToolCard {
  provider: Provider;
  configured: boolean;
  keyMasked: string;
  platform?: string;
  groupId?: number;
  selectedKeyId?: number | null;
  inLive?: boolean | null;
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
  simple = false,
  onSelectApp,
}: {
  app: AppId;
  onOpenProviders?: () => void;
  simple?: boolean;
  onSelectApp?: (app: AppId) => void;
}) {
  const client = useQueryClient();
  const wallet = useWalletActions();
  const [selectingTool, setSelectingTool] = useState(false);
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
      const result = await dispatch<{
        warnings?: string[];
        selectedModel?: string | null;
      }>(start ? "start-tool-card" : "save-tool-key", {
        app,
        providerId: card.provider.id,
        ...(selectedKey ? { keyId: Number(selectedKey) } : {}),
      });
      setEditing(null);
      setKeyId("");
      let imagegenNote = "";
      if (start && app === "codex") {
        try {
          const setup = await invoke<{ status: string }>(
            "codex_imagegen_guide",
          );
          imagegenNote =
            setup.status === "written"
              ? "命令行生图需重启 Codex Desktop，并在本机安装 Python 3.11+ 和 OpenAI SDK；WSL、远程主机和容器需分别配置。"
              : "命令行生图未能自动接入。请检查 Codex 的 shell_environment_policy 和 PYTHONPATH 设置，修复后重启 Codex Desktop。";
        } catch {
          imagegenNote =
            "命令行生图接入状态不可用，请检查 Codex 配置后重启 Desktop。";
        }
      }
      setNotice(
        start
          ? app === "dsh"
            ? `已启用 ${card.provider.name}。可以同时启用多个分组；请在 DeepSeek Harness 中选择模型。`
            : `已切换到 ${card.provider.name}。${result.selectedModel ? `默认模型：${result.selectedModel}。` : ""}在 ${APP_DISPLAY_NAME[app]} 中通过 /model 切换模型。${imagegenNote}`
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
        title={simple ? "AIHubMix Switch" : APP_DISPLAY_NAME[app]}
        variant={simple ? "page" : "app"}
        hideBalance={simple}
        icon={simple ? undefined : <AppGlyph app={app} size={22} />}
        actions={
          <>
            {onOpenProviders && (
              <Button variant="outline" onClick={onOpenProviders}>
                其他供应商
              </Button>
            )}
            {!simple && (
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
            )}
            {!simple && (
              <Button onClick={() => setApply(true)}>申请 API</Button>
            )}
          </>
        }
      />
      {simple && <AnnouncementBanner />}
      <main
        id="main-content"
        className="aihub-page-enter min-h-0 flex-1 overflow-y-auto scroll-stable px-6 py-6"
      >
        {simple && <HomeWalletBalance onRecharge={wallet?.recharge} />}
        {app === "dsh" && (
          <p className="mb-4 text-body text-fg-3">
            DeepSeek Harness 支持同时启用多个分组，请在 DeepSeek Harness
            中选择模型。
          </p>
        )}
        {simple && (
          <div className="mb-6">
            <h2 className="mb-3 text-title font-semibold">AI 工具</h2>
            <button
              type="button"
              onClick={() => setSelectingTool(true)}
              aria-label={`选择 AI 工具，当前为 ${APP_DISPLAY_NAME[app]}`}
              className="flex w-full items-center gap-4 rounded-panel border border-action bg-action-soft px-5 py-4 text-start transition-colors hover:bg-subtle focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring motion-reduce:transition-none"
            >
              <AppGlyph app={app} size={32} />
              <span className="flex-1 text-strong font-semibold">
                {APP_DISPLAY_NAME[app]}
              </span>
              <span className="flex items-center gap-1 text-caption font-medium text-action-text">
                <Plus className="h-4 w-4" aria-hidden="true" />
                切换工具
              </span>
            </button>
            {app === "codex" && <CodexDesktopControls />}
          </div>
        )}
        {simple && (
          <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
            <div>
              <h2 className="text-title font-semibold">模型分组</h2>
              <p className="mt-1 text-caption text-fg-3">
                选择当前工具使用的服务
              </p>
            </div>
            <Button
              variant="outline"
              disabled={busy || isPending || !!loadError}
              onClick={() => {
                setError("");
                if (!groups.length) {
                  setApply(true);
                } else {
                  setGroupId(String(availableGroups[0]?.id ?? ""));
                  setAdding(true);
                  void refetch();
                }
              }}
            >
              添加分组
            </Button>
          </div>
        )}
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
          {data?.cards
            .filter((card) => !simple || card.groupId != null)
            .map((card) => {
              const current =
                app !== "dsh" && card.provider.id === data.currentProviderId;
              const enabled = app === "dsh" && card.inLive === true;
              const dshUnknown = app === "dsh" && card.inLive == null;
              const group = groups.find((item) => item.id === card.groupId);
              const brandIcon =
                card.groupId == null
                  ? undefined
                  : groupBrandIcon(
                      group?.name,
                      group?.subscriptionType,
                      card.provider.name,
                      card.platform,
                    );
              if (simple) {
                return (
                  <article
                    key={card.provider.id}
                    className="flex flex-wrap items-center gap-4 rounded-panel border border-border bg-surface px-4 py-4 transition-colors hover:border-border-strong motion-reduce:transition-none"
                  >
                    <ProviderIconBox
                      icon={brandIcon ?? card.provider.icon}
                      name={card.provider.name}
                      color={brandIcon ? undefined : card.provider.iconColor}
                      className="h-10 w-10"
                    />
                    <div className="min-w-0 flex-1">
                      <h3 className="truncate font-semibold text-fg-1">
                        {card.provider.name}
                      </h3>
                      <p className="mt-1 text-caption text-fg-3">
                        {card.configured ? "API 已配置" : "选择 API 后即可使用"}
                      </p>
                    </div>
                    {card.configured && (
                      <Button
                        variant="ghost"
                        size="compact"
                        disabled={busy}
                        aria-label={`管理 ${card.provider.name} 的 API`}
                        onClick={() => edit(card)}
                      >
                        管理 API
                      </Button>
                    )}
                    {current || enabled ? (
                      <span className="rounded-full bg-success-soft px-3 py-1 text-caption font-medium text-success-text">
                        {enabled ? "已启用" : "使用中"}
                      </span>
                    ) : (
                      <Button
                        variant="outline"
                        size="compact"
                        disabled={busy}
                        aria-label={`${card.configured ? (app === "dsh" ? "启用" : "切换到") : app === "dsh" ? "选择 API 并启用" : "选择 API 并使用"} ${card.provider.name}`}
                        onClick={() =>
                          card.configured
                            ? void operate(card, true)
                            : edit(card)
                        }
                      >
                        {card.configured
                          ? app === "dsh"
                            ? "启用"
                            : "切换"
                          : "选择 API"}
                      </Button>
                    )}
                    {dshUnknown && (
                      <span className="text-caption text-fg-3">状态未知</span>
                    )}
                  </article>
                );
              }
              return (
                <ProviderCard
                  key={card.provider.id}
                  provider={{
                    ...card.provider,
                    icon: brandIcon ?? card.provider.icon,
                    iconColor: brandIcon ? undefined : card.provider.iconColor,
                  }}
                  appId={app}
                  fixedService
                  isCurrent={current}
                  isInConfig={app === "dsh" ? enabled : undefined}
                  presentation={{
                    tone: current || enabled ? "direct" : undefined,
                    status:
                      current || enabled
                        ? {
                            label: enabled ? "已启用" : "使用中",
                            dot: "direct",
                          }
                        : dshUnknown
                          ? { label: "状态未知", dot: "muted" }
                          : undefined,
                    buttons:
                      current || enabled
                        ? []
                        : [
                            {
                              key: "switch",
                              label: `${app === "dsh" ? "启用" : "启动"} ${card.provider.name}`,
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
          {data &&
            simple &&
            !data.warning &&
            !data.cards.some((card) => card.groupId != null) && (
              <div className="flex flex-col items-center gap-3 py-10 text-center">
                <Layers className="h-8 w-8 text-fg-3" aria-hidden="true" />
                <p className="font-semibold">还没有添加模型分组</p>
                <p className="text-body text-fg-3">
                  申请 API 后，即可在这里选择分组并开始使用。
                </p>
                <Button onClick={() => setApply(true)}>申请 API</Button>
              </div>
            )}
          {data && !simple && !data.cards.length && (
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
      {selectingTool && (
        <Dialog open onOpenChange={setSelectingTool}>
          <DialogContent className="w-[calc(100vw-2rem)] max-w-2xl">
            <DialogHeader>
              <DialogTitle>选择工具</DialogTitle>
              <DialogDescription>
                选择后在首页展示该工具及其分组。
              </DialogDescription>
            </DialogHeader>
            <div className="grid max-h-[60vh] grid-cols-1 gap-1 overflow-y-auto px-6 pb-6 sm:grid-cols-2">
              {APP_IDS.map((item) => (
                <button
                  key={item}
                  type="button"
                  aria-current={item === app ? "true" : undefined}
                  onClick={() => {
                    setSelectingTool(false);
                    onSelectApp?.(item);
                  }}
                  className="flex items-center gap-3 rounded-control px-3 py-2 text-start text-body transition-colors hover:bg-subtle focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring motion-reduce:transition-none"
                >
                  <AppGlyph app={item} size={20} />
                  <span className="flex-1">{APP_DISPLAY_NAME[item]}</span>
                  {item === app && (
                    <span className="text-caption text-action-text">当前</span>
                  )}
                </button>
              ))}
            </div>
          </DialogContent>
        </Dialog>
      )}
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
                {app === "dsh"
                  ? "仅显示此分组下可用的 API Key。启用后请在 DeepSeek Harness 中选择模型。"
                  : "仅显示此分组下可用的 API Key。模型可在 Agent 中通过 /model 切换。"}
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
                  {busy
                    ? "正在处理…"
                    : app === "dsh"
                      ? "保存并启用"
                      : "保存并启动"}
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
