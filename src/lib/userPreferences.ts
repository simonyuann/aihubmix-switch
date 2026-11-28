/**
 * 用户偏好/界面状态的本地持久化(localStorage 的单一触点)。
 *
 * 与 `localCache`(JSON 缓存,`cc-switch-cache:` 前缀)语义不同:这里保存的是
 * 用户可感知的偏好(主题、上次视图、语言、侧栏折叠……),丢失是功能回退而非
 * 缓存重建。因此:
 * - 键保持历史物理键不变,已存数据完全兼容;
 * - 读写都做 try/catch 硬化(隐私模式 / 配额满时静默降级,不抛异常打断 UI);
 * - 跨窗口共享的键(如主题,主窗口与托盘面板共用)也由本模块管理,物理键不变,
 *   共享语义保持。
 *
 * JSON 序列化由调用方负责(各偏好有自己的格式与默认值逻辑)。
 */

export function readPreference<T = string>(key: string): T | null {
  try {
    return (localStorage.getItem(key) as T | null) ?? null;
  } catch {
    // 隐私模式等 localStorage 不可用:当作没有保存过
    return null;
  }
}

export function writePreference(key: string, value: string): void {
  try {
    localStorage.setItem(key, value);
  } catch {
    // 写不进去就只在本次会话生效
  }
}

export function removePreference(key: string): void {
  try {
    localStorage.removeItem(key);
  } catch {
    // 删除失败无补救手段,忽略
  }
}
