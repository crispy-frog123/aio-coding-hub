# aio-coding-hub Plugin Extension Host Platform Design

日期：2026-06-27

## Summary

当前分支尚未发布，因此插件平台可以丢弃旧的 gateway-rule-first 设计，改成 Extension Host first。新的插件目标不是“让插件改请求体”，而是让插件在宿主明确授权和生命周期管理下扩展 AIO 的 UI、Provider、协议转译、Gateway、命令、诊断和开发工具。

核心结论：

- `declarativeRules` 不再是插件生态主线，只能作为 legacy gateway rule 能力保留或迁移。
- 新主线是 TypeScript Extension Host：插件作者写 TypeScript，插件运行在宿主管理的独立 extension host process 中，通过 RPC 访问 host-mediated APIs。
- UI 扩展不直接执行第三方 React 组件。第一阶段采用 host-rendered UI schema，插件贡献字段、区块、按钮、菜单、详情 tab 和页面入口，宿主用自己的 React 组件渲染。
- Provider 和协议转译是核心上限。平台必须能支持 OpenRouter Provider、OpenAI/Claude/Gemini bridge、request debug panel、settings extension 这类真实插件。
- Runtime Lifecycle Registry 不是独立目标，而是 Extension Host 的生命周期子系统，负责启动、激活、失活、销毁、失败熔断、资源回收和旧 contribution 驱逐。

一句话目标：AIO 插件应该成为可控的桌面应用扩展平台，而不是单一 gateway hook 机制。

## Product Ceiling

未来插件允许扩展这些宿主能力：

1. UI contribution：给多个宿主页面添加区块、字段、按钮、菜单、详情 tab、卡片和页面入口。
2. Provider extension：新增 provider 类型、provider 配置字段、校验、鉴权、请求准备、模型映射、健康检查。
3. Protocol bridge extension：贡献 OpenAI、Claude、Gemini、OpenAI Responses 或自定义协议之间的转译能力。
4. Gateway extension：扩展请求、响应、stream、错误、日志、路由和 failover 策略。
5. Command/workflow extension：注册命令、按钮动作、诊断动作、导出动作、后台任务。
6. Diagnostics/devtools extension：访问 runtime reports、trace replay、bridge replay、plugin diagnostics 和 publish helpers。
7. Plugin storage/config：保存插件配置、provider 扩展值、运行状态和有界缓存。

插件不允许：

- 直接修改宿主源码或 monkey patch React 组件。
- 直接控制 Tauri WebView。
- 直接访问 SQLite connection、内部 Rust references 或 provider secrets。
- 绕过宿主权限、生命周期、兼容性、启禁用、隔离和卸载逻辑。
- 直接加载第三方 native code 到 Rust 主进程。
- 在未授权时访问文件、网络、secret、系统命令或敏感 request payload。

## Target Plugin Archetypes

后续设计和验收都必须能支撑这些插件。若支撑不了，说明平台上限不足。

### 1. OpenRouter Provider Plugin

能力：

- 在 Provider 页面新增 OpenRouter provider 类型。
- 在 provider editor 添加 OpenRouter 专属字段，例如 route、provider sorting、model fallback、HTTP headers policy。
- 保存字段到 provider extension storage。
- 在 gateway 请求准备阶段读取这些字段。
- 做模型映射、错误映射、健康检查和可用模型发现。

需要的扩展点：

- `contributes.providers`
- `contributes.ui.providers.editor.sections`
- `contributes.ui.providers.card.badges`
- provider extension storage
- provider request preparation API
- model mapper API
- provider availability API

### 2. Claude/OpenAI/Gemini Bridge Plugin

能力：

- 声明一个 bridge type。
- 把 source protocol 转为 AIO IR，再转为 target protocol。
- 支持非流式和流式响应。
- 记录 bridge trace，可 replay。
- 失败时输出稳定错误码和诊断。

需要的扩展点：

- `contributes.protocols`
- `contributes.protocolBridges`
- `contributes.modelMappers`
- `contributes.providerAdapters`
- protocol bridge registry
- bridge trace/replay API

### 3. Request Debug Panel Plugin

能力：

