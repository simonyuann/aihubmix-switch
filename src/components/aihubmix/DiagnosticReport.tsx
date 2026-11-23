import { Button } from "@/components/ui/button";
import { copyText } from "@/lib/clipboard";

export function DiagnosticReport({
  report,
  onNotice,
}: {
  report: string;
  onNotice: (message: string) => void;
}) {
  return (
    <div>
      <textarea
        aria-label="网络诊断结果"
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
            () => onNotice("已复制诊断结果"),
            () => onNotice("请选中诊断结果并复制。"),
          )
        }
      >
        复制诊断结果
      </Button>
    </div>
  );
}
