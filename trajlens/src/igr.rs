/// IGR (Intermediate Graph Representation) serialization and deserialization.
///
/// IGR is the canonical interchange format between graph builders and renderers.
/// Uses TOML as the wire format. Each graph type serializes to a self-describing
/// TOML document with a "graph_type" discriminator.
///
/// Every graph MUST pass through IGR before rendering.
use toml::Value;

use crate::models::{
    ActivityEdge, ActivityGraph, ActivityNode, Cost, CostMap, CostMapNode, GoalCategory, GoalEdge,
    GoalEdgeType, GoalNode, GoalStatus, GoalTransitionTree, GoalType, GraphEnum, InsightStatus,
    OpType, Operation, ReasoningArtifactDAG, ReasoningArtifactNode, ReasoningEdge,
    ReasoningEdgeType, ReasoningNodeType,
};

#[derive(Debug)]
pub enum IgrError {
    TomlSer(String),
    TomlDe(String),
    UnknownGraphType(String),
    MissingField(String),
}

impl std::fmt::Display for IgrError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IgrError::TomlSer(msg) => write!(f, "TOML serialization error: {}", msg),
            IgrError::TomlDe(msg) => write!(f, "TOML deserialization error: {}", msg),
            IgrError::UnknownGraphType(t) => write!(f, "unknown graph type: {}", t),
            IgrError::MissingField(field) => write!(f, "missing field: {}", field),
        }
    }
}

impl std::error::Error for IgrError {}

/// Serialize a GraphEnum to IGR TOML string.
pub fn serialize(graph: &GraphEnum) -> Result<String, IgrError> {
    match graph {
        GraphEnum::GoalTree(tree) => serialize_goal_tree(tree),
        GraphEnum::ReasoningDAG(dag) => serialize_reasoning_dag(dag),
        GraphEnum::ActivityGraph(ag) => serialize_activity_graph(ag),
        GraphEnum::CostMap(cm) => serialize_cost_map(cm),
    }
}

/// Deserialize an IGR TOML string into a GraphEnum.
pub fn deserialize(toml_str: &str) -> Result<GraphEnum, IgrError> {
    let value: Value = toml_str
        .parse::<Value>()
        .map_err(|e| IgrError::TomlDe(e.to_string()))?;

    let graph_type = value
        .get("graph_type")
        .and_then(|v| v.as_str())
        .ok_or_else(|| IgrError::MissingField("graph_type".into()))?;

    match graph_type {
        "goal_transition_tree" => deserialize_goal_tree(&value).map(GraphEnum::GoalTree),
        "reasoning_artifact_dag" => deserialize_reasoning_dag(&value).map(GraphEnum::ReasoningDAG),
        "activity_graph" => deserialize_activity_graph(&value).map(GraphEnum::ActivityGraph),
        "cost_map" => deserialize_cost_map(&value).map(GraphEnum::CostMap),
        other => Err(IgrError::UnknownGraphType(other.into())),
    }
}

// --- Goal Transition Tree ---

