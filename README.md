# AIHubMix Switch

基于 [my-cc](https://github.com/simonyuann/my-cc) 的 AIHubMix 桌面客户端。界面、供应商管理、配置、代理、协议转换、官方授权、会话、MCP、Skills、提示词和用量功能直接采用 CC Switch；独立增加 AIHubMix 平台账户、Key、钱包与充值。

本次重构版本为 **2.0.0**，源码基线为 my-cc `42ef63471f0ed879913f8de0d0c7e3e580293193`。保留上游 MIT 许可，见 [第三方声明](THIRD_PARTY_NOTICES.md)。

## 使用

沿用 CC 的侧栏与工具页面，支持其全部 13 种工具。新增的「AIHubMix 账户」入口用于登录、管理平台 Key 和本机 Key、查看钱包、充值，以及将 Key 保存为 CC 供应商。

添加供应商时选择目标工具、模型和适用的推理强度。保存后在对应工具页面切换或应用配置；配置文件、代理和启动由 CC 原有服务处理。AIHubMix 平台登录与侧栏「授权中心」中的工具官方登录相互独立。

服务地址可配置，默认 `https://csmy.fun:23457`。钱包余额按账户接口返回显示，不能将 API Key 配额视为钱包余额。充值只在明确提交后创建订单；结果不明时先刷新订单记录再操作。

## 从旧版升级

应用标识继续使用 `fun.csmy.aihubmix-switch`。新配置库位于 `~/.aihubmix-switch/`，沿用 CC 的数据库结构；不读取或覆盖独立安装的 CC Switch 数据库。

旧版账户、Key、设置与备份位于系统应用数据目录的 `aihubmix-switch/`。在「AIHubMix 账户」选择导入旧版数据后，程序先验证加密存储，再建立独立快照；原始 `state-v2.json`、`vault-v2.bin` 和 `backups-v2/` 保留。已导入时不会重复覆盖。钥匙串锁定或 vault 损坏时保留原文件并显示错误。

导入平台快照不会自动修改工具配置或会话。旧工具/模型元数据随快照保留，供应商通过新界面选择并保存。旧备份有单独的打开入口，不冒充 CC 的原生还原记录。

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

保留 AIHubMix 更新公钥和下载站 `https://dl.csmy.fun:23457/updates/aihubmix-switch/latest.json`，更新按 macOS App、Windows NSIS/便携版、Linux AppImage/deb 区分目标。更新器要求签名绑定版本并禁止降级。便携版从 AIHubMix 发布页下载覆盖。

内部 Rust crate 和可执行文件继续采用 CC 名称以减少上游差异，安装包和公开下载文件使用 AIHubMix Switch 品牌。现有发布脚本保留 `AIHubMix-Switch_<版本>_<平台>` 命名，支持下载站同步；只有带版本 tag 的 CI 才能发布。此次开发分支不自动发布更新。

## 源码维护

产品适配集中于 `src-tauri/src/aihubmix/`、`src/components/aihubmix/`、`src/lib/api/aihubmix.ts`、少量导航/品牌/目录/更新配置和发布脚本。供应商配置、代理和工具功能均复用 CC；数据库文件名、schema 和内部模块名保留，方便与上游比对更新。
