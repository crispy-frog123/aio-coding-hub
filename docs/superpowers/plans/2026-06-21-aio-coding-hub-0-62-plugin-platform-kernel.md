# aio-coding-hub 0.62.0 Plugin Platform Kernel Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Rework the plugin platform internals around a contract-driven hook, runtime, and provider-adapter kernel while preserving Plugin API v1 external behavior.

**Architecture:** Keep `docs/plugins/plugin-api-v1-contract.json` as the canonical public contract, then derive or structurally check Rust, TypeScript, docs, scaffold, and runtime behavior against it. Introduce internal registries and facades in layers: contract metadata, hook descriptors, mutation/context enforcement, runtime manager, and provider adapter facade. No new public plugin API is added in 0.62.0.

**Tech Stack:** Rust/Tauri 2/Axum/SQLite/Specta, TypeScript/React/Vite/Vitest, Node.js contract scripts, Cargo tests, pnpm workspace tooling.

---

## Scope Note

The design covers Contract, Hook Registry, Runtime, and Provider Adapter layers. These are not independent product features; they are dependent platform layers. This plan keeps them in one master implementation plan so each task can validate external Plugin API v1 compatibility before the next layer builds on it.

## File Responsibility Map

- `docs/plugins/plugin-api-v1-contract.json`: canonical Plugin API v1 contract.
- `scripts/check-plugin-api-contract.mjs`: structural drift checker for contract, Rust, SDK, docs, scaffold, and WASM SDK.
- `scripts/check-plugin-api-contract.selftest.mjs`: checker regression tests using temporary fixture repositories.
- `src-tauri/src/gateway/plugins/contract.rs`: Rust view of hook, permission, runtime, timeout, and mutation metadata.
- `src-tauri/src/gateway/plugins/registry.rs`: internal `HookRegistry` and `HookDescriptor` lookup APIs.
- `src-tauri/src/gateway/plugins/mutation.rs`: hook result permission enforcement and mutation application helpers.
- `src-tauri/src/gateway/plugins/context.rs`: visible hook context types and context builders.
- `src-tauri/src/gateway/plugins/pipeline.rs`: hook ordering, timeout, circuit, audit, and executor orchestration.
- `src-tauri/src/app/plugins/runtime_manager.rs`: runtime dispatch facade used by gateway plugin pipeline.
- `src-tauri/src/app/plugins/runtime_policy.rs`: host runtime policy for WASM/process/native runtime availability.
- `src-tauri/src/app/plugins/runtime_cache.rs`: cache key and cache retention helpers shared by runtime implementations.
- `src-tauri/src/app/plugins/runtimes/declarative_rules.rs`: declarative rules runtime module after extraction from current rule runtime.
- `src-tauri/src/app/plugins/runtimes/official_privacy_filter.rs`: official native privacy filter module after extraction.
- `src-tauri/src/app/plugins/runtimes/wasm_policy.rs`: policy-gated WASM runtime entrypoint that returns stable disabled errors.
- `src-tauri/src/app/plugins/runtime_executor.rs`: temporary compatibility wrapper until call sites use `PluginRuntimeManager`.
- `src-tauri/src/gateway/proxy/provider_adapters/mod.rs`: provider adapter facade and registry.
- `src-tauri/src/gateway/proxy/provider_adapters/cx2cc.rs`: CX2CC adapter facade.
- `src-tauri/src/gateway/proxy/provider_adapters/gemini_oauth.rs`: Gemini OAuth adapter facade.
- `src-tauri/src/gateway/proxy/provider_adapters/codex_chatgpt.rs`: Codex ChatGPT adapter facade.
- `src-tauri/src/gateway/proxy/provider_adapters/claude.rs`: Claude model mapping and Claude-specific adapter facade.
- `src-tauri/src/gateway/proxy/handler/*`: gateway call sites that should call facades instead of adding new provider-specific branches.
- `packages/plugin-sdk/src/index.ts`: public TypeScript Plugin API v1 SDK, unchanged externally.
- `packages/create-aio-plugin/src/devtools.ts`: author tooling replay/pack behavior, unchanged externally.
- `docs/plugins/reference/*.md`: public docs, updated only to describe internal 0.62 compatibility guarantees where needed.

## Baseline Verification

### Task 0: Record Current State

**Files:**
- Read: `package.json`
- Read: `docs/plugins/plugin-api-v1-contract.json`
- Read: `docs/superpowers/specs/2026-06-21-aio-coding-hub-0-62-plugin-platform-kernel-design.md`
- No source changes

- [ ] **Step 1: Confirm a clean or understood worktree**

Run:

```bash
git status --short
```

Expected: either no output, or only files intentionally owned by this task. Do not revert unrelated user changes.

- [ ] **Step 2: Run plugin contract checks**

Run:

```bash
pnpm check:plugin-api-contract
pnpm check:plugin-system-docs
pnpm check:plugin-system-completion
```

Expected: all pass before refactor work begins. If a command fails, record the failing command and exact output in the task journal before changing code.

- [ ] **Step 3: Run SDK and author-tool tests**

Run:

```bash
pnpm plugin-sdk:typecheck
pnpm --filter @aio-coding-hub/plugin-sdk test
pnpm create-aio-plugin:test
pnpm plugin-wasm-sdk:test
```

Expected: all pass before refactor work begins.

- [ ] **Step 4: Run Rust plugin/gateway/provider baseline tests**

Run:

```bash
cd src-tauri && cargo test plugin --lib
cd src-tauri && cargo test provider --lib
cd src-tauri && cargo test gateway --lib
```

Expected: all pass, or any pre-existing failures are documented with exact test names.

- [ ] **Step 5: Commit baseline note only if files were changed**

If no files changed, do not commit. If a local journal file was intentionally updated, run:

```bash
git add <journal-file>
git commit -m "test: record plugin platform baseline"
```

Expected: either no commit is created, or exactly the journal file is committed.

## Contract Layer

### Task 1: Extend the Contract JSON Shape Without Changing Public API

**Files:**
- Modify: `docs/plugins/plugin-api-v1-contract.json`
- Modify: `scripts/check-plugin-api-contract.mjs`
- Modify: `scripts/check-plugin-api-contract.selftest.mjs`
- Test: `scripts/check-plugin-api-contract.selftest.mjs`

- [ ] **Step 1: Write a failing self-test for missing hook matrix fields**

Modify `scripts/check-plugin-api-contract.selftest.mjs` so its temporary `plugin-api-v1-contract.json` includes an active hook entry missing `kind`, `status`, or `mutationFields`. Add this assertion near the existing spawn check:

