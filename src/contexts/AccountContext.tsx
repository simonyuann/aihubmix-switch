import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useTranslation } from "react-i18next";
import { useQueryClient } from "@tanstack/react-query";
import { useTauriEvent } from "@/hooks/useTauriEvent";
import {
  dispatch,
  type AccountState,
  type LogoutResult,
  type WalletBalance,
} from "@/lib/api/aihubmix";

interface AccountSession {
  state: AccountState | null;
  checking: boolean;
  error: string;
  wallet: WalletBalance | null;
  walletError: string;
  refresh: () => Promise<void>;
  refreshWallet: () => Promise<void>;
  authenticate: (
    command: "login" | "register",
    payload: unknown,
  ) => Promise<void>;
  logout: () => Promise<void>;
}
const Context = createContext<AccountSession | null>(null);
export function useAccount() {
  const value = useContext(Context);
  if (!value) throw new Error("AccountProvider is required");
  return value;
}
export function useOptionalAccount() {
  return useContext(Context);
}
export function AccountProvider({ children }: { children: ReactNode }) {
  const { t } = useTranslation();
  const client = useQueryClient();
  const [state, setState] = useState<AccountState | null>(null);
  const [checking, setChecking] = useState(true);
  const [error, setError] = useState("");
  const [wallet, setWallet] = useState<WalletBalance | null>(null);
  const [walletError, setWalletError] = useState("");
  const revision = useRef(0);
  const walletRevision = useRef(0);
  const loggingOut = useRef(false);
  const activeAccount = useRef<string | null>(null);
  const accountIdentity = (next: AccountState) =>
    JSON.stringify([next.relayBaseUrl, String(next.account.userId)]);
  useTauriEvent<{ userId: string | number; wallet: WalletBalance | null }>(
    "aihubmix-wallet-updated",
    (payload) => {
      if (
        !state?.account.loggedIn ||
        String(payload.userId) !== String(state.account.userId)
      )
        return;
      walletRevision.current++;
      setWallet(payload.wallet);
      setWalletError(
        payload.wallet ? "" : t("accountContext.error.walletUnavailable"),
      );
    },
  );
  const refresh = useCallback(async () => {
    if (loggingOut.current) return;
    const rev = ++revision.current;
    try {
      const next = await dispatch<AccountState>("session");
      if (rev !== revision.current) return;
      if (!next.account.loggedIn)
        throw new Error(t("accountContext.error.pleaseLogin"));
      const identity = accountIdentity(next);
      if (
        activeAccount.current !== null &&
        activeAccount.current !== identity
      ) {
        walletRevision.current++;
        setWallet(null);
        setWalletError("");
        client.clear();
      }
      activeAccount.current = identity;
      setState(next);
      setError(
        next.sessionOffline ? t("accountContext.status.offlineUsingSaved") : "",
      );
    } catch (e) {
      if (rev !== revision.current) return;
      const message = String(e);
      if (
        /AUTH_REQUIRED|请先登录|登录已过期|登录已失效|请重新登录/.test(message)
      ) {
        setState(null);
        setWallet(null);
        walletRevision.current++;
        activeAccount.current = null;
        client.clear();
        setError(
          /登录已失效|登录信息已变更|登录已过期/.test(message)
            ? t("accountContext.error.sessionExpired")
            : "",
        );
      } else {
        // Keep an existing session when verification cannot be completed.
        setError(
          message.includes("无法解锁系统凭据存储")
            ? t("accountContext.error.keychainLockedDetail")
            : t("accountContext.error.connectionUnavailable"),
        );
      }
    } finally {
      if (rev === revision.current) setChecking(false);
    }
  }, [client]);
  const refreshWallet = useCallback(async () => {
    if (loggingOut.current) return;
    const rev = ++walletRevision.current;
    setWalletError("");
    try {
      const value = await dispatch<WalletBalance>("payment-balance");
      if (rev === walletRevision.current) setWallet(value);
    } catch (e) {
      if (rev === walletRevision.current) {
        setWallet(null);
        setWalletError(String(e));
      }
    }
  }, []);
  useEffect(() => {
    void refresh();
    const expired = () => {
      revision.current++;
      walletRevision.current++;
      setState(null);
      setWallet(null);
      setChecking(false);
      setError(t("accountContext.error.sessionExpired"));
      activeAccount.current = null;
      client.clear();
    };
    window.addEventListener("aihubmix:session-expired", expired);
    const online = () => void refresh();
    window.addEventListener("online", online);
    return () => {
      revision.current++;
      walletRevision.current++;
      window.removeEventListener("aihubmix:session-expired", expired);
      window.removeEventListener("online", online);
    };
  }, [client, refresh]);
  useEffect(() => {
    if (!state?.account.loggedIn) return;
    void refreshWallet();
    // Revalidate the stored session rather than trusting the local loggedIn flag.
    const timer = window.setInterval(() => {
      void refresh();
    }, 60_000);
    return () => window.clearInterval(timer);
  }, [
    state?.account.userId,
    state?.account.loggedIn,
    state?.sessionOffline,
    refresh,
    refreshWallet,
  ]);
  async function authenticate(command: "login" | "register", payload: unknown) {
    const rev = ++revision.current;
    // The server's successful login response already identifies the account.
    // A second request here could fail after the backend saved the session and
    // leave the frontend stuck on the login page with an active backend.
    const next = await dispatch<AccountState>(command, payload);
    if (rev !== revision.current) return;
    if (!next.account.loggedIn)
      throw new Error(t("accountContext.error.reloginRequired"));
    walletRevision.current++;
    client.clear();
    setWallet(null);
    setWalletError("");
    activeAccount.current = accountIdentity(next);
    setState(next);
    setError("");
    setChecking(false);
  }
  async function logout() {
    if (loggingOut.current) return;
    loggingOut.current = true;
    revision.current++;
    walletRevision.current++;
    setChecking(true);
    let logoutNotice = "";
    try {
      const result = await dispatch<LogoutResult>("logout");
      logoutNotice = result?.logoutNotice ?? result?.revocationWarning ?? "";
    } catch (error) {
      loggingOut.current = false;
      setChecking(false);
      throw error;
    }
    revision.current++;
    walletRevision.current++;
    setState(null);
    setWallet(null);
    setWalletError("");
    setError(logoutNotice);
    setChecking(false);
    activeAccount.current = null;
    client.clear();
    loggingOut.current = false;
  }
  return (
    <Context.Provider
      value={{
        state,
        checking,
        error,
        wallet,
        walletError,
        refresh,
        refreshWallet,
        authenticate,
        logout,
      }}
    >
      {children}
    </Context.Provider>
  );
}
