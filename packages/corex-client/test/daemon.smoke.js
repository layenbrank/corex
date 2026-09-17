//! 对着**真的 corex-daemon** 跑一遍。默认跳过：需要 `COREX_DAEMON` 指向那个二进制。
//!
//! ```powershell
//! $env:COREX_DAEMON = "$env:USERPROFILE\.cache\rust\debug\corex-daemon.exe"
//! node --test packages/corex-client/test/daemon.smoke.js
//! ```
//!
//! 假 daemon 那套（`client.test.js`）钉的是**读法**；这里钉的是**两边真的能说上话**：
//! 端点发现、token 解析、进度帧、`shutdown` 之后的清理。corex 改了线格式而没同步客户端时，
//! 只有这个文件会红——所以它值得在发版前手动跑一次。

import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { test } from 'node:test';

import { connect, discover } from '../src/index.js';

const DAEMON = process.env.COREX_DAEMON;

/** 一份只服务这次运行的配置：端点与锁都钉住，不碰用户的数据目录。 */
function writeConfig(dir) {
  const name = `corex-client-smoke-${process.pid}`;
  // Windows 上端点只能是命名管道；TOML 用单引号字面量，免得反斜杠被当成转义。
  const endpoint =
    process.platform === 'win32'
      ? `\\\\.\\pipe\\${name}`
      : path.join(dir, `${name}.sock`);
  const config = path.join(dir, 'smoke.toml');
  fs.writeFileSync(
    config,
    ['[daemon]', `socket_path = '${endpoint}'`, `lock_path = 'smoke.lock'`, ''].join('\n'),
  );
  return { config, endpoint };
}

function writeDirective(dir, source, target) {
  const directives = path.join(dir, 'directives');
  fs.mkdirSync(directives, { recursive: true });
  // YAML 里的 Windows 路径得用正斜杠：反斜杠会被当成转义序列。
  const slash = (value) => value.replaceAll('\\', '/');
  fs.writeFileSync(
    path.join(directives, 'probe.yaml'),
    [
      'name: probe',
      'description: 冒烟用',
      'permissions:',
      '  filesystem: true',
      'steps:',
      '  - id: copy',
      '    action: file.copy',
      '    params:',
      `      from: "${slash(source)}"`,
      `      to: "${slash(target)}"`,
      '    save_to: out',
      '',
    ].join('\n'),
  );
}

test('对着真 daemon：发现 → 鉴权 → 目录 → 指令与进度帧 → 收掉', { skip: !DAEMON }, async () => {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'corex-client-smoke-'));
  const { config, endpoint } = writeConfig(dir);

  // 3 MiB：够 file.copy 分三次上报，又不至于让冒烟测试变慢。
  const source = path.join(dir, 'big.bin');
  const target = path.join(dir, 'big-copy.bin');
  fs.writeFileSync(source, Buffer.alloc(3 * 1024 * 1024));
  writeDirective(dir, source, target);

  const client = await connect({
    dataDir: dir,
    spawn: true,
    spawnTimeoutMs: 20000,
    spawnOptions: { daemonPath: DAEMON, args: ['--config', config] },
  });

  try {
    // 端点来自 daemon 写下的记录（不是我们猜的平台默认）；token 来自它创建的 token 文件
    // ——首轮连上时那个文件可能还不存在，所以 `connect` 会在每轮重试里重新解析一次。
    assert.equal(client.endpoint, endpoint);
    const live = discover({ dataDir: dir });
    assert.equal(live.source, 'record');
    assert.match(live.token ?? '', /^[0-9a-f]{64}$/, 'daemon 创建的 token 该被读到');
    assert.equal(await client.ping(), true);

    const actions = await client.actions();
    assert.ok(actions.length > 0, '目录不该是空的');
    const copy = actions.find((entry) => entry.id === 'file.copy');
    assert.deepEqual(copy.permissions, ['filesystem']);
    assert.equal(copy.input_schema.type, 'object');

    const frames = [];
    const result = await client.run('probe', { onProgress: (progress) => frames.push(progress) });
    assert.ok(frames.some((frame) => frame.kind === 'step_start'), `没收到进度帧: ${frames.length}`);
    assert.ok(
      frames.some((frame) => frame.kind === 'step_progress' && frame.unit === 'bytes'),
      '分块拷贝该上报字节进度',
    );
    // `Pipeline::execute` 回的是**最后一步的值**（不是变量表）：单步指令于是直接给出
    // `file.copy` 的目标路径。
    assert.ok(
      typeof result === 'string' && result.endsWith('big-copy.bin'),
      `结果该是最后一步的值: ${JSON.stringify(result)}`,
    );
    assert.equal(fs.statSync(target).size, fs.statSync(source).size);

    // `close()` 会请 daemon 退出——它写下的记录也该随之消失。
    const child = client.daemon?.child;
    await client.close();
    assert.ok(child && child.exitCode !== null, 'daemon 该已经退出');
    assert.equal(fs.existsSync(path.join(dir, 'endpoint.json')), false, '退出后该删掉记录');
  } finally {
    await client.close().catch(() => {});
    fs.rmSync(dir, { recursive: true, force: true });
  }
});
