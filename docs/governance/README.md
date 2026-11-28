# 架构治理(Governance)

> 目标:对整个代码库进行系统性的架构降熵、规范化归一、模块解耦和复杂度治理,演进为高度规整、极易理解、极易维护、适合大量 Agent 并行开发的工程系统。
> 分支:`Z_code_work`(隔离 worktree)。每批有效改进一个独立 commit。

## 工程性质(验收轴)

1. **Single Source of Truth** — 业务规则/数据定义/配置/状态/设计规范各拥有唯一权威来源。
2. **Canonical Implementation** — 同一语义只保留一种标准实现;平行实现必须迁移后删除。
3. **Deep Modules**(Ousterhout)— 复杂性封装在内部,以简单稳定接口暴露;消灭浅转发层。
4. **High Cohesion, Low Coupling** — 明确职责/所有权/依赖方向;消灭循环依赖、隐式耦合、共享可变状态。
5. **Locality of Change** — 改一个业务概念只改一个逻辑位置。
6. **Deletion over Accumulation** — 引入新标准实现后必须迁移调用方并删除旧实现;禁止新旧长期并存。
7. **Enforceable Architecture** — 已确立的规范转化为机器可执行约束(Lint/AST/契约测试/CI)。

## 治理循环(每轮必须闭环)

扫描 → 发现熵点 → 分析真实调用方 → 多方案对比 → 选总复杂度最低方案 → 建立行为基线/测试 → 完成迁移 → **删除旧实现** → 独立复审 → 执行验证 → 建立防回归约束 → 记录(PROGRESS/DECISIONS/TASKS)→ 下一轮。

**禁止**:只建新抽象/新目录/新接口而不迁移调用方;过度抽象;无价值目录搬迁;为行数牺牲可读性;合并语义不同的概念。

**Deletion Test**:如果删除某层抽象,复杂性会消失还是会扩散到调用方?只有真正封装复杂性的抽象才保留。

## 验证矩阵(本机能力)

| 验证手段 | 覆盖范围 | 状态 |
| --- | --- | --- |
| `pnpm typecheck`(tsc --noEmit) | 前端 TS 全量 | ✅ 基线绿 |
| `pnpm test:unit`(vitest) | 前端组件/集成/node 测试 | 基线:5 文件 flaky(13↔18 预存失败,详见 PROGRESS Round 0/T17) |
| `pnpm lint:arch`(dependency-cruiser) | 前端依赖方向/边界 | ✅ 0 违规(T9 落地) |
| `pnpm format:check` | 前端格式 | ✅ 绿(T11) |
| `cargo check` / `cargo check --all-targets` | Rust 全量类型检查(含测试目标) | ✅ GNU 目标通过;**本地 Rust 重构的类型级验证手段** |
| `cargo test` | Rust 行为级测试 | ⛔ 本机受阻:`webview2_com_sys` 无法以 rlib 形式链接(WebView2 COM 栈为 MSVC 导入库导向;GNU 目标 E0463)。行为级验证以 CI 为准;如需本地解锁需安装 MSVC Build Tools |
| `cargo fmt --check` | Rust 格式 | ✅ 基线绿 |

**本机 Rust 验证方法**:本机无 MSVC Build Tools,且 Git Bash 的 coreutils `link.exe` 会遮蔽 rustc 的链接器。使用 GNU 目标工具链规避:
`RUSTUP_TOOLCHAIN=1.95.0-x86_64-pc-windows-gnu cargo check [--all-targets]`
(`rust-toolchain.toml` 锚定 1.95.0 版本不变,仅 target 换为 gnu。`check --all-targets` 覆盖测试目标的类型正确性;行为级验证依赖 CI 的 MSVC cargo test。)

**本机 Rust 验证方法**:本机无 MSVC Build Tools,且 Git Bash 的 coreutils `link.exe` 会遮蔽 rustc 的链接器。使用 GNU 目标工具链规避:
`RUSTUP_TOOLCHAIN=1.95.0-x86_64-pc-windows-gnu cargo check/test`
(工具链已装,`rust-toolchain.toml` 锚定 1.95.0 版本不变,仅 target 换为 gnu。CI 的 MSVC 结果仍以 CI 为准,gnu 目标在 `target_os` 层面语义一致,`target_env` 敏感的代码极少。)

约束:无 MSVC Build Tools 时,涉及 proc-macro/链接的 cargo 任务可能失败;Rust 侧重构验证以实际可运行为准,不可验证的批次要降级为"结构只读分析 + 待验证标记"。

## 文档体系

- `PROGRESS.md` — 每轮治理日志(做了什么、验证结果、提交号)。
- `TASKS.md` — 优化任务池(熵点清单、证据、优先级、状态、验证计划)。
- `DECISIONS.md` — 架构决策记录(ADR,含"待确认"决策)。
- `ARCHITECTURE-MAP.md` — 架构地图/依赖图/状态所有权(随治理更新)。

## 待确认事项的处理

凡涉及产品决策、重大架构变更、破坏性迁移 → 记入 `DECISIONS.md` 的"待确认"节,不阻塞其他独立任务。
