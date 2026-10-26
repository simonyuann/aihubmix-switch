# CC 卡片与功能恢复验证（2026-10-26）

本次调整覆盖此前专属客户端报告中的功能裁剪。保留账号登录门禁；恢复 CC 官方卡片、设置中的应用配置、MCP、Skills、提示词。授权中心与通用供应商新增入口继续隐藏。

## 当前行为

- 侧栏继续使用 CC 原组件与顺序；充值、API 管理依次排列在 MCP 上方，使用原 NavItem 样式。余额移到工具页右上角「添加分组」左侧，纯展示，不跳转 API 管理。
- 工具页直接使用 CC ProviderCard。官方卡片由 CC 原种子提供，启动调用 CC 恢复/切换；平台卡片按可用分组及工具支持的协议创建；默认保留首个分组，用户可添加其他分组卡片。卡片内按 groupId 下拉筛选 active API Key，不编辑地址、模型或任意供应商配置。未来新增 DeepSeek、GLM 等分组可刷新后添加，无品牌硬编码。
- 平台地址固定。服务卡片 ID 按账号、工具、服务分隔；后台校验服务来自平台可用组，并校验所选 Key 属于当前账号、该精确分组且仍为 active。空卡片与所选 Key ID 按账号保存，完整凭据不回传卡片编辑器。卡片列表只回传显示信息与脱敏 Key。
- 原生切换、文件投影、校验、协议路由仍由 CC ProviderService 与 mode controller 执行。模型目录来自平台 `/v1/models`；Codex 使用 CC modelCatalog，其他需要原生目录的工具自动生成声明，桌面无模型选择器。Claude/Gemini/Codex 不写用户选择的模型或推理强度。
- 官方配置只允许 CC 种子的原始配置，不能通过标记任意行为 official 绕过平台限制。平台目录请求失败时，已验证的登录会话仍可看到官方卡片；MCP、Skills、提示词使用原命令，仍受登录门禁限制。
- 充值弹窗挂载到根层，使用带内边距的滚动正文与关闭按钮；订单创建、重复提交限制、核验和不确定结果处理保持此前逻辑。申请 API 不会在打开窗口时自动创建 Key。

## 会话和用量

与 CC 导入基线 `718e675` 比较：用量组件、统计服务及命令未修改；会话差异仅为既有品牌/产品路径替换。此次没有改动统计、解析器或会话显示模块。

之前打开的测试包使用 `CC_SWITCH_TEST_HOME`，该目录没有历史日志，真实工具目录有历史记录。将 Claude 7 个、Codex 53 个 JSONL 文件复制到测试 home 后，原测试包无需改代码即可在 CC 会话页面显示 31 个 Codex 会话，并在用量页自动同步得到 1,869 条当天请求记录及图表。文件数与会话数不同，沿用 CC 对日志类型与会话的识别规则。

复制的是本机日志副本，没有删除或编辑真实会话；未向平台上传会话内容。正式运行仍按 CC 默认读取本机工具目录；测试 home 仅用于本地隔离验收。

## 自动检查

- TypeScript 类型检查通过。
- 前端全量首轮：264 个文件、3,350 项；256 文件通过，8 文件出现旧断言或超时。修正新的导航断言后，本次账户/卡片/钱包/侧栏 4 文件 28 项通过；最后界面变更另跑 2 文件 15 项通过。其余失败文件降低并发重跑后通过，3 项 5 秒超时测试使用单 worker、20 秒上限复核通过；未修改上游表单实现来规避超时。
- 原生全量回归：18 个测试目标，4,393 项通过，11 项上游忽略。账户、支付、代理测试使用本机临时 HTTP 服务，需要解除 localhost 沙盒限制；无真实账号交易。
- 上述全量原生回归完成后，模型目录与协议地址追加修正的账户/原生目录定向测试 25 项通过；后续分组卡片与 API 管理变更另做定向回归（见最终验收记录）。
- Rust fmt、Clippy（沿用既有 deprecated/for_kv_map/result_large_err 例外）及前端格式检查通过；macOS ARM64 Debug 未签名应用和压缩包已成功构建，最终分组版本重新打包。

## 验收边界

注册/找回密码邮件、创建真实 API Key、充值付款与实际模型推理没有由代理提交。登录状态由用户自行登录；代理未代填真实凭据。当前只制作 macOS ARM64 本地未签名 Debug 测试包，未推送、发布或部署。

## 最终分组与管理验收

- API 管理复用现有 keys/get-key/delete-key 后端，支持复制与删除确认。失败时保留列表并要求刷新核对；不自动重试删除。
- 初版新版窗口已实测 CC 官方/平台卡片、仅 Key 编辑、侧栏顺序；充值弹窗已确认有内边距、可滚动且关闭正常。后续以平台分组选择 Key 的编辑器取代手填 Key。
- 独立验收窗口使用不同 bundle identifier，未关闭原用户窗口；原关闭操作曾被自动审批拒绝（可能丢失未保存窗口状态），已采用独立窗口完成可执行的检查。

