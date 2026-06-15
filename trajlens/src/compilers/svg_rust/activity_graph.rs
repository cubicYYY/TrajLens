/// Activity Graph SVG rendering.
///
/// Renders ActivityGraph as a hierarchical node-link diagram using Sugiyama layout.
/// Nodes are colored by goal category; operations are listed as table rows.
use std::collections::HashMap;

use crate::compilers::layout::{sugiyama_layout, LayoutConfig, LayoutEdge, LayoutNode};
use crate::config::get_config;
use crate::models::{ActivityGraph, ActivityNode, GoalCategory};

use super::{escape_xml, truncate, wrap_text, SVGCompiler};

impl SVGCompiler {
    pub(super) fn render_activity_graph(&self, graph: &ActivityGraph) -> String {
        let config = get_config();
        let svg_config = &config.rendering.svg;

        let mut node_dims = HashMap::new();
        for node in &graph.nodes {
            let width = svg_config.node_width;
            let displayed_ops = node.operations.len().min(11); // 10 shown + 1 "...and X more" line
            let height = svg_config.node_height_base
                + (displayed_ops as f64 * svg_config.node_height_per_operation);
            node_dims.insert(node.node_id.clone(), (width, height));
        }

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

        let layout_config = LayoutConfig {
            x_spacing: config.rendering.layout.x_spacing,
            y_spacing: config.rendering.layout.y_spacing,
        };

        let positioned = sugiyama_layout(&layout_nodes, &layout_edges, &layout_config);

        let (canvas_w, canvas_h) = if positioned.is_empty() {
            (400.0, 300.0)
        } else {
            let max_x = positioned
                .iter()
                .map(|n| n.x + n.width / 2.0)
                .fold(0.0f64, f64::max);
            let max_y = positioned
                .iter()
                .map(|n| n.y + n.height / 2.0)
                .fold(0.0f64, f64::max);
            (max_x + 20.0, max_y + 20.0)
        };

        let mut svg = String::new();
        self.svg_header(&mut svg, canvas_w, canvas_h);

        let pos_map: HashMap<_, _> = positioned
            .iter()
            .map(|n| (n.id.clone(), (n.x, n.y)))
            .collect();

        // Draw edges between specific operations (not node centers)
        let config = get_config();
        let header_h = 30.0;
        let row_height = config.rendering.svg.node_height_per_operation;

        for edge in &graph.edges {
            if let (Some(&src_pos), Some(&tgt_pos)) =
                (pos_map.get(&edge.source_id), pos_map.get(&edge.target_id))
            {
                let src_dim = node_dims[&edge.source_id];
                let tgt_dim = node_dims[&edge.target_id];

                // Compute Y offset for the specific operation within each node
                let src_op_y_offset =
                    header_h + (edge.source_operation_index.min(10) as f64 + 0.5) * row_height;
                let tgt_op_y_offset =
                    header_h + (edge.target_operation_index.min(10) as f64 + 0.5) * row_height;

                let sx = src_pos.0 + src_dim.0 / 2.0; // right edge of source
                let sy = src_pos.1 - src_dim.1 / 2.0 + src_op_y_offset;
                let tx = tgt_pos.0 - tgt_dim.0 / 2.0; // left edge of target
                let ty = tgt_pos.1 - tgt_dim.1 / 2.0 + tgt_op_y_offset;

                svg.push_str(&format!(
                    "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#666\" stroke-width=\"1\" marker-end=\"url(#arrowhead)\"/>\n",
                    sx, sy, tx, ty
                ));
            }
        }

        let node_map: HashMap<_, _> = graph.nodes.iter().map(|n| (n.node_id.clone(), n)).collect();

        for pnode in &positioned {
            if let Some(node) = node_map.get(&pnode.id) {
                self.draw_activity_node(
                    &mut svg,
                    node,
                    pnode.x,
                    pnode.y,
                    pnode.width,
                    pnode.height,
                );
            }
        }

        self.draw_activity_graph_legend(&mut svg, canvas_h);
        self.svg_footer(&mut svg);
        svg
    }

