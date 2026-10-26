import { useCallback, useEffect, useRef, useState } from "react";
import QRCode from "qrcode";
import { settingsApi } from "@/lib/api/settings";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogDescription,
} from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import {
  dispatch,
  type RechargeOrder,
  type RechargeOverview,
} from "@/lib/api/aihubmix";
const pending = (order: RechargeOrder) =>
  ["PENDING", "PAID", "RECHARGING"].includes(order.status);
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
    [amount, setAmount] = useState("50"),
    [method, setMethod] = useState(""),
    [order, setOrder] = useState<RechargeOrder | null>(null),
    [qr, setQr] = useState(""),
    [error, setError] = useState(""),
    [busy, setBusy] = useState(false),
    [loading, setLoading] = useState(false),
    [uncertain, setUncertain] = useState(false),
    [now, setNow] = useState(Date.now());
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
        if (creation) setUncertain(false);
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
      <DialogContent className="max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>账户充值</DialogTitle>
          <DialogDescription>
            钱包余额与 Key 配额分别计算。创建订单后请在支付页面完成付款。
          </DialogDescription>
        </DialogHeader>
        <p>
          {overview?.email} · 钱包余额：{overview?.currency ?? ""}{" "}
          {overview?.balance?.toFixed(2) ?? "—"}
        </p>
        <Button
          variant="outline"
          disabled={busy || loading}
          onClick={() => void refresh()}
        >
          刷新余额与充值记录
        </Button>
        {error && (
          <p role="alert" className="text-destructive">
            {error}
          </p>
        )}
        {uncertain && (
          <p role="alert">
            提交结果不确定。请成功刷新订单记录后再创建，避免重复付款。
          </p>
        )}
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
        {order && (
          <section className="space-y-2 rounded-panel border border-border p-3">
            <p>
              订单 #{order.id} · {order.status}
            </p>
            <p>
              支付金额：{order.currency} {order.payAmount ?? "—"}
            </p>
            {order.status === "PENDING" && <p>剩余 {remaining} 秒</p>}
            {qr && order.status === "PENDING" && remaining > 0 && (
              <img src={qr} alt="订单付款二维码" width={260} height={260} />
            )}
            <div className="flex gap-2">
              {order.payUrl && order.status === "PENDING" && remaining > 0 && (
                <Button
                  variant="outline"
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
                  disabled={busy}
                  onClick={() =>
                    void operate("payment-cancel", { id: order.id })
                  }
                >
                  取消订单
                </Button>
              )}
            </div>
          </section>
        )}
        <h3 className="font-semibold">充值记录</h3>
        {overview?.orders.map((o) => (
          <button
            key={o.id}
            className="flex w-full justify-between rounded-control border border-border p-2 text-start hover:bg-subtle"
            onClick={() => void operate("payment-order", { id: o.id })}
            disabled={busy}
          >
            <span>
              #{o.id} · {o.currency} {o.amount ?? "—"}
            </span>
            <span>{o.status}</span>
          </button>
        ))}
        {overview && !overview.orders.length && (
          <p className="text-fg-3">暂无充值记录</p>
        )}
      </DialogContent>
    </Dialog>
  );
}
