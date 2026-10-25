# CC Switch 实现参考

本次核对开源仓库 [farion1231/cc-switch](https://github.com/farion1231/cc-switch)，参考提交 `de0a235ad6389ce4afa333b472cdee615a3f3987`（MIT）。本项目独立实现协议适配与更新状态管理，未复制其代码。

- `src-tauri/src/commands/settings.rs`：更新由 Rust 后端串联检查、下载、安装与重启；下载进度用事件传给前端。避免替换 macOS App 后依赖旧 WebView 再发重启命令。
- `src-tauri/tauri.conf.json`：固定更新公钥与 Release 清单，采用官方 Tauri updater。
- `src-tauri/src/proxy/providers/transform.rs`、`claude.rs`、`codex_chat_common.rs`：DeepSeek 工具调用需要回传 `reasoning_content`，转换时不能丢失；通用 OpenAI 接口不凭空注入该字段。

对应到本项目：`updates.rs` 后端完成验签、安装与重启；`protocol.rs` 保留上游实际返回的明文或加密 reasoning，用不透明 signature 在 Messages 会话中回放，Chat 转回 `reasoning_content`，Responses 原样回传 reasoning item。不伪造缺失的思考内容，也不全局关闭思考模式。

导入验证使用 `tool_choice: auto`，通过提示要求 `connection_check`，仍严格验证工具名、参数、调用 ID、工具结果读取和两轮流正常完成。DeepSeek 验证输出上限为 2048，避免思考占满原来的 512 token 限制。

官方协议依据：

- [DeepSeek Thinking Mode](https://api-docs.deepseek.com/guides/thinking_mode/)
- [DeepSeek Chat API](https://api-docs.deepseek.com/api/create-chat-completion/)
- [DeepSeek Responses API](https://api-docs.deepseek.com/guides/responses_api/)

已增加双协议模拟回归：拒绝强制 tool choice；第二轮缺少思考内容则拒绝；校验实际工具往返。未持有用户报错的 DeepSeek 分组 Key，因此不能把模拟测试称为该分组线上实测。
