/// Deterministic Cost Map builder.
///
/// Builds a treemap-style CostMap from a Trajectory. Groups costs by action
/// category as top-level children of a single root node.
///
/// When a GoalTransitionTree is available, costs are grouped by goal first,
/// then by category within each goal.
use std::collections::HashMap;

use crate::models::{Cost, CostMap, CostMapNode, GoalTransitionTree, Trajectory};

/// Map sub_category to cost map category string.
fn category_from_sub(sub_category: Option<&str>) -> String {
    match sub_category {
        None => "run".into(),
        Some(sub) => {
            let s = sub.to_lowercase();
            if s.contains("read") {
                "read".into()
            } else if s.contains("write") || s.contains("edit") {
                "write".into()
            } else if s.contains("bash") || s.contains("run") || s.contains("script") {
                "run".into()
            } else if s.contains("think") || s.contains("reason") {
                "think".into()
            } else if s.contains("search") || s.contains("grep") || s.contains("find") {
                "run".into()
            } else if s.contains("setup") {
                "event".into()
            } else if s.contains("tool_output") {
                "run".into()
            } else {
                "run".into()
            }
        }
    }
}

/// Build a CostMap from a Trajectory.
/// If goal_tree is provided, costs are grouped hierarchically by goal.
/// Otherwise flat grouping by category.
pub fn build(trajectory: &Trajectory, goal_tree: Option<&GoalTransitionTree>) -> CostMap {
    match goal_tree {
        Some(tree) => build_with_goals(trajectory, tree),
        None => build_flat(trajectory),
    }
}

