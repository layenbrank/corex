# Rust CLI 重构 Vue2 H5+ 部署链路 —— 详细规划文档

## 第一部分：现状分析与兼容基线

### 1.1 现有脚本执行流程

```
┌─────────────────────────────────────────────────────────────────┐
│                    Current Deployment Flow                       │
├─────────────────────────────────────────────────────────────────┤
│ Scenario 1: 日常发布 (deploy.bat)                              │
│  1. git pull origin master                                       │
│  2. git fetch --all                                              │
│  3. git reset --hard origin/master                               │
│  4. call install.bat                                             │
│     └─ nvm use 16.20.2                                           │
│     └─ npm install                                               │
│  5. call build.bat                                               │
│     └─ node start.js (生成 src/assets/js/version.js)            │
│     └─ npm run build (→ app/)                                    │
│  6. del ../H58991839.wgt                                         │
│  7. 7z.exe a -tzip ../H58991839.wgt ./app/*                     │
│  8. del ./version.json && del ../version.json                    │
│  9. node version_json.js (生成 YYYYMMDD)                        │
│ 10. copy version.json ../version.json                            │
│                                                                  │
│ Scenario 2: 同日多发 (deploy-app.bat)                           │
│  1. (git ops: 注释掉)                                            │
│  2. node version_jsonapp.js --full (生成 YYYYMMDDHHmmssSSS)    │
│  3. npm run build (→ app/)                                       │
│  4. del ../H58991839.wgt                                         │
│  5. 7z.exe a -tzip ../H58991839.wgt ./app/*                     │
│  6. copy version.json ../version.json                            │
└─────────────────────────────────────────────────────────────────┘
```

### 1.2 重复与冗余点统计

| 重复项                   | 文件位置                             | 行号      | 性质         | 建议             |
| ------------------------ | ------------------------------------ | --------- | ------------ | ---------------- |
| git pull + fetch + reset | install.bat + deploy.bat             | L1-3 各处 | 流程重复     | 统一为单一触发点 |
| version.json 生成        | version_json.js + version_jsonapp.js | 全文      | 代码重复     | 合并为一个模块   |
| version.json 删除 + 生成 | deploy.bat L10-12                    | -         | 临时文件策略 | 改为原子写入     |
| 版本号精度选择           | deploy.bat vs deploy-app.bat         | L12 vs L7 | 策略分叉     | 默认统一，可配置 |
| 打包命令                 | 7z.exe 调用                          | 两处      | 外部依赖     | 替换为 Rust zip  |

### 1.3 版本号机制详解

**当前状态：**

- `version_json.js`：输出 `YYYYMMDD`（精度：天）
  ```
  20260508 → { "version": "20260508" }
  ```
- `version_jsonapp.js --full`：输出 `YYYYMMDDHHmmssSSS`（精度：毫秒）
  ```
  20260509143527123 → { "version": "20260509143527123" }
  ```

**文件分布：**

- 生成目标：`./version.json`, `./public/version.json`, `./app/version.json`
- 产出位置：`src/assets/js/version.js` (ES6 export function)