```javascript
if (result.status === 0 || !result.stderr.includes("hookMatrix.gateway.request.afterBodyRead.kind")) {
  throw new Error(`expected hookMatrix kind failure, got status ${result.status}\n${result.stderr}`);
}
```

- [ ] **Step 2: Run the self-test and verify it fails**

Run:

```bash
node scripts/check-plugin-api-contract.selftest.mjs
```

Expected: FAIL because `scripts/check-plugin-api-contract.mjs` does not yet validate structured hook matrix metadata.

- [ ] **Step 3: Add structured fields to the canonical contract**

Update every `hookMatrix` entry in `docs/plugins/plugin-api-v1-contract.json` to include these exact fields:

```json
{
  "kind": "request",
  "status": "active",
  "defaultFailurePolicy": "fail-open",
  "timeoutMs": 150,
  "reservedHeaderPolicy": "block-gateway-owned"
}
```

Use `kind` values `request`, `response`, `stream`, and `log`. Use `status: "active"` for active hooks. Keep the existing active/reserved hook arrays unchanged.

- [ ] **Step 4: Implement structured checker helpers**

In `scripts/check-plugin-api-contract.mjs`, add helpers:

```javascript
function requireObject(path, value) {
  if (value == null || typeof value !== "object" || Array.isArray(value)) {
    failures.push(`${path} must be an object`);
    return null;
  }
  return value;
}

function requireArray(path, value) {
  if (!Array.isArray(value)) {
    failures.push(`${path} must be an array`);
    return [];
  }
  return value;
}

function requireOneOf(path, value, allowed) {
  if (!allowed.includes(value)) {
    failures.push(`${path} must be one of ${allowed.join(", ")}`);
  }
}
```

Then validate each `contract.activeHooks` entry:

```javascript
const matrix = requireObject(`${contractPath}.hookMatrix`, contract.hookMatrix) ?? {};
for (const hook of contract.activeHooks ?? []) {
  const entry = requireObject(`hookMatrix.${hook}`, matrix[hook]);
  if (!entry) continue;
  requireOneOf(`hookMatrix.${hook}.kind`, entry.kind, ["request", "response", "stream", "log"]);
  requireOneOf(`hookMatrix.${hook}.status`, entry.status, ["active", "reserved"]);
  requireArray(`hookMatrix.${hook}.readPermissions`, entry.readPermissions);
  requireArray(`hookMatrix.${hook}.writePermissions`, entry.writePermissions);
  requireArray(`hookMatrix.${hook}.mutationFields`, entry.mutationFields);
  requireArray(`hookMatrix.${hook}.contextFields`, entry.contextFields);
  if (entry.timeoutMs !== contract.defaultHookTimeoutMs) {
    failures.push(`hookMatrix.${hook}.timeoutMs must equal defaultHookTimeoutMs`);
  }
}
```

- [ ] **Step 5: Run the self-test and verify it passes**

Run:

```bash
node scripts/check-plugin-api-contract.selftest.mjs
```

Expected: PASS because the checker detects the intentionally incomplete fixture.

- [ ] **Step 6: Run the real contract check**

Run:

```bash
pnpm check:plugin-api-contract
```

Expected: PASS against the real repository.

- [ ] **Step 7: Commit contract checker hardening**

Run:

```bash
git add docs/plugins/plugin-api-v1-contract.json scripts/check-plugin-api-contract.mjs scripts/check-plugin-api-contract.selftest.mjs
git commit -m "test: harden plugin api contract checker"
```

Expected: commit contains only contract JSON and checker files.

### Task 2: Add Rust Contract Metadata

**Files:**
- Create: `src-tauri/src/gateway/plugins/contract.rs`
- Modify: `src-tauri/src/gateway/plugins/mod.rs`
- Modify: `src-tauri/src/domain/plugins.rs`
- Test: Rust unit tests in `src-tauri/src/gateway/plugins/contract.rs`

- [ ] **Step 1: Write Rust metadata tests**

Create `src-tauri/src/gateway/plugins/contract.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_hook_metadata_matches_plugin_api_v1() {
        let ids: Vec<&'static str> = ACTIVE_HOOKS.iter().map(|hook| hook.id).collect();
        assert_eq!(
            ids,
            vec![
                "gateway.request.afterBodyRead",
                "gateway.request.beforeSend",
                "gateway.response.chunk",
                "gateway.response.after",
                "gateway.error",
                "log.beforePersist",
            ]
        );
    }

    #[test]
    fn reserved_hook_metadata_matches_plugin_api_v1() {
        assert!(is_reserved_hook("gateway.request.received"));
        assert!(is_reserved_hook("gateway.request.beforeProviderResolution"));
        assert!(is_reserved_hook("gateway.response.headers"));
        assert!(!is_reserved_hook("gateway.response.after"));
    }

    #[test]
    fn permission_metadata_marks_reserved_permissions() {
        assert!(is_reserved_permission("network.fetch"));
        assert!(is_reserved_permission("file.read"));
        assert!(!is_reserved_permission("request.body.read"));
    }
}
```

- [ ] **Step 2: Run the new Rust test and verify it fails to compile**

Run:

```bash
cd src-tauri && cargo test active_hook_metadata_matches_plugin_api_v1 --lib
```

Expected: FAIL because `ACTIVE_HOOKS`, `is_reserved_hook`, and `is_reserved_permission` are not implemented yet.

- [ ] **Step 3: Implement Rust contract metadata**

