# CC 分组与原功能恢复

使用 CC 原布局、ProviderCard、ProviderService、mode controller 及现有工具配置写入。官方卡片使用 CC 原种子及恢复逻辑。平台服务地址固定，禁止新增外部供应商、修改地址或选择模型；模型在 Agent 中通过 `/model` 切换，目录由平台 API 自动读取。

## 平台分组与 API

右上角为「添加分组」和「申请 API」。申请 API 读取 sub2api 当前账号可申请的分组。「添加分组」读取平台分组与当前账号 active API Key 的交集，并按工具支持的协议筛选；没有 GLM Key 时不显示 GLM，平台开放 GLM、用户申请到 Key 后才允许添加。打开添加窗口自动刷新，也提供手动刷新。已有分组显示「已添加」并禁止重复添加，每个账号/工具/分组对应一张卡片。

分组显示名及 ID 都来自 sub2api，不写死 OpenAI/DeepSeek/GLM 列表。已知 OpenAI Responses、Anthropic、Gemini 等类型使用 CC 对应协议；sub2api 的其他兼容服务类型使用 CC OpenAI Chat 适配。Pi/OMP/OpenClaw 使用 openai-completions；Claude/Codex 复用 openai_chat 转换，Codex Chat 启动进入 CC 本地路由。

创建分组卡片只保存本地分组引用，不写工具配置。卡片中下拉选择当前账号在该精确 groupId 下的 active Key；前后端均校验归属，即使两个分组协议相同也不混用。完整凭据在原生层取得，卡片编辑器只接收 Key ID 与脱敏显示。选好 Key 后保存或启动，沿用 CC 原服务。

## 原功能与账户布局

设置保留应用配置。MCP、Skills、提示词使用 CC 原页面及命令；会话、用量统计继续使用原代码。余额在工具页右上角「添加分组」左侧纯展示，不响应点击；充值、API 管理依次放在 MCP 上方，其余沿用 CC 原布局。账户弹窗根层挂载，避免侧栏裁切。

API 管理复用账户 Key 列表及后端，支持脱敏显示、复制、刷新、删除；删除使用 CC ConfirmDialog 确认并防止重复提交，删除成功清理卡片 Key 绑定。保留右上角申请入口。保留登录/注册/找回密码和登录门禁。

## 验收边界

使用模拟服务回归账号、充值、分组及 Key 选择；不创建/删除真实 Key 或订单，不代填真实凭据，不发布。会话空数据已确认为隔离测试 home 无日志；复制日志副本后 CC 会话和用量正常显示，不改统计逻辑。实机验收使用独立窗口和测试 home，保留用户现有窗口。
