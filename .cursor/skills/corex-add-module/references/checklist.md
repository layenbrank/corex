# 新增 Action 检查清单（v4）

复制此清单，逐项勾选。Action 名记为 `<name>`，ID 记为 `<id>`（如 `foo.bar`）。

## 1. 设计与范围

- [ ] 确认是内置 Action（非 WASM、非引擎控制流）
- [ ] 选定 Action ID（点分，不与现有冲突）
- [ ] 选定参考实现（template / file / copy / codec）
- [ ] 列出外部 crate 依赖

## 2. 源码文件

- [ ] `crates/registry/src/builtin/<name>.rs`
- [ ] 实现 `Action` trait（`meta` + `execute`）
- [ ] `pub fn register(registry: &mut ActionRegistry)`

## 3. meta / params

- [ ] `ActionMeta::new("<id>", ...)` ID 正确
- [ ] `ParamSchema` 覆盖所需参数
- [ ] `execute` 无 `println!` / `eprintln!`
- [ ] 错误使用 `ActionError::*`

## 4. Cargo.toml（registry）

- [ ] `act-<name> = []` 或 `["dep:..."]`
- [ ] 按需加入 `full` feature 列表
- [ ] 新依赖写入 workspace + registry `optional`

## 5. builtin/mod.rs

- [ ] `#[cfg(feature = "act-<name>")] pub mod <name>;`
- [ ] `register_all` 内调用 `<name>::register(registry)`

## 6. 测试

- [ ] 单元测试或 registry 集成测试
- [ ] `cargo test -p corex-registry --features act-<name>`
- [ ] `cargo build -p corex -p corex-daemon`
- [ ] `cargo test --workspace`（若已进 full）

## 7. 文档（按需）

- [ ] `docs/architecture.md` / README（若用户要求）
- [ ] 示例 Shortcut YAML（若对外演示）

## 8. 禁止项（旧架构）

- [ ] **未** 创建 `corex-core/src/<module>/`
- [ ] **未** 修改 `invoke/registry.rs` / `command/mod.rs`
- [ ] **未** 依赖已删除的 `corex-serve` / `corex-capture`