- 在 request log detail 中添加一个 tab。
- 展示该 trace 经过哪些插件、provider、bridge、gateway hooks。
- 提供导出诊断包、复制 replay fixture、运行 bridge replay 等命令。

需要的扩展点：

- `contributes.ui.logs.detail.tabs`
- `contributes.commands`
- trace/read-only diagnostics API
- runtime reports API
- replay fixture export API

### 4. Settings Extension Plugin

能力：

- 在 Settings 页面添加插件配置区块。
- 使用 host-rendered controls。
- 保存插件配置。
- 禁用插件后 UI 立即消失，配置保留但不生效。

需要的扩展点：

- `contributes.ui.settings.sections`
- plugin config storage
- contribution invalidation on disable/update/uninstall

### 5. Dashboard Card Plugin

能力：

- 在首页添加只读统计卡片。
- 从宿主提供的 usage/request/provider read APIs 读取摘要。
- 支持刷新、错误态、空态。

需要的扩展点：

- `contributes.ui.home.overview.cards`
- read-only analytics APIs
- command/action contribution

## Current Infrastructure Audit

### Plugin Manifest and SDK

Current files:

- `src-tauri/src/domain/plugins.rs`
- `packages/plugin-sdk/src/index.ts`
- `docs/plugins/plugin-api-v1-contract.json`

Current shape:

```text
PluginManifest
  runtime
  hooks
  permissions
  configSchema
  hostCompatibility
```

Assessment:

- Good for gateway hook plugins.
- Not enough for Extension Host.
- Missing `main`, `activationEvents`, `contributes`, `capabilities`, `extensionKind`, `engines`, UI contributions, provider contributions, protocol contributions, commands, storage scopes.

Decision:

- Replace the public plugin manifest direction with Extension Manifest v1.
- Keep old gateway rule support only as legacy migration or as a nested contribution type.

### Runtime

Current files:

- `src-tauri/src/app/plugins/runtime_executor.rs`
- `src-tauri/src/app/plugins/runtime_lifecycle.rs`
- `src-tauri/src/app/plugins/process_runtime.rs`
- `src-tauri/src/app/plugins/wasm_runtime.rs`
- `src-tauri/src/app/plugins/rule_runtime.rs`

Assessment:

- `RuntimeGatewayPluginExecutor` is gateway-hook specific.
- `RuntimeLifecycleRegistry` only registers caches and calls retain/clear.
- `process_runtime` already has the right isolation direction, but is still PoC and not wired as Extension Host.
- WASM is useful for future pure compute modules, but not enough for UI/provider/platform contributions.

Decision:

- Build Extension Host runtime on top of the process isolation direction.
- Upgrade lifecycle registry from cache registry to extension instance registry.
- Keep WASM as optional compute worker behind Extension Host, not the platform mainline.

### UI Pages

Current files:

- `src/app/AppRoutes.tsx`
- `src/pages/ProvidersPage.tsx`
- `src/pages/providers/ProviderEditorDialog.tsx`
- `src/pages/settings/SettingsPage.tsx`
- `src/pages/PluginsPage.tsx`
- request log, usage, home, sessions, MCP and skills pages under `src/pages` and `src/components`.

Assessment:

- Current UI is fixed React routes and fixed page components.
- No global contribution registry.
- No `pageId` / `slotId` / host-rendered schema renderer.
- Provider editor fields are hardcoded.
- Settings sections are hardcoded.

Decision:

- Add a frontend UI Contribution Framework.
- Initial UI is host-rendered schema, not plugin React components.
- Pages opt into stable slots.

### Provider Model

Current files:

- `src-tauri/src/domain/providers/types.rs`
- `src-tauri/src/domain/providers/queries.rs`
- `src/pages/providers/providerEditorSubmitModel.ts`
- `src/services/providers/providers.ts`

Assessment:

- Provider data is fixed-column.
- `ProviderUpsertParams`, `ProviderSummary`, `ProviderForGateway` have no extension value container.
- Current `bridge_type` is a string column, but registry can only resolve host-defined Rust bridge factories.
- Provider editor payload cannot carry plugin-owned fields.

Decision:

- Add provider extension storage.
- Prefer an independent table over unbounded provider columns:

