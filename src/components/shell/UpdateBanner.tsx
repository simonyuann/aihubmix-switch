import { ArrowUpCircle, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { useUpdate } from "@/contexts/UpdateContext";
import { Button } from "@/components/ui/button";

export function UpdateBanner({ onOpenUpdate }: { onOpenUpdate: () => void }) {
  const { t } = useTranslation();
  const { hasUpdate, updateInfo, isDismissed, dismissUpdate } = useUpdate();

  if (!hasUpdate || !updateInfo || isDismissed) return null;

  return (
    <div
      role="status"
      className="flex flex-wrap items-center gap-3 border-b border-border bg-surface px-5 py-3"
    >
      <ArrowUpCircle
        className="h-4 w-4 shrink-0 text-action"
        aria-hidden="true"
      />
      <span className="min-w-0 flex-1 text-body text-fg-1">
        {t("settings.updateAvailable", {
          version: updateInfo.availableVersion,
        })}
      </span>
      <Button
        type="button"
        variant="solid"
        size="compact"
        onClick={onOpenUpdate}
      >
        {t("settings.updateTo", { version: updateInfo.availableVersion })}
      </Button>
      <button
        type="button"
        onClick={dismissUpdate}
        aria-label={t("common.close")}
        className="rounded p-1 text-fg-2 hover:text-fg-1"
      >
        <X className="h-4 w-4" aria-hidden="true" />
      </button>
    </div>
  );
}