Add the implementation above the tests in `contract.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HookKind {
    Request,
    Response,
    Stream,
    Log,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HookStatus {
    Active,
    Reserved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HookContract {
    pub(crate) id: &'static str,
    pub(crate) kind: HookKind,
    pub(crate) status: HookStatus,
    pub(crate) read_permissions: &'static [&'static str],
    pub(crate) write_permissions: &'static [&'static str],
    pub(crate) mutation_fields: &'static [&'static str],
    pub(crate) timeout_ms: u64,
    pub(crate) default_failure_policy: &'static str,
}

pub(crate) const DEFAULT_HOOK_TIMEOUT_MS: u64 = 150;
pub(crate) const DEFAULT_FAILURE_POLICY: &str = "fail-open";

pub(crate) const ACTIVE_HOOKS: &[HookContract] = &[
    HookContract {
        id: "gateway.request.afterBodyRead",
        kind: HookKind::Request,
        status: HookStatus::Active,
        read_permissions: &[
            "request.meta.read",
            "request.header.read",
            "request.header.readSensitive",
            "request.body.read",
        ],
        write_permissions: &["request.header.write", "request.body.write"],
        mutation_fields: &["headers", "requestBody"],
        timeout_ms: DEFAULT_HOOK_TIMEOUT_MS,
        default_failure_policy: DEFAULT_FAILURE_POLICY,
    },
    HookContract {
        id: "gateway.request.beforeSend",
        kind: HookKind::Request,
        status: HookStatus::Active,
        read_permissions: &[
            "request.meta.read",
            "request.header.read",
            "request.header.readSensitive",
            "request.body.read",
        ],
        write_permissions: &["request.header.write", "request.body.write"],
        mutation_fields: &["headers", "requestBody"],
        timeout_ms: DEFAULT_HOOK_TIMEOUT_MS,
        default_failure_policy: DEFAULT_FAILURE_POLICY,
    },
    HookContract {
        id: "gateway.response.chunk",
        kind: HookKind::Stream,
        status: HookStatus::Active,
        read_permissions: &["stream.inspect"],
        write_permissions: &["stream.modify"],
        mutation_fields: &["streamChunk"],
        timeout_ms: DEFAULT_HOOK_TIMEOUT_MS,
        default_failure_policy: DEFAULT_FAILURE_POLICY,
    },
    HookContract {
        id: "gateway.response.after",
        kind: HookKind::Response,
        status: HookStatus::Active,
        read_permissions: &["response.header.read", "response.body.read"],
        write_permissions: &["response.header.write", "response.body.write"],
        mutation_fields: &["headers", "responseBody"],
        timeout_ms: DEFAULT_HOOK_TIMEOUT_MS,
        default_failure_policy: DEFAULT_FAILURE_POLICY,
    },
    HookContract {
        id: "gateway.error",
        kind: HookKind::Response,
        status: HookStatus::Active,
        read_permissions: &["response.header.read", "response.body.read"],
        write_permissions: &["response.header.write", "response.body.write"],
        mutation_fields: &["headers", "responseBody"],
        timeout_ms: DEFAULT_HOOK_TIMEOUT_MS,
        default_failure_policy: DEFAULT_FAILURE_POLICY,
    },
    HookContract {
        id: "log.beforePersist",
        kind: HookKind::Log,
        status: HookStatus::Active,
        read_permissions: &["log.redact"],
        write_permissions: &["log.redact"],
        mutation_fields: &["logMessage"],
        timeout_ms: DEFAULT_HOOK_TIMEOUT_MS,
        default_failure_policy: DEFAULT_FAILURE_POLICY,
    },
];

pub(crate) const RESERVED_HOOKS: &[&str] = &[
    "gateway.request.received",
    "gateway.request.beforeProviderResolution",
    "gateway.response.headers",
];

pub(crate) const RESERVED_PERMISSIONS: &[&str] = &[
    "plugin.storage",
    "network.fetch",
    "file.read",
    "file.write",
    "secret.read",
];

pub(crate) fn hook_contract(id: &str) -> Option<&'static HookContract> {
    ACTIVE_HOOKS.iter().find(|hook| hook.id == id)
}

pub(crate) fn is_active_hook(id: &str) -> bool {
    hook_contract(id).is_some()
}

pub(crate) fn is_reserved_hook(id: &str) -> bool {
    RESERVED_HOOKS.iter().any(|hook| *hook == id)
}

pub(crate) fn is_known_hook(id: &str) -> bool {
    is_active_hook(id) || is_reserved_hook(id)
}

pub(crate) fn is_reserved_permission(permission: &str) -> bool {
    RESERVED_PERMISSIONS.iter().any(|item| *item == permission)
}
```

- [ ] **Step 4: Export the contract module**

Modify `src-tauri/src/gateway/plugins/mod.rs`:

```rust
pub(crate) mod audit;
pub(crate) mod context;
pub(crate) mod contract;
pub(crate) mod permissions;
pub(crate) mod pipeline;
```

- [ ] **Step 5: Use contract metadata in domain validation**

In `src-tauri/src/domain/plugins.rs`, replace the bodies of `is_known_hook`, `is_active_gateway_hook`, `is_reserved_gateway_hook`, and `is_reserved_permission`:

```rust
pub fn is_known_hook(hook: &str) -> bool {
    crate::gateway::plugins::contract::is_known_hook(hook)
}

pub fn is_active_gateway_hook(hook: &str) -> bool {
    crate::gateway::plugins::contract::is_active_hook(hook)
}

pub fn is_reserved_gateway_hook(hook: &str) -> bool {
    crate::gateway::plugins::contract::is_reserved_hook(hook)
}

pub fn is_reserved_permission(permission: &str) -> bool {
    crate::gateway::plugins::contract::is_reserved_permission(permission)
}
```

- [ ] **Step 6: Run focused Rust tests**

Run:

```bash
cd src-tauri && cargo test active_hook_metadata_matches_plugin_api_v1 --lib
cd src-tauri && cargo test reserved_hook_metadata_matches_plugin_api_v1 --lib
cd src-tauri && cargo test plugin_manifest --lib
```

Expected: all pass.

- [ ] **Step 7: Run contract drift check**

Run:

```bash
pnpm check:plugin-api-contract
```

Expected: PASS.

- [ ] **Step 8: Commit Rust contract metadata**

Run:

```bash
git add src-tauri/src/gateway/plugins/contract.rs src-tauri/src/gateway/plugins/mod.rs src-tauri/src/domain/plugins.rs
git commit -m "refactor(plugins): centralize rust plugin contract metadata"
```

Expected: commit contains only Rust contract metadata and validation call-through changes.

## Hook Registry Layer

### Task 3: Introduce Hook Registry Descriptors

**Files:**
- Create: `src-tauri/src/gateway/plugins/registry.rs`
- Modify: `src-tauri/src/gateway/plugins/mod.rs`
- Modify: `src-tauri/src/gateway/plugins/context.rs`
- Test: Rust unit tests in `src-tauri/src/gateway/plugins/registry.rs`

- [ ] **Step 1: Write registry tests**

Create `registry.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::plugins::context::GatewayPluginHookName;

    #[test]
    fn registry_resolves_active_request_hook() {
        let registry = HookRegistry::new();
        let descriptor = registry
            .descriptor(GatewayPluginHookName::RequestAfterBodyRead)
            .expect("descriptor");
        assert_eq!(descriptor.id, "gateway.request.afterBodyRead");
        assert_eq!(descriptor.kind, HookKind::Request);
        assert!(descriptor.allows_read_permission("request.body.read"));
        assert!(descriptor.allows_mutation_field("requestBody"));
    }

    #[test]
    fn registry_marks_stream_chunk_as_stream_kind() {
        let registry = HookRegistry::new();
        let descriptor = registry
            .descriptor(GatewayPluginHookName::ResponseChunk)
            .expect("descriptor");
        assert_eq!(descriptor.kind, HookKind::Stream);
        assert!(descriptor.allows_read_permission("stream.inspect"));
        assert!(descriptor.allows_write_permission("stream.modify"));
    }
}
```

