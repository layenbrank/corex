# 发布与 Changelog

本文说明 **tag 发布前**如何用 [git-cliff](https://git-cliff.org/docs/) 生成 `CHANGELOG.md`，并与 `Cargo.toml` 版本、GitHub Release CI 对齐。

配置文件：[cliff.toml](../../cliff.toml)（仓库根目录）。
发布工作流：[.github/workflows/publish-release.yml](../../.github/workflows/publish-release.yml)。

> **要点：** CI **不会**自动跑 `git-cliff`。Changelog 由维护者在打 tag **之前**本地生成并提交。CI 仅在仓库已有 `CHANGELOG.md` 时把它打进 release ZIP；GitHub Release 正文另用 `generate_release_notes`。

---

## 1. 前置

```powershell
cargo install git-cliff --locked
# 可选：与门禁一并安装
cargo install cargo-deny typos-cli --locked
```

确认：

- 提交信息尽量遵循 [Conventional Commits](https://www.conventionalcommits.org/)（`feat` / `fix` / `docs` / …）
- 破坏性变更用 `feat!:` / `fix!:` 或正文 `BREAKING CHANGE:`（会进 💥 分组并抬 major）
- 纯中文标题、`[skip]`、只改 `task_plan.md` / `progress.md` 等会被 cliff 跳过（见 `cliff.toml`）

---

## 2. 标准发布流程（手动，tag 前）

在 `master`（或发布分支）上、**尚未打 tag** 时执行：

```powershell
# 1) 预览自上一 tag 以来的变更
git-cliff -l

# 2) 按 cliff [bump] 规则查看建议版本号
git-cliff --bumped-version
# 例: 6.1.0  （feat → minor；breaking → major；chore/docs 等不抬版本）

# 3) 对齐 workspace 版本（与即将打的 tag 去掉 v 后一致）
#    编辑 Cargo.toml → [workspace.package] version = "X.Y.Z"

# 4) 生成 / 覆盖根目录 CHANGELOG.md（cliff.toml 已设 output）
git-cliff -o CHANGELOG.md
# 等价: git-cliff   （配置了 output = "CHANGELOG.md"）

# 5) 提交版本与 changelog
git add Cargo.toml Cargo.lock CHANGELOG.md
git commit -m "chore(release): bump version to X.Y.Z"

# 6) 打 tag 并推送（触发 Publish Release）
git tag vX.Y.Z
git push origin HEAD
git push origin vX.Y.Z
```

预发布渠道（与 CI 约定一致）：

```powershell
# Cargo.toml version = "4.0.0-beta.1"
git tag v4.0.0-beta.1
git push origin v4.0.0-beta.1
```

也可用 Actions → **Publish Release** → `workflow_dispatch` 指定已存在的 tag。

### 版本对齐门禁

推送 `v*` tag 后，CI 会校验：

`tag` 去掉 `v` 后 == `Cargo.toml` 的 `[workspace.package].version`

不一致则构建失败。因此必须先改 Cargo 版本并提交，再打同名 tag。

---

## 3. 常用 git-cliff 命令

| 目的 | 命令 |
|------|------|
| 预览上一 tag → HEAD | `git-cliff -l` |
| 仅未发布提交 | `git-cliff -u` |
| 写出 `CHANGELOG.md` | `git-cliff -o CHANGELOG.md` |
| 未发布段前置追加 | `git-cliff -u -p CHANGELOG.md` |
| 建议下一 SemVer | `git-cliff --bumped-version` |
| 本地默认（不调 GitHub API） | 已在 `cliff.toml`：`[remote] offline = true` |
| 拉取 PR / 新贡献者元数据 | 见下节 |

带 GitHub 元数据（可选）：

```powershell
$env:GIT_CLIFF_OFFLINE = "false"
$env:GITHUB_TOKEN = "ghp_…"   # 或 --github-token
git-cliff -o CHANGELOG.md
```

说明见 [git-cliff Remote](https://git-cliff.org/docs/configuration/remote) 与 [GitHub 集成](https://git-cliff.org/docs/integration/github)。

---

## 4. Changelog 长什么样

- 分组带图标：🚀 Features、🐛 Bug Fixes、📚 Documentation、…
- 破坏性提交另有 **💥 Breaking Changes** 区（需 `!` 或 `BREAKING CHANGE`）
- 条目含短 hash 链接；有 token 时可显示 `@user` / PR 号
- 页脚为 Keep a Changelog 风格的版本对照链接

重新全量生成：

```powershell
git-cliff -o CHANGELOG.md
```

---

## 5. 与 CI 的分工

| 步骤 | 谁做 |
|------|------|
| 改 `Cargo.toml` version | 维护者（本地） |
| 生成并提交 `CHANGELOG.md` | 维护者（`git-cliff`，本地） |
| `git tag vX.Y.Z` + push | 维护者 |
| 版本对齐检查、构建、打 ZIP、创建 GitHub Release | `publish-release.yml` |
| 把已有 `CHANGELOG.md` 打进 ZIP | CI（文件存在才复制） |
| Release 说明正文 | CI（`generate_release_notes: true`） |

---

## 6. 提交约定（影响分组与 bump）

| 前缀 | Changelog 分组 | 版本影响（默认） |
|------|----------------|------------------|
| `feat` | 🚀 Features | minor |
| `fix` | 🐛 Bug Fixes | patch |
| `feat!` / `fix!` / `BREAKING CHANGE` | 💥 + 原分组 | major |
| `docs` / `style` / `test` / `ci` / `build` / `chore` | 对应分组 | **不**抬版本（`no_increment_regex`） |
| `refactor` | ♻️ Refactoring | patch（无 `!` 时） |

更多解析规则见仓库根目录 `cliff.toml` 的 `[git]` / `[bump]`。

---

## 相关链接

- [架构 — 构建与开发工具链](../reference/架构.md#构建与开发工具链)
- [快速开始 — 本地门禁](../guide/快速开始.md#贡献者本地门禁可选)
- [git-cliff 文档](https://git-cliff.org/docs/)
- [破坏性变更 v5](../changelog/破坏性变更-v5.md)
