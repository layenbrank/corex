use crate::morph::schema::Hit;

/// 大小写不敏感查找，返回匹配起始的字节偏移
pub fn find_insensitive(haystack: &str, needle: &str) -> Vec<usize> {
    let needle = needle.trim();
    if needle.is_empty() {
        return Vec::new();
    }

    let needle_chars: Vec<char> = needle.chars().collect();
    let needle_lower: Vec<String> = needle_chars
        .iter()
        .map(|c| c.to_lowercase().collect::<String>())
        .collect();

    let char_starts: Vec<usize> = haystack.char_indices().map(|(i, _)| i).collect();
    let haystack_chars: Vec<char> = haystack.chars().collect();
    let haystack_lower: Vec<String> = haystack_chars
        .iter()
        .map(|c| c.to_lowercase().collect::<String>())
        .collect();

    let mut hits = Vec::new();
    if needle_lower.is_empty() || haystack_lower.len() < needle_lower.len() {
        return hits;
    }

    for i in 0..=(haystack_lower.len() - needle_lower.len()) {
        let matched = (0..needle_lower.len()).all(|j| haystack_lower[i + j] == needle_lower[j]);
        if matched {
            hits.push(char_starts[i]);
        }
    }
    hits
}

/// 命中字节跨度（`at` 起共 `n_chars` 个字符）
fn match_bytes(content: &str, at: usize, n_chars: usize) -> usize {
    content[at..]
        .char_indices()
        .nth(n_chars)
        .map(|(i, _)| i)
        .unwrap_or(content.len().saturating_sub(at))
}

/// 按命中字节偏移截取上下文 snippet（对齐 UTF-8 字符边界）
pub fn snippet_at(content: &str, at: usize, match_len: usize) -> String {
    let start = content.floor_char_boundary(at.saturating_sub(15));
    let end = content.ceil_char_boundary((at + match_len).saturating_add(15).min(content.len()));
    if start >= end {
        return String::new();
    }
    content[start..end].replace('\n', " ")
}

/// 在单页文本中搜索，返回命中列表（`offset` 为页索引）
pub fn find_hits(content: &str, query: &str, offset: u32) -> Vec<Hit> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    let n_chars = query.chars().count();
    find_insensitive(content, query)
        .into_iter()
        .map(|at| {
            let span = match_bytes(content, at, n_chars);
            Hit {
                offset,
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
                snippet: snippet_at(content, at, span),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query() {
        assert!(find_insensitive("hello", "").is_empty());
        assert!(find_insensitive("hello", "   ").is_empty());
        assert!(find_hits("hello", "", 0).is_empty());
    }

    #[test]
    fn ascii_ci() {
        let hits = find_insensitive("Hello World", "world");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0], 6);
    }

    #[test]
    fn snippet_bounds() {
        let s = snippet_at("abcdef", 100, 3);
        assert!(s.is_empty() || s.len() <= 33);
    }

    #[test]
    fn chinese_snippet() {
        let content = "前言：这是一段中文测试内容，用于搜索命中。";
        let hits = find_hits(content, "中文", 0);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].snippet.contains("中文"));
        // 不 panic，且为合法 UTF-8
        let _ = hits[0].snippet.chars().count();
    }
}
