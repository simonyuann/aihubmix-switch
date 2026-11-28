import { useEffect, useRef, useState } from "react";
import { Check, Loader2, Play, RotateCcw } from "lucide-react";
import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import {
  claudeDesktop,
  claudeDesktopError,
  setClaudeDesktopPreferences,
  useClaudeDesktopPreferences,
  type ClaudeLaunchPhase,
  type ClaudeLaunchProgress,
} from "@/lib/claudeDesktop";
import { isWindows, isMac } from "@/lib/platform";
import { useTranslation } from "react-i18next";

const phaseLabels: Record<ClaudeLaunchPhase, string> = {
  preparing: "claudeDesktop.phase.preparing",
  authorizing: "claudeDesktop.phase.authorizing",
  checking_backup: "claudeDesktop.phase.checking_backup",
  cleaning: "claudeDesktop.phase.cleaning",
  closing: "claudeDesktop.phase.closing",
  permissions: "claudeDesktop.phase.permissions",
  copying: "claudeDesktop.phase.copying",
  extracting: "claudeDesktop.phase.extracting",
  translating: "claudeDesktop.phase.translating",
  resources: "claudeDesktop.phase.resources",
  packing: "claudeDesktop.phase.packing",
  signing: "claudeDesktop.phase.signing",
  verifying: "claudeDesktop.phase.verifying",
  replacing: "claudeDesktop.phase.replacing",
  restoring: "claudeDesktop.phase.restoring",
  launching: "claudeDesktop.phase.launching",
  completed: "claudeDesktop.phase.completed",
  cancelled: "claudeDesktop.phase.cancelled",
};

