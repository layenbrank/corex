# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

See also [Conventional Commits](https://www.conventionalcommits.org/).

---
## [6.0.1](https://github.com/layenbrank/corex/compare/v6.0.0..v6.0.1) - 2026-09-03

### 🐛 Bug Fixes
- **(engine)** 多轮解析 variables 交叉引用，消除偶发未定义 · ([`f92e3c7`](https://github.com/layenbrank/corex/commit/f92e3c7d56d0acd0e81ecd6be1d29382f9e6a40b)) · lh


### 📦 Release
- **(release)** bump version to 6.0.1

---
## [6.0.0](https://github.com/layenbrank/corex/compare/v5.3.1..v6.0.0) - 2026-09-01

### 🚀 Features
- **(engine)** 用 throttle 串联 FS debounce 并拒绝 cooldown_ms · ([`4409234`](https://github.com/layenbrank/corex/commit/440923454ec6ea79107bd79690cae2deebd7beb0)) · lh


### 🐛 Bug Fixes
- **(cli)** 对齐 typed AuditEntry 写入路径 · ([`6db350e`](https://github.com/layenbrank/corex/commit/6db350efcf488b7ab6b1a26abced4bddcf7f289d)) · lh
- **(engine)** 修复 watch 竞态并在 cron 退出时 unregister · ([`0b2de70`](https://github.com/layenbrank/corex/commit/0b2de70080009e084d44df57e40ad6fe6dc9c6d1)) · lh


### 📚 Documentation
- 同步 find_*、throttle_ms 与审计字段说明 · ([`b6ffa50`](https://github.com/layenbrank/corex/commit/b6ffa5020b424cb0235da0c0d372f3896e40c128)) · lh


### ♻️ Refactoring
- **(core)** 提供稳定错误 kind 并重命名 ActionStore 查询 API · ([`1bdf6d8`](https://github.com/layenbrank/corex/commit/1bdf6d879221b9234c64f8c26e93ae2884e707bd)) · lh
- **(core)** 将 get_variable/get_path 重命名为 find_* · ([`c9b9859`](https://github.com/layenbrank/corex/commit/c9b9859e3fb39cdee533fd0800136dc6811a6d2f)) · lh
- **(engine)** 用 typed error 驱动 audit/history 与 on_error · ([`d6e9d0b`](https://github.com/layenbrank/corex/commit/d6e9d0b263d49f24877454748052e8585e7b43bd)) · lh


### 🎨 Style
- 对工作区 Rust 源码统一 rustfmt · ([`c2ea6fc`](https://github.com/layenbrank/corex/commit/c2ea6fc0bccad19344fdd13fb5107bb81d035f7c)) · lh


### 📦 Release
- **(release)** bump version to 6.0.0 · ([`052c5a1`](https://github.com/layenbrank/corex/commit/052c5a1e6c2dd9325b3ba9a3f5397d51ff97e250)) · lh


### 🧰 Tooling
- **(tooling)** 引入 pre-commit、cargo-deny、typos 与 git-cliff · ([`a9218cd`](https://github.com/layenbrank/corex/commit/a9218cdf23ab0f5388c8e028af4b4f69f1e5e75c)) · lh

---
## [5.3.1](https://github.com/layenbrank/corex/compare/v5.3.0..v5.3.1) - 2026-09-01

### 🐛 Bug Fixes
- **(registry)** Windows 启动进程时抑制控制台闪窗 · ([`11d283f`](https://github.com/layenbrank/corex/commit/11d283fc8945148ff142975bf73119f940f585a7)) · lh


### 📦 Release
- **(release)** bump workspace to 5.3.1 · ([`d51352f`](https://github.com/layenbrank/corex/commit/d51352fe2055d9f2df8aaf37e0d07a006cb45ba1)) · lh

---
## [5.3.0](https://github.com/layenbrank/corex/compare/v5.2.0..v5.3.0) - 2026-09-01

### 🚀 Features
- **(cron)** 支持触发器与运行时默认时区 · ([`56f25b8`](https://github.com/layenbrank/corex/commit/56f25b8b83c5486af161c2ddec9ebabc0c6957db)) · lh


### 📚 Documentation
- 补充 cron 表达式参考并说明时区配置 · ([`94b566f`](https://github.com/layenbrank/corex/commit/94b566fbe00e917452866741dac7a95d6389f431)) · lh


### 📦 Release
- **(release)** bump workspace to 5.3.0 · ([`46761dc`](https://github.com/layenbrank/corex/commit/46761dc765367d73c560b32bb93096d68b699cbb)) · lh

---
## [5.2.0](https://github.com/layenbrank/corex/compare/v5.1.0..v5.2.0) - 2026-08-29

### 🚀 Features
- **(file)** 统一 file/dir CRUD 并提供迷你 IDE 编辑面 · ([`51de096`](https://github.com/layenbrank/corex/commit/51de09673f6bc8c5ee037742086f6c3e68ffe3f1)) · layen
- **(ipc)** 优先可写 exe 目录并统一端点命名 · ([`c16bf46`](https://github.com/layenbrank/corex/commit/c16bf46c26d6633c6622cb45d20bab36d3767bfb)) · layen
- **(registry)** 拆分 UI 模块并补齐 PC 自动化 Action · ([`f8bbee8`](https://github.com/layenbrank/corex/commit/f8bbee8b8d2fa26f6779edc9e673026b7d5ca5bd)) · layen


### 📚 Documentation
- 同步 file 迷你 IDE 与 PC 自动化说明 · ([`2053d1c`](https://github.com/layenbrank/corex/commit/2053d1c0a3caa4baeec8a2571b9c25a9c526fb1d)) · layen


### 📦 Release
- **(release)** bump workspace to 5.2.0 · ([`39a3284`](https://github.com/layenbrank/corex/commit/39a3284d523842402e0c6bc39716c14958a55b9e)) · layen

---
## [5.1.0](https://github.com/layenbrank/corex/compare/v5.0.0..v5.1.0) - 2026-08-28

### 🚀 Features
- **(cli)** watch/cron 支持 attach、logs 与指令名操作 · ([`888921e`](https://github.com/layenbrank/corex/commit/888921eba626d8eabcffd4ae7a9aabee12f8d708)) · lh
- **(cli)** 将 watch/cron start 合并为 run · ([`6422b99`](https://github.com/layenbrank/corex/commit/6422b9912ab34418049d9759fa9e2f508a0a902e)) · lh
- **(engine)** supervisor 启动时解析 watch/cron 占位符 · ([`05004fd`](https://github.com/layenbrank/corex/commit/05004fd79d69c55d7f362ce75d084194eef940a7)) · lh
- **(engine)** job 按指令名解析并写入 supervisor 日志 · ([`c40d7ee`](https://github.com/layenbrank/corex/commit/c40d7ee78c775f4cb2f25d10a86cff32afa6a08c)) · lh
- **(engine)** 强化 supervisor 身份校验与 watch 事件管道 · ([`a708728`](https://github.com/layenbrank/corex/commit/a7087285a0da5e9e155278da4f9ceddfabd0ee38)) · lh


### 📚 Documentation
- **(examples)** 补充单 Action 示例与公网可跑指令 · ([`d6471ec`](https://github.com/layenbrank/corex/commit/d6471ecdb0248a4289a4e1332951578dc9379f47)) · lh
- 更新 watch/cron CLI 使用说明 · ([`951e880`](https://github.com/layenbrank/corex/commit/951e880d68f6ae35f67a506a20766d571aed9420)) · lh
- 同步 supervisor/watch 架构与 CLI 变更说明 · ([`4647302`](https://github.com/layenbrank/corex/commit/4647302c4987320516f2ec541bf64441cd05860d)) · lh
- 按用途分层并汉化文档导航 · ([`1675212`](https://github.com/layenbrank/corex/commit/16752120e3f609db10551195a13433a0f58b9a26)) · lh


### 📦 Release
- **(release)** bump workspace to 5.1.0 · ([`7f9d245`](https://github.com/layenbrank/corex/commit/7f9d245ff9b050b1e5286f2f990c4b75a50a82df)) · lh

---
## [5.0.0](https://github.com/layenbrank/corex/compare/v2.1.5..v5.0.0) - 2026-08-27

### 🚀 Features
- **(capture)** 重构截图模块为通用捕获模块 · ([`ef18243`](https://github.com/layenbrank/corex/commit/ef1824371576578936ebae73afc37c5ade1cf123)) · layen
- **(capture)** 通过格式和质量选项增强截图功能 · ([`79fd740`](https://github.com/layenbrank/corex/commit/79fd740a9755c9d5a70ac0ef8efd0f458dc4cbc9)) · layen
- **(cli)** add v5 directive runner and REPL updates · ([`843d36d`](https://github.com/layenbrank/corex/commit/843d36d5ce2d4578dfb77b4df3b79b6f0fc95c82)) · lh
- **(cli)** 新增 corex ui 交互式探针命令 · ([`ada7d3e`](https://github.com/layenbrank/corex/commit/ada7d3edb170dcd876bcd6fad9b6c381fd636385)) · layen
- **(core)** add path confinement, ui session, and v5 helpers · ([`e0246de`](https://github.com/layenbrank/corex/commit/e0246ded1e65e33e78e06fc02a4bd32920a8d092)) · lh
- **(core)** 新增 UI 运行时配置与选择器链上限 · ([`3050831`](https://github.com/layenbrank/corex/commit/3050831aa695a41682bac9c543abc7a4a5f225af)) · layen
- **(daemon)** auth token, path confinement, and full config load · ([`6715ac3`](https://github.com/layenbrank/corex/commit/6715ac31295914a80f98bce068e21a333425ca9a)) · Cursor Agent
- **(daemon)** align invoke path with v5 runtime config · ([`6c9df83`](https://github.com/layenbrank/corex/commit/6c9df83350548ea9daaae579f5b93297b4f85079)) · lh
- **(daemon)** 从 corex.toml 接入运行时 UI 覆盖项 · ([`1eea316`](https://github.com/layenbrank/corex/commit/1eea3166795f0dd830230cc27f417a47827663a3)) · layen
- **(engine)** concurrent parallel steps with context merge · ([`f12b9e6`](https://github.com/layenbrank/corex/commit/f12b9e674329116e7822d4478bb756b74a5c8b88)) · Cursor Agent
- **(engine)** add input defaults, audit trail, and v5 pipeline · ([`7cb6cac`](https://github.com/layenbrank/corex/commit/7cb6cac655dbf91aa9610856b5aa77baf411abd5)) · lh
- **(enterprise)** P0–P3 门禁对齐、路径沙箱与 history 收敛 · ([`04f2113`](https://github.com/layenbrank/corex/commit/04f2113bfd0fbf10841c9a2acc0972f6a01d0e1f)) · Cursor Agent
- **(http)** 以 http.send 替换 http.request 并扩展请求参数 · ([`60b6dae`](https://github.com/layenbrank/corex/commit/60b6daef7d630c79ff4550e00466e062d3f8062e)) · lh
- **(morph)** 添加PDF页面垂直堆叠功能 · ([`cc39fbf`](https://github.com/layenbrank/corex/commit/cc39fbff94467bdd45edfd2ac74434e581350554)) · layen
- **(registry)** unify process launch and Windows UI automation · ([`a620d42`](https://github.com/layenbrank/corex/commit/a620d427b395694888824bfc88692ba527580d2e)) · lh
- **(registry)** 增强 UI 元素检测并新增 probe/pick 探针 · ([`d7e3075`](https://github.com/layenbrank/corex/commit/d7e30756291e2d1b3c947f7fb3e16867572e9164)) · layen
- **(shell)** shell.run 执行时实时输出 stdout/stderr · ([`d109955`](https://github.com/layenbrank/corex/commit/d1099553e60c7816149c6b4d889900ff7769ec91)) · lh
- **(triggers)** watch/cron 触发器、supervisor 与 CLI 管理 · ([`36cd4df`](https://github.com/layenbrank/corex/commit/36cd4df86aecfb2c894d12b457bc32213bb8e629)) · lh
- **(ui)** 企业级 UI Inspector 双模式 CLI + pick 修复 · ([`f3c5145`](https://github.com/layenbrank/corex/commit/f3c51457bbce3532e568bf06f2dd616b4380ea55)) · Cursor Agent
- scaffold corex enterprise architecture (v4 workspace) · ([`1a4c9c6`](https://github.com/layenbrank/corex/commit/1a4c9c68c10d6862a7f5cd21a82ae414d1b0401a)) · Cursor Agent
- P3 WASM host skeleton + P5 hardening, docs, and CI · ([`14a534b`](https://github.com/layenbrank/corex/commit/14a534b4cc187cbd9880247e8ae0b6e62a2ac823)) · Cursor Agent
- Windows Named Pipe IPC, CLI REPL, and monolith cleanup · ([`b980469`](https://github.com/layenbrank/corex/commit/b9804696a5e5a1266d4135c3097d9c4a34fa5f8b)) · Cursor Agent


### 🐛 Bug Fixes
- **(capture)** xcap 启用 wgc feature 修复 HiDPI 截图模糊 · ([`4ea747d`](https://github.com/layenbrank/corex/commit/4ea747d18b57999c892039f55ee516fe68719954)) · layen
- **(cli)** treat missing auth token as daemon stopped · ([`d4e2b3a`](https://github.com/layenbrank/corex/commit/d4e2b3a52e5038448b22ba4ebcc64a7b0207cab2)) · Cursor Agent
- **(core)** Windows 路径输出统一使用平台分隔符 · ([`b8eddf0`](https://github.com/layenbrank/corex/commit/b8eddf05846891d271e86f15e924f425e0dd8262)) · lh
- **(ipc)** pass Path to connect_by_path for ToWtf16 on Windows · ([`cceded9`](https://github.com/layenbrank/corex/commit/cceded919941e9fb66a670b2a7580997e25406fa)) · Cursor Agent
- **(ipc)** centralize platform data directory · ([`116fc09`](https://github.com/layenbrank/corex/commit/116fc09e62662114b51362c21c2e7da96c90bab4)) · lh
- **(registry)** drop suggest.bing and adapt builtins to new deps · ([`2e358ec`](https://github.com/layenbrank/corex/commit/2e358ec3db5a454365d94d3b8ac2ed8ee70ccaee)) · lh
- **(skill)** include auth_token in IPC Invoke template · ([`125541b`](https://github.com/layenbrank/corex/commit/125541b16967d64b1e9bc9e7dcfc1798b775f110)) · Cursor Agent
- **(ui)** 修复 Windows 上 ui_probe Tree 格式的 E0382 部分移动 · ([`6e80eda`](https://github.com/layenbrank/corex/commit/6e80eda66564f5bdf1f706b9b7ccb6f5d13c2961)) · Cursor Agent
- **(ui)** 落实 code review 企业门禁与桌面/CLI 行为 · ([`1156961`](https://github.com/layenbrank/corex/commit/115696123f93d3dce556d46b8c1b3ae12b93350b)) · Cursor Agent
- **(ui)** Tree 格式路径将 Value 声明为 mut · ([`03ae1e5`](https://github.com/layenbrank/corex/commit/03ae1e532d7e2f05e9c38b1872b8cd1902572803)) · Cursor Agent
- **(ui)** FindWindowExW 传 HWND 而非 Option<HWND> · ([`0c1330e`](https://github.com/layenbrank/corex/commit/0c1330e689b8b7a0440d3158fb1a2b337339a681)) · Cursor Agent
- **(windows)** 剥离 launch 路径的 /?\ 前缀 · ([`4d065b5`](https://github.com/layenbrank/corex/commit/4d065b57743ca6e87de2b5f3b30d21067a82aa8a)) · Cursor Agent
- compile registry full+wasm with migrated builtins · ([`6ff4069`](https://github.com/layenbrank/corex/commit/6ff4069481d5a74a9d648c63bc69ca9b51ba7bbf)) · Cursor Agent
- harden IPC/engine behavior and align v4 documentation · ([`efec776`](https://github.com/layenbrank/corex/commit/efec776305d701af5ec1eb1e3dc51994c0ede977)) · Cursor Agent


### 📚 Documentation
- sync add-module skill copies and record test results · ([`da3affa`](https://github.com/layenbrank/corex/commit/da3affa54d46b5c11f252760ac9b5bf9b0ba1ddf)) · Cursor Agent
- add v5 directive guides and enterprise compliance · ([`3e5b54b`](https://github.com/layenbrank/corex/commit/3e5b54b4001274565693b4c3f14de37b9b358e77)) · lh
- 补充 corex ui 探针与运行时 UI 预设文档 · ([`4259c23`](https://github.com/layenbrank/corex/commit/4259c230b9792a0dc0d0f6cee4dc3196f00222dd)) · layen
- v5 文档、示例与插件说明更新 · ([`5ee6d07`](https://github.com/layenbrank/corex/commit/5ee6d078231dac145ef0534b680e2017a9887e3e)) · lh


### ♻️ Refactoring
- **(engine)** 统一默认值辅助函数命名为 init_* · ([`e63db39`](https://github.com/layenbrank/corex/commit/e63db3913107dbbf9fb3a818616c536c28ff7654)) · layen
- **(ipc)** 将 default_endpoint 重命名为 platform_endpoint · ([`ed5bd6e`](https://github.com/layenbrank/corex/commit/ed5bd6e0729443e357f8d9c7b6e8904446f256f5)) · layen


### 🔧 Miscellaneous
- **(config)** add enterprise runtime profile defaults · ([`99bf700`](https://github.com/layenbrank/corex/commit/99bf700f8d9176f886cfe1c83c74a4c273205888)) · lh
- **(config)** 将 default.toml 更名为 corex.toml · ([`ba7f310`](https://github.com/layenbrank/corex/commit/ba7f310177963adf3d6bb5c064fdd864407aef01)) · layen
- **(examples)** migrate shortcuts to directives layout · ([`8529570`](https://github.com/layenbrank/corex/commit/852957069548dba6519fb994d31a2f1b5cf4138b)) · lh
- **(examples)** 新增 Win11 记事本 UI 冒烟测试 directive · ([`6396caa`](https://github.com/layenbrank/corex/commit/6396caa2f70bcafc075e9633146f09ff3e27f1aa)) · layen
- **(plans)** 记录 UI 元素点击选择实现计划 · ([`ac4d52c`](https://github.com/layenbrank/corex/commit/ac4d52c03cee20fc6a04081a15fd439198c11bdb)) · layen
- **(skills)** update corex-add-module for directive v5 · ([`2e4e4c9`](https://github.com/layenbrank/corex/commit/2e4e4c9b9a669fd472c93522d6a17e8e5f86971c)) · lh
- **(windows)** 升级 windows crate 至 0.62.2 · ([`f78195e`](https://github.com/layenbrank/corex/commit/f78195e64b4b4daf68dd7d7d9d66e12d2ffd3d74)) · Cursor Agent
- **(windows)** 按 act-* 拆分 windows features · ([`ec93dc1`](https://github.com/layenbrank/corex/commit/ec93dc1f24014887b63565bcabafb750312c414f)) · Cursor Agent
- 去掉 launch 中无用的 mut · ([`62add95`](https://github.com/layenbrank/corex/commit/62add95bb84373f89e674871b8a068abb625d475)) · Cursor Agent

---
## [2.1.5](https://github.com/layenbrank/corex/compare/v2.1.4..v2.1.5) - 2026-08-07

### 🐛 Bug Fixes
- **(serve)** Named Pipe 握手失败时重试而非退出 · ([`c8411d8`](https://github.com/layenbrank/corex/commit/c8411d8504ed97b9fd6b341399a59216eb476835)) · lh


### 🔧 Miscellaneous
- bump version to 2.1.5 for tag release · ([`a7a8ebd`](https://github.com/layenbrank/corex/commit/a7a8ebd26135881d7a30fabb61d376f44b5dbccc)) · lh

---
## [2.1.4](https://github.com/layenbrank/corex/compare/v2.1.2..v2.1.4) - 2026-08-06

### 🚀 Features
- **(morph)** 对齐 IPC 短字段并补页整理操作 · ([`ee6d83d`](https://github.com/layenbrank/corex/commit/ee6d83dccbc8de426968809efad25ee8f137b5be)) · layen


### 🐛 Bug Fixes
- **(pdfium)** 抽出独立构建辅助 crate · ([`1ac4949`](https://github.com/layenbrank/corex/commit/1ac4949e2a8a198225321480892016c9a785c712)) · layen


### 🔧 Miscellaneous
- bump version to 2.1.3 for tag release · ([`ed23b2f`](https://github.com/layenbrank/corex/commit/ed23b2fe99b1d97130099b008fde5230f709a0d9)) · layen
- bump version to 2.1.4 for tag release · ([`fe069c5`](https://github.com/layenbrank/corex/commit/fe069c5abb1d27cdf0e87fd2ce033ee4ae2d56a0)) · layen

---
## [2.1.2](https://github.com/layenbrank/corex/compare/v2.1.1..v2.1.2) - 2026-08-05

### 🐛 Bug Fixes
- **(invoke)** 按 feature 门控 InvokeContext 的 pipeline/serve 字段 · ([`3304ddb`](https://github.com/layenbrank/corex/commit/3304ddb2e3a88df8ae572d7cecc5fd4c243e51df)) · lh
- **(pdfium)** 钉死绑定版本 7881 并与 DLL 资产对齐 · ([`58b738f`](https://github.com/layenbrank/corex/commit/58b738f17cd597fc95f2fbce0949caa3a8dca135)) · lh


### 🔧 Miscellaneous
- bump version to 2.1.2 for tag release · ([`c6677e0`](https://github.com/layenbrank/corex/commit/c6677e0f9ae1b4e5ca611bb6ae66a24bd9d3ade1)) · lh

---
## [2.1.1](https://github.com/layenbrank/corex/compare/v2.1.0..v2.1.1) - 2026-08-05

### 🐛 Bug Fixes
- **(ci)** 修复发布打包步骤的 PowerShell 变量插值 · ([`c658caf`](https://github.com/layenbrank/corex/commit/c658caf9fbba4dede5b472bc05315da0944db8e4)) · lh
- **(exec)** 优先使用 pwsh 执行 ps1 脚本 · ([`73129da`](https://github.com/layenbrank/corex/commit/73129da3ce6fcc6f28e913ac376cb9eb5f4fe284)) · lh


### 🤖 CI
- 强化 Windows x64 企业级发布流程 · ([`a97df67`](https://github.com/layenbrank/corex/commit/a97df67a1e1f52ce5b1a35e570a1d98155253ea7)) · lh


### 🔧 Miscellaneous
- **(cursor)** 升级 planning-with-files 至 3.9.0 · ([`23111a1`](https://github.com/layenbrank/corex/commit/23111a1fc8c71e304507ba7a89d7ec4ff091e088)) · lh
- bump version to 2.1.1 for tag release · ([`aeb408b`](https://github.com/layenbrank/corex/commit/aeb408b4ffb46543379194902637dce8ca551e01)) · layen

---
## [2.1.0](https://github.com/layenbrank/corex/compare/v2.0.6..v2.1.0) - 2026-07-27

### 🚀 Features
- 增强脚本执行功能，支持实时输出流处理 · ([`aaf0f44`](https://github.com/layenbrank/corex/commit/aaf0f4492969b02da16499663533be95d16ccb3b)) · lh
- 添加 engine 模块以支持 Bing 搜索建议和生成 CVID · ([`efc2b10`](https://github.com/layenbrank/corex/commit/efc2b102a70d3c349ed1675983d78b692ddcfcff)) · layen


### 🐛 Bug Fixes
- 移除导致 CI 失败的 notify_flood_probe · ([`3e5ff7b`](https://github.com/layenbrank/corex/commit/3e5ff7be28335ba50da5f0079ca4a9429a034b65)) · lh


### ♻️ Refactoring
- 重构模块架构，优化参数解析与命名约定 · ([`d8e6f22`](https://github.com/layenbrank/corex/commit/d8e6f226ca004ea0eee42efc9e2737b163f980b5)) · lh


### 🔧 Miscellaneous
- 更新版本号至 2.0.7，优化 Cargo.toml 格式 · ([`0290faa`](https://github.com/layenbrank/corex/commit/0290faa11dafbce5b323a95e7e71c5098f20d7da)) · lh
- 移除不再使用的批处理和配置文件 · ([`97e7fdc`](https://github.com/layenbrank/corex/commit/97e7fdc3995be156049ed74e65b85d9fe33f1e00)) · lh
- 更新版本号至 2.1.0，调整 Cargo.toml 配置 · ([`0480587`](https://github.com/layenbrank/corex/commit/04805879688ce73e325a2d648f501422cecd4a76)) · layen

---
## [2.0.6](https://github.com/layenbrank/corex/compare/v2.0.5..v2.0.6) - 2026-07-13

### 🚀 Features
- 添加冷却时间配置以优化 watch 功能 · ([`40381ec`](https://github.com/layenbrank/corex/commit/40381ecdbd8817dccd3fd498aad3b6f9506c8353)) · lh
- 移除 Handlebars 依赖并引入 exec 模块 · ([`1eff59a`](https://github.com/layenbrank/corex/commit/1eff59a30a5c8f8c6ddc37926ff4dd75b6a07038)) · lh

---
## [2.0.5](https://github.com/layenbrank/corex/compare/v2.0.4..v2.0.5) - 2026-07-13

### 🚀 Features
- 增强变量解析功能，支持嵌套引用 · ([`182b5b3`](https://github.com/layenbrank/corex/commit/182b5b3f5de65cb8e8fcd19fbbdf6f13c979b279)) · lh

---
## [2.0.4](https://github.com/layenbrank/corex/compare/v2.0.3..v2.0.4) - 2026-07-12

### 🚀 Features
- 增强 PDFium 支持与 CI/CD 工作流 · ([`8615323`](https://github.com/layenbrank/corex/commit/86153231ddd843f1f4a2a7d3db50e2c5bd128f9a)) · layen

---
## [2.0.3](https://github.com/layenbrank/corex/compare/v2.0.2..v2.0.3) - 2026-07-11

### 🔧 Miscellaneous
- 更新 Cargo.toml，调整版本和格式 · ([`bebfbe2`](https://github.com/layenbrank/corex/commit/bebfbe29ca29d2c0e50995f0160aec47c60296dc)) · layen
- 更新依赖版本以提升稳定性和安全性 · ([`f53b846`](https://github.com/layenbrank/corex/commit/f53b84674dca97798b6e038ee5b2a2fb3a5591fd)) · layen
- 更新 CI/CD 工作流以使用最新的 GitHub Actions 版本 · ([`e4f1ab9`](https://github.com/layenbrank/corex/commit/e4f1ab97eaa96c477cd5060b12824eb661aba532)) · layen

---
## [2.0.2](https://github.com/layenbrank/corex/compare/v2.0.1..v2.0.2) - 2026-07-10

### 🚀 Features
- 重命名 corex-shot 为 corex-capture，优化模块结构 · ([`7db7711`](https://github.com/layenbrank/corex/commit/7db7711bb8a7462b7fe7c97ada425c5d7e62e943)) · layen

---
## [2.0.1](https://github.com/layenbrank/corex/compare/v2.0.0..v2.0.1) - 2026-07-10

### 🔧 Miscellaneous
- 更新 CI/CD 配置以支持 sccache · ([`e42c402`](https://github.com/layenbrank/corex/commit/e42c40250bcea474159daa1cacff17f519c86263)) · lh

---
## [2.0.0](https://github.com/layenbrank/corex/compare/v1.0.1..v2.0.0) - 2026-07-10

### 🚀 Features
- **(generate)** 添加文件生成支持，新增模板处理功能及相关参数 · ([`188db37`](https://github.com/layenbrank/corex/commit/188db37edc88fd44519dbda09240d4ab81f14cec)) · lh
- 重构压缩和图像处理任务 · ([`a6d0399`](https://github.com/layenbrank/corex/commit/a6d0399a84cab2c54a1fd4aa5208554242111062)) · lh
- 添加 corex-serve 和 corex-shot 模块，更新依赖和文档 · ([`b0716b2`](https://github.com/layenbrank/corex/commit/b0716b2237d08a7b6d338e6d038ec0c403b2a5ec)) · lh
- 添加文档和任务计划文件，记录 Corex 项目进展与发现 · ([`8e8a708`](https://github.com/layenbrank/corex/commit/8e8a7089ca3d8523d5ab0f62ce532544739ec38c)) · lh
- 更新 .gitignore 和新增 IPC 示例 · ([`09f5f9b`](https://github.com/layenbrank/corex/commit/09f5f9b34af951778417adb00a20a8a376db0323)) · layen
- 更新文档与示例，增强 Corex 与 Tauri 集成 · ([`ab1613c`](https://github.com/layenbrank/corex/commit/ab1613ca29eeab25c805ba452cc06985d91e1b3f)) · lh
- 更新依赖和文档，重构 Corex 项目结构 · ([`a447b5b`](https://github.com/layenbrank/corex/commit/a447b5b313fd19fbfd5ce22cdb1a67b1e2e87c70)) · lh

---
## [1.0.1](https://github.com/layenbrank/corex/compare/v1.0.0..v1.0.1) - 2026-06-12

### 🔧 Miscellaneous
- **(pipelines)** 优化流水线路径配置与调度时间 · ([`447422a`](https://github.com/layenbrank/corex/commit/447422a34527d9a633ce9366556b66a695d5df00)) · lh

---
## [1.0.0](https://github.com/layenbrank/corex/compare/v0.2.9..v1.0.0) - 2026-06-11

### 🚀 Features
- 添加压缩任务支持，更新配置和调度逻辑 · ([`7f2f5b3`](https://github.com/layenbrank/corex/commit/7f2f5b368b5460805773336c66c8ed9dac43d3ee)) · lh
- 更新依赖版本，添加 UUID 生成任务支持，优化调度逻辑 · ([`cf3a3df`](https://github.com/layenbrank/corex/commit/cf3a3dfbbc56a4e65dfafda24c723f9d464cd82d)) · lh
- 更新依赖版本，删除无用的项目设置文件 · ([`3846c81`](https://github.com/layenbrank/corex/commit/3846c81761987479a08291edf4290c955972b3c7)) · lh
- 更新依赖版本，重构路径验证和忽略处理逻辑，优化代码结构 · ([`7598646`](https://github.com/layenbrank/corex/commit/75986467e8877d9e3ca5a16ed13e0f1c14af6f98)) · lh


### ♻️ Refactoring
- **(corex)** 重构核心库并优化命令行架构 · ([`752380f`](https://github.com/layenbrank/corex/commit/752380f612a72bc6aa6d0448f7d8c8abd5d204c9)) · lh
- **(schedule)** 重构任务调度器支持流水线及步骤执行 · ([`95171c3`](https://github.com/layenbrank/corex/commit/95171c360f2c86b57f9033d9e37b467eec5a06f6)) · lh

---
## [0.2.9](https://github.com/layenbrank/corex/compare/v0.2.8..v0.2.9) - 2026-05-14

### 🚀 Features
- 更新 README.md，完善环境配置和参数说明，添加定时任务功能示例 · ([`f6ef714`](https://github.com/layenbrank/corex/commit/f6ef714603d11865114de5ff750063ab41215a93)) · lh
- 更新压缩打包示例，修正输出文件名和版本格式 · ([`7b026e0`](https://github.com/layenbrank/corex/commit/7b026e03275e3080670c62c96a4c46cd09038267)) · lh
- 更新压缩打包功能，完善版本信息文件生成及示例 · ([`0de80e6`](https://github.com/layenbrank/corex/commit/0de80e67e8e2f0d0ef6cadb19e998a90708faece)) · lh

---
## [0.2.8](https://github.com/layenbrank/corex/compare/v0.2.7..v0.2.8) - 2026-05-11

### 🚀 Features
- 在 Rust 编译配置中添加单一代码生成单元以提升优化效果 · ([`7ef0803`](https://github.com/layenbrank/corex/commit/7ef0803c74492bdd96396ac5a84088c9361ade36)) · lh
- 添加支持命令行界面 (CLI) 的压缩模块 · ([`e199374`](https://github.com/layenbrank/corex/commit/e19937442e08aff120fcfa49b0b8aa5f648f9416)) · lh
- 添加时间戳功能并生成版本文件，更新依赖项 · ([`641c63d`](https://github.com/layenbrank/corex/commit/641c63d782770a6806d77977bdaa08bc0e2529c6)) · lh
- 更新 README.md，添加压缩打包功能说明及示例 · ([`e0a737a`](https://github.com/layenbrank/corex/commit/e0a737a04b00fc94aba0af6a24017032154bb68f)) · lh


### 🔧 Miscellaneous
- 删除 Visual Studio 相关配置文件和索引，清理项目目录 · ([`874929d`](https://github.com/layenbrank/corex/commit/874929dcd79563d3e8bdc12beded4a5fa80b6227)) · lh

---
## [0.2.7](https://github.com/layenbrank/corex/compare/v0.2.6..v0.2.7) - 2026-01-08

### 🚀 Features
- 更新项目描述为 corex-cli · ([`9995fa6`](https://github.com/layenbrank/corex/commit/9995fa60b245329bbb97db03b13950a9afcfdd20)) · lh
- 添加扫描功能，支持目录和文件的扫描 · ([`d5a467f`](https://github.com/layenbrank/corex/commit/d5a467f7ffd7e05c202f0404ac8027b12c6a4275)) · lh
- feat: 添加 bootstrap 功能，支持环境变量的插入和强制更新 · ([`6b221a5`](https://github.com/layenbrank/corex/commit/6b221a5648047d5aea7f7cf37743a2aee41ae16b)) · lh
- 添加 sysinfo 依赖并实现操作系统信息扫描功能 · ([`6dec49d`](https://github.com/layenbrank/corex/commit/6dec49d42af4e1569e4c3b520123bb54c879d434)) · lh
- 添加 .cargo/config.toml 配置，更新 .gitignore，新增 VSCode 任务，更新依赖版本，调整代码结构 · ([`3a91d62`](https://github.com/layenbrank/corex/commit/3a91d62c8c8435886d9f0514fde8afed4866a71b)) · lh
- 添加 thiserror 依赖并重构清理功能，更新 VS 配置文件和索引 · ([`2fc7ec5`](https://github.com/layenbrank/corex/commit/2fc7ec523563cf30aae57a359159cea9ad13203c)) · lh
- 更新 Scrub 任务，添加递归参数并优化代码格式 · ([`9437214`](https://github.com/layenbrank/corex/commit/9437214eb01c0402d0e6126cc7fb4b6e41bc868a)) · lh
- 重构 Scrub 功能，支持异步删除，优化命令行参数，更新依赖 · ([`1b32e7d`](https://github.com/layenbrank/corex/commit/1b32e7dd5a3ee1db1914a6428604b91d6c14a263)) · lh


### 🐛 Bug Fixes
- 修正变量名一致性，确保文件名变量在整个函数中保持一致 · ([`5891db5`](https://github.com/layenbrank/corex/commit/5891db5076779e8510b37c7cd730acc634e11900)) · lh

---
## [0.2.6](https://github.com/layenbrank/corex/compare/v0.2.5..v0.2.6) - 2025-10-26

### 🚀 Features
- 添加用户目录配置文件检查，确保任务配置有效 · ([`b639a2b`](https://github.com/layenbrank/corex/commit/b639a2bdf871eb98392061cbd102c21355d683e1)) · lh
- feat：重构项目结构并添加清理功能 · ([`8c8e6da`](https://github.com/layenbrank/corex/commit/8c8e6da5952dc9ef2d87ac0ff9a892a25ffc6cbe)) · lh

---
## [0.2.4](https://github.com/layenbrank/corex/compare/v0.2.3..v0.2.4) - 2025-08-20

### 🚀 Features
- feat：初始化 corex 项目结构和配置 · ([`7aa7d25`](https://github.com/layenbrank/corex/commit/7aa7d2511a3f5d0c6c565746f541c79e1d27e0c8)) · lh
- 更新发布工作流，添加提交信息获取和通知功能；重构文件处理和进度显示模块 · ([`b52c35e`](https://github.com/layenbrank/corex/commit/b52c35ec713e9f8431da734672a46e138e63b7fe)) · 李贺


### 📚 Documentation
- 更新 README.md，重构环境配置和功能说明，优化文档结构和示例 · ([`4b10d78`](https://github.com/layenbrank/corex/commit/4b10d78842482c47acc14369c0a94508b024ae3c)) · 李贺

---
## [0.1.1](https://github.com/layenbrank/corex/tree/v0.1.1) - 2025-08-15

### 🌀 Other
- copy 命令行工具 · ([`1011a85`](https://github.com/layenbrank/corex/commit/1011a85b83a8ebee2114593cc28669872c122ae5)) · 李贺
- Refactor project structure and implement notification system · ([`693963d`](https://github.com/layenbrank/corex/commit/693963dea6b28c3e02540f9ee8735cfca4df21ab)) · 李贺
- Refactor notification system and reorganize project structure · ([`250d829`](https://github.com/layenbrank/corex/commit/250d829b9345dd767430a816aa0822657e587098)) · 李贺
- fluxor.yml · ([`322cb9c`](https://github.com/layenbrank/corex/commit/322cb9cb8c1e91f0b0a0b47e0014ae4c872fa846)) · layenbrank
- Remove legacy notification examples and related files to streamline the project structure and focus on the updated notification system. This includes deleting old test files, configuration files, and XML templates that are no longer in use. · ([`fc89b31`](https://github.com/layenbrank/corex/commit/fc89b313725223dd1b0ce38ed5ad6b877f9a16e8)) · 李贺

[6.0.0]: https://github.com/layenbrank/corex/compare/v5.3.1..v6.0.0
[5.3.1]: https://github.com/layenbrank/corex/compare/v5.3.0..v5.3.1
[5.3.0]: https://github.com/layenbrank/corex/compare/v5.2.0..v5.3.0
[5.2.0]: https://github.com/layenbrank/corex/compare/v5.1.0..v5.2.0
[5.1.0]: https://github.com/layenbrank/corex/compare/v5.0.0..v5.1.0
[5.0.0]: https://github.com/layenbrank/corex/compare/v2.1.5..v5.0.0
[2.1.5]: https://github.com/layenbrank/corex/compare/v2.1.4..v2.1.5
[2.1.4]: https://github.com/layenbrank/corex/compare/v2.1.2..v2.1.4
[2.1.2]: https://github.com/layenbrank/corex/compare/v2.1.1..v2.1.2
[2.1.1]: https://github.com/layenbrank/corex/compare/v2.1.0..v2.1.1
[2.1.0]: https://github.com/layenbrank/corex/compare/v2.0.6..v2.1.0
[2.0.6]: https://github.com/layenbrank/corex/compare/v2.0.5..v2.0.6
[2.0.5]: https://github.com/layenbrank/corex/compare/v2.0.4..v2.0.5
[2.0.4]: https://github.com/layenbrank/corex/compare/v2.0.3..v2.0.4
[2.0.3]: https://github.com/layenbrank/corex/compare/v2.0.2..v2.0.3
[2.0.2]: https://github.com/layenbrank/corex/compare/v2.0.1..v2.0.2
[2.0.1]: https://github.com/layenbrank/corex/compare/v2.0.0..v2.0.1
[2.0.0]: https://github.com/layenbrank/corex/compare/v1.0.1..v2.0.0
[1.0.1]: https://github.com/layenbrank/corex/compare/v1.0.0..v1.0.1
[1.0.0]: https://github.com/layenbrank/corex/compare/v0.2.9..v1.0.0
[0.2.9]: https://github.com/layenbrank/corex/compare/v0.2.8..v0.2.9
[0.2.8]: https://github.com/layenbrank/corex/compare/v0.2.7..v0.2.8
[0.2.7]: https://github.com/layenbrank/corex/compare/v0.2.6..v0.2.7
[0.2.6]: https://github.com/layenbrank/corex/compare/v0.2.5..v0.2.6
[0.2.4]: https://github.com/layenbrank/corex/compare/v0.2.3..v0.2.4
[0.1.1]: https://github.com/layenbrank/corex/releases/tag/v0.1.1

<!-- generated by git-cliff -->
