/// 模块调用上下文（变量解析 + 可选 Daemon 状态）
pub struct InvokeContext<'a> {
    #[cfg(feature = "pipeline")]
    pub pipeline: Option<&'a crate::pipeline::context::PipelineContext>,
    #[cfg(feature = "serve")]
    pub daemon: Option<&'a mut crate::serve::state::DaemonState>,
    #[cfg(not(any(feature = "pipeline", feature = "serve")))]
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> InvokeContext<'a> {
    pub fn empty() -> InvokeContext<'static> {
        InvokeContext {
            #[cfg(feature = "pipeline")]
            pipeline: None,
            #[cfg(feature = "serve")]
            daemon: None,
            #[cfg(not(any(feature = "pipeline", feature = "serve")))]
            _phantom: std::marker::PhantomData,
        }
    }

    #[cfg(feature = "pipeline")]
    pub fn pipeline(ctx: &'a crate::pipeline::context::PipelineContext) -> Self {
        Self {
            pipeline: Some(ctx),
            #[cfg(feature = "serve")]
            daemon: None,
        }
    }

    pub fn parse(&self, input: &str) -> String {
        #[cfg(feature = "pipeline")]
        {
            match self.pipeline {
                Some(ctx) => return ctx.parse(input),
                None => {}
            }
        }
        input.to_string()
    }

    /// Daemon 预热的显示器列表（IPC 截图加速）；无 daemon 时返回 None。
    #[cfg(all(feature = "capture", feature = "serve"))]
    pub fn cached_monitors(&self) -> Option<&[xcap::Monitor]> {
        self.daemon.as_ref().and_then(|d| d.monitors.as_deref())
    }

    #[cfg(all(feature = "capture", not(feature = "serve")))]
    pub fn cached_monitors(&self) -> Option<&[xcap::Monitor]> {
        None
    }
}

#[cfg(feature = "serve")]
impl<'a> InvokeContext<'a> {
    pub fn daemon(state: &'a mut crate::serve::state::DaemonState) -> Self {
        Self {
            #[cfg(feature = "pipeline")]
            pipeline: None,
            daemon: Some(state),
        }
    }

    #[cfg(feature = "pipeline")]
    pub fn pipeline_and_daemon(
        ctx: &'a crate::pipeline::context::PipelineContext,
        state: &'a mut crate::serve::state::DaemonState,
    ) -> Self {
        Self {
            pipeline: Some(ctx),
            daemon: Some(state),
        }
    }
}