- [ ] **Step 2: Run registry tests and verify compile failure**

Run:

```bash
cd src-tauri && cargo test registry_resolves_active_request_hook --lib
```

Expected: FAIL because `HookRegistry` is not implemented.

- [ ] **Step 3: Implement HookRegistry**

Add implementation above the tests:

```rust
use super::contract::{self, HookKind};
use super::context::GatewayPluginHookName;

#[derive(Debug, Clone, Copy)]
pub(crate) struct HookDescriptor {
    pub(crate) hook_name: GatewayPluginHookName,
    pub(crate) id: &'static str,
    pub(crate) kind: HookKind,
    pub(crate) read_permissions: &'static [&'static str],
    pub(crate) write_permissions: &'static [&'static str],
    pub(crate) mutation_fields: &'static [&'static str],
    pub(crate) timeout_ms: u64,
    pub(crate) default_failure_policy: &'static str,
}

impl HookDescriptor {
    pub(crate) fn allows_read_permission(self, permission: &str) -> bool {
        self.read_permissions.iter().any(|item| *item == permission)
    }

    pub(crate) fn allows_write_permission(self, permission: &str) -> bool {
        self.write_permissions.iter().any(|item| *item == permission)
    }

    pub(crate) fn allows_mutation_field(self, field: &str) -> bool {
        self.mutation_fields.iter().any(|item| *item == field)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct HookRegistry;

impl HookRegistry {
    pub(crate) fn new() -> Self {
        Self
    }

    pub(crate) fn descriptor(self, hook_name: GatewayPluginHookName) -> Option<HookDescriptor> {
        let contract = contract::hook_contract(hook_name.as_str())?;
        Some(HookDescriptor {
            hook_name,
            id: contract.id,
            kind: contract.kind,
            read_permissions: contract.read_permissions,
            write_permissions: contract.write_permissions,
            mutation_fields: contract.mutation_fields,
            timeout_ms: contract.timeout_ms,
            default_failure_policy: contract.default_failure_policy,
        })
    }
}
```

- [ ] **Step 4: Export registry module**

Modify `src-tauri/src/gateway/plugins/mod.rs`:

```rust
pub(crate) mod registry;
```

- [ ] **Step 5: Add string conversion helper on hook names**

In `src-tauri/src/gateway/plugins/context.rs`, add:

```rust
impl GatewayPluginHookName {
    pub(crate) fn from_str(raw: &str) -> Option<Self> {
        match raw {
            "gateway.request.received" => Some(Self::RequestReceived),
            "gateway.request.afterBodyRead" => Some(Self::RequestAfterBodyRead),
            "gateway.request.beforeProviderResolution" => Some(Self::RequestBeforeProviderResolution),
            "gateway.request.beforeSend" => Some(Self::RequestBeforeSend),
            "gateway.response.headers" => Some(Self::ResponseHeaders),
            "gateway.response.chunk" => Some(Self::ResponseChunk),
            "gateway.response.after" => Some(Self::ResponseAfter),
            "gateway.error" => Some(Self::Error),
            "log.beforePersist" => Some(Self::LogBeforePersist),
            _ => None,
        }
    }
}
```

- [ ] **Step 6: Run registry tests**

Run:

```bash
cd src-tauri && cargo test registry_resolves_active_request_hook --lib
cd src-tauri && cargo test registry_marks_stream_chunk_as_stream_kind --lib
```

Expected: PASS.

- [ ] **Step 7: Commit hook registry skeleton**

Run:

```bash
git add src-tauri/src/gateway/plugins/registry.rs src-tauri/src/gateway/plugins/mod.rs src-tauri/src/gateway/plugins/context.rs
git commit -m "refactor(plugins): add internal hook registry"
```

Expected: commit contains registry skeleton and hook string conversion.

### Task 4: Move Mutation Enforcement Behind Descriptors

**Files:**
- Create: `src-tauri/src/gateway/plugins/mutation.rs`
- Modify: `src-tauri/src/gateway/plugins/mod.rs`
- Modify: `src-tauri/src/gateway/plugins/permissions.rs`
- Modify: `src-tauri/src/gateway/plugins/pipeline.rs`
- Test: Rust unit tests in `mutation.rs`

- [ ] **Step 1: Write descriptor-driven mutation tests**

Create `mutation.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::plugins::context::{
        GatewayHookAction, GatewayHookResult, GatewayPluginHookName,
    };
    use crate::gateway::plugins::registry::HookRegistry;

    #[test]
    fn request_body_mutation_requires_descriptor_permission() {
        let descriptor = HookRegistry::new()
            .descriptor(GatewayPluginHookName::RequestBeforeSend)
            .expect("descriptor");
        let result = GatewayHookResult {
            action: GatewayHookAction::Continue,
            request_body: Some("changed".to_string()),
            response_body: None,
            stream_chunk: None,
            headers: Default::default(),
            log_message: None,
            reason: None,
        };

        let err = enforce_descriptor_permissions(descriptor, &[], &result)
            .expect_err("missing write permission");
        assert_eq!(err.code_for_logging(), "PLUGIN_PERMISSION_DENIED");

        enforce_descriptor_permissions(
            descriptor,
            &["request.body.write".to_string()],
            &result,
        )
        .expect("permission granted");
    }
}
```

- [ ] **Step 2: Add `code_for_logging` accessor**

In `src-tauri/src/gateway/plugins/permissions.rs`, add a non-test accessor:

```rust
impl GatewayPluginError {
    pub(crate) fn code_for_logging(&self) -> &'static str {
        self.code
    }
}
```

Keep the existing `#[cfg(test)] fn code(&self)` until all tests are migrated.

- [ ] **Step 3: Run mutation test and verify compile failure**

Run:

```bash
cd src-tauri && cargo test request_body_mutation_requires_descriptor_permission --lib
```

Expected: FAIL because `enforce_descriptor_permissions` does not exist.

- [ ] **Step 4: Implement descriptor-driven enforcement**

Add implementation above tests in `mutation.rs`:

