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
