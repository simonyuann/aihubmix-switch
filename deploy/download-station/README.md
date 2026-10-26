# CSMY 下载站更新源

这些文件运行在现有下载站 `/srv/hdd/download-station`，不需要客户端访问 GitHub。只从正式 Release 同步，SHA256 全部通过且六种更新格式、清单版本、包地址与签名文件一致后才切换 `data/current`。客户端继续使用固定公钥验证签名及版本。

部署时先备份现有 `server.py`、`sync_release.py`、`Dockerfile`，再应用 `integration.patch`，将本目录 `sync_release.py`、`update_feed.py` 复制到下载站根目录。将下载站 `package-source.py` 的源码文件列表加入 `update_feed.py`，重新生成公开源码包，然后 `docker compose up -d --build`。原服务的数据目录、持久化文件和凭据无需改动。

`/updates/aihubmix-switch/latest.json` 返回当前完整版本的清单；`/updates/aihubmix-switch/v<版本>/<文件名>` 返回相应签名包，均直接读取本地磁盘，不代理上游。旧版本目录保留 7 天，公开目录仍只展示当前版本。服务器启动后同步一次，以补齐已有 Release 的清单和签名；同步失败保持上一个完整版本。

同步器默认读取 GitHub Release API；共享出口触发未认证 API 限流时，会自动从 Release 的公开 `latest.json` 和 `SHA256SUMS.txt` 发现同一稳定版本并继续同步，不需要在服务器保存 GitHub Token。

本地回归：`python3 -m unittest discover -s deploy/download-station -p 'test_*.py'`。测试覆盖校验失败不切换、异常清单不切换、版本切换后旧包可下载、保留期限、目录限制及手工持久化文件保留。
