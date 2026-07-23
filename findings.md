# Findings

## 线格式决策
- 对外：`module` + `action?` + `format?`|`algorithm?` + 扁平 `params`/`args`
- 对内：`assemble_typed` → PascalCase clap Args（serde flatten 后无 scheme 键）
- compression format 词表：`zip` / `tar-gz` / `7z`（CLI SevenZ 子命令名为 `7z`）
- codec：`scheme` 重命名为 `algorithm`；compression：`format`

## 审查修复（2026-07-23）
- 文档扫尾：ipc-protocol / README / breaking-changes 去掉残留 PascalCase
- `assemble_typed` 各臂 `#[cfg(feature)]`；UNIT action 拒绝非空 flags
- shade 误用步骤 `format` 提示写 `params.format`
- `validate_wire`：assemble + 无 `${` 时 Args decode；有占位符则跳过
- 单测：bootstrap / morph kebab / codec 错配 / shade format / validate_wire

## 验证
- `cargo test -p corex-core --lib` / `cli_contract` 通过
