/// React Flow renderer: outputs positioned JSON for React Flow library.
///
/// Produces JSON with nodes and edges in React Flow format:
/// ```json
/// {
///   "nodes": [
///     {"id": "n0", "position": {"x": 100, "y": 50}, "data": {...}, "type": "activityNode"}
///   ],
///   "edges": [
///     {"id": "e0", "source": "n0", "target": "n1", "type": "default"}
///   ]
/// }
/// ```
use std::collections::HashMap;

use serde_json::{json, Value};

use crate::compilers::layout::{sugiyama_layout, LayoutConfig, LayoutEdge, LayoutNode};
use crate::compilers::traits::Renderer;
use crate::models::{ActivityGraph, CostMap, GoalTransitionTree, GraphEnum, ReasoningArtifactDAG};

/// React Flow renderer producing JSON output for browser rendering.
pub struct ReactFlowCompiler {
    config: LayoutConfig,
}

impl ReactFlowCompiler {
    pub fn new() -> Self {
        Self {
            config: LayoutConfig {
                x_spacing: 100.0,
                y_spacing: 150.0,
            },
        }
    }

    pub fn with_config(config: LayoutConfig) -> Self {
        Self { config }
    }
}

impl Default for ReactFlowCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphCompiler for ReactFlowCompiler {
    type Output = Value;

    fn compile(&self, graph: &GraphEnum) -> Self::Output {
        match graph {
            GraphEnum::ActivityGraph(ag) => self.render_activity_graph(ag),
            GraphEnum::CostMap(cm) => self.render_cost_map(cm),
            GraphEnum::GoalTree(gt) => self.render_goal_tree(gt),
            GraphEnum::ReasoningDAG(rd) => self.render_reasoning_dag(rd),
        }
    }

    fn name(&self) -> &'static str {
        "reactflow"
    }
}

impl ReactFlowCompiler {
    fn render_activity_graph(&self, graph: &ActivityGraph) -> Value {
        // Compute node dimensions
        let node_dims: HashMap<_, _> = graph
            .nodes
            .iter()
            .map(|n| {
                let width = 200.0;
                let height = 80.0 + (n.operations.len() as f64 * 20.0);
                (n.node_id.clone(), (width, height))
            })
            .collect();

        // Build layout input
        let layout_nodes: Vec<_> = graph
            .nodes
            .iter()
            .map(|n| LayoutNode {
                id: n.node_id.clone(),
                width: node_dims[&n.node_id].0,
                height: node_dims[&n.node_id].1,
            })
            .collect();

        let layout_edges: Vec<_> = graph
            .edges
            .iter()
            .map(|e| LayoutEdge {
                source: e.source_id.clone(),
                target: e.target_id.clone(),
            })
            .collect();

        let positioned = sugiyama_layout(&layout_nodes, &layout_edges, &self.config);

        // Convert to React Flow format
        let mut nodes = Vec::new();
        let node_map: HashMap<_, _> = graph.nodes.iter().map(|n| (n.node_id.clone(), n)).collect();

        for pnode in &positioned {
            if let Some(node) = node_map.get(&pnode.id) {
                nodes.push(json!({
                    "id": node.node_id,
                    "type": "activityNode",
                    "position": {
                        "x": pnode.x,
                        "y": pnode.y
                    },
                    "data": {
                        "label": node.label,
                        "category": format!("{:?}", node.goal_category),
                        "operations": node.operations.iter().map(|op| json!({
                            "type": format!("{:?}", op.op_type),
                            "detail": op.detail,
                            "callIndex": op.call_index
                        })).collect::<Vec<_>>(),
                        "cost": {
                            "inputTokens": node.total_cost.input_tokens,
                            "outputTokens": node.total_cost.output_tokens,
                            "dollarCost": node.total_cost.dollar_cost
                        }
                    },
                    "width": pnode.width,
                    "height": pnode.height
                }));
            }
        }

        let edges: Vec<_> = graph
            .edges
            .iter()
            .enumerate()
            .map(|(i, e)| {
                json!({
                    "id": format!("e{}", i),
                    "source": e.source_id,
                    "target": e.target_id,
                    "type": "default",
                    "animated": false
                })
            })
            .collect();

        json!({
            "nodes": nodes,
            "edges": edges
        })
    }

