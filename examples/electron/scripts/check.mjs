/**
 * 脱离 Electron 验证这个示例的主机侧逻辑。
 *
 * ```powershell
 * $env:COREX_DAEMON = "$env:USERPROFILE\.cache\rust\debug\corex-daemon.exe"
 * node scripts/check.mjs
 * ```
 *
 * 它走的是 `main.js` 用的**同一份** `src/corex-host.js`，所以这个示例里没被验证过的部分，
 * 就只剩 `main.js` / `preload.js` / `renderer.js` 里那几行 Electron API 调用。
 *
 * 之所以要这么一层：本仓库的 CI 装不了 Electron（示例的 `node_modules` 要 `npm install`），
 * 但「宿主能不能连上、能不能跑一条指令、退出时有没有把 daemon 收掉」这些是**能**验证的，
 * 而且正是最容易写错的地方。
 */

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { createHost } from '../src/corex-host.js';

// 直接指向客户端源码：示例的 node_modules 需要 `npm install`，而这里想做到 clone 下来就能跑。
import * as client from '../../../packages/corex-client/src/index.js';

const DAEMON = process.env.COREX_DAEMON;

let failed = 0;

function check(label, condition, detail = '') {
  if (condition) {
    console.log(`  ok   ${label}`);
    return;
  }
  failed += 1;
  console.log(`  FAIL ${label}${detail ? ` —— ${detail}` : ''}`);
}

/** 一份只服务这次检查的配置：端点与锁都钉住，不碰用户的数据目录。 */
function writeConfig(dir) {
  const name = `corex-electron-example-${process.pid}`;
  const endpoint =
    process.platform === 'win32' ? `\\\\.\\pipe\\${name}` : path.join(dir, `${name}.sock`);
  const config = path.join(dir, 'check.toml');
  fs.writeFileSync(
    config,
    ['[daemon]', `socket_path = '${endpoint}'`, `lock_path = 'check.lock'`, ''].join('\n'),
  );
  return { config, endpoint };
}

/** YAML 里的 Windows 路径得用正斜杠：反斜杠会被当成转义序列。 */
function writeDirective(dir, target) {
  const directives = path.join(dir, 'directives');
  fs.mkdirSync(directives, { recursive: true });
  fs.writeFileSync(
    path.join(directives, 'probe.yaml'),
    [
      'name: probe',
      'description: 示例检查用',
      'permissions:',
      '  filesystem: true',
      'steps:',
      '  - id: render',
      '    action: template.render',
      '    params:',
      '      template: "hi"',
      '    save_to: message',
      '  - id: write',
      '    action: file.write',
      '    params:',
      `      path: "${target.replaceAll('\\', '/')}"`,
      '      content: "{{message}}"',
      '',
    ].join('\n'),
  );
}

if (!DAEMON) {
  console.log('跳过：没有设 COREX_DAEMON。它该指向 corex-daemon，例如');
  console.log('  cargo build -p corex-daemon');
  console.log('  $env:COREX_DAEMON = "...\\target\\debug\\corex-daemon.exe"');
  process.exit(0);
}

const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'corex-electron-check-'));
const { config, endpoint } = writeConfig(dir);
const target = path.join(dir, 'probe.txt');
writeDirective(dir, target);

const frames = [];
const host = createHost({
  client,
  daemonPath: DAEMON,
  args: ['--config', config],
  // 数据目录必须两边一致：daemon 从它那里读配置、写端点记录与 token，我们也从它那里找。
  dataDir: dir,
  onProgress: (frame) => frames.push(frame),
});

try {
  console.log('启动（连不上就拉起一个 daemon）');
  const started = await host.start();
  check('状态是运行中', started.state === 'running', JSON.stringify(started));
  check('端点来自 daemon 写下的记录', started.endpoint === endpoint, started.endpoint);
  check('认出是自己拉起的', started.spawned === true);

  console.log('目录');
  const actions = await host.actions();
  check('目录非空', actions.length > 0, `${actions.length} 个`);
  const copy = actions.find((entry) => entry.id === 'file.copy');
  check('带权限声明', JSON.stringify(copy?.permissions) === '["filesystem"]');
  check('带 input_schema', copy?.input_schema?.type === 'object');

  console.log('动作');
  const stamp = await host.invoke('generate.timestamp', {});
  check('generate.timestamp 有返回值', typeof stamp?.value === 'string', JSON.stringify(stamp));
  // 直接调动作时 daemon 给的步骤名就是 `invoke`；指令那条路才是 YAML 里写的 id。
  check(
    'invoke 也上报了进度帧',
    frames.some((frame) => frame.kind === 'step_start' && frame.step === 'invoke'),
    JSON.stringify(frames),
  );

  console.log('指令与进度帧');
  frames.length = 0;
  const result = await host.run('probe', {});
  check('指令跑完了', result !== undefined, JSON.stringify(result));
  check('文件真被写出来', fs.existsSync(target) && fs.readFileSync(target, 'utf8') === 'hi');
  check('指令列表里有 probe', (await host.directives()).includes('probe'));
  // 给了 `onProgress` 才会置 `stream`；没收到帧说明这条链路断了（而结果是看不出来的）。
  const steps = frames.filter((frame) => frame.kind === 'step_start').map((frame) => frame.step);
  check('收到每个步骤的开始帧', JSON.stringify(steps) === '["render","write"]', JSON.stringify(steps));
  check('最后一条帧是 end 而不是被当成结果', frames.at(-1)?.kind === 'step_end', JSON.stringify(frames.at(-1)));

  console.log('收尾');
  const stopped = await host.stop();
  check('状态回到已停止', stopped.state === 'stopped');
  check(
    'daemon 有序退出并删掉记录',
    !fs.existsSync(path.join(dir, 'endpoint.json')),
    '记录还在，说明没走到清理',
  );
} catch (error) {
  failed += 1;
  console.log(`  FAIL 抛出异常: ${error.stack ?? error.message}`);
} finally {
  await host.stop().catch(() => {});
  fs.rmSync(dir, { recursive: true, force: true });
}

console.log(failed === 0 ? '\n全部通过' : `\n${failed} 项失败`);
process.exit(failed === 0 ? 0 : 1);
