# Extension Host-only Plugin Architecture Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Converge community plugins on Extension Host-only architecture by removing the old `declarativeRules` public runtime, enforcing capabilities, adding managed Extension Host lifecycle, and wiring an Extension Host gateway hook MVP.

**Architecture:** The public plugin contract becomes `plugin.json + extensionHost + contributes + capabilities`. SDK/devtools/Rust validation reject legacy rule runtimes, while the Rust host remains the authority for capability enforcement and runtime access. Commands and gateway hooks execute through a managed Extension Host instance registry; official host-owned privacy filter remains separate from community runtime policy.

**Tech Stack:** TypeScript, Vitest, Rust, Tauri 2, Tokio, rquickjs, JSON-RPC process runtime, rusqlite-backed plugin repository.

---

## Spec

Implement this approved design:

- `docs/superpowers/specs/2026-06-29-aio-coding-hub-extension-host-only-plugin-architecture-design.md`

## Scope Rules

- Do not push.
- Commit after each task.
- Preserve official privacy filter behavior.
- Do not delete internal gateway/provider rules that are not plugin `declarativeRules`.
- Do not open WASM, arbitrary process runtime, or third-party native plugins as community runtime choices.
- Keep UI extensions host-rendered through schema slots. Do not add plugin React injection.

## File Structure Map

### SDK and Tooling

- `packages/plugin-sdk/src/index.ts`: public manifest/runtime/contribution/capability types and SDK validation.
- `packages/plugin-sdk/src/index.test.ts`: SDK validation tests.
- `packages/plugin-sdk/src/index.typecheck.ts`: compile-time SDK contract sample.
- `packages/create-aio-plugin/src/scaffold.ts`: generated plugin templates.
- `packages/create-aio-plugin/src/scaffold.test.ts`: scaffold and devtools tests.
- `packages/create-aio-plugin/src/devtools.ts`: validate/doctor/pack/replay tooling. Remove rule replay/explain and migrate Extension Host checks.

### Rust Domain and Contribution Index

- `src-tauri/src/domain/plugins.rs`: manifest/runtime model and Rust validation.
- `src-tauri/src/domain/plugin_contributions.rs`: contribution structs, active slots, active capabilities.
- `src-tauri/src/app/plugins/contribution_registry.rs`: active contribution snapshot.
- `src-tauri/src/app/plugin_service.rs`: plugin list/get/preview/install/update/enable/disable/command service.
- `src-tauri/src/commands/plugins.rs`: Tauri IPC plugin commands.
- `src/generated/bindings.ts`: generated IPC bindings after Rust command/type changes.

### Extension Host Runtime

- `src-tauri/src/app/plugins/extension_host.rs`: Extension Host process wrapper and Host API handler.
- `src-tauri/src/app/plugins/extension_host_worker.rs`: rquickjs worker API injection and extension method dispatch.
- Create `src-tauri/src/app/plugins/extension_host_registry.rs`: managed warm Extension Host instance registry.
- `src-tauri/src/app/plugins/runtime_lifecycle.rs`: app-level lifecycle registry traits and dispose boundaries.
- `src-tauri/src/app/plugins/mod.rs`: module exports.
- `src-tauri/src/app/plugin_registry.rs`: Tauri managed state registration.
- `src-tauri/src/app/cleanup.rs`: app shutdown dispose hook.

### Gateway and Protocol Bridge

- `src-tauri/src/app/plugins/runtime_manager.rs`: runtime dispatch policy.
- `src-tauri/src/app/plugins/runtime_executor.rs`: gateway plugin executor.
- `src-tauri/src/app/plugins/rule_runtime.rs`: old community declarative rule runtime, to remove.
- `src-tauri/src/gateway/plugins/pipeline.rs`: ordered gateway plugin pipeline.
- `src-tauri/src/gateway/plugins/context.rs`: visible gateway context/result types.
- `src-tauri/src/gateway/plugins/registry.rs`: hook descriptors.
- `src-tauri/src/gateway/proxy/protocol_bridge/*`: existing built-in protocol bridge registry; plugin bridge skeleton must not replace built-ins.

### Frontend Product Copy and UI

- `src/pages/plugins/pluginProductCopy.ts`: runtime/capability copy.
- `src/services/plugins.ts` and `src/services/__tests__/plugins.test.ts`: plugin DTO expectations.
- `src/query/plugins.ts` and `src/query/__tests__/plugins.test.tsx`: command/query surfaces.
- `src/plugins/contributions/*`: host-rendered contribution behavior.
- `src/pages/PluginsPage.tsx` and tests: plugin runtime display and unsupported legacy copy if surfaced.

### Docs and Contract Checks

- `docs/plugin-manifest-v1.md`: public manifest reference.
- `docs/plugins/plugin-api-v1-contract.json`: machine-readable plugin API contract.
- `docs/plugins/**`: developer docs/examples.
- `scripts/check-plugin-api-contract.mjs`
- `scripts/check-plugin-system-docs.mjs`
- `scripts/check-plugin-system-completion.mjs`

## Task 1: SDK Contract Becomes Extension Host-only

**Files:**
- Modify: `packages/plugin-sdk/src/index.ts`
- Modify: `packages/plugin-sdk/src/index.test.ts`
- Modify: `packages/plugin-sdk/src/index.typecheck.ts`

- [ ] **Step 1: Write failing SDK tests for removed legacy runtime**

In `packages/plugin-sdk/src/index.test.ts`, add tests that assert:

```ts
test("rejects declarativeRules as unsupported public runtime", () => {
  const manifest = {
    ...baseManifest,
    runtime: { kind: "declarativeRules", rules: ["rules/main.json"] },
    hooks: [{ name: "gateway.request.afterBodyRead" }],
    permissions: ["request.body.read"],
  } as unknown as PluginManifest;

  expect(validateManifest(manifest)).toEqual({
    ok: false,
    error: {
      code: "PLUGIN_UNSUPPORTED_RUNTIME",
      message: "community plugins must use extensionHost runtime",
    },
  });
});

test("rejects gatewayRules contributions", () => {
  const manifest = extensionManifest({
    contributes: {
      gatewayRules: [{ rules: ["rules/main.json"] }],
    },
  });

  expect(validateManifest(manifest)).toEqual({
    ok: false,
    error: {
      code: "PLUGIN_INVALID_CONTRIBUTION",
      message: "gatewayRules are no longer supported; use gatewayHooks",
    },
  });
});
```

