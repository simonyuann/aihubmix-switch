import { useState } from "react";
import { Coins, RefreshCw, Wallet } from "lucide-react";
import { useOptionalAccount } from "@/contexts/AccountContext";
import { Button } from "@/components/ui/button";
import type { WalletBalance } from "@/lib/api/aihubmix";

function formatBalance(wallet: WalletBalance | null | undefined) {
  const amount =
    wallet?.balance == null
      ? "—"
      : new Intl.NumberFormat("zh-CN", {
          minimumFractionDigits: 2,
          maximumFractionDigits: 2,
        }).format(wallet.balance);
  return `${wallet?.currency ?? ""} ${amount}`.trim();
}

export function HomeWalletBalance({ onRecharge }: { onRecharge?: () => void }) {
  const account = useOptionalAccount();
  const [refreshing, setRefreshing] = useState(false);
  if (!account?.state?.account.loggedIn) return null;

  return (
    <section
      aria-label="账户余额"
      className="mb-6 flex flex-wrap items-center gap-x-4 gap-y-3 rounded-panel border border-border bg-card px-4 py-4"
    >
      <span className="flex h-11 w-11 shrink-0 items-center justify-center rounded-full bg-action-soft text-action-text">
        <Coins aria-hidden="true" className="h-5 w-5" />
      </span>
      <div className="min-w-0">
        <div className="text-caption text-fg-3">账户余额</div>
        <div className="mt-0.5 text-xl font-semibold tabular-nums text-fg-1" aria-live="polite">
          {account.walletError ? "余额暂不可用" : formatBalance(account.wallet)}
        </div>
      </div>
      <button
        type="button"
        aria-label="刷新余额"
        title="刷新余额"
        disabled={refreshing}
        className="inline-flex h-9 items-center gap-1.5 rounded-control px-2 text-caption text-fg-3 transition-colors hover:bg-subtle hover:text-fg-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50 motion-reduce:transition-none"
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
          className={`h-4 w-4 ${refreshing ? "animate-spin motion-reduce:animate-none" : ""}`}
        />
        {refreshing ? "刷新中" : "刷新"}
      </button>
      <div className="ms-auto border-s border-border ps-4">
        <Button className="h-10 min-w-[104px] rounded-[10px] px-6" onClick={onRecharge} disabled={!onRecharge}>
          充值
        </Button>
      </div>
      {account.walletError && (
        <span role="status" className="sr-only">
          余额刷新失败，请重试
        </span>
      )}
    </section>
  );
}

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
          余额刷新失败，请重试
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
      <span>{error ? "余额暂不可用" : "余额"}</span>
      {!error && (
        <span className="tabular-nums text-fg-1">
          {formatBalance(wallet)}
        </span>
      )}
    </div>
  );
}
