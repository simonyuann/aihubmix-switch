# Dedicated Client Implementation Plan

> Execute sequentially in the existing isolated worktree. User clarification authorizes this design; do not add design approval gates. No subagents required.

**Goal:** Turn the CC rebuild into a login-required client exclusively for the existing AIHubMix service.
**Architecture:** Product policy enforces native account/endpoint restrictions; a React session boundary mounts CC only after verified login. CC's native configuration services remain the single implementation for thirteen tools.
**Tech Stack:** Existing Rust/Tauri, React/TypeScript, CC UI controls and query cache.
**Spec:** docs/superpowers/specs/2026-10-26-dedicated-client-design.md

## Global Constraints

- Work in feat/cc-rebuild, /Users/simon/aihubmix-switch-cc-rebuild.
- Fixed product origin https://csmy.fun:23457; no third-party suppliers or configurable origin.
- Keep CC services, schema, identifier, data directory and signing/feed contracts.
- No real login, registration, emails, Key/payment creation, deployment or publication during verification.

## Tasks

- [ ] Native policy: runtime-only activation, verified session state, authenticated IPC boundary, fixed origin/no redirects, own provider generation, block generic edits/imports, no official seed/live import/startup proxy before login; tests.
- [ ] Session UI: shared account context, boot verification, login, email-code registration, forgot-password email, logout/session invalidation; tests.
- [ ] Navigation: remove requested four pages and persisted entry access; sidebar wallet, recharge, apply API; dedicated Key/account page and one fixed tool configuration per app; tests.
- [ ] Verify typecheck, scoped and full frontend tests, native fmt/check/tests, production/native build, desktop isolated login gate. Document limitations and commit clean result.
