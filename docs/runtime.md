# Corex Runtime 层

`corex-core/src/runtime/` 定义进程级运行时契约：全局 CLI 选项、输出格式、退出码与 tracing 初始化。

## RuntimeOpts

全局 clap 参数（通过 `Args` flatten）：

| 选项 | 说明 |
|------|------|
| `--format human\|json` | 输出格式，默认 `human` |
| `-q, --quiet` | 仅结果 / 退出码，抑制进度与 banner |
| `-v, --verbose` | 启用 DEBUG 级 tracing（可重复） |
| `--color auto\|always\|never` | 终端颜色 |

## Emitter

- **human**：crossterm 彩色进度（开发友好）
- **json**：stdout 仅输出机器可读 JSON；stderr 仍走 tracing

Pipeline 示例：

```bash
corex pipeline --validate --format json --config pipelines.yaml
corex pipeline --id build-h5 --format json --report-file report.json
```

## 退出码

| Code | 含义 |
|------|------|
| 0 | 成功 |
| 1 | 用户输入错误（clap、无效参数） |
| 2 | 配置错误（YAML 解析、validate 失败） |
| 3 | 运行时业务错误（模块 execute 失败） |
| 4 | I/O 或系统错误 |
| 5 | 内部错误 |

`main` 返回 `ExitStatus`，实现 `std::process::Termination`。

## Tracing

- 依赖：`tracing`、`tracing-subscriber`（`env-filter`、`json`）
- 每个 pipeline step 一个 span：`step_id`、`module`
- CI 可设 `RUST_LOG=corex=debug` 或 `COREX_LOG_FORMAT=json`

## 配置覆盖

变量合并优先级（高 → 低）：

1. CLI `-D key=value`
2. 环境变量 `COREX_VAR_<NAME>`
3. `pipelines.yaml` 中 `variables`
4. 默认值

敏感值使用 `${env.NAME}`，禁止在配置中明文写入密码。

## 初始化

```rust
runtime::init(args.runtime.clone())?;
// ...
runtime::state().emitter.json(&report)?;
```
