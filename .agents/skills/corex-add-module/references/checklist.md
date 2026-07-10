# 新增模块检查清单

复制此清单，逐项勾选。模块名记为 `<module>`。

## 1. 设计与范围

- [ ] 确认模块类型：Invoke 业务 / 编排 / 仅 CLI
- [ ] 确认模块名（snake_case，不与现有模块冲突）
- [ ] 选定参考模块（scan / copy / codec / bootstrap）
- [ ] 列出外部 crate 依赖

## 2. 源码文件

- [ ] `corex-core/src/<module>/mod.rs`
- [ ] `corex-core/src/<module>/schema.rs`（Parser + Serialize + Deserialize）
- [ ] `corex-core/src/<module>/service.rs`（`execute` + `run`）
- [ ] `corex-core/src/<module>/parse.rs`（仅当需要 Pipeline 占位符）

## 3. schema.rs

- [ ] 所有 IPC 字段可用 serde 反序列化
- [ ] 子命令结构使用 enum + `#[command(subcommand)]`
- [ ] 路径参数使用 `verifier::path` 或 `utils::paths` 校验
- [ ] `cargo run -p corex -- <module> --help` 可正常显示

## 4. service.rs

- [ ] `execute` 无 `println!` / `eprintln!`
- [ ] `run` 调用 `execute` 并格式化输出
- [ ] 错误使用 `anyhow::Context` 附带上下文
- [ ] `Output` 结构体定义清晰
- [ ] 如需 IPC data，实现 `into_invoke_result()` 或等价方法

## 5. Cargo.toml

- [ ] `<module> = ["dep:..."]` feature 定义
- [ ] 加入 `command` feature 列表
- [ ] 加入 `invoke` feature 列表（若可被 Pipeline/IPC 调用）
- [ ] 加入 `daemon` feature 列表（若 serve 需支持）
- [ ] 新依赖写入 `[dependencies]`

## 6. lib.rs

- [ ] `#[cfg(feature = "<module>")] pub mod <module>;`

## 7. command/mod.rs

- [ ] `use crate::<module>;`
- [ ] `Commands::<Module>` variant + doc comment
- [ ] `dispatch()` 分支 → `<module>::run(&a)`

## 8. invoke/registry.rs（Invoke 模块）

- [ ] `invoke()` match 分支
- [ ] `known_modules()` 列表项
- [ ] `invoke_<module>()` 辅助函数
- [ ] 正确的 `InvokeResult` 映射（path / data / default）
- [ ] `parse_args` 调用（若存在 parse.rs）

## 9. 测试

- [ ] service 单元测试
- [ ] `tests/invoke_modules.rs` 集成测试（推荐）
- [ ] `cargo build --workspace` 通过
- [ ] `cargo test -p corex-core --features invoke` 通过

## 10. 文档（按需）

- [ ] `docs/architecture.md` 模块表
- [ ] `docs/ipc-protocol.md` module 列表（若对外 IPC）
- [ ] README 或迁移说明（若用户要求）

## 11. 迁移专项检查（从 Tauri/其他项目迁入）

- [ ] 已移除 Tauri / UI / DB 依赖
- [ ] Tauri 侧改为 `corex_ipc::invoke` 薄调用
- [ ] IPC args 使用 typed 格式，非 legacy 扁平 JSON
- [ ] 无重复业务逻辑留在 Tauri utils