export function ClaudeDesktopControls({
  showRestore = false,
}: {
  showRestore?: boolean;
}) {
  const preferences = useClaudeDesktopPreferences();
  const { t } = useTranslation();
  const [pending, setPending] = useState<"chinese" | "restore" | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState("");
  const gate = useRef(false);
  const [status, setStatus] = useState<ClaudeLaunchProgress>({
    phase: "preparing",
    interruptible: false,
  });
  const [steps, setSteps] = useState<ClaudeLaunchPhase[]>([]);
  const [stopping, setStopping] = useState(false);
  const [canCancel, setCanCancel] = useState(false);
  const [failed, setFailed] = useState(false);
  const operationId = useRef("");
  const lastActivity = useRef(Date.now());
  const previousStatus = useRef("");
  const [elapsed, setElapsed] = useState(0);
  const [idleSeconds, setIdleSeconds] = useState(0);
  useEffect(() => {
    if (!busy) return;
    const started = Date.now();
    const timer = window.setInterval(() => {
      setElapsed(Math.floor((Date.now() - started) / 1000));
      setIdleSeconds(Math.floor((Date.now() - lastActivity.current) / 1000));
    }, 1000);
    return () => window.clearInterval(timer);
  }, [busy]);
  const phase = status.phase;
  const phaseLabel = phaseLabels[phase];
  const counted =
    status.total != null && status.total > 0 && status.completed != null;
  const percent = counted
    ? Math.floor((status.completed! / status.total!) * 100)
    : undefined;

  async function cancel() {
    if (stopping) return;
    setStopping(true);
    try {
      await claudeDesktop.cancel(operationId.current);
    } catch {
      setStopping(false);
      setMessage(t("claudeDesktop.message.cancelFailed"));
    }
  }

  const progress = (
    <div
      className="space-y-4 rounded-lg border border-border bg-subtle p-4"
      aria-busy={busy}
    >
      {steps.length > 1 && (
        <ol
          className="space-y-1.5 text-caption text-fg-3"
          aria-label={t("claudeDesktop.aria.steps")}
        >
          {steps.slice(-5, -1).map((step, index) => (
            <li key={`${step}-${index}`} className="flex items-center gap-2">
              <Check className="h-3.5 w-3.5 text-success" aria-hidden="true" />
              {t(phaseLabels[step])}
            </li>
          ))}
        </ol>
      )}
      <div className="flex items-center justify-between gap-4 text-sm">
        <span role="status" className="font-medium text-fg-1">
          {failed ? t("claudeDesktop.status.failedPrefix") : ""}
          {t(phaseLabel)}
        </span>
        <span className="shrink-0 tabular-nums text-fg-3">
          {t("claudeDesktop.progress.elapsed", {
            minutes: Math.floor(elapsed / 60),
            seconds: elapsed % 60,
          })}
        </span>
      </div>
      <div
        role="progressbar"
        aria-label={t(phaseLabel)}
        aria-valuenow={percent}
        aria-valuemin={counted ? 0 : undefined}
        aria-valuemax={counted ? 100 : undefined}
        className="h-1.5 overflow-hidden rounded-full bg-action-soft"
      >
        <div
          className={`h-full rounded-full bg-action ${!counted && busy ? "motion-safe:animate-pulse" : ""}`}
          style={{ width: counted ? `${percent}%` : "33%" }}
        />
      </div>
      {counted && (
        <p className="text-caption tabular-nums text-fg-2">
          {t("claudeDesktop.progress.countedLine", {
            label: t(
              phase === "signing"
                ? "claudeDesktop.progress.signed"
                : "claudeDesktop.progress.processed",
            ),
            completed: status.completed,
            total: status.total,
            percent,
          })}
        </p>
      )}
      <p className="text-caption text-fg-3" aria-live="polite">
        {stopping
          ? phase === "authorizing"
            ? t("claudeDesktop.progress.cancelRecordedAuth")
            : status.interruptible
              ? t("claudeDesktop.progress.cancellingCleanup")
              : t("claudeDesktop.progress.safeStop")
          : phase === "authorizing"
            ? t("claudeDesktop.progress.authorizeHint")
            : failed
              ? t("claudeDesktop.progress.retryHint")
              : t("claudeDesktop.progress.autoNext")}
      </p>
      {busy && phase !== "authorizing" && (
        <p className="text-caption tabular-nums text-fg-3">
          {t("claudeDesktop.progress.idle", { seconds: idleSeconds })}
        </p>
      )}
      {busy && (
        <Button
          size="compact"
          variant="outline"
          disabled={stopping || !canCancel}
          onClick={() => void cancel()}
        >
          {stopping
            ? t("claudeDesktop.action.stopping")
            : t("claudeDesktop.action.cancelOperation")}
        </Button>
      )}
    </div>
  );

  async function run(
    action: "chinese" | "launch" | "restore",
    consent = false,
  ) {
    if (gate.current) return;
    gate.current = true;
    setBusy(true);
    const id = crypto.randomUUID();
    operationId.current = id;
    setStatus({ phase: "preparing", interruptible: false });
    setSteps(["preparing"]);
    setStopping(false);
    setCanCancel(false);
    setFailed(false);
    lastActivity.current = Date.now();
    previousStatus.current = "";
    setIdleSeconds(0);
    setElapsed(0);
    setMessage("");
    try {
      const result = await claudeDesktop.run(
        preferences.executablePath,
        action,
        consent,
        (next) => {
          if (operationId.current !== id) return;
          const key = JSON.stringify(next);
          if (previousStatus.current !== key) {
            lastActivity.current = Date.now();
            setIdleSeconds(0);
            previousStatus.current = key;
          }
          setCanCancel(
            next.phase !== "launching" && next.phase !== "cancelled",
          );
          setStatus(next);
          setSteps((current) =>
            current.at(-1) === next.phase ? current : [...current, next.phase],
          );
        },
        id,
      );
      setPending(null);
      if (result === "stopped_after_install") {
        setMessage(t("claudeDesktop.message.stoppedAfterInstall"));
      } else if (action === "restore") {
        const saved = setClaudeDesktopPreferences({ language: "system" });
        setMessage(
          saved
            ? t("claudeDesktop.message.restoredKept")
            : t("claudeDesktop.message.restoredNotSaved"),
        );
      } else {
        setMessage(
          action === "chinese"
            ? t("claudeDesktop.message.chineseReady")
            : t("claudeDesktop.message.launchRequested"),
        );
      }
    } catch (error) {
      if (error === "consent_required")
        setPending(action === "restore" ? "restore" : "chinese");
      else {
        setPending(null);
        setFailed(error !== "cancelled");
        setMessage(claudeDesktopError(error));
      }
    } finally {
      gate.current = false;
      setBusy(false);
    }
  }

  return (
    <div className="mt-3 space-y-2">
      <div className="flex flex-wrap items-center gap-2">
        <Button
          size="compact"
          disabled={busy}
          onClick={() =>
            void run(preferences.language === "zh-CN" ? "chinese" : "launch")
          }
        >
          {busy ? (
            <Loader2 className="mr-2 h-4 w-4 animate-spin" />
          ) : (
            <Play className="mr-2 h-4 w-4" />
          )}
          {busy
            ? phaseLabel
            : preferences.language === "zh-CN"
              ? t("claudeDesktop.action.chineseStart")
              : t("claudeDesktop.action.start")}
        </Button>
        {showRestore && (
          <Button
            size="compact"
            variant="outline"
            disabled={busy}
            onClick={() => setPending("restore")}
          >
            <RotateCcw className="mr-2 h-4 w-4" />
            {t("claudeDesktop.action.restore")}
          </Button>
        )}
      </div>
      {(busy || failed) && pending === null && progress}
      {message && (
        <p role="status" className="text-caption text-fg-2">
          {message}
        </p>
      )}
      <Dialog
        open={pending !== null}
        onOpenChange={(open) => {
          if (!open && !busy) setPending(null);
        }}
      >
        <DialogContent
          className="overflow-y-auto"
          onEscapeKeyDown={(e) => {
            if (busy) e.preventDefault();
          }}
        >
          <DialogHeader>
            <DialogTitle>
              {pending === "restore"
                ? t("claudeDesktop.dialog.restoreTitle")
                : t("claudeDesktop.dialog.chineseTitle")}
            </DialogTitle>
            <DialogDescription>
              {busy
                ? t("claudeDesktop.dialog.preparing")
                : pending === "restore"
                  ? t("claudeDesktop.dialog.restoreDescription")
                  : t("claudeDesktop.dialog.chineseDescription")}
            </DialogDescription>
          </DialogHeader>
          {busy && <div className="px-6 pb-4">{progress}</div>}
          {!busy && pending === "chinese" && (
            <div className="space-y-3 px-6 pb-4 text-body text-fg-2">
              <p>
                {isWindows()
                  ? t("claudeDesktop.dialog.windowsNote")
                  : isMac()
                    ? t("claudeDesktop.dialog.macNote")
                    : t("claudeDesktop.dialog.linuxNote")}
              </p>
              <p>{t("claudeDesktop.dialog.relocalizeNote")}</p>
            </div>
          )}
          {!busy && (
            <DialogFooter>
              <Button
                variant="outline"
                disabled={busy}
                onClick={() => setPending(null)}
              >
                {t("claudeDesktop.action.cancel")}
              </Button>
              <Button
                disabled={busy}
                onClick={() => pending && void run(pending, true)}
              >
                {busy
                  ? t("claudeDesktop.status.processing")
                  : pending === "restore"
                    ? t("claudeDesktop.action.restore")
                    : t("claudeDesktop.action.agreeAndStart")}
              </Button>
            </DialogFooter>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
