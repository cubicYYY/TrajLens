use crate::compilers::traits::Renderer;
/// Neo4j renderer: outputs Cypher statements for graph database import.
///
/// Produces a list of Cypher CREATE/MATCH statements to import the graph
/// into Neo4j for querying and analysis.
use crate::models::{ActivityGraph, CostMap, GoalTransitionTree, GraphEnum, ReasoningArtifactDAG};

/// Neo4j renderer producing Cypher statements.
pub struct Neo4jCompiler;

impl Neo4jCompiler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for Neo4jCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphCompiler for Neo4jCompiler {
    type Output = Vec<String>;

    fn compile(&self, graph: &GraphEnum) -> Self::Output {
        match graph {
            GraphEnum::ActivityGraph(ag) => self.render_activity_graph(ag),
            GraphEnum::CostMap(cm) => self.render_cost_map(cm),
            GraphEnum::GoalTree(gt) => self.render_goal_tree(gt),
            GraphEnum::ReasoningDAG(rd) => self.render_reasoning_dag(rd),
        }
    }

    fn name(&self) -> &'static str {
        "neo4j"
    }
}

impl Neo4jCompiler {
    fn render_activity_graph(&self, graph: &ActivityGraph) -> Vec<String> {
        let mut statements = Vec::new();

        // Create nodes
        for node in &graph.nodes {
            statements.push(format!(
                "CREATE (n:Activity {{id: '{}', label: '{}', category: '{}', primaryObject: '{}', cost: {:.4}}})",
                escape_cypher(&node.node_id),
                escape_cypher(&node.label),
                format!("{:?}", node.goal_category),
                escape_cypher(&node.primary_object),
                node.total_cost.dollar_cost
            ));

            // Create operation nodes
            for (i, op) in node.operations.iter().enumerate() {
                statements.push(format!(
                    "CREATE (op:Operation {{id: '{}:op{}', type: '{}', detail: '{}', callIndex: {}}})",
                    escape_cypher(&node.node_id),
                    i,
                    format!("{:?}", op.op_type),
                    escape_cypher(&op.detail),
                    op.call_index
                ));
                statements.push(format!(
                    "MATCH (a:Activity {{id: '{}'}}), (o:Operation {{id: '{}:op{}'}}) CREATE (a)-[:HAS_OPERATION]->(o)",
                    escape_cypher(&node.node_id),
                    escape_cypher(&node.node_id),
                    i
                ));
            }

            // Create parent relationship
            if let Some(parent_id) = &node.parent_id {
                statements.push(format!(
                    "MATCH (child:Activity {{id: '{}'}}), (parent:Activity {{id: '{}'}}) CREATE (child)-[:CHILD_OF]->(parent)",
                    escape_cypher(&node.node_id),
                    escape_cypher(parent_id)
                ));
            }
        }

        // Create edges
        for edge in &graph.edges {
            statements.push(format!(
                "MATCH (a:Activity {{id: '{}'}}), (b:Activity {{id: '{}'}}) CREATE (a)-[:NEXT]->(b)",
                escape_cypher(&edge.source_id),
                escape_cypher(&edge.target_id)
            ));
        }

        statements
    }

    fn render_goal_tree(&self, tree: &GoalTransitionTree) -> Vec<String> {
        let mut statements = Vec::new();

        for node in &tree.nodes {
            statements.push(format!(
                "CREATE (n:Goal {{id: '{}', label: '{}', status: '{}', goalType: '{}', stepStart: {}, stepEnd: {}}})",
                escape_cypher(&node.node_id),
                escape_cypher(&node.label),
                format!("{:?}", node.status).to_lowercase(),
                format!("{:?}", node.goal_type).to_lowercase(),
                node.step_range.0,
                node.step_range.1
            ));
        }

        for edge in &tree.edges {
            let rel_type = match edge.edge_type {
                crate::models::GoalEdgeType::Sub => "SUB",
                crate::models::GoalEdgeType::Next => "NEXT",
                crate::models::GoalEdgeType::Backtrack => "BACKTRACK",
            };
            statements.push(format!(
                "MATCH (a:Goal {{id: '{}'}}), (b:Goal {{id: '{}'}}) CREATE (a)-[:{}]->(b)",
                escape_cypher(&edge.source_id),
                escape_cypher(&edge.target_id),
                rel_type
            ));
        }

        statements
    }

    fn render_reasoning_dag(&self, dag: &ReasoningArtifactDAG) -> Vec<String> {
        let mut statements = Vec::new();

        for node in &dag.nodes {
            statements.push(format!(
                "CREATE (n:Reasoning {{id: '{}', nodeType: '{}', content: '{}', confidence: {:.2}, sourceStep: {}}})",
                escape_cypher(&node.node_id),
                format!("{:?}", node.node_type).to_lowercase(),
                escape_cypher(&node.content),
                node.confidence,
                node.source_step_id
            ));
        }

        for edge in &dag.edges {
            let rel_type = match edge.edge_type {
                crate::models::ReasoningEdgeType::Infers => "INFERS",
                crate::models::ReasoningEdgeType::Contradicts => "CONTRADICTS",
                crate::models::ReasoningEdgeType::Supersedes => "SUPERSEDES",
            };
            for source_id in &edge.source_ids {
                statements.push(format!(
                    "MATCH (a:Reasoning {{id: '{}'}}), (b:Reasoning {{id: '{}'}}) CREATE (a)-[:{}]->(b)",
                    escape_cypher(source_id),
                    escape_cypher(&edge.target_id),
                    rel_type
                ));
            }
        }

        statements
    }

    fn render_cost_map(&self, cost_map: &CostMap) -> Vec<String> {
        let mut statements = Vec::new();
        self.render_cost_node(&cost_map.root, None, &mut statements);
        statements
    }

    fn render_cost_node(
        &self,
        node: &crate::models::CostMapNode,
        parent_id: Option<&str>,
        statements: &mut Vec<String>,
    ) {
        // Create node
        statements.push(format!(
            "CREATE (n:CostNode {{id: '{}', label: '{}', cost: {:.4}, category: '{}'}})",
            escape_cypher(&node.node_id),
            escape_cypher(&node.label),
            node.cost.dollar_cost,
            node.category.as_ref().unwrap_or(&"container".to_string())
        ));

        // Create parent relationship
        if let Some(pid) = parent_id {
            statements.push(format!(
                "MATCH (child:CostNode {{id: '{}'}}), (parent:CostNode {{id: '{}'}}) CREATE (child)-[:PART_OF]->(parent)",
                escape_cypher(&node.node_id),
                pid
            ));
        }

        // Recurse for children
        for child in &node.children {
            self.render_cost_node(child, Some(&node.node_id), statements);
        }
    }
}

/// Escape single quotes in Cypher strings.
fn escape_cypher(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ActivityNode, Cost, GoalCategory, OpType, Operation};

    #[test]
    fn test_neo4j_output_format() {
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

        let compiler = Neo4jCompiler::new();
        let output = renderer.compile(&GraphEnum::ActivityGraph(graph));

        assert!(!output.is_empty());
        assert!(output[0].starts_with("CREATE"));
    }

    #[test]
    fn test_escape_cypher() {
        assert_eq!(escape_cypher("test's"), "test\\'s");
        assert_eq!(escape_cypher("test\\path"), "test\\\\path");
    }
}
