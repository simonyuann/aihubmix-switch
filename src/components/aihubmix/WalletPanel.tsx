import { useState } from "react";
import { Wallet, RefreshCw } from "lucide-react";
import { useOptionalAccount } from "@/contexts/AccountContext";
import { Button } from "@/components/ui/button";
import { HoverTip } from "@/components/ui/hover-tip";
import { RechargeDialog } from "./RechargeDialog";
import { ApplyApiDialog } from "./ApplyApiDialog";
export function WalletPanel({
  collapsed,
  onOpenAccount,
}: {
  collapsed: boolean;
  onOpenAccount: () => void;
}) {
  const account = useOptionalAccount();
  const [recharge, setRecharge] = useState(false);
  const [apply, setApply] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  if (!account?.state?.account.loggedIn) return null;
  const wallet = account.wallet;
  const amount =
    wallet?.balance == null
      ? "—"
      : new Intl.NumberFormat("zh-CN", {
          minimumFractionDigits: 2,
          maximumFractionDigits: 2,
        }).format(wallet.balance);
  return (
    <>
      <div
        className={
          collapsed
            ? "flex flex-col items-center gap-2 py-3"
            : "mx-3 mb-2 mt-1 rounded-panel border border-border bg-surface p-3"
        }
      >
        {collapsed ? (
          <HoverTip
            content={`余额 ${wallet?.currency ?? ""} ${amount} · 充值`}
            side="right"
          >
            <button
              aria-label="余额与充值"
              className="flex h-9 w-10 items-center justify-center rounded-control hover:bg-subtle"
              onClick={() => setRecharge(true)}
            >
              <Wallet className="h-5 w-5" />
            </button>
          </HoverTip>
        ) : (
          <>
            <div className="flex items-center justify-between text-caption text-fg-3">
              <span>账户余额</span>
              <button
                aria-label="刷新余额"
                disabled={refreshing}
                onClick={() => {
                  setRefreshing(true);
                  void account
                    .refreshWallet()
                    .finally(() => setRefreshing(false));
                }}
                className="rounded-control p-1 hover:bg-subtle"
              >
                <RefreshCw
                  className={`h-3.5 w-3.5 ${refreshing ? "animate-spin" : ""}`}
                />
              </button>
            </div>
            <div className="mb-3 mt-1 truncate font-semibold tabular-nums">
              <span className="me-1 text-caption text-fg-3">
                {wallet?.currency ?? ""}
              </span>
              <span className="text-[23px] leading-8">{amount}</span>
            </div>
            {account.walletError && (
              <p
                role="alert"
                className="mb-2 whitespace-normal text-caption text-destructive"
              >
                余额暂不可用，请刷新
              </p>
            )}
            <Button className="h-8 w-full" onClick={() => setRecharge(true)}>
              充值
            </Button>
          </>
        )}
        <Button
          variant="outline"
          className={
            collapsed
              ? "h-auto w-12 whitespace-normal px-1 py-1.5 text-caption"
              : "mt-2 h-8 w-full"
          }
          onClick={() => setApply(true)}
        >
          申请 API
        </Button>
      </div>
      {recharge && (
        <RechargeDialog
          open
          onClose={() => setRecharge(false)}
          onPaid={() => void account.refreshWallet()}
        />
      )}
      {apply && (
        <ApplyApiDialog
          onClose={() => setApply(false)}
          onCreated={() => {
            setApply(false);
            onOpenAccount();
          }}
        />
      )}
    </>
  );
}
