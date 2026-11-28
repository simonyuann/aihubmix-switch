import { invoke } from "@tauri-apps/api/core";

export interface CodexImagegenGuide {
  status: string;
}

export const codexImagegenApi = {
  /** 写入/更新 Codex 命令行生图的备用配置说明,返回处理状态 */
  async getSetupGuide(): Promise<CodexImagegenGuide> {
    return await invoke("codex_imagegen_guide");
  },
};
