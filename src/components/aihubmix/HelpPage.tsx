import { useState } from "react";
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
  [
    "如何切换模型分组？",
    "在首页选择工具，然后在模型分组中选择要使用的服务。切换后请按提示重新打开工具。",
  ],
  [
    "充值后余额没有更新怎么办？",
    "请先刷新余额，并在充值记录中查看订单状态。正在到账的订单请稍候，不要重复付款。",
  ],
  [
    "如何在 Codex 中切换模型？",
    "配置好分组后，在 Codex 中打开模型选择器；命令行版本也可以使用 /model。",
  ],
];

export function HelpPage() {
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
        title="帮助与答疑"
        icon={<CircleHelp className="h-5 w-5" />}
      />
      <main
        id="main-content"
        className="aihub-page-enter min-h-0 flex-1 overflow-y-auto px-6 py-6"
      >
        <div className="max-w-4xl space-y-8">
          <section>
            <h2 className="mb-3 text-lg font-semibold">联系方式</h2>
            <div className="overflow-hidden rounded-panel border border-border bg-surface">
              <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-4 px-5 py-4">
                <div className="min-w-0">
                  <h3 className="flex items-center gap-2 text-base font-semibold text-fg-1">
                    <MessageCircle
                      className="h-5 w-5 text-fg-2"
                      aria-hidden="true"
                    />
                    AIHubMix 交流群
                  </h3>
                  <p className="mt-2 text-body text-fg-2">
                    打开手机 QQ，扫一扫加入群聊。
                  </p>
                  <div className="mt-4 flex items-center gap-2 text-body">
                    <span className="text-fg-3">群号</span>
                    <span className="select-text font-medium tabular-nums text-fg-1">
                      {groupNumber}
                    </span>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 text-fg-3"
                      aria-label="复制群号"
                      title="复制群号"
                      onClick={() =>
                        void copyText(groupNumber).then(
                          () => toast.success("群号已复制"),
                          () => toast.error("请选中群号并复制。"),
                        )
                      }
                    >
                      <Copy className="h-3.5 w-3.5" aria-hidden="true" />
                    </Button>
                  </div>
                </div>
                <img
                  src={groupQr}
                  alt="AIHubMix QQ 交流群二维码"
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
                      <span>其他联系方式暂时无法加载。</span>
                      <Button
                        variant="outline"
                        onClick={() => void contact.refetch()}
                      >
                        重试
                      </Button>
                    </div>
                  ) : (
                    <p role="status" className="text-body text-fg-3">
                      正在获取其他联系方式…
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
              {diagnostics.isFetching ? "正在检查账号和模型接口…" : "网络诊断"}
            </Button>
            {diagnostics.isError && <p role="alert">诊断未完成，请重试。</p>}
            {diagnostics.data && (
              <DiagnosticReport
                report={diagnostics.data}
                onNotice={setNotice}
              />
            )}
            {notice && <p role="status">{notice}</p>}
          </section>
          <section>
            <h2 className="mb-3 text-lg font-semibold">常见问题</h2>
            <div className="divide-y divide-border rounded-panel border border-border px-4">
              {questions.map(([question, answer]) => (
                <details key={question} className="py-4">
                  <summary className="cursor-pointer font-medium">
                    {question}
                  </summary>
                  <p className="mt-3 text-body leading-6 text-fg-2">{answer}</p>
                </details>
              ))}
            </div>
            <p className="mt-4 text-caption text-fg-3">
              请勿向他人发送 API Key。
            </p>
          </section>
        </div>
      </main>
    </>
  );
}
