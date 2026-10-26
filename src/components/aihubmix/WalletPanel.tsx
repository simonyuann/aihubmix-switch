import {
  createContext,
  useContext,
  useState,
  type ReactNode,
  type ComponentType,
} from "react";
import { CreditCard, KeyRound } from "lucide-react";
import { useOptionalAccount } from "@/contexts/AccountContext";
import { RechargeDialog } from "./RechargeDialog";

const WalletActions = createContext<{
  recharge: () => void;
} | null>(null);

// Dialogs belong to the shell, outside the sidebar's layout and clipping context.
export function WalletActionsProvider({ children }: { children: ReactNode }) {
  const account = useOptionalAccount();
  const [recharge, setRecharge] = useState(false);
  return (
    <WalletActions.Provider
      value={{
        recharge: () => setRecharge(true),
      }}
    >
      {children}
      {recharge && (
        <RechargeDialog
          open
          onClose={() => setRecharge(false)}
          onPaid={() => void account?.refreshWallet()}
        />
      )}
    </WalletActions.Provider>
  );
}

export interface WalletNavItem {
  label: string;
  icon: ComponentType<{ className?: string }>;
  trailing?: string;
  trailingLabel?: string;
  onClick: () => void;
}
export function WalletPanel({
  onOpenAccount,
  renderItem,
}: {
  onOpenAccount: () => void;
  renderItem: (item: WalletNavItem) => ReactNode;
}) {
  const account = useOptionalAccount();
  const actions = useContext(WalletActions);
  if (!account?.state?.account.loggedIn) return null;
  return (
    <>
      {renderItem({
        label: "充值",
        icon: CreditCard,
        onClick: () => actions?.recharge(),
      })}
      {renderItem({
        label: "API 管理",
        icon: KeyRound,
        onClick: onOpenAccount,
      })}
    </>
  );
}
