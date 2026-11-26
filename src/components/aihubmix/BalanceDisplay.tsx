import { useState } from "react";
import { ChevronRight, RefreshCw, Wallet } from "lucide-react";
import { useOptionalAccount } from "@/contexts/AccountContext";
import { Button } from "@/components/ui/button";
import { HoverTip } from "@/components/ui/hover-tip";
import type { WalletBalance } from "@/lib/api/aihubmix";

function formatBalance(
  wallet: WalletBalance | null | undefined,
  compactCurrency = false,
) {
  const amount =
    wallet?.balance == null
      ? "—"
      : new Intl.NumberFormat("zh-CN", {
          minimumFractionDigits: 2,
          maximumFractionDigits: 2,
        }).format(wallet.balance);
  if (compactCurrency && wallet?.currency === "CNY") return `¥${amount}`;
  return `${wallet?.currency ?? ""} ${amount}`.trim();
}

export function SidebarWalletCard({
  collapsed,
  onRecharge,
}: {
  collapsed: boolean;
  onRecharge?: () => void;
}) {
  const account = useOptionalAccount();
  const [refreshing, setRefreshing] = useState(false);
  if (!account?.state?.account.loggedIn) return null;

  const amount = account.walletError
    ? "余额暂不可用"
    : formatBalance(account.wallet, true);
  if (collapsed) {
    return (
      <HoverTip
        content={`${amount} · 充值`}
        side="right"
        disableHoverableContent
      >
        <Button
          size="icon"
          variant="quiet"
          className="mx-auto mb-1 h-8 w-12 shrink-0 text-fg-2"
          aria-label={`账户余额 ${amount}，充值`}
          onClick={onRecharge}
          disabled={!onRecharge}
        >
          <Wallet className="h-5 w-5" aria-hidden="true" />
        </Button>
      </HoverTip>
    );
  }

  return (
    <section
      aria-label="账户余额"
      className="mx-2 mb-2 grid shrink-0 rounded-control bg-subtle px-2 py-1.5"
    >
      <div className="flex items-center text-caption text-fg-2">
        <span>账户余额</span>
        <Button
          type="button"
          variant="quiet"
          size="icon"
          aria-label="刷新余额"
          title="刷新余额"
          disabled={refreshing}
          className="h-6 w-6 text-fg-3 hover:bg-selected hover:text-fg-1"
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
            className={`h-3 w-3 ${refreshing ? "animate-spin motion-reduce:animate-none" : ""}`}
          />
          <span className="sr-only">{refreshing ? "刷新中" : "刷新"}</span>
        </Button>
      </div>
      <div className="flex min-w-0 flex-wrap items-center gap-x-1">
        <strong
          className={`min-w-0 max-w-full truncate font-semibold tabular-nums ${account.walletError ? "text-caption text-fg-2" : "text-[18px] leading-6 tracking-tight text-fg-1"}`}
          title={amount}
          aria-live="polite"
        >
          {amount}
        </strong>
        <Button
          className="-me-1 ms-auto h-7 shrink-0 gap-0.5 px-1 text-caption text-[var(--home-accent)] hover:bg-selected"
          variant="quiet"
          size="compact"
          onClick={onRecharge}
          disabled={!onRecharge}
        >
          充值
          <ChevronRight className="h-3 w-3 rtl:rotate-180" aria-hidden="true" />
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
        title={account.walletError ? "余额刷新失败，点击重试" : "刷新余额"}
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
        <span className="tabular-nums text-fg-1">{formatBalance(wallet)}</span>
      )}
    </div>
  );
}