If the current test helpers use different names, adapt the test body to existing helpers but preserve the assertions.

- [ ] **Step 2: Run SDK tests and verify failure**

Run:

```bash
pnpm --filter @aio-coding-hub/plugin-sdk test -- index.test.ts
```

Expected: fail because `declarativeRules` and `gatewayRules` are still accepted.

- [ ] **Step 3: Update SDK runtime and manifest types**

In `packages/plugin-sdk/src/index.ts`:

- Remove `LegacyPluginRuntime`.
- Define `PluginRuntime = ExtensionRuntime`.
- Allow `ExtensionRuntime.language` to be `"typescript" | "javascript"` if the existing host validation is updated in Task 2; otherwise keep `"typescript"` and document JavaScript as generated output only.
- Remove `GatewayRuleContribution`.
- Remove `gatewayRules?: GatewayRuleContribution[]`.
- Remove top-level legacy manifest union that requires `hooks` and `permissions`.
- Keep `hooks?: never` and `permissions?: never` out of the public TypeScript type if practical. If that causes too much churn, keep fields absent from `PluginManifestBase` and rely on validation for raw manifests.

The resulting public shape should make this valid:

```ts
export type PluginManifest = PluginManifestBase & {
  runtime: ExtensionRuntime;
};
```

- [ ] **Step 4: Update SDK validation**

In `validateManifest`:

- Reject any runtime whose kind is not `"extensionHost"` with `PLUGIN_UNSUPPORTED_RUNTIME`.
- Reject top-level `hooks` if present with `PLUGIN_INVALID_MANIFEST`.
- Reject top-level `permissions` if present with `PLUGIN_INVALID_MANIFEST`.
- Reject `contributes.gatewayRules` with `PLUGIN_INVALID_CONTRIBUTION`.
- Require `main` for Extension Host.
- Validate activation events, contributes, and capabilities.
- Add contribution-to-capability validation:
  - `contributes.commands` requires `commands.execute`.
  - UI button fields require `commands.execute`.
  - `contributes.providers` requires `provider.extensionValues`.
  - `ui["providers.editor.sections"]` requires `provider.extensionValues`.
  - `contributes.gatewayHooks` requires `gateway.hooks`.
  - `contributes.protocolBridges` requires `protocol.bridge`.

Add a helper:

```ts
function validateCapabilityDependencies(
  contributes: PluginContributes,
  capabilities: readonly PluginCapability[]
): ValidationResult | null {
  const capabilitySet = new Set(capabilities);
  const requireCapability = (needed: PluginCapability, reason: string) => {
    if (!capabilitySet.has(needed)) {
      return invalid("PLUGIN_MISSING_CAPABILITY", `${reason} requires ${needed}`);
    }
    return null;
  };

  if ((contributes.commands?.length ?? 0) > 0) {
    const error = requireCapability("commands.execute", "commands contribution");
    if (error) return error;
  }
  if ((contributes.providers?.length ?? 0) > 0) {
    const error = requireCapability("provider.extensionValues", "provider contribution");
    if (error) return error;
  }
  if ((contributes.gatewayHooks?.length ?? 0) > 0) {
    const error = requireCapability("gateway.hooks", "gatewayHooks contribution");
    if (error) return error;
  }
  if ((contributes.protocolBridges?.length ?? 0) > 0) {
    const error = requireCapability("protocol.bridge", "protocolBridges contribution");
    if (error) return error;
  }
  if ((contributes.ui?.["providers.editor.sections"]?.length ?? 0) > 0) {
    const error = requireCapability(
      "provider.extensionValues",
      "providers.editor.sections UI contribution"
    );
    if (error) return error;
  }
  if (uiHasButtonCommand(contributes.ui)) {
    const error = requireCapability("commands.execute", "UI command field");
    if (error) return error;
  }
  return null;
}
```

Implement `uiHasButtonCommand` by walking `section` and `panel` schema fields and checking `field.type === "button"`.

- [ ] **Step 5: Update typecheck sample**

In `packages/plugin-sdk/src/index.typecheck.ts`:

- Replace the declarativeRules sample with an Extension Host manifest.
- Remove branches expecting `runtime.kind !== "declarativeRules"`.
- Keep examples for command, UI, provider, gatewayHooks, protocolBridges with required capabilities.

- [ ] **Step 6: Run SDK verification**

Run:

```bash
pnpm --filter @aio-coding-hub/plugin-sdk test
pnpm --filter @aio-coding-hub/plugin-sdk typecheck
```

Expected: both pass.

- [ ] **Step 7: Commit**

```bash
git add packages/plugin-sdk/src/index.ts packages/plugin-sdk/src/index.test.ts packages/plugin-sdk/src/index.typecheck.ts
git commit -m "feat(plugins): make sdk contract extension host only"
```

## Task 2: Rust Manifest and Contribution Validation Match SDK

**Files:**
- Modify: `src-tauri/src/domain/plugins.rs`
- Modify: `src-tauri/src/domain/plugin_contributions.rs`
- Modify: `src-tauri/src/app/plugins/contribution_registry.rs`

- [ ] **Step 1: Write failing Rust domain tests**

In `src-tauri/src/domain/plugins.rs` test module, add tests that assert:

- `PluginRuntime::DeclarativeRules` manifest fails with `PLUGIN_UNSUPPORTED_RUNTIME`.
- `PluginRuntime::Wasm` manifest fails with `PLUGIN_UNSUPPORTED_RUNTIME`.
- third-party `PluginRuntime::Native` manifest fails with `PLUGIN_UNSUPPORTED_RUNTIME`.
- Extension Host with top-level `hooks` fails.
- Extension Host with top-level `permissions` fails.
- Extension Host with `gateway_rules` fails.
- Extension Host `commands` without `commands.execute` fails with `PLUGIN_MISSING_CAPABILITY`.
- Extension Host `gateway_hooks` without `gateway.hooks` fails.
- Extension Host `protocol_bridges` without `protocol.bridge` fails.
- Extension Host `providers` or `providers.editor.sections` without `provider.extensionValues` fails.

Use existing test helpers around `serde_json::from_value::<PluginManifest>` where possible.

- [ ] **Step 2: Run targeted Rust tests and verify failure**

Run:

```bash
cd src-tauri && cargo test domain::plugins::tests:: --lib
```

