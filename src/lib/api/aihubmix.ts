import { invoke } from "@tauri-apps/api/core";
import type { Provider } from "@/types";
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
export interface AccountState {
  relayBaseUrl: string;
  account: { loggedIn: boolean; email?: string; userId?: string | number };
  localKeys: KeyInfo[];
  legacyData: boolean;
  storageError: string | null;
}
export const dispatch = <T>(command: string, payload?: unknown): Promise<T> =>
  invoke("aihubmix_dispatch", { command, payload: payload ?? null });
export const aihubmixApi = {
  state: () => dispatch<AccountState>("state"),
  draft: (payload: unknown) => dispatch<Provider>("provider", payload),
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
