import {
  createContext,
  useContext,
  useMemo,
  useState,
  type ReactNode,
  type ComponentType,
} from "react";
import { CreditCard } from "lucide-react";
import { useOptionalAccount } from "@/contexts/AccountContext";
import { RechargeDialog, type PaymentLinks } from "./RechargeDialog";

const WalletActions = createContext<{
  recharge: () => void;
} | null>(null);

export function useWalletActions() {
  return useContext(WalletActions);
}

// Dialogs belong to the shell, outside the sidebar's layout and clipping context.
export function WalletActionsProvider({ children }: { children: ReactNode }) {
  const account = useOptionalAccount();
  const [recharge, setRecharge] = useState(false);
  const owner = JSON.stringify([
    account?.state?.relayBaseUrl,
    account?.state?.account.userId,
  ]);
  const paymentLinks = useMemo<PaymentLinks>(() => new Map(), [owner]);
  return (
    <WalletActions.Provider
      value={{
        recharge: () => setRecharge(true),
      }}
    >
      {children}
      {recharge && (
        <RechargeDialog
          key={owner}
          paymentLinks={paymentLinks}
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
  renderItem,
}: {
  renderItem: (item: WalletNavItem) => ReactNode;
}) {
  const account = useOptionalAccount();
  const actions = useContext(WalletActions);
  if (!account?.state?.account.loggedIn) return null;
  return renderItem({
    label: "充值",
    icon: CreditCard,
    onClick: () => actions?.recharge(),
  });
}
