use indicatif::{ProgressBar, ProgressStyle};
use std::{sync::Arc, time::Duration};

pub struct Progress;

impl Progress {
    /// 创建加载旋转器
    pub fn spinner(msg: &str) -> ProgressBar {
        let sp = ProgressBar::new_spinner();
        sp.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.cyan} {msg}")
                .unwrap()
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );

        sp.set_message(msg.to_string());
        sp.enable_steady_tick(Duration::from_millis(80));
        sp
    }

    /// 创建进度条
    pub fn progress(total: u64) -> Arc<ProgressBar> {
        let pb = ProgressBar::new(total);
        pb.set_style(
            ProgressStyle::default_bar()
                .template(&format!(
                    "{}\n{} 📁 [{}] {}/{} ({}%) | ⏱️  {} | 🚀 {}",
                    "{msg}",
                    "{spinner:.green}",
                    "{bar:40.cyan/blue}",
                    "{pos:>7}",
                    "{len:7}",
                    "{percent:>3}",
                    "{elapsed_precise}",
                    "{eta_precise}"
                ))
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  ")
                .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
        );
        pb.enable_steady_tick(Duration::from_millis(120));
        Arc::new(pb)
    }
}
