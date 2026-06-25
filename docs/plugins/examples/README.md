# 插件示例

这里放官方示例和推荐社区插件形态。示例的目标是展示插件系统应该怎样被使用，而不是扩展宿主内置插件数量。

- [Privacy Filter](./privacy-filter.md)：当前唯一内置官方插件 `official.privacy-filter`，对齐 `packyme/privacy-filter` 的核心脱敏能力。

## 示例清单

| 示例 ID | 目标 | Hooks | Permissions | Fixtures / 覆盖路径 |
| --- | --- | --- | --- | --- |
| `official.privacy-filter` | 请求和日志脱敏 | `gateway.request.afterBodyRead`, `gateway.request.beforeSend`, `log.beforePersist` | `request.body.read`, `request.body.write`, `log.redact` | 官方 fixture 存在于宿主资源目录；覆盖配置 UI、request replay export 和日志脱敏边界 |
| `examples/prompt-helper` | 在请求进入 provider 前补充提示词约束 | `gateway.request.afterBodyRead` | `request.body.read`, `request.body.write` | 应包含 Claude messages 和 OpenAI/Codex Responses fixture；覆盖 trace replay 后的请求 mutation |
| `examples/redactor` | 展示社区 declarativeRules 脱敏形态 | `gateway.request.beforeSend`, `log.beforePersist` | `request.body.read`, `request.body.write`, `log.redact` | 应包含命中和未命中的 replay fixture；覆盖 pack、publish-check 和市场安装元数据 |
| `examples/response-guard` | 在响应返回后做轻量检查或标记 | `gateway.response.beforeSend` | `response.body.read`, `response.body.write` | 应包含 streamed/non-streamed 响应 fixture；覆盖失败策略、运行诊断和 replay notes |

这些示例都保持在 Plugin API v1 范围内。宿主负责运行诊断、fixture 导出、安装校验和市场索引解析；插件只声明 manifest、hooks、permissions 和自己的规则或运行时代码。