```rust
use super::context::GatewayHookResult;
use super::permissions::GatewayPluginError;
use super::registry::HookDescriptor;

pub(crate) fn enforce_descriptor_permissions(
    descriptor: HookDescriptor,
    permissions: &[String],
    result: &GatewayHookResult,
) -> Result<(), GatewayPluginError> {
    if result.request_body.is_some() {
        require_mutation(descriptor, "requestBody", "request.body.write")?;
        require_permission(permissions, "request.body.write")?;
    }
    if result.response_body.is_some() {
        require_mutation(descriptor, "responseBody", "response.body.write")?;
        require_permission(permissions, "response.body.write")?;
    }
    if result.stream_chunk.is_some() {
        require_mutation(descriptor, "streamChunk", "stream.modify")?;
        require_permission(permissions, "stream.modify")?;
    }
    if result.log_message.is_some() {
        require_mutation(descriptor, "logMessage", "log.redact")?;
        require_permission(permissions, "log.redact")?;
    }
    if !result.headers.is_empty() {
        require_mutation(descriptor, "headers", header_write_permission(descriptor)?)?;
        require_permission(permissions, header_write_permission(descriptor)?)?;
    }
    Ok(())
}

fn header_write_permission(descriptor: HookDescriptor) -> Result<&'static str, GatewayPluginError> {
    if descriptor.allows_write_permission("request.header.write") {
        Ok("request.header.write")
    } else if descriptor.allows_write_permission("response.header.write") {
        Ok("response.header.write")
    } else {
        Err(GatewayPluginError::new(
            "PLUGIN_PERMISSION_DENIED",
            format!("headers cannot be mutated in {}", descriptor.id),
        ))
    }
}

fn require_mutation(
    descriptor: HookDescriptor,
    field: &'static str,
    permission: &'static str,
) -> Result<(), GatewayPluginError> {
    if descriptor.allows_mutation_field(field) && descriptor.allows_write_permission(permission) {
        Ok(())
    } else {
        Err(GatewayPluginError::new(
            "PLUGIN_PERMISSION_DENIED",
            format!("{field} mutation is not allowed in {}", descriptor.id),
        ))
    }
}

fn require_permission(
    permissions: &[String],
    permission: &'static str,
) -> Result<(), GatewayPluginError> {
    if permissions.iter().any(|item| item == permission) {
        Ok(())
    } else {
        Err(GatewayPluginError::new(
            "PLUGIN_PERMISSION_DENIED",
            format!("missing plugin permission: {permission}"),
        ))
    }
}
```

- [ ] **Step 5: Export mutation module**

Modify `src-tauri/src/gateway/plugins/mod.rs`:

```rust
pub(crate) mod mutation;
```

- [ ] **Step 6: Use descriptor enforcement in pipeline**

In `src-tauri/src/gateway/plugins/pipeline.rs`, replace calls to `enforce_hook_result_permissions` with:

```rust
let descriptor = crate::gateway::plugins::registry::HookRegistry::new()
    .descriptor(input.hook_name)
    .ok_or_else(|| GatewayPluginError::new(
        "PLUGIN_UNKNOWN_HOOK",
        format!("unknown plugin hook: {}", input.hook_name.as_str()),
    ))?;
if let Err(err) = crate::gateway::plugins::mutation::enforce_descriptor_permissions(
    descriptor,
    &plugin.granted_permissions,
    &result,
) {
    // keep the existing failure/audit/fail-open or fail-closed branch unchanged
}
```

Apply the same pattern to request, response, stream, and log hook execution paths.

- [ ] **Step 7: Keep compatibility wrapper**

Leave `enforce_hook_result_permissions` in `permissions.rs` as a wrapper:

```rust
pub(crate) fn enforce_hook_result_permissions(
    hook_name: GatewayPluginHookName,
    permissions: &[String],
    result: &GatewayHookResult,
) -> Result<(), GatewayPluginError> {
    let descriptor = crate::gateway::plugins::registry::HookRegistry::new()
        .descriptor(hook_name)
        .ok_or_else(|| {
            GatewayPluginError::new(
                "PLUGIN_UNKNOWN_HOOK",
                format!("unknown plugin hook: {}", hook_name.as_str()),
            )
        })?;
    crate::gateway::plugins::mutation::enforce_descriptor_permissions(
        descriptor,
        permissions,
        result,
    )
}
```

- [ ] **Step 8: Run mutation and pipeline tests**

Run:

```bash
cd src-tauri && cargo test request_body_mutation_requires_descriptor_permission --lib
cd src-tauri && cargo test gateway_plugin_context_permission_enforces_write_permissions --lib
cd src-tauri && cargo test gateway_plugin_response_pipeline_applies_body_and_header_changes --lib
```

Expected: all pass.

- [ ] **Step 9: Commit mutation descriptor enforcement**

Run:

```bash
git add src-tauri/src/gateway/plugins/mutation.rs src-tauri/src/gateway/plugins/mod.rs src-tauri/src/gateway/plugins/permissions.rs src-tauri/src/gateway/plugins/pipeline.rs
git commit -m "refactor(plugins): enforce mutations through hook descriptors"
```

Expected: commit contains descriptor enforcement only.

## Runtime Layer

### Task 5: Introduce Runtime Policy and Runtime Manager Facade

**Files:**
- Create: `src-tauri/src/app/plugins/runtime_policy.rs`
- Create: `src-tauri/src/app/plugins/runtime_manager.rs`
- Modify: `src-tauri/src/app/plugins/mod.rs`
- Modify: `src-tauri/src/app/plugins/runtime_executor.rs`
- Test: Rust unit tests in `runtime_manager.rs`

- [ ] **Step 1: Write runtime manager tests**

Create `runtime_manager.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::plugins::runtime_policy::RuntimePolicy;
    use crate::domain::plugins::PluginRuntime;

    #[test]
    fn runtime_manager_rejects_wasm_when_policy_disabled() {
        let manager = PluginRuntimeManager::for_tests(RuntimePolicy {
            wasm_enabled: false,
            process_enabled: false,
        });
        let err = manager
            .validate_runtime_policy(&PluginRuntime::Wasm {
                abi_version: "1.0.0".to_string(),
                memory_limit_bytes: Some(16 * 1024 * 1024),
            })
            .expect_err("wasm disabled");
        assert_eq!(err.code_for_logging(), "PLUGIN_RUNTIME_DISABLED");
    }

    #[test]
    fn runtime_manager_allows_declarative_rules_policy() {
        let manager = PluginRuntimeManager::for_tests(RuntimePolicy::default());
        manager
            .validate_runtime_policy(&PluginRuntime::DeclarativeRules {
                rules: vec!["rules/main.json".to_string()],
            })
            .expect("declarative rules allowed");
    }
}
```

- [ ] **Step 2: Add runtime policy module**

Create `runtime_policy.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RuntimePolicy {
    pub(crate) wasm_enabled: bool,
    pub(crate) process_enabled: bool,
}

impl Default for RuntimePolicy {
    fn default() -> Self {
        Self {
            wasm_enabled: false,
            process_enabled: false,
        }
    }
}
```

