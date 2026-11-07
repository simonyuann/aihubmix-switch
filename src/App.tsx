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
      {state.sessionOffline && (
        <div
          role="status"
          className="fixed bottom-4 right-4 z-[100] max-w-sm rounded-lg border border-amber-200 bg-amber-50 px-4 py-2 text-sm text-amber-900 shadow-md dark:border-amber-800 dark:bg-amber-950 dark:text-amber-100"
        >
          当前离线，已恢复本机登录状态。联网后自动验证，余额等在线功能暂不可用。
        </div>
      )}
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
