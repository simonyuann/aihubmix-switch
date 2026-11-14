# Claude Desktop 中文启动

Based on javaht/claude-desktop-zh-cn (MIT), pinned to
`fc3fb5e782b933c40dbac89801ec6db36a8c7358`.
Upstream: https://github.com/javaht/claude-desktop-zh-cn
Copyright and license are retained in LICENSE.

AIHub integration changes: explicit installation selection; no runtime downloads,
transcripts, or automatic updater checks; launch in the original user's session;
version-scoped Windows backups; guarded macOS restore; Linux graphical elevation;
temporary-workspace cleanup. Simplified Chinese is the supported UI option.

The Windows patch includes upstream's executable integrity changes and Cowork
client-signature check modification. macOS uses ad-hoc code signing. These changes
require explicit in-app acknowledgment before installing or reapplying a patch.
No account, API key, provider configuration, or model routing is changed.
