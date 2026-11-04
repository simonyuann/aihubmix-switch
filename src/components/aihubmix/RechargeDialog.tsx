import { useCallback, useEffect, useRef, useState } from "react";
import QRCode from "qrcode";
import { ChevronDown, RefreshCw } from "lucide-react";
import { settingsApi } from "@/lib/api/settings";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
  DialogFooter,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  Collapsible,
  CollapsibleContent,
  CollapsibleTrigger,
} from "@/components/ui/collapsible";
import {
  dispatch,
  type RechargeOrder,
  type RechargeOverview,
} from "@/lib/api/aihubmix";
const pending = (order: RechargeOrder) =>
  ["PENDING", "PAID", "RECHARGING"].includes(order.status);
const orderStatus: Record<string, string> = {
  PENDING: "待付款",
  PAID: "已付款",
  RECHARGING: "充值中",
  COMPLETED: "已完成",
  CANCELLED: "已取消",
  EXPIRED: "已过期",
  FAILED: "失败",
};
function orderDate(value: string) {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? "充值订单"
    : new Intl.DateTimeFormat("zh-CN", {
        dateStyle: "short",
        timeStyle: "short",
      }).format(date);
}
export function validAmount(amount: string, minimum: number, maximum: number) {
  return (
    /^\d{1,9}(\.\d{0,2})?$/.test(amount) &&
    Number(amount) >= minimum &&
    Number(amount) <= maximum
  );
}
export function RechargeDialog({
  open,
  onClose,
  onPaid,
}: {
  open: boolean;
  onClose: () => void;
  onPaid: () => void;
}) {
  const [overview, setOverview] = useState<RechargeOverview | null>(null),
    [amount, setAmount] = useState("20"),
    [method, setMethod] = useState(""),
    [order, setOrder] = useState<RechargeOrder | null>(null),
    [qr, setQr] = useState(""),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [loading, setLoading] = useState(false),
    [uncertain, setUncertain] = useState(false),
    [historyOpen, setHistoryOpen] = useState(false),
    [now, setNow] = useState(Date.now());
  const bodyRef = useRef<HTMLDivElement>(null);
  const mounted = useRef(true),
    gate = useRef(false),
    revision = useRef(0),
    paid = useRef(new Set<number>()),
    paidCallback = useRef(onPaid);
  const paymentLinks = useRef(
    new Map<number, Pick<RechargeOrder, "qrCode" | "payUrl">>(),
  );
  const preservePaymentLinks = useCallback(
    (next: RechargeOrder): RechargeOrder => {
      const previous = paymentLinks.current.get(next.id);
      const links = {
        qrCode: next.qrCode || previous?.qrCode,
        payUrl: next.payUrl || previous?.payUrl,
      };
      if (links.qrCode || links.payUrl)
        paymentLinks.current.set(next.id, links);
      return { ...next, ...links };
    },
    [],
  );
  paidCallback.current = onPaid;
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      revision.current++;
    };
  }, []);
  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const next = await dispatch<RechargeOverview>("payment-overview");
      if (!mounted.current) return;
      setOverview({ ...next, orders: next.orders.map(preservePaymentLinks) });
      setMethod((current) =>
        next.methods.some((m) => m.id === current)
          ? current
          : (next.methods[0]?.id ?? ""),
      );
      setUncertain(false);
      setError("");
    } catch (e) {
      if (mounted.current) {
        setError(String(e));
        setOverview((current) =>
          current ? { ...current, balance: null } : null,
        );
      }
    } finally {
      if (mounted.current) setLoading(false);
    }
  }, [preservePaymentLinks]);
  useEffect(() => {
    if (open) void refresh();
  }, [open, refresh]);
  useEffect(() => {
    if (bodyRef.current) bodyRef.current.scrollTop = 0;
  }, [order?.id]);
  const apply = useCallback(
    (next: RechargeOrder) => {
      if (!mounted.current) return;
      next = preservePaymentLinks(next);
      setOrder(next);
      setOverview((current) =>
        current
          ? {
              ...current,
              orders: [next, ...current.orders.filter((o) => o.id !== next.id)],
              balance: next.status === "COMPLETED" ? null : current.balance,
            }
          : current,
      );
      if (next.status === "COMPLETED" && !paid.current.has(next.id)) {
        paid.current.add(next.id);
        void refresh();
        paidCallback.current();
      }
    },
    [refresh, preservePaymentLinks],
  );
  useEffect(() => {
    if (!open || !order || !pending(order)) return;
    let stopped = false,
      polling = false;
    const poll = async () => {
      if (gate.current || polling) return;
      polling = true;
      const rev = revision.current;
      try {
        const next = await dispatch<RechargeOrder>("payment-order", {
          id: order.id,
        });
        if (!stopped && mounted.current && rev === revision.current)
          apply(next);
      } catch (e) {
        if (!stopped && mounted.current) setError(String(e));
      } finally {
        polling = false;
      }
    };
    void poll();
    const timer = setInterval(() => void poll(), 5000);
    const clock = setInterval(() => setNow(Date.now()), 1000);
    return () => {
      stopped = true;
      clearInterval(timer);
      clearInterval(clock);
    };
  }, [open, order?.id, order?.status, apply]);
  useEffect(() => {
    let active = true;
    setQr("");
    const content = order?.qrCode || order?.payUrl;
    if (content && order?.status === "PENDING")
      void QRCode.toDataURL(content, {
        width: 260,
        margin: 2,
        errorCorrectionLevel: "M",
      })
        .then((value) => {
          if (active) setQr(value);
        })
        .catch(() => {
          if (active) setError("付款码生成失败，请打开支付页面。");
        });
    return () => {
      active = false;
    };
  }, [order?.id, order?.status, order?.qrCode, order?.payUrl]);
  async function operate(command: string, payload: unknown, creation = false) {
    if (gate.current) return;
    gate.current = true;
    revision.current++;
    setBusy(true);
    setError("");
    try {
      const next = await dispatch<RechargeOrder>(command, payload);
      if (mounted.current) {
        apply(next);
        if (creation) {
          setUncertain(false);
          setHistoryOpen(false);
        }
      }
    } catch (e) {
      if (mounted.current) {
        setError(String(e));
        if (creation) setUncertain(true);
      }
    } finally {
      gate.current = false;
      if (mounted.current) setBusy(false);
    }
  }
  const selected = overview?.methods.find((m) => m.id === method);
  const minimum = Math.max(
    0.01,
    overview?.minAmount ?? 0,
    selected?.minAmount ?? 0,
  );
  const maximum = Math.min(
    ...[overview?.maxAmount, selected?.maxAmount].filter(
      (v): v is number => !!v && v > 0,
    ),
  );
  const outstanding =
    overview?.orders.some(pending) || (order && pending(order));
  const remaining = order
    ? Math.max(0, Math.ceil((Date.parse(order.expiresAt) - now) / 1000))
    : 0;
  return (
    <Dialog
      open={open}
      onOpenChange={(value) => {
        if (!value && !busy) onClose();
      }}
    >
      <DialogContent className="max-h-[calc(100dvh-2rem)] w-[calc(100vw-2rem)] overflow-hidden">
        <DialogHeader>
          <DialogTitle>账户充值</DialogTitle>
          <DialogDescription>
            钱包余额与 Key 配额分别计算。创建订单后扫码付款。
          </DialogDescription>
        </DialogHeader>
        <div
          ref={bodyRef}
          className="min-h-0 space-y-3 overflow-y-auto px-6 pb-1 text-body [&_label]:block [&_label]:space-y-2"
        >
          <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
            <p className="min-w-0 break-all text-fg-3">{overview?.email}</p>
            <div className="flex shrink-0 items-center gap-1">
              <p className="tabular-nums">
                钱包余额：{overview?.currency ?? ""}{" "}
                {overview?.balance?.toFixed(2) ?? "—"}
              </p>
              <Button
                variant="ghost"
                size="icon-compact"
                aria-label="刷新余额与充值记录"
                title="刷新余额与充值记录"
                disabled={busy || loading}
                onClick={() => void refresh()}
              >
                <RefreshCw
                  aria-hidden="true"
                  className={`h-3.5 w-3.5 ${loading ? "animate-spin motion-reduce:animate-none" : ""}`}
                />
              </Button>
            </div>
          </div>
          {error && (
            <p role="alert" className="break-words text-destructive">
              {error}
            </p>
          )}
          {uncertain && (
            <p role="alert">
              提交结果不确定。请成功刷新订单记录后再创建，避免重复付款。
            </p>
          )}
          {!order && (
            <div className="space-y-3 rounded-panel border border-border p-4">
              <label>
                充值金额
                <Input
                  value={amount}
                  inputMode="decimal"
                  onChange={(e) => setAmount(e.target.value)}
                  disabled={busy}
                />
              </label>
              <p className="text-fg-3">
                范围：{minimum}–{Number.isFinite(maximum) ? maximum : "不限"}
                ；手续费：{selected?.feeRate ?? 0}
              </p>
              <label>
                支付方式
                <select
                  className="w-full rounded-control border border-border bg-surface p-2"
                  value={method}
                  onChange={(e) => setMethod(e.target.value)}
                >
                  {overview?.methods.map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.name}
                    </option>
                  ))}
                </select>
              </label>
              <Button
                className="w-full"
                disabled={
                  !overview?.enabled ||
                  !selected ||
                  !validAmount(amount, minimum, maximum) ||
                  !!outstanding ||
                  uncertain ||
                  busy ||
                  loading
                }
                onClick={() =>
                  void operate("payment-create", { amount, method }, true)
                }
              >
                创建充值订单
              </Button>
            </div>
          )}
          {order && (
            <section className="space-y-3 rounded-panel border border-border p-3">
              <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
                <p className="font-semibold">充值订单</p>
                <p className="text-fg-3">
                  {orderStatus[order.status] ?? order.status}
                </p>
              </div>
              <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
                <p className="tabular-nums">
                  支付金额：{order.currency} {order.payAmount ?? "—"}
                </p>
                {order.status === "PENDING" && (
                  <p className="text-fg-3 tabular-nums">剩余 {remaining} 秒</p>
                )}
              </div>
              {qr && order.status === "PENDING" && remaining > 0 && (
                <img
                  src={qr}
                  alt="订单付款二维码"
                  width={260}
                  height={260}
                  className="mx-auto aspect-square h-auto w-[clamp(9rem,32dvh,14rem)] max-w-full rounded-control bg-white object-contain"
                />
              )}
              <div className="flex flex-wrap justify-center gap-2">
                {order.payUrl &&
                  order.status === "PENDING" &&
                  remaining > 0 && (
                    <Button
                      variant="outline"
                      size="compact"
                      onClick={() =>
                        void settingsApi
                          .openExternal(order.payUrl!)
                          .catch((e) => setError(String(e)))
                      }
                    >
                      打开支付页面
                    </Button>
                  )}
                {pending(order) && (
                  <Button
                    size="compact"
                    disabled={busy}
                    onClick={() =>
                      void operate("payment-verify", { id: order.id })
                    }
                  >
                    我已付款，核验
                  </Button>
                )}
                {order.status === "PENDING" && (
                  <Button
                    variant="outline"
                    size="compact"
                    disabled={busy}
                    onClick={() =>
                      void operate("payment-cancel", { id: order.id })
                    }
                  >
                    取消订单
                  </Button>
                )}
                {!pending(order) && (
                  <Button disabled={busy} onClick={() => setOrder(null)}>
                    返回充值
                  </Button>
                )}
              </div>
            </section>
          )}
          <Collapsible open={historyOpen} onOpenChange={setHistoryOpen}>
            <CollapsibleTrigger asChild>
              <button className="flex w-full items-center justify-between gap-2 rounded-control py-2 text-start text-fg-2 hover:text-fg-1 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring">
                <span>
                  充值记录
                  {overview && (
                    <span className="ms-2 text-fg-3">
                      {overview.orders.length}
                    </span>
                  )}
                </span>
                <ChevronDown
                  aria-hidden="true"
                  className={`h-4 w-4 transition-transform ${historyOpen ? "rotate-180" : ""}`}
                />
              </button>
            </CollapsibleTrigger>
            <CollapsibleContent className="space-y-2 pb-2 pt-1">
              {overview?.orders.map((o) => (
                <button
                  key={o.id}
                  className="flex w-full justify-between gap-2 rounded-control border border-border p-2 text-start hover:bg-subtle"
                  onClick={() => void operate("payment-order", { id: o.id })}
                  disabled={busy}
                >
                  <span>
                    {orderDate(o.createdAt)} ·{" "}
                    {o.method === "wxpay"
                      ? "微信"
                      : o.method === "alipay"
                        ? "支付宝"
                        : "充值"}{" "}
                    · {o.currency} {o.amount ?? "—"}
                  </span>
                  <span>{orderStatus[o.status] ?? o.status}</span>
                </button>
              ))}
              {overview && !overview.orders.length && (
                <p className="text-fg-3">暂无充值记录</p>
              )}
            </CollapsibleContent>
          </Collapsible>
        </div>
        <DialogFooter>
          <Button variant="outline" disabled={busy} onClick={onClose}>
            关闭
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
