use regex::Regex;
/// Deterministic Activity Graph builder.
///
/// Transforms a parsed Trajectory into an ActivityGraph by:
/// 1. Extracting path targets from each action item
/// 2. Grouping operations by (goal_category, target_path)
/// 3. Building a directory hierarchy via path containment (parent_id)
/// 4. Creating edges between operations in chronological visit order
use std::collections::HashMap;
use std::sync::LazyLock;

use crate::models::{
    ActivityEdge, ActivityGraph, ActivityNode, Cost, GoalCategory, OpType, Operation, Trajectory,
};

/// Matches filesystem paths (absolute or relative) in command strings.
static PATH_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|\s)(/[^\s;|&<>]+|\.{0,2}/[^\s;|&<>]+|[a-zA-Z0-9_-]+/[^\s;|&<>]+)").unwrap()
});

/// Matches HTTP(S) URLs in command strings.
static URL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"https?://[^\s;|&<>'""]+"#).unwrap());

/// Classify into a GoalCategory from sub_category string or args.
fn classify_goal_category(sub_category: Option<&str>) -> GoalCategory {
    match sub_category {
        Some(sub) if !sub.is_empty() => {
            let s = sub.to_lowercase();
            if s == "read" || s.contains("read_file") {
                GoalCategory::Read
            } else if s == "write" || s.contains("write_file") {
                GoalCategory::Write
            } else if s == "edit" || s.contains("edit_file") || s.contains("patch") {
                GoalCategory::Edit
            } else if s == "bash" || s == "run" || s.contains("exec") || s.contains("command") {
                GoalCategory::Run
            } else if s == "grep"
                || s == "glob"
                || s == "find"
                || s.contains("list")
                || s.contains("ls")
                || s.contains("search")
            {
                GoalCategory::List
            } else {
                GoalCategory::Run
            }
        }
        _ => GoalCategory::Run, // Default for items with args but no sub_category (pocgen)
    }
}

/// Extract the filesystem path target from an action item's args or content.
///
/// Priority: args["file_path"] > args["command"] > args["pattern"] > content scan > sub_category.
/// The content scan handles parsers that put the command text in content (e.g., "[COMMAND] curl ...")
/// rather than in args["command"].
fn extract_target_path(
    args: &HashMap<String, String>,
    sub_category: Option<&str>,
    content: &str,
) -> String {
    if let Some(path) = args.get("file_path") {
        return path.clone();
    }
    if let Some(cmd) = args.get("command") {
        return extract_target_from_command(cmd);
    }
    if let Some(pattern) = args.get("pattern") {
        return format!("[search: {}]", &pattern[..pattern.len().min(20)]);
    }
    // Fallback: extract from content field (handles parsers that put commands in content)
    if let Some(target) = extract_target_from_content(content) {
        return target;
    }
    if let Some(sub) = sub_category {
        if !sub.is_empty() {
            return format!("[{}]", sub);
        }
    }
    "[workspace]".to_string()
}

/// Extract the primary TARGET OBJECT from a command string.
/// Priority: URL > filesystem path > command name (last resort).
/// The target is what the command ACTS ON, not the command itself.
fn extract_target_from_command(cmd: &str) -> String {
    // URLs are first-class targets (endpoints the agent interacts with)
    if let Some(m) = URL_RE.find(cmd) {
        let url = m.as_str();
        // Normalize: strip query params, keep path
        if let Some(path_start) = url.find("://").map(|i| i + 3) {
            if let Some(path_pos) = url[path_start..].find('/') {
                let host_and_path = &url[..path_start
                    + path_pos
                    + url[path_start + path_pos..]
                        .find('?')
                        .unwrap_or(url.len() - path_start - path_pos)];
                return host_and_path.to_string();
            }
        }
        return url.split('?').next().unwrap_or(url).to_string();
    }
    // Filesystem paths (absolute or relative)
    if let Some(m) = PATH_RE.find(cmd) {
        let path = m.as_str().trim().trim_end_matches('/');
        // Skip if it's just a flag like --include=*.py
        if !path.starts_with('-') && path.len() > 2 {
            return path.to_string();
        }
    }
    // Try: for commands like "cat file.txt" or "cat > file.txt", extract the filename
    let words: Vec<&str> = cmd.split_whitespace().collect();
    if words.len() >= 2 {
        // Skip flags (words starting with -)
        for w in &words[1..] {
            if *w == ">" || *w == ">>" {
                continue;
            }
            if w.starts_with('-') {
                continue;
            }
            if w.starts_with('|') || w.starts_with(';') {
                break;
            }
            // Looks like a filename or path (has a dot or slash, or no special chars)
            if (w.contains('.') || w.contains('/')) && !w.starts_with('{') && !w.starts_with('(') {
                return w.trim_end_matches('/').to_string();
            }
        }
    }
    // True last resort
    let first_word = words.first().unwrap_or(&"cmd");
    let cmd_name = first_word.rsplit('/').next().unwrap_or(first_word);
    format!("[{}]", cmd_name)
}

