//! 客户端的关键行为：帧的路由与优先级规则。
//!
//! 这里不拉起真的 daemon——Rust 侧 `bins/daemon/tests/streaming.rs` 已经用真进程覆盖了
//! 线格式。这一层要钉住的是**读法**：进度帧不能当成回答、错误要带 code、
//! 连接断了不能把在途请求吊死。所以假 daemon 只负责把用例指定的字节写回去。

import assert from 'node:assert/strict'
import fs from 'node:fs'
import net from 'node:net'
import os from 'node:os'
import path from 'node:path'
import { after, test } from 'node:test'

import { connect, discover, resolveDataDir } from '../src/index.js'

const tempDirs = []
let pipeCounter = 0

/** 本用例专属的端点：Windows 命名管道是全局名字，用例之间不能撞。 */
function freshEndpoint(tag) {
  pipeCounter += 1
  const name = `${process.pid}-${tag}-${pipeCounter}`
  return process.platform === 'win32'
    ? `\\\\.\\pipe\\corex-client-test-${name}`
    : path.join(os.tmpdir(), `corex-client-test-${name}.sock`)
}

/** 往连接上写一条 NDJSON。 */
function send(socket, message) {
  socket.write(`${JSON.stringify(message)}\n`)
}

/**
 * 假 daemon：把收到的请求交给 `handler`，请求本身也记下来供断言。
 *
 * 刻意不自动回话——回什么、按什么顺序回，正是各用例要控制的东西。
 */
async function startServer(handler) {
  const at = freshEndpoint('fake')
  const seen = []
  const server = net.createServer((socket) => {
    let buffer = ''
    socket.on('data', (chunk) => {
      buffer += chunk
      for (;;) {
        const end = buffer.indexOf('\n')
        if (end < 0) {
          break
        }
        const line = buffer.slice(0, end)
        buffer = buffer.slice(end + 1)
        if (!line.trim()) {
          continue
        }
        const request = JSON.parse(line)
        seen.push(request)
        handler(request, socket)
      }
    })
    // 用例会故意掐断连接，假 daemon 不该因此让测试进程炸掉。
    socket.on('error', () => {})
  })
  await new Promise((resolve, reject) => {
    server.once('error', reject)
    server.listen(at, resolve)
  })
  return {
    endpoint: at,
    seen,
    async close() {
      server.closeAllConnections?.()
      await new Promise((resolve) => server.close(resolve))
      if (process.platform !== 'win32') {
        fs.rmSync(at, { force: true })
      }
    }
  }
}

/** 造一个只属于本用例的数据目录。 */
function freshDataDir({ record, token } = {}) {
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'corex-client-data-'))
  tempDirs.push(dir)
  if (record) {
    fs.writeFileSync(path.join(dir, 'endpoint.json'), JSON.stringify(record))
  }
  if (token) {
    fs.writeFileSync(path.join(dir, 'token'), token)
  }
  return dir
}

after(() => {
  for (const dir of tempDirs) {
    fs.rmSync(dir, { recursive: true, force: true })
  }
})

/** 等到 `predicate` 成立。用它代替固定等待，避免用例在慢机器上闪。 */
async function until(predicate, timeoutMs = 3000) {
  const deadline = Date.now() + timeoutMs
  for (;;) {
    if (predicate()) {
      return
    }
    assert.ok(Date.now() <= deadline, '等待超时')
    await new Promise((resolve) => setTimeout(resolve, 10))
  }
}

test('ping 在 pong 上收尾', async () => {
  const server = await startServer((request, socket) => {
    send(socket, { type: 'pong', id: request.id })
  })
  const client = await connect({ endpoint: server.endpoint })
  assert.equal(await client.ping(), true)
  await client.close()
  await server.close()
})

test('invoke 先按序回调进度帧，终帧才当结果', async () => {
  const server = await startServer((request, socket) => {
    send(socket, {
      type: 'event',
      id: request.id,
      progress: { kind: 'step_start', seq: 1, step: 'copy', action: 'file.copy' }
    })
    send(socket, {
      type: 'event',
      id: request.id,
      progress: {
        kind: 'step_progress',
        step: 'copy',
        action: 'file.copy',
        done: 1,
        total: 2,
        unit: 'bytes'
      }
    })
    send(socket, { type: 'ok', id: request.id, data: { copied: true } })
  })

  const client = await connect({ endpoint: server.endpoint })
  const kinds = []
  const data = await client.invoke(
    'file.copy',
    { from: 'a', to: 'b' },
    { onProgress: (progress) => kinds.push(progress.kind) }
  )

  assert.deepEqual(kinds, ['step_start', 'step_progress'])
  assert.deepEqual(data, { copied: true })
  assert.equal(server.seen[0].stream, true, '给了回调才该置 stream')
  assert.deepEqual(server.seen[0].params, { from: 'a', to: 'b' })

  await client.close()
  await server.close()
})

test('不给回调就不置 stream', async () => {
  const server = await startServer((request, socket) => {
    send(socket, { type: 'ok', id: request.id, data: [] })
  })
  const client = await connect({ endpoint: server.endpoint })
  await client.actions()
  assert.equal(server.seen[0].stream, undefined)
  await client.close()
  await server.close()
})

test('error 帧变成带 code 的 RpcError', async () => {
  const server = await startServer((request, socket) => {
    send(socket, {
      type: 'error',
      id: request.id,
      error: { code: 403, message: '动作已禁用: shell.run' }
    })
  })
  const client = await connect({ endpoint: server.endpoint })
  await assert.rejects(
    () => client.invoke('shell.run', { command: 'echo' }),
    (err) => err.name === 'RpcError' && err.code === 403 && err.message.includes('shell.run')
  )
  await client.close()
  await server.close()
})