- [ ] **Step 3: Run tests and verify compile failure**

Run:

```bash
cd src-tauri && cargo test runtime_manager_rejects_wasm_when_policy_disabled --lib
```

Expected: FAIL because `PluginRuntimeManager` is not implemented.

- [ ] **Step 4: Implement runtime manager policy facade**

Add implementation above tests in `runtime_manager.rs`:

```rust
use crate::app::plugins::runtime_policy::RuntimePolicy;
use crate::domain::plugins::PluginRuntime;
use crate::gateway::plugins::permissions::GatewayPluginError;

#[derive(Default)]
pub(crate) struct PluginRuntimeManager {
    policy: RuntimePolicy,
}

impl PluginRuntimeManager {
    #[cfg(test)]
    pub(crate) fn for_tests(policy: RuntimePolicy) -> Self {
        Self { policy }
    }

    pub(crate) fn new(policy: RuntimePolicy) -> Self {
        Self { policy }
    }

    pub(crate) fn validate_runtime_policy(
        &self,
        runtime: &PluginRuntime,
    ) -> Result<(), GatewayPluginError> {
        match runtime {
            PluginRuntime::DeclarativeRules { .. } => Ok(()),
            PluginRuntime::Native { engine } if engine == "privacyFilter" => Ok(()),
            PluginRuntime::Native { engine } => Err(GatewayPluginError::new(
                "PLUGIN_UNSUPPORTED_RUNTIME",
                format!("unsupported native plugin runtime engine: {engine}"),
            )),
            PluginRuntime::Wasm { .. } if !self.policy.wasm_enabled => {
                Err(GatewayPluginError::new(
                    "PLUGIN_RUNTIME_DISABLED",
                    "wasm runtime execution is disabled by host policy",
                ))
            }
            PluginRuntime::Wasm { .. } => Err(GatewayPluginError::new(
                "PLUGIN_WASM_NOT_WIRED",
                "wasm runtime policy is enabled but gateway execution is not wired in this release",
            )),
        }
    }
}
```

- [ ] **Step 5: Export runtime modules**

Modify `src-tauri/src/app/plugins/mod.rs`:

```rust
pub(crate) mod runtime_manager;
pub(crate) mod runtime_policy;
```

- [ ] **Step 6: Delegate old executor policy checks**

In `runtime_executor.rs`, keep the public struct but call the manager:

```rust
let manager = crate::app::plugins::runtime_manager::PluginRuntimeManager::new(
    crate::app::plugins::runtime_policy::RuntimePolicy {
        wasm_enabled: self.policy.wasm_enabled,
        process_enabled: false,
    },
);
manager.validate_runtime_policy(&plugin.manifest.runtime)?;
```

Place this at the start of `execute_plugin_sync`, then keep the existing declarative/native dispatch branches. The WASM branches become unreachable after policy validation, so return the same existing errors if they are still matched.

- [ ] **Step 7: Run focused runtime tests**

Run:

```bash
cd src-tauri && cargo test runtime_manager_rejects_wasm_when_policy_disabled --lib
cd src-tauri && cargo test runtime_executor_returns_clear_error_for_policy_disabled_wasm --lib
```

Expected: all pass and error code remains `PLUGIN_RUNTIME_DISABLED`.

- [ ] **Step 8: Commit runtime manager facade**

Run:

```bash
git add src-tauri/src/app/plugins/runtime_manager.rs src-tauri/src/app/plugins/runtime_policy.rs src-tauri/src/app/plugins/mod.rs src-tauri/src/app/plugins/runtime_executor.rs
git commit -m "refactor(plugins): introduce runtime manager policy facade"
```

Expected: commit keeps external runtime behavior unchanged.

### Task 6: Extract Runtime Cache Helpers

**Files:**
- Create: `src-tauri/src/app/plugins/runtime_cache.rs`
- Modify: `src-tauri/src/app/plugins/mod.rs`
- Modify: `src-tauri/src/app/plugins/rule_runtime.rs`
- Test: Rust unit tests in `runtime_cache.rs`

- [ ] **Step 1: Write cache key tests**

Create `runtime_cache.rs` with tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_key_changes_with_updated_at() {
        let a = RuntimeCacheKeyInput {
            plugin_id: "acme.redactor",
            version: "1.0.0",
            installed_dir: "/tmp/acme",
            updated_at: 1,
            runtime_key: "rules/main.json",
        };
        let b = RuntimeCacheKeyInput { updated_at: 2, ..a };
        assert_ne!(runtime_cache_key(a), runtime_cache_key(b));
    }

    #[test]
    fn cache_key_includes_runtime_key() {
        let a = RuntimeCacheKeyInput {
            plugin_id: "acme.redactor",
            version: "1.0.0",
            installed_dir: "/tmp/acme",
            updated_at: 1,
            runtime_key: "rules/a.json",
        };
        let b = RuntimeCacheKeyInput {
            runtime_key: "rules/b.json",
            ..a
        };
        assert_ne!(runtime_cache_key(a), runtime_cache_key(b));
    }
}
```

- [ ] **Step 2: Implement runtime cache key helper**

Add above tests:

```rust
#[derive(Debug, Clone, Copy)]
pub(crate) struct RuntimeCacheKeyInput<'a> {
    pub(crate) plugin_id: &'a str,
    pub(crate) version: &'a str,
    pub(crate) installed_dir: &'a str,
    pub(crate) updated_at: i64,
    pub(crate) runtime_key: &'a str,
}

