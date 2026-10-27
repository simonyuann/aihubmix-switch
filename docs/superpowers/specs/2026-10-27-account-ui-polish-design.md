# Account and wallet UI polish

User authorized sequential implementation of items 1–4 followed by pushing and publishing a release. The follow-up requires no GitHub link in About, only the official website.

1. Remove the About Star banner, dismissal state, and GitHub button. Preserve the official website at `https://csmy.fun:23457/`, version and updater controls.
2. Move logout from API management into Settings → General. Reuse AccountContext.logout so successful logout clears authenticated state and returns to login. Disable repeated submissions and surface failure locally.
3. Remove the misleading Key quota line without changing backend key data or key operations.
4. Add the existing balance display to the common page header. A workspace context enables it on non-settings pages; remove the previous tool-only duplicates. Refresh the main account wallet once per minute as well as after payment. The independent tray window queries the same read-only payment-balance command every time it opens, using the same formatter in a compact header between title and settings. Loading/failure displays a dash, not zero or an outdated amount.
5. Publish v1.1.2 with signed macOS ARM/Intel and Windows installer/portable artifacts through the existing CI. Preserve the configured managed-application update feed and existing uncommitted download-entry/recharge changes after reviewing and validating them. The download station performs its own scheduled synchronization; no station deployment is required.

Validation covers About controls, logout failure and duplicate clicks, API key operations, wallet visibility in API management and absence in settings, tray refresh/failure behavior, TypeScript, frontend production build, release manifest signatures, and native checks. CI must complete and release assets must be verified before making the draft public.