Expected: new tests fail because legacy runtimes and missing capabilities are still accepted.

- [ ] **Step 3: Remove public legacy runtime variants**

In `src-tauri/src/domain/plugins.rs`, update `PluginRuntime`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PluginRuntime {
    ExtensionHost { language: String },
    Native { engine: String },
}
```

Keep `Native` only because official privacy filter manifests still need to deserialize. Validation must reject third-party native manifests outside official install paths.

Remove `DeclarativeRules` and `Wasm` branches from public validation. If deserializing unknown runtime kinds currently fails before validation, adjust install/preview code in Task 4 to convert that failure into `PLUGIN_UNSUPPORTED_RUNTIME`.

- [ ] **Step 4: Remove gatewayRules contribution model**

In `src-tauri/src/domain/plugin_contributions.rs`:

- Remove `gateway_rules` from `PluginContributes`.
- Remove `GatewayRuleContribution`.
- Keep `gateway_hooks`.
- Keep `protocol_bridges`.

In `src-tauri/src/app/plugins/contribution_registry.rs`:

- Remove `ActiveGatewayRuleContribution`.
- Remove `gateway_rules` from `ActiveContributionSnapshot`.
- Remove snapshot population for `contributes.gateway_rules`.

- [ ] **Step 5: Implement Rust capability dependency validation**

In `src-tauri/src/domain/plugins.rs`, add a helper close to `validate_capabilities`:

```rust
fn has_capability(capabilities: &[String], capability: &str) -> bool {
    capabilities.iter().any(|item| item == capability)
}

fn require_capability(
    capabilities: &[String],
    capability: &str,
    reason: &str,
) -> Result<(), PluginValidationError> {
    if has_capability(capabilities, capability) {
        return Ok(());
    }
    Err(PluginValidationError::new(
        "PLUGIN_MISSING_CAPABILITY",
        format!("{reason} requires {capability}"),
    ))
}
```

Then validate:

- commands -> `commands.execute`
- providers -> `provider.extensionValues`
- gateway_hooks -> `gateway.hooks`
- protocol_bridges -> `protocol.bridge`
- UI slot `providers.editor.sections` -> `provider.extensionValues`
- button fields -> `commands.execute`

Implement a small helper that detects `HostRenderedField::Button`.

- [ ] **Step 6: Enforce Extension Host-only manifest rules**

In `validate_manifest`:

- For `ExtensionHost`, reject non-empty `hooks` with `PLUGIN_INVALID_MANIFEST`.
- For `ExtensionHost`, reject non-empty `permissions` with `PLUGIN_INVALID_MANIFEST`.
- Require `main`.
- Accept `language == "typescript"` and optionally `"javascript"` if Task 1 did.
- Validate activation, contributes, capabilities, and capability dependencies.
- For `Native`, return `PLUGIN_UNSUPPORTED_RUNTIME` unless the official privacy filter install path has a dedicated bypass. If the current validator is used for official plugin manifests too, add an explicit `validate_manifest_for_official_plugin` helper rather than weakening community validation.

- [ ] **Step 7: Run Rust domain verification**

Run:

```bash
cd src-tauri && cargo test domain::plugins::tests:: domain::plugin_contributions --lib
```

Expected: pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/domain/plugins.rs src-tauri/src/domain/plugin_contributions.rs src-tauri/src/app/plugins/contribution_registry.rs
git commit -m "feat(plugins): enforce extension host manifest contract"
```

## Task 3: Remove declarativeRules Runtime Dispatch and Keep Official Privacy Filter

**Files:**
- Modify: `src-tauri/src/app/plugins/mod.rs`
- Modify: `src-tauri/src/app/plugins/runtime_manager.rs`
- Modify: `src-tauri/src/app/plugins/runtime_executor.rs`
- Delete: `src-tauri/src/app/plugins/rule_runtime.rs`
- Modify tests in `src-tauri/src/gateway/routes.rs`, `src-tauri/src/gateway/plugins/pipeline.rs`, `src-tauri/src/gateway/streams/plugin_chunk.rs`, `src-tauri/src/app/plugins/official.rs`, and `src-tauri/src/app/plugin_service.rs` where they create declarative rule plugin fixtures.

- [ ] **Step 1: Write failing runtime manager tests**

In `src-tauri/src/app/plugins/runtime_manager.rs`, replace `runtime_manager_allows_declarative_rules_policy` with:

```rust
#[test]
fn runtime_manager_rejects_non_extension_host_community_runtime() {
    let manager = PluginRuntimeManager::for_tests(RuntimePolicy::default());
    let runtime = PluginRuntime::Native {
        engine: "privacyFilter".to_string(),
    };

    let err = manager
        .runtime_dispatch("example.privacy-filter", &runtime)
        .expect_err("community native runtime should be rejected");

    assert_eq!(err.code(), "PLUGIN_UNSUPPORTED_RUNTIME");
}
```

Keep the test that official `official.privacy-filter` resolves to `NativePrivacyFilter`.

- [ ] **Step 2: Run targeted runtime tests and verify failure**

Run:

```bash
cd src-tauri && cargo test app::plugins::runtime_manager::tests:: app::plugins::runtime_executor::tests:: --lib
```

Expected: fail while runtime executor still references rule runtime.

- [ ] **Step 3: Remove rule runtime module and dispatch**

In `src-tauri/src/app/plugins/mod.rs`, remove:

```rust
pub(crate) mod rule_runtime;
```

In `runtime_manager.rs`:

- Remove `RuntimeDispatch::DeclarativeRules`.
- Remove `WasmNotWired` if `PluginRuntime::Wasm` was removed.
- Keep `RuntimeDispatch::ExtensionHost`.
- Keep `RuntimeDispatch::NativePrivacyFilter`.

In `runtime_executor.rs`:

- Remove `rule_runtime` field.
- Remove rule cache registration.
- Remove `RuntimeDispatch::DeclarativeRules` branch.
- For `RuntimeDispatch::ExtensionHost`, keep a temporary error `PLUGIN_EXTENSION_HOST_GATEWAY_NOT_WIRED` until Task 8 wires gateway hooks.
- Keep official privacy filter branch unchanged.

- [ ] **Step 4: Delete `rule_runtime.rs`**

Delete the file:

```bash
git rm src-tauri/src/app/plugins/rule_runtime.rs
```

