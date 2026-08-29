# Findings

## Architecture
- `ui.rs` 拆为 `ui/{mod,window,element,input,win}`
- 单一执行面：Action → domain → win
- `act-sys`：dialog / url / process
- `capture.find` 匹配算法与 Action 分离

## Key symbols
- `ui_desktop_icons_impl`、`element_at_point`、`ui_pick::probe_pick`
- `process_launch` ToolHelp 可供 `process.list` 复用

## File mini-IDE
- `file.write`：append / str_replace / 行编辑 / splice / patch / newline
- `file.read`：lines + 行窗；`dir.read` 参数 `mode`（flat|tree）
- 依赖：ropey、diffy
