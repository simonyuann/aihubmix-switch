# 优化任务池(Task Pool)

> 熵点清单。每项:证据 → 方向 → 优先级 → 状态 → 验证计划。
> 状态:`open` / `in-progress` / `done` / `deferred(原因)` / `blocked(待确认)`。
> 规则:任何重构批次必须闭环(迁移调用方+删除旧实现+验证),未闭环不标 done。

## P0 — 立即可做、可验证

### T1 双份构建配置归一 `open`
- 证据:`vite.config.ts` 与 `vite.config.mts`、`vitest.config.ts` 与 `vitest.config.mts` 并存,`tsconfig.json/web/node` 三份,职责划分待勘察确认。
- 方向:确定实际被消费的一份(查 package.json scripts / 引用),删除另一份;若两份各有用途,显式命名并写明理由。
- 验证:tsc + vitest 全绿 + 构建脚本可用。

### T2 版本号 Single Source of Truth `open`
- 证据:版本出现在 `package.json`(1.2.1)、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` —— 是否手工三处同步待确认。
- 方向:单一权威源 + 构建期校验/同步脚本(防回归约束)。
- 验证:校验脚本对三处一致性断言;故意改错能被抓住。

### T3 前端 invoke 调用归一 `open`
- 证据:待勘察(前端代理报告中量化 lib/api 封装外的散落 `invoke(` 调用)。
- 方向:所有 Tauri invoke 收敛到 lib/api 层深模块;类型化命令契约。
- 验证:tsc + vitest + AST/Lint 规则禁止 lib/api 之外 import `invoke`。

## P1 — 高价值,依赖勘察报告细化

### T4 `commands/misc.rs`(10952 行)拆分 `open(验证受限)`
- 证据:剪贴板/外部 URL 校验/应用更新/便携模式/迁移结果/8 个工具的版本探测+生命周期(install/update/uninstall)/semver 解析/WSL+PowerShell 包装混在一个文件;存在 4 组 `#[cfg]` 平台重复函数(`tool_env_type_and_wsl_distro`×4、`mcode_installer_update_command`×2、`codex_installer_update_command`×2)。
- 方向:按领域拆为 `commands/tool_lifecycle.rs`、`commands/tool_versions.rs`、`commands/app_updates.rs`、`commands/clipboard.rs` 等;`#[cfg]` 平台变体收敛为平台模块(参照 codex_desktop_platform.rs 模式)。
- 验证:cargo check + cargo test(受本机 MSVC 缺失约束,见 README 验证矩阵)。**依赖 T0 工具链结论**。

### T5 `*_config.rs` 家族收敛为深度模块 `open(验证受限)`
- 证据:14 个文件 ~25K 行(`codex_config.rs` 6644 行/187 fn、`claude_desktop_config.rs` 2885 行/114 fn、`dsh_config.rs` 3544 行…),全部平行暴露 `get_*_dir()` / `get_*_config_path()` / `get_*_env_path()` 形状的函数;`jsonc_document.rs` 已存在但家族内部使用程度待勘察。
- 方向:抽"tool-config 引擎"深模块(定位家目录、路径约定、受管块读写、迁移检测),每工具退化为薄声明(路径模板 + 工具特有语义,如 codex 的 `experimental_bearer_token`、gemini 的 `.env` 管理块)。分工具分批迁移,每批迁移后删除旧实现。
- 验证:cargo check + cargo test;每工具迁移都有对应 Rust 测试覆盖行为(读/写/迁移)。

### T6 跨层引用治理 `open(验证受限)`
- 证据:`services/ → crate::commands::` 17 处;`aihubmix/ → commands::` 1 处;`proxy/ → commands::` 1 处。依赖方向应为 commands(编排层)→ services(领域层)。
- 方向:逐处反转依赖:把被调逻辑下沉 services,或抽出共享内核模块;消灭 services 对 commands 的依赖。
- 验证:cargo check + 新增依赖方向静态检查(见 T9)。

### T7 死亡代码删除 `open`
- 证据:待勘察(前端/Rust 报告的零引用候选)。
- 方向:确认零调用方后删除;Rust 侧注意宏注册(`generate_handler`)不算零引用。
- 验证:tsc/vitest;Rust 侧 cargo check。

## P2 — 防回归约束建设

### T8 前端 Lint 与架构边界约束 `open`
- 证据:仓库当前仅有 prettier(format/format:check),无 eslint/biome,无边界检查。
- 方向:引入轻量 linter(倾向 oxlint/biome,速度优先)+ 自定义规则:禁止 lib/api 之外的 `invoke(`、禁止跨层 import(如 components→database 类)、强制命名约定。
- 验证:linter 在 CI/本地 script 跑通,并对违规样例报错。

### T9 依赖方向静态检查 `open`
- 证据:无任何架构约束自动化。
- 方向:前端 dependency-cruiser(或等效)规则:contexts→components 允许、components→components(跨子域)警示、lib/api 为唯一后端通道等;Rust 侧以测试或脚本检查 `use crate::` 方向(services 不得 use crate::commands)。
- 验证:对当前已治理模块通过;人工注入违规能被抓住。

### T10 i18n key 覆盖一致性检查 `open`
- 证据:待勘察(i18n 目录结构、key 是否有缺漏)。
- 方向:脚本断言各语言 key 集合一致 + 代码引用的 key 存在。
- 验证:脚本接入 CI。

## 待勘察补充(占位)

- 前端重复 UI 实现清单(设计系统一致性)— 勘察中
- s3_sync / webdav_sync / sync_protocol 平行实现关系 — 勘察中
- preset 数据文件(7215 行 openclaw / 4929 行 codex)与 Rust 侧 preset 的重复 — 勘察中
- 巨型文件治理优先级列表(components 97K 行的分布)— 勘察中
- 测试结构分片与重复 setup — 勘察中
