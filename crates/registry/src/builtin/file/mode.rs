//! `file.write` 各 `mode` 的策略：一个取值一个函数，外加它们搭建的
//! 基于 rope 的行编辑器。
//!
//! 这些都是纯文本变换（文本进 → 文本出），所以不必碰文件系统就能推导
//! 和测试；动作层面的管道由 `super` 负责。

use super::*;

pub(super) fn splice(
    content: &str,
    start: &str,
    end: &str,
    replacement: &str,
    nth: usize,
    include_markers: bool,
    on_missing: &str,
) -> Result<(String, bool), ActionError> {
    if nth == 0 {
        return Err(ActionError::InvalidParams(
            "nth 为 1-based，不能为 0".into(),
        ));
    }
    let mut from = 0usize;
    let mut start_pos = None;
    for i in 1..=nth {
        match content[from..].find(start) {
            Some(rel) => {
                let abs = from + rel;
                if i == nth {
                    start_pos = Some(abs);
                    break;
                }
                from = abs + start.len();
            }
            None => {
                start_pos = None;
                break;
            }
        }
    }
    let Some(start_pos) = start_pos else {
        return match on_missing {
            "noop" => Ok((content.to_string(), false)),
            "error" => Err(ActionError::execution(format!(
                "未找到第 {nth} 个起始 marker: {start}"
            ))),
            other => Err(ActionError::InvalidParams(format!(
                "不支持的 on_missing: {other}（error|noop）"
            ))),
        };
    };
    let after_start = start_pos + start.len();
    let end_rel = content[after_start..].find(end);
    let Some(end_rel) = end_rel else {
        return match on_missing {
            "noop" => Ok((content.to_string(), false)),
            "error" => Err(ActionError::execution(format!("未找到结束 marker: {end}"))),
            other => Err(ActionError::InvalidParams(format!(
                "不支持的 on_missing: {other}（error|noop）"
            ))),
        };
    };
    let end_abs = after_start + end_rel;
    let (cut_from, cut_to) = if include_markers {
        (start_pos, end_abs + end.len())
    } else {
        (after_start, end_abs)
    };
    let mut out = String::with_capacity(content.len() + replacement.len());
    out.push_str(&content[..cut_from]);
    out.push_str(replacement);
    out.push_str(&content[cut_to..]);
    let changed = out != content;
    Ok((out, changed))
}

pub(super) fn str_replace_exact(
    content: &str,
    old: &str,
    new: &str,
    replace_all: bool,
) -> Result<(String, usize), ActionError> {
    if old.is_empty() {
        return Err(ActionError::InvalidParams("old 不能为空".into()));
    }
    let matches = content.matches(old).count();
    if matches == 0 {
        return Err(ActionError::execution(format!("未找到要替换的文本: {old}")));
    }
    if !replace_all && matches > 1 {
        return Err(ActionError::execution(format!(
            "old 匹配 {matches} 处，默认要求唯一；可设 replace_all: true"
        )));
    }
    let out = if replace_all {
        content.replace(old, new)
    } else {
        content.replacen(old, new, 1)
    };
    Ok((out, matches))
}

pub(super) fn regex(
    content: &str,
    pattern: &str,
    replacement: &str,
) -> Result<(String, usize), ActionError> {
    if pattern.len() > MAX_REGEX_PATTERN_LEN {
        return Err(ActionError::InvalidParams(format!(
            "regex pattern 超过 {MAX_REGEX_PATTERN_LEN} 字符"
        )));
    }
    let re =
        Regex::new(pattern).map_err(|e| ActionError::InvalidParams(format!("无效 regex: {e}")))?;
    let matches = re.find_iter(content).count();
    let out = re.replace_all(content, replacement).into_owned();
    if out.len() > MAX_REGEX_REPLACE_BYTES {
        return Err(ActionError::execution(format!(
            "regex 替换结果超过 {MAX_REGEX_REPLACE_BYTES} 字节"
        )));
    }
    Ok((out, matches))
}

pub(super) fn json_set(
    existing: &str,
    pointer: &str,
    value: &Value,
) -> Result<String, ActionError> {
    let mut root: Value = if existing.trim().is_empty() {
        Value::Map(BTreeMap::new())
    } else {
        let json: serde_json::Value = serde_json::from_str(existing)
            .map_err(|e| ActionError::execution(format!("JSON 解析失败: {e}")))?;
        Value::from_json(json)
    };
    set_dot_path(&mut root, pointer, value.clone())?;
    let json = root.to_json();
    serde_json::to_string_pretty(&json)
        .map_err(|e| ActionError::execution(format!("JSON 序列化失败: {e}")))
}

