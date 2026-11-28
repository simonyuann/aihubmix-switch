import { invoke } from "@tauri-apps/api/core";

export const windowApi = {
  /** 同步原生窗口主题（"light" | "dark" | "system"） */
  async setWindowTheme(theme: string): Promise<void> {
    await invoke("set_window_theme", { theme });
  },
};
