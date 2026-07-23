# Task Plan: Pipeline 小写路由 + 扁平 params

## Goal
破坏性重构线格式：`module` / `action` / `format`|`algorithm` + 扁平 `params`/`args`；组装层生成 clap Args；去掉 scheme 与 PascalCase 嵌套。

## Phases

| Phase | Task | Status |
|-------|------|--------|
| 1 | StepConfig + IPC 信封扩展 | complete |
| 2 | invoke 组装层 | complete |
| 3 | codec/compression scheme 重命名 | complete |
| 4 | 迁移示例与测试 | complete |
| 5 | 文档与 skill | complete |
| 6 | cargo test 验证 | complete |
