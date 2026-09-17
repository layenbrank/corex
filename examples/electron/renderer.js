'use strict';

/**
 * 渲染进程：一个能真跑的面板。
 *
 * 它只碰 `window.corex`（`preload.js` 暴露的那组能力），没有 Node、也没有 `ipcRenderer`。
 * 界面上有一处值得留意：**参数模板是从目录里的 `input_schema` 现推的**——这正是
 * `corex actions --json` / IPC `list_actions` 把 schema 一起回给宿主的用处，
 * 宿主不必自己抄一份参数表。
 */

const el = (id) => document.getElementById(id);

/** 目录里与 `actions()` 同名的那份数据，按 id 索引。 */
let catalog = new Map();

function show(value) {
  el('result').textContent = typeof value === 'string' ? value : JSON.stringify(value, null, 2);
}

function log(line) {
  el('log').textContent = `${line}\n${el('log').textContent}`.trim();
}

/** 每个动作都包一层：失败要显示出来，而不是静默什么都不发生。 */
async function guard(task) {
  try {
    await task();
  } catch (error) {
    show(`错误: ${error.message}`);
  }
}

async function refreshStatus() {
  const status = await window.corex.status();
  el('status').textContent =
    status.state === 'running'
      ? `运行中 · ${status.endpoint}${status.spawned ? '（本应用拉起的）' : '（连的已有进程）'}`
      : `已停止${status.error ? ` · ${status.error}` : ''}`;
}

/** 按 `input_schema` 生成一份最小可跑的 params 模板。 */
function template(entry) {
  const values = {};
  for (const [name, spec] of Object.entries(entry.input_schema.properties ?? {})) {
    if ('default' in spec) {
      values[name] = spec.default;
    } else if (spec.type === 'integer' || spec.type === 'number') {
      values[name] = 0;
    } else if (spec.type === 'boolean') {
      values[name] = false;
    } else if (spec.type === 'array') {
      values[name] = [];
    } else if (spec.type === 'object') {
      values[name] = {};
    } else {
      values[name] = '';
    }
  }
  return values;
}

function describe(entry) {
  const required = entry.input_schema.required ?? [];
  const params = entry.params
    .map((param) => `${param.name}${required.includes(param.name) ? '*' : ''}:${param.ty}`)
    .join(' ');
  const permissions = entry.permissions.join('/') || '无';
  return `${entry.bucket} · 权限 ${permissions} · 参数 ${params || '（无）'}`;
}

function selectAction() {
  const entry = catalog.get(el('action').value);
  if (!entry) {
    return;
  }
  el('action-meta').textContent = describe(entry);
  el('params').value = JSON.stringify(template(entry), null, 2);
}

async function loadCatalog() {
  const actions = await window.corex.actions();
  catalog = new Map(actions.map((entry) => [entry.id, entry]));
  el('action').replaceChildren(...actions.map((entry) => new Option(entry.id, entry.id)));
  // 预选一个**不需要参数**的动作，示例一打开就能跑出东西来。
  const immediate = actions.find((entry) => (entry.input_schema.required ?? []).length === 0);
  el('action').value = (immediate ?? actions[0]).id;
  selectAction();
}

async function loadDirectives() {
  const names = await window.corex.directives();
  el('directive').replaceChildren(...names.map((name) => new Option(name, name)));
}

el('start').addEventListener('click', () =>
  guard(async () => {
    await window.corex.start();
    await refreshStatus();
    await loadCatalog();
    await loadDirectives();
  }),
);

el('stop').addEventListener('click', () =>
  guard(async () => {
    await window.corex.stop();
    await refreshStatus();
  }),
);

el('action').addEventListener('change', selectAction);

el('invoke').addEventListener('click', () =>
  guard(async () => {
    let params;
    try {
      params = JSON.parse(el('params').value || '{}');
    } catch (error) {
      show(`params 不是合法 JSON: ${error.message}`);
      return;
    }
    show(await window.corex.invoke(el('action').value, params));
  }),
);

el('run').addEventListener('click', () =>
  guard(async () => {
    show(await window.corex.run(el('directive').value, {}));
  }),
);

// 进度帧是主进程主动推的：一条请求可以来很多帧，终帧（结果）走 invoke/run 的 Promise。
window.corex.onProgress((frame) => {
  const done =
    frame.kind === 'step_progress' && frame.total ? `${frame.done}/${frame.total} ${frame.unit}` : '';
  const end = frame.kind === 'step_end' ? `${frame.took_ms}ms ${frame.ok ? '✓' : '✗'}` : '';
  log(`${frame.kind}  ${frame.action}  ${frame.step}  ${done} ${end}`.trimEnd());
});

guard(async () => {
  // 主进程可能还在连（或刚才失败过），这里主动拉一次；`start()` 是幂等的。
  await window.corex.start().catch(() => {});
  await refreshStatus();
  await loadCatalog();
  await loadDirectives();
});