fn serialize_goal_tree(tree: &GoalTransitionTree) -> Result<String, IgrError> {
    let mut out = String::new();
    out.push_str("graph_type = \"goal_transition_tree\"\n");
    out.push_str(&format!("root_id = \"{}\"\n", tree.root_id));

    for node in &tree.nodes {
        out.push_str("\n[[nodes]]\n");
        out.push_str(&format!("node_id = \"{}\"\n", node.node_id));
        out.push_str(&format!(
            "label = \"{}\"\n",
            escape_toml_string(&node.label)
        ));
        out.push_str(&format!(
            "goal_type = \"{}\"\n",
            goal_type_str(&node.goal_type)
        ));
        out.push_str(&format!("status = \"{}\"\n", goal_status_str(&node.status)));
        if !node.result.is_empty() {
            out.push_str(&format!(
                "result = \"{}\"\n",
                escape_toml_string(&node.result)
            ));
        }
        if !node.details.is_empty() {
            out.push_str(&format!(
                "details = \"{}\"\n",
                escape_toml_string(&node.details)
            ));
        }
        out.push_str(&format!("level = {}\n", node.level));
        out.push_str(&format!(
            "step_range = [{}, {}]\n",
            node.step_range.0, node.step_range.1
        ));
        out.push_str(&serialize_cost_inline(&node.cost));
        out.push_str(&format!(
            "reasoning_artifacts = [{}]\n",
            node.reasoning_artifacts
                .iter()
                .map(|s| format!("\"{}\"", escape_toml_string(s)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }

    for edge in &tree.edges {
        out.push_str("\n[[edges]]\n");
        out.push_str(&format!(
            "edge_type = \"{}\"\n",
            goal_edge_type_str(&edge.edge_type)
        ));
        out.push_str(&format!("source_id = \"{}\"\n", edge.source_id));
        out.push_str(&format!("target_id = \"{}\"\n", edge.target_id));
        out.push_str(&format!(
            "label = \"{}\"\n",
            escape_toml_string(&edge.label)
        ));
    }

    Ok(out)
}

fn deserialize_goal_tree(value: &Value) -> Result<GoalTransitionTree, IgrError> {
    let root_id = get_str(value, "root_id")?;

    let nodes_arr = value
        .get("nodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| IgrError::MissingField("nodes".into()))?;

    let mut nodes = Vec::new();
    for n in nodes_arr {
        nodes.push(GoalNode {
            node_id: get_str(n, "node_id")?,
            label: get_str(n, "label")?,
            goal_type: parse_goal_type(&get_str_or(n, "goal_type", "explore"))?,
            status: parse_goal_status(&get_str(n, "status")?)?,
            result: get_str_or(n, "result", ""),
            details: get_str_or(n, "details", ""),
            level: get_usize(n, "level")?,
            step_range: {
                let arr = n
                    .get("step_range")
                    .and_then(|v| v.as_array())
                    .ok_or_else(|| IgrError::MissingField("step_range".into()))?;
                (
                    arr[0].as_integer().unwrap_or(0) as usize,
                    arr[1].as_integer().unwrap_or(0) as usize,
                )
            },
            cost: parse_cost(
                n.get("cost")
                    .ok_or_else(|| IgrError::MissingField("cost".into()))?,
            )?,
            reasoning_artifacts: n
                .get("reasoning_artifacts")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
        });
    }

    let edges_arr = value
        .get("edges")
        .and_then(|v| v.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    let mut edges = Vec::new();
    for e in edges_arr {
        edges.push(GoalEdge {
            edge_type: parse_goal_edge_type(&get_str(e, "edge_type")?)?,
            source_id: get_str(e, "source_id")?,
            target_id: get_str(e, "target_id")?,
            label: get_str_or(e, "label", ""),
        });
    }

    Ok(GoalTransitionTree {
        nodes,
        edges,
        root_id,
    })
}

// --- Reasoning Artifact DAG ---

fn serialize_reasoning_dag(dag: &ReasoningArtifactDAG) -> Result<String, IgrError> {
    let mut out = String::new();
    out.push_str("graph_type = \"reasoning_artifact_dag\"\n");

    for node in &dag.nodes {
        out.push_str("\n[[nodes]]\n");
        out.push_str(&format!("node_id = \"{}\"\n", node.node_id));
        out.push_str(&format!(
            "node_type = \"{}\"\n",
            reasoning_node_type_str(&node.node_type)
        ));
        out.push_str(&format!(
            "content = \"{}\"\n",
            escape_toml_string(&node.content)
        ));
        out.push_str(&format!("source_step_id = {}\n", node.source_step_id));
        out.push_str(&format!("confidence = {}\n", node.confidence));
        let status_str = match &node.status {
            Some(s) => insight_status_str(s),
            None => "",
        };
        out.push_str(&format!("status = \"{}\"\n", status_str));
        if let Some((start, end)) = node.step_range {
            out.push_str(&format!("step_range = [{}, {}]\n", start, end));
        }
    }

    for edge in &dag.edges {
        out.push_str("\n[[edges]]\n");
        out.push_str(&format!(
            "edge_type = \"{}\"\n",
            reasoning_edge_type_str(&edge.edge_type)
        ));
        out.push_str(&format!(
            "source_ids = [{}]\n",
            edge.source_ids
                .iter()
                .map(|s| format!("\"{}\"", s))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        out.push_str(&format!("target_id = \"{}\"\n", edge.target_id));
    }

    Ok(out)
}

fn deserialize_reasoning_dag(value: &Value) -> Result<ReasoningArtifactDAG, IgrError> {
    let nodes_arr = value
        .get("nodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| IgrError::MissingField("nodes".into()))?;

    let mut nodes = Vec::new();
    for n in nodes_arr {
        let status_str = get_str_or(n, "status", "");
        let status = if status_str.is_empty() {
            None
        } else {
            Some(parse_insight_status(&status_str)?)
        };
        let step_range = n
            .get("step_range")
            .and_then(|v| v.as_array())
            .and_then(|arr| {
                if arr.len() == 2 {
                    let start = arr[0].as_integer().unwrap_or(0) as usize;
                    let end = arr[1].as_integer().unwrap_or(0) as usize;
                    Some((start, end))
                } else {
                    None
                }
            });
        nodes.push(ReasoningArtifactNode {
            node_id: get_str(n, "node_id")?,
            node_type: parse_reasoning_node_type(&get_str(n, "node_type")?)?,
            content: get_str(n, "content")?,
            source_step_id: get_usize(n, "source_step_id")?,
            confidence: get_f64(n, "confidence"),
            status,
            step_range,
        });
    }

    let edges_arr = value
        .get("edges")
        .and_then(|v| v.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    let mut edges = Vec::new();
    for e in edges_arr {
        edges.push(ReasoningEdge {
            edge_type: parse_reasoning_edge_type(&get_str(e, "edge_type")?)?,
            source_ids: e
                .get("source_ids")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            target_id: get_str(e, "target_id")?,
        });
    }

    Ok(ReasoningArtifactDAG { nodes, edges })
}

// --- Activity Graph ---

fn serialize_activity_graph(ag: &ActivityGraph) -> Result<String, IgrError> {
    let mut out = String::new();
    out.push_str("graph_type = \"activity_graph\"\n");

    for node in &ag.nodes {
        out.push_str("\n[[nodes]]\n");
        out.push_str(&format!("node_id = \"{}\"\n", node.node_id));
        out.push_str(&format!(
            "label = \"{}\"\n",
            escape_toml_string(&node.label)
        ));
        out.push_str(&format!(
            "goal_category = \"{}\"\n",
            node.goal_category.as_str()
        ));
        out.push_str(&format!(
            "primary_object = \"{}\"\n",
            escape_toml_string(&node.primary_object)
        ));
        out.push_str(&format!(
            "parent_id = \"{}\"\n",
            node.parent_id.as_deref().unwrap_or("")
        ));
        out.push_str(&format!(
            "call_indices = [{}]\n",
            node.call_indices
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));

        for op in &node.operations {
            out.push_str("\n[[nodes.operations]]\n");
            out.push_str(&format!("op_type = \"{}\"\n", op_type_str(&op.op_type)));
            out.push_str(&format!(
                "detail = \"{}\"\n",
                escape_toml_string(&op.detail)
            ));
            out.push_str(&format!("call_index = {}\n", op.call_index));
        }

        out.push_str(&format!(
            "\n{}",
            serialize_cost_section(&node.total_cost, "nodes.total_cost")
        ));
    }

    for edge in &ag.edges {
        out.push_str("\n[[edges]]\n");
        out.push_str(&format!("edge_type = \"{}\"\n", edge.edge_type));
        out.push_str(&format!("source_id = \"{}\"\n", edge.source_id));
        out.push_str(&format!(
            "source_operation_index = {}\n",
            edge.source_operation_index
        ));
        out.push_str(&format!("target_id = \"{}\"\n", edge.target_id));
        out.push_str(&format!(
            "target_operation_index = {}\n",
            edge.target_operation_index
        ));
    }

    Ok(out)
}

fn deserialize_activity_graph(value: &Value) -> Result<ActivityGraph, IgrError> {
    let nodes_arr = value
        .get("nodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| IgrError::MissingField("nodes".into()))?;

    let mut nodes = Vec::new();
    for n in nodes_arr {
        let parent_id_str = get_str_or(n, "parent_id", "");
        let parent_id = if parent_id_str.is_empty() {
            None
        } else {
            Some(parent_id_str)
        };

        let ops_arr = n
            .get("operations")
            .and_then(|v| v.as_array())
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let operations: Vec<Operation> = ops_arr
            .iter()
            .map(|o| Operation {
                op_type: parse_op_type(&get_str_or(o, "op_type", "other")),
                detail: get_str_or(o, "detail", ""),
                call_index: o
                    .get("call_index")
                    .and_then(|v| v.as_integer())
                    .unwrap_or(0) as usize,
            })
            .collect();

        nodes.push(ActivityNode {
            node_id: get_str(n, "node_id")?,
            label: get_str(n, "label")?,
            goal_category: parse_goal_category(&get_str(n, "goal_category")?),
            primary_object: get_str_or(n, "primary_object", ""),
            parent_id,
            call_indices: n
                .get("call_indices")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_integer().map(|i| i as usize))
                        .collect()
                })
                .unwrap_or_default(),
            operations,
            total_cost: parse_cost(
                n.get("total_cost")
                    .unwrap_or(&Value::Table(Default::default())),
            )?,
        });
    }

    let edges_arr = value
        .get("edges")
        .and_then(|v| v.as_array())
        .map(|v| v.as_slice())
        .unwrap_or(&[]);

    let mut edges = Vec::new();
    for e in edges_arr {
        edges.push(ActivityEdge {
            edge_type: get_str_or(e, "edge_type", "next"),
            source_id: get_str(e, "source_id")?,
            source_operation_index: e
                .get("source_operation_index")
                .and_then(|v| v.as_integer())
                .unwrap_or(0) as usize,
            target_id: get_str(e, "target_id")?,
            target_operation_index: e
                .get("target_operation_index")
                .and_then(|v| v.as_integer())
                .unwrap_or(0) as usize,
        });
    }

    Ok(ActivityGraph { nodes, edges })
}

// --- Cost Map ---

fn serialize_cost_map(cm: &CostMap) -> Result<String, IgrError> {
    let mut out = String::new();
    out.push_str("graph_type = \"cost_map\"\n\n");
    out.push_str("[root]\n");
    serialize_cost_map_node(&mut out, &cm.root, "root");
    Ok(out)
}

fn serialize_cost_map_node(out: &mut String, node: &CostMapNode, prefix: &str) {
    out.push_str(&format!("node_id = \"{}\"\n", node.node_id));
    out.push_str(&format!(
        "label = \"{}\"\n",
        escape_toml_string(&node.label)
    ));
    out.push_str(&format!(
        "category = \"{}\"\n",
        node.category.as_deref().unwrap_or("")
    ));
    if let Some((start, end)) = node.step_range {
        out.push_str(&format!("step_range = [{}, {}]\n", start, end));
    }

    out.push_str(&format!("\n[{}.cost]\n", prefix));
    out.push_str(&format!("input_tokens = {}\n", node.cost.input_tokens));
    out.push_str(&format!("output_tokens = {}\n", node.cost.output_tokens));
    out.push_str(&format!(
        "cache_read_tokens = {}\n",
        node.cost.cache_read_tokens
    ));
    out.push_str(&format!(
        "cache_write_tokens = {}\n",
        node.cost.cache_write_tokens
    ));
    out.push_str(&format!("dollar_cost = {}\n", node.cost.dollar_cost));

    for (i, child) in node.children.iter().enumerate() {
        let child_prefix = format!("{}.children", prefix);
        out.push_str(&format!("\n[[{child_prefix}]]\n"));
        serialize_cost_map_node_nested(out, child, &format!("{child_prefix}[{i}]"));
    }
}

fn serialize_cost_map_node_nested(out: &mut String, node: &CostMapNode, _prefix: &str) {
    out.push_str(&format!("node_id = \"{}\"\n", node.node_id));
    out.push_str(&format!(
        "label = \"{}\"\n",
        escape_toml_string(&node.label)
    ));
    out.push_str(&format!(
        "category = \"{}\"\n",
        node.category.as_deref().unwrap_or("")
    ));
    if let Some((start, end)) = node.step_range {
        out.push_str(&format!("step_range = [{}, {}]\n", start, end));
    }
    out.push_str(&format!("cost = {{ input_tokens = {}, output_tokens = {}, cache_read_tokens = {}, cache_write_tokens = {}, dollar_cost = {} }}\n",
        node.cost.input_tokens, node.cost.output_tokens, node.cost.cache_read_tokens, node.cost.cache_write_tokens, node.cost.dollar_cost));
    if node.children.is_empty() {
        out.push_str("children = []\n");
    } else {
        // Serialize children as inline tables (leaf items only get node_id, label, cost)
        out.push_str("children = [");
        for (i, child) in node.children.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            let sr = child
                .step_range
                .map(|(s, e)| format!(", step_range = [{}, {}]", s, e))
                .unwrap_or_default();
            if child.children.is_empty() {
                out.push_str(&format!(
                    "{{ node_id = \"{}\", label = \"{}\", category = \"{}\", cost = {{ input_tokens = {}, output_tokens = {}, cache_read_tokens = {}, cache_write_tokens = {}, dollar_cost = {} }}{}, children = [] }}",
                    child.node_id,
                    escape_toml_string(&child.label),
                    child.category.as_deref().unwrap_or(""),
                    child.cost.input_tokens, child.cost.output_tokens,
                    child.cost.cache_read_tokens, child.cost.cache_write_tokens,
                    child.cost.dollar_cost,
                    sr,
                ));
            } else {
                // Child has grandchildren — serialize recursively (but still inline for TOML compat)
                out.push_str(&format!(
                    "{{ node_id = \"{}\", label = \"{}\", category = \"{}\", cost = {{ input_tokens = {}, output_tokens = {}, cache_read_tokens = {}, cache_write_tokens = {}, dollar_cost = {} }}{}, children = [",
                    child.node_id,
                    escape_toml_string(&child.label),
                    child.category.as_deref().unwrap_or(""),
                    child.cost.input_tokens, child.cost.output_tokens,
                    child.cost.cache_read_tokens, child.cost.cache_write_tokens,
                    child.cost.dollar_cost,
                    sr,
                ));
                for (j, grandchild) in child.children.iter().enumerate() {
                    if j > 0 {
                        out.push_str(", ");
                    }
                    let gsr = grandchild
                        .step_range
                        .map(|(s, e)| format!(", step_range = [{}, {}]", s, e))
                        .unwrap_or_default();
                    out.push_str(&format!(
                        "{{ node_id = \"{}\", label = \"{}\", category = \"{}\", cost = {{ input_tokens = {}, output_tokens = {}, cache_read_tokens = {}, cache_write_tokens = {}, dollar_cost = {} }}{}, children = [] }}",
                        grandchild.node_id,
                        escape_toml_string(&grandchild.label),
                        grandchild.category.as_deref().unwrap_or(""),
                        grandchild.cost.input_tokens, grandchild.cost.output_tokens,
                        grandchild.cost.cache_read_tokens, grandchild.cost.cache_write_tokens,
                        grandchild.cost.dollar_cost,
                        gsr,
                    ));
                }
                out.push_str("] }");
            }
        }
        out.push_str("]\n");
    }
}

fn deserialize_cost_map(value: &Value) -> Result<CostMap, IgrError> {
    let root_value = value
        .get("root")
        .ok_or_else(|| IgrError::MissingField("root".into()))?;
    let root = deserialize_cost_map_node(root_value)?;
    Ok(CostMap { root })
}

fn deserialize_cost_map_node(value: &Value) -> Result<CostMapNode, IgrError> {
    let category_str = get_str_or(value, "category", "");
    let category = if category_str.is_empty() {
        None
    } else {
        Some(category_str)
    };

    let children = value
        .get("children")
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .map(|c| deserialize_cost_map_node(c))
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();

    let step_range = value
        .get("step_range")
        .and_then(|v| v.as_array())
        .and_then(|arr| {
            if arr.len() == 2 {
                Some((
                    arr[0].as_integer().unwrap_or(0) as usize,
                    arr[1].as_integer().unwrap_or(0) as usize,
                ))
            } else {
                None
            }
        });

    Ok(CostMapNode {
        node_id: get_str(value, "node_id")?,
        label: get_str_or(value, "label", ""),
        cost: parse_cost(
            value
                .get("cost")
                .unwrap_or(&Value::Table(Default::default())),
        )?,
        children,
        category,
        step_range,
    })
}

// --- Helpers ---

fn get_str(value: &Value, field: &str) -> Result<String, IgrError> {
    value
        .get(field)
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| IgrError::MissingField(field.into()))
}

fn get_str_or(value: &Value, field: &str, default: &str) -> String {
    value
        .get(field)
        .and_then(|v| v.as_str())
        .unwrap_or(default)
        .to_string()
}

fn get_usize(value: &Value, field: &str) -> Result<usize, IgrError> {
    value
        .get(field)
        .and_then(|v| v.as_integer())
        .map(|i| i as usize)
        .ok_or_else(|| IgrError::MissingField(field.into()))
}

fn get_f64(value: &Value, field: &str) -> f64 {
    value
        .get(field)
        .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
        .unwrap_or(0.0)
}

fn parse_cost(value: &Value) -> Result<Cost, IgrError> {
    Ok(Cost {
        input_tokens: value
            .get("input_tokens")
            .and_then(|v| v.as_integer())
            .unwrap_or(0),
        output_tokens: value
            .get("output_tokens")
            .and_then(|v| v.as_integer())
            .unwrap_or(0),
        cache_read_tokens: value
            .get("cache_read_tokens")
            .and_then(|v| v.as_integer())
            .unwrap_or(0),
        cache_write_tokens: value
            .get("cache_write_tokens")
            .and_then(|v| v.as_integer())
            .unwrap_or(0),
        dollar_cost: value
            .get("dollar_cost")
            .and_then(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
            .unwrap_or(0.0),
    })
}

fn serialize_cost_inline(cost: &Cost) -> String {
    format!("cost = {{ input_tokens = {}, output_tokens = {}, cache_read_tokens = {}, cache_write_tokens = {}, dollar_cost = {} }}\n",
        cost.input_tokens, cost.output_tokens, cost.cache_read_tokens, cost.cache_write_tokens, cost.dollar_cost)
}

fn serialize_cost_section(cost: &Cost, section: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("[{}]\n", section));
    out.push_str(&format!("input_tokens = {}\n", cost.input_tokens));
    out.push_str(&format!("output_tokens = {}\n", cost.output_tokens));
    out.push_str(&format!("cache_read_tokens = {}\n", cost.cache_read_tokens));
    out.push_str(&format!(
        "cache_write_tokens = {}\n",
        cost.cache_write_tokens
    ));
    out.push_str(&format!("dollar_cost = {}\n", cost.dollar_cost));
    out
}

fn escape_toml_string(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

fn goal_type_str(gt: &GoalType) -> &'static str {
    match gt {
        GoalType::Explore => "explore",
        GoalType::Think => "think",
        GoalType::Act => "act",
    }
}

fn parse_goal_type(s: &str) -> Result<GoalType, IgrError> {
    match s {
        "explore" => Ok(GoalType::Explore),
        "think" | "analyze" | "plan" => Ok(GoalType::Think),
        "write" | "act" | "verify" | "report" | "execute" => Ok(GoalType::Act),
        _ => Err(IgrError::TomlDe(format!("unknown goal_type: {}", s))),
    }
}

fn goal_status_str(gs: &GoalStatus) -> &'static str {
    match gs {
        GoalStatus::Done => "done",
        GoalStatus::Failed => "failed",
        GoalStatus::Abandoned => "abandoned",
        GoalStatus::Partial => "partial",
    }
}

fn parse_goal_status(s: &str) -> Result<GoalStatus, IgrError> {
    match s {
        "done" => Ok(GoalStatus::Done),
        "failed" => Ok(GoalStatus::Failed),
        "abandoned" => Ok(GoalStatus::Abandoned),
        "partial" => Ok(GoalStatus::Partial),
        _ => Err(IgrError::TomlDe(format!("unknown goal status: {}", s))),
    }
}

fn goal_edge_type_str(et: &GoalEdgeType) -> &'static str {
    match et {
        GoalEdgeType::Next => "next",
        GoalEdgeType::Backtrack => "backtrack",
        GoalEdgeType::Sub => "sub",
    }
}

fn parse_goal_edge_type(s: &str) -> Result<GoalEdgeType, IgrError> {
    match s {
        "next" => Ok(GoalEdgeType::Next),
        "backtrack" => Ok(GoalEdgeType::Backtrack),
        "sub" => Ok(GoalEdgeType::Sub),
        _ => Err(IgrError::TomlDe(format!("unknown goal edge type: {}", s))),
    }
}

fn reasoning_node_type_str(nt: &ReasoningNodeType) -> &'static str {
    match nt {
        ReasoningNodeType::GroundTruth => "ground_truth",
        ReasoningNodeType::Insight => "insight",
    }
}

fn parse_reasoning_node_type(s: &str) -> Result<ReasoningNodeType, IgrError> {
    match s {
        "ground_truth" => Ok(ReasoningNodeType::GroundTruth),
        "insight" => Ok(ReasoningNodeType::Insight),
        _ => Err(IgrError::TomlDe(format!(
            "unknown reasoning node type: {}",
            s
        ))),
    }
}

fn insight_status_str(s: &InsightStatus) -> &'static str {
    match s {
        InsightStatus::Verified => "verified",
        InsightStatus::SelfFalsed => "self-falsed",
        InsightStatus::Unverified => "unverified",
    }
}

fn parse_insight_status(s: &str) -> Result<InsightStatus, IgrError> {
    match s {
        "verified" => Ok(InsightStatus::Verified),
        "self-falsed" => Ok(InsightStatus::SelfFalsed),
        "unverified" => Ok(InsightStatus::Unverified),
        _ => Err(IgrError::TomlDe(format!("unknown insight status: {}", s))),
    }
}

fn reasoning_edge_type_str(et: &ReasoningEdgeType) -> &'static str {
    match et {
        ReasoningEdgeType::Infers => "infers",
        ReasoningEdgeType::Contradicts => "contradicts",
        ReasoningEdgeType::Supersedes => "supersedes",
    }
}

fn parse_reasoning_edge_type(s: &str) -> Result<ReasoningEdgeType, IgrError> {
    match s {
        "infers" => Ok(ReasoningEdgeType::Infers),
        "contradicts" => Ok(ReasoningEdgeType::Contradicts),
        "supersedes" => Ok(ReasoningEdgeType::Supersedes),
        _ => Err(IgrError::TomlDe(format!(
            "unknown reasoning edge type: {}",
            s
        ))),
    }
}

fn op_type_str(ot: &OpType) -> &'static str {
    match ot {
        OpType::Read => "read",
        OpType::Write => "write",
        OpType::Edit => "edit",
        OpType::List => "list",
        OpType::Run => "run",
        OpType::Other => "other",
    }
}

fn parse_op_type(s: &str) -> OpType {
    match s {
        "read" => OpType::Read,
        "write" => OpType::Write,
        "edit" => OpType::Edit,
        "list" => OpType::List,
        "run" => OpType::Run,
        _ => OpType::Other,
    }
}

fn parse_goal_category(s: &str) -> GoalCategory {
    match s {
        "read" => GoalCategory::Read,
        "write" => GoalCategory::Write,
        "edit" => GoalCategory::Edit,
        "list" => GoalCategory::List,
        "run" => GoalCategory::Run,
        _ => GoalCategory::Other,
    }
}