pub(crate) fn runtime_cache_key(input: RuntimeCacheKeyInput<'_>) -> String {
    format!(
        "{}\u{1e}{}\u{1e}{}\u{1e}{}\u{1e}{}",
        input.plugin_id,
        input.version,
        input.installed_dir,
        input.updated_at,
        input.runtime_key
    )
}
```

- [ ] **Step 3: Export runtime cache module**

Modify `src-tauri/src/app/plugins/mod.rs`:

```rust
pub(crate) mod runtime_cache;
```

- [ ] **Step 4: Use helper in rule runtime cache keys**

In `src-tauri/src/app/plugins/rule_runtime.rs`, replace the string formatting in `rule_runtime_cache_key` with:

```rust
crate::app::plugins::runtime_cache::runtime_cache_key(
    crate::app::plugins::runtime_cache::RuntimeCacheKeyInput {
        plugin_id: &plugin.summary.plugin_id,
        version,
        installed_dir,
        updated_at,
        runtime_key: &rules,
    },
)
```

Replace `privacy_filter_cache_key` similarly, using runtime key `"native:privacyFilter"`.

- [ ] **Step 5: Run cache and rule runtime tests**

Run:

```bash
cd src-tauri && cargo test cache_key_changes_with_updated_at --lib
cd src-tauri && cargo test cache_key_includes_runtime_key --lib
cd src-tauri && cargo test rule_runtime --lib
```

Expected: all pass.

- [ ] **Step 6: Commit runtime cache extraction**

Run:

```bash
git add src-tauri/src/app/plugins/runtime_cache.rs src-tauri/src/app/plugins/mod.rs src-tauri/src/app/plugins/rule_runtime.rs
git commit -m "refactor(plugins): share runtime cache key helpers"
```

Expected: commit contains cache helper extraction only.

## Provider Adapter Layer

### Task 7: Add Provider Adapter Facade Without Moving Behavior

**Files:**
- Create: `src-tauri/src/gateway/proxy/provider_adapters/mod.rs`
- Create: `src-tauri/src/gateway/proxy/provider_adapters/cx2cc.rs`
- Create: `src-tauri/src/gateway/proxy/provider_adapters/gemini_oauth.rs`
- Create: `src-tauri/src/gateway/proxy/provider_adapters/codex_chatgpt.rs`
- Create: `src-tauri/src/gateway/proxy/provider_adapters/claude.rs`
- Modify: `src-tauri/src/gateway/proxy/mod.rs`
- Test: Rust unit tests in `provider_adapters/mod.rs`

- [ ] **Step 1: Write provider adapter registry tests**

Create `provider_adapters/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_identifies_cx2cc_bridge_capability() {
        let caps = ProviderCapabilities {
            cx2cc_bridge: true,
            ..ProviderCapabilities::default()
        };
        assert!(caps.cx2cc_bridge);
        assert!(caps.supports_count_tokens_local_intercept());
    }

    #[test]
    fn registry_default_capabilities_are_plain_provider() {
        let caps = ProviderCapabilities::default();
        assert!(!caps.cx2cc_bridge);
        assert!(!caps.gemini_oauth);
        assert!(!caps.codex_chatgpt_backend);
    }
}
```

- [ ] **Step 2: Implement provider capability facade**

Add above tests:

```rust
pub(crate) mod claude;
pub(crate) mod codex_chatgpt;
pub(crate) mod cx2cc;
pub(crate) mod gemini_oauth;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ProviderCapabilities {
    pub(crate) anthropic_compatible: bool,
    pub(crate) openai_responses_compatible: bool,
    pub(crate) codex_chatgpt_backend: bool,
    pub(crate) gemini_oauth: bool,
    pub(crate) cx2cc_bridge: bool,
    pub(crate) service_tier_adjustment: bool,
    pub(crate) stream_idle_timeout_override: bool,
}

impl ProviderCapabilities {
    pub(crate) fn supports_count_tokens_local_intercept(self) -> bool {
        self.cx2cc_bridge
    }
}
```

- [ ] **Step 3: Add empty adapter modules with explicit purpose**

Create each module with a narrow compatibility shim.

`cx2cc.rs`:

```rust
pub(crate) fn is_count_tokens_intercept_supported(
    is_claude_count_tokens: bool,
    capabilities: super::ProviderCapabilities,
) -> bool {
    is_claude_count_tokens && capabilities.supports_count_tokens_local_intercept()
}
```

`gemini_oauth.rs`:

```rust
pub(crate) fn is_gemini_oauth(capabilities: super::ProviderCapabilities) -> bool {
    capabilities.gemini_oauth
}
```

`codex_chatgpt.rs`:

```rust
pub(crate) fn is_codex_chatgpt_backend(capabilities: super::ProviderCapabilities) -> bool {
    capabilities.codex_chatgpt_backend
}
```

`claude.rs`:

```rust
pub(crate) fn is_anthropic_compatible(capabilities: super::ProviderCapabilities) -> bool {
    capabilities.anthropic_compatible
}
```

- [ ] **Step 4: Export provider adapter module**

Modify `src-tauri/src/gateway/proxy/mod.rs`:

```rust
pub(crate) mod provider_adapters;
```

- [ ] **Step 5: Run provider adapter tests**

Run:

```bash
cd src-tauri && cargo test registry_identifies_cx2cc_bridge_capability --lib
cd src-tauri && cargo test registry_default_capabilities_are_plain_provider --lib
```

Expected: PASS.

- [ ] **Step 6: Commit provider adapter facade**

Run:

```bash
git add src-tauri/src/gateway/proxy/provider_adapters src-tauri/src/gateway/proxy/mod.rs
git commit -m "refactor(gateway): add provider adapter capability facade"
```

Expected: commit adds facade only and does not change gateway behavior.

### Task 8: Route CX2CC Count Tokens Through Provider Adapter Facade

**Files:**
- Modify: `src-tauri/src/gateway/proxy/handler/middleware/cx2cc_count_tokens_interceptor.rs`
- Modify: `src-tauri/src/gateway/proxy/provider_adapters/cx2cc.rs`
- Test: existing tests in `cx2cc_count_tokens_interceptor.rs`

- [ ] **Step 1: Write a facade-focused CX2CC test**

In `cx2cc_count_tokens_interceptor.rs`, add:

```rust
#[test]
fn cx2cc_count_tokens_uses_adapter_capability() {
    let capabilities = crate::gateway::proxy::provider_adapters::ProviderCapabilities {
        cx2cc_bridge: true,
        ..Default::default()
    };
    assert!(
        crate::gateway::proxy::provider_adapters::cx2cc::is_count_tokens_intercept_supported(
            true,
            capabilities
        )
    );
}
```

- [ ] **Step 2: Run the test**

Run:

```bash
cd src-tauri && cargo test cx2cc_count_tokens_uses_adapter_capability --lib
```

Expected: PASS if Task 7 facade exists. If it fails, fix facade exports before changing behavior.

- [ ] **Step 3: Add provider-to-capabilities conversion**

In `provider_adapters/cx2cc.rs`, add:

```rust
pub(crate) fn capabilities_for_provider(
    provider: &crate::providers::ProviderForGateway,
) -> super::ProviderCapabilities {
    super::ProviderCapabilities {
        cx2cc_bridge: provider.is_cx2cc_bridge(),
        ..Default::default()
    }
}
```

- [ ] **Step 4: Use facade in interceptor**

Replace `should_intercept_cx2cc_count_tokens` body with:

```rust
pub(in crate::gateway::proxy::handler) fn should_intercept_cx2cc_count_tokens(
    is_claude_count_tokens: bool,
    providers: &[providers::ProviderForGateway],
) -> bool {
    providers.first().is_some_and(|provider| {
        let capabilities =
            crate::gateway::proxy::provider_adapters::cx2cc::capabilities_for_provider(provider);
        crate::gateway::proxy::provider_adapters::cx2cc::is_count_tokens_intercept_supported(
            is_claude_count_tokens,
            capabilities,
        )
    })
}
```

- [ ] **Step 5: Run existing CX2CC tests**

Run:

```bash
cd src-tauri && cargo test intercepts_count_tokens_only_when_first_provider_is_cx2cc --lib
cd src-tauri && cargo test cx2cc_count_tokens_response_body_is_positive --lib
cd src-tauri && cargo test cx2cc_count_tokens_response_sets_intercept_headers --lib
```

Expected: all pass with identical behavior.

- [ ] **Step 6: Commit CX2CC facade routing**

Run:

```bash
git add src-tauri/src/gateway/proxy/handler/middleware/cx2cc_count_tokens_interceptor.rs src-tauri/src/gateway/proxy/provider_adapters/cx2cc.rs
git commit -m "refactor(gateway): route cx2cc count tokens through provider adapter"
```

Expected: commit routes one provider-specific behavior through the adapter facade.

## Integration and Documentation

### Task 9: Add Compatibility Documentation for 0.62 Internal Refactor

**Files:**
- Modify: `docs/plugins/architecture/README.md`
- Modify: `docs/plugins/architecture/audit.md`
- Modify: `docs/plugins/reference/compatibility.md`
- Modify: `scripts/check-plugin-system-docs.mjs`

- [ ] **Step 1: Write docs checker assertion**

In `scripts/check-plugin-system-docs.mjs`, add required phrases for `docs/plugins/reference/compatibility.md`:

```javascript
const compatibility = readText("docs/plugins/reference/compatibility.md");
for (const phrase of [
  "Plugin API v1 remains externally compatible in 0.62",
  "0.62 does not add public provider plugin APIs",
  "0.62 keeps third-party JavaScript and WebView plugin execution unsupported",
]) {
  if (!compatibility.includes(phrase)) {
    failures.push(`docs/plugins/reference/compatibility.md: missing "${phrase}"`);
  }
}
```

- [ ] **Step 2: Run docs checker and verify failure**

Run:

```bash
pnpm check:plugin-system-docs
```

Expected: FAIL because compatibility docs do not yet include the new 0.62 statements.

- [ ] **Step 3: Update compatibility docs**

Add this section to `docs/plugins/reference/compatibility.md`:

```markdown
## 0.62 Internal Platform Kernel

