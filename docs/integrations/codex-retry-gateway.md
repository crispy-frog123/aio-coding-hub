# codex-retry-gateway integration notes

This document records how AIO currently tracks and reimplements
[`nonononull/codex-retry-gateway`](https://github.com/nonononull/codex-retry-gateway).
It is intentionally a mapping and update checklist, not a user guide.

## Upstream Tracking

- Upstream repository: `https://github.com/nonononull/codex-retry-gateway`
- Upstream branch: `main`
- Last reviewed upstream commit: `827c9188974d8191a8413792bcc450c994efc2c5`
- Last reviewed upstream subject: `Merge pull request #23 from nonononull/codex/semantic-continuation-recovery`
- AIO integration style: manual Rust/React reimplementation inside AIO, not vendoring or executing upstream `gateway.mjs`.

When updating this integration, first compare upstream changes from the commit above. Do not replace AIO gateway code with `gateway.mjs`.

## Upstream Source Areas

The upstream project is centered on `gateway.mjs`.

| Upstream area | Upstream responsibility |
| --- | --- |
| `DEFAULT_CONFIG` | Defaults for listen address, endpoints, `reasoning_equals`, interception flags, retry count, stream action, and active probe. |
| `extractReasoningTokens` | Reads `reasoning_tokens` from known JSON pointer locations. |
| `intercept_rule_mode` | Selects between `reasoning_tokens` matching and final-answer-only high/xhigh matching. |
| `reasoning_match_mode` | Selects manual token rules or the default `518*n - 2` formula. |
| `stream_action` / continuation recovery | Controls stream guard handling, including strict error, disconnect-compatible behavior, and continuation recovery. |
| final-answer-only helpers | Classifies Responses JSON/SSE output as final text only, commentary, tool call, or reasoning item. |
| context compaction detection | Detects Codex remote/context compaction requests and exempts them from interception. |
| upstream capacity retry | Detects the specific upstream capacity message and retries internally with guard retry budget. |
| `handleNonStreaming` | Buffers non-stream JSON responses, checks reasoning tokens, retries or returns a guard error. |
| `handleStreaming` | Buffers or relays SSE streams depending on `stream_action`; in strict mode, detects guard hits before returning a final response. |
| `proxyRequest` | Routes supported Codex/OpenAI paths to the upstream provider and applies guard handling. |
| model insight helpers | Tracks local/upstream model consistency and suspicious model samples. |
| active probe helpers | Runs scheduled/manual probes for long context, image input, response structure, identity consistency, and knowledge cutoff. |
| reasoning analytics helpers | Tracks richer reasoning/interception observations, dashboard API data, exports, and imports. |
| install/restore scripts | Rewrites and restores Codex local config for the standalone Node gateway. |

## AIO Implementation Map

AIO has its own gateway runtime, provider routing, failover, logging, settings, and UI. The upstream behavior is split across these files:

| AIO file | Current responsibility |
| --- | --- |
| `src-tauri/src/gateway/routes.rs` | Axum routes for AIO gateway. `/v1` and `/v1/*path` are treated as Codex routes; `/:cli_key/*path` handles explicit CLI routes. |
| `src-tauri/src/gateway/control_service.rs` | Starts and stops the AIO gateway listener. |
| `src-tauri/src/gateway/proxy/handler/runtime_settings.rs` | Reads runtime settings for the proxy handler, including Codex reasoning guard settings. |
| `src-tauri/src/gateway/proxy/handler/middleware/model_inference.rs` | Extracts requested model, explicit Codex reasoning effort, and Codex context-compaction request kind. |
| `src-tauri/src/gateway/proxy/handler/failover_loop/response/codex_reasoning_guard.rs` | Core degraded-reasoning detection, rule resolution, retry-budget decision, special-setting payloads, and attempt logging helpers. |
| `src-tauri/src/gateway/proxy/handler/failover_loop/response/success_non_stream.rs` | Applies Codex reasoning guard to successful non-stream responses after response body buffering and optional response fixing. |
| `src-tauri/src/gateway/proxy/handler/failover_loop/response/success_event_stream.rs` | Buffers Codex Responses SSE for guard inspection, aggregates the stream, strips auto-requested encrypted reasoning from client output, and applies retry/continuation handling. |
| `src-tauri/src/gateway/proxy/protocol_bridge/stream.rs` | Aggregates OpenAI Responses event-stream payloads into JSON used by the stream guard path. |
| `src-tauri/src/gateway/proxy/handler/failover_loop/attempt/attempt_executor.rs` | Holds per-provider retry-loop state, including Codex reasoning guard hit count and continuation recovery state. |
| `src-tauri/src/gateway/proxy/handler/failover_loop/mod.rs` | Wires request context into the failover loop. |
| `src-tauri/src/gateway/proxy/request_context.rs` | Carries Codex guard settings into request handling. |
| `src-tauri/src/gateway/proxy/request_end.rs` | Persists request-end details and normalizes guard attempt display data. |
| `src-tauri/src/gateway/proxy/mod.rs` | Request observation rules, request-start seeding, and local probe helpers. Current working tree also seeds in-progress logs for Codex `/responses` and `/v1/responses`. |
| `src-tauri/src/infra/cli_proxy/codex.rs` | Codex `config.toml` and `auth.json` rewrite/restore logic for AIO CLI proxy. Current working tree treats remote provider key `aio` differently from local AIO proxy URLs. |
| `src-tauri/src/infra/cli_proxy/mod.rs` | CLI proxy manifest, backup, enable/disable, restore, startup synchronization, and stale proxy recovery. |
| `src-tauri/src/infra/settings/types.rs` | Rust setting types and defaults for Codex reasoning guard. |
| `src-tauri/src/app/settings_service.rs` | IPC-facing settings read/update model for Codex reasoning guard fields. |
| `src-tauri/src/commands/request_logs.rs` | IPC command for Codex reasoning guard statistics. |
| `src/services/settings/settingsValidation.ts` | Frontend defaults and validation for Codex reasoning guard settings. |
| `src/services/settings/settings.ts` | Frontend settings serialization keys. |
| `src/services/gateway/requestLogs.ts` | Frontend service for request log and guard statistics IPC calls. |
| `src/services/gateway/requestLogSpecialSettings.ts` | Frontend parser/formatter for guard special settings in request logs. |
| `src/components/cli-manager/tabs/CodexTab.tsx` | Codex settings UI, guard rule editor, retry budget controls, and guard statistics UI. |

## Current Behavior

### Reasoning Guard

AIO currently detects `reasoning_tokens` at these JSON pointer locations, matching upstream:

- `/usage/output_tokens_details/reasoning_tokens`
- `/usage/completion_tokens_details/reasoning_tokens`
- `/response/usage/output_tokens_details/reasoning_tokens`
- `/response/usage/completion_tokens_details/reasoning_tokens`

Default frontend rule values are `516`, `1034`, and `1552`.

AIO adds behavior beyond the original first integration:

- Rule mode setting: `reasoning_tokens` or `final_answer_only_high_xhigh`.
- Reasoning match mode setting: `formula_518n_minus_2` or `manual`.
- Stream action setting: `continuation_recovery`, `strict_502`, or `disconnect`.
- Continuation marker text setting, default `Continue thinking...`.
- `equals` and `less_than_or_equal` compare modes.
- Per-requested-model rules.
- Immediate retry budget.
- Delayed retry budget and delay milliseconds.
- Exhausted action: return guard error or switch provider.
- Attempt/request log special settings for UI statistics.
- Provider failover integration instead of only retrying the same upstream URL.

In `reasoning_tokens` mode, AIO now defaults to upstream's recommended formula match:
`reasoning_tokens >= 516 && (reasoning_tokens + 2) % 518 == 0`. This catches
`516`, `1034`, `1552`, `2070`, and later values on the same sequence. The saved
global/per-model token rules are still preserved and can be used by switching
`reasoning_match_mode` back to `manual`.

In `final_answer_only_high_xhigh` mode, AIO ignores token compare/model rules for matching but preserves their saved values. A request only matches when it explicitly asks for `reasoning.effort` `high` or `xhigh`, and the response has visible final answer text without commentary, tool/function calls, or reasoning items. This mode still uses AIO's existing immediate/delayed retry budget and exhausted action.

Codex context-compaction request detection follows upstream `170bdd2`: AIO checks Codex headers (`x-codex-request-kind`, `x-codex-purpose`, `x-codex-turn-metadata`) and body fields (`metadata`, `codex_request_kind`, `request_kind`, `purpose`) when they contain `remote_compaction_v2`, `remote_compaction`, or `context_compaction`. AIO intentionally does not treat `x-codex-beta-features` or `openai-beta` alone as context-compaction evidence, because those can appear on ordinary turns.

Context compaction is only intercept-exempt when the observed response has `reasoning_tokens = 0`. Context compaction responses with nonzero reasoning values such as `516`, `1034`, or `1552` still use the active guard rule mode and can be retried.

In `final_answer_only_high_xhigh` mode, `reasoning_tokens = 0` is treated as a normal successful response and is not intercepted. Missing/null reasoning tokens and positive reasoning token values can still match when the response is final-answer-only and the request explicitly asked for `high` or `xhigh`.

For non-success upstream responses, AIO also ports upstream's specific capacity retry behavior: if a Codex upstream response body contains `Selected model is at capacity. Please try a different model.` or the equivalent lower-case phrase pair (`selected model is at capacity` and `try a different model`), AIO records a `codex_upstream_capacity_retry` attempt and retries the same provider using the existing Codex guard retry budget. This does not generalize ordinary 429 or 5xx responses.

### Non-Stream Responses

AIO buffers successful non-stream responses in `success_non_stream.rs`, parses JSON, and calls `codex_reasoning_guard::detect_from_json`. On a match, it records an attempt and then either retries the same provider, returns a `GW_CODEX_REASONING_GUARD` error, or switches provider according to budget settings.

### Stream Responses

AIO buffers Codex Responses event streams when:

- `codex_reasoning_guard_enabled` is true,
- `cli_key == "codex"`,
- forwarded path is `/responses` or `/v1/responses`.

The stream is aggregated through `protocol_bridge::stream::aggregate_responses_event_stream`, then inspected through the same guard helper used by non-stream responses.

For `stream_action = continuation_recovery`, AIO follows upstream `827c918`: continuation recovery is only used for `reasoning_tokens` rule-mode hits on Codex Responses streams. AIO no longer auto-adds `reasoning.encrypted_content`, no longer replays hit-round encrypted reasoning items, and no longer carries `previous_response_id` into continuation requests. A continuation retry is built from the original stream request input plus the configured commentary marker; original input reasoning items and `encrypted_content` fields are filtered before replay. When continuation recovery is active, client-visible SSE output is sanitized so `encrypted_content` is not exposed, including malformed SSE fallback paths. Continuation attempts and successes are written to request-log special settings and reasoning analytics.

For `stream_action = strict_502` and `disconnect`, AIO keeps its own Rust gateway/failover semantics rather than executing upstream `gateway.mjs`; these modes remain compatibility choices exposed through settings and UI.

### CLI Proxy

AIO owns its own CLI proxy system. Upstream install/restore scripts are not used.

Current Codex proxy integration responsibilities:

- Backup the user's Codex files under AIO's CLI proxy data directory.
- Rewrite Codex `config.toml` to point at AIO gateway when proxy is enabled.
- Restore from backup when proxy is disabled.
- Treat a remote provider named `aio` as valid direct configuration when its `base_url` is not a local AIO URL.
- Detect stale local proxy config on startup and restore it when the manifest says the proxy is disabled.

Local AIO proxy detection is based on local host URLs such as `http://127.0.0.1:<port>/v1`, not on provider key name alone.

### Request Logs

In the current working tree, AIO seeds in-progress request logs for:

- Claude `/v1/messages`
- Codex `/responses`
- Codex `/v1/responses`

This is an AIO behavior difference from earlier code where Codex relied mainly on realtime request-start events until completion.

## Not Currently Equivalent To Upstream

These upstream areas are not fully ported as one-to-one features:

| Upstream feature | AIO status |
| --- | --- |
| Standalone Node gateway process | Not used. AIO has a Rust gateway. |
| `gateway.mjs` management UI | Not used. AIO has its own desktop UI. |
| `config.json` endpoint list | Not directly used. AIO routes are Rust routes and settings. |
| `intercept_streaming` / `intercept_non_streaming` flags | Not exposed with the same names. AIO has one Codex reasoning guard enable switch. |
| `guard_retry_attempts` | Replaced by AIO immediate/delayed retry budgets. |
| `retry_upstream_capacity_errors` UI toggle | Not exposed as a separate AIO setting. AIO ports the default upstream behavior for Codex capacity errors and uses the existing guard retry budget. |
| `stream_action` internals | Exposed in AIO settings/UI and now follows upstream's semantic continuation recovery rules, but remains implemented through AIO's Rust retry/failover loop rather than upstream's Node stream plumbing. |
| model consistency monitor | Not fully ported. AIO request logs and provider availability are separate systems. |
| `active_probe` scheduled probes | Not fully ported. AIO has provider availability/service status features, but not upstream's exact active-probe suite. |
| standalone reasoning analytics dashboard/export/import/background jobs | Partially ported. AIO has an internal Reasoning Guard page and analytics storage/API shape for desktop use, but does not reuse upstream's standalone Node management UI. |
| upstream install/restore scripts | Not used. AIO uses `src-tauri/src/infra/cli_proxy`. |

## Future Update Checklist

Use this process whenever upstream `codex-retry-gateway` changes.

1. Fetch upstream and identify new commits.

   ```powershell
   $up = "$env:TEMP\codex-retry-gateway-upstream"
   git -C $up fetch origin main
   git -C $up log --oneline 827c9188974d8191a8413792bcc450c994efc2c5..origin/main
   git -C $up diff --name-status 827c9188974d8191a8413792bcc450c994efc2c5..origin/main
   ```

2. Review the upstream diff by concern.

   ```powershell
   git -C $up diff 827c9188974d8191a8413792bcc450c994efc2c5..origin/main -- gateway.mjs config.example.json README.md err.md
   ```

3. Classify upstream changes before editing AIO.

| Upstream changed area | AIO port target |
| --- | --- |
| `REASONING_POINTERS`, `extractReasoningTokens`, rule matching | `codex_reasoning_guard.rs`, Rust tests, frontend defaults if needed |
| `reasoning_match_mode`, formula sequence, manual token matching | `codex_reasoning_guard.rs`, settings migration/defaults, `CodexTab.tsx`, `ReasoningGuardPage.tsx`, tests |
| `intercept_rule_mode`, final-only matching, response structure classification | `codex_reasoning_guard.rs`, `CodexTab.tsx`, `requestLogSpecialSettings.ts`, Rust/frontend tests |
| `stream_action`, continuation marker, encrypted reasoning include/retry | `attempt_executor.rs`, `success_event_stream.rs`, `codex_reasoning_guard.rs`, request-log/analytics formatting, UI settings |
| context compaction markers or detection | `model_inference.rs`, `request_context.rs`, `codex_reasoning_guard.rs`, Rust tests |
| upstream capacity retry | `upstream_error.rs`, guard retry budget state, request-log special settings |
| `reasoning_equals` default | `src-tauri/src/infra/settings/types.rs`, `src/services/settings/settingsValidation.ts`, tests |
| non-stream guard behavior | `success_non_stream.rs`, route/failover tests |
| stream guard behavior or SSE parsing | `success_event_stream.rs`, `protocol_bridge/stream.rs`, stream tests |
| retry count/budget semantics | `codex_reasoning_guard.rs`, `runtime_settings.rs`, `CodexTab.tsx` |
| blocked response shape/header/error code | `codex_reasoning_guard.rs`, `constants/gatewayErrorCodes.ts`, request log formatting |
| supported paths/endpoints | `routes.rs`, `proxy/mod.rs`, stream/non-stream guard gating |
| install/restore behavior | `infra/cli_proxy/codex.rs`, `infra/cli_proxy/mod.rs`, CLI proxy tests |
| active probe | Decide product scope first; likely new domain/commands/UI work, not a small guard patch |
| reasoning analytics dashboard/API/export/import/background jobs | Decide product scope first; not part of the core guard behavior port |
| model consistency insights | Decide product scope first; likely request-log or provider-observability work |
| upstream UI-only changes | Usually no AIO port unless they reveal new behavior |

4. Add or update focused tests.

Prefer small tests near the target module:

- `codex_reasoning_guard.rs` unit tests for JSON pointers, rule modes, response structure, context compaction exemption, and match semantics.
- `routes.rs` gateway tests for non-stream and stream end-to-end behavior.
- `gateway/proxy/tests.rs` for observation and in-progress log seeding.
- `infra/cli_proxy/tests.rs` for config backup/restore behavior.
- frontend tests near `settingsValidation`, `requestLogSpecialSettings`, and `CodexTab` when UI/settings behavior changes.

5. Run the smallest useful checks first.

   ```powershell
   cd D:\retry-gateway\aio-coding-hub-fingercaster
   pnpm typecheck
   pnpm lint
   cd src-tauri
   cargo fmt -- --check
   cargo check --lib --quiet
   ```

6. If the upstream commit is fully reviewed and any needed AIO changes are merged, update the "Last reviewed upstream commit" at the top of this document.

## Notes For Maintainers

- Do not assume `model_provider = "aio"` means the Codex config points to local AIO. The user's direct remote provider may intentionally use the provider key `aio`.
- Local AIO proxy state should be detected from local loopback `base_url`, for example `http://127.0.0.1:<port>/v1`.
- Avoid merging upstream install scripts into AIO directly. The backup/restore semantics belong to AIO's `cli_proxy` modules.
- Treat active probe and model consistency as separate product features. They are larger than the Codex reasoning guard integration.