/// Extract target from step content. Handles common patterns:
/// - "[COMMAND] <cmd>" lines
/// - "command=<cmd>" in content
/// - Bare filesystem paths
fn extract_target_from_content(content: &str) -> Option<String> {
    // Look for [COMMAND] prefix (codex parser format)
    for line in content.lines().take(5) {
        let trimmed = line.trim();
        if let Some(cmd) = trimmed.strip_prefix("[COMMAND]") {
            let cmd = cmd.trim();
            if !cmd.is_empty() {
                return Some(extract_target_from_command(cmd));
            }
        }
        if let Some(cmd) = trimmed.strip_prefix("command=") {
            let cmd = cmd.trim();
            if !cmd.is_empty() {
                return Some(extract_target_from_command(cmd));
            }
        }
    }
    // Look for a filesystem path in first few lines
    for line in content.lines().take(3) {
        if let Some(m) = PATH_RE.find(line) {
            return Some(m.as_str().trim().trim_end_matches('/').to_string());
        }
    }
    None
}

/// Get display label from a full path (basename or last segment).
fn path_label(path: &str) -> String {
    let stripped = path.trim_end_matches('/');
    if let Some(pos) = stripped.rfind('/') {
        let base = &stripped[pos + 1..];
        if base.is_empty() {
            stripped.to_string()
        } else {
            base.to_string()
        }
    } else {
        stripped.to_string()
    }
}

