import { useRef, useState } from "react";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { invoke } from "@tauri-apps/api/core";
import { ArrowLeftRight, Layers, Plus } from "lucide-react";
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
import { ClaudeDesktopControls } from "./ClaudeDesktopControls";
import { friendlyError } from "./friendlyError";
interface ToolCard {
  provider: Provider;
  configured: boolean;
  keyMasked: string;
  verificationPending?: boolean;
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
    networkMode: "always",
    refetchOnReconnect: "always",
    queryFn: () => dispatch<ToolCards>("tool-cards", { app }),
  });
  const [editing, setEditing] = useState<ToolCard | null>(null);
  const [removing, setRemoving] = useState<ToolCard | null>(null);
  const [replacement, setReplacement] = useState("");
  const needsReplacement =
    removing != null &&
    (removing.provider.id === data?.currentProviderId ||
      removing.inLive === true);
  const alternatives = (data?.cards ?? []).filter(
    (card) =>
      card.provider.id !== removing?.provider.id &&
      card.configured &&
      !card.verificationPending,
  );
  const [keyId, setKeyId] = useState("");
  const [adding, setAdding] = useState(false);
  const [groupId, setGroupId] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [notice, setNotice] = useState("");
  const [imagegenNote, setImagegenNote] = useState("");
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
  const officialCodex =
    app === "codex"
      ? data?.cards.find(
          (card) =>
            card.provider.category === "official" && card.groupId == null,
        )
      : undefined;
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
  function openAddGroup() {
    setError("");
    setGroupId(String(availableGroups[0]?.id ?? ""));
    setAdding(true);
    void refetch();
  }
  async function createCard() {
    if (gate.current || !groupId) return;
    gate.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    setImagegenNote("");
    try {
      await dispatch("create-tool-card", { app, groupId: Number(groupId) });
      await Promise.all([
        refetch(),
        client.invalidateQueries({ queryKey: ["platform-keys"] }),
      ]);
      setAdding(false);
      setNotice("分组已添加。");
    } catch (e) {
      setError(friendlyError(e, "添加分组失败，请重试。"));
    } finally {
      gate.current = false;
      setBusy(false);
    }
  }
  function confirmRemoval(card: ToolCard) {
    setError("");
    setNotice("");
    setReplacement("");
    setRemoving(card);
  }
  async function removeCard() {
    if (!removing || gate.current || (needsReplacement && !replacement)) return;
    gate.current = true;
    setBusy(true);
    setError("");
    try {
      const result = await dispatch<{ warnings?: string[] }>(
        "remove-tool-card",
        {
          app,
          groupId: removing.groupId,
          providerId: removing.provider.id,
          ...(needsReplacement ? { replacementId: replacement } : {}),
        },
      );
      setRemoving(null);
      setNotice(
        ["分组已从此工具移除。", ...(result?.warnings ?? [])].join(" "),
      );
    } catch (e) {
      setError(friendlyError(e, "移除分组失败，请检查当前配置后重试。"));
    } finally {
      await refetch();
      await client.invalidateQueries({ queryKey: ["providers", app] });
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
    setImagegenNote("");
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
      if (start && app === "codex" && card.provider.category !== "official") {
        try {
          const setup = await invoke<{ status: string }>(
            "codex_imagegen_guide",
          );
          setImagegenNote(
            setup.status === "written"
              ? "如需使用命令行生图备用方式，还需在本机安装 Python 3.11+ 和 OpenAI SDK；WSL、远程主机和容器需分别配置。"
              : "命令行生图备用方式未能自动接入，这不代表原生生图不可用。请检查 Codex 的 shell_environment_policy 和 PYTHONPATH 设置。",
          );
        } catch {
          setImagegenNote(
            "命令行生图备用方式的状态不可用，这不代表原生生图不可用。",
          );
        }
      }
      setNotice(
        start
          ? app === "codex" && card.provider.category === "official"
            ? `已切换到 ${card.provider.name}。请重启 Codex；如果官方登录已失效，请在 Codex 中重新登录。`
            : app === "dsh"
              ? `已启用 ${card.provider.name}。可以同时启用多个分组；请在 DeepSeek Harness 中选择模型。`
              : `已切换到 ${card.provider.name}。${result.selectedModel ? `默认模型：${result.selectedModel}。` : ""}${app === "codex" ? "请重启 Codex，使配置生效。" : `在 ${APP_DISPLAY_NAME[app]} 中选择模型。`}`
          : "API Key 已保存。",
      );
      if (result?.warnings?.length) {
        const messages = result.warnings.map((warning) =>
          friendlyError(warning, "部分配置未能确认，请刷新状态后重试。"),
        );
        setNotice((notice) => `${notice} ${[...new Set(messages)].join(" ")}`);
      }
      await Promise.all([
        refetch(),
        client.invalidateQueries({ queryKey: ["providers", app] }),
        client.invalidateQueries({ queryKey: ["proxy"] }),
      ]);
    } catch (e) {
      setError(friendlyError(e, "操作失败，请检查网络后重试。"));
    } finally {
      gate.current = false;
      setBusy(false);
    }
  }
  return (
    <>
      <AppPageHeader
        className={simple ? "aihub-home-header" : undefined}
        title={simple ? "首页" : APP_DISPLAY_NAME[app]}
        variant={simple ? "page" : "app"}
        hideBalance={simple}
        icon={simple ? undefined : <AppGlyph app={app} size={22} />}
        actions={
          <>
            {simple && <HomeWalletBalance onRecharge={wallet?.recharge} />}
            {onOpenProviders && (
              <Button variant="outline" onClick={onOpenProviders}>
                其他供应商
              </Button>
            )}
            {!simple && (
              <Button
                variant="outline"
                disabled={busy || isPending || !!loadError}
                onClick={openAddGroup}
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
        className={`aihub-page-enter min-h-0 flex-1 overflow-y-auto scroll-stable px-6 py-6${simple ? " aihub-home" : ""}`}
      >
        {app === "dsh" && (
          <p className="mb-4 text-body text-fg-3">
            DeepSeek Harness 支持同时启用多个分组，请在 DeepSeek Harness
            中选择模型。
          </p>
        )}
        {simple && (
          <section className="aihub-tool-panel" aria-labelledby="current-tool-name">
            <div className="min-w-0">
              <p className="text-caption font-medium text-fg-3">当前工具</p>
              <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1">
                <h2 id="current-tool-name" className="aihub-tool-name">
                  <AppGlyph app={app} size={32} />
                  {APP_DISPLAY_NAME[app]}
                </h2>
                <Button
                  variant="ghost"
                  size="compact"
                  onClick={() => setSelectingTool(true)}
                  aria-label={`选择 AI 工具，当前为 ${APP_DISPLAY_NAME[app]}`}
                >
                  <ArrowLeftRight aria-hidden="true" className="h-3.5 w-3.5" />
                  切换工具
                </Button>
              </div>
            </div>
            {app === "codex" && <CodexDesktopControls />}
            {app === "claude-desktop" && <ClaudeDesktopControls />}
          </section>
        )}
        {simple && (
          <div className="mb-2 flex flex-wrap items-center justify-between gap-3">
            <div>
              <h2 className="text-title font-semibold">模型分组</h2>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              {officialCodex && (
                <Button
                  variant="outline"
                  disabled={
                    busy ||
                    isPending ||
                    !!loadError ||
                    officialCodex.provider.id === data?.currentProviderId
                  }
                  onClick={() => void operate(officialCodex, true)}
                >
                  {officialCodex.provider.id === data?.currentProviderId
                    ? "当前为官方配置"
                    : "恢复官方登录"}
                </Button>
              )}
              <Button
                variant="outline"
                disabled={busy || isPending || !!loadError}
                onClick={openAddGroup}
              >
                <Plus aria-hidden="true" className="h-4 w-4" />
                添加分组
              </Button>
            </div>
          </div>
        )}
        <div className={simple ? "aihub-group-list" : "space-y-3"}>
          {isPending && (
            <p role="status" className="text-body text-fg-3">
              正在加载模型服务…
            </p>
          )}
          {loadError && (
            <div role="alert" className="text-body text-destructive">
              {friendlyError(loadError, "加载模型服务失败，请重试。")}{" "}
              <Button variant="outline" onClick={() => void refetch()}>
                重试
              </Button>
            </div>
          )}
          {data?.warning && (
            <p role="alert" className="text-body text-fg-3">
              {friendlyError(data.warning, "模型服务暂不可用，请重试。")}{" "}
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
              // The backend supplies the group's current platform separately;
              // public cards deliberately omit provider metadata and secrets.
              // Unknown or conversion platforms must not promise a rewrite.
              const canReapplyCodex =
                current &&
                app === "codex" &&
                card.groupId != null &&
                ["openai", "openai_responses"].includes(
                  card.platform?.toLowerCase() ?? "",
                );
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
                    className="aihub-group-row"
                  >
                    <ProviderIconBox
                      icon={brandIcon ?? card.provider.icon}
                      name={card.provider.name}
                      color={brandIcon ? undefined : card.provider.iconColor}
                      className="h-10 w-10"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
                        <h3 className="truncate text-[15px] font-semibold text-fg-1">
                          {card.provider.name}
                        </h3>
                        {(current || enabled) && (
                          <span className="aihub-active-group text-caption font-medium text-success-text">
                            {enabled ? "已启用" : "使用中"}
                          </span>
                        )}
                      </div>
                      <p className="mt-1 text-caption text-fg-3">
                        {card.verificationPending
                          ? "待联网验证"
                          : card.configured
                            ? "分组已就绪"
                            : "切换时自动配置"}
                      </p>
                    </div>
                    {current || enabled ? (
                      canReapplyCodex && (
                        <Button
                          variant="outline"
                          size="compact"
                          disabled={busy}
                          aria-label={`重新启用 ${card.provider.name}`}
                          onClick={() => void operate(card, true)}
                        >
                          重新启用
                        </Button>
                      )
                    ) : (
                      <Button
                        variant="outline"
                        size="compact"
                        disabled={busy}
                        aria-label={`${app === "dsh" ? "启用" : "切换到"} ${card.provider.name}`}
                        onClick={() => void operate(card, true)}
                      >
                        {app === "dsh" ? "启用" : "切换"}
                      </Button>
                    )}
                    {card.groupId != null && (
                      <Button
                        variant="ghost"
                        size="compact"
                        disabled={busy || dshUnknown}
                        aria-label={`移除 ${card.provider.name}`}
                        onClick={() => confirmRemoval(card)}
                      >
                        移除
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
                    buttons: [
                      ...(canReapplyCodex
                        ? [
                            {
                              key: "switch",
                              label: `重新启用 ${card.provider.name}`,
                              disabledReason: busy ? "正在切换配置" : undefined,
                              onClick: () =>
                                card.configured
                                  ? void operate(card, true)
                                  : edit(card),
                            },
                          ]
                        : current || enabled
                          ? []
                          : [
                              {
                                key: "switch",
                                label: `${app === "dsh" ? "启用" : "启动"} ${card.provider.name}`,
                                disabledReason: busy
                                  ? "正在切换配置"
                                  : undefined,
                                onClick: () =>
                                  card.configured
                                    ? void operate(card, true)
                                    : edit(card),
                              },
                            ]),
                      ...(card.groupId != null
                        ? [
                            {
                              key: "remove-group",
                              label: `移除 ${card.provider.name}`,
                              disabledReason:
                                busy || dshUnknown
                                  ? "请等待配置状态确认"
                                  : undefined,
                              onClick: () => confirmRemoval(card),
                            },
                          ]
                        : []),
                    ],
                    chips:
                      card.provider.category === "official"
                        ? []
                        : [
                            {
                              key: "key",
                              label: card.verificationPending
                                ? "待联网验证"
                                : card.configured
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
                  点「添加分组」选择要使用的模型分组。
                </p>
              </div>
            )}
          {data && !simple && !data.cards.length && (
            <p className="text-body text-fg-3">
              当前暂无适用于此工具的模型服务。
            </p>
          )}
          {error && !editing && !adding && !removing && (
            <p role="alert" className="break-words text-body text-destructive">
              {error}
            </p>
          )}
          {notice && (
            <p role="status" className="text-body text-fg-2">
              {notice}
            </p>
          )}
          {imagegenNote && (
            <details className="text-caption text-fg-3">
              <summary className="cursor-pointer">命令行生图备用方式</summary>
              <p className="mt-1">{imagegenNote}</p>
            </details>
          )}
        </div>
      </main>
      {removing && (
        <Dialog
          open
          onOpenChange={(open) => {
            if (!open && !busy) setRemoving(null);
          }}
        >
          <DialogContent className="max-w-md">
            <DialogHeader>
              <DialogTitle>移除 {removing.provider.name}</DialogTitle>
              <DialogDescription>
                从此工具移除分组后，仍可重新添加，平台 API Key 会保留。
              </DialogDescription>
            </DialogHeader>
            {needsReplacement && (
              <label className="grid gap-2 text-sm">
                替代分组
                <select
                  value={replacement}
                  disabled={busy}
                  onChange={(e) => setReplacement(e.target.value)}
                  className="rounded border bg-background p-2"
                >
                  <option value="">请选择替代分组</option>
                  {alternatives.map((card) => (
                    <option key={card.provider.id} value={card.provider.id}>
                      {card.provider.name}
                    </option>
                  ))}
                </select>
                {!alternatives.length && (
                  <span>请先添加并配置另一个分组，再移除此分组。</span>
                )}
              </label>
            )}
            {error && (
              <p role="alert" className="text-sm text-destructive">
                {error}
              </p>
            )}
            <div className="flex justify-end gap-2">
              <Button
                variant="outline"
                disabled={busy}
                onClick={() => setRemoving(null)}
              >
                取消
              </Button>
              <Button
                disabled={busy || (needsReplacement && !replacement)}
                onClick={() => void removeCard()}
              >
                {needsReplacement ? "切换并移除" : "确认移除"}
              </Button>
            </div>
          </DialogContent>
        </Dialog>
      )}
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
                选择要添加到当前工具的模型分组。添加后可随时切换。
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
                  当前工具暂无可添加的分组。
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
                  {friendlyError(keyQuery.error, "加载 API 列表失败，请重试。")}{" "}
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
