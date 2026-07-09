#[cfg(feature = "screenshot")]
use xcap::Monitor;

/// Daemon 启动时预热的共享状态
pub struct DaemonState {
    #[cfg(feature = "screenshot")]
    pub monitors: Option<Vec<Monitor>>,
}

impl DaemonState {
    pub fn init() -> anyhow::Result<Self> {
        #[cfg(feature = "screenshot")]
        {
            let monitors = Monitor::all().map_err(|e| anyhow::anyhow!(e.to_string()))?;
            if monitors.is_empty() {
                anyhow::bail!("没有找到可用显示器");
            }
            eprintln!(
                "corex-serve: 已缓存 {} 个显示器",
                monitors.len()
            );
            Ok(Self {
                monitors: Some(monitors),
            })
        }

        #[cfg(not(feature = "screenshot"))]
        {
            Ok(Self {})
        }
    }
}