Plugin API v1 remains externally compatible in 0.62. The release reorganizes host internals around contract metadata, hook descriptors, runtime policy, runtime cache lifecycle, and provider adapter facades.

0.62 does not add public provider plugin APIs. Provider adapter work is host-internal and exists to reduce gateway/provider maintenance cost before any future public API is considered.

0.62 keeps third-party JavaScript and WebView plugin execution unsupported. Community plugins continue to use `declarativeRules`; WASM remains controlled by host policy.
```

- [ ] **Step 4: Update architecture audit**

Add a short 0.62 decision note to `docs/plugins/architecture/audit.md`:

```markdown
## 0.62 Platform Kernel Decision

0.62 keeps Plugin API v1 externally compatible and focuses on internal platform boundaries. Contract metadata becomes the source for drift checks; hook behavior is routed through internal descriptors; runtime dispatch is separated from pipeline orchestration; provider-specific behavior starts moving behind provider adapter facades.
```

- [ ] **Step 5: Run docs checks**

Run:

```bash
pnpm check:plugin-system-docs
pnpm check:plugin-api-contract
```

Expected: both pass.

- [ ] **Step 6: Commit docs compatibility update**

Run:

```bash
git add docs/plugins/architecture/README.md docs/plugins/architecture/audit.md docs/plugins/reference/compatibility.md scripts/check-plugin-system-docs.mjs
git commit -m "docs: document 0.62 plugin platform compatibility"
```

Expected: commit contains docs and checker updates only.

### Task 10: Full Verification Gate

**Files:**
- No source changes unless a verification failure requires a focused fix

- [ ] **Step 1: Run frontend and plugin checks**

Run:

```bash
pnpm lint
pnpm typecheck
pnpm check:no-instant-now-sub
pnpm check:plugin-api-contract
pnpm check:plugin-system-docs
pnpm check:plugin-system-completion
pnpm plugin-sdk:typecheck
pnpm --filter @aio-coding-hub/plugin-sdk test
pnpm create-aio-plugin:test
pnpm plugin-wasm-sdk:test
```

Expected: all pass.

- [ ] **Step 2: Run Rust checks**

Run:

```bash
pnpm tauri:fmt
pnpm tauri:check
cd src-tauri && cargo test plugin --lib
cd src-tauri && cargo test provider --lib
cd src-tauri && cargo test gateway --lib
```

Expected: all pass.

- [ ] **Step 3: Run focused gateway behavior tests**

Run:

```bash
cd src-tauri && cargo test gateway_plugin_response_pipeline_applies_body_and_header_changes --lib
cd src-tauri && cargo test gateway_plugin_stream_pipeline_applies_chunk_changes --lib
cd src-tauri && cargo test intercepts_count_tokens_only_when_first_provider_is_cx2cc --lib
cd src-tauri && cargo test runtime_executor_returns_clear_error_for_policy_disabled_wasm --lib
```

Expected: all pass, proving Plugin API v1 and first provider adapter facade behavior remain compatible.

- [ ] **Step 4: Inspect final diff**

Run:

```bash
git status --short
git log --oneline -10
```

Expected: worktree clean after all task commits; recent commits correspond to this plan's task commits.

- [ ] **Step 5: Prepare completion summary**

Write a short summary for the user containing:

```text
- Contract checker now validates structured Plugin API v1 metadata.
- Rust hook metadata and HookRegistry centralize hook descriptors.
- Mutation enforcement uses descriptors while keeping Plugin API v1 behavior.
- Runtime policy/cache boundaries are separated from gateway pipeline orchestration.
- CX2CC count_tokens is the first provider-specific behavior routed through provider adapter facade.
- Verification commands run and their pass/fail status.
```

Expected: user can see what changed, what was verified, and what remains internal-only.

## Plan Self-Review

- Spec coverage: Contract, Hook Registry, Runtime, Provider Adapter, frontend/backend boundary, non-goals, behavior compatibility, tests, and performance concerns are represented by Tasks 0-10.
- Placeholder scan: no task uses open-ended instructions; each implementation task has concrete files, snippets, commands, and expected outcomes.
- Type consistency: names introduced in earlier tasks are reused consistently: `HookContract`, `HookRegistry`, `HookDescriptor`, `RuntimePolicy`, `PluginRuntimeManager`, `ProviderCapabilities`.
