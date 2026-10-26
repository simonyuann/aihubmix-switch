# CSMY 下载站更新源

这些文件运行在现有下载站 `/srv/hdd/download-station`，不需要客户端访问 GitHub。只从正式 Release 同步，SHA256 全部通过且六种更新格式、清单版本、包地址与签名文件一致后才切换 `data/current`。客户端继续使用固定公钥验证签名及版本。

部署时先备份现有 `server.py`、`sync_release.py`、`Dockerfile`，再应用 `integration.patch`，将本目录 `sync_release.py`、`update_feed.py` 复制到下载站根目录。将下载站 `package-source.py` 的源码文件列表加入 `update_feed.py`，重新生成公开源码包，然后 `docker compose up -d --build`。原服务的数据目录、持久化文件和凭据无需改动。

`/updates/aihubmix-switch/latest.json` 返回当前完整版本的清单；`/updates/aihubmix-switch/v<版本>/<文件名>` 返回相应签名包，均直接读取本地磁盘，不代理上游。旧版本目录保留 7 天，公开目录仍只展示当前版本。服务器启动后同步一次，以补齐已有 Release 的清单和签名；同步失败保持上一个完整版本。

同步器先读取不消耗 API 配额的 `<repo>/releases/latest` 重定向（必要时退回 `releases.atom`）得到最新正式版本，再按该 tag 下载公开的 `SHA256SUMS.txt` 和 `latest.json` 组装清单并校验，因此发布后几十秒内即可发现新版本，正常轮询不占用未认证 API 的 60 次/小时配额。`api.github.com` 仅在两者都失败时兜底；在 Compose 中设置 `GITHUB_TOKEN` 可把兜底配额提升到 5000 次/小时。预发布 tag 会被跳过。单个资产若被代理截断或校验失败，会在同一次运行内重新下载（最多 3 次），不必等下一个轮询周期；`Content-Length` 与实际字节数不符时直接判定为截断。

本地回归：`python3 -m unittest discover -s deploy/download-station -p 'test_*.py'`。测试覆盖校验失败不切换、异常清单不切换、版本切换后旧包可下载、保留期限、目录限制及手工持久化文件保留。
