use crate::compilers::traits::Renderer;
/// Mermaid renderer: outputs Mermaid.js diagram syntax.
///
/// Produces text in Mermaid diagram language that can be rendered by:
/// - mermaid-cli (`mmdc -i input.mmd -o output.svg`)
/// - Browser via mermaid.js `<script>` tag
/// - GitHub/GitLab markdown fenced blocks (```mermaid)
/// - Any Mermaid-compatible viewer
///
/// Each graph type maps to the most appropriate Mermaid diagram type:
/// - Goal Tree → flowchart TD (top-down)
/// - Reasoning DAG → flowchart TD
/// - Activity Graph → flowchart LR (left-right)
/// - Cost Map → flowchart TD (nested subgraphs)
use crate::models::{ActivityGraph, CostMap, GoalTransitionTree, GraphEnum, ReasoningArtifactDAG};

/// Mermaid.js renderer producing diagram text.
pub struct MermaidCompiler;

impl MermaidCompiler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for MermaidCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphCompiler for MermaidCompiler {
    type Output = String;

    fn compile(&self, graph: &GraphEnum) -> Self::Output {
        match graph {
            GraphEnum::GoalTree(gt) => self.render_goal_tree(gt),
            GraphEnum::ReasoningDAG(rd) => self.render_reasoning_dag(rd),
            GraphEnum::ActivityGraph(ag) => self.render_activity_graph(ag),
            GraphEnum::CostMap(cm) => self.render_cost_map(cm),
        }
    }

    fn name(&self) -> &'static str {
        "mermaid"
    }
}

impl MermaidCompiler {
    fn render_goal_tree(&self, tree: &GoalTransitionTree) -> String {
        let mut out = String::from("flowchart TD\n");

        // Node definitions with status-based styling
        for node in &tree.nodes {
            let shape = match node.status {
                crate::models::GoalStatus::Done => format!(
                    "{}[\"✓ {} ({})\"]",
                    escape_id(&node.node_id),
                    escape_mmd(&node.label),
                    &node.node_id
                ),
                crate::models::GoalStatus::Failed => format!(
                    "{}[\"✗ {} ({})\"]",
                    escape_id(&node.node_id),
                    escape_mmd(&node.label),
                    &node.node_id
                ),
                crate::models::GoalStatus::Partial => format!(
                    "{}[\"◐ {} ({})\"]",
                    escape_id(&node.node_id),
                    escape_mmd(&node.label),
                    &node.node_id
                ),
                crate::models::GoalStatus::Abandoned => format!(
                    "{}[\"⊘ {} ({})\"]",
                    escape_id(&node.node_id),
                    escape_mmd(&node.label),
                    &node.node_id
                ),
            };
            out.push_str(&format!("    {}\n", shape));
        }

        out.push('\n');

        // Edges
        for edge in &tree.edges {
            let (style, label) = match edge.edge_type {
                crate::models::GoalEdgeType::Sub => ("-->", "new plan"),
                crate::models::GoalEdgeType::Next => ("-->", "next"),
                crate::models::GoalEdgeType::Backtrack => ("-.->", "backtrack"),
            };
            out.push_str(&format!(
                "    {} {}|{}| {}\n",
                escape_id(&edge.source_id),
                style,
                label,
                escape_id(&edge.target_id)
            ));
        }

        out.push('\n');

        // Styling by status
        out.push_str("    classDef done fill:#C8E6C9,stroke:#388E3C\n");
        out.push_str("    classDef failed fill:#FFCDD2,stroke:#D32F2F\n");
        out.push_str("    classDef wip fill:#FFF9C4,stroke:#F9A825\n");
        out.push_str("    classDef abandoned fill:#E0E0E0,stroke:#616161\n");

        for node in &tree.nodes {
            let class = match node.status {
                crate::models::GoalStatus::Done => "done",
                crate::models::GoalStatus::Failed => "failed",
                crate::models::GoalStatus::Partial => "wip",
                crate::models::GoalStatus::Abandoned => "abandoned",
            };
            out.push_str(&format!(
                "    class {} {}\n",
                escape_id(&node.node_id),
                class
            ));
        }

        out
    }

    fn render_reasoning_dag(&self, dag: &ReasoningArtifactDAG) -> String {
        let mut out = String::from("flowchart TD\n");

        for node in &dag.nodes {
            let type_marker = match node.node_type {
                crate::models::ReasoningNodeType::GroundTruth => "📌",
                crate::models::ReasoningNodeType::Insight => "💡",
            };
            let label = format!(
                "{} {} (conf:{:.1})",
                type_marker,
                escape_mmd(&node.content),
                node.confidence
            );
            let truncated = if label.len() > 60 {
                format!("{}...", &label[..57])
            } else {
                label
            };
            out.push_str(&format!(
                "    {}[\"{}\"]\n",
                escape_id(&node.node_id),
                truncated
            ));
        }

        out.push('\n');

        for edge in &dag.edges {
            let (style, label) = match edge.edge_type {
                crate::models::ReasoningEdgeType::Infers => ("-->", "infers"),
                crate::models::ReasoningEdgeType::Contradicts => ("-- ✗ -->", "contradicts"),
                crate::models::ReasoningEdgeType::Supersedes => ("-. replaces .->", "supersedes"),
            };
            for source_id in &edge.source_ids {
                out.push_str(&format!(
                    "    {} {}|{}| {}\n",
                    escape_id(source_id),
                    style,
                    label,
                    escape_id(&edge.target_id)
                ));
            }
        }

        out.push('\n');
        out.push_str("    classDef groundTruth fill:#E3F2FD,stroke:#1565C0\n");
        out.push_str("    classDef insight fill:#FFF3E0,stroke:#E65100\n");

        for node in &dag.nodes {
            let class = match node.node_type {
                crate::models::ReasoningNodeType::GroundTruth => "groundTruth",
                crate::models::ReasoningNodeType::Insight => "insight",
            };
            out.push_str(&format!(
                "    class {} {}\n",
                escape_id(&node.node_id),
                class
            ));
        }

        out
    }

