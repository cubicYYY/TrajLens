/// Goal Transition Tree (G1) builder - LLM-based goal extraction.
///
/// Analyzes trajectory to extract:
/// - Hierarchical goal structure
/// - Goal transitions (next, backtrack, refinement)
/// - Achievement status
/// - Cost per goal
use crate::models::{GoalEdge, GoalNode, GoalTransitionTree, Trajectory};

#[cfg(feature = "llm")]
use crate::llm::traits::LLMResult;

/// Build a Goal Transition Tree from a trajectory using an LLM.
///
/// # Arguments
/// * `trajectory` - The parsed trajectory
/// * `model` - Model specification in "provider/model-name" format
///   - "anthropic/claude-sonnet-4-6"
///   - "bedrock/us.anthropic.claude-sonnet-4-6"
///
/// # Returns
/// Result containing the GoalTransitionTree or an error message
///
/// # Example
/// ```rust,no_run
/// use trajlens::graphs::goal_tree;
///
/// #[tokio::main]
/// async fn main() {
///     let tree = goal_tree::build_with_llm(&trajectory, "anthropic/claude-sonnet-4-6")
///         .await.unwrap();
/// }
/// ```
/// Stage 1a: Compute consensus boundaries from multiple goal tree runs.
///
/// Takes N goal trees (built from the same trajectory) and finds step indices
/// where at least `min_agreement` runs placed a node boundary. Merges boundaries
/// within `tolerance` steps of each other.
///
/// Returns sorted, deduplicated boundary points suitable for `build_with_boundaries`.
pub fn compute_boundaries_from_runs(
    trees: &[&GoalTransitionTree],
    min_agreement: usize,
    tolerance: usize,
) -> Vec<usize> {
    use std::collections::HashMap;
    let mut boundary_counts: HashMap<usize, usize> = HashMap::new();

    for tree in trees {
        for node in &tree.nodes {
            if node.node_id == tree.root_id {
                continue;
            }
            *boundary_counts.entry(node.step_range.0).or_insert(0) += 1;
            *boundary_counts.entry(node.step_range.1).or_insert(0) += 1;
        }
    }

    let mut valid: Vec<usize> = boundary_counts
        .into_iter()
        .filter(|(_, count)| *count >= min_agreement)
        .map(|(step, _)| step)
        .collect();
    valid.sort_unstable();

    let mut merged = Vec::new();
    for b in valid {
        if merged.is_empty() || b - *merged.last().unwrap() > tolerance {
            merged.push(b);
        }
    }

    merged
}

/// Stage 1b: LLM-based segmentation — semantically divide the trajectory into
/// goal-aligned chunks.
///
/// Use this when no user-provided boundaries or consensus runs are available.
/// The LLM reads the trajectory and identifies where the agent's current goal
/// changes (pivot points, phase transitions, new attempts).
///
/// Returns sorted boundary step indices.
#[cfg(feature = "llm")]
pub async fn compute_boundaries_with_llm(
    trajectory: &Trajectory,
    model: &str,
) -> LLMResult<Vec<usize>> {
    use crate::llm::model_registry;
    let llm_client = model_registry::create_client(model).await?;

    let step_count = trajectory.steps.len();
    let sample = sample_trajectory_steps(trajectory, 30);

    let system_prompt = r#"You are analyzing an AI agent's execution trajectory to identify GOAL BOUNDARIES — the step numbers where the agent's current objective changes.

A boundary occurs when:
- The agent finishes one task and starts a different one
- The agent abandons an approach and pivots to something new
- The agent transitions from exploring to acting (or vice versa)
- A sub-task completes and control returns to the parent goal

Do NOT place boundaries between minor variations of the same approach.
DO place boundaries where a human would say "now the agent is doing something DIFFERENT."

Return ONLY a JSON array of step numbers (integers) where boundaries occur.
Include the first step and the last step as boundaries.
Aim for 8-20 boundaries for a 50-150 step trajectory."#;

    let user_message = format!(
        "Trajectory: {} steps, outcome: {}\n\nSteps:\n{}\n\n\
         Identify the goal-change boundaries. Return ONLY a JSON array of integers.",
        step_count, trajectory.outcome, sample
    );

    let response = llm_client
        .as_ref()
        .complete(system_prompt, &user_message)
        .await?;

    // Parse the response as a JSON array of integers
    let trimmed = response.trim();
    let json_str = if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            &trimmed[start..=end]
        } else {
            trimmed
        }
    } else {
        trimmed
    };

    let mut boundaries: Vec<usize> = serde_json::from_str(json_str).unwrap_or_else(|_| {
        // Fallback: split evenly into ~10 segments
        (0..=step_count).step_by(step_count / 10).collect()
    });

    // Ensure first and last are included
    if boundaries.first() != Some(&0) && boundaries.first() != Some(&1) {
        boundaries.insert(0, 0);
    }
    if boundaries.last() != Some(&step_count) {
        boundaries.push(step_count);
    }

    boundaries.sort_unstable();
    boundaries.dedup();

    eprintln!(
        "[GoalTree] Stage 1 (LLM segmentation): {} boundaries → {} segments",
        boundaries.len(),
        boundaries.len() - 1
    );

    Ok(boundaries)
}

/// Full two-stage pipeline: LLM segments → LLM annotates.
/// Use when no boundaries are provided.
#[cfg(feature = "llm")]
pub async fn build_two_stage(
    trajectory: &Trajectory,
    model: &str,
) -> LLMResult<GoalTransitionTree> {
    let boundaries = compute_boundaries_with_llm(trajectory, model).await?;
    build_with_boundaries(trajectory, &boundaries, model).await
}

#[cfg(feature = "llm")]
pub async fn build_with_llm(trajectory: &Trajectory, model: &str) -> LLMResult<GoalTransitionTree> {
    build_with_llm_retries(trajectory, model, 3).await
}