Do not delete:

- `src-tauri/src/gateway/proxy/upstream_client_error_rules.rs`
- official privacy filter runtime files
- protocol bridge built-in files

- [ ] **Step 5: Update old rule fixtures in Rust tests**

For tests that only assert pipeline plumbing, convert fixture plugin manifests to Extension Host manifests with `contributes.gatewayHooks` and `capabilities: ["gateway.hooks"]`.

For tests that assert declarative rule replacement behavior, mark them for migration in Task 8 rather than keeping them as rule runtime tests.

- [ ] **Step 6: Run runtime verification**

Run:

```bash
cd src-tauri && cargo test app::plugins::runtime_manager::tests:: app::plugins::runtime_executor::tests:: app::plugins::official::tests:: --lib
```

Expected: pass and official privacy filter tests still pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/app/plugins/mod.rs src-tauri/src/app/plugins/runtime_manager.rs src-tauri/src/app/plugins/runtime_executor.rs src-tauri/src/app/plugins/official.rs src-tauri/src/app/plugin_service.rs src-tauri/src/gateway/routes.rs src-tauri/src/gateway/plugins/pipeline.rs src-tauri/src/gateway/streams/plugin_chunk.rs
git add -u src-tauri/src/app/plugins/rule_runtime.rs
git commit -m "refactor(plugins): remove declarative rule runtime dispatch"
```

## Task 4: Install/Preview Compatibility for Unsupported Legacy Plugins

**Files:**
- Modify: `src-tauri/src/app/plugin_service.rs`
- Modify: `src-tauri/src/infra/plugins/package.rs` if package preview decodes manifest directly.
- Modify: `src/services/__tests__/plugins.test.ts`
- Modify: `src/pages/plugins/pluginProductCopy.ts`
- Modify: `src/pages/__tests__/PluginsPage.test.tsx` if runtime/status copy is asserted.

- [ ] **Step 1: Write failing service tests**

In `src-tauri/src/app/plugin_service.rs` tests, add coverage:

- previewing a package with `runtime.kind = "declarativeRules"` returns `PLUGIN_UNSUPPORTED_RUNTIME`.
- installing a package with `runtime.kind = "declarativeRules"` returns `PLUGIN_UNSUPPORTED_RUNTIME`.
- listing an existing DB plugin row with legacy runtime normalizes summary to disabled or incompatible and never returns it as enabled for gateway.

Use existing local package test helpers in this file.

- [ ] **Step 2: Run targeted service tests and verify failure**

Run:

```bash
cd src-tauri && cargo test app::plugin_service::tests:: --lib
```

Expected: fail because legacy package handling is not normalized.

- [ ] **Step 3: Convert decode failures for legacy runtime to unsupported runtime**

Where plugin package manifests are read:

- Detect JSON `runtime.kind` before typed deserialize if necessary.
- If `kind` is `"declarativeRules"`, `"wasm"`, or `"process"`, return `PLUGIN_UNSUPPORTED_RUNTIME`.
- If `kind` is `"native"` and plugin id is not official privacy filter install path, return `PLUGIN_UNSUPPORTED_RUNTIME`.

Do not let raw serde unknown variant errors become confusing generic decode failures for legacy pre-release plugins.

- [ ] **Step 4: Disable unsupported local DB legacy plugins on read**

Add a normalization helper in `plugin_service.rs`:

```rust
fn normalize_unsupported_legacy_plugin_detail(mut detail: PluginDetail) -> PluginDetail {
    if is_unsupported_legacy_runtime_summary(&detail.summary.runtime) {
        detail.summary.status = PluginStatus::Disabled;
        detail.summary.last_error = Some(
            "Unsupported pre-release plugin runtime; reinstall an Extension Host version".to_string(),
        );
    }
    detail
}
```

Use it in list/get paths before returning frontend DTOs and in `enabled_plugins_for_gateway` so unsupported legacy plugins never execute.

- [ ] **Step 5: Update product copy**

In `src/pages/plugins/pluginProductCopy.ts`:

- Remove `declarativeRules` and `wasm` as normal runtime descriptions.
- Add copy for `extensionHost`.
- For unknown/unsupported runtime, show "不支持的旧插件运行时".
- Keep `native:privacyFilter` copy.

- [ ] **Step 6: Run verification**

Run:

```bash
cd src-tauri && cargo test app::plugin_service::tests:: --lib
pnpm test:unit -- src/services/__tests__/plugins.test.ts src/pages/__tests__/PluginsPage.test.tsx src/pages/plugins/__tests__/pluginProductCopy.test.ts
```

Expected: pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/app/plugin_service.rs src-tauri/src/infra/plugins/package.rs src/pages/plugins/pluginProductCopy.ts src/services/__tests__/plugins.test.ts src/pages/__tests__/PluginsPage.test.tsx src/pages/plugins/__tests__/pluginProductCopy.test.ts
git commit -m "feat(plugins): reject unsupported legacy plugin packages"
```

## Task 5: Capability Enforcement in Host API and Worker Injection

**Files:**
- Modify: `src-tauri/src/app/plugins/extension_host.rs`
- Modify: `src-tauri/src/app/plugins/extension_host_worker.rs`
- Modify: `src-tauri/src/domain/plugins.rs` if helper visibility is needed.

- [ ] **Step 1: Write failing Extension Host API tests**

In `src-tauri/src/app/plugins/extension_host.rs` tests, add tests:

- plugin with `storage.plugin` can call `api.storage.set/get`.
- plugin without `storage.plugin` gets `PLUGIN_EXTENSION_HOST_FORBIDDEN`.
- plugin with `diagnostics.read` can call `api.diagnostics.getRuntimeReports`.
- plugin without `diagnostics.read` gets `PLUGIN_EXTENSION_HOST_FORBIDDEN`.
- plugin with command contribution but missing `commands.execute` fails before command execution.

Use test extension source such as:

```js
module.exports.activate = function(api) {
  api.commands.registerCommand("acme.echo", function() {
    api.storage.set("key", { ok: true });
    return api.storage.get("key");
  });
};
```

- [ ] **Step 2: Run targeted tests and verify failure**

Run:

```bash
cd src-tauri && cargo test app::plugins::extension_host::tests:: --lib
```

Expected: storage/diagnostics calls succeed without capabilities today, so the negative tests fail.

- [ ] **Step 3: Add capability-aware Host API handler**