test('run 带上 input 与 path', async () => {
  const server = await startServer((request, socket) => {
    send(socket, { type: 'ok', id: request.id, data: { done: true } })
  })
  const client = await connect({ endpoint: server.endpoint })
  await client.run('hello', { input: { who: 'corex' }, file: 'hello.yaml' })
  assert.equal(server.seen[0].name, 'hello')
  assert.deepEqual(server.seen[0].input, { who: 'corex' })
  assert.equal(server.seen[0].path, 'hello.yaml')
  await client.close()
  await server.close()
})

test('连接断开时在途请求被拒，而不是永远挂着', async () => {
  let held = null
  const server = await startServer((_request, socket) => {
    // 刻意不回话，等用例把连接掐掉。
    held = socket
  })
  const client = await connect({ endpoint: server.endpoint })
  // 先把可能的拒绝接住：不然它可能在断言挂上之前变成“未处理的拒绝”。
  const settled = client.invoke('file.copy', { from: 'a', to: 'b' }).then(
    () => null,
    (err) => err
  )
  await until(() => held !== null)
  held.destroy()

  const err = await settled
  assert.ok(err instanceof Error, `在途请求应当被拒，实际拿到 ${err}`)
  assert.match(err.message, /连接已断开/)

  await client.close()
  await server.close()
})

test('discover 的优先级：显式选项 → 记录 → 平台默认', async () => {
  const recorded = freshEndpoint('from-record')
  const dataDir = freshDataDir({ token: 'from-file\n' })
  fs.writeFileSync(
    path.join(dataDir, 'endpoint.json'),
    JSON.stringify({
      version: 1,
      pid: 4242,
      endpoint: recorded,
      kind: 'pipe',
      token_file: path.join(dataDir, 'token')
    })
  )

  const byDefault = discover({ dataDir })
  assert.equal(byDefault.source, 'record')
  assert.equal(byDefault.endpoint, recorded)
  assert.equal(byDefault.token, 'from-file', 'token 文件里的空白该被去掉')
  assert.equal(byDefault.pid, 4242)

  const byOption = discover({ dataDir, endpoint: 'explicit' })
  assert.equal(byOption.source, 'option')
  assert.equal(byOption.endpoint, 'explicit')

  const empty = discover({ dataDir: freshDataDir() })
  assert.equal(empty.source, 'default')
  assert.equal(empty.token, undefined)
  assert.ok(empty.endpoint.length > 0)
})

test('损坏或版本不认识的记录按“没有记录”处理', async () => {
  const broken = freshDataDir({ token: 'from-file' })
  fs.writeFileSync(path.join(broken, 'endpoint.json'), '{ 这不是 JSON')
  assert.equal(discover({ dataDir: broken }).source, 'default')
  // 不算记录，于是回到「没有记录」那条路：数据目录里的文件就是答案。
  assert.equal(discover({ dataDir: broken }).token, 'from-file')

  const future = freshDataDir({
    record: { version: 99, pid: 1, endpoint: 'x', kind: 'pipe' }
  })
  assert.equal(discover({ dataDir: future }).source, 'default')
})

test('记录里没有 token_file 时不去读 <数据目录>/token', async () => {
  // daemon 的 token 来自 `COREX_TOKEN` 或配置——那两处的值属于调用方，不会被写进记录。
  // 此时 `<数据目录>/token` 是**上一个** daemon 留下的东西，拿它去连只会得到 401。
  const dataDir = freshDataDir({
    record: { version: 1, pid: 4242, endpoint: freshEndpoint('no-token'), kind: 'pipe' },
    token: '上一个 daemon 留下的'
  })
  assert.equal(discover({ dataDir }).token, undefined)
  // 显式给的仍然算数（配置里的 token 由调用方读出来传进来）。
  assert.equal(discover({ dataDir, token: 'explicit' }).token, 'explicit')
})

test('COREX_TOKEN 压过 token 文件，显式 token 压过两者', async () => {
  const dataDir = freshDataDir({ token: 'from-file' })
  const saved = process.env.COREX_TOKEN
  try {
    process.env.COREX_TOKEN = 'from-env'
    assert.equal(discover({ dataDir }).token, 'from-env')
    assert.equal(discover({ dataDir, token: 'explicit' }).token, 'explicit')
    delete process.env.COREX_TOKEN
    assert.equal(discover({ dataDir }).token, 'from-file')
  } finally {
    if (saved === undefined) {
      delete process.env.COREX_TOKEN
    } else {
      process.env.COREX_TOKEN = saved
    }
  }
})

test('COREX_DATA_DIR 决定数据目录；显式参数压过它', async () => {
  const saved = process.env.COREX_DATA_DIR
  try {
    process.env.COREX_DATA_DIR = path.join(os.tmpdir(), 'corex-client-env-dir')
    assert.equal(resolveDataDir(), path.resolve(path.join(os.tmpdir(), 'corex-client-env-dir')))
    assert.equal(resolveDataDir('relative-dir'), path.resolve('relative-dir'))
  } finally {
    if (saved === undefined) {
      delete process.env.COREX_DATA_DIR
    } else {
      process.env.COREX_DATA_DIR = saved
    }
  }
})

test('鉴权：token 会被附到请求上', async () => {
  const server = await startServer((request, socket) => {
    send(socket, { type: 'pong', id: request.id })
  })
  const client = await connect({ endpoint: server.endpoint, token: 'secret' })
  await client.ping()
  assert.equal(server.seen[0].auth_token, 'secret')
  await client.close()
  await server.close()
})
