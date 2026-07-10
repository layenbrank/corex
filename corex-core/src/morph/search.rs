use crate::morph::schema::SearchMatch;

/// 在文本中按 Unicode 大小写不敏感搜索，返回匹配起始字节偏移
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

    let mut matches = Vec::new();
    if needle_lower.is_empty() || haystack_lower.len() < needle_lower.len() {
        return matches;
    }

    for i in 0..=(haystack_lower.len() - needle_lower.len()) {
        let matched = (0..needle_lower.len()).all(|j| haystack_lower[i + j] == needle_lower[j]);
        if matched {
            matches.push(char_starts[i]);
        }
    }
    matches
}

/// 构建 snippet（基于原文字节偏移）
pub fn snippet_at(content: &str, byte_offset: usize, query_len: usize) -> String {
    let snippet_s = byte_offset.saturating_sub(15);
    let snippet_e = (byte_offset + query_len + 15).min(content.len());
    if snippet_s >= snippet_e {
        return String::new();
    }
    content[snippet_s..snippet_e].replace('\n', " ")
}

/// 在 PDF 页面文本中搜索并返回匹配列表
pub fn search_text(content: &str, query: &str, page_index: u32) -> Vec<SearchMatch> {
    let query = query.trim();
    if query.is_empty() {
        return Vec::new();
    }

    find_insensitive(content, query)
        .into_iter()
        .map(|byte_offset| SearchMatch {
            page_index,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            snippet: snippet_at(content, byte_offset, query.len()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_returns_no_matches() {
        assert!(find_insensitive("hello", "").is_empty());
        assert!(find_insensitive("hello", "   ").is_empty());
        assert!(search_text("hello", "", 0).is_empty());
    }

    #[test]
    fn ascii_case_insensitive() {
        let hits = find_insensitive("Hello World", "world");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0], 6);
    }

    #[test]
    fn snippet_respects_bounds() {
        let s = snippet_at("abcdef", 100, 3);
        assert!(s.is_empty() || s.len() <= 33);
    }
}