fn build_flat(trajectory: &Trajectory) -> CostMap {
    let mut category_costs: HashMap<String, Cost> = HashMap::new();

    for step in &trajectory.steps {
        for item in &step.items {
            let cat = category_from_sub(item.sub_category.as_deref());
            let entry = category_costs.entry(cat).or_insert_with(Cost::default);
            *entry = entry.add(&item.cost);
        }
    }

    let mut children: Vec<CostMapNode> = category_costs
        .into_iter()
        .filter(|(_, cost)| cost.dollar_cost > 0.0 || cost.input_tokens > 0)
        .map(|(cat, cost)| {
            let label = capitalize(&cat);
            CostMapNode {
                node_id: format!("cat_{}", cat),
                label,
                cost,
                children: Vec::new(),
                category: Some(cat),
                step_range: None,
            }
        })
        .collect();

    children.sort_by(|a, b| {
        b.cost
            .dollar_cost
            .partial_cmp(&a.cost.dollar_cost)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut total_cost = trajectory.total_cost.clone();
    if total_cost.dollar_cost == 0.0 && !children.is_empty() {
        total_cost = Cost::default();
        for child in &children {
            total_cost = total_cost.add(&child.cost);
        }
    }

    let label = if trajectory.label.is_empty() {
        "Trajectory".into()
    } else {
        trajectory.label.clone()
    };
    CostMap {
        root: CostMapNode {
            node_id: "root".into(),
            label,
            cost: total_cost,
            children,
            category: None,
            step_range: None,
        },
    }
}

/// Build cost map mirroring the goal tree's hierarchical structure.
/// Each goal node becomes a cost map node; parent-child relationships are preserved.
fn build_with_goals(trajectory: &Trajectory, goal_tree: &GoalTransitionTree) -> CostMap {
    // Build parent map from goal tree edges (Sub = parent-child, Next = siblings)
    let mut parent_map: HashMap<String, Option<String>> = HashMap::new();
    let root_id = if !goal_tree.root_id.is_empty() {
        goal_tree.root_id.clone()
    } else {
        goal_tree.nodes[0].node_id.clone()
    };
    parent_map.insert(root_id.clone(), None);

    let mut changed = true;
    while changed {
        changed = false;
        for edge in &goal_tree.edges {
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

    // Compute cost per goal from trajectory step ranges
    let goal_costs: HashMap<String, Cost> = goal_tree
        .nodes
        .iter()
        .map(|goal_node| {
            let (start, end) = goal_node.step_range;
            let mut cost = Cost::default();
            for step in &trajectory.steps {
                if step.step_id >= start && step.step_id <= end {
                    for item in &step.items {
                        cost = cost.add(&item.cost);
                    }
                }
            }
            (goal_node.node_id.clone(), cost)
        })
        .collect();

    // Build cost map nodes recursively from root.
    // Steps not covered by any child goal become individual step sub-nodes.
    fn build_node(
        node_id: &str,
        goal_tree: &GoalTransitionTree,
        parent_map: &HashMap<String, Option<String>>,
        goal_costs: &HashMap<String, Cost>,
        trajectory: &Trajectory,
    ) -> CostMapNode {
        let goal_node = goal_tree.nodes.iter().find(|n| n.node_id == node_id);
        let label = goal_node
            .map(|n| n.label.clone())
            .unwrap_or_else(|| node_id.to_string());
        let cost = goal_costs.get(node_id).cloned().unwrap_or_default();
        let step_range = goal_node.map(|n| n.step_range);

        // Find goal-children of this node
        let children_ids: Vec<String> = parent_map
            .iter()
            .filter_map(|(child_id, parent_opt)| {
                if let Some(parent_id) = parent_opt {
                    if parent_id == node_id {
                        Some(child_id.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        let mut children: Vec<CostMapNode> = children_ids
            .iter()
            .map(|child_id| build_node(child_id, goal_tree, parent_map, goal_costs, trajectory))
            .collect();

        // Group items by (action_type, target) for steps not covered by sub-goals.
        if let Some((my_start, my_end)) = step_range {
            let child_ranges: Vec<(usize, usize)> = children_ids
                .iter()
                .filter_map(|cid| {
                    goal_tree
                        .nodes
                        .iter()
                        .find(|n| n.node_id == *cid)
                        .map(|n| n.step_range)
                })
                .collect();

            let is_leaf_goal = children_ids.is_empty();

            // Collect items grouped by (action_type, target).
            // Tool outputs are attributed to the most recent preceding tool call.
            let mut groups: HashMap<(String, String), (Cost, usize)> = HashMap::new();

            for step in &trajectory.steps {
                if step.step_id < my_start || step.step_id > my_end {
                    continue;
                }
                if !is_leaf_goal {
                    let covered_by_child = child_ranges
                        .iter()
                        .any(|(cs, ce)| step.step_id >= *cs && step.step_id <= *ce);
                    if covered_by_child {
                        continue;
                    }
                }

                let mut last_tool_key: (String, String) = ("other".into(), String::new());

                for item in &step.items {
                    if item.cost.dollar_cost == 0.0 && item.cost.input_tokens == 0 {
                        continue;
                    }
                    let (action_type, target) = item_action_target(item);

                    if action_type == "tool_output" {
                        // Attribute to the last tool call
                        let entry = groups
                            .entry(last_tool_key.clone())
                            .or_insert_with(|| (Cost::default(), 0));
                        entry.0 = entry.0.add(&item.cost);
                        // Don't increment count — this is part of the same tool call
                    } else {
                        last_tool_key = (action_type.clone(), target.clone());
                        let entry = groups
                            .entry((action_type, target))
                            .or_insert_with(|| (Cost::default(), 0));
                        entry.0 = entry.0.add(&item.cost);
                        entry.1 += 1;
                    }
                }
            }

            for ((action_type, target), (group_cost, count)) in groups {
                let label = if target.is_empty() {
                    if count > 1 {
                        format!("{} (x{})", action_type, count)
                    } else {
                        action_type.clone()
                    }
                } else {
                    if count > 1 {
                        format!("{}: {} (x{})", action_type, target, count)
                    } else {
                        format!("{}: {}", action_type, target)
                    }
                };
                let cat = category_from_sub(Some(&action_type));
                let safe_id: String = label
                    .chars()
                    .take(20)
                    .filter(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                children.push(CostMapNode {
                    node_id: format!("{}_{}", node_id, safe_id),
                    label,
                    cost: group_cost,
                    children: Vec::new(),
                    category: Some(cat),
                    step_range: Some((my_start, my_end)),
                });
            }
        }

        children.sort_by(|a, b| {
            b.cost
                .dollar_cost
                .partial_cmp(&a.cost.dollar_cost)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        CostMapNode {
            node_id: node_id.to_string(),
            label,
            cost,
            children,
            category: None,
            step_range,
        }
    }

    let root_node = build_node(&root_id, goal_tree, &parent_map, &goal_costs, trajectory);

    let label = if trajectory.label.is_empty() {
        "Trajectory".into()
    } else {
        trajectory.label.clone()
    };
    CostMap {
        root: CostMapNode {
            node_id: "root".into(),
            label,
            cost: trajectory.total_cost.clone(),
            children: root_node.children,
            category: None,
            step_range: None,
        },
    }
}

/// Extract (action_type, target) from an item for grouping.
/// Examples: ("read", "pdf_font.c"), ("bash", "grep -n ..."), ("reasoning", "")
fn item_action_target(item: &crate::models::Item) -> (String, String) {
    use crate::models::ItemCategory;

    // Determine action type: prefer sub_category, then infer from args/content
    let action = if let Some(ref sub) = item.sub_category {
        if !sub.is_empty() {
            sub.to_lowercase()
        } else {
            infer_action(item)
        }
    } else {
        infer_action(item)
    };

    // Determine target
    let target = if let Some(fp) = item.args.get("file_path") {
        fp.rsplit('/').next().unwrap_or(fp).to_string()
    } else if let Some(cmd) = item.args.get("command") {
        let parts: Vec<&str> = cmd.splitn(2, ' ').collect();
        let cmd_name = parts[0].rsplit('/').next().unwrap_or(parts[0]);
        if parts.len() > 1 {
            let args_clean: String = parts[1]
                .chars()
                .filter(|c| *c != '"' && *c != '\'' && *c != '\\')
                .take(25)
                .collect();
            format!("{} {}", cmd_name, args_clean)
        } else {
            cmd_name.to_string()
        }
    } else if let Some(pattern) = item.args.get("pattern") {
        let clean: String = pattern
            .chars()
            .filter(|c| *c != '"' && *c != '\'' && *c != '\\')
            .take(25)
            .collect();
        clean
    } else {
        String::new()
    };

    (action, target)
}

fn infer_action(item: &crate::models::Item) -> String {
    use crate::models::ItemCategory;

    if matches!(item.category, ItemCategory::Think) {
        return "reasoning".into();
    }

    // Infer from args
    if item.args.contains_key("file_path") {
        return "read".into();
    }
    if item.args.contains_key("command") {
        let cmd = item.args.get("command").unwrap();
        let first_word = cmd.split_whitespace().next().unwrap_or("");
        let cmd_name = first_word.rsplit('/').next().unwrap_or(first_word);
        return match cmd_name {
            "grep" | "rg" | "find" | "ag" => "search".into(),
            "cat" | "head" | "tail" | "less" => "read".into(),
            "sed" | "tee" => "write".into(),
            "python3" | "python" | "ruby" | "node" => "script".into(),
            _ => "bash".into(),
        };
    }
    if item.args.contains_key("pattern") {
        return "search".into();
    }

    // Infer from content
    let c = item.content.to_lowercase();
    if c.contains("tool_result") || c.starts_with("}]},") || c.starts_with("]},") {
        return "tool_output".into();
    }
    if c.contains("'input': {") || c.contains("\"input\": {") {
        // JSON-embedded tool call (e.g. {'input': {'binary': ...}})
        if c.contains("binary") || c.contains("run") {
            return "run".into();
        }
        return "bash".into();
    }
    if c.contains("reasoning:") || c.contains("let me") || c.contains("i need to") {
        return "reasoning".into();
    }
    if c.contains("workspace") || c.contains("prepared") || c.contains("setup") {
        return "setup".into();
    }

    "misc".into()
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

/// Build a clean label for a cost map leaf node from an item.
/// Strips JSON cruft, tool_result wrappers, and extracts meaningful content.
fn clean_item_label(item: &crate::models::Item, step_id: usize, item_idx: usize) -> String {
    use crate::models::ItemCategory;

    // Use sub_category or args for structured items
    if let Some(ref sub) = item.sub_category {
        if !sub.is_empty() {
            let args_summary = {
                let args = &item.args;
                if let Some(cmd) = args.get("command") {
                    truncate_str(cmd, 40)
                } else if let Some(fp) = args.get("file_path") {
                    fp.clone()
                } else {
                    String::new()
                }
            };

            if args_summary.is_empty() {
                return format!("{} (s{}:{})", sub, step_id, item_idx);
            } else {
                return format!("{}: {}", sub, truncate_str(&args_summary, 40));
            }
        }
    }

    // For think items, use the beginning of content
    if matches!(item.category, ItemCategory::Think) {
        let clean = strip_json_prefix(&item.content);
        return format!("[think] {}", truncate_str(&clean, 40));
    }

    // For other items, try to extract meaningful text
    let clean = strip_json_prefix(&item.content);
    if clean.is_empty() {
        format!("s{}:{}", step_id, item_idx)
    } else {
        truncate_str(&clean, 50)
    }
}

fn strip_json_prefix(content: &str) -> String {
    let s = content.trim();
    // Skip leading JSON structural characters
    let stripped = s.trim_start_matches(|c: char| {
        c == '}'
            || c == ']'
            || c == ','
            || c == '{'
            || c == '['
            || c == '\''
            || c == '"'
            || c == ' '
            || c == '\n'
    });
    // If it still looks like JSON tool_result, try to extract 'content' field
    if stripped.contains("'content':") || stripped.contains("\"content\":") {
        if let Some(pos) = stripped.find("'content': '") {
            let after = &stripped[pos + 12..];
            if let Some(end) = after.find('\'') {
                let extracted = &after[..end];
                if extracted.len() > 5 {
                    return extracted.replace("\\n", " ").trim().to_string();
                }
            }
        }
        if let Some(pos) = stripped.find("'content': \"") {
            let after = &stripped[pos + 12..];
            if let Some(end) = after.find('"') {
                let extracted = &after[..end];
                if extracted.len() > 5 {
                    return extracted.replace("\\n", " ").trim().to_string();
                }
            }
        }
    }
    // Just return the first line, cleaned
    let first_line = stripped.lines().next().unwrap_or("").trim();
    first_line.replace('\t', " ").replace("  ", " ")
}

fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let end: String = s.chars().take(max - 3).collect();
        format!("{}...", end)
    }
}
