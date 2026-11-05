import { Loader2 } from "lucide-react";
import { Fragment } from "react";
import { AccountProvider, useAccount } from "@/contexts/AccountContext";
import { LoginPage } from "@/components/aihubmix/LoginPage";
import { WalletActionsProvider } from "@/components/aihubmix/WalletPanel";
import { AnnouncementProvider } from "@/components/aihubmix/Announcements";
import CcWorkspace from "@/CcWorkspace";

function LoadingScreen() {
  return (
    <div
      role="status"
      className="flex h-screen items-center justify-center gap-3 bg-app text-body text-fg-3"
    >
      <Loader2 className="h-5 w-5 animate-spin" />
      正在验证登录…
    </div>
  );
}
export function AccountBoundary({ children }: { children: React.ReactNode }) {
  const { state, checking } = useAccount();
  if (checking) return <LoadingScreen />;
  return state?.account.loggedIn ? (
    <Fragment
      key={JSON.stringify([state.relayBaseUrl, String(state.account.userId)])}
    >
      {children}
    </Fragment>
  ) : (
    <LoginPage />
  );
}
export default function App() {
  return (
    <AccountProvider>
      <AccountBoundary>
        <AnnouncementProvider>
          <WalletActionsProvider>
            <CcWorkspace />
          </WalletActionsProvider>
        </AnnouncementProvider>
      </AccountBoundary>
    </AccountProvider>
  );
}
