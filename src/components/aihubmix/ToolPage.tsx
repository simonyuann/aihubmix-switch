import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { ArrowLeftRight, Layers, Plus } from "lucide-react";
import { APP_IDS } from "@/config/appConfig";
import { codexImagegenApi } from "@/lib/api/codexImagegen";
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
import { HomeGroupRow } from "./HomeGroupRow";
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
  imageModel?: string | null;
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
  onOpenAnnouncements,
  simple = false,
  onSelectApp,
}: {
  app: AppId;
  onOpenProviders?: () => void;
  onOpenAnnouncements?: () => void;
  simple?: boolean;
  onSelectApp?: (app: AppId) => void;
}) {
  const client = useQueryClient();
  const { t } = useTranslation();
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
      setNotice(t("toolPage.notice.groupAdded"));
    } catch (e) {
      setError(friendlyError(e, t("toolPage.error.addGroup")));
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
        [t("toolPage.notice.groupRemoved"), ...(result?.warnings ?? [])].join(
          " ",
        ),
      );
    } catch (e) {
      setError(friendlyError(e, t("toolPage.error.removeGroup")));
    } finally {
      await refetch();
      await client.invalidateQueries({ queryKey: ["providers", app] });
      gate.current = false;
      setBusy(false);
    }
  }
  async function changeImageModel(card: ToolCard, model: string | null) {
    if (gate.current || card.provider.id !== data?.currentProviderId) return;
    gate.current = true;
    setBusy(true);
    setError("");
    setNotice("");
    try {
      const result = await dispatch<{
        model: string | null;
        restartRequired: boolean;
      }>("set-tool-image-model", {
        app,
        groupId: card.groupId,
        providerId: card.provider.id,
        model,
      });
      client.setQueryData<ToolCards>(
        ["tool-cards", app],
        (old) =>
          old && {
            ...old,
            cards: old.cards.map((item) =>
              item.provider.id === card.provider.id
                ? { ...item, imageModel: result.model }
                : item,
            ),
          },
      );
      setNotice(
        result.restartRequired
          ? t("toolPage.notice.imagegenSavedRestart")
          : t("toolPage.notice.imagegenSaved"),
      );
    } catch (e) {
      setError(friendlyError(e, t("toolPage.error.saveImagegen")));
      await client.invalidateQueries({
        queryKey: ["tool-image-models", card.provider.id],
      });
    } finally {
      await refetch();
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
          const setup = await codexImagegenApi.getSetupGuide();
          setImagegenNote(
            setup.status === "written"
              ? t("toolPage.notice.imagegenManualRequired")
              : t("toolPage.notice.imagegenManualFailed"),
          );
        } catch {
          setImagegenNote(t("toolPage.notice.imagegenStatusUnavailable"));
        }
      }
      setNotice(
        start
          ? app === "codex" && card.provider.category === "official"
            ? t("toolPage.notice.switchedOfficialRestart", {
                name: card.provider.name,
              })
            : app === "dsh"
              ? t("toolPage.notice.dshMultiEnabled", {
                  name: card.provider.name,
                })
              : t("toolPage.notice.switchedTo", { name: card.provider.name }) +
                (result.selectedModel
                  ? t("toolPage.notice.defaultModelSet", {
                      model: result.selectedModel,
                    })
                  : "") +
                (app === "codex"
                  ? t("toolPage.notice.restartCodex")
                  : t("toolPage.notice.pickModelInApp", {
                      app: APP_DISPLAY_NAME[app],
                    }))
          : t("toolPage.notice.apiKeySaved"),
      );
      if (result?.warnings?.length) {
        const messages = result.warnings.map((warning) =>
          friendlyError(warning, t("toolPage.error.partialConfig")),
        );
        setNotice((notice) => `${notice} ${[...new Set(messages)].join(" ")}`);
      }
      await Promise.all([
        refetch(),
        client.invalidateQueries({ queryKey: ["providers", app] }),
        client.invalidateQueries({ queryKey: ["proxy"] }),
      ]);
    } catch (e) {
      setError(friendlyError(e, t("toolPage.error.operationFailed")));
    } finally {
      gate.current = false;
      setBusy(false);
    }
  }
  return (
    <>
      <AppPageHeader
        className={simple ? "aihub-home-header" : undefined}
        title={simple ? t("toolPage.title.home") : APP_DISPLAY_NAME[app]}
        variant={simple ? "page" : "app"}
        icon={simple ? undefined : <AppGlyph app={app} size={22} />}
        actions={
          <>
            {onOpenProviders && (
              <Button variant="outline" onClick={onOpenProviders}>
                {t("toolPage.action.openProviders")}
              </Button>
            )}
            {!simple && (
              <Button
                variant="outline"
                disabled={busy || isPending || !!loadError}
                onClick={openAddGroup}
              >
                {t("toolPage.action.addGroup")}
              </Button>
            )}
            {!simple && (
              <Button onClick={() => setApply(true)}>
                {t("toolPage.action.applyApi")}
              </Button>
            )}
          </>
        }
      />
      {simple && onOpenAnnouncements && (
        <AnnouncementBanner onOpen={onOpenAnnouncements} />
      )}
      <main
        id="main-content"
        className={`aihub-page-enter min-h-0 flex-1 overflow-y-auto scroll-stable px-6 py-6${simple ? " aihub-home" : ""}`}
      >
        {app === "dsh" && (
          <p className="mb-4 text-body text-fg-3">
            {t("toolPage.hint.dshMulti")}
          </p>
        )}
        {simple && (
          <section
            className="aihub-tool-panel"
            aria-labelledby="current-tool-name"
          >
            <div className="min-w-0">
              <p className="text-caption font-medium text-fg-3">
                {t("toolPage.label.currentTool")}
              </p>
              <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1">
                <h2 id="current-tool-name" className="aihub-tool-name">
                  <AppGlyph app={app} size={32} />
                  {APP_DISPLAY_NAME[app]}
                </h2>
                <Button
                  variant="ghost"
                  size="compact"
                  onClick={() => setSelectingTool(true)}
                  aria-label={t("toolPage.aria.selectTool", {
                    name: APP_DISPLAY_NAME[app],
                  })}
                >
                  <ArrowLeftRight aria-hidden="true" className="h-3.5 w-3.5" />
                  {t("toolPage.action.switchTool")}
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
              <h2 className="text-title font-semibold">
                {t("toolPage.section.groups")}
              </h2>
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
                    ? t("toolPage.action.officialCurrent")
                    : t("toolPage.action.restoreOfficial")}
                </Button>
              )}
              <Button
                variant="outline"
                disabled={busy || isPending || !!loadError}
                onClick={openAddGroup}
              >
                <Plus aria-hidden="true" className="h-4 w-4" />
                {t("toolPage.action.addGroup")}
              </Button>
            </div>
          </div>
        )}
        <div className={simple ? "aihub-group-list" : "space-y-3"}>
          {isPending && (
            <p role="status" className="text-body text-fg-3">
              {t("toolPage.status.loadingGroups")}
            </p>
          )}
          {loadError && (
            <div role="alert" className="text-body text-destructive">
              {friendlyError(loadError, t("toolPage.error.loadGroups"))}{" "}
              <Button variant="outline" onClick={() => void refetch()}>
                {t("toolPage.action.retry")}
              </Button>
            </div>
          )}
          {data?.warning && (
            <p role="alert" className="text-body text-fg-3">
              {friendlyError(
                data.warning,
                t("toolPage.error.groupsUnavailable"),
              )}{" "}
              <Button variant="outline" onClick={() => void refetch()}>
                {t("toolPage.action.retry")}
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
                // An offline card may omit its platform. The backend only includes
                // imageModel on Codex cards that support image configuration.
                const { groupId, imageModel } = card;
                const hasImageSettings =
                  current &&
                  app === "codex" &&
                  groupId != null &&
                  imageModel !== undefined;
                return (
                  <HomeGroupRow
                    key={`${card.provider.id}:${hasImageSettings}`}
                    name={card.provider.name}
                    icon={
                      <ProviderIconBox
                        icon={brandIcon ?? card.provider.icon}
                        name={card.provider.name}
                        color={brandIcon ? undefined : card.provider.iconColor}
                        className="h-10 w-10"
                      />
                    }
                    activeLabel={
                      enabled
                        ? t("toolPage.status.enabled")
                        : current
                          ? t("toolPage.status.inUse")
                          : undefined
                    }
                    status={
                      card.verificationPending
                        ? t("toolPage.status.verificationPending")
                        : card.configured
                          ? t("toolPage.status.ready")
                          : t("toolPage.status.autoConfig")
                    }
                    modelSettings={
                      hasImageSettings
                        ? {
                            providerId: card.provider.id,
                            groupId,
                            keyId: card.selectedKeyId,
                            model: imageModel,
                            disabled: busy,
                            onChange: (model) =>
                              void changeImageModel(card, model),
                          }
                        : undefined
                    }
                  >
                    {current || enabled ? (
                      canReapplyCodex && (
                        <Button
                          variant="outline"
                          size="compact"
                          disabled={busy}
                          aria-label={t("toolPage.aria.reEnable", {
                            name: card.provider.name,
                          })}
                          onClick={() => void operate(card, true)}
                        >
                          {t("toolPage.action.reEnable")}
                        </Button>
                      )
                    ) : (
                      <Button
                        variant="outline"
                        size="compact"
                        disabled={busy}
                        aria-label={t(
                          app === "dsh"
                            ? "toolPage.aria.enable"
                            : "toolPage.aria.switchTo",
                          { name: card.provider.name },
                        )}
                        onClick={() => void operate(card, true)}
                      >
                        {app === "dsh"
                          ? t("toolPage.action.enable")
                          : t("toolPage.action.switch")}
                      </Button>
                    )}
                    {card.groupId != null && (
                      <Button
                        variant="ghost"
                        size="compact"
                        disabled={busy || dshUnknown}
                        aria-label={t("toolPage.aria.remove", {
                          name: card.provider.name,
                        })}
                        onClick={() => confirmRemoval(card)}
                      >
                        {t("toolPage.action.remove")}
                      </Button>
                    )}
                    {dshUnknown && (
                      <span className="text-caption text-fg-3">
                        {t("toolPage.status.unknown")}
                      </span>
                    )}
                  </HomeGroupRow>
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
                            label: enabled
                              ? t("toolPage.status.enabled")
                              : t("toolPage.status.inUse"),
                            dot: "direct",
                          }
                        : dshUnknown
                          ? {
                              label: t("toolPage.status.unknown"),
                              dot: "muted",
                            }
                          : undefined,
                    buttons: [
                      ...(canReapplyCodex
                        ? [
                            {
                              key: "switch",
                              label: t("toolPage.aria.reEnable", {
                                name: card.provider.name,
                              }),
                              disabledReason: busy
                                ? t("toolPage.status.switchingConfig")
                                : undefined,
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
                                label: t(
                                  app === "dsh"
                                    ? "toolPage.aria.enable"
                                    : "toolPage.aria.start",
                                  { name: card.provider.name },
                                ),
                                disabledReason: busy
                                  ? t("toolPage.status.switchingConfig")
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
                              label: t("toolPage.aria.remove", {
                                name: card.provider.name,
                              }),
                              disabledReason:
                                busy || dshUnknown
                                  ? t("toolPage.error.waitStateConfirm")
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
                                ? t("toolPage.status.verificationPending")
                                : card.configured
                                  ? card.keyMasked ||
                                    t("toolPage.status.configured")
                                  : t("toolPage.status.keyNotSelected"),
                              tone: "outline",
                            },
                          ],
                    editDisabledReason: busy
                      ? t("toolPage.status.savingConfig")
                      : undefined,
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
                <p className="font-semibold">
                  {t("toolPage.empty.noGroupsTitle")}
                </p>
                <p className="text-body text-fg-3">
                  {t("toolPage.empty.noGroupsHint")}
                </p>
              </div>
            )}
          {data && !simple && !data.cards.length && (
            <p className="text-body text-fg-3">
              {t("toolPage.empty.noServices")}
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
              <summary className="cursor-pointer">
                {t("toolPage.summary.imagegenFallback")}
              </summary>
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
              <DialogTitle>
                {t("toolPage.dialog.removeTitle", {
                  name: removing.provider.name,
                })}
              </DialogTitle>
              <DialogDescription>
                {t("toolPage.dialog.removeDescription")}
              </DialogDescription>
            </DialogHeader>
            {needsReplacement && (
              <label className="grid gap-2 text-sm">
                {t("toolPage.dialog.replacementGroup")}
                <select
                  value={replacement}
                  disabled={busy}
                  onChange={(e) => setReplacement(e.target.value)}
                  className="rounded border bg-background p-2"
                >
                  <option value="">
                    {t("toolPage.dialog.selectReplacement")}
                  </option>
                  {alternatives.map((card) => (
                    <option key={card.provider.id} value={card.provider.id}>
                      {card.provider.name}
                    </option>
                  ))}
                </select>
                {!alternatives.length && (
                  <span>{t("toolPage.dialog.replacementRequired")}</span>
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
                {t("toolPage.action.cancel")}
              </Button>
              <Button
                disabled={busy || (needsReplacement && !replacement)}
                onClick={() => void removeCard()}
              >
                {needsReplacement
                  ? t("toolPage.action.switchAndRemove")
                  : t("toolPage.action.confirmRemove")}
              </Button>
            </div>
          </DialogContent>
        </Dialog>
      )}
      {selectingTool && (
        <Dialog open onOpenChange={setSelectingTool}>
          <DialogContent className="w-[calc(100vw-2rem)] max-w-2xl">
            <DialogHeader>
              <DialogTitle>{t("toolPage.dialog.selectToolTitle")}</DialogTitle>
              <DialogDescription>
                {t("toolPage.dialog.selectToolDescription")}
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
                    <span className="text-caption text-action-text">
                      {t("toolPage.status.current")}
                    </span>
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
              <DialogTitle>{t("toolPage.action.addGroup")}</DialogTitle>
              <DialogDescription>
                {t("toolPage.dialog.addDescription")}
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
                <span>{t("toolPage.dialog.platformGroup")}</span>
                <select
                  aria-label={t("toolPage.dialog.platformGroup")}
                  className="h-10 w-full rounded-control border border-border bg-surface px-3"
                  value={groupId}
                  onChange={(e) => setGroupId(e.target.value)}
                  disabled={busy || !groups.length}
                >
                  <option value="">{t("toolPage.dialog.selectGroup")}</option>
                  {groups.map((group) => (
                    <option
                      key={group.id}
                      value={group.id}
                      disabled={groupAdded(group.id)}
                    >
                      {group.name}
                      {groupAdded(group.id)
                        ? t("toolPage.dialog.alreadyAdded")
                        : ""}
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
                {t("toolPage.action.refreshGroups")}
              </Button>
              {!availableGroups.length && (
                <p className="text-body text-fg-3">
                  {t("toolPage.dialog.noAvailableGroups")}
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
                  {t("toolPage.action.cancel")}
                </Button>
                <Button
                  aria-label={t("toolPage.aria.confirmAdd")}
                  disabled={
                    busy ||
                    !availableGroups.some(
                      (group) => String(group.id) === groupId,
                    )
                  }
                >
                  {busy
                    ? t("toolPage.status.adding")
                    : t("toolPage.action.addGroup")}
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
                  ? t("toolPage.dialog.keyDescriptionDsh")
                  : t("toolPage.dialog.keyDescription")}
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
                  <option value="">{t("toolPage.dialog.selectApiKey")}</option>
                  {keys.map((key) => (
                    <option key={key.id} value={key.id}>
                      {key.name} · {key.keyMasked}
                    </option>
                  ))}
                </select>
              </label>
              {keyQuery.isPending && (
                <p role="status" className="text-body text-fg-3">
                  {t("toolPage.status.loadingKeys")}
                </p>
              )}
              {keyQuery.error && (
                <p role="alert" className="text-body text-destructive">
                  {friendlyError(keyQuery.error, t("toolPage.error.loadKeys"))}{" "}
                  <Button
                    type="button"
                    variant="outline"
                    onClick={() => void keyQuery.refetch()}
                  >
                    {t("toolPage.action.retry")}
                  </Button>
                </p>
              )}
              {!keyQuery.isPending && !keyQuery.error && !keys.length && (
                <p className="text-body text-fg-3">
                  {t("toolPage.dialog.noKeys")}
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
                  {t("toolPage.action.cancel")}
                </Button>
                <Button
                  variant="outline"
                  disabled={
                    busy || !keys.some((key) => String(key.id) === keyId)
                  }
                >
                  {t("toolPage.action.save")}
                </Button>
                <Button
                  type="button"
                  disabled={
                    busy || !keys.some((key) => String(key.id) === keyId)
                  }
                  onClick={() => void operate(editing, true, keyId)}
                >
                  {busy
                    ? t("toolPage.status.processing")
                    : app === "dsh"
                      ? t("toolPage.action.saveAndEnable")
                      : t("toolPage.action.saveAndStart")}
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