**运行时比较逻辑：** [src/App.vue#L508]

```javascript
if (serverVersion && serverVersion !== appVer) {
	this.newDown = true // 触发更新提示
}
```

**关键约束：** 三处 version.json 必须内容一致，否则客户端可能读到错误版本号。

### 1.4 H5+ 应用元数据

**manifest.json 约束（[public/manifest.json#L3], [public/manifest.json#L6]）：**

```json
{
	"id": "H58991839", // 固定值，用于 App 路由
	"version": {
		"name": "1.1.1", // 仅供 HBuilder 编辑器显示
		"code": 111 // 仅供 HBuilder 编辑器显示（无实际运行时用途）
	}
}
```

**推论：** H5+ 不强制使用 version.json；这是你们业务自定的"远端版本检查"协议。

- `manifest.json` 的 version 仅在编译时打入 app，运行时不读
- `version.json` 由客户端主动 HTTP GET 拉取

### 1.5 兼容性约束表

| 约束项       | 现状                              | Rust CLI 保持           | 理由                 |
| ------------ | --------------------------------- | ----------------------- | -------------------- |
| 包名         | `H58991839.wgt`                   | ✓ 同                    | manifest.json 硬编码 |
| 版本格式     | `YYYYMMDD` 或 `YYYYMMDDHHmmssSSS` | ✓ 默认日期版，可切      | 主流程习惯           |
| 版本文件位置 | 3 处 (root/public/app)            | ✓ 同                    | app 版本供运行时读   |
| Git 策略     | reset --hard                      | ✓ 同                    | CI/CD 友好           |
| 外部工具     | 7z.exe、nvm、npm                  | ✓ 7z→Rust，保留 nvm/npm | 内聚打包             |
| 日志         | 无格式约定                        | ✓ 新增 JSON + 彩色 text | 便于 CI/CD 和人工读  |
| 失败处理     | 不统一                            | ✓ 非 0 exit code        | POSIX 标准           |

---

## 第二部分：Rust CLI 架构设计

### 2.1 项目结构

```
deploy/                   # Rust 项目根
├── Cargo.toml                   # 项目元数据 & 依赖声明
├── Cargo.lock                   # 依赖锁定
├── src/
│   ├── main.rs                  # CLI 入口，子命令路由
│   ├── lib.rs                   # 导出公共 API
│   ├── cli/
│   │   ├── mod.rs              # CLI 模块索引
│   │   ├── args.rs             # 命令行参数定义（clap）
│   │   └── config.rs           # 配置加载与优先级
│   ├── commands/
│   │   ├── mod.rs              # 子命令索引
│   │   ├── preflight.rs        # 环境检查
│   │   ├── install.rs          # 依赖安装
│   │   ├── build.rs            # Vue 构建
│   │   ├── version.rs          # 版本文件生成
│   │   ├── package.rs          # WGT 打包
│   │   ├── deploy.rs           # 编排执行
│   │   ├── verify.rs           # 产物验证
│   │   └── upload.rs           # 上传发布
│   ├── core/
│   │   ├── mod.rs              # 核心库索引
│   │   ├── version_manager.rs  # 版本号生成与格式化
│   │   ├── file_ops.rs         # 原子写入、路径管理
│   │   ├── process.rs          # 子进程执行（npm、git）
│   │   ├── zipper.rs           # ZIP 打包（无 7z 依赖）
│   │   └── error.rs            # 统一错误类型
│   └── utils/
│       ├── mod.rs              # 工具库索引
│       ├── logger.rs           # 日志（text + JSON）
│       ├── validators.rs       # 验证规则（版本格式等）
│       └── constants.rs        # 常量定义
├── tests/
│   ├── integration_tests.rs    # 集成测试
│   └── fixtures/               # 测试数据
├── docs/
│   ├── ARCHITECTURE.md         # 详细设计文档
│   ├── COMMAND_SPEC.md         # 子命令规范
│   └── MIGRATION.md            # 从 bat 迁移指南
└── README.md
```

### 2.2 依赖清单

```toml
[dependencies]
# CLI 框架
clap = { version = "4.4", features = ["derive"] }  # 命令行参数解析
anyhow = "1.0"                                       # 错误处理（Option 转 Error）
thiserror = "1.0"                                    # 自定义 Error type
log = "0.4"                                          # 日志接口
tracing = "0.1"                                      # 结构化日志
tracing-subscriber = "0.3"                           # 日志实现

# 文件 & 压缩
walkdir = "2.4"                                      # 目录遍历
zip = "0.6"                                          # ZIP 打包（无外部依赖）
serde_json = "1.0"                                   # JSON 解析与生成
toml = "0.8"                                         # TOML 配置解析
serde = { version = "1.0", features = ["derive"] }  # 序列化框架

# 时间
chrono = { version = "0.4", features = ["serde"] }  # 时间处理
chrono-tz = "0.8"                                    # 时区支持

# 系统与进程
tokio = { version = "1", features = ["process", "macros"] }  # 异步运行时（可选）
std::process                                         # 标准进程库（足够用）
tempfile = "3.8"                                     # 临时文件（原子写入）

# 网络（upload 命令）
reqwest = { version = "0.11", features = ["json"] } # HTTP 客户端
rustssh2 = "0.23"                                    # SFTP 支持（可选）

# 开发工具
[dev-dependencies]
tempfile = "3.8"                                     # 测试临时目录
mockito = "1.2"                                      # HTTP mock
```

### 2.3 命令接口契约

#### 全局参数

```
--config <PATH>          # 配置文件路径（默认 deploy.toml）
--log-level <LEVEL>      # 日志级别：trace|debug|info|warn|error
--log-format <FMT>       # 日志格式：text|json
--dry-run                # 模拟运行，不改动文件系统
```

#### 子命令规范

**1. preflight — 环境检查**

```bash
cargo run -- preflight \
  --check-node              # 检查 Node.js
  --check-nvm               # 检查 nvm
  --list-installed          # 列出已安装 Node 版本
```

- 输入：无
- 输出：
  ```json
  {
  	"status": "ok|warning|error",
  	"checks": [
  		{ "name": "Node.js", "version": "v16.20.2", "status": "ok" },
  		{ "name": "nvm", "status": "ok" },
  		{ "name": "npm", "version": "8.19.4", "status": "ok" },
  		{ "name": "git", "version": "2.40.0", "status": "ok" }
  	]
  }
  ```
- 退出码：0 (全部通过) | 1 (缺失必要工具) | 2 (警告)

**2. install — 依赖安装**

```bash
cargo run -- install \
  --sync-git origin/master     # Git 同步分支
  --node-version 16.20.2       # 指定 Node 版本（覆盖 .nvmrc）
  --npm-ci                     # 使用 npm ci（生产环保）
```

- 输入：Git 远端可达、nvm 已装、.nvmrc 或 package.json 存在
- 执行步骤：
  1. git pull origin/master
  2. git fetch --all
  3. git reset --hard origin/master
  4. nvm use 16.20.2 (或从 .nvmrc)
  5. npm install (或 npm ci)
- 输出：执行日志 + 最终状态
- 退出码：0 (成功) | 1 (git 失败，回滚状态) | 2 (npm 失败)

**3. build — Vue 编译**

```bash
cargo run -- build \
  --mode production \          # 或 development
  --output-dir ./app \         # 覆盖 vue.config.js 的 outputDir
  --max-heap 9000              # Node 堆大小 (MB)
```

- 输入：node_modules/ 存在
- 执行：npm run build (或 npm run build:dev)
- 输出：./app/ 目录填充，验证存在 index.html
- 退出码：0 (成功) | 1 (编译失败)

**4. version — 版本文件生成**

```bash
cargo run -- version \
  --granularity date \         # 或 timestamp
  --timestamp 2026-05-09T14:30:00Z \  # 可指定源时间（幂等构建）
  --targets root,public,app \  # 写入目标（默认全部）
  --source-version-file \      # 生成 src/assets/js/version.js
  --base-time "2026-05-09"     # 仅日期版有效，指定日期部分
```

- 输入：无（或接收时间源参数）
- 输出：

  ```
  root/version.json:
  { "version": "20260509" }

  public/version.json:
  { "version": "20260509" }

  app/version.json:
  { "version": "20260509" }

  src/assets/js/version.js (可选):
  export function version() {
    return "20260509";
  }
  ```

- 退出码：0 (成功) | 1 (写入失败)

**5. package — WGT 打包**

```bash
cargo run -- package \
  --input-dir ./app \              # 源目录
  --output-wgt ../H58991839.wgt \  # 输出路径
  --cleanup-existing \             # 安全删除已存在文件
  --manifest-id H58991839          # 写入到 manifest 元数据（可选）
```

- 输入：./app 存在且包含 index.html
- 过程：
  1. 验证 input-dir 有效
  2. 清理已存在的 output-wgt
  3. ZIP 打包所有文件（相对路径）
  4. 输出大小统计
- 输出：
  ```
  Packaged ./app/ → ../H58991839.wgt (42.5 MB)
  Files: 1428, Dirs: 156
  ```
- 退出码：0 (成功) | 1 (打包失败)

**6. deploy — 完整编排**

```bash
cargo run -- deploy \
  --mode app \                 # 或 app-ts
  --manifest-id H58991839 \
  --skip-git \                 # 跳过 git 同步
  --parallel-version-build \   # version 与 build 并行（高级）
  --dry-run                    # 模拟执行
```

- 执行计划：
  ```
  preflight
    ↓
  install (git + npm)
    ↓
  version (YYYYMMDD)
    ↓
  build (npm run build)
    ↓
  package (zip → wgt)
    ↓
  verify (一致性校验)
    ↓
  ✓ Deploy completed
  ```
- 输出：每步的状态 + 耗时统计
- 退出码：0 (成功) | 1 (任意步骤失败)

**7. verify — 产物验证**

```bash
cargo run -- verify \
  --wgt-path ../H58991839.wgt \
  --manifest-json public/manifest.json \
  --strict \                   # 严格模式（检查文件权限等）
```

- 检查项：
  1. WGT 文件存在且可读
  2. WGT 内包含 manifest.json
  3. root/public/app 三份 version.json 内容一致
  4. 版本号格式有效（YYYYMMDD）
  5. 版本号不小于前一个版本（可选告警）
- 输出：
  ```json
  {
  	"status": "ok",
  	"checks": [
  		{ "check": "WGT file exists", "status": "ok" },
  		{ "check": "Version consistency", "status": "ok", "version": "20260509" },
  		{ "check": "Format valid", "status": "ok" }
  	]
  }
  ```
- 退出码：0 (全通过) | 1 (关键失败) | 2 (警告)

**8. upload — 上传发布**

```bash
cargo run -- upload \
  --wgt-path ../H58991839.wgt \
  --version-json ./version.json \
  --target-dir /resource/app \
  --server http://example.com \  # HTTP 上传
  --token <TOKEN> \
  --skip-verify                  # 跳过 verify 前置检查
```

- 执行：
  1. (可选) verify --strict
  2. POST /resource/app/H58991839.wgt
  3. POST /resource/app/version.json
  4. 验证响应 200 OK
- 输出：上传进度 + 最终 URL
- 退出码：0 (成功) | 1 (网络/权限失败)

---

## 第三部分：实现细节与代码框架

### 3.1 配置加载优先级（3层）

**deploy.toml** (示例)

```toml
[environment]
node_version = "16.20.2"
nvm_path = "C:/nvm"
shell = "cmd"  # Windows 特定

[git]
auto_sync = true
origin_branch = "master"
reset_hard = true

[build]
mode = "production"
output_dir = "./app"
max_heap_size_mb = 9000
npm_ci = true

[version]
granularity = "date"  # "date" | "timestamp"
write_targets = ["root", "public", "app"]  # 必须包含至少 app
generate_src_version_js = true

[package]
manifest_id = "H58991839"
wgt_output = "../H58991839.wgt"
wgt_input = "./app"

[verify]
strict = false
check_version_history = false

[upload]
enabled = true
target_server = "http://example.com"
target_path = "/resource/app"
timeout_seconds = 300
```

**优先级：** CLI arg > env var > config file > default value

### 3.2 版本号生成算法

```
Input:
  - granularity: "date" | "timestamp"
  - base_time: Option<DateTime> (用于重放构建)

Output:
  - "YYYYMMDD" (日期版)
  - "YYYYMMDDHHmmssSSS" (时间戳版)

Logic (date):
  t = base_time || SystemTime::now()
  YYYY = t.year()
  MM = t.month().zero_padded()
  DD = t.day().zero_padded()
  result = format!("{}{:02}{:02}", YYYY, MM, DD)

Logic (timestamp):
  same as above, plus:
  HH = t.hour().zero_padded()
  mm = t.minute().zero_padded()
  ss = t.second().zero_padded()
  SSS = t.millisecond().zero_padded(3)
  result = format!("{}{:02}{:02}{:02}{:02}{:02}{:03}",
                    YYYY, MM, DD, HH, mm, ss, SSS)

Validation:
  - date: length == 8, all digits, month 01-12, day 01-31
  - timestamp: length == 17, all digits, + HH 00-23, mm/ss 00-59, ms 000-999
```

### 3.3 原子文件写入流程

```
write_version_files(
  versions: {root, public, app},
  version_str: "20260509",
  write_src_js: bool
) → Result<()>

Steps:
1. For each target_dir in [root, public, app]:
   a. Create temp file: {target_dir}/.version.json.tmp
   b. Write JSON: {"version": "20260509"}
   c. fsync() to disk
   d. Rename .tmp → version.json (atomic on POSIX/NTFS)
   e. Log: "✓ Wrote {target_dir}/version.json"

2. (Optional) If write_src_js:
   a. Create temp: src/assets/js/.version.js.tmp
   b. Write:
      export function version() {
        return "20260509";
      }
   c. fsync()
   d. Rename
   e. Log

3. On error at any step:
   a. Delete .tmp files (cleanup)
   b. Return Err with context (which file, why)
   c. Do NOT rollback already-written files (明确设计)
      原因：部分写入后回滚反而更危险；让管理员手工决定
```

### 3.4 ZIP 打包实现（无 7z 依赖）

```rust
// pseudocode
fn create_wgt_package(
  input_dir: PathBuf,  // ./app
  output_path: PathBuf  // ../H58991839.wgt
) -> Result<()> {
  // 1. Validate input
  if !input_dir.is_dir() {
    return Err("input_dir not a directory");
  }
  if !input_dir.join("index.html").exists() {
    return Err("index.html not found in input_dir");
  }

  // 2. Create output file
  let output_file = File::create(&output_path)?;
  let mut zip = ZipWriter::new(output_file);

  // 3. Walk directory tree
  for entry in WalkDir::new(&input_dir)
    .into_iter()
    .filter_map(|e| e.ok())
  {
    let path = entry.path();

    // Skip directories (ZIP includes them implicitly)
    if path.is_dir() {
      continue;
    }

    // Calculate relative path inside ZIP
    let rel_path = path.strip_prefix(&input_dir)?;

    // Add file to ZIP
    let options = FileOptions::default()
      .compression_method(zip::CompressionMethod::Deflated)
      .compression_level(Some(6));

    zip.start_file(rel_path.to_string_lossy(), options)?;

    let mut file = File::open(path)?;
    std::io::copy(&mut file, &mut zip)?;
  }

  // 4. Finalize
  zip.finish()?;

  // 5. Log stats
  let meta = output_path.metadata()?;
  info!("Packaged {} → {} ({} MB)",
        input_dir.display(),
        output_path.display(),
        meta.len() as f64 / 1024.0 / 1024.0);

  Ok(())
}
```

### 3.5 错误处理设计

```rust
// Custom Error Type
#[derive(thiserror::Error, Debug)]
pub enum DeployError {
  #[error("Git operation failed: {reason}")]
  GitFailed { reason: String, code: i32 },

  #[error("Version format invalid: expected YYYYMMDD, got {input}")]
  InvalidVersionFormat { input: String },

  #[error("File operation failed: {path}: {source}")]
  FileError {
    path: PathBuf,
    #[from]
    source: std::io::Error,
  },

  #[error("Build failed: {reason}")]
  BuildFailed { reason: String },

  #[error("Package creation failed: {reason}")]
  PackageFailed { reason: String },

  #[error("Configuration error: {reason}")]
  ConfigError { reason: String },

  #[error("Network error: {reason}")]
  NetworkError { reason: String },
}

// 使用方式
fn main() -> Result<()> {
  let config = load_config().context("Failed to load config")?;
  let version = generate_version(&config)
    .context("Failed to generate version")?;

  Ok(())
}
```

### 3.6 日志与输出格式

**Text 格式（默认，用于终端）**

```
2026-05-09 14:30:15 [INFO]  ✓ Environment check passed
2026-05-09 14:30:16 [INFO]  → git pull origin master
2026-05-09 14:30:18 [INFO]  ✓ Git synchronized
2026-05-09 14:30:19 [INFO]  → npm install
2026-05-09 14:31:00 [INFO]  ✓ Dependencies installed
2026-05-09 14:31:01 [INFO]  → Generating version: 20260509
2026-05-09 14:31:01 [INFO]  ✓ Version files written
2026-05-09 14:31:02 [INFO]  → npm run build
2026-05-09 14:35:00 [INFO]  ✓ Build completed (5 mins)
2026-05-09 14:35:01 [INFO]  → Packaging WGT
2026-05-09 14:35:15 [INFO]  ✓ Packaged 42.5 MB
2026-05-09 14:35:16 [INFO]  ✓ Deploy completed (5 mins)
```

**JSON 格式（用于 CI/CD）**

```json
{
  "timestamp": "2026-05-09T14:30:15Z",
  "event": "deploy_start",
  "mode": "app",
  "config": {
    "version_granularity": "date",
    "git_sync": true,
    "dry_run": false
  }
}

{
  "timestamp": "2026-05-09T14:30:18Z",
  "event": "step_completed",
  "step": "preflight",
  "status": "ok",
  "duration_ms": 3000
}

{
  "timestamp": "2026-05-09T14:35:16Z",
  "event": "deploy_completed",
  "status": "ok",
  "total_duration_ms": 300000,
  "output": {
    "wgt_path": "../H58991839.wgt",
    "wgt_size_mb": 42.5,
    "version": "20260509"
  }
}
```

---

## 第四部分：迁移策略

### 4.1 灰度替换方案（分 3 阶段）

**阶段 1：兼容包装层（1-2 周）**

```batch
:: deploy.bat (修改后)
@echo off
cd /d "%~dp0"
REM 转调 Rust CLI 而非直接执行 Node 脚本
cargo run --release -- deploy --mode app
if errorlevel 1 (
  echo Deployment failed with code %errorlevel%
  exit /b %errorlevel%
)
echo Deployment completed successfully
```

**阶段 2：并行验证（2-4 周）**

- 保留旧 bat 脚本，新增 Rust CLI
- 关键发布时同时运行两套，比对输出
- 修复发现的差异

**阶段 3：完全切换（第 4 周后）**

- 删除 node 版本脚本（start.js, version_json\*.js）
- 仅保留 Rust CLI

### 4.2 运维手册（摘要）

**旧命令 ↔ 新命令对照**

```
旧：deploy.bat                    → 新：cargo run -- deploy --mode app
旧：deploy-app.bat              → 新：cargo run -- deploy --mode app-ts
旧：install.bat                 → 新：cargo run -- install
旧：build.bat                   → 新：cargo run -- build
旧：node version_json.js        → 新：cargo run -- version
旧：(手工验证)                  → 新：cargo run -- verify
旧：(手工上传 FTP)              → 新：cargo run -- upload
```

**故障恢复**

- 如果 Rust CLI 出错，回退至 old\_\*.bat（备份）
- version.json 写入失败时，不会删除原文件（设计）

---

## 第五部分：测试计划

### 5.1 单元测试

```
tests/unit/
├── version_manager_test.rs
│   ├── test_format_date_version()
│   ├── test_format_timestamp_version()
│   └── test_invalid_version_format()
├── file_ops_test.rs
│   ├── test_atomic_write_success()
│   ├── test_atomic_write_cleanup_on_error()
│   └── test_version_consistency_check()
├── zipper_test.rs
│   ├── test_create_wgt_package()
│   ├── test_directory_structure_preserved()
│   └── test_zip_integrity()
└── config_test.rs
    ├── test_cli_args_override_config()
    ├── test_env_var_override()
    └── test_default_values()
```

### 5.2 集成测试

```
tests/integration/
├── deploy_flow_test.rs
│   ├── test_full_deploy_dry_run()
│   ├── test_deploy_with_git_sync()
│   └── test_deploy_skip_git()
├── version_consistency_test.rs
│   ├── test_three_version_files_identical()
│   └── test_src_js_version_matches()
└── backward_compat_test.rs
    ├── test_wgt_installable_by_plus_runtime()
    └── test_app_can_fetch_version_json()
```

### 5.3 验收标准

✓ 生成的 WGT 可被现有 App 通过 plus.runtime.install() 安装
✓ 版本比较逻辑不变（serverVersion !== appVer）
✓ 三份 version.json 内容完全一致
✓ 日志输出可被人工和 CI 工具无缝消费
✓ 错误时非 0 exit code，message 清晰可操作

---

## 第六部分：时间与资源估算

| 阶段      | 任务                    | 估计                        | 关键路径       |
| --------- | ----------------------- | --------------------------- | -------------- |
| 设计      | 架构定稿、接口契约      | 2 天                        | ✓ 阻塞后续     |
| 实现-核心 | 版本、zip、git/npm 封装 | 5-7 天                      | ✓ 并行就绪     |
| 实现-编排 | deploy、verify、upload  | 3-4 天                      | 依赖核心       |
| 集成测试  | 单测 + 集成 + 回归      | 3-4 天                      | 可与实现并行   |
| 灰度验证  | bat 包装、并行运行      | 2-4 周                      | 低风险生产验证 |
| **总计**  |                         | **2-3 周**（Rust 代码交付） | 灰度再 2-4 周  |

---

## 第七部分：风险与缓解

| 风险                               | 影响                  | 缓解方案                                         |
| ---------------------------------- | --------------------- | ------------------------------------------------ |
| Rust zip crate 在大文件上性能差    | 打包耗时剧增          | 第 1 阶段用 7z 兜底；后续根据实测优化            |
| Windows 路径分隔符（\ vs /）不兼容 | 打包后 ZIP 内路径乱码 | 统一用 PathBuf.to_string_lossy() + forward slash |
| 回滚时需从备份恢复 version.json    | 重放构建困难          | 设计完全幂等，存储时间戳参数便于重放             |
| 同日多发版本冲突（日期版限制）     | 无法短时间多次发布    | 文档说明；或切换到时间戳版                       |
| Git reset --hard 删除本地改动      | 手工部署时误操作      | 显著日志警告 + 文档强调                          |