In `extension_host.rs`, change `ExtensionHostApiHandler`:

```rust
struct ExtensionHostApiHandler {
    db: db::Db,
    plugin_id: String,
    capabilities: std::collections::BTreeSet<String>,
}
```

When starting with host API, populate it from `manifest.capabilities`.

Add:

```rust
fn require_capability(&self, capability: &str) -> AppResult<()> {
    if self.capabilities.contains(capability) {
        return Ok(());
    }
    Err(AppError::new(
        "PLUGIN_EXTENSION_HOST_FORBIDDEN",
        format!("extension host API requires {capability}"),
    ))
}
```

Call it before:

- `storage.get` and `storage.set` -> `storage.plugin`
- `diagnostics.getRuntimeReports` -> `diagnostics.read`

- [ ] **Step 4: Add worker-side API pruning**

In `extension_host_worker.rs`:

- Pass manifest capabilities into activation API construction.
- Only set `api.storage` when `storage.plugin` is present.
- Only set `api.diagnostics` when `diagnostics.read` is present.
- Only set `api.commands` when `commands.execute` is present.

Keep Rust Host API enforcement even after pruning.

- [ ] **Step 5: Run verification**

Run:

```bash
cd src-tauri && cargo test app::plugins::extension_host::tests:: domain::plugins::tests:: --lib
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/app/plugins/extension_host.rs src-tauri/src/app/plugins/extension_host_worker.rs src-tauri/src/domain/plugins.rs
git commit -m "feat(plugins): enforce extension host capabilities"
```

## Task 6: Managed Extension Host Instance Registry

**Files:**
- Create: `src-tauri/src/app/plugins/extension_host_registry.rs`
- Modify: `src-tauri/src/app/plugins/mod.rs`
- Modify: `src-tauri/src/app/plugins/runtime_lifecycle.rs`
- Modify: `src-tauri/src/app/plugin_registry.rs`
- Modify: `src-tauri/src/app/cleanup.rs`

- [ ] **Step 1: Write failing registry tests**

Create `src-tauri/src/app/plugins/extension_host_registry.rs` with tests first.

Test cases:

- `registry_reuses_warm_instance_for_same_key`
- `registry_replaces_instance_when_contribution_hash_changes`
- `registry_disposes_plugin_instances`
- `registry_disposes_idle_instances`
- `registry_evicts_least_recently_used_idle_instance`

Use dependency injection to avoid spawning real processes for pure registry tests. Define a test-only fake instance factory:

```rust
#[cfg(test)]
trait ExtensionHostInstanceFactory: Send + Sync {
    fn start(&self, key: ExtensionHostInstanceKey) -> usize;
}
```

If the production code uses async start, make the test fake async too.

- [ ] **Step 2: Run registry tests and verify failure**

Run:

```bash
cd src-tauri && cargo test app::plugins::extension_host_registry::tests:: --lib
```

Expected: fail or module missing.

- [ ] **Step 3: Implement registry types**

Create:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ExtensionHostInstanceKey {
    pub(crate) plugin_id: String,
    pub(crate) version: String,
    pub(crate) installed_dir: String,
    pub(crate) main: String,
    pub(crate) runtime_kind: String,
    pub(crate) runtime_language: String,
    pub(crate) contribution_hash: String,
}
```

Add:

```rust
pub(crate) struct ExtensionHostInstanceRegistry {
    db: db::Db,
    instances: tokio::sync::Mutex<BTreeMap<ExtensionHostInstanceKey, ManagedExtensionHostInstance>>,
    limits: ExtensionHostRegistryLimits,
}
```

Use:

- per-plugin execution lock by holding the instance mutex while executing.
- global warm instance limit default 8.
- idle recycle default 120 seconds.
- LRU idle eviction when limit exceeded.

- [ ] **Step 4: Add production execute_command**

Add:

```rust
pub(crate) async fn execute_command(
    &self,
    detail: PluginDetail,
    command: &str,
    args: serde_json::Value,
) -> AppResult<ExtensionHostCommandOutput>
```

`ExtensionHostCommandOutput` should include:

```rust
pub(crate) struct ExtensionHostCommandOutput {
    pub(crate) value: serde_json::Value,
    pub(crate) cold_start: bool,
}
```

Start with:

```rust
ExtensionHostInstance::start_with_host_api(detail.manifest.clone(), plugin_root, self.db.clone())
```

Reuse when key matches and instance is running.

- [ ] **Step 5: Make lifecycle disposal async-safe**

`PluginRuntimeInstanceRegistry` is currently sync. Since Extension Host dispose is async, use one of these simple approaches:

- Preferred: change lifecycle trait methods to async by returning boxed futures.
- Acceptable: keep `ExtensionHostInstanceRegistry` outside the old trait and call it directly from Tauri state/cleanup for this phase.

Choose the smaller implementation that still covers disable/uninstall/update/app shutdown. Do not block forever inside sync dispose.

- [ ] **Step 6: Register managed state**

In `plugin_registry.rs`, add:

```rust
.manage(crate::app::plugins::extension_host_registry::ExtensionHostRuntimeState::default())
```

Use a state wrapper that lazily binds DB on first command execution if DB is only available after `ensure_db_ready`.

In `cleanup.rs`, call `dispose_all` during `cleanup_before_exit`.

- [ ] **Step 7: Run verification**

Run:

```bash
cd src-tauri && cargo test app::plugins::extension_host_registry::tests:: app::plugins::runtime_lifecycle::tests:: --lib
```

Expected: pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/app/plugins/extension_host_registry.rs src-tauri/src/app/plugins/mod.rs src-tauri/src/app/plugins/runtime_lifecycle.rs src-tauri/src/app/plugin_registry.rs src-tauri/src/app/cleanup.rs
git commit -m "feat(plugins): add extension host instance registry"
```

## Task 7: Command Execution Uses Registry

**Files:**
- Modify: `src-tauri/src/app/plugin_service.rs`
- Modify: `src-tauri/src/commands/plugins.rs`
- Modify: `src-tauri/src/app/plugins/extension_host_registry.rs`
- Modify: `src-tauri/src/infra/plugins/runtime_reports.rs` if adding `coldStart` to budget summaries.
- Regenerate: `src/generated/bindings.ts` if command signatures change.

- [ ] **Step 1: Write failing command reuse test**

