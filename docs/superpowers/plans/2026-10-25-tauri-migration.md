# Tauri 2 迁移与 v0.2.0 发布进度

用户已批准「我的 API + 按需导入」方案，并授权迁移 Rust、支持 macOS、Linux 和发布 GitHub Release；后续追加 Claude Code 兼容接入与 API Key 删除。

- [x] Electron 迁移为 Tauri 2，Rust 负责网络、系统安全存储、配置操作与备份。
- [x] 白色 API 工作台、注册登录、创建/查看/复制/删除、按需导入、已接入与充值占位。
- [x] 四种智能体配置适配器，保留 JSONC/TOML 注释、无关配置与原始还原基线。
- [x] Claude Code 本地兼容连接、回环地址鉴权、SSE、图片、工具调用、加密推理上下文与托盘运行。
- [x] 验证真实 AIHubMix 上游的两轮工具调用，并通过本机真实 Claude Code CLI 验证；不修改用户配置。
- [x] 类型检查、10 项前端交互测试、24 项 Rust 测试、严格 Clippy 和格式检查通过。
- [x] 初版 macOS 原生窗口、签名和安装包验证；原三平台 CI 已通过。
- [ ] 最终 macOS arm64/x64、Windows x64、Linux x64（AppImage/deb）全部构建成功。
- [ ] 合并主分支并发布 v0.2.0，核对全部下载资产和 SHA256。

旧版 state.json 和 backups 保留，新版使用 state-v2.json、vault-v2.bin、backups-v2。旧密钥需重新登录或添加。macOS 当前使用 ad-hoc 签名，未进行 Apple Developer ID 公证，发布说明如实标注。Linux 安全存储使用 Secret Service；未提供安全存储时不回退为明文。
