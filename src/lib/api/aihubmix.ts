import { invoke } from "@tauri-apps/api/core";
export interface KeyInfo {
  id: number;
  name: string;
  keyMasked: string;
  groupId: number | null;
  groupName: string;
  status: string;
  quotaUsed: number | null;
  quota: number | null;
  createdAt: string;
  local: boolean;
}
export interface LegacyMapping {
  app: string;
  appLabel: string;
  model: string;
}
export interface LogoutResult {
  logoutNotice?: string;
  revocationWarning?: string;
}
export interface AccountState {
  sessionOffline?: boolean;
  legacyMappings?: LegacyMapping[];
  relayBaseUrl: string;
  account: {
    loggedIn: boolean;
    email?: string;
    userId?: string | number;
  };
  localKeys: KeyInfo[];
  legacyData: boolean;
  storageError: string | null;
}
export async function dispatch<T>(
  command: string,
  payload?: unknown,
): Promise<T> {
  try {
    return await invoke("aihubmix_dispatch", {
      command,
      payload: payload ?? null,
    });
  } catch (error) {
    if (
      !["login", "register", "session", "state"].includes(command) &&
      String(error).startsWith("AUTH_REQUIRED:")
    ) {
      window.dispatchEvent(new Event("aihubmix:session-expired"));
    }
    throw error;
  }
}
export const aihubmixApi = {
  state: () => dispatch<AccountState>("state"),
};
export interface GroupInfo {
  id: number;
  name: string;
  description: string;
  platform: string;
  rateMultiplier: number;
  allowMessagesDispatch: boolean;
  subscriptionType: string;
}
export interface RechargeMethod {
  id: string;
  name: string;
  minAmount: number;
  maxAmount: number;
  feeRate: number;
}
export interface RechargeOrder {
  id: number;
  amount: number | null;
  payAmount: number | null;
  bonusAmount: number | null;
  currency: string;
  method: string;
  status: string;
  createdAt: string;
  expiresAt: string;
  tradeNo: string;
  checkoutWarning?: string;
  qrCode?: string;
  payUrl?: string;
}
export interface RechargeOverview {
  enabled: boolean;
  email: string;
  balance: number | null;
  currency: string;
  minAmount: number;
  maxAmount: number;
  methods: RechargeMethod[];
  orders: RechargeOrder[];
}
export interface WalletBalance {
  email: string;
  balance: number | null;
  currency: string;
}
export interface Announcement {
  id: number;
  title: string;
  content: string;
  notifyMode: string;
  readAt: string | null;
  createdAt: string;
}