    fn render_goal_tree(&self, tree: &GoalTransitionTree) -> Value {
        let node_width = 240.0;
        let node_height = 90.0;

        let layout_nodes: Vec<_> = tree
            .nodes
            .iter()
            .map(|n| LayoutNode {
                id: n.node_id.clone(),
                width: node_width,
                height: node_height,
            })
            .collect();

        let layout_edges: Vec<_> = tree
            .edges
            .iter()
            .filter(|e| {
                e.edge_type == crate::models::GoalEdgeType::Sub
                    || e.edge_type == crate::models::GoalEdgeType::Next
            })
            .map(|e| LayoutEdge {
                source: e.source_id.clone(),
                target: e.target_id.clone(),
            })
            .collect();

        let positioned = sugiyama_layout(&layout_nodes, &layout_edges, &self.config);
        let pos_map: HashMap<String, _> = positioned.iter().map(|p| (p.id.clone(), p)).collect();

        let status_colors: HashMap<&str, &str> = [
            ("done", "#dcfce7"),
            ("failed", "#fecaca"),
            ("partial", "#fef9c3"),
            ("abandoned", "#e5e7eb"),
        ]
        .into_iter()
        .collect();

        let nodes: Vec<_> = tree
            .nodes
            .iter()
            .map(|node| {
                let (x, y) = pos_map
                    .get(&node.node_id)
                    .map(|p| (p.x, p.y))
                    .unwrap_or((0.0, 0.0));
                let status_str = format!("{:?}", node.status).to_lowercase();
                let goal_type_str = format!("{:?}", node.goal_type).to_lowercase();
                let bg = status_colors.get(status_str.as_str()).unwrap_or(&"#fff");
                let border_color = match node.status {
                    crate::models::GoalStatus::Failed => "#dc2626",
                    crate::models::GoalStatus::Done => "#16a34a",
                    _ => "#6b7280",
                };
                json!({
                    "id": node.node_id,
                    "type": "goalNode",
                    "position": {"x": x, "y": y},
                    "data": {
                        "nodeId": node.node_id,
                        "label": node.label,
                        "category": goal_type_str.to_uppercase(),
                        "status": status_str,
                        "result": node.result,
                        "details": node.details,
                        "stepRange": [node.step_range.0, node.step_range.1],
                        "cost": node.cost.dollar_cost,
                    },
                    "style": {
                        "background": bg,
                        "border": format!("2px solid {}", border_color),
                        "borderRadius": "12px",
                        "padding": "0",
                        "width": node_width,
                    },
                })
            })
            .collect();

        let edges: Vec<_> = tree
            .edges
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let (stroke, animated) = match e.edge_type {
                    crate::models::GoalEdgeType::Sub => ("#3b82f6", false),
                    crate::models::GoalEdgeType::Next => ("#22c55e", false),
                    crate::models::GoalEdgeType::Backtrack => ("#ef4444", true),
                };
                json!({
                    "id": format!("e{}", i),
                    "source": e.source_id,
                    "target": e.target_id,
                    "type": "default",
                    "style": {"stroke": stroke, "strokeWidth": 2},
                    "animated": animated,
                    "data": {"edgeType": match e.edge_type {
                        crate::models::GoalEdgeType::Sub => "sub",
                        crate::models::GoalEdgeType::Next => "next",
                        crate::models::GoalEdgeType::Backtrack => "backtrack",
                    }, "label": e.label},
                })
            })
            .collect();

        json!({"nodes": nodes, "edges": edges})
    }

    fn render_reasoning_dag(&self, dag: &ReasoningArtifactDAG) -> Value {
        let node_width = 220.0;
        let node_height = 90.0;

        let layout_nodes: Vec<_> = dag
            .nodes
            .iter()
            .map(|n| LayoutNode {
                id: n.node_id.clone(),
                width: node_width,
                height: node_height,
            })
            .collect();

        let mut layout_edges = Vec::new();
        for edge in &dag.edges {
            for source_id in &edge.source_ids {
                layout_edges.push(LayoutEdge {
                    source: source_id.clone(),
                    target: edge.target_id.clone(),
                });
            }
        }

        let positioned = sugiyama_layout(&layout_nodes, &layout_edges, &self.config);
        let pos_map: HashMap<String, _> = positioned.iter().map(|p| (p.id.clone(), p)).collect();

        let nodes: Vec<_> = dag
            .nodes
            .iter()
            .map(|node| {
                let (x, y) = pos_map
                    .get(&node.node_id)
                    .map(|p| (p.x, p.y))
                    .unwrap_or((0.0, 0.0));
                json!({
                    "id": node.node_id,
                    "type": "reasoningNode",
                    "position": {"x": x, "y": y},
                    "data": {
                        "content": node.content,
                        "nodeType": format!("{:?}", node.node_type).to_lowercase(),
                        "confidence": node.confidence,
                        "sourceStepId": node.source_step_id
                    },
                    "width": node_width,
                    "height": node_height
                })
            })
            .collect();

        let mut edges = Vec::new();
        let mut edge_idx = 0;
        for edge in &dag.edges {
            let edge_type_str = match edge.edge_type {
                crate::models::ReasoningEdgeType::Infers => "infers",
                crate::models::ReasoningEdgeType::Contradicts => "contradicts",
                crate::models::ReasoningEdgeType::Supersedes => "supersedes",
            };
            for source_id in &edge.source_ids {
                edges.push(json!({
                    "id": format!("e{}", edge_idx),
                    "source": source_id,
                    "target": edge.target_id,
                    "type": "default",
                    "label": edge_type_str,
                    "animated": false
                }));
                edge_idx += 1;
            }
        }

        json!({"nodes": nodes, "edges": edges})
    }

    fn render_cost_map(&self, cost_map: &CostMap) -> Value {
        // Render cost map as a single hierarchical node
        json!({
            "nodes": [
                {
                    "id": "cost-root",
                    "type": "costMapNode",
                    "position": {"x": 0, "y": 0},
                    "data": self.serialize_cost_node(&cost_map.root),
                    "width": 800,
                    "height": 600
                }
            ],
            "edges": []
        })
    }

    fn serialize_cost_node(&self, node: &crate::models::CostMapNode) -> Value {
        json!({
            "label": node.label,
            "cost": {
                "inputTokens": node.cost.input_tokens,
                "outputTokens": node.cost.output_tokens,
                "dollarCost": node.cost.dollar_cost
            },
            "category": node.category,
            "children": node.children.iter().map(|c| self.serialize_cost_node(c)).collect::<Vec<_>>()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ActivityNode, Cost, GoalCategory, OpType, Operation};

    #[test]
    fn test_reactflow_output_structure() {
        let graph = ActivityGraph {
            nodes: vec![ActivityNode {
                node_id: "n0".into(),
                label: "test.rs".into(),
                goal_category: GoalCategory::Read,
                primary_object: "/test.rs".into(),
                parent_id: None,
                call_indices: vec![0],
                operations: vec![Operation {
                    op_type: OpType::Read,
                    detail: "L1-L10".into(),
                    call_index: 0,
                }],
                total_cost: Cost::default(),
            }],
            edges: vec![],
        };

        let compiler = ReactFlowCompiler::new();
        let output = renderer.compile(&GraphEnum::ActivityGraph(graph));

        assert!(output.get("nodes").is_some());
        assert!(output.get("edges").is_some());
        assert_eq!(output["nodes"].as_array().unwrap().len(), 1);
    }
}
