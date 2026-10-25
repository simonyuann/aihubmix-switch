# Tauri 2 迁移与 v0.2.0 发布计划

用户已批准以「我的 API + 按需导入」为中心的方案，并授权迁移 Rust、支持 macOS 和发布 GitHub Release。

- [x] 检查现有适配器、账户接口、存储和发布权限。
- [ ] 替换 Electron 为 Tauri 2，React + Vite，Rust 完成账户、网络、系统凭据存储和配置操作。
- [ ] 原子写入、JSONC/TOML 保留无关配置、按智能体备份还原、记录每个智能体的 Key 指纹。
- [ ] 实现我的 API、注册登录、创建/查看/复制、按需导入、已接入、充值占位和高级设置。
- [ ] 覆盖 Rust 配置读写/还原与多 Key、网络响应解析和前端关键交互测试。
- [ ] 验证浅色界面和 macOS 本地构建，配置 macOS arm64/x64 与 Windows x64 CI。
- [ ] 推送经过验证的版本，发布 v0.2.0，等待 CI 产物并确认 Release 下载资产。

旧 Electron state.json 与 backups 保留，新设置文件 state-v2.json、备份 backups-v2；原先的加密 Key 需要重新登录或粘贴，文档明确说明。macOS 未提供 Developer ID 证书时使用 ad-hoc 签名并在 Release 说明中如实标注，不能声称已经公证。
