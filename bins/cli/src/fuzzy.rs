//! 名字的模糊匹配：只在“猜用户想敲哪条指令”时用。
//!
//! 目标是给出一句像样的纠正建议，不是实现搜索引擎，所以宁可退回编辑距离这种
//! 一眼能看懂、能单独测的实现，也不引依赖。

/// 最多给几个候选。
const MAX_SUGGESTIONS: usize = 5;

/// 在候选里挑最像 `needle` 的几个，按接近程度排序。
pub(crate) fn nearest(needle: &str, candidates: &[String]) -> Vec<String> {
    let needle = needle.to_lowercase();
    let slack = slack(&needle);
    let mut scored: Vec<(usize, &String)> = candidates
        .iter()
        .map(|candidate| (likeness(&needle, &candidate.to_lowercase()), candidate))
        .filter(|(score, _)| *score <= slack)
        .collect();
    scored.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(b.1)));
    scored.truncate(MAX_SUGGESTIONS);
    scored.into_iter().map(|(_, name)| name.clone()).collect()
}

/// 允许的差距：短名字容不下错字，长名字可以放宽一点。
fn slack(needle: &str) -> usize {
    match needle.chars().count() {
        0..=4 => 1,
        5..=8 => 2,
        _ => 3,
    }
}

/// 越小越像：完全相同 0，包含 1，其余取编辑距离。
fn likeness(needle: &str, candidate: &str) -> usize {
    if candidate == needle {
        0
    } else if candidate.contains(needle) {
        1
    } else {
        distance(candidate, needle)
    }
}

/// Levenshtein 距离；名字都很短，两行滚动数组足够。
fn distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() {
        return b.len();
    }
    // 行内只保留上一行与当前行。
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    let mut current = vec![0usize; b.len() + 1];
    for (i, ca) in a.iter().enumerate() {
        current[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != cb);
            current[j + 1] = (previous[j] + cost)
                .min(previous[j + 1] + 1)
                .min(current[j] + 1);
        }
        std::mem::swap(&mut previous, &mut current);
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names() -> Vec<String> {
        ["hello", "file-ops-demo", "http-post-json", "hello-world"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn exact_name_comes_first() {
        let near = nearest("http-post-json", &names());
        assert_eq!(near.first().map(String::as_str), Some("http-post-json"));
    }

    #[test]
    fn typo_still_finds_the_target() {
        let near = nearest("hllo", &names());
        assert!(near.contains(&"hello".to_string()), "{near:?}");
    }

    #[test]
    fn unrelated_name_gets_no_suggestion() {
        assert!(nearest("zzzzzzzzzz", &names()).is_empty());
    }

    #[test]
    fn distance_is_symmetric_enough() {
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("abc", ""), 3);
    }
}
