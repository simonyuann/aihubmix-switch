import { Wallet } from "lucide-react";
import { useOptionalAccount } from "@/contexts/AccountContext";

export function BalanceDisplay() {
  const account = useOptionalAccount();
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
    <div
      className="me-2 flex shrink-0 items-center gap-2 whitespace-nowrap text-body text-fg-3"
      title={account.walletError ? "余额暂不可用" : undefined}
    >
      <Wallet aria-hidden="true" className="h-4 w-4" />
      <span>余额</span>
      <span className="tabular-nums text-fg-1">
        {`${wallet?.currency ?? ""} ${amount}`.trim()}
      </span>
    </div>
  );
}
