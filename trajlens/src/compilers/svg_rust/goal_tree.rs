/// Goal Transition Tree SVG rendering.
///
/// Renders GoalTransitionTree as a hierarchical tree with:
/// - Nodes positioned by level (root at top, leaves at bottom)
/// - Sub edges (parent→child), Next edges (sibling→sibling), Backtrack edges
/// - Synthetic success backtrack edges for last node in each plan
/// - Hierarchical ID labels (1, 1.1, 1.2, 1.2.1, etc.)
/// - Color coding by goal status (done/failed/abandoned/in-progress)
use std::collections::HashMap;

use crate::config::get_config;
use crate::models::GoalTransitionTree;

use super::{escape_xml, wrap_text, SVGCompiler};

impl SVGCompiler {
    /// Validate Goal Tree IGR structure.
    pub(super) fn validate_goal_tree(&self, tree: &GoalTransitionTree) -> Result<(), String> {
        let mut parent_map: HashMap<String, Option<String>> = HashMap::new();
        let root_id = if !tree.root_id.is_empty() {
            &tree.root_id
        } else {
            &tree.nodes[0].node_id
        };
        parent_map.insert(root_id.clone(), None);

        for edge in &tree.edges {
            match edge.edge_type {
                crate::models::GoalEdgeType::Sub => {
                    parent_map.insert(edge.target_id.clone(), Some(edge.source_id.clone()));
                }
                crate::models::GoalEdgeType::Next => {
                    if let Some(source_parent) = parent_map.get(&edge.source_id) {
                        parent_map.insert(edge.target_id.clone(), source_parent.clone());
                    }
                }
                _ => {}
            }
        }

        let mut outgoing_counts: HashMap<String, usize> = HashMap::new();
        for node in &tree.nodes {
            outgoing_counts.insert(node.node_id.clone(), 0);
        }
        for edge in &tree.edges {
            *outgoing_counts.entry(edge.source_id.clone()).or_insert(0) += 1;
        }

        for node in &tree.nodes {
            let count = outgoing_counts.get(&node.node_id).unwrap_or(&0);
            if *count == 0 && node.node_id != *root_id {
                return Err(format!("Node {} has no outgoing edges", node.node_id));
            } else if *count > 2 {
                return Err(format!(
                    "Node {} has {} outgoing edges (max 2: Sub+Next)",
                    node.node_id, count
                ));
            }
        }

        for edge in &tree.edges {
            match edge.edge_type {
                crate::models::GoalEdgeType::Sub => {
                    if parent_map.get(&edge.target_id) != Some(&Some(edge.source_id.clone())) {
                        return Err(format!(
                            "Invalid Sub edge: {} → {} (target's parent should be source)",
                            edge.source_id, edge.target_id
                        ));
                    }
                }
                crate::models::GoalEdgeType::Next => {
                    let source_parent = parent_map.get(&edge.source_id);
                    let target_parent = parent_map.get(&edge.target_id);
                    if source_parent != target_parent {
                        return Err(format!(
                            "Invalid Next edge: {} → {} (should be siblings with same parent)",
                            edge.source_id, edge.target_id
                        ));
                    }
                }
                crate::models::GoalEdgeType::Backtrack => {
                    let source_parent = parent_map.get(&edge.source_id).and_then(|p| p.as_ref());
                    if source_parent != Some(&edge.target_id) {
                        return Err(format!(
                            "Invalid Backtrack edge: {} → {} (source's parent should be target)",
                            edge.source_id, edge.target_id
                        ));
                    }

                    let has_next_out = tree.edges.iter().any(|e| {
                        e.source_id == edge.source_id
                            && e.edge_type == crate::models::GoalEdgeType::Next
                    });
                    if has_next_out {
                        return Err(format!(
                            "Node {} has Backtrack edge but also has outgoing Next edge",
                            edge.source_id
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    /// Render Goal Transition Tree as a hierarchical tree.
    pub(super) fn render_goal_tree(&self, tree: &GoalTransitionTree) -> String {
        if let Err(err) = self.validate_goal_tree(tree) {
            eprintln!("Warning: Goal tree validation failed: {}", err);
        }

        let config = get_config();
        let gt_config = &config.rendering.svg.goal_tree;
        let gt_colors = &gt_config.colors;

        let node_width = gt_config.node_width;
        let level_spacing = gt_config.level_spacing;
        let node_spacing = gt_config.node_spacing;

        // Compute uniform node height: header + label lines + result lines + footer.
        let max_content_lines = tree
            .nodes
            .iter()
            .map(|n| {
                let label_lines = wrap_text(&n.label, gt_config.text_wrap_max_chars).len();
                let result_lines = if n.result.is_empty() {
                    0
                } else {
                    wrap_text(&n.result, gt_config.text_wrap_max_chars + 5)
                        .len()
                        .min(2)
                };
                label_lines + result_lines
            })
            .max()
            .unwrap_or(2);
        // header(13) + gap(3) + content lines + gap(4) + footer(12)
        let node_height = (32.0 + max_content_lines as f64 * gt_config.line_height + 16.0)
            .max(gt_config.node_height)
            .min(gt_config.node_height * 2.5);

        // Assign levels via BFS: Sub edges → level+1, Next edges → same level
        let mut hierarchy: HashMap<usize, Vec<&crate::models::GoalNode>> = HashMap::new();
        let mut node_levels: HashMap<String, usize> = HashMap::new();
        let root_id = if !tree.root_id.is_empty() {
            &tree.root_id
        } else {
            &tree.nodes[0].node_id
        };
        node_levels.insert(root_id.clone(), 0);

        let mut queue: Vec<(String, usize)> = vec![(root_id.clone(), 0)];
        let mut visited = std::collections::HashSet::new();

        while !queue.is_empty() {
            let (node_id, level) = queue.remove(0);
            if !visited.insert(node_id.clone()) {
                continue;
            }

            for edge in &tree.edges {
                if edge.source_id == node_id {
                    let target_level: usize = match edge.edge_type {
                        crate::models::GoalEdgeType::Sub => level + 1,
                        crate::models::GoalEdgeType::Next => level,
                        crate::models::GoalEdgeType::Backtrack => level.saturating_sub(1),
                    };

                    if !node_levels.contains_key(&edge.target_id) {
                        node_levels.insert(edge.target_id.clone(), target_level);
                        queue.push((edge.target_id.clone(), target_level));
                    }
                }
            }
        }

        for node in &tree.nodes {
            let level = *node_levels.get(&node.node_id).unwrap_or(&0);
            hierarchy.entry(level).or_insert_with(Vec::new).push(node);
        }

        // Build parent map from edges (Sub takes priority over Next)
        let mut parent_ids: HashMap<String, Option<String>> = HashMap::new();
        parent_ids.insert(root_id.clone(), None);

        let mut changed = true;
        while changed {
            changed = false;
            for edge in &tree.edges {
                if edge.edge_type == crate::models::GoalEdgeType::Sub {
                    let existing = parent_ids.get(&edge.target_id);
                    if existing.is_none() || existing == Some(&None) {
                        parent_ids.insert(edge.target_id.clone(), Some(edge.source_id.clone()));
                        changed = true;
                    }
                }
            }
            for edge in &tree.edges {
                if edge.edge_type == crate::models::GoalEdgeType::Next {
                    if let Some(source_parent) = parent_ids.get(&edge.source_id).cloned() {
                        if !parent_ids.contains_key(&edge.target_id) {
                            parent_ids.insert(edge.target_id.clone(), source_parent);
                            changed = true;
                        }
                    }
                }
            }
        }

        // Node IDs are already hierarchical (assigned by the builder: 1, 1.1, 1.2, etc.)
        // Use them directly for display and sorting.
        let hierarchical_ids: HashMap<String, String> = tree
            .nodes
            .iter()
            .map(|n| (n.node_id.clone(), n.node_id.clone()))
            .collect();

        // Determine layout direction: if any level has >5 nodes, use horizontal (LR)
        // so the tree grows left-to-right instead of top-to-bottom.
        let max_level_width = hierarchy.values().map(|v| v.len()).max().unwrap_or(0);
        let use_horizontal = max_level_width > 5;

        // Position nodes sorted by hierarchical ID within each level
        let mut positions: HashMap<String, (f64, f64)> = HashMap::new();

        for (level, nodes) in &mut hierarchy {
            nodes.sort_by(|a, b| {
                let hid_a = hierarchical_ids
                    .get(&a.node_id)
                    .cloned()
                    .unwrap_or_default();
                let hid_b = hierarchical_ids
                    .get(&b.node_id)
                    .cloned()
                    .unwrap_or_default();
                let parts_a: Vec<usize> = hid_a.split('.').filter_map(|s| s.parse().ok()).collect();
                let parts_b: Vec<usize> = hid_b.split('.').filter_map(|s| s.parse().ok()).collect();
                parts_a.cmp(&parts_b)
            });

            if use_horizontal {
                // LR layout: level → x-axis (left-to-right), index → y-axis (top-to-bottom)
                let x = (*level as f64) * (node_width + level_spacing) + 50.0;
                let total_height =
                    nodes.len() as f64 * (node_height + node_spacing) - node_spacing;
                let start_y = -total_height / 2.0 + node_height / 2.0;

                for (i, node) in nodes.iter().enumerate() {
                    let y = start_y + (i as f64) * (node_height + node_spacing);
                    positions.insert(node.node_id.clone(), (x, y));
                }
            } else {
                // TB layout: level → y-axis (top-to-bottom), index → x-axis (left-to-right)
                let y = (*level as f64) * level_spacing + 50.0;
                let total_width =
                    nodes.len() as f64 * (node_width + node_spacing) - node_spacing;
                let start_x = -total_width / 2.0 + node_width / 2.0;

                for (i, node) in nodes.iter().enumerate() {
                    let x = start_x + (i as f64) * (node_width + node_spacing);
                    positions.insert(node.node_id.clone(), (x, y));
                }
            }
        }

        // Calculate canvas bounds
        let mut min_x = f64::MAX;
        let mut max_x = f64::MIN;
        let mut min_y = f64::MAX;
        let mut max_y = f64::MIN;

        for &(x, y) in positions.values() {
            min_x = min_x.min(x - node_width / 2.0);
            max_x = max_x.max(x + node_width / 2.0);
            min_y = min_y.min(y - node_height / 2.0);
            max_y = max_y.max(y + node_height / 2.0);
        }

        let margin = gt_config.margin;
        let legend_height = gt_config.legend_height;
        let canvas_width = (max_x - min_x + 2.0 * margin).max(800.0);
        let canvas_height = (max_y - min_y + 2.0 * margin + legend_height).max(600.0);

        let offset_x = margin - min_x;
        let offset_y = margin - min_y;
        for pos in positions.values_mut() {
            pos.0 += offset_x;
            pos.1 += offset_y;
        }

        let pos_map = positions;

        // Build SVG
        let mut svg = String::new();
        svg.push_str(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\">\n\
<defs>\n\
<marker id=\"arrowhead\" markerWidth=\"10\" markerHeight=\"10\" refX=\"9\" refY=\"3\" orient=\"auto\">\n\
<polygon points=\"0 0, 10 3, 0 6\" fill=\"{}\" />\n\
</marker>\n\
</defs>\n",
            canvas_width, canvas_height, canvas_width, canvas_height, gt_colors.node_border
        ));

        // Edges (drawn before nodes so arrows aren't covered)
        for edge in &tree.edges {
            if let (Some(&(sx, sy)), Some(&(tx, ty))) =
                (pos_map.get(&edge.source_id), pos_map.get(&edge.target_id))
            {
                let (color, dashed) = match edge.edge_type {
                    crate::models::GoalEdgeType::Next => (gt_colors.edge_next.as_str(), false),
                    crate::models::GoalEdgeType::Backtrack => {
                        let source_node = tree.nodes.iter().find(|n| n.node_id == edge.source_id);
                        let is_success = source_node
                            .map(|n| n.status == crate::models::GoalStatus::Done)
                            .unwrap_or(false);
                        if is_success {
                            (gt_colors.edge_success.as_str(), true)
                        } else {
                            (gt_colors.edge_backtrack.as_str(), true)
                        }
                    }
                    crate::models::GoalEdgeType::Sub => (gt_colors.edge_sub.as_str(), false),
                };

                // pos_map stores (center_x, top_y) for each node.
                // Rect is drawn at (x - w/2, y) with size (w, h).
                let tip_offset = 1.0;
                let (x1, y1, x2, y2) = if use_horizontal {
                    // LR layout: levels go left→right, siblings go top→bottom
                    match edge.edge_type {
                        crate::models::GoalEdgeType::Sub => (
                            // Right edge of source → left edge of target
                            sx + node_width / 2.0,
                            sy + node_height / 2.0,
                            tx - node_width / 2.0 - tip_offset,
                            ty + node_height / 2.0,
                        ),
                        crate::models::GoalEdgeType::Next => (
                            // Bottom of source → top of target (siblings stacked vertically)
                            sx,
                            sy + node_height,
                            tx,
                            ty - tip_offset,
                        ),
                        crate::models::GoalEdgeType::Backtrack => (
                            // Left edge of source → right edge of target
                            sx - node_width / 2.0,
                            sy + node_height / 2.0,
                            tx + node_width / 2.0 + tip_offset,
                            ty + node_height / 2.0,
                        ),
                    }
                } else {
                    // TB layout: levels go top→bottom, siblings go left→right
                    match edge.edge_type {
                        crate::models::GoalEdgeType::Sub => (
                            // Bottom of source → top of target
                            sx,
                            sy + node_height,
                            tx,
                            ty - tip_offset,
                        ),
                        crate::models::GoalEdgeType::Next => (
                            // Right edge of source → left edge of target
                            sx + node_width / 2.0,
                            sy + node_height / 2.0,
                            tx - node_width / 2.0 - tip_offset,
                            ty + node_height / 2.0,
                        ),
                        crate::models::GoalEdgeType::Backtrack => (
                            // Top of source → bottom of target
                            sx,
                            sy,
                            tx,
                            ty + node_height + tip_offset,
                        ),
                    }
                };

                let dash_attr = if dashed {
                    " stroke-dasharray=\"6,4\""
                } else {
                    ""
                };

                svg.push_str(&format!(
                    "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{} marker-end=\"url(#arrowhead)\"/>\n",
                    x1, y1, x2, y2, color, gt_config.edge_stroke_width, dash_attr
                ));
            }
        }

        // Nodes
        for node in &tree.nodes {
            if let Some(&(x, y)) = pos_map.get(&node.node_id) {
                let fill_color = match node.status {
                    crate::models::GoalStatus::Done => gt_colors.status_done.as_str(),
                    crate::models::GoalStatus::Failed => gt_colors.status_failed.as_str(),
                    crate::models::GoalStatus::Abandoned => gt_colors.status_abandoned.as_str(),
                    crate::models::GoalStatus::Partial => gt_colors.status_partial.as_str(),
                };

                // Wrap node in <g> with <title> for hover tooltip showing details
                if !node.details.is_empty() {
                    svg.push_str(&format!(
                        "  <g><title>{}</title>\n",
                        escape_xml(&node.details)
                    ));
                }

                svg.push_str(&format!(
                    "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" rx=\"{}\"/>\n",
                    x - node_width / 2.0, y, node_width, node_height, fill_color,
                    gt_colors.node_border, gt_config.node_stroke_width, gt_config.node_corner_radius
                ));

                // Header: ID + category tag
                let hid = hierarchical_ids
                    .get(&node.node_id)
                    .map(|s| s.as_str())
                    .unwrap_or(&node.node_id);
                let cat_tag = match node.goal_type {
                    crate::models::GoalType::Explore => "EXPLORE",
                    crate::models::GoalType::Think => "THINK",
                    crate::models::GoalType::Act => "ACT",
                };
                svg.push_str(&format!(
                    "  <text x=\"{}\" y=\"{}\" fill=\"{}\" font-size=\"{}\" font-weight=\"bold\" text-anchor=\"middle\">{} [{}]</text>\n",
                    x, y + 13.0, gt_colors.text_secondary, gt_config.font_size_id, escape_xml(hid), cat_tag
                ));

                // Goal label (wrapped)
                let wrapped_lines = wrap_text(&node.label, gt_config.text_wrap_max_chars);
                let start_y = y + 26.0;
                for (i, line) in wrapped_lines.iter().enumerate() {
                    let text_y = start_y + (i as f64 * gt_config.line_height);
                    svg.push_str(&format!(
                        "  <text x=\"{}\" y=\"{}\" fill=\"{}\" font-size=\"{}\" font-weight=\"bold\" text-anchor=\"middle\">{}</text>\n",
                        x, text_y, gt_colors.text_primary, gt_config.font_size_label, escape_xml(line)
                    ));
                }

                // Result line (below label, italic, smaller)
                let result_y = start_y + (wrapped_lines.len() as f64 * gt_config.line_height) + 4.0;
                if !node.result.is_empty() {
                    let result_lines = wrap_text(&node.result, gt_config.text_wrap_max_chars + 5);
                    for (i, line) in result_lines.iter().take(2).enumerate() {
                        svg.push_str(&format!(
                            "  <text x=\"{}\" y=\"{}\" fill=\"{}\" font-size=\"{}\" font-style=\"italic\" text-anchor=\"middle\">{}</text>\n",
                            x, result_y + (i as f64 * gt_config.line_height),
                            gt_colors.text_secondary, gt_config.font_size_detail, escape_xml(line)
                        ));
                    }
                }

                // Footer: step range
                svg.push_str(&format!(
                    "  <text x=\"{}\" y=\"{}\" fill=\"{}\" font-size=\"{}\" text-anchor=\"middle\">Steps: {}-{}</text>\n",
                    x, y + node_height - 6.0, gt_colors.text_secondary,
                    gt_config.font_size_detail, node.step_range.0, node.step_range.1
                ));

                if !node.details.is_empty() {
                    svg.push_str("  </g>\n");
                }
            }
        }

        self.draw_goal_tree_legend(&mut svg, canvas_height, gt_colors);

        svg.push_str("</svg>\n");
        svg
    }

    fn draw_goal_tree_legend(
        &self,
        svg: &mut String,
        canvas_height: f64,
        colors: &crate::config::GoalTreeColors,
    ) {
        let config = get_config();
        let gt_config = &config.rendering.svg.goal_tree;

        let legend_x = 10.0;
        let legend_y = canvas_height - 150.0;

        svg.push_str(&format!(
            "  <rect x=\"{}\" y=\"{}\" width=\"200\" height=\"140\" fill=\"white\" stroke=\"{}\" stroke-width=\"1\" rx=\"5\" opacity=\"0.95\"/>\n",
            legend_x, legend_y, colors.legend_border
        ));

        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" fill=\"{}\" font-size=\"{}\" font-weight=\"bold\">Legend</text>\n",
            legend_x + 10.0, legend_y + 20.0, colors.text_primary, gt_config.font_size_legend_title
        ));

        let statuses = [
            ("Done", colors.status_done.as_str()),
            ("Partial", colors.status_partial.as_str()),
            ("Failed", colors.status_failed.as_str()),
            ("Abandoned", colors.status_abandoned.as_str()),
        ];

        for (i, (label, color)) in statuses.iter().enumerate() {
            let y = legend_y + 40.0 + (i as f64 * 25.0);
            svg.push_str(&format!(
                "  <rect x=\"{}\" y=\"{}\" width=\"20\" height=\"15\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\" rx=\"2\"/>\n",
                legend_x + 10.0, y - 10.0, color, colors.node_border
            ));
            svg.push_str(&format!(
                "  <text x=\"{}\" y=\"{}\" fill=\"{}\" font-size=\"{}\">{}</text>\n",
                legend_x + 35.0,
                y,
                colors.text_primary,
                gt_config.font_size_label,
                label
            ));
        }
    }
}
