use std::time::Duration;

pub struct File;

impl File {
    /// 格式化文件大小
    pub fn format_size(size: u64) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
        let mut size = size as f64;
        let mut unit_index = 0;

        while size >= 1024.0 && unit_index < UNITS.len() - 1 {
            size /= 1024.0;
            unit_index += 1;
        }

        if unit_index == 0 {
            format!("{} {}", size as u64, UNITS[unit_index])
        } else {
            format!("{:.2} {}", size, UNITS[unit_index])
        }
    }

    /// 格式化持续时间
    pub fn format_duration(duration: Duration) -> String {
        let secs = duration.as_secs();
        if secs < 60 {
            format!("{:.1}s", duration.as_secs_f64())
        } else if secs < 3600 {
            format!("{}m{}s", secs / 60, secs % 60)
        } else {
            format!("{}h{}m", secs / 3600, (secs % 3600) / 60)
        }
    }

    /// 计算传输速度
    pub fn calc_speed(bytes: u64, elapsed: Duration) -> u64 {
        let seconds = elapsed.as_secs();
        if seconds > 0 { bytes / seconds } else { 0 }
    }

    /// 安全地截断文件名，避免 Unicode 字符边界问题
    pub fn truncate(string: &str, max_len: usize) -> String {
        if string.len() <= max_len {
            return string.to_string();
        }

        // 使用字符迭代器来安全处理 Unicode
        let chars: Vec<char> = string.chars().collect();
        if chars.len() <= max_len {
            return string.to_string();
        }

        let prefix_len = max_len.saturating_sub(3) / 2;
        let suffix_len = max_len.saturating_sub(3).saturating_sub(prefix_len);

        if prefix_len + suffix_len + 3 > chars.len() {
            return string.to_string();
        }

        format!(
            "{}...{}",
            chars[..prefix_len].iter().collect::<String>(),
            chars[chars.len().saturating_sub(suffix_len)..]
                .iter()
                .collect::<String>()
        )
    }
}
