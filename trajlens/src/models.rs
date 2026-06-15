/// Core data models for TrajLens.
///
/// All structs represent the structured trajectory data (steps, items, costs)
/// and the four graph types (Goal Transition Tree, Reasoning Artifact DAG,
/// Activity Graph, Cost Map) with their nodes, edges, and containers.
use std::collections::HashMap;

use chrono::NaiveDateTime;
use serde::{Deserialize, Serialize};

/// Token usage and dollar cost for a single LLM call or aggregated over a range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Cost {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub dollar_cost: f64,
}

impl Default for Cost {
    fn default() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cache_read_tokens: 0,
            cache_write_tokens: 0,
            dollar_cost: 0.0,
        }
    }
}

impl Cost {
    pub fn add(&self, other: &Cost) -> Cost {
        Cost {
            input_tokens: self.input_tokens + other.input_tokens,
            output_tokens: self.output_tokens + other.output_tokens,
            cache_read_tokens: self.cache_read_tokens + other.cache_read_tokens,
            cache_write_tokens: self.cache_write_tokens + other.cache_write_tokens,
            dollar_cost: self.dollar_cost + other.dollar_cost,
        }
    }
}

/// Category of an Item within a Step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemCategory {
    Input,
    Think,
    Action,
    Event,
    Unknown,
}

/// A single atomic element within a Step.
///
/// Each item represents one thing the agent did or received:
/// - Input: user/system message injected into the conversation
/// - Think: LLM reasoning text
/// - Action: a tool call (sub_category holds e.g. "bash_action", "read_action")
/// - Event: a system event (sub_category holds e.g. "context_compact_event")
/// - Unknown: unclassified content awaiting LLM fixer
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Item {
    pub category: ItemCategory,
    pub sub_category: Option<String>,
    pub args: HashMap<String, String>,
    pub content: String,
    pub cost: Cost,
}

/// One iteration of the agent's Read-Eval-Print loop.
///
/// A Step is a single LLM call + tool execution cycle. Multiple steps make up
/// one turn (human interaction boundary). For single-input agents like Claude Code
/// or PoCGen, there is one turn containing many steps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub step_id: usize,
    pub items: Vec<Item>,
    pub timestamp_start: NaiveDateTime,
    pub timestamp_end: NaiveDateTime,
    pub raw_line_range: (usize, usize),
}

/// A complete agent execution session parsed from a single log file.
///
/// Contains one or more steps. For single-input agents (Claude Code, PoCGen),
/// the entire log is one turn with many steps.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trajectory {
    pub label: String,
    pub steps: Vec<Step>,
    pub total_cost: Cost,
    pub outcome: String,
}

/// Operation type for file/command actions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OpType {
    Read,
    Write,
    Edit,
    List,
    Run,
    Other,
}

/// A single file operation within an ActivityNode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operation {
    pub op_type: OpType,
    pub detail: String,
    pub call_index: usize,
}

/// Goal type in Goal Transition Tree — what the agent is DOING at this node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalType {
    /// Gather information: read, search, probe, enumerate, discover
    Explore,
    /// Reason and decide: analyze findings, form hypothesis, choose approach
    Think,
    /// Execute an action: write code, run command, modify state, submit
    Act,
}

/// Status of a goal node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalStatus {
    Done,
    Failed,
    Abandoned,
    /// Partially succeeded: some sub-goals achieved but overall goal not fully met
    Partial,
}

/// A node in the Goal Transition Tree representing a purposeful intent.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalNode {
    pub node_id: String,
    pub label: String,
    pub goal_type: GoalType,
    pub status: GoalStatus,
    /// What was the outcome/result of this goal (shown in node).
    #[serde(default)]
    pub result: String,
    /// Detailed evidence: key commands run, outputs received, error messages.
    /// Not shown in the graph node itself — available in a side panel on click.
    #[serde(default)]
    pub details: String,
    pub level: usize,
    pub step_range: (usize, usize),
    pub cost: Cost,
    pub reasoning_artifacts: Vec<String>,
}

/// Status of a reasoning insight.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InsightStatus {
    Verified,
    #[serde(rename = "self-falsed")]
    SelfFalsed,
    Unverified,
}

/// Type of a reasoning artifact node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningNodeType {
    GroundTruth,
    Insight,
}

/// A node in the Reasoning Artifact DAG representing a belief or fact.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReasoningArtifactNode {
    pub node_id: String,
    pub node_type: ReasoningNodeType,
    pub content: String,
    pub source_step_id: usize,
    pub confidence: f64,
    pub status: Option<InsightStatus>,
    /// Step range [formed_at, abandoned_or_verified_at] for hypothesis nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_range: Option<(usize, usize)>,
}