- 最终分组版本：TypeScript、Prettier、Rust fmt、`cargo clippy --locked --lib` 通过。前端 4 文件 23 项通过；原生平台模块 27 项通过。测试覆盖新增 GLM 分组卡片、同协议不同分组 Key 隔离、禁用/本机 Key 排除、删除确认/防重复/失败保留、复制不展示明文及旧配置兼容。
- 最终原生包构建成功（175.49 MiB）。使用独立 `CC_SWITCH_TEST_HOME` 启动，实机已看到「余额 → 充值 → API 管理 → MCP」顺序、右上角「添加分组 / 申请 API」、Pi 平台卡片，以及同组 API Key 下拉选择器。用户正在自行操作新版窗口，未关闭其窗口或提交真实 Key 删除。
- 最终产物：`artifacts/local/group-cards/AIHubMix Switch.app`；压缩包 `artifacts/local/AIHubMix-Switch_2.0.0_macos-arm64-group-cards-test.zip`。均为本地 macOS ARM64 未签名 Debug 测试包，未发布。
- 对验收前哈希清单复核，真实 Claude/Codex/Gemini 工具配置未变化；应用自身 settings.json 在 UI 使用期间创建。独立验收使用日志副本，保留真实工具会话与配置。

## 分组选择无法点击修复

现场在「申请 API」列表确认平台返回 `deepseek · deepseek` 和 `openai · openai`。原兼容性检查只识别协议名，把真实 `platform=deepseek` 过滤了，剩余 OpenAI 分组又已存在卡片，最终导致添加窗口没有可选项并禁用下拉框。此前使用 `platform=openai` 的品牌分组夹具未覆盖这个情况。

修复在筛选前把 deepseek/glm/zhipu/zhipuai 映射为 CC 的 openai_chat 协议。Pi/OMP/OpenClaw 用原生 openai-completions，OpenCode 用 CC 既有 openai-compatible SDK，MCode/Kimi/Hermes 使用各自的 Chat 配置格式；Claude/Codex 复用 CC Chat 转换与 mode controller，Codex Chat 进入 CC 路由。分组 ID 和 Key 归属校验保持独立，不能因协议相同而混用 Key。真实分组名仍来自平台。

## 最新分组规则

根据用户补充，添加入口更名「添加分组」。列表由 sub2api 当前可用分组、工具协议支持、当前账号 active API 的交集生成；平台存在 GLM 但账号未申请 GLM Key 时不显示，申请到 Key 后刷新出现。已有分组显示「已添加」且禁选；打开窗口自动重新获取数据。申请 API 仍显示平台允许申请的分组，供用户先申请 Key。

协议适配进一步去除品牌白名单：已知原生协议保持 CC 适配，其他 sub2api 兼容平台类型使用 CC Chat Completions。分组仍按 ID 隔离。新增回归验证没有 GLM API 时不可添加、只有 disabled API 时不可添加、获得 active API 后可添加，以及未来类型可经 CC Chat 适配。

## 最终交付检查

最终规则的前端 4 文件 23 项、原生平台模块 29 项全部通过；TypeScript、Prettier、Rust fmt、Clippy lib 检查通过。macOS ARM64 未签名 Debug 应用构建成功（175.53 MiB），ZIP 完整性检查通过。

分组规则阶段包：`artifacts/local/platform-groups/AIHubMix Switch.app`；`artifacts/local/AIHubMix-Switch_2.0.0_macos-arm64-platform-groups-test.zip`。最新交付见下方余额布局调整。

真实 DeepSeek 筛选问题已在旧窗口复现，并确认平台类型值为 deepseek；修复后独立 Fix 窗口曾显示非禁用的平台分组下拉及 DeepSeek 卡片。最后加入「添加分组」更名、显示已添加分组及按已有 API 筛选后，已再次跑完自动回归并构建。最终 Groups 验收窗口已启动，但 CUA 原生管道报 `Sky Computer Use native pipe startup failed`，重置重连仍失败，因此未宣称最后一轮实机点击路径验证完成。该限制不影响已完成的代码与包交付。

## 余额布局调整（最新交付）

按用户反馈移除侧栏余额入口，在工具页头部「添加分组」左侧显示余额；使用普通文本容器，无点击操作。复用 AccountContext 的余额与币种格式，未知余额保持「—」。左下角充值、API 管理仍在 MCP 上方。

更新原有集成回归，验证余额位于页头且在添加分组前、不是按钮或链接、侧栏不含余额，并通过 API 管理退出登录。前端 4 文件 23 项、TypeScript、相关 Prettier 检查与 git diff --check 均通过。本次只修改前端显示，未重复执行已通过的原生平台 29 项回归。

重新构建 macOS ARM64 未签名 Debug 应用成功（175.53 MiB），ZIP 完整性检查通过。最新包：`artifacts/local/header-balance/AIHubMix Switch.app`；`artifacts/local/AIHubMix-Switch_2.0.0_macos-arm64-header-balance-test.zip`。其他目录保留为此前阶段产物。

再次尝试 CUA 应用列表仍返回 `Sky Computer Use native pipe startup failed`，因此没有完成本次最终视觉复核，未关闭或替换用户正在运行的旧窗口。
