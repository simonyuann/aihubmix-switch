import { Wallet } from "lucide-react";
import { useOptionalAccount } from "@/contexts/AccountContext";
import type { WalletBalance } from "@/lib/api/aihubmix";

export function BalanceDisplay() {
  const account = useOptionalAccount();
  if (!account?.state?.account.loggedIn) return null;
  return (
    <WalletBalanceDisplay wallet={account.wallet} error={account.walletError} />
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