    fn render_activity_graph(&self, graph: &ActivityGraph) -> String {
        let mut out = String::from("flowchart LR\n");

        for node in &graph.nodes {
            let ops_summary = if node.operations.len() <= 3 {
                node.operations
                    .iter()
                    .map(|op| format!("{:?}", op.op_type).to_lowercase())
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                format!("{} ops", node.operations.len())
            };
            out.push_str(&format!(
                "    {}[\"{}\\n({})\"]\n",
                escape_id(&node.node_id),
                escape_mmd(&node.label),
                ops_summary
            ));
        }

        out.push('\n');

        for edge in &graph.edges {
            out.push_str(&format!(
                "    {} --> {}\n",
                escape_id(&edge.source_id),
                escape_id(&edge.target_id)
            ));
        }

        out.push('\n');
        out.push_str("    classDef read fill:#e3f2fd,stroke:#1565C0\n");
        out.push_str("    classDef write fill:#fce4ec,stroke:#C62828\n");
        out.push_str("    classDef edit fill:#fff3e0,stroke:#E65100\n");
        out.push_str("    classDef run fill:#e8f5e9,stroke:#2E7D32\n");
        out.push_str("    classDef list fill:#f3e5f5,stroke:#6A1B9A\n");
        out.push_str("    classDef other fill:#eeeeee,stroke:#424242\n");

        for node in &graph.nodes {
            let class = match node.goal_category {
                crate::models::GoalCategory::Read => "read",
                crate::models::GoalCategory::Write => "write",
                crate::models::GoalCategory::Edit => "edit",
                crate::models::GoalCategory::Run => "run",
                crate::models::GoalCategory::List => "list",
                crate::models::GoalCategory::Other => "other",
            };
            out.push_str(&format!(
                "    class {} {}\n",
                escape_id(&node.node_id),
                class
            ));
        }

        out
    }

    fn render_cost_map(&self, cost_map: &CostMap) -> String {
        let mut out = String::from("flowchart TD\n");
        self.render_cost_node(&cost_map.root, &mut out, 0);
        out
    }

    fn render_cost_node(&self, node: &crate::models::CostMapNode, out: &mut String, depth: usize) {
        let indent = "    ".repeat(depth + 1);
        let cost_str = format!("${:.3}", node.cost.dollar_cost);

        if node.children.is_empty() {
            out.push_str(&format!(
                "{}{}[\"{}\\n{}\"]\n",
                indent,
                escape_id(&node.node_id),
                escape_mmd(&node.label),
                cost_str
            ));
        } else {
            out.push_str(&format!(
                "{}subgraph {}[\"{} ({})\"]\n",
                indent,
                escape_id(&node.node_id),
                escape_mmd(&node.label),
                cost_str
            ));
            for child in &node.children {
                self.render_cost_node(child, out, depth + 1);
            }
            out.push_str(&format!("{}end\n", indent));
        }
    }
}

/// Escape node IDs for Mermaid (replace dots with underscores).
fn escape_id(id: &str) -> String {
    id.replace('.', "_").replace('-', "_")
}

/// Escape text for Mermaid labels (quotes and special chars).
fn escape_mmd(text: &str) -> String {
    text.replace('"', "'")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('&', "&amp;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Cost, GoalEdge, GoalEdgeType, GoalNode, GoalStatus, GoalType};

    #[test]
    fn test_goal_tree_output() {
        let tree = GoalTransitionTree {
            root_id: "1".into(),
            nodes: vec![
                GoalNode {
                    node_id: "1".into(),
                    label: "Root goal".into(),
                    goal_type: GoalType::Explore,
                    status: GoalStatus::Done,
                    level: 0,
                    step_range: (0, 10),
                    cost: Cost::default(),
                    result: String::new(),
                    details: String::new(),
                    reasoning_artifacts: vec![],
                },
                GoalNode {
                    node_id: "1.1".into(),
                    label: "Sub goal".into(),
                    goal_type: GoalType::Explore,
                    status: GoalStatus::Done,
                    level: 1,
                    step_range: (0, 5),
                    cost: Cost::default(),
                    result: String::new(),
                    details: String::new(),
                    reasoning_artifacts: vec![],
                },
            ],
            edges: vec![GoalEdge {
                edge_type: GoalEdgeType::Sub,
                source_id: "1".into(),
                target_id: "1.1".into(),
                label: String::new(),
            }],
        };

        let compiler = MermaidCompiler::new();
        let output = renderer.compile(&GraphEnum::GoalTree(tree));

        assert!(output.starts_with("flowchart TD"));
        assert!(output.contains("1_1"));
        assert!(output.contains("new plan"));
        assert!(output.contains("classDef done"));
    }
}
