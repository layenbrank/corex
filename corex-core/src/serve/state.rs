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
            match Monitor::all() {
                Ok(monitors) if !monitors.is_empty() => {
                    eprintln!("corex-serve: 已缓存 {} 个显示器", monitors.len());
                    Ok(Self {
                        monitors: Some(monitors),
                    })
                }
                Ok(_) => {
                    eprintln!("corex-serve: 警告：未找到显示器，截图时将按需重试");
                    Ok(Self { monitors: None })
                }
                Err(err) => {
                    eprintln!("corex-serve: 警告：Monitor::all 失败 ({err})，截图时将按需重试");
                    Ok(Self { monitors: None })
                }
            }
        }

        #[cfg(not(feature = "screenshot"))]
        {
            Ok(Self {})
        }
    }

    /// 刷新显示器缓存（Capture / Monitors 前可选调用）
    #[cfg(feature = "screenshot")]
    pub fn refresh_monitors(&mut self) -> anyhow::Result<&[Monitor]> {
        let monitors = Monitor::all().map_err(|e| anyhow::anyhow!(e.to_string()))?;
        if monitors.is_empty() {
            anyhow::bail!("没有找到可用显示器");
        }
        self.monitors = Some(monitors);
        Ok(self.monitors.as_deref().unwrap())
    }
}