```text
provider_extension_values
  provider_id
  plugin_id
  namespace
  values_json
  updated_at
```

- Provider editor must load, validate and save extension values by plugin/namespace.
- Gateway provider selection must expose extension values to authorized Extension Host APIs.

### Protocol Bridge

Current files:

- `src-tauri/src/gateway/proxy/protocol_bridge/bridge.rs`
- `src-tauri/src/gateway/proxy/protocol_bridge/traits.rs`
- `src-tauri/src/gateway/proxy/protocol_bridge/ir.rs`
- `src-tauri/src/gateway/proxy/protocol_bridge/registry.rs`

Assessment:

- The IR architecture is a strong foundation.
- Current registry is host Rust factory based.
- `Inbound`, `Outbound`, `ModelMapper` are Rust traits, not plugin-callable contracts.
- No bridge contribution manifest, no external bridge RPC, no bridge trace boundary.

Decision:

- Keep the IR concept.
- Add a host/plugin bridge registry.
- Plugin bridge adapters run in Extension Host and communicate with Rust through JSON-RPC envelopes.
- Rust keeps ownership of transport, provider credentials, request limits, stream framing and diagnostics.

### Gateway Hooks and Reports

Current files:

- `src-tauri/src/gateway/plugins/pipeline.rs`
- `src-tauri/src/gateway/plugins/context.rs`
- `src-tauri/src/infra/plugins/runtime_reports.rs`
- `src-tauri/src/infra/plugins/replay_export.rs`

Assessment:

- Gateway hook execution, reports, replay export and retention are useful.
- Current model is plugin-runtime specific and centered on hook names.
- It does not cover provider/bridge/UI commands.

Decision:

- Reframe gateway hooks as one contribution family.
- Generalize runtime reports into extension execution reports over multiple contribution types.

### Package, Install, Market

Current files:

- `src-tauri/src/app/plugin_service.rs`
- `src-tauri/src/infra/plugins/package.rs`
- `src-tauri/src/infra/plugins/repository.rs`
- `src-tauri/src/infra/plugins/market.rs`
- `src/pages/PluginsPage.tsx`

Assessment:

- Package install, checksum, signature, preview, update diff, rollback and quarantine are valuable.
- Preview/diff currently explain runtime/hooks/permissions, not UI/provider/protocol contributions.
- Market listing does not describe contribution surfaces.

Decision:

- Reuse package lifecycle and trust infrastructure.
- Change preview/diff to explain contribution impact:
  - pages extended
  - providers added
  - protocols/bridges added
  - commands registered
  - host APIs requested

### Developer Tools

Current files:

- `packages/create-aio-plugin`
- `packages/plugin-sdk`
- `packages/plugin-wasm-sdk`

Assessment:

- Current tooling generates rule/wasm templates and validates gateway manifests.
- It does not scaffold TypeScript extension plugins.
- It lacks dev server, extension host simulator, contribution tests, UI schema validation and bridge test harness.

Decision:

- Replace public devtool flow with Extension Host tooling.
- Keep rule replay only as a legacy/gateway helper.

## Architecture

### 1. Extension Manifest v1

New manifest shape:

```json
{
  "id": "acme.openrouter",
  "name": "OpenRouter Provider",
  "version": "0.1.0",
  "apiVersion": "1.0.0",
  "main": "dist/extension.js",
  "runtime": {
    "kind": "extensionHost",
    "language": "typescript"
  },
  "activationEvents": [
    "onStartup",
    "onProviderEditor:openrouter",
    "onProtocolBridge:openrouter"
  ],
  "contributes": {
    "providers": [],
    "protocols": [],
    "protocolBridges": [],
    "ui": {},
    "commands": [],
    "gatewayHooks": []
  },
  "capabilities": [],
  "hostCompatibility": {
    "app": ">=0.62.0 <1.0.0",
    "pluginApi": "^1.0.0",
    "platforms": ["macos", "windows", "linux"]
  }
}
```

Rules:

- `main` is required for `extensionHost`.
- `runtime.kind = "extensionHost"` is the mainline.
- `declarativeRules` is not a top-level ecosystem direction. If retained, it appears under `contributes.gatewayRules` or a legacy compatibility loader.
- Contributions are declarative declarations; runtime logic is invoked through explicit host APIs.

