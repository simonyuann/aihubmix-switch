// T22 HelpPage 批次:硬编码中文 → helpPage.* keys
import fs from "node:fs";
const p = "src/components/aihubmix/HelpPage.tsx";
let s = fs.readFileSync(p, "utf8");

const pairs = [
  // questions 数组:key 表(渲染处 t())
  [`const questions = [
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
];`, `const questions = [
  ["helpPage.faq.switchGroup.q", "helpPage.faq.switchGroup.a"],
  ["helpPage.faq.rechargeBalance.q", "helpPage.faq.rechargeBalance.a"],
  ["helpPage.faq.codexModel.q", "helpPage.faq.codexModel.a"],
];`],
  [`        title="帮助与答疑"`, `        title={t("helpPage.title")}`],
  [`            <h2 className="mb-3 text-lg font-semibold">联系方式</h2>`, `            <h2 className="mb-3 text-lg font-semibold">
              {t("helpPage.section.contact")}
            </h2>`],
  [`                    AIHubMix 交流群`, `                    {t("helpPage.contact.qqGroup")}`],
  [`                  <p className="mt-2 text-body text-fg-2">
                    打开手机 QQ，扫一扫加入群聊。
                  </p>`, `                  <p className="mt-2 text-body text-fg-2">
                    {t("helpPage.contact.qqScanHint")}
                  </p>`],
  [`                    <span className="text-fg-3">群号</span>`, `                    <span className="text-fg-3">
                      {t("helpPage.contact.groupNumberLabel")}
                    </span>`],
  [`                      aria-label="复制群号"
                      title="复制群号"`, `                      aria-label={t("helpPage.aria.copyGroupNumber")}
                      title={t("helpPage.aria.copyGroupNumber")}`],
  [`                          () => toast.success("群号已复制"),
                          () => toast.error("请选中群号并复制。"),`, `                          () => toast.success(t("helpPage.toast.groupNumberCopied")),
                          () => toast.error(t("helpPage.toast.copyFailed")),`],
  [`                  alt="AIHubMix QQ 交流群二维码"`, `                  alt={t("helpPage.contact.qqGroupQrAlt")}`],
  [`                      <span>其他联系方式暂时无法加载。</span>`, `                      <span>{t("helpPage.contact.loadFailed")}</span>`],
  [`                        重试
                      </Button>`, `                        {t("helpPage.action.retry")}
                      </Button>`],
  [`                    <p role="status" className="text-body text-fg-3">
                      正在获取其他联系方式…
                    </p>`, `                    <p role="status" className="text-body text-fg-3">
                      {t("helpPage.contact.loading")}
                    </p>`],
  [`              {diagnostics.isFetching ? "正在检查账号和模型接口…" : "网络诊断"}`, `              {diagnostics.isFetching
                ? t("helpPage.status.diagnosticRunning")
                : t("helpPage.action.networkDiagnostics")}`],
  [`            {diagnostics.isError && <p role="alert">诊断未完成，请重试。</p>}`, `            {diagnostics.isError && (
              <p role="alert">{t("helpPage.error.diagnosticFailed")}</p>
            )}`],
  [`            <h2 className="mb-3 text-lg font-semibold">常见问题</h2>`, `            <h2 className="mb-3 text-lg font-semibold">
              {t("helpPage.section.faq")}
            </h2>`],
  [`            <p className="mt-4 text-caption text-fg-3">
              请勿向他人发送 API Key。
            </p>`, `            <p className="mt-4 text-caption text-fg-3">
              {t("helpPage.warning.noApiKeySharing")}
            </p>`],
  // questions 渲染处 t()
  [`              {questions.map(([question, answer]) => (
                <details key={question} className="py-4">
                  <summary className="cursor-pointer font-medium">
                    {question}
                  </summary>
                  <p className="mt-3 text-body leading-6 text-fg-2">{answer}</p>`, `              {questions.map(([question, answer]) => (
                <details key={question} className="py-4">
                  <summary className="cursor-pointer font-medium">
                    {t(question)}
                  </summary>
                  <p className="mt-3 text-body leading-6 text-fg-2">
                    {t(answer)}
                  </p>`],
  // useTranslation
  [`import { useState } from "react";`, `import { useState } from "react";
import { useTranslation } from "react-i18next";`],
  [`export function HelpPage() {
  const [notice, setNotice] = useState("");`, `export function HelpPage() {
  const { t } = useTranslation();
  const [notice, setNotice] = useState("");`],
];

let applied = 0;
const missed = [];
for (const [oldStr, newStr] of pairs) {
  if (s.includes(oldStr)) {
    s = s.split(oldStr).join(newStr);
    applied++;
  } else missed.push(oldStr.slice(0, 60).replace(/\n/g, "\\n"));
}
fs.writeFileSync(p, s);
console.log(`applied: ${applied}/${pairs.length}`);
if (missed.length) {
  console.log("MISSED:");
  for (const m of missed) console.log("  -", m);
}
