import { useState } from "react";
import { RefreshCw, Wallet } from "lucide-react";
import { useOptionalAccount } from "@/contexts/AccountContext";
import type { WalletBalance } from "@/lib/api/aihubmix";

export function BalanceDisplay() {
  const account = useOptionalAccount();
  const [refreshing, setRefreshing] = useState(false);
  if (!account?.state?.account.loggedIn) return null;
  return (
    <div
      className="flex items-center"
      style={{ WebkitAppRegion: "no-drag" } as React.CSSProperties}
    >
      <WalletBalanceDisplay
        wallet={account.wallet}
        error={account.walletError}
      />
      <button
        type="button"
        aria-label="刷新余额"
        title={account.walletError || "刷新余额"}
        disabled={refreshing}
        className="-ms-1 me-2 rounded p-1 text-fg-3 hover:bg-subtle hover:text-fg-1 disabled:opacity-50"
        onClick={async () => {
          setRefreshing(true);
          try {
            await account.refreshWallet();
          } finally {
            setRefreshing(false);
          }
        }}
      >
        <RefreshCw
          aria-hidden="true"
          className={`h-3.5 w-3.5 ${refreshing ? "animate-spin motion-reduce:animate-none" : ""}`}
        />
      </button>
      {account.walletError && (
        <span role="status" className="sr-only">
          余额刷新失败：{account.walletError}
        </span>
      )}
    </div>
  );
}

export function WalletBalanceDisplay({
  wallet,
  error,
  compact = false,
}: {
  wallet: WalletBalance | null | undefined;
  error?: string;
  compact?: boolean;
}) {
  const amount =
    wallet?.balance == null
      ? "—"
      : new Intl.NumberFormat("zh-CN", {
          minimumFractionDigits: 2,
          maximumFractionDigits: 2,
        }).format(wallet.balance);
  return (
    <div
      className={
        compact
          ? "flex min-w-0 items-center justify-center gap-1 whitespace-nowrap text-[11px] text-muted-foreground"
          : "me-2 flex shrink-0 items-center gap-2 whitespace-nowrap text-body text-fg-3"
      }
      title={error ? "余额暂不可用" : undefined}
    >
      {!compact && <Wallet aria-hidden="true" className="h-4 w-4" />}
      <span>余额</span>
      <span className="tabular-nums text-fg-1">
        {`${wallet?.currency ?? ""} ${amount}`.trim()}
      </span>
    </div>
  );
}
