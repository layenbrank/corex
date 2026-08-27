# Findings — 企业架构加固（P0–P3）

## 评分缺口（已确认）
- `docs/enterprise-deploy.md` 落后于 `config/enterprise.toml`（缺 desktop/point/pick）
- preset 未禁：`capture.monitors`、`keyring.*`、`scan.os`
- `exec.run` 未走 `confine_path`；`shell.run`/`exec.run` 的 `cwd` 同理
- daemon `check_invoke_allowed` 与 `ui_probe::check_probe_allowed` 双份逻辑
- history 写入完整 `e.to_string()`，可能含路径/敏感片段
- `architecture.md` 缺 enterprise 配置项；无最小构建文档

## 代码锚点
| 能力 | 路径 |
|------|------|
| enterprise preset | `config/enterprise.toml` |
| probe 门禁 | `crates/registry/src/ui_probe.rs` |
| Invoke 门禁 | `bins/daemon/src/main.rs` `check_invoke_allowed` |
| PermissionKind | `crates/engine/src/definition.rs`（将下沉 core） |
| confine_path | `crates/registry/src/builtin/util.rs` |
| exec.run | `crates/registry/src/builtin/exec.rs` |
| shell.run | `crates/registry/src/builtin/shell.rs` |
| history | `crates/engine/src/pipeline.rs` `record_history` |

## 依赖约束
- `corex-engine` → `corex-registry`：registry **不能**依赖 engine
- 统一门禁必须放在 `corex-core`
