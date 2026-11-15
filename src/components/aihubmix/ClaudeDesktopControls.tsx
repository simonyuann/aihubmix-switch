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

const phaseLabels: Record<ClaudeLaunchPhase, string> = {
  preparing: "检查应用与语言资源",
  authorizing: "等待系统授权",
  checking_backup: "检查旧补丁与备份",
  cleaning: "清理旧补丁组件",
  closing: "关闭 Claude 与后台服务",
  permissions: "准备应用目录写入权限",
  copying: "创建应用副本",
  extracting: "解包界面资源",
  translating: "汉化界面文字",
  resources: "写入中文语言资源",
  packing: "重新打包与更新完整性信息",
  signing: "重新签名应用",
  verifying: "校验应用文件",
  replacing: "备份并安装应用",
  restoring: "恢复原版文件",
  launching: "启动 Claude Desktop",
  completed: "补丁处理完成",
  cancelled: "已取消",
};

export function ClaudeDesktopControls({
  showRestore = false,
}: {
  showRestore?: boolean;
}) {
  const preferences = useClaudeDesktopPreferences();
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
      setMessage("取消请求未能送达，请重试。");
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
          aria-label="处理步骤"
        >
          {steps.slice(-5, -1).map((step, index) => (
            <li key={`${step}-${index}`} className="flex items-center gap-2">
              <Check className="h-3.5 w-3.5 text-success" aria-hidden="true" />
              {phaseLabels[step]}
            </li>
          ))}
        </ol>
      )}
      <div className="flex items-center justify-between gap-4 text-sm">
        <span role="status" className="font-medium text-fg-1">
          {failed ? "未完成 · " : ""}
          {phaseLabel}
        </span>
        <span className="shrink-0 tabular-nums text-fg-3">
          已用 {Math.floor(elapsed / 60)}分{elapsed % 60}秒
        </span>
      </div>
      <div
        role="progressbar"
        aria-label={phaseLabel}
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
          {phase === "signing" ? "已签名" : "已处理"} {status.completed} /{" "}
          {status.total} 项 · 当前阶段 {percent}%
        </p>
      )}
      <p className="text-caption text-fg-3" aria-live="polite">
        {stopping
          ? phase === "authorizing"
            ? "取消请求已记录，请关闭系统授权弹窗。"
            : status.interruptible
              ? "正在取消并清理临时文件…"
              : "正在安全停止。当前写入完成后结束，并停止自动启动。"
          : phase === "authorizing"
            ? "请在系统弹窗中允许此操作。也可以在系统弹窗中取消授权。"
            : failed
              ? "可重试此操作，或在设置中恢复原版。"
              : "处理完成后自动进入下一步。"}
      </p>
      {busy && phase !== "authorizing" && (
        <p className="text-caption tabular-nums text-fg-3">
          任务运行中 · 距上次进展 {idleSeconds} 秒
        </p>
      )}
      {busy && (
        <Button
          size="compact"
          variant="outline"
          disabled={stopping || !canCancel}
          onClick={() => void cancel()}
        >
          {stopping ? "正在安全停止…" : "取消操作"}
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
        setMessage("补丁处理已完成，已按要求停止自动启动。");
      } else if (action === "restore") {
        const saved = setClaudeDesktopPreferences({ language: "system" });
        setMessage(
          saved
            ? "已恢复原版，之后将保留 Claude 自身的语言设置。"
            : "已恢复原版，但语言偏好未能保存，请重新设置。",
        );
      } else {
        setMessage(
          action === "chinese"
            ? "中文补丁已就绪，已发送启动请求。"
            : "已发送启动请求。",
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
              ? "中文启动 Claude Desktop"
              : "启动 Claude Desktop"}
        </Button>
        {showRestore && (
          <Button
            size="compact"
            variant="outline"
            disabled={busy}
            onClick={() => setPending("restore")}
          >
            <RotateCcw className="mr-2 h-4 w-4" />
            恢复原版
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
                ? "恢复 Claude Desktop 原版"
                : "安装 Claude Desktop 中文补丁"}
            </DialogTitle>
            <DialogDescription>
              {busy
                ? "正在准备中文界面，可查看当前步骤或取消操作。"
                : pending === "restore"
                  ? "将关闭 Claude Desktop，并恢复与当前版本匹配的应用备份。账号和会话不会删除。"
                  : "使用开源项目 claude-desktop-zh-cn 的离线汉化资源。首次安装可能需要几分钟，安装前备份应用文件，完成后自动启动；可在设置中恢复原版。"}
            </DialogDescription>
          </DialogHeader>
          {busy && <div className="px-6 pb-4">{progress}</div>}
          {!busy && pending === "chinese" && (
            <div className="space-y-3 px-6 pb-4 text-body text-fg-2">
              <p>
                {isWindows()
                  ? "Windows 新版补丁会修改 Claude.exe 的完整性哈希，使原程序签名失效；同时修改 Cowork 服务，降低对客户端签名的校验。"
                  : isMac()
                    ? "macOS 补丁会修改应用资源并重新进行本地签名，替换原有开发者签名。"
                    : "Linux 补丁会修改 Claude Desktop 的应用资源，需要系统管理员授权。目前支持官方 deb 安装版。"}
              </p>
              <p>
                Claude
                更新后可能需要重新汉化。官方账号界面已有原生中文；本补丁主要用于第三方
                API 模式。
              </p>
            </div>
          )}
          {!busy && (
            <DialogFooter>
              <Button
                variant="outline"
                disabled={busy}
                onClick={() => setPending(null)}
              >
                取消
              </Button>
              <Button
                disabled={busy}
                onClick={() => pending && void run(pending, true)}
              >
                {busy
                  ? "正在处理…"
                  : pending === "restore"
                    ? "恢复原版"
                    : "同意并中文启动"}
              </Button>
            </DialogFooter>
          )}
        </DialogContent>
      </Dialog>
    </div>
  );
}
