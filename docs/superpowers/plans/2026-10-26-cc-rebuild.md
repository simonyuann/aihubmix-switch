# AIHubMix Switch CC Rebuild Implementation Plan

> **For agentic workers:** Use superpowers:subagent-driven-development for isolated tasks and review the integrated result. User has approved execution; no additional design approval is needed.

**Goal:** Replace the legacy application with my-cc's complete frontend and backend, retaining AIHubMix account, keys, wallet, payment and upgrade compatibility.

**Architecture:** Import my-cc at `42ef63471f0ed879913f8de0d0c7e3e580293193`. Preserve its core services, APIs and interface. Add a separate AIHubMix platform module and one account page that builds providers through CC's existing services.

**Tech Stack:** Tauri 2, Rust, React 18, TypeScript, pnpm, TanStack Query, CC's existing UI components.

**Spec:** `docs/superpowers/specs/2026-10-26-cc-rebuild-design.md`

## Global Constraints

- Work only in `feat/cc-rebuild`, `/Users/simon/aihubmix-switch-cc-rebuild`.
- Preserve application identifier `fun.csmy.aihubmix-switch` and the existing updater public key and AIHubMix download endpoint.
- Use a separate `.aihubmix-switch` data directory; never fall back to the installed CC application's database.
- Keep CC internal crate names, schema and module boundaries. Keep upstream MIT attribution.
- Minimum macOS version is 12.0. Do not publish or create real payment orders during verification.

## Task 1: Import the CC foundation and product configuration

**Files:** `src/`, `src-tauri/`, CC root configuration and lockfiles, `THIRD_PARTY_NOTICES.md`, `README.md`.

**Interfaces:** Preserve CC's original `providersApi`, Tauri command names, `AppState`, `ProviderService` and all 13 `AppId` values.

- [ ] Replace legacy source, tests and build configuration with the exact CC baseline, retaining target repository history, release documentation and download-station deployment.
- [ ] Set package/product version to `2.0.0`, product name to `AIHubMix Switch`, identifier to `fun.csmy.aihubmix-switch`.
- [ ] Change product configuration paths and corresponding fixtures from `.cc-switch` to `.aihubmix-switch`; preserve database filename and schema.
- [ ] Replace user-facing product branding and update/download destinations; retain upstream attribution separately.
- [ ] Install pnpm dependencies, run typecheck and frontend production build, then commit the foundation.

## Task 2: Isolated platform backend

**Files:** `src-tauri/src/aihubmix/`, `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`.

**Interfaces:** `aihubmix_dispatch(command: String, payload: Option<Value>) -> Result<Value, String>`; platform state has `relayBaseUrl`, `account`, `localKeys`, `legacyData`, `storageError`. Support `state`, `login`, `register`, `logout`, `set-base`, `keys`, `groups`, `create-key`, `delete-key`, `get-key`, `save-key`, `remove-local-key`, `models`, `provider`, `legacy-import`, `open-legacy-backups`, and the existing `payment-*` operations. `provider` returns a CC `Provider` draft without writing native tool files.

- [ ] Extract encrypted vault and platform account operations from v1.0.2, retaining keyring identity and response contracts, without importing legacy agent/proxy implementations.
- [ ] Reuse the existing payment protocol and amount/order checks; keep platform operations independent from the CC proxy lock.
- [ ] Import legacy settings and vault only on explicit request; preserve originals, reject unreadable vaults and make imports idempotent.
- [ ] Build provider drafts compatible with CC's editor for selected app, key, model and effort; native writes remain CC's responsibility.
- [ ] Test account refresh, credential redaction, exact payment amounts, duplicate submission, foreign orders, provider drafts and legacy import using temporary files and a mock HTTP server.

## Task 3: CC-native account page

**Files:** `src/components/aihubmix/`, `src/lib/api/aihubmix.ts`, `src/lib/navigation.ts`, `src/components/shell/Sidebar.tsx`, `src/App.tsx`.

**Interfaces:** A typed `aihubmixApi` wrapper for Task 2 and a single global navigation view `aihubmix`. Use `providersApi.add` for reviewed provider drafts and query invalidation after changes.

- [ ] Add one AIHubMix account navigation entry, preserving CC's existing layout, theme, pages and tool authorization center.
- [ ] Implement platform login/register/logout, configurable service URL, wallet balance, Key/group CRUD and existing local Key access with CC UI controls.
- [ ] Add a provider draft dialog with app/model/effort choice and explicit save, using CC's provider API for persistence.
- [ ] Port recharge behavior into CC dialogs, including QR code, status, history, order verification/cancellation and uncertain submission handling.
- [ ] Add legacy import and backup-directory access, with distinct loading/empty/error states and stale response protection.
- [ ] Test account switching, request errors, explicit provider save and payment uncertainty with mocked APIs.

## Task 4: Build and integration verification

**Files:** `.github/workflows/`, `scripts/`, `src-tauri` tests, product documentation.

- [ ] Adapt CI to pnpm/CC build and tests, retain AIHubMix signed release artifact naming and mirror deployment contracts.
- [ ] Run TypeScript checks, production frontend build, relevant frontend integration tests, Rust format/check/test and a native desktop build.
- [ ] Review update URLs, product identity, independent data paths and absence of old parallel agent/proxy modules.
- [ ] Inspect the account page and CC UI with browser/desktop tools as available; record actual verification and any platform-specific limitations.
- [ ] Commit reviewed changes and document the final source baseline, development commands, migration behavior and validated artifacts.