/// Two-stage goal tree builder (decoupled segmentation from annotation).
///
/// Stage 1 (SEGMENT): already done externally — caller provides boundaries.
///   Sources: consensus from N runs, user-provided, heuristic splitter, etc.
///
/// Stage 2 (ANNOTATE): this function. LLM labels each segment AND decides
///   the hierarchical relationships (which segments group into phases,
///   which are retries of the same goal, which branch).
///
/// The decoupling means:
/// - Boundaries are stable (computed once, reused across analyses)
/// - Annotations can vary (different LLMs, different prompts, different focus)
/// - Users can skip stage 1 entirely by providing their own boundaries
///
/// # Arguments
/// * `trajectory` - The parsed trajectory
/// * `boundaries` - Sorted list of step indices where segments start/end
/// * `model` - LLM model spec
#[cfg(feature = "llm")]
pub async fn build_with_boundaries(
    trajectory: &Trajectory,
    boundaries: &[usize],
    model: &str,
) -> LLMResult<GoalTransitionTree> {
    use crate::llm::model_registry;
    let llm_client = model_registry::create_client(model).await?;

    // Build segment descriptions from the trajectory
    let segments: Vec<String> = boundaries
        .windows(2)
        .enumerate()
        .map(|(i, w)| {
            let start = w[0];
            let end = w[1];
            let step_count = end - start;

            // Sample content from this segment
            let mut content_sample = String::new();
            for step in trajectory.steps.iter().skip(start).take(step_count.min(5)) {
                for item in &step.items {
                    let snippet = &item.content[..item.content.len().min(100)];
                    content_sample.push_str(snippet);
                    content_sample.push('\n');
                }
            }

            format!(
                "Segment {} (steps {}-{}): {}",
                i + 1,
                start,
                end,
                content_sample.trim()
            )
        })
        .collect();

    let segments_text = segments.join("\n\n");

    // Step 1: Ask LLM to label each segment and assign phase grouping.
    // Simple array response — reliable and fast.
    let system_prompt = r#"You are given pre-defined trajectory segments.
For each segment, provide:
- "label": what the agent was trying to do (max 15 words, specific)
- "goal_type": "explore" or "think" or "act"
- "status": "done" or "failed" or "partial"
- "result": brief outcome (max 20 words)
- "phase": integer 1-4, grouping related segments into major phases

Return ONLY a JSON array of objects, one per segment, in order."#;

    let user_message = format!(
        "Trajectory: {} steps, outcome: {}\n\nSegments:\n{}\n\n\
         Return a JSON array of {} objects. Group into 2-4 phases via \"phase\" field.",
        trajectory.steps.len(),
        trajectory.outcome,
        segments_text,
        segments.len()
    );

    let response = llm_client
        .as_ref()
        .complete(system_prompt, &user_message)
        .await?;

    // Parse segment labels
    let seg_labels: Vec<serde_json::Value> = {
        let trimmed = response.trim();
        let json_str = if let Some(start) = trimmed.find('[') {
            if let Some(end) = trimmed.rfind(']') {
                &trimmed[start..=end]
            } else {
                trimmed
            }
        } else {
            trimmed
        };
        serde_json::from_str(json_str).unwrap_or_else(|_| Vec::new())
    };

    // Step 2: Build tree structure deterministically with proper IDs and loop invariant.
    use crate::models::{GoalEdge, GoalEdgeType, GoalNode, GoalStatus, GoalType};

    let num_segments = boundaries.len() - 1;
    let max_fanout = 4;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    // Group segments by phase
    let mut phase_map: std::collections::BTreeMap<u64, Vec<usize>> =
        std::collections::BTreeMap::new();
    for (i, label) in seg_labels.iter().enumerate().take(num_segments) {
        let phase = label.get("phase").and_then(|v| v.as_u64()).unwrap_or(1);
        phase_map.entry(phase).or_default().push(i);
    }
    if phase_map.is_empty() || phase_map.len() == 1 {
        phase_map.clear();
        let chunk = num_segments / 3;
        for i in 0..num_segments {
            phase_map
                .entry((i / chunk.max(1)).min(2) as u64 + 1)
                .or_default()
                .push(i);
        }
    }

    let root_status = if trajectory.outcome == "SOLVED" {
        GoalStatus::Done
    } else {
        GoalStatus::Failed
    };

    // ROOT
    nodes.push(GoalNode {
        node_id: "ROOT".into(),
        label: "Execute task".into(),
        goal_type: GoalType::Act,
        status: root_status.clone(),
        result: trajectory.outcome.clone(),
        details: String::new(),
        level: 0,
        step_range: (
            *boundaries.first().unwrap_or(&0),
            *boundaries.last().unwrap_or(&0),
        ),
        cost: trajectory.total_cost.clone(),
        reasoning_artifacts: vec![],
    });

    // Phase nodes (level 1): IDs "1", "2", "3", ...
    let phase_count = phase_map.len();
    let phase_ids: Vec<String> = (1..=phase_count).map(|i| format!("{}", i)).collect();

    // ROOT loop: ROOT→sub→1→next→2→next→3→backtrack→ROOT
    edges.push(GoalEdge {
        edge_type: GoalEdgeType::Sub,
        source_id: "ROOT".into(),
        target_id: phase_ids[0].clone(),
        label: String::new(),
    });
    for w in phase_ids.windows(2) {
        edges.push(GoalEdge {
            edge_type: GoalEdgeType::Next,
            source_id: w[0].clone(),
            target_id: w[1].clone(),
            label: String::new(),
        });
    }
    edges.push(GoalEdge {
        edge_type: GoalEdgeType::Backtrack,
        source_id: phase_ids.last().unwrap().clone(),
        target_id: "ROOT".into(),
        label: String::new(),
    });

    for (phase_idx, (_, seg_indices)) in phase_map.iter().enumerate() {
        let phase_id = &phase_ids[phase_idx];
        let p_start = boundaries[*seg_indices.first().unwrap_or(&0)];
        let p_end = boundaries[seg_indices.last().unwrap_or(&0) + 1];

        // Phase label from first segment's label
        let phase_label = seg_labels
            .get(*seg_indices.first().unwrap_or(&0))
            .and_then(|v| v.get("label"))
            .and_then(|v| v.as_str())
            .unwrap_or("Phase")
            .to_string();

        nodes.push(GoalNode {
            node_id: phase_id.clone(),
            label: phase_label,
            goal_type: GoalType::Explore,
            status: root_status.clone(),
            result: String::new(),
            details: String::new(),
            level: 1,
            step_range: (p_start, p_end),
            cost: crate::models::Cost::default(),
            reasoning_artifacts: vec![],
        });

        // Leaf nodes under this phase
        // If >max_fanout: create sub-groups. Otherwise: direct children.
        if seg_indices.len() <= max_fanout {
            // Direct: phase → leaves as "X.1", "X.2", ...
            let leaf_ids: Vec<String> = (1..=seg_indices.len())
                .map(|i| format!("{}.{}", phase_id, i))
                .collect();
            // Loop: phase→sub→first→next→...→last→backtrack→phase
            edges.push(GoalEdge {
                edge_type: GoalEdgeType::Sub,
                source_id: phase_id.clone(),
                target_id: leaf_ids[0].clone(),
                label: String::new(),
            });
            for w in leaf_ids.windows(2) {
                edges.push(GoalEdge {
                    edge_type: GoalEdgeType::Next,
                    source_id: w[0].clone(),
                    target_id: w[1].clone(),
                    label: String::new(),
                });
            }
            edges.push(GoalEdge {
                edge_type: GoalEdgeType::Backtrack,
                source_id: leaf_ids.last().unwrap().clone(),
                target_id: phase_id.clone(),
                label: String::new(),
            });

            for (li, &seg_i) in seg_indices.iter().enumerate() {
                let leaf_id = &leaf_ids[li];
                let lbl = seg_labels.get(seg_i);
                nodes.push(make_leaf_node(
                    leaf_id,
                    seg_i,
                    boundaries,
                    lbl,
                    &root_status,
                ));
            }
        } else {
            // Sub-groups: phase → "X.1", "X.2" (groups), each group → "X.1.1", "X.1.2" (leaves)
            let chunks: Vec<&[usize]> = seg_indices.chunks(max_fanout).collect();
            let group_ids: Vec<String> = (1..=chunks.len())
                .map(|i| format!("{}.{}", phase_id, i))
                .collect();

            // Phase loop
            edges.push(GoalEdge {
                edge_type: GoalEdgeType::Sub,
                source_id: phase_id.clone(),
                target_id: group_ids[0].clone(),
                label: String::new(),
            });
            for w in group_ids.windows(2) {
                edges.push(GoalEdge {
                    edge_type: GoalEdgeType::Next,
                    source_id: w[0].clone(),
                    target_id: w[1].clone(),
                    label: String::new(),
                });
            }
            edges.push(GoalEdge {
                edge_type: GoalEdgeType::Backtrack,
                source_id: group_ids.last().unwrap().clone(),
                target_id: phase_id.clone(),
                label: String::new(),
            });

            for (gi, chunk) in chunks.iter().enumerate() {
                let gid = &group_ids[gi];
                let g_start = boundaries[*chunk.first().unwrap_or(&0)];
                let g_end = boundaries[*chunk.last().unwrap_or(&0) + 1];
                let g_label = seg_labels
                    .get(*chunk.first().unwrap_or(&0))
                    .and_then(|v| v.get("label"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Sub-phase")
                    .to_string();
                nodes.push(GoalNode {
                    node_id: gid.clone(),
                    label: g_label,
                    goal_type: GoalType::Act,
                    status: root_status.clone(),
                    result: String::new(),
                    details: String::new(),
                    level: 2,
                    step_range: (g_start, g_end),
                    cost: crate::models::Cost::default(),
                    reasoning_artifacts: vec![],
                });

                // Group loop
                let leaf_ids: Vec<String> = (1..=chunk.len())
                    .map(|i| format!("{}.{}", gid, i))
                    .collect();
                edges.push(GoalEdge {
                    edge_type: GoalEdgeType::Sub,
                    source_id: gid.clone(),
                    target_id: leaf_ids[0].clone(),
                    label: String::new(),
                });
                for w in leaf_ids.windows(2) {
                    edges.push(GoalEdge {
                        edge_type: GoalEdgeType::Next,
                        source_id: w[0].clone(),
                        target_id: w[1].clone(),
                        label: String::new(),
                    });
                }
                edges.push(GoalEdge {
                    edge_type: GoalEdgeType::Backtrack,
                    source_id: leaf_ids.last().unwrap().clone(),
                    target_id: gid.clone(),
                    label: String::new(),
                });

                for (li, &seg_i) in chunk.iter().enumerate() {
                    let leaf_id = &leaf_ids[li];
                    let lbl = seg_labels.get(seg_i);
                    let mut node = make_leaf_node(leaf_id, seg_i, boundaries, lbl, &root_status);
                    node.level = 3;
                    nodes.push(node);
                }
            }
        }
    }

    let mut tree = GoalTransitionTree {
        nodes,
        edges,
        root_id: "ROOT".into(),
    };

    add_missing_backtrack_edges(&mut tree);
    collapse_single_child_nodes(&mut tree);
    propagate_attributes_bottom_up(&mut tree);
    fill_missing_backtrack_labels(&mut tree);

    eprintln!(
        "Goal tree built from {} boundaries ({} segments)",
        boundaries.len(),
        num_segments
    );
    Ok(tree)
}

/// Collapse single-child intermediate nodes.
/// If a parent has exactly one child, merge the child into the parent:
/// - Parent takes the child's label, result, details, goal_type, status
/// - Child is removed
/// - Any grandchildren become direct children of the parent
fn collapse_single_child_nodes(tree: &mut GoalTransitionTree) {
    use crate::models::GoalEdgeType;

    loop {
        // Find a non-ROOT parent with exactly one child
        let mut to_collapse: Option<(String, String)> = None; // (parent_id, child_id)

        // Build children map
        let mut children_of: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        for edge in &tree.edges {
            if edge.edge_type == GoalEdgeType::Sub {
                let mut chain = vec![edge.target_id.clone()];
                let mut current = edge.target_id.clone();
                loop {
                    let next = tree
                        .edges
                        .iter()
                        .find(|e| e.source_id == current && e.edge_type == GoalEdgeType::Next);
                    match next {
                        Some(e) => {
                            chain.push(e.target_id.clone());
                            current = e.target_id.clone();
                        }
                        None => break,
                    }
                }
                children_of.insert(edge.source_id.clone(), chain);
            }
        }

        for (parent_id, children) in &children_of {
            if parent_id == "ROOT" {
                continue;
            }
            if children.len() == 1 {
                to_collapse = Some((parent_id.clone(), children[0].clone()));
                break;
            }
        }

        let (parent_id, child_id) = match to_collapse {
            Some(pair) => pair,
            None => break, // No more single-child nodes
        };

        // Merge child into parent:
        // 1. Copy child's semantic fields to parent
        if let Some(child_node) = tree.nodes.iter().find(|n| n.node_id == child_id).cloned() {
            if let Some(parent_node) = tree.nodes.iter_mut().find(|n| n.node_id == parent_id) {
                // Keep parent's node_id but take child's content
                if parent_node.label == parent_node.result || parent_node.result.is_empty() {
                    parent_node.label = child_node.label;
                }
                if parent_node.result.is_empty() {
                    parent_node.result = child_node.result;
                }
                if parent_node.details.is_empty() {
                    parent_node.details = child_node.details;
                }
                parent_node.goal_type = child_node.goal_type;
                parent_node.status = child_node.status;
                parent_node.step_range = child_node.step_range;
            }
        }

        // 2. Re-point grandchildren edges: child's sub→grandchild becomes parent's sub→grandchild
        let child_id_clone = child_id.clone();
        for edge in &mut tree.edges {
            if edge.source_id == child_id_clone {
                edge.source_id = parent_id.clone();
            }
            if edge.target_id == child_id_clone {
                edge.target_id = parent_id.clone();
            }
        }

        // 3. Remove the child node
        tree.nodes.retain(|n| n.node_id != child_id);

        // 4. Remove self-referencing edges (parent→parent)
        tree.edges.retain(|e| e.source_id != e.target_id);

        // 5. Remove duplicate edges
        let mut seen = std::collections::HashSet::new();
        tree.edges.retain(|e| {
            let key = format!("{}:{}:{:?}", e.source_id, e.target_id, e.edge_type);
            seen.insert(key)
        });
    }
}

/// Propagate status and goal_type from children to parents (bottom-up).
/// - Parent status = worst child status (failed > partial > done)
/// - Parent goal_type = dominant child type (majority vote)
fn propagate_attributes_bottom_up(tree: &mut GoalTransitionTree) {
    use crate::models::{GoalEdgeType, GoalStatus, GoalType};
    use std::collections::HashMap;

    // Build parent→children map
    let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
    for edge in &tree.edges {
        if edge.edge_type == GoalEdgeType::Sub {
            // Follow next chain from first child
            let mut chain = vec![edge.target_id.clone()];
            let mut current = edge.target_id.clone();
            loop {
                let next = tree
                    .edges
                    .iter()
                    .find(|e| e.source_id == current && e.edge_type == GoalEdgeType::Next);
                match next {
                    Some(e) => {
                        chain.push(e.target_id.clone());
                        current = e.target_id.clone();
                    }
                    None => break,
                }
            }
            children_of.insert(edge.source_id.clone(), chain);
        }
    }

    // Topological order: process leaves first, then parents
    // Repeated passes until stable (simple for small trees)
    let node_map: HashMap<String, usize> = tree
        .nodes
        .iter()
        .enumerate()
        .map(|(i, n)| (n.node_id.clone(), i))
        .collect();

    for _pass in 0..5 {
        for (parent_id, child_ids) in &children_of {
            if parent_id == "ROOT" {
                continue;
            } // ROOT keeps its own status

            let child_statuses: Vec<&GoalStatus> = child_ids
                .iter()
                .filter_map(|id| node_map.get(id))
                .map(|&i| &tree.nodes[i].status)
                .collect();

            let child_types: Vec<&GoalType> = child_ids
                .iter()
                .filter_map(|id| node_map.get(id))
                .map(|&i| &tree.nodes[i].goal_type)
                .collect();

            if child_statuses.is_empty() {
                continue;
            }

            // Worst status propagates up
            let new_status = if child_statuses.iter().any(|s| **s == GoalStatus::Failed) {
                GoalStatus::Failed
            } else if child_statuses.iter().any(|s| **s == GoalStatus::Partial) {
                GoalStatus::Partial
            } else {
                GoalStatus::Done
            };

            // Dominant goal_type (majority via counting)
            let explore_count = child_types
                .iter()
                .filter(|t| ***t == GoalType::Explore)
                .count();
            let think_count = child_types
                .iter()
                .filter(|t| ***t == GoalType::Think)
                .count();
            let act_count = child_types.iter().filter(|t| ***t == GoalType::Act).count();
            let new_type = if explore_count >= think_count && explore_count >= act_count {
                GoalType::Explore
            } else if think_count >= act_count {
                GoalType::Think
            } else {
                GoalType::Act
            };

            if let Some(&idx) = node_map.get(parent_id) {
                tree.nodes[idx].status = new_status;
                tree.nodes[idx].goal_type = new_type;
            }
        }
    }
}

/// Create a leaf node from segment data and LLM labels.
fn make_leaf_node(
    node_id: &str,
    seg_i: usize,
    boundaries: &[usize],
    lbl: Option<&serde_json::Value>,
    default_status: &crate::models::GoalStatus,
) -> GoalNode {
    use crate::models::{GoalStatus, GoalType};
    let seg_start = boundaries[seg_i];
    let seg_end = boundaries[seg_i + 1];
    let label = lbl
        .and_then(|v| v.get("label"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown")
        .to_string();
    let goal_type = match lbl
        .and_then(|v| v.get("goal_type"))
        .and_then(|v| v.as_str())
        .unwrap_or("act")
    {
        "explore" => GoalType::Explore,
        "think" => GoalType::Think,
        _ => GoalType::Act,
    };
    let status = match lbl
        .and_then(|v| v.get("status"))
        .and_then(|v| v.as_str())
        .unwrap_or("failed")
    {
        "done" => GoalStatus::Done,
        "partial" => GoalStatus::Partial,
        "abandoned" => GoalStatus::Abandoned,
        _ => default_status.clone(),
    };
    let result = lbl
        .and_then(|v| v.get("result"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    GoalNode {
        node_id: node_id.to_string(),
        label,
        goal_type,
        status,
        result,
        details: String::new(),
        level: 2,
        step_range: (seg_start, seg_end),
        cost: crate::models::Cost::default(),
        reasoning_artifacts: vec![],
    }
}

/// Fallback: simple 2-level tree (ROOT → leaves) when LLM fails.
fn build_flat_fallback(trajectory: &Trajectory, boundaries: &[usize]) -> GoalTransitionTree {
    use crate::models::{GoalEdge, GoalEdgeType, GoalNode, GoalStatus, GoalType};

    let num_segments = boundaries.len() - 1;
    let mut nodes = Vec::new();
    let mut edges = Vec::new();

    let root_status = if trajectory.outcome == "SOLVED" {
        GoalStatus::Done
    } else {
        GoalStatus::Failed
    };

    nodes.push(GoalNode {
        node_id: "ROOT".to_string(),
        label: "Execute task".to_string(),
        goal_type: GoalType::Act,
        status: root_status.clone(),
        result: trajectory.outcome.clone(),
        details: String::new(),
        level: 0,
        step_range: (
            *boundaries.first().unwrap_or(&0),
            *boundaries.last().unwrap_or(&0),
        ),
        cost: trajectory.total_cost.clone(),
        reasoning_artifacts: vec![],
    });

    // Split into groups of 4 under ROOT
    let max_fanout = 4;
    let chunks: Vec<Vec<usize>> = (0..num_segments)
        .collect::<Vec<_>>()
        .chunks(max_fanout)
        .map(|c| c.to_vec())
        .collect();

    let group_ids: Vec<String> = (0..chunks.len()).map(|i| format!("G{}", i + 1)).collect();

    if let Some(first) = group_ids.first() {
        edges.push(GoalEdge {
            edge_type: GoalEdgeType::Sub,
            source_id: "ROOT".into(),
            target_id: first.clone(),
            label: String::new(),
        });
    }
    for w in group_ids.windows(2) {
        edges.push(GoalEdge {
            edge_type: GoalEdgeType::Next,
            source_id: w[0].clone(),
            target_id: w[1].clone(),
            label: String::new(),
        });
    }
    if let Some(last) = group_ids.last() {
        edges.push(GoalEdge {
            edge_type: GoalEdgeType::Backtrack,
            source_id: last.clone(),
            target_id: "ROOT".into(),
            label: String::new(),
        });
    }

    for (gi, chunk) in chunks.iter().enumerate() {
        let gid = &group_ids[gi];
        let g_start = boundaries[*chunk.first().unwrap()];
        let g_end = boundaries[*chunk.last().unwrap() + 1];
        nodes.push(GoalNode {
            node_id: gid.clone(),
            label: format!("Steps {}-{}", g_start, g_end),
            goal_type: GoalType::Act,
            status: root_status.clone(),
            result: String::new(),
            details: String::new(),
            level: 1,
            step_range: (g_start, g_end),
            cost: crate::models::Cost::default(),
            reasoning_artifacts: vec![],
        });

        let leaf_ids: Vec<String> = chunk.iter().map(|&i| format!("S{}", i + 1)).collect();
        if let Some(first) = leaf_ids.first() {
            edges.push(GoalEdge {
                edge_type: GoalEdgeType::Sub,
                source_id: gid.clone(),
                target_id: first.clone(),
                label: String::new(),
            });
        }
        for w in leaf_ids.windows(2) {
            edges.push(GoalEdge {
                edge_type: GoalEdgeType::Next,
                source_id: w[0].clone(),
                target_id: w[1].clone(),
                label: String::new(),
            });
        }
        if let Some(last) = leaf_ids.last() {
            edges.push(GoalEdge {
                edge_type: GoalEdgeType::Backtrack,
                source_id: last.clone(),
                target_id: gid.clone(),
                label: String::new(),
            });
        }

        for &seg_i in chunk {
            let seg_start = boundaries[seg_i];
            let seg_end = boundaries[seg_i + 1];
            nodes.push(GoalNode {
                node_id: format!("S{}", seg_i + 1),
                label: format!("Segment {} (steps {}-{})", seg_i + 1, seg_start, seg_end),
                goal_type: GoalType::Act,
                status: root_status.clone(),
                result: String::new(),
                details: String::new(),
                level: 2,
                step_range: (seg_start, seg_end),
                cost: crate::models::Cost::default(),
                reasoning_artifacts: vec![],
            });
        }
    }

    GoalTransitionTree {
        nodes,
        edges,
        root_id: "ROOT".to_string(),
    }
}

/// Build goal tree with multi-turn LLM correction.
/// If the LLM produces structural anomalies, they are reported back and
/// the LLM gets a chance to fix them. Retries up to `max_retries` times.
#[cfg(feature = "llm")]
pub async fn build_with_llm_retries(
    trajectory: &Trajectory,
    model: &str,
    max_retries: usize,
) -> LLMResult<GoalTransitionTree> {
    use crate::llm::model_registry;
    let llm_client = model_registry::create_client(model).await?;

    let step_count = trajectory.steps.len();
    let total_cost = trajectory.total_cost.dollar_cost;
    let outcome = &trajectory.outcome;
    let system_prompt = r#"You are an expert at analyzing AI agent execution trajectories and extracting goal hierarchies.
Agent's "current goal" is frequently changing or backtracking; you need to find these goals and reveal how the agent's goal transits.
This can help people better understand why some steps failed or why the agent pivot to another way after something failed.
The key is to find the critical result or decision in each "current goal", which decides the agent's next step.

Your task: build a Goal Transition Tree showing what goals the agent pursued and how they relate.

# CRITICAL: This must be a TREE with depth, NOT a flat chain.

The tree MUST have at least 2 levels of hierarchy. A flat chain (ROOT → 1 → 2 → 3 → ... all via "next" edges) is WRONG.

Correct structure: ROOT has 2-4 children (phases). Each phase has 2-4 sub-children (actions).

# LOOP INVARIANT (most important rule)

Every parent node forms a CLOSED LOOP with its children:
  parent --sub--> first_child --next--> ... --next--> last_child --backtrack--> parent

This means:
- Parent has one "sub" edge per plan it launches (to the first child of that plan)
- Children within a plan are chained by "next" edges (first→second→...→last)
- The LAST child of each chain has a "backtrack" edge returning to the parent
- A node CANNOT have both "next" AND "backtrack" — it's either mid-chain or end-of-chain

Single plan (most common):
  3 --sub--> 3.1 --next--> 3.2 --next--> 3.3 --backtrack--> 3

Multiple plans (parent retries after first plan fails):
  3 --sub--> 3.1 --next--> 3.2 --backtrack--> 3   (first plan failed)
  3 --sub--> 3.3 --next--> 3.4 --backtrack--> 3   (second plan, new approach)

WRONG (node has both next AND backtrack — REJECTED):
  3.2 --next--> 3.3       ← means 3.2 is mid-chain
  3.2 --backtrack--> 3    ← means 3.2 is end-of-chain
  CONTRADICTION: a node is either middle or end, never both.

If 3.2 failed and the parent pivots, 3.2 should be the END of its chain (backtrack),
and the parent launches a NEW sub edge to 3.3 (start of next plan).

Example with full edge set:
  ROOT --sub--> 1
  1 --next--> 2
  2 --next--> 3
  3 --backtrack--> ROOT        ← closes the ROOT loop

  1 --sub--> 1.1
  1.1 --next--> 1.2
  1.2 --backtrack--> 1         ← closes node 1's loop

  3 --sub--> 3.1
  3.1 --next--> 3.2
  3.2 --backtrack--> 3         ← closes node 3's loop

Tree shape:
  ROOT
  ├── 1 (Reconnaissance)
  │   ├── 1.1 (read files)
  │   └── 1.2 (probe endpoints)
  ├── 2 (Analysis)
  └── 3 (Exploitation)
      ├── 3.1 (write exploit)
      └── 3.2 (execute & verify)

WRONG (flat chain — REJECTED):
  ROOT → 1 → 2 → 3 → 4 → 5 (all "next", no sub edges, no loops)

WRONG (missing backtrack — REJECTED):
  ROOT --sub--> 1, 1 --next--> 2, 2 --next--> 3  (no backtrack to ROOT)

# Fanout Constraint

Each node can have AT MOST 4 children. If you need more, add an intermediate grouping layer.

WRONG (5+ children — too flat):
  3 → 3.1, 3.2, 3.3, 3.4, 3.5, 3.6  (6 children — REJECTED)

CORRECT (group into sub-phases):
  3 → 3.1 (initial attempts), 3.2 (refined attempts)
  3.1 → 3.1.1, 3.1.2, 3.1.3
  3.2 → 3.2.1, 3.2.2, 3.2.3

This keeps the tree readable and forces meaningful grouping.

# Node ID Convention

- Root: "ROOT" (level 0 — the only node at this level)
- Children of ROOT: "1", "2", "3" (level 1 — major phases, 2-4 nodes)
- Children of "2": "2.1", "2.2" (level 2 — actions within phase, max 4)
- Children of "2.1": "2.1.1", "2.1.2" (level 3 — if needed for grouping)

# Edge Types

- "sub": parent → first child (exactly ONE per parent that has children)
- "next": sibling → next sibling (chains children left-to-right)
- "backtrack": last child → parent (closes the loop; every parent gets exactly one)

The validator checks:
- Every node with children has exactly 1 outgoing "sub" edge
- Every parent receives exactly 1 incoming "backtrack" edge from its last child
- ROOT's loop is closed (last phase backtracks to ROOT)

# Sibling Differentiation

When the agent retries or refines an approach, each sibling node MUST state what CHANGED
from the previous attempt. Do NOT use vague labels like "Refine exploit" or "Try again".

BAD siblings (indistinguishable):
  3.1: "Write exploit script"
  3.2: "Refine exploit script"
  3.3: "Try exploit again"

GOOD siblings (each states the concrete difference):
  3.1: "Exploit via SQL injection on /search endpoint"
  3.2: "Exploit via file upload with path traversal"
  3.3: "Exploit via SSRF targeting internal PostgreSQL"

BAD (vague refinement):
  3.1: "Attempt authentication bypass"
  3.2: "Retry with different approach"

GOOD (states what changed):
  3.1: "Bypass auth via case-insensitive email registration"
  3.2: "Bypass auth via JWT secret brute-force (wordlist)"

The label must answer: "what is DIFFERENT about THIS attempt vs the previous one?"

# Output Format (strict JSON)

{
  "nodes": [
    {
      "node_id": "ROOT",
      "label": "Short goal statement, max 15 words",
      "goal_type": "explore|think|act",
      "status": "done|failed|abandoned|partial",
      "result": "Brief outcome (max 20 words)",
      "details": "Key evidence from the trajectory: exact commands, error messages, server responses. 2-4 sentences with specifics.",
      "step_range": [start_step, end_step]
    }
  ],
  "edges": [
    {
      "source_id": "ROOT",
      "target_id": "1",
      "edge_type": "sub",
      "label": ""
    },
    {
      "source_id": "3",
      "target_id": "2",
      "edge_type": "backtrack",
      "label": "SQL injection blocked by ORM parameterization; no raw queries in codebase"
    }
  ]
}

NOTE on edge labels:
- "sub" and "next" edges: label = "" (empty)
- "backtrack" from a FAILED node: label MUST be a non-empty sentence explaining the failure.
  The LLM validator REJECTS empty labels on failed backtrack edges. You MUST fill them.

# Node Labels & Examples

Each node: label (max 15 words, the GOAL) + goal_type + result (what happened).

## EXPLORE examples (gathering information):

  { "node_id": "1", "label": "Map target application structure and endpoints",
    "goal_type": "explore", "status": "done",
    "result": "Flask app with 6 REST endpoints, nginx proxy, PostgreSQL DB" }

  { "node_id": "2.1", "label": "Read authentication source code",
    "goal_type": "explore", "status": "done",
    "result": "JWT-based auth using PyJWT; tokens expire after 1h; secret from env var" }

  { "node_id": "3.1", "label": "Probe API endpoints for access control gaps",
    "goal_type": "explore", "status": "done",
    "result": "GET /api/users returns 200 without auth; PUT requires admin JWT" }

## THINK examples (reasoning and deciding):

  { "node_id": "2", "label": "Identify most promising attack vector",
    "goal_type": "think", "status": "done",
    "result": "Best vector: unauthenticated GET leaks user IDs; chain with IDOR on PUT" }

  { "node_id": "3", "label": "Decide between SQL injection vs API abuse",
    "goal_type": "think", "status": "done",
    "result": "SQLAlchemy ORM blocks injection; pivot to mass-assignment via REST" }

  { "node_id": "4", "label": "Plan next approach after first exploit failed",
    "goal_type": "think", "status": "done",
    "result": "JWT forgery seems infeasible; will try session fixation via cookie handling" }

## ACT examples (executing actions):

  { "node_id": "5", "label": "Craft SQL injection payload for search endpoint",
    "goal_type": "act", "status": "failed",
    "result": "Server returned 500; PostgreSQL rejects is_template=0 (needs FALSE)",
    "details": "Ran: curl -X GET '.../surveys?search=x' UNION SELECT...' Response: HTTP 500. Source at routes.py:45 shows raw SQL with f-string interpolation but PostgreSQL requires boolean literals (TRUE/FALSE not 0/1). All UNION variants produce same 500." }

  { "node_id": "4.2", "label": "Write and run authentication bypass script",
    "goal_type": "act", "status": "done",
    "result": "Got valid admin token via case-insensitive email duplicate registration",
    "details": "Discovered /api/auth/me leaks hashed_password field. Registered ADMIN@target.com (uppercase) which created new user but get_or_create matched existing admin. Login as new user returned admin's JWT token." }

  { "node_id": "6", "label": "Submit proof of exploitation to validation service",
    "goal_type": "act", "status": "failed",
    "result": "Validator rejected: extracted value doesn't match expected format",
    "details": "Ran: python3 submit_proof.py data_exfiltration '{\"data\": \"...\"}'. Validator response: 'Expected format: raw string value, not JSON object'. The extracted DB value contains quotes that break the submission format." }

# Details Field

The "details" field captures the EVIDENCE that supports the result. It's not shown in the
graph node (too long) but available in a click-to-expand panel. It should contain:

For FAILED nodes (most important — explains WHY something failed):
  - The exact command/request that was tried
  - The exact error message or unexpected response
  - What specific mechanism blocked the approach (code line, config, server behavior)

For DONE nodes:
  - The key command that worked
  - The specific output/response that confirmed success

CRITICAL: Do not write vague details. Every detail MUST reference something SPECIFIC
from the trajectory: a URL, a function name, an error code, a file path, a response body.

BAD details: "Tried multiple approaches but none worked"
GOOD details: "Ran exploit.py targeting /api/upload with Content-Type: multipart/form-data. Server returned 403 'Forbidden: upload requires admin role'. Checked middleware at auth.py:23 — @require_role('admin') decorator blocks all non-admin uploads."

# Edge Labels

- "sub" and "next" edges: label = "" (empty)
- "backtrack" from DONE nodes: label = "" (empty)
- "backtrack" from FAILED nodes: label MUST state the CONCRETE ROOT CAUSE.
  Not "it failed" or "approach didn't work" — the SPECIFIC technical blocker.

  BAD:  "Exploit attempt failed"
  BAD:  "Could not bypass authentication"
  BAD:  "Approach unsuccessful"
  GOOD: "ORM parameterizes all queries; no raw SQL paths exist for injection"
  GOOD: "secure_filename() strips '../'; uploaded file always lands in /uploads/"
  GOOD: "PostgreSQL port 5432 not exposed; only reachable from container network"
  GOOD: "JWT secret is 32-byte random; brute-force infeasible within time budget"

  The root cause is a TECHNICAL FACT observed from the codebase or server response.
  It answers: "what specific mechanism prevented this from working?"

# Rules

- Root node "ROOT" has exactly one outgoing "sub" edge to its first child
- Every non-root leaf node (no children) that is last in its plan must have a "backtrack" edge to its parent
- Siblings are connected by "next" edges in order: 1 → 2 → 3
- Parent connects to first child via "sub": ROOT → 1, or 2 → 2.1
- A child's step_range MUST be within its parent's step_range
- Node IDs MUST follow the convention above (no "g0", "g1" etc.)
- At most 1 "next" edge per node
- status: "done" = fully achieved, "failed" = attempted but not achieved, "partial" = some progress but not complete, "abandoned" = gave up
- goal_type (3 categories):
    "explore" = gather information (read, search, probe, list, enumerate)
    "think"   = reason and decide (analyze findings, form plan, choose approach)
    "act"     = execute (write code, run command, submit, test, modify state)
- result: SHORT outcome (max 20 words). Must state CONCRETE facts, not vague summaries.
    For explore: what SPECIFIC thing was found. "Flask app uses SQLAlchemy ORM; no raw SQL queries exist"
    For think:   what SPECIFIC decision was made. "Will try mass-assignment via PATCH /api/players/{id}"
    For act (done): what SPECIFIC result occurred. "Got valid session token for user admin@target.com"
    For act (failed): the CONCRETE root cause of failure — not just "it failed" but WHY and WHAT.
      BAD:  "Exploit failed"
      BAD:  "Authentication bypass not achieved"
      BAD:  "Script returned error"
      BAD: "Grep finds candidate dangerous calls in source"
      GOOD: "Server returned 403: endpoint requires valid JWT with admin role"
      GOOD: "SQLAlchemy parameterizes all queries; UNION injection impossible"
      GOOD: "secure_filename() strips path separators; traversal blocked"
      GOOD: "Grep finds candidate dangerous calls to dangerousFunc() in xxx.py"
    The root cause must be a TECHNICAL FACT the agent observed, not a restatement of the goal.
- CRITICAL: If the trajectory Outcome is "FAILED", then the ROOT node MUST have status "failed"
- CRITICAL: backtrack edges from failed nodes MUST have a non-empty label with the CONCRETE root cause
"#;

    let initial_message = format!(
        r#"Analyze this agent trajectory and extract the goal tree:

Trajectory Summary:
- Total steps: {}
- Outcome: {}
- Total cost: ${:.4}

Sample Steps (key moments):
{}

Extract all goals, sub-goals, and transitions. Return ONLY valid JSON, no additional text."#,
        step_count,
        outcome,
        total_cost,
        sample_trajectory_steps(trajectory, 30)
    );

    // [LLM_CALL: cached] system_prompt is fixed; user_message includes trajectory + corrections
    let mut user_message = initial_message.clone();
    #[allow(unused_assignments)]
    let mut last_response = String::new();

    for attempt in 0..=max_retries {
        let response = llm_client
            .as_ref()
            .complete(system_prompt, &user_message)
            .await?;
        last_response = response.clone();

        let mut tree = match parse_goal_tree_response(&response) {
            Ok(t) => t,
            Err(e) => {
                if attempt < max_retries {
                    eprintln!("Attempt {}: parse error, retrying...", attempt + 1);
                    user_message = format!(
                        "{}\n\n---\n\n\
                         Your previous response could not be parsed: {}\n\n\
                         Please output ONLY valid JSON matching the format above.",
                        initial_message, e
                    );
                    continue;
                }
                return Err(e);
            }
        };

        add_missing_backtrack_edges(&mut tree);
        propagate_status(&mut tree);
        let anomalies = collect_anomalies(&tree);

        if anomalies.is_empty() {
            eprintln!("Goal tree built successfully (attempt {})", attempt + 1);
            return Ok(tree);
        }

        if attempt < max_retries {
            eprintln!(
                "Attempt {}: {} anomalies found, asking LLM to fix...",
                attempt + 1,
                anomalies.len()
            );
            user_message = format!(
                "{}\n\n---\n\n\
                 Your previous output had these structural problems:\n{}\n\n\
                 Your previous (broken) JSON was:\n```json\n{}\n```\n\n\
                 Fix the anomalies above and return the corrected FULL JSON. No explanation, just JSON.",
                initial_message,
                anomalies.iter().map(|a| format!("- {}", a)).collect::<Vec<_>>().join("\n"),
                last_response.chars().take(3000).collect::<String>()
            );
        } else {
            for a in &anomalies {
                eprintln!("ANOMALY: {}", a);
            }
            // Fill empty backtrack labels from the source node's own label as fallback
            fill_missing_backtrack_labels(&mut tree);
            return Ok(tree);
        }
    }

    unreachable!()
}

/// Collect structural anomalies without modifying the tree.
/// Returns a list of human-readable anomaly descriptions.
fn collect_anomalies(tree: &GoalTransitionTree) -> Vec<String> {
    use std::collections::HashMap;

    let mut anomalies = Vec::new();

    let mut outgoing_counts: HashMap<&str, usize> = HashMap::new();
    for edge in &tree.edges {
        *outgoing_counts.entry(&edge.source_id).or_insert(0) += 1;
    }

    // Orphan nodes
    let mut reachable = std::collections::HashSet::new();
    let mut queue = vec![tree.root_id.as_str()];
    while let Some(nid) = queue.pop() {
        if !reachable.insert(nid) {
            continue;
        }
        for edge in &tree.edges {
            if edge.source_id == nid
                && (edge.edge_type == crate::models::GoalEdgeType::Sub
                    || edge.edge_type == crate::models::GoalEdgeType::Next)
            {
                queue.push(&edge.target_id);
            }
        }
    }
    for node in &tree.nodes {
        if !reachable.contains(node.node_id.as_str()) {
            anomalies.push(format!(
                "Node '{}' is orphaned (unreachable from root)",
                node.node_id
            ));
        }
    }

    // Missing outgoing edges
    for node in &tree.nodes {
        let count = outgoing_counts
            .get(node.node_id.as_str())
            .copied()
            .unwrap_or(0);
        if node.node_id != tree.root_id && count == 0 {
            anomalies.push(format!("Node '{}' has no outgoing edge", node.node_id));
        }
    }

    // Multiple Next edges
    for node in &tree.nodes {
        let next_count = tree
            .edges
            .iter()
            .filter(|e| {
                e.source_id == node.node_id && e.edge_type == crate::models::GoalEdgeType::Next
            })
            .count();
        if next_count > 1 {
            anomalies.push(format!(
                "Node '{}' has {} Next edges (max 1)",
                node.node_id, next_count
            ));
        }
    }

    // Opaque IDs: valid IDs are "ROOT" or digits/dots (e.g., "1", "2.3", "2.1.4")
    for node in &tree.nodes {
        let id = &node.node_id;
        let is_valid = id == "ROOT" || id.chars().all(|c| c.is_ascii_digit() || c == '.');
        if !is_valid {
            anomalies.push(format!(
                "Node '{}' uses invalid ID (must be ROOT or N.N.N format)",
                id
            ));
        }
    }

    // Backtrack edges from failed nodes must have a label explaining why
    {
        let failed_ids: std::collections::HashSet<&str> = tree
            .nodes
            .iter()
            .filter(|n| {
                n.status == crate::models::GoalStatus::Failed
                    || n.status == crate::models::GoalStatus::Abandoned
            })
            .map(|n| n.node_id.as_str())
            .collect();

        for edge in &tree.edges {
            if edge.edge_type == crate::models::GoalEdgeType::Backtrack
                && failed_ids.contains(edge.source_id.as_str())
                && edge.label.trim().is_empty()
            {
                anomalies.push(format!(
                    "Backtrack edge from failed node '{}' has no label — must explain WHY it failed",
                    edge.source_id
                ));
            }
        }
    }

    // Tree shape checker: enforce hierarchical structure and loop invariant.
    {
        use crate::models::GoalEdgeType;
        let sub_edges: Vec<_> = tree
            .edges
            .iter()
            .filter(|e| e.edge_type == GoalEdgeType::Sub)
            .collect();
        let next_edges: Vec<_> = tree
            .edges
            .iter()
            .filter(|e| e.edge_type == GoalEdgeType::Next)
            .collect();
        let bt_edges: Vec<_> = tree
            .edges
            .iter()
            .filter(|e| e.edge_type == GoalEdgeType::Backtrack)
            .collect();
        let root_id = &tree.root_id;

        // Rule 1: ROOT is level 0 — no sub/next edge points TO ROOT.
        let root_is_child = tree.edges.iter().any(|e| {
            e.target_id == *root_id
                && (e.edge_type == GoalEdgeType::Sub || e.edge_type == GoalEdgeType::Next)
        });
        if root_is_child {
            anomalies
                .push("SHAPE-1: ROOT must be level 0. No sub/next edge should target ROOT.".into());
        }

        // Rule 2: ROOT must have children (at least 2 phases).
        let root_sub: Vec<_> = sub_edges
            .iter()
            .filter(|e| e.source_id == *root_id)
            .collect();
        let mut root_child_count = 0;
        if let Some(first) = root_sub.first() {
            root_child_count = 1;
            let mut current = first.target_id.as_str();
            while let Some(next) = next_edges.iter().find(|e| e.source_id == current) {
                root_child_count += 1;
                current = &next.target_id;
            }
        }
        if root_child_count < 2 && tree.nodes.len() >= 5 {
            anomalies.push(format!(
                "SHAPE-2: ROOT has only {} child(ren). Need 2-4 phases. \
                 Structure: ROOT --sub--> 1 --next--> 2 --next--> 3 --backtrack--> ROOT.",
                root_child_count
            ));
        }

        // Rule 3: Loop invariant — every parent with children must have:
        //   exactly 1 incoming backtrack from its last child.
        // Find all parents (nodes that have an outgoing sub edge).
        let parents: Vec<&str> = sub_edges.iter().map(|e| e.source_id.as_str()).collect();
        for parent_id in &parents {
            // Find children: first child via sub, rest via next chain
            let first_child = sub_edges.iter().find(|e| e.source_id == *parent_id);
            if first_child.is_none() {
                continue;
            }

            let mut last_child = first_child.unwrap().target_id.as_str();
            while let Some(next) = next_edges.iter().find(|e| e.source_id == last_child) {
                last_child = &next.target_id;
            }

            // Check: last_child must have backtrack → parent
            let has_backtrack = bt_edges
                .iter()
                .any(|e| e.source_id == last_child && e.target_id == *parent_id);
            if !has_backtrack {
                anomalies.push(format!(
                    "SHAPE-3: Loop not closed for '{}'. Last child '{}' must have backtrack edge → '{}'. \
                     Add: {{\"source_id\": \"{}\", \"target_id\": \"{}\", \"edge_type\": \"backtrack\", \"label\": \"\"}}",
                    parent_id, last_child, parent_id, last_child, parent_id
                ));
            }
        }

        // Also check ROOT's loop
        if root_child_count >= 2 {
            let root_has_backtrack = bt_edges.iter().any(|e| e.target_id == *root_id);
            if !root_has_backtrack {
                anomalies.push(
                    "SHAPE-3: ROOT loop not closed. Last phase must have backtrack → ROOT.".into(),
                );
            }
        }

        // Rule 3b: Each sub-plan must be a closed chain ending in backtrack.
        // A node with a backtrack edge must NOT also have a "next" edge (it's the end of a chain).
        // A node with a "next" edge must NOT also backtrack (it's in the middle).
        for bt in &bt_edges {
            let source = bt.source_id.as_str();
            let also_has_next = next_edges.iter().any(|e| e.source_id == source);
            if also_has_next {
                let next_target = next_edges.iter().find(|e| e.source_id == source).unwrap();
                anomalies.push(format!(
                    "SHAPE-3b: Node '{}' has BOTH a backtrack (to '{}') AND a next (to '{}'). \
                     A node is either the end of a chain (backtrack only) or in the middle (next only), never both. \
                     If '{}' failed and the parent started a new plan, use a SEPARATE sub edge from the parent.",
                    source, bt.target_id, next_target.target_id, source
                ));
            }
        }

        // Rule 4: Depth — at least one phase must have sub-children.
        let phase_has_sub = parents.iter().any(|p| *p != root_id.as_str());
        if !phase_has_sub && tree.nodes.len() >= 5 {
            anomalies.push(
                "SHAPE-4: No depth — no phase has sub-children. \
                 At least one phase must have sub edges to actions (e.g., 1 --sub--> 1.1)."
                    .into(),
            );
        }

        // Rule 5: No sibling chain longer than 5 (too flat within a level).
        for sub in &sub_edges {
            let mut length = 1;
            let mut current = sub.target_id.as_str();
            while let Some(next) = next_edges.iter().find(|e| e.source_id == current) {
                length += 1;
                current = &next.target_id;
                if length > 5 {
                    break;
                }
            }
            if length > 4 {
                anomalies.push(format!(
                    "SHAPE-5: Node '{}' has {} children (max 4). \
                     Add an intermediate grouping layer. Example: instead of \
                     {0}→A→B→C→D→E, use {0}→X→Y where X→A→B and Y→C→D→E.",
                    sub.source_id, length
                ));
                break;
            }
        }
    }

    // Step range containment: derive parent from ID.
    // "2.1" → parent "2", "3" → parent "ROOT"
    let node_map: HashMap<&str, &crate::models::GoalNode> =
        tree.nodes.iter().map(|n| (n.node_id.as_str(), n)).collect();

    for node in &tree.nodes {
        if node.node_id == tree.root_id {
            continue;
        }
        let parent_id = if let Some(dot_pos) = node.node_id.rfind('.') {
            &node.node_id[..dot_pos]
        } else {
            "ROOT"
        };
        if let Some(parent) = node_map.get(parent_id) {
            let (ps, pe) = parent.step_range;
            let (cs, ce) = node.step_range;
            if cs < ps || ce > pe {
                anomalies.push(format!(
                    "Node '{}' step_range [{}-{}] exceeds parent '{}' range [{}-{}]",
                    node.node_id, cs, ce, parent.node_id, ps, pe
                ));
            }
        }
    }

    // SHAPE-6: Siblings must NOT have overlapping step_ranges.
    // Siblings = nodes sharing the same parent, connected via next edges.
    {
        use crate::models::GoalEdgeType;
        let sub_edges: Vec<_> = tree
            .edges
            .iter()
            .filter(|e| e.edge_type == GoalEdgeType::Sub)
            .collect();
        let next_edges: Vec<_> = tree
            .edges
            .iter()
            .filter(|e| e.edge_type == GoalEdgeType::Next)
            .collect();

        for sub in &sub_edges {
            // Collect sibling chain
            let mut siblings = vec![sub.target_id.as_str()];
            let mut current = sub.target_id.as_str();
            loop {
                let next = next_edges.iter().find(|e| e.source_id == current);
                match next {
                    Some(e) => {
                        siblings.push(&e.target_id);
                        current = &e.target_id;
                    }
                    None => break,
                }
            }

            // Check pairwise overlap between consecutive siblings
            for w in siblings.windows(2) {
                let a = node_map.get(w[0]);
                let b = node_map.get(w[1]);
                if let (Some(a_node), Some(b_node)) = (a, b) {
                    let (a_start, a_end) = a_node.step_range;
                    let (b_start, b_end) = b_node.step_range;
                    // Overlap: a_start < b_end AND b_start < a_end
                    if a_start < b_end && b_start < a_end && a_end > b_start {
                        anomalies.push(format!(
                            "SHAPE-6: Siblings '{}' [{}-{}] and '{}' [{}-{}] have overlapping step_ranges. \
                             Siblings must be non-overlapping and sequential.",
                            w[0], a_start, a_end, w[1], b_start, b_end
                        ));
                    }
                }
            }
        }
    }

    anomalies
}

/// Enforce: each node has at most one outgoing edge.
/// If a node has both Sub and Next outgoing, keep only the Sub (Sub defines hierarchy).
/// If a node has multiple Next outgoing, keep only the first.
#[allow(dead_code)]
/// Fill empty backtrack labels from failed nodes using the node's result field.
/// The result field contains the concrete root cause of failure.
/// Called as a fallback after max retries — ensures the graph is always complete.
fn fill_missing_backtrack_labels(tree: &mut GoalTransitionTree) {
    use std::collections::HashMap;
    let node_info: HashMap<String, (String, String, crate::models::GoalStatus)> = tree
        .nodes
        .iter()
        .map(|n| {
            (
                n.node_id.clone(),
                (n.result.clone(), n.label.clone(), n.status.clone()),
            )
        })
        .collect();

    for edge in &mut tree.edges {
        if edge.edge_type == crate::models::GoalEdgeType::Backtrack && edge.label.trim().is_empty()
        {
            if let Some((result, label, status)) = node_info.get(&edge.source_id) {
                if *status == crate::models::GoalStatus::Failed
                    || *status == crate::models::GoalStatus::Abandoned
                {
                    // Prefer result (has the root cause), fall back to label
                    edge.label = if !result.is_empty() {
                        result.chars().take(120).collect::<String>()
                    } else {
                        format!("Failed: {}", label.chars().take(80).collect::<String>())
                    };
                }
            }
        }
    }
}

fn enforce_single_outgoing_edge(tree: &mut GoalTransitionTree) {
    use std::collections::HashMap;

    let mut outgoing: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, edge) in tree.edges.iter().enumerate() {
        outgoing.entry(edge.source_id.clone()).or_default().push(i);
    }

    let mut to_remove = Vec::new();
    for (_node_id, edge_indices) in &outgoing {
        if edge_indices.len() <= 1 {
            continue;
        }

        let has_sub = edge_indices
            .iter()
            .any(|&i| tree.edges[i].edge_type == crate::models::GoalEdgeType::Sub);

        if has_sub {
            // Keep the Sub edge, remove all Next/Backtrack from this node
            for &i in edge_indices {
                if tree.edges[i].edge_type != crate::models::GoalEdgeType::Sub {
                    to_remove.push(i);
                }
            }
        } else {
            // Multiple Next edges: keep only the first, remove the rest
            for &i in &edge_indices[1..] {
                to_remove.push(i);
            }
        }
    }

    if !to_remove.is_empty() {
        to_remove.sort_unstable();
        to_remove.dedup();
        eprintln!(
            "Removed {} conflicting edge(s) to enforce single-outgoing rule",
            to_remove.len()
        );
        // Remove in reverse order to preserve indices
        for i in to_remove.into_iter().rev() {
            tree.edges.remove(i);
        }
    }
}

/// Ensure every last-in-plan node has a Backtrack edge to its parent.
/// "Last in plan" = no outgoing Next edge AND not the root.
/// This enforces the invariant: every node has exactly one outgoing edge.
fn add_missing_backtrack_edges(tree: &mut GoalTransitionTree) {
    use std::collections::HashMap;

    // Build parent map (Sub priority over Next)
    let mut parent_map: HashMap<String, Option<String>> = HashMap::new();
    parent_map.insert(tree.root_id.clone(), None);
    let mut changed = true;
    while changed {
        changed = false;
        for edge in &tree.edges {
            if edge.edge_type == crate::models::GoalEdgeType::Sub {
                let existing = parent_map.get(&edge.target_id);
                if existing.is_none() || existing == Some(&None) {
                    parent_map.insert(edge.target_id.clone(), Some(edge.source_id.clone()));
                    changed = true;
                }
            }
        }
        for edge in &tree.edges {
            if edge.edge_type == crate::models::GoalEdgeType::Next {
                if let Some(sp) = parent_map.get(&edge.source_id).cloned() {
                    if !parent_map.contains_key(&edge.target_id) {
                        parent_map.insert(edge.target_id.clone(), sp);
                        changed = true;
                    }
                }
            }
        }
    }

    // Find nodes with no outgoing edge and add backtrack to parent
    let mut to_add = Vec::new();
    for node in &tree.nodes {
        if node.node_id == tree.root_id {
            continue;
        }
        let has_outgoing = tree.edges.iter().any(|e| e.source_id == node.node_id);
        if !has_outgoing {
            if let Some(Some(parent_id)) = parent_map.get(&node.node_id) {
                to_add.push(crate::models::GoalEdge {
                    edge_type: crate::models::GoalEdgeType::Backtrack,
                    source_id: node.node_id.clone(),
                    target_id: parent_id.clone(),
                    label: String::new(),
                });
            }
        }
    }

    if !to_add.is_empty() {
        eprintln!("Added {} missing backtrack edge(s)", to_add.len());
        tree.edges.extend(to_add);
    }
}

/// Fix inconsistent statuses bottom-up.
/// If a parent is marked "done" but has any failed/abandoned children,
/// mark it as "partial" (Wip) to indicate partial success — some subgoals
/// succeeded while others didn't. This preserves the LLM's intent without
/// claiming full success where failures exist.
fn propagate_status(tree: &mut GoalTransitionTree) {
    use crate::models::GoalStatus;
    use std::collections::HashMap;

    // Build parent→children map from edges
    let mut parent_map: HashMap<String, Option<String>> = HashMap::new();
    let root_id = tree.root_id.clone();
    parent_map.insert(root_id.clone(), None);
    // Sub edges always define parent (higher priority than Next).
    // Process Sub edges first, then Next edges only for nodes not yet assigned.
    let mut changed = true;
    while changed {
        changed = false;
        // Pass 1: Sub edges (definitive parent-child)
        for edge in &tree.edges {
            if edge.edge_type == crate::models::GoalEdgeType::Sub {
                let existing = parent_map.get(&edge.target_id);
                if existing.is_none() || existing == Some(&None) {
                    parent_map.insert(edge.target_id.clone(), Some(edge.source_id.clone()));
                    changed = true;
                }
            }
        }
        // Pass 2: Next edges (sibling inference, only if Sub didn't already set parent)
        for edge in &tree.edges {
            if edge.edge_type == crate::models::GoalEdgeType::Next {
                if let Some(sp) = parent_map.get(&edge.source_id).cloned() {
                    if !parent_map.contains_key(&edge.target_id) {
                        parent_map.insert(edge.target_id.clone(), sp);
                        changed = true;
                    }
                }
            }
        }
    }

    let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
    for (child, parent_opt) in &parent_map {
        if let Some(parent) = parent_opt {
            children_of
                .entry(parent.clone())
                .or_default()
                .push(child.clone());
        }
    }

    let mut status_map: HashMap<String, GoalStatus> = tree
        .nodes
        .iter()
        .map(|n| (n.node_id.clone(), n.status.clone()))
        .collect();

    // Bottom-up pass: deepest nodes first
    let mut node_ids: Vec<String> = status_map.keys().cloned().collect();
    node_ids.sort_by(|a, b| b.matches('.').count().cmp(&a.matches('.').count()));

    for nid in &node_ids {
        if let Some(children) = children_of.get(nid) {
            if children.is_empty() {
                continue;
            }
            let current_status = status_map.get(nid).cloned().unwrap_or(GoalStatus::Partial);
            let has_failed_child = children.iter().any(|c| {
                let s = status_map.get(c);
                s == Some(&GoalStatus::Failed) || s == Some(&GoalStatus::Abandoned)
            });
            let all_children_failed = children.iter().all(|c| {
                let s = status_map.get(c);
                s == Some(&GoalStatus::Failed) || s == Some(&GoalStatus::Abandoned)
            });

            if all_children_failed {
                status_map.insert(nid.clone(), GoalStatus::Failed);
            } else if current_status == GoalStatus::Done && has_failed_child {
                // Some children failed, some succeeded — partial success
                status_map.insert(nid.clone(), GoalStatus::Partial);
            }
        }
    }

    for node in &mut tree.nodes {
        if let Some(status) = status_map.get(&node.node_id) {
            node.status = status.clone();
        }
    }
}

/// Replace opaque node IDs (g0, g1, ...) with hierarchical IDs (1, 1.1, 1.2, ...).
/// Follows Sub→Next edge chains to determine ordering within each level.
#[allow(dead_code)]
fn assign_hierarchical_ids(tree: &mut GoalTransitionTree) {
    use std::collections::HashMap;

    let root_id = if !tree.root_id.is_empty() {
        tree.root_id.clone()
    } else if let Some(first) = tree.nodes.first() {
        first.node_id.clone()
    } else {
        return;
    };

    // Build parent map from Sub/Next edges
    let mut parent_map: HashMap<String, Option<String>> = HashMap::new();
    parent_map.insert(root_id.clone(), None);
    let mut changed = true;
    while changed {
        changed = false;
        for edge in &tree.edges {
            match edge.edge_type {
                crate::models::GoalEdgeType::Sub => {
                    if !parent_map.contains_key(&edge.target_id) {
                        parent_map.insert(edge.target_id.clone(), Some(edge.source_id.clone()));
                        changed = true;
                    }
                }
                crate::models::GoalEdgeType::Next => {
                    if let Some(source_parent) = parent_map.get(&edge.source_id).cloned() {
                        if !parent_map.contains_key(&edge.target_id) {
                            parent_map.insert(edge.target_id.clone(), source_parent);
                            changed = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // BFS assigning hierarchical IDs. For each node, collect all children
    // (nodes whose parent in the parent_map is this node) and assign in edge order.
    let mut id_map: HashMap<String, String> = HashMap::new();
    let mut counters: HashMap<String, usize> = HashMap::new();
    id_map.insert(root_id.clone(), "ROOT".to_string());

    let mut queue = vec![root_id.clone()];
    while let Some(current) = queue.pop() {
        // Collect all children of this node (from parent_map)
        let children: Vec<String> = parent_map
            .iter()
            .filter_map(|(child, parent_opt)| {
                if let Some(parent) = parent_opt {
                    if parent == &current {
                        Some(child.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Order children: start with Sub target, then follow Next chain,
        // then append any remaining children not in the chain
        let mut ordered: Vec<String> = Vec::new();
        let first_child = tree
            .edges
            .iter()
            .find(|e| e.source_id == current && e.edge_type == crate::models::GoalEdgeType::Sub)
            .map(|e| e.target_id.clone());

        if let Some(first) = first_child {
            ordered.push(first.clone());
            let mut cur = first;
            loop {
                let next = tree.edges.iter().find(|e| {
                    e.source_id == cur
                        && e.edge_type == crate::models::GoalEdgeType::Next
                        && !ordered.contains(&e.target_id)
                });
                match next {
                    Some(e) => {
                        ordered.push(e.target_id.clone());
                        cur = e.target_id.clone();
                    }
                    None => break,
                }
            }
        }
        // Append any children not yet in the ordered list
        for child in &children {
            if !ordered.contains(child) {
                ordered.push(child.clone());
            }
        }

        for child_id in ordered {
            if !id_map.contains_key(&child_id) {
                let counter = counters.entry(current.clone()).or_insert(0);
                *counter += 1;
                let parent_hid = id_map.get(&current).unwrap();
                let child_hid = format!("{}.{}", parent_hid, counter);
                id_map.insert(child_id.clone(), child_hid);
                queue.push(child_id);
            }
        }
    }

    // Apply rename: update all node_ids and edge source/target references
    tree.root_id = id_map
        .get(&tree.root_id)
        .cloned()
        .unwrap_or_else(|| tree.root_id.clone());
    for node in &mut tree.nodes {
        if let Some(new_id) = id_map.get(&node.node_id) {
            node.node_id = new_id.clone();
        }
    }
    for edge in &mut tree.edges {
        if let Some(new_id) = id_map.get(&edge.source_id) {
            edge.source_id = new_id.clone();
        }
        if let Some(new_id) = id_map.get(&edge.target_id) {
            edge.target_id = new_id.clone();
        }
    }
}

/// Remove nodes unreachable from root and edges referencing them.
/// The LLM sometimes generates orphan nodes only referenced by backtrack edges
/// but not connected into the tree via Sub/Next from root.
#[allow(dead_code)]
fn prune_orphan_nodes(tree: &mut GoalTransitionTree) {
    use std::collections::HashSet;

    let root_id = if !tree.root_id.is_empty() {
        tree.root_id.clone()
    } else if let Some(first) = tree.nodes.first() {
        first.node_id.clone()
    } else {
        return;
    };

    // BFS from root following Sub and Next edges (forward direction only)
    let mut reachable = HashSet::new();
    let mut queue = vec![root_id.clone()];
    while let Some(nid) = queue.pop() {
        if !reachable.insert(nid.clone()) {
            continue;
        }
        for edge in &tree.edges {
            if edge.source_id == nid
                && (edge.edge_type == crate::models::GoalEdgeType::Sub
                    || edge.edge_type == crate::models::GoalEdgeType::Next)
            {
                queue.push(edge.target_id.clone());
            }
        }
    }

    let before = tree.nodes.len();
    tree.nodes.retain(|n| reachable.contains(&n.node_id));
    tree.edges
        .retain(|e| reachable.contains(&e.source_id) && reachable.contains(&e.target_id));

    if tree.nodes.len() < before {
        eprintln!(
            "Pruned {} orphan node(s) unreachable from root",
            before - tree.nodes.len()
        );
    }
}

/// Build a stub Goal Transition Tree without LLM (for testing/fallback).
pub fn build_stub(trajectory: &Trajectory) -> GoalTransitionTree {
    use crate::models::{GoalStatus, GoalType};

    // Create a simple single-node tree based on trajectory outcome
    let status = if trajectory.outcome.to_lowercase().contains("solved") {
        GoalStatus::Done
    } else if trajectory.outcome.to_lowercase().contains("failed") {
        GoalStatus::Failed
    } else {
        GoalStatus::Abandoned
    };

    let root_node = GoalNode {
        node_id: "g0".to_string(),
        label: format!("Complete task ({})", trajectory.outcome),
        goal_type: GoalType::Explore,
        status,
        result: String::new(),
        details: String::new(),
        level: 0,
        step_range: (0, trajectory.steps.len() - 1),
        cost: trajectory.total_cost.clone(),
        reasoning_artifacts: vec![],
    };

    GoalTransitionTree {
        nodes: vec![root_node],
        edges: vec![],
        root_id: "g0".to_string(),
    }
}

#[cfg(feature = "llm")]
fn sample_trajectory_steps(trajectory: &Trajectory, max_samples: usize) -> String {
    let step_count = trajectory.steps.len();

    if step_count <= max_samples {
        return trajectory
            .steps
            .iter()
            .enumerate()
            .map(|(idx, step)| format_step_summary(idx, step))
            .collect::<Vec<_>>()
            .join("\n\n");
    }

    // Sample strategy: biased toward the end (where failures/conclusions happen).
    // First 3 + sparse middle + dense last 40% (captures failure details).
    let mut sampled_indices = vec![0, 1, 2]; // First 3

    // Sparse middle (first 60% of trajectory)
    let middle_end = (step_count * 6) / 10;
    let middle_count = max_samples / 3;
    for i in 1..=middle_count {
        let idx = 3 + ((middle_end - 3) * i) / (middle_count + 1);
        sampled_indices.push(idx);
    }

    // Dense tail (last 40% — where exploit attempts and failures live)
    let tail_start = middle_end;
    let tail_count = max_samples - middle_count - 3;
    for i in 0..tail_count {
        let idx = tail_start + ((step_count - tail_start) * i) / tail_count;
        sampled_indices.push(idx);
    }

    // Always include the very last steps
    if step_count >= 3 {
        sampled_indices.extend(vec![step_count - 3, step_count - 2, step_count - 1]);
    }

    // Deduplicate and sort
    sampled_indices.sort_unstable();
    sampled_indices.dedup();

    sampled_indices
        .iter()
        .filter_map(|&idx| trajectory.steps.get(idx).map(|step| (idx, step)))
        .map(|(idx, step)| format_step_summary(idx, step))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(feature = "llm")]
fn format_step_summary(idx: usize, step: &crate::models::Step) -> String {
    let item_summaries: Vec<String> = step
        .items
        .iter()
        .take(5)
        .map(|item| {
            let primary_obj = item
                .args
                .get("file_path")
                .or_else(|| item.args.get("command"))
                .map(|s| s.as_str())
                .unwrap_or("N/A");

            format!(
                "  - {:?} [{}]: {}",
                item.category,
                primary_obj,
                truncate(&item.content, 80)
            )
        })
        .collect();

    let cost = step.items.iter().map(|i| i.cost.dollar_cost).sum::<f64>();

    format!(
        "Step #{} (${:.4}):\n{}{}",
        idx,
        cost,
        item_summaries.join("\n"),
        if step.items.len() > 5 {
            format!("\n  ... and {} more items", step.items.len() - 5)
        } else {
            String::new()
        }
    )
}

#[cfg(feature = "llm")]
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

#[cfg(feature = "llm")]
fn parse_goal_tree_response(response: &str) -> LLMResult<GoalTransitionTree> {
    use serde_json;

    // Try to extract JSON from response (might have markdown code blocks)
    let json_str = if response.contains("```json") {
        response
            .split("```json")
            .nth(1)
            .and_then(|s| s.split("```").next())
            .unwrap_or(response)
    } else if response.contains("```") {
        response.split("```").nth(1).unwrap_or(response)
    } else {
        response
    }
    .trim();

    // Parse JSON
    let parsed: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
        crate::llm::traits::LLMError::InvalidResponse(format!(
            "Failed to parse JSON response: {}",
            e
        ))
    })?;

    // Extract nodes
    let nodes: Vec<GoalNode> = parsed["nodes"]
        .as_array()
        .ok_or_else(|| {
            crate::llm::traits::LLMError::InvalidResponse(
                "Missing 'nodes' array in response".to_string(),
            )
        })?
        .iter()
        .map(|node| {
            use crate::models::{GoalStatus, GoalType};

            let step_range = node["step_range"]
                .as_array()
                .and_then(|arr| Some((arr[0].as_u64()? as usize, arr[1].as_u64()? as usize)))
                .unwrap_or((0, 0));

            let status_str = node["status"].as_str().unwrap_or("partial");
            let status = match status_str {
                "achieved" | "done" => GoalStatus::Done,
                "failed" => GoalStatus::Failed,
                "abandoned" => GoalStatus::Abandoned,
                _ => GoalStatus::Partial,
            };

            let label = node["label"].as_str().unwrap_or("Unknown goal");
            let result = node["result"].as_str().unwrap_or("").to_string();

            // Parse goal_type from LLM output, fall back to inference from label
            let goal_type_str = node["goal_type"].as_str().unwrap_or("");
            let goal_type = match goal_type_str {
                "explore" => GoalType::Explore,
                "think" | "analyze" | "plan" => GoalType::Think,
                "act" | "verify" | "report" | "write" | "execute" => GoalType::Act,
                _ => {
                    let l = label.to_lowercase();
                    if l.contains("write")
                        || l.contains("craft")
                        || l.contains("create")
                        || l.contains("run")
                        || l.contains("execute")
                        || l.contains("submit")
                        || l.contains("test")
                        || l.contains("verify")
                        || l.contains("attempt")
                    {
                        GoalType::Act
                    } else if l.contains("analyze")
                        || l.contains("decide")
                        || l.contains("plan")
                        || l.contains("identify")
                        || l.contains("determine")
                    {
                        GoalType::Think
                    } else {
                        GoalType::Explore
                    }
                }
            };

            let details = node["details"].as_str().unwrap_or("").to_string();

            GoalNode {
                node_id: node["node_id"].as_str().unwrap_or("g0").to_string(),
                label: label.to_string(),
                goal_type,
                status,
                result,
                details,
                level: 0,
                step_range,
                cost: crate::models::Cost::default(),
                reasoning_artifacts: vec![],
            }
        })
        .collect();

    // Extract edges (accept both "edge_type" and legacy "transition_type" field names)
    let edges: Vec<GoalEdge> = parsed["edges"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|edge| {
            use crate::models::GoalEdgeType;

            let type_str = edge["edge_type"]
                .as_str()
                .or_else(|| edge["transition_type"].as_str())
                .unwrap_or("next");
            let edge_type = match type_str {
                "backtrack" => GoalEdgeType::Backtrack,
                "sub" | "refinement" => GoalEdgeType::Sub,
                _ => GoalEdgeType::Next,
            };

            GoalEdge {
                edge_type,
                source_id: edge["source_id"].as_str().unwrap_or("ROOT").to_string(),
                target_id: edge["target_id"].as_str().unwrap_or("1").to_string(),
                label: String::new(),
            }
        })
        .collect();

    // Root is "ROOT" or the first node
    let root_id = nodes
        .first()
        .map(|n| n.node_id.clone())
        .unwrap_or_else(|| "ROOT".to_string());

    Ok(GoalTransitionTree {
        nodes,
        edges,
        root_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Cost, Item, ItemCategory, Step};

    #[test]
    fn test_build_stub() {
        use chrono::NaiveDateTime;
        let epoch = NaiveDateTime::default();

        let trajectory = Trajectory {
            label: "test".to_string(),
            steps: vec![Step {
                step_id: 0,
                items: vec![],
                timestamp_start: epoch,
                timestamp_end: epoch,
                raw_line_range: (0, 0),
            }],
            total_cost: Cost::default(),
            outcome: "FAILED".to_string(),
        };

        let tree = build_stub(&trajectory);
        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.nodes[0].status, crate::models::GoalStatus::Failed);
    }
}
