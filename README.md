# AIHubMix Switch

基于 [my-cc](https://github.com/simonyuann/my-cc) 的专属 API 桌面客户端。沿用 CC 界面、十三种工具的本机配置、代理、协议转换、会话与用量服务，增加平台账户、API Key、余额和充值。

本次重构版本为 **1.1.0**，源码基线为 my-cc `42ef63471f0ed879913f8de0d0c7e3e580293193`。保留上游 MIT 许可，见 [第三方声明](THIRD_PARTY_NOTICES.md)。

## 使用

启动后先登录；支持邮箱验证码注册与找回密码。程序会向平台验证已保存的会话，登录失效后返回账户首页。找回密码通过平台邮件中的链接完成。

登录后，在左侧选择工具，添加已持有 API 的分组，再在卡片中选择对应的 API Key，点击「启动」。模型在对应 Agent 中通过 `/model` 切换。本机配置写入、代理和协议转换继续由 CC 原有服务处理。Claude Desktop 使用 CC 路由模式。

服务默认使用标准 HTTPS 地址 `https://csmy.fun`，旧版保存的 `https://csmy.fun:23457` 会在升级后自动迁移，未迁移的旧配置仍可继续使用；仅支持本平台 API。可以恢复工具官方配置，不支持添加第三方供应商。MCP、Skills、提示词、应用配置、会话和用量统计沿用 CC 原模块。

右上角依次显示余额、「添加分组」和「申请 API」；左下角提供充值和 API 管理，支持复制与删除 Key。申请 API 在确认后创建平台 Key，不会自动创建或收费；充值在明确提交后创建订单。结果不明时先刷新记录再操作。钱包按账户接口返回的币种与余额显示，API Key 配额单独显示。

## 数据与升级

应用标识继续使用 `fun.csmy.aihubmix-switch`。配置库位于 `~/.aihubmix-switch/`，沿用 CC 的数据库结构；不读取或覆盖独立安装的 CC Switch 数据库。

旧版系统应用数据目录中的 `aihubmix-switch/` 及原始备份保留。专属客户端不提供旧版供应商和本机 Key 的导入入口，请登录平台账户重新选择对应分组的 Key。已在重构版保存的其他供应商不会出现在列表或托盘中，也不能切换使用；程序不会删除用户原有工具配置文件。

## 开发

需要 Node.js 20+、pnpm 10.12.3、Rust 1.95、系统 Tauri 构建依赖。macOS 最低 12.0；Windows 使用 WebView2；Linux 需要 GTK 3、WebKitGTK 4.1、AppIndicator 与系统 Secret Service。

```bash
pnpm install --frozen-lockfile
pnpm dev
pnpm typecheck
pnpm test:unit
pnpm build:renderer
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo test --manifest-path src-tauri/Cargo.toml --locked
pnpm build:desktop
```

`pnpm dev:renderer` 只启动前端，真实账户与本机操作需要 Tauri 桌面环境。新增平台测试使用模拟 API 和临时目录，不发起真实充值。CC 原有 `CC_SWITCH_TEST_HOME` 可用于隔离验证环境。

## 构建与更新

保留 AIHubMix 更新公钥和下载站 `https://dl.csmy.fun:23457/updates/aihubmix-switch/latest-v2.json`，更新按 macOS App、Windows NSIS/便携版、Linux AppImage/deb 区分目标。更新器要求签名绑定版本并禁止降级。便携版通过原生打包格式检测并从 AIHubMix 发布页下载覆盖，后端拒绝便携版自动安装。重构版从 1.1.0 开始使用独立的 `latest-v2.json` 路径（沿用既有路径名）；旧 `latest.json` 仅提供已验证的最后一个 1.0.x 版本，没有兼容快照时返回 404，避免向 macOS 11 旧客户端提供不兼容安装包。首次从 1.0.x 升级至 1.1.0 需手动下载安装包，macOS 需 12.0+。本次仅发布 macOS Apple Silicon / Intel 和 Windows x64 安装版 / 便携版，Linux 暂缓；下载站不等待 Linux 即同步本次完整发布。

内部 Rust crate 和可执行文件继续采用 CC 名称以减少上游差异，安装包和公开下载文件使用 AIHubMix Switch 品牌。现有发布脚本保留 `AIHubMix-Switch_<版本>_<平台>` 命名，支持下载站同步；只有带版本 tag 的 CI 才能发布。此次开发分支不自动发布更新。

## 源码维护

产品适配集中于 `src-tauri/src/aihubmix/`、`src/components/aihubmix/`、`src/lib/api/aihubmix.ts`、`src/contexts/AccountContext.tsx`、`src-tauri/src/product_policy.rs`、少量导航/品牌/目录/更新配置和发布脚本。供应商配置、代理和工具功能均复用 CC；数据库文件名、schema 和内部模块名保留，方便与上游比对更新。