In `src-tauri/src/app/plugin_service.rs` tests or `extension_host_registry.rs` tests, add an integration-style test that executes the same command twice and asserts the registry start count is 1.

If using plugin_service is hard because of Tauri state, test the registry directly and add a service test that verifies `plugin_service::execute_plugin_command` accepts a registry reference.

- [ ] **Step 2: Run targeted tests and verify failure**

Run:

```bash
cd src-tauri && cargo test app::plugin_service::tests::plugin_command app::plugins::extension_host_registry::tests:: --lib
```

Expected: fail because `execute_extension_host_command_once` still cold-starts.

- [ ] **Step 3: Change service signature**

Change:

```rust
pub(crate) async fn execute_plugin_command(
    db: &crate::db::Db,
    command: &str,
    args: serde_json::Value,
) -> AppResult<serde_json::Value>
```

to:

```rust
pub(crate) async fn execute_plugin_command(
    db: &crate::db::Db,
    registry: &ExtensionHostInstanceRegistry,
    command: &str,
    args: serde_json::Value,
) -> AppResult<serde_json::Value>
```

Then replace `execute_extension_host_command_once` with:

```rust
let result = registry
    .execute_command(detail.clone(), &command, args.clone())
    .await;
```

Delete `execute_extension_host_command_once`.

- [ ] **Step 4: Update Tauri command**

In `src-tauri/src/commands/plugins.rs`, add state:

```rust
registry_state: tauri::State<'_, ExtensionHostRuntimeState>,
```

Resolve the registry after DB is ready and pass it into `plugin_service::execute_plugin_command`.

- [ ] **Step 5: Dispose registry on plugin lifecycle operations**

In `commands/plugins.rs` or `plugin_service.rs`, after successful:

- `plugin_disable`
- `plugin_uninstall`
- `plugin_update_from_file`
- `plugin_rollback`
- `plugin_quarantine_revoked` if status changes away from enabled

call registry dispose for that plugin id.

If this requires adding registry state to more commands, do so narrowly.

- [ ] **Step 6: Record cold start metadata**

When recording command execution report, include:

```json
{ "coldStart": true }
```

in input or mutation summary budget. Prefer `input_budget` for the command execution report if changing schema is unnecessary.

- [ ] **Step 7: Regenerate bindings if needed**

If command signatures or Specta types changed:

```bash
pnpm tauri:gen-types
```

Expected: `src/generated/bindings.ts` updates consistently.

- [ ] **Step 8: Run verification**

Run:

```bash
cd src-tauri && cargo test app::plugin_service::tests::plugin_command app::plugins::extension_host_registry::tests:: --lib
pnpm test:unit -- src/generated/__tests__/bindings.contract.test.ts src/query/__tests__/plugins.test.tsx
```

Expected: pass.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/app/plugin_service.rs src-tauri/src/commands/plugins.rs src-tauri/src/app/plugins/extension_host_registry.rs src-tauri/src/infra/plugins/runtime_reports.rs src/generated/bindings.ts src/query/__tests__/plugins.test.tsx
git commit -m "feat(plugins): execute commands through extension host registry"
```

## Task 8: Extension Host Gateway Hooks MVP

**Files:**
- Modify: `src-tauri/src/app/plugins/extension_host.rs`
- Modify: `src-tauri/src/app/plugins/extension_host_worker.rs`
- Modify: `src-tauri/src/app/plugins/extension_host_registry.rs`
- Modify: `src-tauri/src/app/plugins/runtime_executor.rs`
- Modify: `src-tauri/src/gateway/plugins/context.rs`
- Modify: `src-tauri/src/gateway/plugins/pipeline.rs`
- Modify: `src-tauri/src/gateway/plugins/mutation.rs`
- Modify tests in `src-tauri/src/gateway/plugins/pipeline.rs`

- [ ] **Step 1: Write failing gateway hook tests**

In `src-tauri/src/gateway/plugins/pipeline.rs` tests or `runtime_executor.rs` tests, add tests for Extension Host plugin fixtures:

- request hook returns continue and leaves body unchanged.
- request hook returns replace and changes body.
- response hook returns warn and records audit event.
- unsupported action for hook is rejected.
- timeout records failure and pipeline fail-open keeps original body.

Use a fake Extension Host gateway executor if real process startup makes pipeline tests slow. The production executor still must call registry.

- [ ] **Step 2: Run targeted tests and verify failure**

Run:

```bash
cd src-tauri && cargo test gateway::plugins::pipeline::tests:: app::plugins::runtime_executor::tests:: --lib
```

Expected: fail because Extension Host runtime still returns not wired for gateway execution.

- [ ] **Step 3: Add worker method for gateway hooks**

In `extension_host_worker.rs`, add JSON-RPC method:

```rust
"gatewayHooks.execute" => {
    let hook = required param string;
    let context = request.params.get("context").cloned().unwrap_or(Value::Null);
    self.execute_gateway_hook(hook, context)
}
```

Expose JS API:

```js
api.gateway.registerHook(name, handler)
```

Require hook name to be declared in `contributes.gatewayHooks`.

Execute handler and require JSON-serializable result.

- [ ] **Step 4: Add host instance method**

In `extension_host.rs`, add:

```rust
pub(crate) async fn execute_gateway_hook(
    &mut self,
    hook: &str,
    context: Value,
) -> AppResult<Value>
```

Use a 150 ms timeout. If current instance call timeout is fixed at start, registry should start gateway-use instances with the gateway timeout or call method should accept per-call timeout. Prefer the minimal change that preserves command timeout at 10 seconds and gateway hook timeout at 150 ms.

- [ ] **Step 5: Add registry gateway hook method**

In `extension_host_registry.rs`, add:

```rust
pub(crate) async fn execute_gateway_hook(
    &self,
    detail: PluginDetail,
    hook: &str,
    context: GatewayVisibleHookContext,
) -> Result<GatewayHookResult, GatewayPluginError>
```

Translate host errors to `GatewayPluginError`.

- [ ] **Step 6: Wire runtime executor**

In `runtime_executor.rs`, for `RuntimeDispatch::ExtensionHost`:

- Check capability `gateway.hooks`.
- Dispatch to registry gateway hook execution.
- Keep official privacy filter path unchanged.

If `RuntimeGatewayPluginExecutor` currently has no DB/registry, update construction in `gateway/control_service.rs` so gateway pipeline gets an executor with access to app-managed registry and DB.

- [ ] **Step 7: Map Extension Host gateway result**

Support these result actions:

- `continue`
- `warn`
- `block`
- `replace`
- `appendMessage`

Map legacy SDK result aliases if currently named `pass` to `continue` for one transition only, or update SDK and examples to use `continue`.

Reject:

- response body replacement from stream hook if not allowed.
- stream chunk replacement from non-stream hook.
- block where hook contract disallows blocking.

Reuse existing mutation permission/budget enforcement where possible.

- [ ] **Step 8: Record reports**

Ensure each Extension Host gateway hook execution records report data with:

- runtime kind `extensionHost`
- hook name
- status
- error code
- input/output budget
- mutation summary
- replayable flag

Avoid unbounded payload storage.

- [ ] **Step 9: Run verification**

Run:

```bash
cd src-tauri && cargo test app::plugins::runtime_executor::tests:: gateway::plugins::pipeline::tests:: app::plugins::extension_host::tests:: --lib
```

Expected: pass.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/app/plugins/extension_host.rs src-tauri/src/app/plugins/extension_host_worker.rs src-tauri/src/app/plugins/extension_host_registry.rs src-tauri/src/app/plugins/runtime_executor.rs src-tauri/src/gateway/plugins/context.rs src-tauri/src/gateway/plugins/pipeline.rs src-tauri/src/gateway/plugins/mutation.rs
git commit -m "feat(plugins): wire extension host gateway hooks"
```