/// Extract detail string describing a specific operation.
/// Falls back to content when args is empty.
fn extract_detail(
    args: &HashMap<String, String>,
    _sub_category: Option<&str>,
    content: &str,
) -> String {
    if let (Some(offset), Some(limit)) = (args.get("offset"), args.get("limit")) {
        if let (Ok(o), Ok(l)) = (offset.parse::<usize>(), limit.parse::<usize>()) {
            return format!("L{}-L{}", o, o + l);
        }
    }
    if let Some(path) = args.get("file_path") {
        return path.clone();
    }
    if let Some(cmd) = args.get("command") {
        return truncate(cmd, 80);
    }
    if let Some(rc) = args.get("rc") {
        return format!("rc={}", rc);
    }
    // Fallback: extract from content
    for line in content.lines().take(5) {
        let trimmed = line.trim();
        if let Some(cmd) = trimmed.strip_prefix("[COMMAND]") {
            return truncate(cmd.trim(), 80);
        }
        if let Some(cmd) = trimmed.strip_prefix("command=") {
            return truncate(cmd.trim(), 80);
        }
    }
    String::new()
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

/// Check if a string looks like a filesystem path.
fn is_path(s: &str) -> bool {
    s.starts_with('/') || s.starts_with("./") || s.starts_with("../")
}

/// Assign parent_id to each node based on longest-prefix path containment.
fn assign_parents(node_map: &mut HashMap<String, ActivityNode>) {
    // Collect (key, path, node_id) for all path-like nodes
    let path_nodes: Vec<(String, String, String)> = node_map
        .iter()
        .filter(|(_, node)| is_path(&node.primary_object))
        .map(|(key, node)| {
            (
                key.clone(),
                node.primary_object.trim_end_matches('/').to_string(),
                node.node_id.clone(),
            )
        })
        .collect();

    // For each node, find the longest prefix match
    for (key_i, path_i, _) in &path_nodes {
        let mut best_parent: Option<String> = None;
        let mut best_len = 0;

        for (key_j, path_j, node_id_j) in &path_nodes {
            if key_i == key_j {
                continue;
            }
            if path_j.len() >= path_i.len() {
                continue;
            }
            if path_i.starts_with(&format!("{}/", path_j)) {
                if path_j.len() > best_len {
                    best_len = path_j.len();
                    best_parent = Some(node_id_j.clone());
                }
            }
        }

        if let Some(parent_id) = best_parent {
            if let Some(node) = node_map.get_mut(key_i) {
                node.parent_id = Some(parent_id);
            }
        }
    }
}

/// Map GoalCategory to OpType.
fn goal_cat_to_op_type(cat: &GoalCategory) -> OpType {
    match cat {
        GoalCategory::Read => OpType::Read,
        GoalCategory::Write => OpType::Write,
        GoalCategory::Edit => OpType::Edit,
        GoalCategory::List => OpType::List,
        GoalCategory::Run => OpType::Run,
        GoalCategory::Other => OpType::Other,
    }
}

/// Build an ActivityGraph from a Trajectory.
pub fn build(trajectory: &Trajectory) -> ActivityGraph {
    let mut node_map: HashMap<String, ActivityNode> = HashMap::new();
    let mut operation_sequence: Vec<(String, usize)> = Vec::new();
    let mut call_index: usize = 0;

    for step in &trajectory.steps {
        for item in &step.items {
            if item.category != crate::models::ItemCategory::Action {
                continue;
            }

            // Skip tool results and JSON blobs (no meaningful target)
            if item.args.is_empty() {
                let c = item.content.trim_start();
                if c.starts_with('}')
                    || c.starts_with(']')
                    || c.starts_with('{')
                    || c.starts_with("Tool result:")
                    || c.contains("tool_result")
                    || c.contains("tool_use_id")
                {
                    continue;
                }
            }

            let goal_cat = classify_goal_category(item.sub_category.as_deref());
            let target_path =
                extract_target_path(&item.args, item.sub_category.as_deref(), &item.content);
            let node_key = format!("{}:{}", goal_cat.as_str(), target_path);

            if !node_map.contains_key(&node_key) {
                node_map.insert(
                    node_key.clone(),
                    ActivityNode {
                        node_id: format!("n{}", node_map.len()),
                        label: path_label(&target_path),
                        goal_category: goal_cat.clone(),
                        primary_object: target_path,
                        parent_id: None,
                        call_indices: Vec::new(),
                        operations: Vec::new(),
                        total_cost: Cost::default(),
                    },
                );
            }

            let node = node_map.get_mut(&node_key).unwrap();
            let detail = extract_detail(&item.args, item.sub_category.as_deref(), &item.content);
            let op = Operation {
                op_type: goal_cat_to_op_type(&goal_cat),
                detail,
                call_index,
            };
            let op_index = node.operations.len();
            node.operations.push(op);
            node.call_indices.push(call_index);
            node.total_cost = node.total_cost.add(&item.cost);
            operation_sequence.push((node.node_id.clone(), op_index));
            call_index += 1;
        }
    }

    assign_parents(&mut node_map);

    // Only create edges when the agent moves from one object to a DIFFERENT object.
    let mut edges: Vec<ActivityEdge> = Vec::new();
    for i in 1..operation_sequence.len() {
        let (src_node_id, src_op_idx) = &operation_sequence[i - 1];
        let (tgt_node_id, tgt_op_idx) = &operation_sequence[i];
        if src_node_id != tgt_node_id {
            edges.push(ActivityEdge {
                edge_type: "next".into(),
                source_id: src_node_id.clone(),
                source_operation_index: *src_op_idx,
                target_id: tgt_node_id.clone(),
                target_operation_index: *tgt_op_idx,
            });
        }
    }

    let nodes: Vec<ActivityNode> = node_map.into_values().collect();
    ActivityGraph { nodes, edges }
}
