# 在 GitHub 网页准备并发布桌面版本

正常标签构建成功后，`Build and release` 会自动创建草稿。已有标签通过 `Rebuild desktop release artifacts` 补构建时，使用以下流程，无需在本地下载或重新编译。

1. 确认补构建的三个平台全部成功，在运行页面 URL 中复制运行编号。
2. Actions → **Prepare desktop release draft** → **Run workflow**，选择 `main`。
3. `tag` 填现有标签，例如 `v1.1.2`；`run_id` 填成功的补构建编号，例如 `40450371173`。
4. 等待流程成功。它会在 GitHub 内下载三个平台的产物，验证版本和生产更新签名，生成更新清单与 SHA256，创建草稿，并逐项核对上传文件的大小及 GitHub SHA256。
5. 从运行 Summary 的草稿链接进入 Release，点击 **Edit**，确认 **Set as the latest release**，再点击 **Publish release**。

流程不会公开发布，也不会操作下载站。公开后，下载站沿用现有定时同步机制。

来源必须是从 `main` 手动触发且全部成功的补构建，三个构建产物须未过期。产物的版本及签名还必须与所选标签匹配。已有草稿或公开版本会阻止重复创建；若流程在上传后失败，检查日志和现有草稿，修复并重新验证后再公开，不能把失败流程视为校验通过。
