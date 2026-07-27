# Task Plan: 修复 CI（flood probe + 发布锁文件）

## Goal
删除导致 build-and-test 失败的本地 probe 测试，并将 v2.1.0 标签移到 Cargo.lock 已同步的提交以修复 publish-release。

## Current Phase
Phase 4: Retag v2.1.0 and push

## Phases

### Phase 1: Planning files
- **Status:** complete

### Phase 2: Delete notify_flood_probe
- **Status:** complete

### Phase 3: Verify --locked and tests
- **Status:** complete

### Phase 4: Retag v2.1.0 and push
- **Status:** in_progress

### Phase 5: Update planning completion
- **Status:** pending

## Decisions Made
| Decision | Rationale |
|----------|-----------|
| 删除 probe 而非 tempfile | 本意是本地探测，非产品回归 |
| 保留 --locked、不改 workflow | 可复现构建；根因是打标签过早 |
| 移动 v2.1.0 而非发 2.1.1 | Build 在 Create Release 前已失败，可安全重打标签 |

## Errors Encountered
| Error | Attempt | Resolution |
|-------|---------|------------|
|         |         |            |
