// T22 RechargeDialog 批次:硬编码中文 → rechargeDialog.* keys
import fs from "node:fs";
const p = "src/components/aihubmix/RechargeDialog.tsx";
let s = fs.readFileSync(p, "utf8");

// orderStatus 映射:key 表(渲染处 t())
const statusPairs = [
  ["PENDING", "待付款"],
  ["PAID", "已付款"],
  ["RECHARGING", "充值中"],
  ["COMPLETED", "已完成"],
  ["CANCELLED", "已取消"],
  ["EXPIRED", "已过期"],
  ["FAILED", "失败"],
];
for (const [id, zhv] of statusPairs) {
  s = s.replace(`${id}: "${zhv}",`, `${id}: "rechargeDialog.status.${id.toLowerCase()}",`);
}

const pairs = [
  [`    ? "充值订单"`, `    ? t("rechargeDialog.fallback.orderTitle")`],
  [`      setMessage("加载充值信息失败，请重试。");`, `      setMessage(t("rechargeDialog.error.loadOverview"));`],
  [`      setMessage("刷新充值状态失败，请重试。");`, `      setMessage(t("rechargeDialog.error.refreshStatus"));`],
  [`      setMessage("付款码生成失败，请打开支付页面。");`, `      setMessage(t("rechargeDialog.error.qrFailed"));`],
  [`      setMessage("充值操作未能确认，请刷新充值记录。");`, `      setMessage(t("rechargeDialog.error.uncertainSubmit"));`],
  [`        ? "正在刷新充值信息，请稍候。"`, `        ? t("rechargeDialog.status.refreshingOverview")`],
  [`        : "充值信息暂不可用，请刷新后重试。";`, `        : t("rechargeDialog.error.overviewUnavailable");`],
  [`          ? "当前充值服务不可用，请稍后重试。"`, `          ? t("rechargeDialog.error.serviceUnavailable")`],
  [`          : "请先刷新充值记录，确认上次提交结果。"`, `          : t("rechargeDialog.error.confirmPreviousFirst")`],
  [`          : "已有待付款订单，请先继续支付或取消。"`, `          : t("rechargeDialog.error.pendingOrderExists")`],
  [`        : "已有充值正在处理，请查看订单状态。"`, `        : t("rechargeDialog.error.orderProcessing")`],
  [`          ? "暂无可用支付方式，请稍后重试。"`, `          ? t("rechargeDialog.error.noPaymentMethod")`],
  [`                ? \`请输入 \${minimum}–\${Number.isFinite(maximum) ? maximum : "不限"} 范围内的金额，最多保留两位小数。\``, `                ? t("rechargeDialog.error.amountRange", {
                    minimum,
                    maximum: Number.isFinite(maximum) ? maximum : "不限",
                  })`],
  [`                ? "正在处理，请稍候。"`, `                ? t("rechargeDialog.status.processing")`],
  [`<DialogTitle>账户充值</DialogTitle>`, `<DialogTitle>{t("rechargeDialog.title")}</DialogTitle>`],
  [`            钱包余额与 Key 配额分别计算。创建订单后扫码付款。`, `            {t("rechargeDialog.description")}`],
  [`                钱包余额：{overview?.currency ?? ""}{" "}`, `                {t("rechargeDialog.label.walletBalance")}
                {overview?.currency ?? ""}{" "}`],
  [`                aria-label="刷新余额与充值记录"
                title="刷新余额与充值记录"`, `                aria-label={t("rechargeDialog.action.refresh")}
                title={t("rechargeDialog.action.refresh")}`],
  [`              提交结果不确定。请成功刷新订单记录后再创建，避免重复付款。`, `              {t("rechargeDialog.error.uncertainResult")}`],
  [`                充值金额`, `                {t("rechargeDialog.label.amount")}`],
  [`<div className="flex flex-wrap gap-2" aria-label="快捷充值金额">`, `<div className="flex flex-wrap gap-2" aria-label={t("rechargeDialog.label.quickAmounts")}>`],
  [`                    {value} 元`, `                    {t("rechargeDialog.label.yuan", { value })}`],
  [`              <p className="text-fg-3">
                范围：{minimum}–{Number.isFinite(maximum) ? maximum : "不限"}
                ；手续费：{selected?.feeRate ?? 0}
              </p>`, `              <p className="text-fg-3">
                {t("rechargeDialog.label.rangeAndFee", {
                  minimum,
                  maximum: Number.isFinite(maximum) ? maximum : "不限",
                  fee: selected?.feeRate ?? 0,
                })}
              </p>`],
  [`              <label>
                支付方式`, `              <label>
                {t("rechargeDialog.label.paymentMethod")}`],
  [`                创建充值订单`, `                {t("rechargeDialog.action.createOrder")}`],
  [`                  查看未完成的充值`, `                  {t("rechargeDialog.action.viewPending")}`],
  [`                <p className="font-semibold">充值订单</p>`, `                <p className="font-semibold">{t("rechargeDialog.label.orderHistory")}</p>`],
  [`                  支付金额：{order.currency} {order.payAmount ?? "—"}`, `                  {t("rechargeDialog.label.payAmount")}
                  {order.currency} {order.payAmount ?? "—"}`],
  [`                  <p className="text-fg-3 tabular-nums">剩余 {remaining} 秒</p>`, `                  <p className="text-fg-3 tabular-nums">
                    {t("rechargeDialog.label.remainingSeconds", { seconds: remaining })}
                  </p>`],
  [`                  alt="订单付款二维码"`, `                  alt={t("rechargeDialog.label.orderQrCode")}`],
  [`                    付款信息暂不可用，可取消此订单后重新创建。`, `                    {t("rechargeDialog.label.payInfoUnavailable")}`],
  [`                                "操作失败，请刷新充值记录后重试。",`, `                                t("rechargeDialog.error.operationFailed"),`],
  [`                      打开支付页面`, `                      {t("rechargeDialog.action.openPayPage")}`],
  [`                    我已付款，核验`, `                    {t("rechargeDialog.action.markPaid")}`],
  [`                    刷新充值状态`, `                    {t("rechargeDialog.action.refreshStatus")}`],
  [`                    取消订单`, `                    {t("rechargeDialog.action.cancelOrder")}`],
  [`                    返回充值`, `                    {t("rechargeDialog.action.backToRecharge")}`],
  [`                  充值记录`, `                  {t("rechargeDialog.label.orderHistory")}`],
  [`                      ? "微信"`, `                      ? t("rechargeDialog.method.wechat")`],
  [`                        ? "支付宝"`, `                        ? t("rechargeDialog.method.alipay")`],
  [`                        : "充值"}{" "}`, `                        : t("rechargeDialog.method.recharge")}{" "}`],
  // useTranslation
  [`import { friendlyError } from "./friendlyError";`, `import { friendlyError } from "./friendlyError";
import { useTranslation } from "react-i18next";`],
  [`  const [overview, setOverview] = useState<RechargeOverview | null>(null),`, `  const { t } = useTranslation();
  const [overview, setOverview] = useState<RechargeOverview | null>(null),`],
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
