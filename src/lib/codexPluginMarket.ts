import { Channel, invoke } from "@tauri-apps/api/core";

export interface CodexMarketProgress {
  phase: "checking" | "downloading" | "verifying" | "registering";
  downloaded: number;
  total: number;
}

export function codexMarketError(error: unknown, fallback: string) {
  const code =
    typeof error === "string"
      ? error
      : error instanceof Error
        ? error.message
        : "";
  if (code === "CODEX_MARKET_RATE_LIMITED")
    return "插件目录访问暂时达到次数限制，请稍后再试。";
  if (code === "CODEX_MARKET_GIT_NETWORK_ERROR")
    return "无法连接 GitHub 下载插件，请检查网络或代理设置后重试。";
  const known = new Set([
    "当前 Codex 版本不支持此插件管理方式，请更新 Codex 后重试",
    "当前 Codex 使用自定义数据目录；此平台暂无法确认桌面版会使用该目录，请先使用默认目录",
    "找不到可用的 Codex 命令行，请先安装或更新 Codex",
    "插件目录已更新，请刷新后再安装",
    "插件操作超时，请检查网络后刷新状态",
    "插件内容校验失败，请刷新目录",
    "无法连接插件文件，请检查网络",
    "无法下载插件文件，请稍后重试",
    "插件文件大小校验失败",
    "插件目录大小校验失败",
    "插件包含不受支持的文件类型",
    "无法连接插件下载站",
    "插件下载站暂不可用",
    "插件包下载失败",
    "插件包下载中断",
    "插件包校验失败",
    "插件内容校验失败",
    "请先开启社区插件市场",
    "插件目录已更新，请重新开启市场",
    "请重新开启社区插件市场以使用国内下载",
  ]);
  return known.has(code) ? code : fallback;
}

export interface CodexMarketPlugin {
  id: string;
  name: string;
  displayName: string;
  description: string;
  version: string;
  author: string;
  license: string;
  requiresAuth: boolean;
  installed: boolean | null;
  skills: number;
  tags: string[];
}

export interface CodexPluginCatalog {
  source: string;
  revision: string;
  plugins: CodexMarketPlugin[];
}

interface CodexPluginChange {
  pluginId: string;
  restartRequired: boolean;
}

interface CodexPluginMarketChange {
  revision?: string;
  restartRequired: boolean;
}

export const codexPluginMarket = {
  list: () => invoke<CodexPluginCatalog>("list_codex_market_plugins"),
  enable: (onProgress?: (progress: CodexMarketProgress) => void) => {
    const progress = new Channel<CodexMarketProgress>();
    progress.onmessage = onProgress ?? (() => {});
    return invoke<CodexPluginMarketChange>("enable_codex_plugin_market", { progress });
  },
  disable: () => invoke<CodexPluginMarketChange>("disable_codex_plugin_market"),
  install: (pluginId: string, revision: string) =>
    invoke<CodexPluginChange>("install_codex_market_plugin", {
      pluginId,
      revision,
    }),
  uninstall: (pluginId: string) =>
    invoke<CodexPluginChange>("uninstall_codex_market_plugin", { pluginId }),
};
