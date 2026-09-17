'use strict';

/**
 * 与 Electron 无关的那一半：把 `corex-client` 包成宿主可用的一组操作。
 *
 * 单独成文件是为了它能**脱离 Electron 被验证**——`scripts/check.mjs` 直接用它对着
 * 真 daemon 跑一遍，于是这个示例里唯一没被验证过的部分，就只剩 `main.js` / `preload.js`
 * 里那几行 Electron API 调用。
 *
 * 设计上的两个决定：
 *
 * - **谁起的谁收**：`start()` 连不上就拉起一个 daemon（`spawn: true`），`stop()` 会把它收掉；
 *   连着别人已经在跑的 daemon 时只断开，不动那个进程。
 * - **进度共用一个回调**：一帧带 `step`/`action`，界面按条追下去就够。真要按请求分流，
 *   自己按 `step` 归位即可——客户端已经保证了帧一定排在终帧之前、且带正确的 `id`。
 */

/** 一个 host 实例。`client` 只在测试/检查脚本里显式传，宿主走 node_modules。 */
function createHost({
  client = require('corex-client'),
  daemonPath,
  args,
  dataDir,
  onProgress,
  onLog,
} = {}) {
  let connection = null;
  let lastError = null;

  /** 当前状态；界面上那一行直接用它。 */
  function status() {
    if (!connection) {
      return { state: 'stopped', error: lastError };
    }
    return {
      state: 'running',
      endpoint: connection.endpoint,
      dataDir: connection.dataDir,
      // 自己拉起来的 daemon：退出时该由我们收掉。
      spawned: connection.daemon !== null,
    };
  }

  /** 还没连上就调用某个方法，属于调用方的顺序错误，而不是 corex 的问题。 */
  function connected() {
    if (!connection) {
      throw new Error('corex 尚未连接（先调用 start()）');
    }
    return connection;
  }

  async function start() {
    if (connection) {
      return status();
    }
    try {
      connection = await client.connect({
        dataDir,
        spawn: true,
        spawnOptions: { daemonPath, args },
        onLog,
      });
      lastError = null;
    } catch (error) {
      lastError = error.message;
      throw error;
    }
    // daemon 自己退出时把状态归位，界面才不会一直显示“运行中”。
    connection.onClose(() => {
      connection = null;
    });
    return status();
  }

  async function stop() {
    const current = connection;
    connection = null;
    if (current) {
      await current.close();
    }
    return status();
  }

  return {
    start,
    stop,
    status,
    actions: () => connected().actions(),
    directives: (dir) => connected().directives(dir),
    invoke: (action, params) => connected().invoke(action, params ?? {}, { onProgress }),
    run: (name, input) => connected().run(name, { input: input ?? {}, onProgress }),
  };
}

module.exports = { createHost };
