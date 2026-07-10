use std::collections::HashMap;

use anyhow::{Context, Result};
use petgraph::Direction;
use petgraph::algo::{is_cyclic_directed, toposort};
use petgraph::graph::{DiGraph, NodeIndex};

use super::config::{PipelineConfig, StepConfig};

/// DAG 步骤图（petgraph）
pub struct StageGraph {
    graph: DiGraph<String, ()>,
}

impl StageGraph {
    pub fn from_pipeline(pipeline: &PipelineConfig) -> Result<Self> {
        let mut graph = DiGraph::<String, ()>::new();
        let mut index = HashMap::new();

        for step in &pipeline.steps {
            let idx = graph.add_node(step.id.clone());
            index.insert(step.id.clone(), idx);
        }

        for (i, step) in pipeline.steps.iter().enumerate() {
            let to = index[&step.id];
            let deps: Vec<String> = if step.depends_on.is_empty() {
                if i == 0 {
                    vec![]
                } else {
                    vec![pipeline.steps[i - 1].id.clone()]
                }
            } else {
                step.depends_on.clone()
            };
            for dep in deps {
                let from = *index
                    .get(&dep)
                    .with_context(|| format!("步骤 '{}' depends_on 未知: {}", step.id, dep))?;
                graph.add_edge(from, to, ());
            }
        }

        Ok(Self { graph })
    }

    pub fn validate(&self) -> Result<()> {
        if is_cyclic_directed(&self.graph) {
            anyhow::bail!("Pipeline 存在循环依赖");
        }
        Ok(())
    }

    /// 拓扑序步骤 ID
    pub fn execution_order(&self) -> Result<Vec<String>> {
        let sorted =
            toposort(&self.graph, None).map_err(|_| anyhow::anyhow!("Pipeline 存在循环依赖"))?;
        Ok(sorted.iter().map(|idx| self.graph[*idx].clone()).collect())
    }

    /// 按依赖深度分层（同层可并发）
    pub fn execution_layers(&self) -> Result<Vec<Vec<String>>> {
        let order =
            toposort(&self.graph, None).map_err(|_| anyhow::anyhow!("Pipeline 存在循环依赖"))?;

        let mut depth: HashMap<NodeIndex, usize> = HashMap::new();
        for idx in &order {
            let d = self
                .graph
                .neighbors_directed(*idx, Direction::Incoming)
                .map(|n| depth[&n] + 1)
                .max()
                .unwrap_or(0);
            depth.insert(*idx, d);
        }

        let max_d = depth.values().copied().max().unwrap_or(0);
        let mut layers = vec![Vec::<String>::new(); max_d + 1];
        for idx in order {
            layers[depth[&idx]].push(self.graph[idx].clone());
        }
        Ok(layers)
    }

    pub fn step_by_id<'a>(&self, pipeline: &'a PipelineConfig, id: &str) -> Option<&'a StepConfig> {
        pipeline.steps.iter().find(|s| s.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::config::StepConfig;

    fn step(id: &str, module: &str, deps: Vec<&str>) -> StepConfig {
        StepConfig {
            id: id.to_string(),
            module: module.to_string(),
            depends_on: deps.into_iter().map(str::to_string).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn implicit_chain_when_no_depends_on() {
        let pipeline = PipelineConfig {
            id: "p".into(),
            description: None,
            schedule: None,
            watch: None,
            steps: vec![
                step("a", "copy", vec![]),
                step("b", "generate", vec![]),
                step("c", "compression", vec![]),
            ],
        };
        let graph = StageGraph::from_pipeline(&pipeline).unwrap();
        graph.validate().unwrap();
        let layers = graph.execution_layers().unwrap();
        assert_eq!(layers.len(), 3);
        assert_eq!(layers[0], vec!["a"]);
        assert_eq!(layers[1], vec!["b"]);
        assert_eq!(layers[2], vec!["c"]);
    }

    #[test]
    fn fork_join_layers() {
        let pipeline = PipelineConfig {
            id: "p".into(),
            description: None,
            schedule: None,
            watch: None,
            steps: vec![
                step("root", "copy", vec![]),
                step("left", "generate", vec!["root"]),
                step("right", "compression", vec!["root"]),
            ],
        };
        let graph = StageGraph::from_pipeline(&pipeline).unwrap();
        let layers = graph.execution_layers().unwrap();
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0], vec!["root"]);
        assert_eq!(layers[1].len(), 2);
        assert!(layers[1].contains(&"left".to_string()));
        assert!(layers[1].contains(&"right".to_string()));
    }

    #[test]
    fn cycle_is_rejected() {
        let pipeline = PipelineConfig {
            id: "p".into(),
            description: None,
            schedule: None,
            watch: None,
            steps: vec![step("a", "copy", vec!["b"]), step("b", "copy", vec!["a"])],
        };
        let graph = StageGraph::from_pipeline(&pipeline).unwrap();
        assert!(graph.validate().is_err());
    }
}
