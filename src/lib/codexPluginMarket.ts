import { invoke } from "@tauri-apps/api/core";

export function codexMarketError(error: unknown, fallback: string) {
  const code = typeof error === "string" ? error : error instanceof Error ? error.message : "";
  return code === "CODEX_MARKET_RATE_LIMITED" ? "插件目录访问暂时达到次数限制，请稍后再试。" : fallback;
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
  enable: () => invoke<CodexPluginMarketChange>("enable_codex_plugin_market"),
  disable: () => invoke<CodexPluginMarketChange>("disable_codex_plugin_market"),
  install: (pluginId: string, revision: string) =>
    invoke<CodexPluginChange>("install_codex_market_plugin", { pluginId, revision }),
  uninstall: (pluginId: string) =>
    invoke<CodexPluginChange>("uninstall_codex_market_plugin", { pluginId }),
};
