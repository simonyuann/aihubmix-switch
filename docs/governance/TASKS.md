# 优化任务池(Task Pool)

> 熵点清单。每项:证据 → 方向 → 优先级 → 状态 → 验证计划。
> 状态:`open` / `in-progress` / `done` / `deferred(原因)` / `blocked(待确认)`。
> 规则:任何重构批次必须闭环(迁移调用方+删除旧实现+验证),未闭环不标 done。

## P0 — 立即可做、可验证

### T1 双份构建配置归一 `done(4cd65a4)`
- 证据(勘察确认):`vite.config.mts`(引用不存在的 `src/renderer`)、`vitest.config.mts`(引用不存在的 `@shared`)、`tsconfig.web.json`(include 不存在目录)均为 Tauri 模板残留;Vite/Vitest 配置解析顺序 `.ts` 先于 `.mts`,零引用。
- 处置:三份死配置删除;`pnpm-workspace.yaml` 的 `allowBuilds`(模板占位文本,非合法字段)一并清除。
- 验证:`pnpm typecheck` ✅;删除均为零引用纯惰性操作。

### T2 版本号 Single Source of Truth `done(已由既有 CI 覆盖,降级为监控)`
- 证据(勘察更新):三处定义(package.json / tauri.conf.json / Cargo.toml)+ `build.yml:49-52` 在 tag 时断言一致,`prepare-desktop-release.yml`、两个 rebuild workflow 同样校验。结构性 SSOT 不成立,但有 CI 强制闸门兜底。
- 处置:保留现状(改造成单点定义需要动 Tauri 构建,收益低);待确认项关闭。后续若版本字段漂移由 CI 抓住。

### T2b 文档叙事与版本记录归一 `in-progress(其余待确认 P-ADR-004)`
- 证据(勘察):① README.md 与 README_ZH/DE/JA.md(上游 cc-switch 叙事)并存且叙事矛盾;② CHANGELOG.md 842KB 为上游遗留(无任何 1.2.x 条目),真实 release notes SSOT 是 `docs/releases/v*.md`,CHANGELOG 不在任何发布链路;③ ~~Node 版本三处漂移~~ → **已归一(bad4dc7)**:`.node-version` 22.12.0→24(与 CI 一致),README 最低要求同步;④ CONTRIBUTING.md 内部英文+中文双份重复;⑤ `CC_SWITCH_TEST_HOME` 文档散布 4 处。
- 修正:勘察初报"README 自述 1.1.0 为版本错误"不成立——`README.md:5` 的"重构版本为 1.1.0"是 fork 基线的历史陈述(源码基线 my-cc 42ef634),非当前版本声明,不改。
- 待确认(见 DECISIONS P-ADR-004):上游遗留文档(README_ZH/DE/JA、CHANGELOG.md、CONTRIBUTING 双语重复)的处置属于产品/品牌决策。

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

### T6 跨层引用治理 `done(收敛,生产反向依赖 0)`
- 证据:`services/ → crate::commands::` 17 处(其中 5 处为测试代码,重分类);`aihubmix/ → commands::` 1 处;`proxy/ → commands::` 1 处(类型)。
- 结果:R1 child_process(6)+ R2 tool_runtime(5)+ R3 codex_imagegen/global_proxy(2)+ R4 OAuth State 类型(1)共闭 14 处;**services/aihubmix/proxy 生产代码对 commands 的依赖全部归零**。
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

## P1.5 — 工具链勘察新增(2026-11-28 报告)

### T17 flaky 测试归因与稳定化(Windows 本地) `open`
- 证据:两次全量运行失败均集中于同样 5 个测试文件(`AppsPage.test.tsx` 124s、`PiProviderForm.test.tsx` 169s、`SwitchModePanel.test.tsx`、`ClaudeDesktopProviderForm.test.tsx`、`IconPicker.test.tsx`),失败测试数 13↔18 随负载漂移,单测 5-8s 超时。CI(Linux)状态待比对。
- 方向:静默复跑归因(真超时?Windows 路径?mock 时序?);逐文件稳定化(fake timer/放宽超时/隔离重试);目标:vitest 在本机可作可靠回归信号。
- 验证:连续 3 次全量运行零失败。

### T11 prettier 配置与覆盖范围 `done(91c805f)`
- 证据:有 prettier 依赖与 format scripts,**但零配置文件**(无 .prettierrc),glob 仅 `src/**`——tests/、scripts/、根配置均不在格式化范围。
- 方向:补 `.prettierrc`(与仓库现状风格一致,避免全量 reformat 噪音)+ 扩 glob 或显式声明排除;`format:check` 进 CI。
- 验证:format:check 通过且不产生大面积 diff。

### T12 Rust 依赖归一(json5/json-five 二选一等) `open(验证受限)`
- 证据:双 JSON5 解析器并存(`json5 0.4` 与 `json-five 0.3.1`,openclaw_config.rs 两者都碰);`serde_yaml 0.9` 上游停维护;`objc2` 双大版本并存(macOS 26 兼容 hack,Cargo.toml 有注释,需保留并说明)。
- 方向:json-five 迁移到 json5(或反向,取特性匹配者),删另一个;objc2 双版本加注释声明为受控例外。
- 验证:cargo check + 相关模块测试。

### T13 构建工作流去重(build.yml vs rebuild-*.yml) `open`
- 证据:两条发布路径的 bundle 步骤逐行复制粘贴(相同矩阵/脚本序列,rebuild 少 Linux);flatpak 手工路径未接任何 workflow。
- 方向:抽取可复用 composite action(或 workflow 内 YAML anchor 等价物);flatpak 标记为手工旁路并在文档注明。
- 验证:workflow 语法检查(actionlint 或 dry-run);下次 tag 实证。

### T14 环境变量读取归一 `open(验证受限)`
- 证据:三种前缀并存(`CC_SWITCH_`/`CCS_`/`AIHUB_`);`DSH_HOME` 4 个模块各自读取;`XDG_DATA_HOME` 4 文件重复解析;`KIMI_CODE_HOME` 3 处读取;`CC_SWITCH_TEST_HOME` 保存/恢复样板复制粘贴 4+ 份(devDep 已有 serial_test 未用于此)。
- 方向:建 `env_resolver` 深模块(单一读取点+类型化访问器);测试 env 样板统一到 serial_test 辅助宏;前缀规范写入 DECISIONS。
- 验证:cargo test;grep 断言单点读取。

### T15 构建/发布脚本命名与注释规范化 `open`
- 证据:scripts/ 11 个发布辅助脚本职责清晰但 generate-download-manifest.mjs 注释要求"与外部仓库 schema 手工同步"(漂移隐患);nextest.toml 声称 CI 用 cargo-nextest 但 CI 实际用 cargo test。
- 方向:修正 nextest.toml 注释或接 nextest;schemas 同步检查脚本(低优先)。
- 验证:人工复核 + CI 绿。

## 待勘察补充(占位)

- 前端重复 UI 实现清单(设计系统一致性)— 勘察中
- s3_sync / webdav_sync / sync_protocol 平行实现关系 — 勘察中
- preset 数据文件(7215 行 openclaw / 4929 行 codex)与 Rust 侧 preset 的重复 — 勘察中
- 巨型文件治理优先级列表(components 97K 行的分布)— 勘察中
- 测试结构分片与重复 setup、13 个预存失败测试的归因 — 勘察中
