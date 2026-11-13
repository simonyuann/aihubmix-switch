import type { CodexMarketProgress as Progress } from "@/lib/codexPluginMarket";

export function CodexMarketProgress({
  progress,
}: {
  progress: Progress | null;
}) {
  const downloading = progress?.phase === "downloading" && progress.total > 0;
  const percent = downloading
    ? Math.min(
        100,
        Math.max(0, Math.floor((progress.downloaded / progress.total) * 100)),
      )
    : 0;
  const mib = (bytes: number) => (bytes / 1024 / 1024).toFixed(1);
  const label = !progress
    ? "正在准备插件库…"
    : progress.phase === "checking"
      ? "正在准备插件库…"
      : progress.phase === "downloading"
        ? `正在下载插件库：${mib(progress.downloaded)} / ${mib(progress.total)} MiB（${percent}%）`
        : progress.phase === "verifying"
          ? "正在安装插件库，请稍候…"
          : "正在安装插件库，请稍候…";
  return (
    <div
      role="status"
      aria-live="polite"
      className="space-y-2 rounded-control bg-subtle px-3 py-2 text-caption text-fg-2"
    >
      <p>{label}</p>
      {downloading && (
        <progress
          aria-label="插件库下载进度"
          value={progress.downloaded}
          max={progress.total}
          className="h-2 w-full accent-primary"
        />
      )}
    </div>
  );
}
