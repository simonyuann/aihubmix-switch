import { invoke } from "@tauri-apps/api/core";

/**
 * 配置加载错误载荷(对应后端 init_status 的 get_init_error)。
 * kind 为 "db_version_too_new" 表示数据库版本过新,渲染应用内升级恢复界面。
 */
export interface ConfigLoadErrorPayload {
  path?: string;
  error?: string;
  kind?: string;
}

export const initApi = {
  /** 启动早期主动查询后端初始化错误,避免事件竞态 */
  async getInitError(): Promise<ConfigLoadErrorPayload | null> {
    return (await invoke("get_init_error")) as ConfigLoadErrorPayload | null;
  },

  /** 配置迁移结果(启动后一次性查询;true 表示本次启动完成了迁移) */
  async getMigrationResult(): Promise<boolean> {
    return (await invoke("get_migration_result")) as boolean;
  },

  /** 技能 SSOT 迁移结果;null 表示没有待报告的迁移 */
  async getSkillsMigrationResult(): Promise<{
    count: number;
    error?: string;
  } | null> {
    return (await invoke("get_skills_migration_result")) as {
      count: number;
      error?: string;
    } | null;
  },
};
