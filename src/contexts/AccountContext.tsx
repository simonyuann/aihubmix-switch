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
import { useTauriEvent } from "@/hooks/useTauriEvent";
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
      setWalletError(payload.wallet ? "" : "余额暂不可用");
    },
  );
  const refresh = useCallback(async () => {
    if (loggingOut.current) return;
    const rev = ++revision.current;
    try {
      const next = await dispatch<AccountState>("session");
      if (rev !== revision.current) return;
      if (!next.account.loggedIn) throw new Error("请先登录账号");
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
      setError("");
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
        setError(/登录已失效|登录信息已变更|登录已过期/.test(message)
          ? "登录已失效。请关闭并重新打开 Codex 和 DeepSeek Harness；已运行的进程可能继续使用旧凭据，DSH 凭据文件也可能保留旧引用。"
          : "");
      } else {
        // Keep only a session already verified in this process. An offline cold
        // start still stays at login; network failures must not log out an
        // already verified user or discard their view and cached state.
        setError("连接暂时不可用，请检查网络后重试。");
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
      setError("登录已失效。请关闭并重新打开 Codex 和 DeepSeek Harness；已运行的进程可能继续使用旧凭据，DSH 凭据文件也可能保留旧引用。");
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
    let revocationWarning = "";
    try {
      const result = await dispatch<{ revocationWarning?: string }>("logout");
      revocationWarning = result?.revocationWarning ?? "";
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
    setError(`${revocationWarning ? `${revocationWarning} ` : ""}请关闭并重新打开 Codex 和 DeepSeek Harness；已运行的进程可能继续使用旧凭据。为保护自定义配置，DSH 凭据文件可能保留旧引用。`);
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