## Task 9: Protocol Bridge Manifest and Minimal Dispatch Skeleton

**Files:**
- Modify: `src-tauri/src/app/plugins/contribution_registry.rs`
- Create: `src-tauri/src/app/plugins/extension_protocol_bridge.rs`
- Modify: `src-tauri/src/app/plugins/mod.rs`
- Modify: `packages/plugin-sdk/src/index.ts`
- Modify tests in `packages/plugin-sdk/src/index.test.ts` and `src-tauri/src/app/plugins/contribution_registry.rs`

- [ ] **Step 1: Write failing protocol bridge tests**

Add tests that assert:

- protocol bridge contribution requires `protocol.bridge`.
- active contribution snapshot includes namespaced protocol bridge.
- non-namespaced bridge id is rejected.
- minimal dispatch skeleton returns `PLUGIN_EXTENSION_PROTOCOL_BRIDGE_NOT_IMPLEMENTED` for unimplemented bridge execution rather than silently ignoring.

- [ ] **Step 2: Run tests and verify failure**

Run:

```bash
pnpm --filter @aio-coding-hub/plugin-sdk test -- index.test.ts
cd src-tauri && cargo test app::plugins::contribution_registry::contribution_registry_tests:: --lib
```

Expected: fail for new skeleton behavior if not implemented.

- [ ] **Step 3: Add protocol bridge extension module**

Create `extension_protocol_bridge.rs` with:

```rust
pub(crate) struct ExtensionProtocolBridgeRegistry;

impl ExtensionProtocolBridgeRegistry {
    pub(crate) fn contribution_id(plugin_id: &str, bridge_type: &str) -> String {
        format!("{plugin_id}:{bridge_type}")
    }
}
```

Add a minimal async dispatch function that returns:

```text
PLUGIN_EXTENSION_PROTOCOL_BRIDGE_NOT_IMPLEMENTED
```

until a future task wires full request/response translation.

- [ ] **Step 4: Keep built-in protocol bridge separate**

Do not modify `src-tauri/src/gateway/proxy/protocol_bridge/registry.rs` to register plugin bridges as built-ins. Plugin bridges must remain host-dispatched Extension Host contributions.

- [ ] **Step 5: Run verification**

Run:

```bash
pnpm --filter @aio-coding-hub/plugin-sdk test
cd src-tauri && cargo test app::plugins::contribution_registry::contribution_registry_tests:: --lib
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/app/plugins/extension_protocol_bridge.rs src-tauri/src/app/plugins/mod.rs src-tauri/src/app/plugins/contribution_registry.rs packages/plugin-sdk/src/index.ts packages/plugin-sdk/src/index.test.ts
git commit -m "feat(plugins): define extension protocol bridge skeleton"
```

## Task 10: Scaffolding and Devtools Migration

**Files:**
- Modify: `packages/create-aio-plugin/src/scaffold.ts`
- Modify: `packages/create-aio-plugin/src/scaffold.test.ts`
- Modify: `packages/create-aio-plugin/src/devtools.ts`

- [ ] **Step 1: Write failing scaffold tests**

Update `scaffold.test.ts` expectations:

- default scaffold contains `"kind": "extensionHost"`.
- default scaffold contains `"main": "dist/extension.js"`.
- generated extension source registers a command.
- generated provider template uses `contributes.providers` and `providers.editor.sections`.
- generated gateway hook template uses `contributes.gatewayHooks` and `gateway.hooks`.
- no generated files contain `"declarativeRules"`.

- [ ] **Step 2: Run create-aio-plugin tests and verify failure**

Run:

```bash
pnpm --filter create-aio-plugin test -- scaffold.test.ts
```

Expected: fail because scaffolds still generate rule templates.

- [ ] **Step 3: Replace templates**

In `scaffold.ts`:

- Replace `ruleTemplate` default with Extension Host command template.
- Replace prompt-helper/redactor/response-guard rule examples with Extension Host gateway hook examples.
- Add `dist/extension.js` source in generated files.
- Keep fixtures, but make replay fixture shape target Extension Host gateway hook replay.

Default command template extension:

```js
module.exports.activate = function(api) {
  api.commands.registerCommand("publisher.plugin-name.hello", function(args) {
    return { ok: true, args };
  });
};
```

Gateway hook template extension:

```js
module.exports.activate = function(api) {
  api.gateway.registerHook("gateway.request.beforeSend", function(context) {
    return { action: "continue" };
  });
};
```

- [ ] **Step 4: Remove declarative rule devtools commands**

In `devtools.ts`:

- Remove rule-only replay/explain implementation.
- Replace help text `rule|wasm` with Extension Host template names.
- `doctor` should report unsupported legacy manifests with clear diagnostic.
- `validate` should delegate to SDK and reject `declarativeRules`.
- `pack` and `publish-check` should require Extension Host package shape.

- [ ] **Step 5: Run verification**

Run:

