import { useTranslation } from "react-i18next";
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
  const { t } = useTranslation();
  const mib = (bytes: number) => (bytes / 1024 / 1024).toFixed(1);
  const label = !progress
    ? t("codexMarket.progress.preparing")
    : progress.phase === "checking"
      ? t("codexMarket.progress.preparing")
      : progress.phase === "downloading"
        ? !progress.total
          ? t("codexMarket.progress.downloadingNoTotal", {
              downloaded: mib(progress.downloaded),
            })
          : t("codexMarket.progress.downloadingWithTotal", {
              downloaded: mib(progress.downloaded),
              total: mib(progress.total),
              percent,
            })
        : progress.phase === "verifying"
          ? t("codexMarket.progress.installing")
          : t("codexMarket.progress.installing");
  return (
    <div
      role="status"
      aria-live="polite"
      className="space-y-2 rounded-control bg-subtle px-3 py-2 text-caption text-fg-2"
    >
      <p>{label}</p>
      {downloading && (
        <progress
          aria-label={t("codexMarket.aria.downloadProgress")}
          value={progress.downloaded}
          max={progress.total}
          className="h-2 w-full accent-primary"
        />
      )}
    </div>
  );
}