/// Goal category for activity nodes (what kind of operation).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalCategory {
    Read,
    Write,
    Edit,
    List,
    Run,
    Other,
}

impl GoalCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            GoalCategory::Read => "read",
            GoalCategory::Write => "write",
            GoalCategory::Edit => "edit",
            GoalCategory::List => "list",
            GoalCategory::Run => "run",
            GoalCategory::Other => "other",
        }
    }
}

/// A node in the Activity Graph representing a distinct operation target.
///
/// Identified by (goal_category, primary_object). Multiple raw steps that share
/// this key are merged into one node. parent_id links to containing directory node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityNode {
    pub node_id: String,
    pub label: String,
    pub goal_category: GoalCategory,
    pub primary_object: String,
    pub parent_id: Option<String>,
    pub call_indices: Vec<usize>,
    pub operations: Vec<Operation>,
    pub total_cost: Cost,
}

/// A node in the Cost Map treemap.
///
/// Leaf nodes have a category and no children.
/// Internal nodes have category=None and represent goals containing sub-goals.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostMapNode {
    pub node_id: String,
    pub label: String,
    pub cost: Cost,
    pub children: Vec<CostMapNode>,
    pub category: Option<String>,
    /// Step range this cost node spans.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step_range: Option<(usize, usize)>,
}

/// Edge type in Goal Transition Tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GoalEdgeType {
    Next,
    Backtrack,
    Sub,
}

/// An edge in the Goal Transition Tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalEdge {
    pub edge_type: GoalEdgeType,
    pub source_id: String,
    pub target_id: String,
    pub label: String,
}

/// Edge type in Reasoning Artifact DAG.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEdgeType {
    Infers,
    Contradicts,
    Supersedes,
}

/// An edge in the Reasoning Artifact DAG.
/// source_ids is a list because "infers" may be N-to-1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReasoningEdge {
    pub edge_type: ReasoningEdgeType,
    pub source_ids: Vec<String>,
    pub target_id: String,
}

/// An edge in the Activity Graph linking consecutive operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityEdge {
    pub edge_type: String,
    pub source_id: String,
    pub source_operation_index: usize,
    pub target_id: String,
    pub target_operation_index: usize,
}

/// Container for the Goal Transition Tree graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GoalTransitionTree {
    pub nodes: Vec<GoalNode>,
    pub edges: Vec<GoalEdge>,
    pub root_id: String,
}

/// Container for the Reasoning Artifact DAG.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReasoningArtifactDAG {
    pub nodes: Vec<ReasoningArtifactNode>,
    pub edges: Vec<ReasoningEdge>,
}

/// Container for the Activity Graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActivityGraph {
    pub nodes: Vec<ActivityNode>,
    pub edges: Vec<ActivityEdge>,
}

/// Container for the Cost Map treemap.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostMap {
    pub root: CostMapNode,
}

/// Union of all graph types for deserialization dispatch.
#[derive(Debug, Clone, PartialEq)]
pub enum GraphEnum {
    GoalTree(GoalTransitionTree),
    ReasoningDAG(ReasoningArtifactDAG),
    ActivityGraph(ActivityGraph),
    CostMap(CostMap),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cost_default_is_zero() {
        let c = Cost::default();
        assert_eq!(c.input_tokens, 0);
        assert_eq!(c.dollar_cost, 0.0);
    }

    #[test]
    fn test_cost_add() {
        let a = Cost {
            input_tokens: 10,
            output_tokens: 5,
            cache_read_tokens: 2,
            cache_write_tokens: 1,
            dollar_cost: 0.01,
        };
        let b = Cost {
            input_tokens: 20,
            output_tokens: 10,
            cache_read_tokens: 3,
            cache_write_tokens: 4,
            dollar_cost: 0.02,
        };
        let c = a.add(&b);
        assert_eq!(c.input_tokens, 30);
        assert_eq!(c.output_tokens, 15);
        assert_eq!(c.dollar_cost, 0.03);
    }

    #[test]
    fn test_item_category_serde() {
        let json = serde_json::to_string(&ItemCategory::Action).unwrap();
        assert_eq!(json, "\"action\"");
        let parsed: ItemCategory = serde_json::from_str("\"think\"").unwrap();
        assert_eq!(parsed, ItemCategory::Think);
    }

    #[test]
    fn test_goal_category_as_str() {
        assert_eq!(GoalCategory::Read.as_str(), "read");
        assert_eq!(GoalCategory::Run.as_str(), "run");
    }
}
