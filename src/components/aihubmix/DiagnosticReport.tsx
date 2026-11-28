import { Button } from "@/components/ui/button";
import { useTranslation } from "react-i18next";
import { copyText } from "@/lib/clipboard";

export function DiagnosticReport({
  report,
  onNotice,
}: {
  report: string;
  onNotice: (message: string) => void;
}) {
  const { t } = useTranslation();
  return (
    <div>
      <textarea
        aria-label={t("diagnosticReport.aria.report")}
        readOnly
        rows={5}
        className="w-full select-text rounded border p-2 text-start text-sm"
        value={report}
        onFocus={(e) => e.target.select()}
      />
      <Button
        variant="ghost"
        onClick={() =>
          void copyText(report).then(
            () => onNotice(t("diagnosticReport.toast.copied")),
            () => onNotice(t("diagnosticReport.toast.copyFailed")),
          )
        }
      >
        {t("diagnosticReport.action.copy")}
      </Button>
    </div>
  );
}