### 2. Extension Host Process

The Extension Host is a host-managed process with a narrow JSON-RPC protocol.

Lifecycle:

```text
installed
  -> resolved
  -> activated
  -> contributing
  -> idle
  -> deactivated
  -> disposed
```

Host responsibilities:

- Start process with bounded startup timeout.
- Perform handshake: plugin id, version, api version, contribution hash.
- Load contributions.
- Dispatch commands/provider hooks/bridge calls/gateway hooks.
- Enforce timeouts, byte limits and concurrency limits.
- Kill and reap on timeout/crash/uninstall/update/shutdown.
- Record extension execution reports.

Plugin responsibilities:

- Export activation entrypoint through SDK.
- Register handlers only for declared contributions.
- Return JSON-serializable results.
- Never assume direct file, network, secret, database or WebView access.

### 3. Host-Mediated APIs

Host APIs are grouped by capability:

```text
aio.ui
aio.provider
aio.protocol
aio.gateway
aio.commands
aio.storage
aio.diagnostics
aio.market
```

Examples:

- `aio.provider.getExtensionValues(providerId, namespace)`
- `aio.provider.setExtensionValues(providerId, namespace, values)`
- `aio.protocol.runBridge(bridgeId, input)`
- `aio.diagnostics.getTrace(traceId)`
- `aio.commands.execute(commandId, args)`

Every API must define:

- permission/capability name
- input/output schema
- timeout
- size limit
- failure code
- audit/report behavior

### 4. UI Contribution Framework

Frontend receives enabled contributions from the host and renders them through stable slots.

Core model:

```text
UIContribution
  pluginId
  contributionId
  pageId
  slotId
  type
  order
  schema
  activationEvent
```

Initial slot families:

```text
app.sidebar.items
home.overview.cards
providers.editor.sections
providers.editor.fields
providers.card.badges
providers.card.actions
settings.sections
logs.detail.tabs
logs.detail.actions
usage.panels
plugins.detail.panels
```

Rendering rules:

- Host React owns all components.
- Plugins provide schema and actions.
- Supported controls include text, password, number, boolean, select, multi-select, textarea, code, info, button/action, badge and read-only panel.
- Dynamic data is loaded through host-mediated commands, not direct React execution.
- Disable/update/uninstall invalidates contributions immediately.

### 5. Provider Extension Model

Provider contributions include:

```text
ProviderContribution
  providerType
  displayName
  targetCliKeys
  editorSections
  validation
  auth
  requestPreparation
  modelDiscovery
  healthCheck
```

Storage:

```text
provider_extension_values
  provider_id INTEGER
  plugin_id TEXT
  namespace TEXT
  values_json TEXT
  updated_at INTEGER
```

Gateway flow:

```text
ProviderForGateway
  + extensionValues
  -> provider contribution requestPreparation
  -> protocol bridge selection
  -> upstream request
```

The host still owns credentials, base URL selection, request limits, routing and failover.

### 6. Protocol Bridge Extension Model

Protocol bridge contributions should align with the existing IR architecture.

Contribution shape:

```text
ProtocolContribution
  protocolId
  direction: inbound | outbound | both

ProtocolBridgeContribution
  bridgeType
  inboundProtocol
  outboundProtocol
  modelMapper
  supportsStreaming
  configSchema
```

Execution flow:

```text
Client request JSON
  -> host or extension inbound adapter
  -> AIO IR
  -> model mapper
  -> host or extension outbound adapter
  -> provider request JSON
```

For streaming:

```text
provider SSE event
  -> outbound event to IR chunks
  -> inbound IR chunks to client SSE
```

The Rust host should keep final control over:

- HTTP transport
- credential injection
- stream framing
- provider route selection
- request/response byte limits
- trace logging
- failure classification

### 7. Gateway Contribution Model

Gateway hooks remain, but become a contribution family:

```text
contributes.gatewayHooks
contributes.gatewayRules
contributes.routePolicies
contributes.failoverPolicies
```

Legacy `declarativeRules` can migrate into `gatewayRules`:

```json
{
  "contributes": {
    "gatewayRules": [
      {
        "rules": ["rules/main.json"]
      }
    ]
  }
}
```

