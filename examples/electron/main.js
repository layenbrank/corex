'use strict';

/**
 * Electron 主进程：建窗口 + 把渲染进程的几个名字接到 host 上。
 *
 * 这里刻意很薄——真正与 corex 打交道的是 `src/corex-host.js`，那一半能脱离 Electron
 * 被 `node scripts/check.mjs` 验证。安全上按 Electron 的推荐来：`contextIsolation` 开、
 * `nodeIntegration` 关、`sandbox` 开，渲染进程只能看见 `preload.js` 暴露的那几个名字。
 *
 * 与 Tauri 示例一样，重依赖（OCR / UIAutomation / WASM）全留在 `corex-daemon` 里，
 * 宿主进程只连一条管道。
 */

const path = require('node:path');
const { app, BrowserWindow, ipcMain } = require('electron');

const { createHost } = require('./src/corex-host');

/**
 * 随应用分发的 sidecar 路径。
 *
 * 打包时把 `corex-daemon` 放进 resources 目录（如 `app.getAppPath()/../corex-daemon.exe`），
 * 这里指向它；留空则用 PATH 里的 `corex-daemon`——开发时最省事。
 */
const DAEMON_PATH = process.env.COREX_DAEMON || undefined;

/** 主进程是被拉起的那一方，窗口还没建好就可能收到帧，所以要判一下。 */
let window = null;

function broadcast(channel, payload) {
  if (window && !window.isDestroyed()) {
    window.webContents.send(channel, payload);
  }
}

const host = createHost({
  daemonPath: DAEMON_PATH,
  onProgress: (frame) => broadcast('corex:progress', frame),
  onLog: (text) => console.log('[corex-daemon]', text.trimEnd()),
});

function createWindow() {
  window = new BrowserWindow({
    width: 960,
    height: 720,
    title: 'Corex × Electron',
    webPreferences: {
      preload: path.join(__dirname, 'preload.js'),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  window.loadFile('index.html');
  window.on('closed', () => {
    window = null;
  });
}

// 渲染进程能调到的就是这几个，一一对应 host 上的方法。加一个能力只改这一处
// 与 preload 里那一处。
ipcMain.handle('corex:status', () => host.status());
ipcMain.handle('corex:start', () => host.start());
ipcMain.handle('corex:stop', () => host.stop());
ipcMain.handle('corex:actions', () => host.actions());
ipcMain.handle('corex:directives', () => host.directives());
ipcMain.handle('corex:invoke', (_event, action, params) => host.invoke(action, params));
ipcMain.handle('corex:run', (_event, name, input) => host.run(name, input));

app.whenReady().then(async () => {
  createWindow();
  // 启动就连；连不上就拉起一个 daemon。**失败不该让窗口打不开**——界面要能把
  // 失败原因显示出来，否则用户只看到一个白窗口。
  try {
    await host.start();
  } catch (error) {
    console.error('[corex] 启动失败:', error.message);
  }
  broadcast('corex:status', host.status());

  app.on('activate', () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createWindow();
    }
  });
});

app.on('window-all-closed', () => {
  if (process.platform !== 'darwin') {
    app.quit();
  }
});

// 谁起的谁收。`before-quit` 不等 async 回调，所以先拦一次、收完再真的退——
// 少了这段，我们自己拉起的 daemon 会变成孤儿进程留在机器上。
let quitting = false;
app.on('before-quit', (event) => {
  if (quitting) {
    return;
  }
  event.preventDefault();
  quitting = true;
  host
    .stop()
    .catch((error) => console.error('[corex] 收尾失败:', error.message))
    .finally(() => app.quit());
});
