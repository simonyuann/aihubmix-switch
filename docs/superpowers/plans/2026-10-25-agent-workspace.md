# 智能体工作台首批执行计划

## 最新版本安排

- [x] 0.3.0：补齐 CC Switch 13 种智能体的原生配置/专用启动方式，保留简洁导入目录。
- [x] Gemini 原生两轮流式工具验证、Claude Desktop 独立网关 profile、Hermes/MCode/OMP YAML 保护、agy 显示名与专用启动命令。
- [x] 更新功能说明与接入限制，避免把手动指南计入自动配置。
- [x] 0.3.0 最终检查、push、合并与四平台签名 Release。
- [ ] 0.3.1：Codex CLI 统一续聊历史、可选迁移和条件还原（按用户最新安排，不纳入 0.3.0）。
- [ ] 0.3.2：优先连接诊断和已接入快捷操作。

以下保留首批开发记录，状态以本段为准。

0.3.0 最终本地验证：64 项 Rust 回归通过、3 项显式线上/原生演练忽略；20 项界面交互与 2 项签名清单测试通过，TypeScript、Clippy（零警告）、Rust 格式与 Web 构建通过。浏览器假数据预览已检查 13 种目录与新增工具说明。发布将由 tag 工作流再完成各目标平台的原生回归和签名打包。

0.3.0 已发布：源码 `e747d34`，PR #1 已合并；[发布工作流 40207780524](https://github.com/simonyuann/aihubmix-switch/actions/runs/40207780524) 的基础检查、macOS arm64 / Intel、Windows x64、Linux x64 与发布任务全部通过。Windows 原生测试通过 npm 式启动器核对实际进程的参数与环境变量，覆盖受限脚本策略及中文、空格、引号路径。

[公开 Release](https://github.com/simonyuann/aihubmix-switch/releases/tag/v0.3.0) 于 2026-10-25 15:47:41 UTC 发布。重新下载全部 16 个文件，逐项核对 GitHub SHA256、文件大小和校验清单，并使用客户端已有公钥验证六种更新格式的签名。匿名请求生产 `releases/latest/download/latest.json`，内容与已验证的 0.3.0 清单一致，SHA256 为 `bc3e0d2206c7299b28e07394bfbf7d6e5cfa8d87134f997d66ec1d12dd5d5416`。未安装替换用户电脑上的应用，也未用真实用户凭据逐一运行全部第三方智能体。

- [x] 核对 CC Switch 真实功能与源码，用户确认分阶段顺序并将会话历史纳入范围。
- [x] 写明主流程、目录/会话交互、原生与手动接入边界、后续能力范围和验收标准。
- [x] 搜索目录与共享元数据，保留已接入列表简洁。
- [x] Grok Build、Kimi Code、OpenClaw 原生配置及检测、协议验证、备份还原。
- [x] 只读统一会话服务、来源状态、搜索阅读与恢复命令。
- [ ] 后续批次单独实现 CC Switch 同类的「Codex CLI 统一续聊历史」开关：provider 分组兼容、可选旧历史迁移、JSONL/SQLite 一致性、备份与条件还原。会话浏览页完成不代表该项完成。
- [x] 前端和 Rust 回归、UI 检查、四平台构建。
- [x] 首批开发代码 push GitHub，建立草稿 PR #1，保留稳定版更新渠道。
- [x] 后续完善后发布可通过 v0.2.2 更新的新版本。

按用户最新顺序，先发布不含工作台扩展的 v0.2.2（已完成），再在 `feat/agent-workspace` 继续开发。首批本地回归：51 项 Rust 测试与 19 项界面测试通过，类型检查 / Clippy / Web 构建通过；会话预览布局已检查。自动适配使用临时配置和模拟模型服务验证，未使用用户真实凭据逐一调用新增工具。

首批构建提交 `b679565`：工作流 `40201707049` 完成 macOS arm64 / Intel、Windows x64、Linux x64 的签名打包、原生更新格式检查与配置 / 会话回归。PR：https://github.com/simonyuann/aihubmix-switch/pull/1 。0.3.0 仍为开发草稿，未打发布 tag，公开 latest 保持 0.2.2。
