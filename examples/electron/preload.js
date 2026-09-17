'use strict';

/**
 * 渲染进程与主进程之间唯一的桥。
 *
 * 只暴露**能力**（一组函数），不暴露 channel 名、更不暴露 `ipcRenderer` 本身：
 * 渲染进程因此拿不到「随便往主进程发消息」的能力，注入脚本也少了落脚点。
 */

const { contextBridge, ipcRenderer } = require('electron');

contextBridge.exposeInMainWorld('corex', {
  status: () => ipcRenderer.invoke('corex:status'),
  start: () => ipcRenderer.invoke('corex:start'),
  stop: () => ipcRenderer.invoke('corex:stop'),
  actions: () => ipcRenderer.invoke('corex:actions'),
  directives: () => ipcRenderer.invoke('corex:directives'),
  invoke: (action, params) => ipcRenderer.invoke('corex:invoke', action, params),
  run: (name, input) => ipcRenderer.invoke('corex:run', name, input),

  /**
   * 进度帧是主进程**主动推**的，不是任何请求的回话，所以这里没有 request/response 的形状。
   * 返回退订函数，界面重开时不会累积监听。
   */
  onProgress: (handler) => {
    const listener = (_event, frame) => handler(frame);
    ipcRenderer.on('corex:progress', listener);
    return () => ipcRenderer.off('corex:progress', listener);
  },
});
