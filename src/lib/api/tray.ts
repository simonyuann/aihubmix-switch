import { invoke } from "@tauri-apps/api/core";
import type { AppId } from "./types";

/** 托盘导航的原始载荷(对应后端 `tray::TrayNavigation`) */
export interface RawTrayNavigation {
  app?: string;
  section?: string;
  intent?: string;
  providerId?: string;
}

export const trayApi = {
  /** 取走(消费)一次待处理的托盘导航;事件 "tray-navigate" 触发后调用 */
  async takeNavigation(): Promise<RawTrayNavigation | null> {
    return (await invoke("take_tray_navigation")) as RawTrayNavigation | null;
  },

  /**
   * 主界面正显示某个应用的供应商页:告诉托盘这个应用的问题行处理过了
   * (规格「打开过对应页面」)。
   */
  async reportAppPageSeen(app: AppId): Promise<void> {
    await invoke("tray_app_page_seen", { appType: app });
  },
};