    fn draw_activity_node(
        &self,
        svg: &mut String,
        node: &ActivityNode,
        cx: f64,
        cy: f64,
        w: f64,
        h: f64,
    ) {
        let config = get_config();
        let colors = &config.rendering.svg.colors;

        let x = cx - w / 2.0;
        let y = cy - h / 2.0;

        let fill = match node.goal_category {
            GoalCategory::Read => colors.read.as_str(),
            GoalCategory::Write => colors.write.as_str(),
            GoalCategory::Edit => colors.edit.as_str(),
            GoalCategory::List => colors.list.as_str(),
            GoalCategory::Run => colors.run.as_str(),
            GoalCategory::Other => colors.other.as_str(),
        };

        self.draw_rect(
            svg,
            x,
            y,
            w,
            h,
            fill,
            &colors.border,
            &config.rendering.svg.stroke_width.to_string(),
        );

        let header_h = 30.0;
        self.draw_rect(svg, x, y, w, header_h, "#e0e0e0", "none", "0");

        let label_lines = wrap_text(
            &node.label,
            config.rendering.svg.text_wrap_max_chars as usize,
        );
        let mut text_y = y + config.rendering.svg.font_size_label + 6.0;
        for line in label_lines.iter().take(2) {
            self.draw_text(
                svg,
                cx,
                text_y,
                line,
                config.rendering.svg.font_size_label as i32,
                "middle",
                &colors.text,
            );
            text_y += config.rendering.svg.font_size_label + 2.0;
        }

        let mut row_y = y + header_h;
        let row_height = config.rendering.svg.node_height_per_operation;
        for (idx, op) in node.operations.iter().enumerate() {
            if idx >= 10 {
                self.draw_text(
                    svg,
                    x + 5.0,
                    row_y + config.rendering.svg.font_size_detail,
                    &format!("...and {} more", node.operations.len() - 10),
                    config.rendering.svg.font_size_detail as i32,
                    "start",
                    "#666",
                );
                break;
            }

            let op_text = format!(
                "#{} {} {}",
                op.call_index,
                format!("{:?}", op.op_type).to_lowercase(),
                truncate(&op.detail, 18)
            );
            self.draw_text(
                svg,
                x + 5.0,
                row_y + config.rendering.svg.font_size_detail,
                &op_text,
                config.rendering.svg.font_size_detail as i32,
                "start",
                "#333",
            );
            row_y += row_height;
        }
    }

    fn draw_activity_graph_legend(&self, svg: &mut String, canvas_height: f64) {
        let config = get_config();
        let colors = &config.rendering.svg.colors;
        let legend_x = 10.0;
        let legend_y = canvas_height - 180.0;

        svg.push_str(&format!(
            "  <rect x=\"{}\" y=\"{}\" width=\"180\" height=\"170\" fill=\"white\" stroke=\"#999\" stroke-width=\"1\" rx=\"5\" opacity=\"0.95\"/>\n",
            legend_x, legend_y
        ));

        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" fill=\"#000\" font-size=\"12\" font-weight=\"bold\">Legend</text>\n",
            legend_x + 10.0, legend_y + 20.0
        ));

        let categories = [
            ("Read", colors.read.as_str()),
            ("Write", colors.write.as_str()),
            ("Edit", colors.edit.as_str()),
            ("Run", colors.run.as_str()),
            ("List", colors.list.as_str()),
            ("Other", colors.other.as_str()),
        ];

        for (i, (label, color)) in categories.iter().enumerate() {
            let y = legend_y + 40.0 + (i as f64 * 22.0);
            svg.push_str(&format!(
                "  <rect x=\"{}\" y=\"{}\" width=\"20\" height=\"15\" fill=\"{}\" stroke=\"#999\" stroke-width=\"1\" rx=\"2\"/>\n",
                legend_x + 10.0, y - 10.0, color
            ));
            svg.push_str(&format!(
                "  <text x=\"{}\" y=\"{}\" fill=\"#000\" font-size=\"10\">{}</text>\n",
                legend_x + 35.0,
                y,
                escape_xml(label)
            ));
        }
    }
}