### 8. Commands and Workflows

Commands are stable action IDs:

```text
CommandContribution
  command
  title
  category
  enablement
  inputSchema
```

Commands can appear in:

- UI buttons/actions
- command palette if added later
- diagnostics menus
- provider cards
- plugin detail panels

### 9. Extension Reports

Generalize runtime reports:

```text
ExtensionExecutionReport
  pluginId
  contributionType
  contributionId
  commandOrHook
  traceId
  status
  durationMs
  failureKind
  errorCode
  inputBudget
  outputBudget
  mutationSummary
  createdAt
```

This replaces a gateway-only view with a platform-wide diagnostic record.

## Old Design Disposition

### Keep and Upgrade

- Package install/update/rollback/quarantine/trust infrastructure.
- Runtime reports retention.
- Gateway hook pipeline.
- Protocol IR concept.
- Process runtime PoC concepts: start timeout, hook timeout, kill/reap, JSON-RPC.
- Official Privacy Filter as a bundled official extension or host built-in contribution.

### Replace

- Public manifest centered on `runtime + hooks`.
- `declarativeRules` as the recommended community path.
- create-aio-plugin rule-first scaffolding.
- PluginsPage market copy that implies rule plugins are the main ecosystem.

### Defer

- Arbitrary plugin React components.
- Arbitrary native plugins.
- Full marketplace social/product features.
- Enterprise policy/secret/file/network APIs beyond the capability framework.

## Development Plan

### Phase 0: Supersede Old Plugin Direction

Deliverables:

- Mark old Runtime Lifecycle Registry spec as superseded.
- Update plugin architecture docs to say Extension Host is the target.
- Freeze old `declarativeRules` as legacy/low-level gateway rules.

Acceptance:

- No current planning document describes `declarativeRules` as the public plugin mainline.
- The top-level spec lists target plugin archetypes and required contribution points.

### Phase 1: Extension Manifest and SDK Contract

Deliverables:

- Define Extension Manifest v1 in Rust domain and TypeScript SDK.
- Add `contributes`, `activationEvents`, `main`, `capabilities`, `runtime.kind = extensionHost`.
- Update manifest validation, package inspection, preview and update diff.
- Add tests for valid/invalid contribution declarations.

Acceptance:

- SDK validates OpenRouter Provider Plugin manifest.
- SDK validates Request Debug Panel manifest.
- Rust validates the same manifests.
- Old gateway rule manifests are either rejected with clear migration errors or accepted only through explicit legacy compatibility.

### Phase 2: Extension Contribution Registry

Deliverables:

- Host-side registry for enabled plugin contributions.
- Frontend query/API for active contributions.
- Contribution invalidation on enable/disable/update/uninstall/quarantine.
- Stable `pageId` and `slotId` constants.

Acceptance:

- Disabling a plugin removes its UI contributions without app restart.
- Updating a plugin replaces old contribution definitions.
- Contribution registry rejects undeclared or unknown slot IDs.

### Phase 3: Host-Rendered UI Schema

Deliverables:

- Shared UI schema types.
- React renderer for fields, sections, actions, badges, tabs and panels.
- Initial slots:
  - `providers.editor.sections`
  - `settings.sections`
  - `logs.detail.tabs`
  - `plugins.detail.panels`
- Frontend tests for rendering, ordering, disabled state and invalid schema.

Acceptance:

- A test plugin can add a Provider editor section.
- A test plugin can add a Settings section.
- A test plugin can add a Request Log detail tab.
- Invalid schema is ignored with diagnostics, not a crashed page.

### Phase 4: Provider Extension Storage and APIs

Deliverables:

- `provider_extension_values` table and migration.
- Provider summary/upsert APIs carry extension values by namespace.
- Provider editor loads/saves plugin field values.
- Gateway provider selection can access extension values.
- Copy/duplicate provider behavior is explicit.

Acceptance:

- OpenRouter Provider Plugin fields persist.
- Editing provider keeps unrelated plugin namespaces intact.
- Disabling plugin hides fields but does not delete values.
- Gateway can read provider extension values only through host-mediated path.

### Phase 5: Extension Host Runtime

Deliverables:

