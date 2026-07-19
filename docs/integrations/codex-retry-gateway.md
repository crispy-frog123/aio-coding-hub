# codex-retry-gateway integration notes

This document records how AIO currently tracks and reimplements
[`nonononull/codex-retry-gateway`](https://github.com/nonononull/codex-retry-gateway).
It is intentionally a mapping and update checklist, not a user guide.

## Upstream Tracking

- Upstream repository: `https://github.com/nonononull/codex-retry-gateway`
- Upstream branch: `main`
- Last reviewed upstream commit: `ef7fc5a0f9da125b91431cd99bcf6fd9387a53b2`
- Last reviewed upstream subject: `Merge pull request #27 from nonononull/codex/passthrough-retry-timeout-policies`
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
| Capacity and HTTP 429 policies | Classifies exact Capacity errors before generic 429, applies configurable pass-through/502/retry actions, honors `Retry-After`, and uses full-jitter fallback. |
| `latency_guard` | Enforces per-attempt first meaningful progress timeout and a cross-attempt total deadline. |
| request-scoped retry budget | Shares `guard_retry_attempts` across reasoning retries, continuation recovery, Capacity, 429, and first-progress timeout retries. |
| attempt telemetry | Records upstream dispatch, first progress, client forwarding, policy decision, retry delay/budget, and timeout final action. |
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
| `src-tauri/src/gateway/proxy/handler/failover_loop/layered_policy.rs` | Implements request-scoped shared retry budget, exact Capacity/generic 429 policy actions, `Retry-After` and full-jitter delays, first-progress/total deadlines, strict inspection limits, 502 contracts, and per-attempt telemetry. |
| `src-tauri/src/gateway/proxy/handler/failover_loop/attempt/send.rs` | Applies the earliest absolute upstream first-byte, first-progress, or total deadline while waiting for response headers. |
| `src-tauri/src/gateway/proxy/handler/failover_loop/response/response_router.rs` | Records per-attempt upstream status and routes declared or request-indicated stream candidates into strict stream handling. |
| `src-tauri/src/gateway/proxy/handler/failover_loop/response/codex_reasoning_guard.rs` | Core degraded-reasoning detection, rule resolution, retry-budget decision, special-setting payloads, and attempt logging helpers. |
| `src-tauri/src/gateway/proxy/handler/failover_loop/response/success_non_stream.rs` | Applies Codex reasoning guard to successful non-stream responses after response body buffering and optional response fixing. |
| `src-tauri/src/gateway/proxy/handler/failover_loop/response/success_event_stream.rs` | Buffers Codex Responses SSE for guard inspection, aggregates the stream, safely removes encrypted replay items only after an actual continuation attempt, and applies retry/continuation handling. |
| `src-tauri/src/gateway/proxy/protocol_bridge/stream.rs` | Aggregates OpenAI Responses event-stream payloads into JSON used by the stream guard path. |
| `src-tauri/src/domain/codex_reasoning_analytics.rs` | Stores/imports/exports reasoning samples and layered policy telemetry with schema-compatible defaults. |
| `src-tauri/src/gateway/proxy/handler/failover_loop/attempt/attempt_executor.rs` | Starts policy timing at the real upstream dispatch and holds per-provider continuation/retry state. |
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
| `src/pages/ReasoningGuardPage.tsx` | Dedicated rule/analytics page for guard rules, layered Capacity/429 actions, latency guard controls, and policy telemetry. |

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

Reasoning observation accepts `none`, `minimal`, `low`, `medium`, `high`, `xhigh`,
`max`, and `ultra`. Reasoning Analytics keeps `gpt-5.6-sol`, `gpt-5.6-terra`, and
`gpt-5.6-luna` as separate model families, including model names with a hyphenated
suffix such as `gpt-5.6-sol-ultra`. Prefix lookalikes such as `gpt-5.6-solar` are
not folded into those families, and unrelated AIO model names retain the existing
arbitrary-model grouping behavior.

In `final_answer_only_high_xhigh` mode, AIO ignores token compare/model rules for matching but preserves their saved values. A request only matches when it explicitly asks for `reasoning.effort` `high` or `xhigh`, and the response has visible final answer text without commentary, tool/function calls, or reasoning items. This mode still uses AIO's existing immediate/delayed retry budget and exhausted action.

`max` and `ultra` are observation and analytics values only. They do not expand
the deliberately named `final_answer_only_high_xhigh` experimental rule, which
continues to match only explicit `high` and `xhigh` requests.

Codex context-compaction request detection follows upstream `170bdd2`: AIO checks Codex headers (`x-codex-request-kind`, `x-codex-purpose`, `x-codex-turn-metadata`) and body fields (`metadata`, `codex_request_kind`, `request_kind`, `purpose`) when they contain `remote_compaction_v2`, `remote_compaction`, or `context_compaction`. AIO intentionally does not treat `x-codex-beta-features` or `openai-beta` alone as context-compaction evidence, because those can appear on ordinary turns.

All detected context-compaction requests are observe-only and intercept-exempt, regardless of whether `reasoning_tokens` is missing, zero, or a formula-match value such as `516`, `1034`, or `1552`. They do not consume the reasoning retry budget and do not trigger continuation recovery or provider switching.

In `final_answer_only_high_xhigh` mode, `reasoning_tokens = 0` is treated as a normal successful response and is not intercepted. Missing/null reasoning tokens and positive reasoning token values can still match when the response is final-answer-only and the request explicitly asked for `high` or `xhigh`.

### Layered Policies And Shared Budget

AIO ports the upstream layered policies as native Rust failover behavior for managed Codex Responses paths:

- Exact Capacity errors and generic HTTP 429 are separate policies. Capacity classification wins when both apply.
- Each policy supports `pass_through`, `return_502`, `retry_then_pass_through`, or `retry_then_502`.
- Capacity defaults to `retry_then_pass_through`; generic 429 defaults to `pass_through`.
- HTTP 429 retries honor seconds or HTTP-date `Retry-After` values up to 60 seconds. Missing/invalid values use capped full-jitter backoff.
- Ordinary non-Capacity 5xx responses are not generalized into this policy.
- Reasoning retries, continuation recovery, Capacity, generic 429, and first-progress timeout retries consume the same AIO immediate/delayed retry budget. Provider switching does not create a fresh policy budget.
- The circuit-breaker failure threshold is health accounting only; it does not inflate ordinary per-provider retries. `failover_max_attempts_per_provider` remains the request retry limit, apart from reserved one-shot OAuth/protocol repair attempts.
- A total deadline, when enabled, starts on the first real upstream dispatch and remains absolute across internal attempts and retry waits.

Latency guard is disabled by default. `first_progress_timeout_ms` applies per real upstream attempt; `total_timeout_ms` applies to the whole client request. A first-progress timeout can return 502 immediately or retry within the shared budget before returning 502. A total timeout is terminal and never retries.

Each real dispatch creates a `codex_gateway_policy_attempt` special-setting record. It stores provider/attempt identity, upstream start/status, first meaningful progress, client header/first-write timing, policy trigger/action, `Retry-After`, retry delay and shared budget, timeout phase/control loss, and final action. Reasoning analytics schema 3 imports and exports the corresponding aggregate policy fields while old samples remain readable through serde defaults.

### Non-Stream Responses

AIO buffers successful non-stream responses when reasoning inspection or latency guard requires it, parses JSON, and calls `codex_reasoning_guard::detect_from_json`. Absolute first-progress/total deadlines are rechecked while reading, after JSON/plugin processing, before guard decisions, and before client forwarding. On a rule match, it records the attempt and then retries, returns a guard error, or switches provider according to the shared budget and exhausted action.

### Stream Responses

AIO buffers Codex Responses event streams when:

- `codex_reasoning_guard_enabled` is true,
- `cli_key == "codex"`,
- forwarded path is `/responses` or `/v1/responses`.

The stream is aggregated through `protocol_bridge::stream::aggregate_responses_event_stream`, then inspected through the same guard helper used by non-stream responses.

The strict stream path recognizes LF, CRLF, CR, mixed boundaries, split UTF-8 BOM, and EOF trailing-CR events. Lifecycle/heartbeat/metadata/encrypted-reasoning events do not count as first meaningful progress; visible text/commentary/final answer/tool calls do. Two independent 1 MiB limits prevent unbounded buffering: the reasoning guard fails closed when one SSE event cannot be inspected safely, while the latency guard flushes a cumulative pre-progress prefix that would exceed the limit and records that a later timeout can only disconnect. With reasoning interception disabled, an oversized event records inspection/control loss but does not by itself turn observe-only passthrough into a 502. Once headers or content are irrevocably forwarded, deadline expiry cancels upstream and disconnects downstream with an explicit telemetry final action rather than attempting to rewrite status or splice a retry response.

For `stream_action = continuation_recovery`, AIO follows upstream `827c918`: continuation recovery is only used for `reasoning_tokens` rule-mode hits on Codex Responses streams. AIO no longer auto-adds `reasoning.encrypted_content`, no longer replays hit-round encrypted reasoning items, and no longer carries `previous_response_id` into continuation requests. A continuation retry is built from the original stream request input plus a top-level `phase = commentary` marker. Replay sanitization removes complete `reasoning`, `reasoning_item`, and nested `encrypted_content` items, including missing/null shells, instead of deleting only their required field. Responses that never trigger continuation remain byte-for-byte untouched by this safety filter. After an actual continuation attempt, client-visible SSE removes complete encrypted replay items (and safely redacts malformed payloads), preventing both encrypted state exposure and invalid `content[n].encrypted_content` history. Continuation attempts and successes are written to request-log special settings and reasoning analytics.

For `stream_action = strict_502` and `disconnect`, AIO keeps its own Rust gateway/failover semantics rather than executing upstream `gateway.mjs`; these modes remain compatibility choices exposed through settings and UI.

### CLI Proxy

AIO owns its own CLI proxy system. Upstream install/restore scripts are not used.

Current Codex proxy integration responsibilities:

- Backup the user's Codex files under AIO's CLI proxy data directory.
- Rewrite Codex `config.toml` to point at AIO gateway when proxy is enabled.
- Restore from backup when proxy is disabled.
- Treat a remote provider named `aio` as valid direct configuration when its `base_url` is not a local AIO URL.
- Detect stale local proxy config on startup and restore it when the manifest says the proxy is disabled.
- Skip config synchronization when the enabled manifest already targets the current gateway and the live files are already applied. In this current-state path, AIO leaves the manifest, Codex `config.toml`, and `auth.json` contents unchanged.

Local AIO proxy detection is based on local host URLs such as `http://127.0.0.1:<port>/v1`, not on provider key name alone.

The latest upstream PID, child-process identity, health endpoint, and standalone
`config.json` recovery changes are not copied into AIO. They manage a separately
spawned Node gateway, while AIO owns an in-process Rust listener. AIO's equivalent
configuration responsibility remains manifest-based drift detection, backup,
restore, target rebind, and idempotent `sync_enabled` handling.

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
| Standalone Node gateway process and PID/health recovery | Not used. AIO has an in-process Rust gateway and does not spawn or identify an upstream Node child process. |
| `gateway.mjs` management UI | Not used. AIO has its own desktop UI. |
| `config.json` endpoint list | Not directly used. AIO routes are Rust routes and settings. |
| `intercept_streaming` / `intercept_non_streaming` flags | Not exposed with the same names. AIO has one Codex reasoning guard enable switch. |
| `guard_retry_attempts` | Replaced by AIO immediate/delayed retry budgets. |
| legacy `retry_upstream_capacity_errors` boolean | Not exposed. AIO uses the newer four-state `codex_gateway_capacity_error_action` setting as the source of truth. |
| `stream_action` internals | Exposed in AIO settings/UI and now follows upstream's semantic continuation recovery rules, but remains implemented through AIO's Rust retry/failover loop rather than upstream's Node stream plumbing. |
| model consistency monitor | Not fully ported. AIO request logs and provider availability are separate systems. |
| `active_probe` scheduled probes | Not fully ported. AIO has provider availability/service status features, but not upstream's exact active-probe suite or its GPT-5.6 target-specific effort bounds. |
| standalone reasoning analytics dashboard/background jobs | Partially ported. AIO has its own Reasoning Guard analytics page plus SQLite import/export APIs, but does not reuse the Node management UI or its standalone background-job process. |
| upstream install/restore scripts | Not used. AIO uses `src-tauri/src/infra/cli_proxy`. |

## Future Update Checklist

Use this process whenever upstream `codex-retry-gateway` changes.

1. Fetch upstream and identify new commits.

   ```powershell
   $up = "$env:TEMP\codex-retry-gateway-upstream"
   git -C $up fetch origin main
   git -C $up log --oneline ef7fc5a0f9da125b91431cd99bcf6fd9387a53b2..origin/main
   git -C $up diff --name-status ef7fc5a0f9da125b91431cd99bcf6fd9387a53b2..origin/main
   ```

2. Review the upstream diff by concern.

   ```powershell
   git -C $up diff ef7fc5a0f9da125b91431cd99bcf6fd9387a53b2..origin/main -- gateway.mjs config.example.json README.md err.md
   ```

3. Classify upstream changes before editing AIO.

| Upstream changed area | AIO port target |
| --- | --- |
| `REASONING_POINTERS`, `extractReasoningTokens`, rule matching | `codex_reasoning_guard.rs`, Rust tests, frontend defaults if needed |
| `reasoning_match_mode`, formula sequence, manual token matching | `codex_reasoning_guard.rs`, settings migration/defaults, `CodexTab.tsx`, `ReasoningGuardPage.tsx`, tests |
| `intercept_rule_mode`, final-only matching, response structure classification | `codex_reasoning_guard.rs`, `CodexTab.tsx`, `requestLogSpecialSettings.ts`, Rust/frontend tests |
| `stream_action`, continuation marker, encrypted reasoning include/retry | `attempt_executor.rs`, `success_event_stream.rs`, `codex_reasoning_guard.rs`, request-log/analytics formatting, UI settings |
| context compaction markers or detection | `model_inference.rs`, `request_context.rs`, `codex_reasoning_guard.rs`, Rust tests |
| Capacity/429 actions, `Retry-After`, jitter, or policy priority | `layered_policy.rs`, `upstream_error.rs`, settings/UI, request-log parser, tests |
| first-progress or total deadline semantics | `attempt/send.rs`, `success_non_stream.rs`, `success_event_stream.rs`, `layered_policy.rs`, tests |
| shared retry budget semantics | `layered_policy.rs`, `codex_reasoning_guard.rs`, continuation paths, settings UI |
| attempt telemetry or analytics export fields | `layered_policy.rs`, `codex_reasoning_analytics.rs`, generated bindings, request-log parser/UI |
| `reasoning_equals` default | `src-tauri/src/infra/settings/types.rs`, `src/services/settings/settingsValidation.ts`, tests |
| model-family or reasoning-effort observation | `model_inference.rs`, `codex_reasoning_analytics.rs`, `requestLogSpecialSettings.ts`, Codex/CX2CC controls, tests |
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
