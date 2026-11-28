import { useState } from "react";
import { useTranslation } from "react-i18next";
import { DiagnosticReport } from "./DiagnosticReport";
import { CircleHelp, Copy, MessageCircle } from "lucide-react";
import { useQuery } from "@tanstack/react-query";
import { AppPageHeader } from "@/components/shell/AppPageHeader";
import { Button } from "@/components/ui/button";
import { dispatch } from "@/lib/api/aihubmix";
import { copyText } from "@/lib/clipboard";
import { toast } from "@/lib/toast";
import groupQr from "@/assets/support/qq-group.png";

const groupNumber = "1091823578";

const questions = [
  ["helpPage.faq.switchGroup.q", "helpPage.faq.switchGroup.a"],
  ["helpPage.faq.rechargeBalance.q", "helpPage.faq.rechargeBalance.a"],
  ["helpPage.faq.codexModel.q", "helpPage.faq.codexModel.a"],
];

export function HelpPage() {
  const { t } = useTranslation();
  const [notice, setNotice] = useState("");
  const diagnostics = useQuery({
    queryKey: ["help-network-diagnostics"],
    queryFn: () => dispatch<string>("registration-diagnostics"),
    enabled: false,
    retry: false,
    networkMode: "always",
  });
  const contact = useQuery({
    queryKey: ["platform-support-contact"],
    queryFn: () => dispatch<{ contactInfo: string }>("support-contact"),
    retry: false,
    staleTime: 60_000,
  });
  return (
    <>
      <AppPageHeader
        title={t("helpPage.title")}
        icon={<CircleHelp className="h-5 w-5" />}
      />
      <main
        id="main-content"
        className="aihub-page-enter min-h-0 flex-1 overflow-y-auto px-6 py-6"
      >
        <div className="max-w-4xl space-y-8">
          <section>
            <h2 className="mb-3 text-lg font-semibold">
              {t("helpPage.section.contact")}
            </h2>
            <div className="overflow-hidden rounded-panel border border-border bg-surface">
              <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-4 px-5 py-4">
                <div className="min-w-0">
                  <h3 className="flex items-center gap-2 text-base font-semibold text-fg-1">
                    <MessageCircle
                      className="h-5 w-5 text-fg-2"
                      aria-hidden="true"
                    />
                    {t("helpPage.contact.qqGroup")}
                  </h3>
                  <p className="mt-2 text-body text-fg-2">
                    {t("helpPage.contact.qqScanHint")}
                  </p>
                  <div className="mt-4 flex items-center gap-2 text-body">
                    <span className="text-fg-3">
                      {t("helpPage.contact.groupNumberLabel")}
                    </span>
                    <span className="select-text font-medium tabular-nums text-fg-1">
                      {groupNumber}
                    </span>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 text-fg-3"
                      aria-label={t("helpPage.aria.copyGroupNumber")}
                      title={t("helpPage.aria.copyGroupNumber")}
                      onClick={() =>
                        void copyText(groupNumber).then(
                          () =>
                            toast.success(
                              t("helpPage.toast.groupNumberCopied"),
                            ),
                          () => toast.error(t("helpPage.toast.copyFailed")),
                        )
                      }
                    >
                      <Copy className="h-3.5 w-3.5" aria-hidden="true" />
                    </Button>
                  </div>
                </div>
                <img
                  src={groupQr}
                  alt={t("helpPage.contact.qqGroupQrAlt")}
                  width={168}
                  height={168}
                  className="h-auto w-[168px] max-w-full rounded-lg"
                />
              </div>
              {(contact.data?.contactInfo?.trim() ||
                contact.isError ||
                contact.isFetching) && (
                <div className="border-t border-border px-5 py-3">
                  {contact.data?.contactInfo?.trim() ? (
                    <p className="whitespace-pre-wrap break-words text-body text-fg-2">
                      {contact.data.contactInfo}
                    </p>
                  ) : contact.isError ? (
                    <div
                      role="alert"
                      className="flex flex-wrap items-center gap-2 text-body text-fg-3"
                    >
                      <span>{t("helpPage.contact.loadFailed")}</span>
                      <Button
                        variant="outline"
                        onClick={() => void contact.refetch()}
                      >
                        {t("helpPage.action.retry")}
                      </Button>
                    </div>
                  ) : (
                    <p role="status" className="text-body text-fg-3">
                      {t("helpPage.contact.loading")}
                    </p>
                  )}
                </div>
              )}
            </div>
          </section>
          <section>
            <Button
              variant="outline"
              disabled={diagnostics.isFetching}
              onClick={() => {
                setNotice("");
                void diagnostics.refetch();
              }}
            >
              {diagnostics.isFetching
                ? t("helpPage.status.diagnosticRunning")
                : t("helpPage.action.networkDiagnostics")}
            </Button>
            {diagnostics.isError && (
              <p role="alert">{t("helpPage.error.diagnosticFailed")}</p>
            )}
            {diagnostics.data && (
              <DiagnosticReport
                report={diagnostics.data}
                onNotice={setNotice}
              />
            )}
            {notice && <p role="status">{notice}</p>}
          </section>
          <section>
            <h2 className="mb-3 text-lg font-semibold">
              {t("helpPage.section.faq")}
            </h2>
            <div className="divide-y divide-border rounded-panel border border-border px-4">
              {questions.map(([question, answer]) => (
                <details key={question} className="py-4">
                  <summary className="cursor-pointer font-medium">
                    {t(question)}
                  </summary>
                  <p className="mt-3 text-body leading-6 text-fg-2">
                    {t(answer)}
                  </p>
                </details>
              ))}
            </div>
            <p className="mt-4 text-caption text-fg-3">
              {t("helpPage.warning.noApiKeySharing")}
            </p>
          </section>
        </div>
      </main>
    </>
  );
}
