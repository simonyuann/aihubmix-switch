# CC rebuild verification

Source: `simonyuann/my-cc` at `42ef63471f0ed879913f8de0d0c7e3e580293193` (CC 4.0.20), imported exactly in `718e675`. Product base: AIHubMix Switch 1.0.2 (`617592d`). Implementation branch: `feat/cc-rebuild`; product version: 2.0.0.

The CC interface, database schema, thirteen application integrations, provider services, native configuration and proxy implementation are reused. The additional account page calls the isolated `src-tauri/src/aihubmix` module; provider drafts are saved through CC's existing API. The product uses `.aihubmix-switch` and never implicitly opens the installed CC database.

## Local verification (2026-10-26)

Environment: Apple Silicon macOS, Node 24.22.1, pnpm 10.12.3, installed Rust stable 1.99.1. CI retains upstream's pinned Rust 1.95. Local `+stable` was used because the pinned toolchain was not available reliably; cross-platform CI has not been executed in this session.

| Check | Result |
| --- | --- |
| TypeScript `pnpm typecheck` | Passed after matching official Tauri JS/native minor versions |
| Frontend production build | Passed; inherited large-chunk/browser-data warnings |
| AIHubMix frontend integration | 10 passed, including StrictMode, account switching, explicit provider save, uncertain payment, QR history, currency and imported mappings |
| Signed update manifest tests | 4 passed, including six formats, signature/version binding and major-version feed selection |
| Download-station tests | 14 passed, including compatible legacy feed selection, 1.x retention and missing-compatible-release 404 |
| Rust format | Passed after formatting-only commit `d96e453`; final fix modules formatted |
| Full frontend | 263 files, 3,368 tests passed under Node 24; no failures |
| Full Rust | `cargo +stable test --locked`: 4,383 passed across 18 test targets; 11 upstream tests ignored, no failures. Includes all 43 golden tests after restoring an accidentally branded internal fixture ID. |
| Native check | `cargo +stable check --locked` passed; final source also compiled in the full test and desktop builds |
| Clippy | All targets passed with `-D warnings -A deprecated -A clippy::for_kv_map -A clippy::result_large_err`. These exclusions cover unchanged upstream code flagged by the newer local Rust 1.99 toolchain. Strict unmodified `-D warnings` and pinned Rust 1.95 CI were not certified locally. |
| Native macOS build | `pnpm tauri build --debug --bundles app --no-sign --ci -- --locked` passed. Local debug app is 175.27 MiB; unsigned updater archive is 41.78 MiB. |
| Bundle metadata/policy | Identifier `fun.csmy.aihubmix-switch`, version `2.0.0`, minimum macOS `12.0`; actual app reports update format `app` and install mode `automatic` |
| Desktop QA | Packaged app started; CC's thirteen-tool navigation, Gemini provider page and AIHubMix account page rendered. Account forms, local Key empty state and explicit legacy import controls inspected; login/register states observed. |

Foundation, platform and final integration reviews all passed after their scoped fixes. The final golden fixture correction also received a scoped review against the exact CC baseline. Reviews do not substitute for the execution results above.

Local artifact: `artifacts/local/AIHubMix Switch.app` (git-ignored), copied from the successful native bundle. It is a development build without signing/notarization. Windows, Intel macOS and Linux artifacts remain for CI. Actual server login, payment and update installation were not exercised; those operations use mocks in the automated tests. The expected `latest-v2.json` response is unavailable until the new server routing is deployed.

Desktop test data uses `/private/tmp/aihubmix-cc-qa.Itr7Rk`, verified through native startup logs and the created database path. The first GUI launch did not inherit the isolation environment: it created a new `~/.aihubmix-switch` directory and imported the existing native tool settings. That process was exited and the newly created directory was preserved as `first-launch-snapshot` inside the temporary test directory; the home directory was restored to its prior absence. The app was then launched with an explicit `CC_SWITCH_TEST_HOME`. No provider was activated, no legacy import submitted, and no real credentials entered during UI QA.

## Compatibility and release boundary

- The app identifier and updater public key are retained. Six package formats and immutable package URLs retain their existing contracts.
- v2 uses `latest-v2.json` at the existing download service. Legacy `latest.json` serves the highest verified 1.x snapshot or 404 and retains its packages beyond normal cleanup. This prevents macOS 11 clients from replacing their working application with v2, which requires macOS 12+.
- The first upgrade from 1.x to 2.x is manual on every platform. Deploy the new server routes **before** publishing v2. No server deployment or publication was performed here.
- Standalone portable executables use manual updates. Both the UI and native install command enforce this; packaging CI checks the actual marker-free binary.
- Legacy account/settings/vault import is explicit and preserves originals. Native tool files are written through CC only after a user saves and activates a provider.
- Automated verification uses temporary data and mock payment services. No real account login, recharge, payment order or release signing is part of local QA.

## Decisions made during integration

1. Separate major-version feeds while preserving the service and signing key, because old macOS 11 clients cannot enforce the new OS requirement. Cost: manual initial major upgrade and server-first routing deployment.
2. Match the official JS Tauri 2.12 and updater 2.14 SDKs to the resolved Rust crates, because the CLI rejected mismatched minor versions. Cost: dependency lockfile changes and verification with those SDK versions.
