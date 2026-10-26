import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { useQueryClient } from "@tanstack/react-query";
import {
  dispatch,
  type AccountState,
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
  const client = useQueryClient();
  const [state, setState] = useState<AccountState | null>(null);
  const [checking, setChecking] = useState(true);
  const [error, setError] = useState("");
  const [wallet, setWallet] = useState<WalletBalance | null>(null);
  const [walletError, setWalletError] = useState("");
  const revision = useRef(0);
  const walletRevision = useRef(0);
  const refresh = useCallback(async () => {
    const rev = ++revision.current;
    try {
      const next = await dispatch<AccountState>("session");
      if (rev !== revision.current) return;
      if (!next.account.loggedIn) throw new Error("请先登录账号");
      setState(next);
      setError("");
    } catch (e) {
      if (rev !== revision.current) return;
      setState(null);
      setWallet(null);
      walletRevision.current++;
      client.clear();
      const message = String(e);
      setError(/AUTH_REQUIRED|请先登录/.test(message) ? "" : message);
    } finally {
      if (rev === revision.current) setChecking(false);
    }
  }, [client]);
  const refreshWallet = useCallback(async () => {
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
      setError("登录已失效，请重新登录");
      client.clear();
    };
    window.addEventListener("aihubmix:session-expired", expired);
    return () => {
      revision.current++;
      walletRevision.current++;
      window.removeEventListener("aihubmix:session-expired", expired);
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
  }, [state?.account.userId, state?.account.loggedIn, refresh, refreshWallet]);
  async function authenticate(command: "login" | "register", payload: unknown) {
    const rev = ++revision.current;
    await dispatch(command, payload);
    if (rev !== revision.current) return;
    // Verification failure must remain visible on the login form.
    const next = await dispatch<AccountState>("session");
    if (rev !== revision.current) return;
    if (!next.account.loggedIn) throw new Error("请重新登录");
    walletRevision.current++;
    client.clear();
    setWallet(null);
    setState(next);
    setError("");
    setChecking(false);
  }
  async function logout() {
    await dispatch("logout");
    revision.current++;
    walletRevision.current++;
    setState(null);
    setWallet(null);
    setError("");
    setChecking(false);
    client.clear();
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
