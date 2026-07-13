# Findings — v3 模块架构重构

## Code Review（Critical）
- Pipeline 失败时不输出 RunReport（orchestrator 直接 Err）
- IPC screenshot：InvokeContext::daemon 未接线；Monitors/Windows data 丢失
- 并行 DAG 失败无 JoinSet abort

## Tauri 集成
- Capture 路径正常；Monitors/Windows 需 invoke_screenshot 映射 Output.data
- pipeline/schedule/watch 不在 IPC 范围（设计如此）

## watch 模块
- 非 Invoke 编排模块，对标 schedule
- 配置：`pipelines.yaml` 的 `watch.paths/includes/excludes/debounce_ms`
- 过滤复用 `utils/filter.rs`（原 ignore.rs）

## Pipeline v3
- 变量：`${steps.step_id.artifact.path}` / `${steps.step_id.artifact.data.key}`
- version: 3 必填；无 mode/action

## 模块迁移状态
| 模块 | execute | resolve 下沉 |
|------|---------|--------------|
| compression | yes | yes |
| copy/scrub/shade/generate/bootstrap | yes | copy/generate 部分 |
| codec/scan/morph/screenshot | yes | registry（待续） |

## 触发模块命名（Phase 8）

| 旧名 | 新名 |
|------|------|
| `validate_triggers` | `check` |
| `serve_both` | `serve_dual` |
| `RunMode::Both` | `RunMode::Dual` |
| `mode()` | `run_mode()` |
| `new_running_set` | `new_set` |
| `spawn_guarded` | `spawn` |
| `run_guarded` | `run_sync` |
| `prepare` / `serve_targets` | `resolve` / `run_loop` |
| `serve_loop_for` | `loop_for` |
| `has_scheduled` / `validate_cron` | `has_cron` / `check_cron` |
| `fail_err` / `fail_msg` | `into_err` / `message` |
| `split_fail_msg` | `parse_fail` |

## 逻辑修复
- `serve_dual` cron 线程错误不再静默（`eprintln`）
- `schedule::serve` 入口统一 `check_cron`，与 `corex pipeline` 校验一致
- `check` 解析结果复用于 Watch / Dual，避免重复 `resolve`

## Binary 重命名
- `corex-shot` → `corex-capture`（对齐 `screenshot capture` 子命令）
- `screenshot` feature 含 `cli`；`parse` 模块仅在 `invoke` feature 下编译

## exec 模块（Phase 9）
- stdout 最后一行 JSON，**必填** `path`（string）+ `data`（object）；`data` 结构脚本自定
- v1 无 `env` / `timeout_secs`：子进程继承环境；无限等待
- shell：`.ps1` → powershell -File；`.bat`/`.cmd` → cmd /C
- H5+：`generate-version.ps1` 一次写 version.json + version.js，copy 步骤复用 artifact.path
