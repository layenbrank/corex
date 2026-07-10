use anyhow::Result;
use futures::Stream;
use std::pin::Pin;

use super::item::PipelineItem;
use crate::invoke::Artifact;

/// Stage 种类
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageKind {
    Batch,
    Stream,
    Signal,
}

pub trait StageSource: Send {
    fn into_stream(self) -> Pin<Box<dyn Stream<Item = Result<PipelineItem>> + Send>>;
}

pub trait StageTransform: Send + Sync {
    fn transform(&self, item: PipelineItem) -> Result<Option<PipelineItem>>;
}

pub trait StageSink: Send {
    fn consume(&mut self, item: PipelineItem) -> Result<()>;
    fn finish(self) -> Result<Artifact>;
}