- Host-managed TypeScript/JavaScript extension process.
- JSON-RPC handshake, activation, command dispatch and deactivate.
- Bounded start/call timeout, request/response bytes, stderr diagnostics.
- Lifecycle registry upgraded to manage extension instances.
- Execution reports for activation and command calls.

Acceptance:

- Extension host crash does not crash AIO.
- Timeout kills/reaps child process.
- Update/uninstall/dispose releases old extension process.
- Contributions from a failed extension are marked unavailable.

### Phase 6: Protocol Bridge Contributions

Deliverables:

- Manifest declarations for protocols and bridges.
- Bridge registry combines host built-ins and plugin contributions.
- JSON-RPC envelopes for inbound/outbound/model mapper calls.
- Bridge trace and replay diagnostics.
- Streaming contract with bounded chunks.

Acceptance:

- Test plugin registers a bridge type.
- Provider can choose plugin bridge type.
- Request translates through AIO IR and returns expected provider body.
- Bridge failure produces stable report and does not corrupt gateway state.

### Phase 7: Gateway Contributions Migration

Deliverables:

- Reframe existing gateway hooks under `contributes.gatewayHooks`.
- Migrate declarative rules to `contributes.gatewayRules` or legacy loader.
- Keep current privacy filter behavior.
- Update replay/export tools to understand extension contributions.

Acceptance:

- Existing Privacy Filter behavior remains equivalent.
- Gateway hook plugin tests pass under the new contribution model.
- Runtime reports identify contribution type.

### Phase 8: Developer Tooling and Examples

Deliverables:

- `create-aio-plugin extension` template.
- TypeScript SDK with manifest/contribution types.
- Local extension host simulator.
- Example plugins:
  - OpenRouter Provider Plugin
  - Claude/OpenAI/Gemini Bridge Plugin
  - Request Debug Panel Plugin
  - Settings Extension Plugin
- Validate/package/publish-check output includes contribution impact.

Acceptance:

- Example plugins build and validate.
- Package preview shows pages/providers/protocols/commands affected.
- Market listing displays contribution categories.

## Testing Strategy

Test layers:

- SDK unit tests: manifest and contribution validation.
- Rust domain tests: manifest parity with SDK.
- Repository/migration tests: provider extension values and plugin contributions.
- Runtime tests: extension host start/activate/dispatch/dispose/failure.
- Frontend tests: contribution renderer and page slots.
- Gateway tests: provider extension values, bridge contributions, gateway hook migration.
- E2E-style tests: target plugin archetypes.

Minimum release gates:

```bash
pnpm typecheck
pnpm lint
pnpm --filter @aio-coding-hub/plugin-sdk test
pnpm --filter create-aio-plugin test
cargo test --manifest-path src-tauri/Cargo.toml plugin
cargo test --manifest-path src-tauri/Cargo.toml protocol_bridge
```

Exact Rust commands may be split during implementation for speed, but coverage must include plugin service, extension host runtime, provider extension storage and protocol bridge registry.

## Acceptance Criteria

- Top-level docs and SDK no longer position `declarativeRules` as the primary plugin path.
- Extension Manifest v1 can describe UI, provider, protocol, gateway and command contributions.
- Host can load active contributions and invalidate them on plugin lifecycle changes.
- Host-rendered UI schema supports at least Provider editor, Settings, Request Log detail and Plugin detail slots.
- Provider extension values are stored separately and survive plugin disable/update.
- Protocol bridge registry can represent plugin-contributed bridge types.
- Extension Host runtime is isolated, bounded and disposable.
- Install preview and update diff explain contribution impact, not just hooks/permissions.
- Example target plugins can be represented by manifests and at least one phase has executable validation tests before implementation is considered complete.

## Open Decisions for Implementation Plan

These should be resolved in the implementation plan, not by changing the product direction:

- Whether to bundle a Node-compatible runtime or use a Rust JS runtime for Extension Host.
- Whether Phase 1 rejects all old manifests immediately or supports a temporary migration loader.
- Exact UI schema field set for M1.
- Exact provider extension duplication behavior.
- Exact bridge JSON-RPC envelope shape.

The product direction is not open: the plugin platform is Extension Host first.
