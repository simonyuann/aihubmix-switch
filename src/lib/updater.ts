import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";

export type UpdateChannel = "stable" | "beta";

export interface UpdateInfo {
  currentVersion: string;
  availableVersion: string;
  notes?: string;
  pubDate?: string;
}

export interface CheckOptions {
  timeout?: number;
  channel?: UpdateChannel;
}

export async function getCurrentVersion(): Promise<string> {
  try {
    return await getVersion();
  } catch {
    return "";
  }
}

export async function checkForUpdate(
  opts: CheckOptions = {},
): Promise<
  { status: "up-to-date" } | { status: "available"; info: UpdateInfo }
> {
  // 动态引入，避免在未安装插件时导致打包期问题
  const { check } = await import("@tauri-apps/plugin-updater");

  const currentVersion = await getCurrentVersion();
  const target = await invoke<string>("product_update_target");
  const update = await check({ timeout: opts.timeout ?? 30000, target });

  if (!update) {
    return { status: "up-to-date" };
  }

  try {
    return {
      status: "available",
      info: {
        currentVersion,
        availableVersion: update.version,
        notes: update.body,
        pubDate: update.date,
      },
    };
  } finally {
    await update.close();
  }
}

/**
 * 检查后端侧可用更新,返回新版本号;无可用更新时返回 null。
 */
export async function checkAppUpdateAvailable(): Promise<string | null> {
  return (await invoke<string | null>("check_app_update_available")) as
    | string
    | null;
}

/**
 * 下载并安装更新后重启。成功时后端会下载+安装+重启,不会返回;
 * 返回 false 表示(竞态)检查时有更新、安装时已无。
 */
export async function installUpdateAndRestart(): Promise<boolean> {
  return (await invoke<boolean>("install_update_and_restart")) as boolean;
}
