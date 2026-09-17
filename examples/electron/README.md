# Electron × corex 集成示例

重依赖（OCR / UIAutomation / 进程 / WASM）全留在 **`corex-daemon`** 里，Electron 进程只连一条
管道。宿主因此不必链接任何 native 依赖，也不必自己拼端点、找 token、分流进度帧——后面这三件
由 [`packages/corex-client`](../../packages/corex-client/README.md) 做掉了。

> **接入文档：** [接入总览](../../docs/integration/接入总览.md) · [IPC 接入指南](../../docs/integration/IPC接入指南.md)
> **Tauri 版本：** [`examples/tauri/`](../tauri/README.md)（两侧思路一致，只是 Tauri 侧要用 Rust 客户端）

## 文件清单

| 文件                 | 作用                                                       |
| -------------------- | ---------------------------------------------------------- |
| `src/corex-host.js`  | **与 Electron 无关的那一半**：把客户端包成宿主可用的一组操作 |
| `main.js`            | 建窗口 + 把渲染进程的名字接到 host 上 + 退出时收掉 daemon    |
| `preload.js`         | `contextBridge` 暴露的那组能力（只有函数，没有 channel 名） |
| `index.html` / `renderer.js` | 能真跑的面板：选 Action、按 `input_schema` 预填参数、看进度帧 |
| `scripts/check.mjs`  | **不开 Electron** 也能验证宿主侧逻辑（见下）                |

把 `src/corex-host.js` 单独拆出来不是洁癖：它让「能不能连上、能不能跑指令、退出有没有收掉
daemon」这些**最容易写错**的部分可以在没有 Electron 的环境里验证。

## 构建 sidecar

```powershell
cargo build -p corex-daemon --release
```

打包时把 `corex-daemon.exe` 放进应用资源目录，并让 `main.js` 的 `DAEMON_PATH` 指过去；开发时
用环境变量最省事：

```powershell
$env:COREX_DAEMON = "路径\corex-daemon.exe"
```

## 运行

```powershell
cd examples/electron
npm install        # 需要网络：electron 与 file: 形式的 corex-client
npm start
```

## 不开 Electron 也能验证

```powershell
$env:COREX_DAEMON = "路径\corex-daemon.exe"
node scripts/check.mjs
```

它走的是 `main.js` 用的**同一份** `corex-host.js`，对着真 daemon 检查：发现端点（来自 daemon
写下的记录）、目录带权限与 `input_schema`、`invoke` 与 `run` 的结果、每个步骤的开始帧、
以及 `stop()` 之后 daemon 是否有序退出并删掉端点记录。

所以这个示例里**没被验证过的部分只剩 `main.js` / `preload.js` / `renderer.js` 里那几行
Electron API 调用**——本仓库的 CI 装不了 Electron，但上面这些正是会悄悄写错的地方。

## 几个刻意的选择

**Electron 侧用 CommonJS，客户端是 ESM。** 不是随手写的：ESM 的 preload 脚本要求
`sandbox: false`，而示例想演示推荐的沙箱配置。所以 `corex-host.js` 用
`require('corex-client')` 之外还支持注入（`createHost({ client })`），`check.mjs` 于是能
直接用 `import` 指向 `packages/corex-client/src/index.js`，**clone 下来无需 `npm install` 就能验证**。

**数据目录必须两边一致。** 客户端会把它设成子进程的 `COREX_DATA_DIR`。不设的话父子可能各看
一份目录（Rust 的 `data_dir()` 有一档「二进制所在目录」是 JS 侧看不到的），表现成最难查的
那种「连不上」。随宿主分发 corex 时**务必**设它。

**退出时先收 daemon。** `before-quit` 不等 async 回调，所以 `main.js` 先 `preventDefault()`、
收完再 `app.quit()`；少了这段，自己拉起的 daemon 会变成孤儿进程留在用户机器上。

**渲染进程只拿到能力，不拿到 `ipcRenderer`。** `preload.js` 暴露的是一组函数，channel 名不外泄；
`index.html` 另带一条 CSP（`script-src 'self'`），这也是 Electron 开发时提示缺的那一条。

**参数模板是从目录现推的。** 面板按所选动作的 `input_schema.properties` 预填 JSON
（有 `default` 用默认值，否则按类型给空值）。这正是 `list_actions` 把 schema 一起回给宿主的
用处——宿主不必自己抄一份参数表。

## 指令列表是空的？

`directives()` 列的是 `<数据目录>/directives/*.yaml`。新装的机器上它是空的，可以：

```powershell
corex create hello          # 或把 examples/directives/ 里的一条复制过去
```