fn set_dot_path(val: &mut Value, path: &str, new_value: Value) -> Result<(), ActionError> {
    if path.is_empty() {
        *val = new_value;
        return Ok(());
    }
    let mut parts: Vec<&str> = path.split('.').collect();
    let last = parts
        .pop()
        .ok_or_else(|| ActionError::InvalidParams("空 pointer".into()))?;
    let mut current = val;
    for segment in parts {
        match current {
            Value::Map(m) => {
                if !m.contains_key(segment) {
                    m.insert(segment.to_string(), Value::Map(BTreeMap::new()));
                }
                current = m
                    .get_mut(segment)
                    .ok_or_else(|| ActionError::execution(format!("无法设置路径: {path}")))?;
            }
            _ => {
                return Err(ActionError::execution(format!(
                    "路径 {path} 中间节点不是对象"
                )));
            }
        }
    }
    match current {
        Value::Map(m) => {
            m.insert(last.to_string(), new_value);
            Ok(())
        }
        Value::Array(l) => {
            let idx: usize = last
                .parse()
                .map_err(|_| ActionError::InvalidParams(format!("无效列表索引: {last}")))?;
            if idx >= l.len() {
                return Err(ActionError::execution(format!("列表索引越界: {idx}")));
            }
            l[idx] = new_value;
            Ok(())
        }
        _ => Err(ActionError::execution(format!("无法在路径 {path} 设置值"))),
    }
}

pub(super) fn unified_patch(base: &str, diff: &str) -> Result<String, ActionError> {
    let patch = diffy::Patch::from_str(diff)
        .map_err(|e| ActionError::InvalidParams(format!("无效 patch: {e}")))?;
    diffy::apply(base, &patch).map_err(|e| ActionError::execution(format!("应用 patch 失败: {e}")))
}

fn line_char_range(
    rope: &Rope,
    start_line: usize,
    end_line: usize,
) -> Result<(usize, usize), ActionError> {
    let total = rope.len_lines();
    if start_line == 0 || end_line == 0 {
        return Err(ActionError::InvalidParams(
            "行号为 1-based，不能为 0".into(),
        ));
    }
    if end_line < start_line {
        return Err(ActionError::InvalidParams(
            "end_line 不能小于 start_line".into(),
        ));
    }
    let start0 = start_line - 1;
    if start0 >= total {
        return Err(ActionError::InvalidParams(format!(
            "start_line {start_line} 超出总行数 {total}"
        )));
    }
    let end0_excl = end_line.min(total);
    let a = rope.line_to_char(start0);
    let b = if end0_excl >= total {
        rope.len_chars()
    } else {
        rope.line_to_char(end0_excl)
    };
    Ok((a, b))
}

pub(super) fn rope_replace_lines(
    rope: &mut Rope,
    start_line: usize,
    end_line: usize,
    content: &str,
) -> Result<(usize, usize, usize), ActionError> {
    let (a, b) = line_char_range(rope, start_line, end_line)?;
    rope.remove(a..b);
    rope.insert(a, content);
    let affected = end_line - start_line + 1;
    Ok((start_line, end_line, affected))
}

pub(super) fn rope_delete_lines(
    rope: &mut Rope,
    start_line: usize,
    end_line: usize,
) -> Result<(usize, usize, usize), ActionError> {
    let (a, b) = line_char_range(rope, start_line, end_line)?;
    rope.remove(a..b);
    let affected = end_line - start_line + 1;
    Ok((start_line, end_line, affected))
}

pub(super) fn rope_insert_lines(
    rope: &mut Rope,
    after_line: usize,
    content: &str,
) -> Result<(usize, usize, usize), ActionError> {
    let total = rope.len_lines();
    let char_idx = if after_line == 0 {
        0
    } else if after_line >= total {
        rope.len_chars()
    } else {
        // after_line 是 1-based：插在下一行的开头
        rope.line_to_char(after_line)
    };
    rope.insert(char_idx, content);
    let inserted = content.lines().count().max(1);
    let start = after_line + 1;
    Ok((start, after_line + inserted, inserted))
}
