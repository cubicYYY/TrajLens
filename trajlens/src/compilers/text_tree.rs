/// Text tree renderer: outputs a plain-text indented tree representation.
///
/// Example output:
///   ROOT [ACT] (failed) Achieve RCE on target
///   ├── 1 [EXPLORE] (done) Reconnaissance and source analysis
///   │   ├── 1.1 [EXPLORE] (done) Enumerate files and configs
///   │   └── 1.2 [EXPLORE] (partial) Check nginx and eval patterns
///   ├── 2 [ACT] (failed) Vulnerability testing
///   │   ├── 2.1 [ACT] (failed) SQL injection and SSTI attempts
///   │   └── 2.2 [ACT] (failed) Path traversal and debug pin
///   └── 3 [ACT] (failed) File-write RCE via __init__.py
///       ├── 3.1 [EXPLORE] (done) Identify writable directory
///       └── 3.2 [ACT] (failed) Deploy and trigger payload
use std::collections::HashMap;

use crate::models::{GoalEdgeType, GoalTransitionTree, GraphEnum};

use super::GraphCompiler;

pub struct TextTreeCompiler;

impl TextTreeCompiler {
    pub fn new() -> Self {
        Self
    }
}

impl Default for TextTreeCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphCompiler for TextTreeCompiler {
    type Output = String;

    fn compile(&self, graph: &GraphEnum) -> Self::Output {
        match graph {
            GraphEnum::GoalTree(tree) => render_goal_tree_text(tree),
            _ => "(text tree only supports GoalTree)".to_string(),
        }
    }

    fn name(&self) -> &'static str {
        "text_tree"
    }
}

fn render_goal_tree_text(tree: &GoalTransitionTree) -> String {
    // Build children map: parent → ordered children list
    let mut children_of: HashMap<&str, Vec<&str>> = HashMap::new();

    for edge in &tree.edges {
        if edge.edge_type == GoalEdgeType::Sub {
            // Follow next chain from first child to build ordered list
            let mut chain = vec![edge.target_id.as_str()];
            let mut current = edge.target_id.as_str();
            loop {
                let next = tree
                    .edges
                    .iter()
                    .find(|e| e.source_id == current && e.edge_type == GoalEdgeType::Next);
                match next {
                    Some(e) => {
                        chain.push(&e.target_id);
                        current = &e.target_id;
                    }
                    None => break,
                }
            }
            children_of.insert(&edge.source_id, chain);
        }
    }

    let node_map: HashMap<&str, &crate::models::GoalNode> =
        tree.nodes.iter().map(|n| (n.node_id.as_str(), n)).collect();

    let mut output = String::new();
    let root_id = if tree.root_id.is_empty() {
        tree.nodes
            .first()
            .map(|n| n.node_id.as_str())
            .unwrap_or("ROOT")
    } else {
        &tree.root_id
    };

    render_node(
        &mut output,
        root_id,
        "",
        true,
        true,
        &children_of,
        &node_map,
    );
    output
}

fn render_node(
    output: &mut String,
    node_id: &str,
    prefix: &str,
    is_root: bool,
    is_last: bool,
    children_of: &HashMap<&str, Vec<&str>>,
    node_map: &HashMap<&str, &crate::models::GoalNode>,
) {
    let node = match node_map.get(node_id) {
        Some(n) => n,
        None => return,
    };

    let cat = match node.goal_type {
        crate::models::GoalType::Explore => "EXPLORE",
        crate::models::GoalType::Think => "THINK",
        crate::models::GoalType::Act => "ACT",
    };
    let status = match node.status {
        crate::models::GoalStatus::Done => "done",
        crate::models::GoalStatus::Failed => "failed",
        crate::models::GoalStatus::Partial => "partial",
        crate::models::GoalStatus::Abandoned => "abandoned",
    };

    let result_part = if node.result.is_empty() {
        String::new()
    } else {
        format!(" → {}", node.result)
    };

    if is_root {
        output.push_str(&format!(
            "{} [{}] ({}) {}{}\n",
            node.node_id, cat, status, node.label, result_part
        ));
    } else {
        let connector = if is_last { "└── " } else { "├── " };
        output.push_str(&format!(
            "{}{}{} [{}] ({}) {}{}\n",
            prefix, connector, node.node_id, cat, status, node.label, result_part
        ));
    }

    if let Some(children) = children_of.get(node_id) {
        let child_prefix = if is_root {
            "".to_string()
        } else if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };

        for (i, &child_id) in children.iter().enumerate() {
            let child_is_last = i == children.len() - 1;
            render_node(
                output,
                child_id,
                &child_prefix,
                false,
                child_is_last,
                children_of,
                node_map,
            );
        }
    }
}