```bash
pnpm --filter create-aio-plugin test
pnpm --filter create-aio-plugin typecheck
```

Expected: pass.

- [ ] **Step 6: Commit**

```bash
git add packages/create-aio-plugin/src/scaffold.ts packages/create-aio-plugin/src/scaffold.test.ts packages/create-aio-plugin/src/devtools.ts
git commit -m "feat(plugins): scaffold extension host plugins"
```

## Task 11: Docs, Contract JSON, and Product Copy Cleanup

**Files:**
- Modify: `docs/plugin-manifest-v1.md`
- Modify: `docs/plugins/plugin-api-v1-contract.json`
- Modify: `docs/plugins/**`
- Modify: `src/pages/plugins/pluginProductCopy.ts`
- Modify: docs checker scripts only if their expected phrases are now wrong.

- [ ] **Step 1: Write failing grep/check expectations**

Run:

```bash
rg -n "declarativeRules|gatewayRules|rule runtime|规则插件" docs packages src src-tauri \
  -g '!docs/superpowers/**' \
  -g '!target/**'
```

Expected before cleanup: many hits.

Allowed hits after cleanup:

- migration/unsupported copy that explicitly says old pre-release runtime is unsupported.
- this implementation plan and superseded Superpowers docs.
- internal non-plugin rule files such as upstream client error rules.

- [ ] **Step 2: Update public manifest docs**

In `docs/plugin-manifest-v1.md`:

- Make Extension Host the only community runtime.
- Remove declarative rule examples.
- Replace hooks/permissions examples with `contributes.gatewayHooks` and capabilities.
- Add capability dependency table.
- Add lifecycle and UI boundary language.

- [ ] **Step 3: Update contract JSON**

In `docs/plugins/plugin-api-v1-contract.json`:

- `communityRuntimes` should be `["extensionHost"]`.
- Remove `declarativeRules` runtime contract.
- Remove `gatewayRules`.
- Add or update capability dependency metadata.
- Keep official privacy filter separately marked host-owned.

- [ ] **Step 4: Update plugin docs/examples**

Update `docs/plugins/**` so:

- examples are Extension Host TypeScript.
- gateway examples use `api.gateway.registerHook`.
- provider examples use host-rendered UI schema.
- protocol bridge docs clearly say MVP skeleton exists and full bridge execution is future work if not completed in Task 9.

- [ ] **Step 5: Update product copy**

Ensure UI copy presents:

- Extension Host plugin
- Built-in privacy filter
- Unsupported old plugin runtime

Do not present WASM/process/native as user choices.

- [ ] **Step 6: Run docs/contract verification**

Run:

```bash
pnpm check:plugin-api-contract
pnpm check:plugin-system-docs
pnpm check:plugin-system-completion
rg -n "declarativeRules|gatewayRules" docs packages src src-tauri \
  -g '!docs/superpowers/**' \
  -g '!target/**'
```

Expected: checks pass. Any remaining grep hits are explicitly reviewed and documented as allowed internal/unsupported mentions.

- [ ] **Step 7: Commit**

```bash
git add docs/plugin-manifest-v1.md docs/plugins src/pages/plugins/pluginProductCopy.ts scripts/check-plugin-api-contract.mjs scripts/check-plugin-system-docs.mjs scripts/check-plugin-system-completion.mjs
git commit -m "docs(plugins): document extension host only plugin api"
```

## Task 12: Final Verification and Cleanup

**Files:**
- Modify only files required by failures discovered during verification.

- [ ] **Step 1: Run full plugin TypeScript verification**

Run:

```bash
pnpm --filter @aio-coding-hub/plugin-sdk test
pnpm --filter @aio-coding-hub/plugin-sdk typecheck
pnpm --filter create-aio-plugin test
pnpm --filter create-aio-plugin typecheck
```

Expected: all pass.

- [ ] **Step 2: Run frontend verification**

Run:

```bash
pnpm test:unit -- src/plugins/contributions src/pages/plugins src/services/__tests__/plugins.test.ts src/query/__tests__/plugins.test.tsx
pnpm typecheck
```

Expected: all pass.

- [ ] **Step 3: Run Rust verification**

Run:

```bash
cd src-tauri && cargo test --lib
cd src-tauri && cargo check --locked
```

Expected: all pass.

- [ ] **Step 4: Run plugin docs/contract checks**

Run:

```bash
pnpm check:plugin-api-contract
pnpm check:plugin-system-docs
pnpm check:plugin-system-completion
```

Expected: all pass.

- [ ] **Step 5: Search for disallowed legacy public API**

Run:

```bash
rg -n "declarativeRules|gatewayRules|RuleRuntimeGatewayPluginExecutor|rule_runtime" \
  packages src src-tauri docs \
  -g '!docs/superpowers/**' \
  -g '!target/**'
```

Expected: no public API/tooling/runtime hits. Allowed hits must be internal non-plugin rules or explicit unsupported migration copy.

- [ ] **Step 6: Review diff**

Run:

```bash
git diff --stat origin/main...HEAD
git diff --name-status origin/main...HEAD
```

Expected: changes match this plan. No unrelated files should be modified.

- [ ] **Step 7: Commit final fixes if any**

If Step 1-6 required cleanup:

```bash
git add <changed-files>
git commit -m "chore(plugins): finalize extension host only migration"
```

If no cleanup is needed, do not create an empty commit.

## Execution Notes for Subagents

- Tasks 1 and 2 should run first because they define the contract.
- Task 3 depends on Task 2.
- Task 4 depends on Task 2 and Task 3.
- Task 5 can start after Task 2.
- Task 6 can start after Task 5.
- Task 7 depends on Task 6.
- Task 8 depends on Task 6 and Task 7.
- Task 9 can run after Task 2, but should not modify gateway built-in bridge registry.
- Task 10 depends on Task 1 and Task 8 if gateway hook templates are expected to replay.
- Task 11 should run after Tasks 1-10.
- Task 12 runs last.

## Risk Checklist

- Official privacy filter must still install, enable, and run.
- Existing app UI must not expose browser-like plugin execution.
- Gateway hook timeouts must stay short.
- Extension Host command timeout must not accidentally become 150 ms.
- Registry dispose must happen on app exit and plugin disable/uninstall/update.
- Capability enforcement must live in Rust Host API, not only SDK.
- Deleting `rule_runtime.rs` must not delete unrelated internal rules.

