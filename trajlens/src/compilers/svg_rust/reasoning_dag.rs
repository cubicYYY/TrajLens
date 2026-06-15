/// Reasoning Artifact DAG SVG rendering.
///
/// Renders ReasoningArtifactDAG as a directed graph using Sugiyama layout.
/// Nodes colored by type+status: ground_truth (green), hypothesis-verified (green),
/// hypothesis-self-falsed (red/orange), insight (blue). Edges have per-type arrowheads.
use std::collections::HashMap;

use crate::compilers::layout::{sugiyama_layout, LayoutConfig, LayoutEdge, LayoutNode};
use crate::config::get_config;
use crate::models::{InsightStatus, ReasoningArtifactDAG, ReasoningNodeType};

use super::{escape_xml, wrap_text, SVGCompiler};

const COLOR_GROUND_TRUTH: &str = "#c8e6c9";
const COLOR_HYPOTHESIS_VERIFIED: &str = "#a5d6a7";
const COLOR_HYPOTHESIS_FALSED: &str = "#ffcdd2";
const COLOR_HYPOTHESIS_UNVERIFIED: &str = "#fff9c4";
const COLOR_INSIGHT: &str = "#bbdefb";
const COLOR_EDGE_INFERS: &str = "#1565c0";
const COLOR_EDGE_CONTRADICTS: &str = "#c62828";
const COLOR_EDGE_SUPERSEDES: &str = "#ff8f00";
const COLOR_BORDER_DEFAULT: &str = "#424242";
const COLOR_BORDER_FALSED: &str = "#c62828";
const COLOR_BORDER_VERIFIED: &str = "#2e7d32";

impl SVGCompiler {
    pub(super) fn render_reasoning_dag(&self, dag: &ReasoningArtifactDAG) -> String {
        let config = get_config();
        let rd_config = &config.rendering.svg.reasoning_dag;

        let node_width = rd_config.node_width;
        let node_height = rd_config.node_height;

        let layout_nodes: Vec<_> = dag
            .nodes
            .iter()
            .map(|n| LayoutNode {
                id: n.node_id.clone(),
                width: node_width,
                height: node_height,
            })
            .collect();

        let mut layout_edges = Vec::new();
        for edge in &dag.edges {
            for source_id in &edge.source_ids {
                layout_edges.push(LayoutEdge {
                    source: source_id.clone(),
                    target: edge.target_id.clone(),
                });
            }
        }

        let layout_config = LayoutConfig {
            x_spacing: config.rendering.layout.x_spacing,
            y_spacing: config.rendering.layout.y_spacing,
        };

        let positioned = sugiyama_layout(&layout_nodes, &layout_edges, &layout_config);

        let pos_map: HashMap<String, &crate::compilers::layout::PositionedNode> =
            positioned.iter().map(|p| (p.id.clone(), p)).collect();

        let mut min_x = f64::MAX;
        let mut min_y = f64::MAX;
        let mut max_x = f64::MIN;
        let mut max_y = f64::MIN;

        // Account for outer frames (trajectory frame gap=4, divergence frame gap=8)
        let frame_pad = 12.0;
        for pos in &positioned {
            min_x = min_x.min(pos.x - frame_pad);
            min_y = min_y.min(pos.y - frame_pad);
            max_x = max_x.max(pos.x + node_width + frame_pad);
            max_y = max_y.max(pos.y + node_height + frame_pad);
        }

        // Reserve a fixed footprint for the legend so it never overlaps nodes.
        // The legend is drawn at the bottom-left after all nodes; we extend
        // canvas_height by LEGEND_HEIGHT (+ separator gap) to make room.
        // Width buffer ensures the legend (270 wide) fits even in narrow graphs.
        const LEGEND_WIDTH: f64 = 270.0;
        const LEGEND_HEIGHT: f64 = 330.0;
        const LEGEND_GAP: f64 = 40.0;

        let nodes_width = max_x - min_x;
        let nodes_height = max_y - min_y;
        let canvas_width = (nodes_width + 2.0 * self.margin)
            .max(LEGEND_WIDTH + 2.0 * self.margin)
            .max(800.0);
        let canvas_height =
            (nodes_height + LEGEND_HEIGHT + LEGEND_GAP + 2.0 * self.margin).max(600.0);

        let mut svg = String::new();
        svg.push_str(&format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\">\n\
  <defs>\n\
    <marker id=\"arrow-infers\" markerWidth=\"10\" markerHeight=\"10\" refX=\"9\" refY=\"3\" orient=\"auto\">\n\
      <polygon points=\"0 0, 10 3, 0 6\" fill=\"{}\" />\n\
    </marker>\n\
    <marker id=\"arrow-contradicts\" markerWidth=\"10\" markerHeight=\"10\" refX=\"9\" refY=\"3\" orient=\"auto\">\n\
      <polygon points=\"0 0, 10 3, 0 6\" fill=\"{}\" />\n\
    </marker>\n\
    <marker id=\"arrow-supersedes\" markerWidth=\"10\" markerHeight=\"10\" refX=\"9\" refY=\"3\" orient=\"auto\">\n\
      <polygon points=\"0 0, 10 3, 0 6\" fill=\"{}\" />\n\
    </marker>\n\
  </defs>\n",
            canvas_width,
            canvas_height,
            min_x - self.margin,
            min_y - self.margin,
            canvas_width,
            canvas_height,
            COLOR_EDGE_INFERS,
            COLOR_EDGE_CONTRADICTS,
            COLOR_EDGE_SUPERSEDES,
        ));

        // Draw edges with arrowheads offset to node border.
        // Multi-source edges (N-to-1) use a junction point: lines from each source
        // converge to the junction, then a single arrow goes from junction to target.
        for edge in &dag.edges {
            let (color, marker, dasharray) = match edge.edge_type {
                crate::models::ReasoningEdgeType::Infers => (COLOR_EDGE_INFERS, "arrow-infers", ""),
                crate::models::ReasoningEdgeType::Contradicts => {
                    (COLOR_EDGE_CONTRADICTS, "arrow-contradicts", "6,4")
                }
                crate::models::ReasoningEdgeType::Supersedes => {
                    (COLOR_EDGE_SUPERSEDES, "arrow-supersedes", "6,4")
                }
            };

            let dash_attr = if dasharray.is_empty() {
                String::new()
            } else {
                format!(" stroke-dasharray=\"{}\"", dasharray)
            };

            let tpos = match pos_map.get(&edge.target_id) {
                Some(p) => p,
                None => continue,
            };
            let tx = tpos.x + node_width / 2.0;
            let ty = tpos.y;

            // Collect valid source positions
            let sources: Vec<(f64, f64)> = edge
                .source_ids
                .iter()
                .filter_map(|sid| pos_map.get(sid))
                .map(|spos| (spos.x + node_width / 2.0, spos.y + node_height))
                .collect();

            if sources.is_empty() {
                continue;
            }

            if sources.len() == 1 {
                // Simple 1-to-1 edge
                let (sx, sy) = sources[0];
                svg.push_str(&format!(
                    "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\" marker-end=\"url(#{})\"{}/>",
                    sx, sy, tx, ty, color, rd_config.edge_stroke_width, marker, dash_attr
                ));
                svg.push('\n');
            } else {
                // N-to-1 edge: compute junction point (average x of sources, midway y)
                let avg_x: f64 = sources.iter().map(|(x, _)| x).sum::<f64>() / sources.len() as f64;
                let max_sy = sources.iter().map(|(_, y)| *y).fold(f64::MIN, f64::max);
                let jy = (max_sy + ty) / 2.0;
                let jx = (avg_x + tx) / 2.0;

                // Draw lines from each source to junction
                for (sx, sy) in &sources {
                    svg.push_str(&format!(
                        "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\"{}/>",
                        sx, sy, jx, jy, color, rd_config.edge_stroke_width, dash_attr
                    ));
                    svg.push('\n');
                }

                // Draw junction dot
                svg.push_str(&format!(
                    "  <circle cx=\"{}\" cy=\"{}\" r=\"4\" fill=\"{}\" />",
                    jx, jy, color
                ));
                svg.push('\n');

                // Draw line from junction to target with arrowhead
                svg.push_str(&format!(
                    "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"{}\" marker-end=\"url(#{})\"{}/>",
                    jx, jy, tx, ty, color, rd_config.edge_stroke_width, marker, dash_attr
                ));
                svg.push('\n');
            }
        }

        // Draw nodes
        for node in &dag.nodes {
            if let Some(pos) = pos_map.get(&node.node_id) {
                let (fill_color, border_color) = node_colors(node);

                // Divergence node: detected by "DIVERGE_" prefix in node_id
                let is_divergence = node.node_id.starts_with("DIVERGE_");
                if is_divergence {
                    // Outer gold glow
                    let gap = 8.0;
                    svg.push_str(&format!(
                        "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"#ff6f00\" stroke-width=\"4\" rx=\"{}\" />\n",
                        pos.x - gap, pos.y - gap, node_width + 2.0 * gap, node_height + 2.0 * gap,
                        rd_config.node_corner_radius + 4.0
                    ));
                    // Warning triangle icon top-left
                    let tx = pos.x - 4.0;
                    let ty = pos.y - 4.0;
                    svg.push_str(&format!(
                        "  <polygon points=\"{},{} {},{} {},{}\" fill=\"#ff6f00\" stroke=\"#fff\" stroke-width=\"1\"/>\n",
                        tx, ty - 12.0, tx - 7.0, ty, tx + 7.0, ty
                    ));
                    svg.push_str(&format!(
                        "  <text x=\"{}\" y=\"{}\" fill=\"#fff\" font-size=\"8\" font-weight=\"bold\" text-anchor=\"middle\">!</text>\n",
                        tx, ty - 3.0
                    ));
                }

                // Outer trajectory-source frame (with gap)
                let frame_color = trajectory_frame_color(&node.node_id);
                if let Some(fc) = frame_color {
                    let gap = 4.0;
                    svg.push_str(&format!(
                        "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"none\" stroke=\"{}\" stroke-width=\"2.5\" rx=\"{}\" stroke-dasharray=\"8,3\"/>\n",
                        pos.x - gap, pos.y - gap, node_width + 2.0 * gap, node_height + 2.0 * gap,
                        fc, rd_config.node_corner_radius + 2.0
                    ));
                }

                svg.push_str(&format!(
                    "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\" rx=\"{}\"/>\n",
                    pos.x, pos.y, node_width, node_height, fill_color,
                    border_color, rd_config.node_stroke_width, rd_config.node_corner_radius
                ));

                // Type badge + node ID
                let badge = node_badge(node);
                svg.push_str(&format!(
                    "  <text x=\"{}\" y=\"{}\" fill=\"{}\" font-size=\"8\" font-weight=\"bold\">{}</text>\n",
                    pos.x + 5.0, pos.y + 12.0, border_color, badge
                ));
                svg.push_str(&format!(
                    "  <text x=\"{}\" y=\"{}\" fill=\"#888\" font-size=\"7\" text-anchor=\"end\">{}</text>\n",
                    pos.x + node_width - 5.0, pos.y + 12.0, escape_xml(&node.node_id)
                ));

                let wrapped_lines = wrap_text(&node.content, rd_config.text_wrap_max_chars);
                let display_lines: Vec<_> = wrapped_lines
                    .iter()
                    .take(rd_config.max_text_lines)
                    .collect();

                let start_y = pos.y + 26.0;
                for (i, line) in display_lines.iter().enumerate() {
                    let y = start_y + (i as f64 * rd_config.line_height);
                    svg.push_str(&format!(
                        "  <text x=\"{}\" y=\"{}\" fill=\"#222\" font-size=\"{}\" text-anchor=\"middle\">{}</text>\n",
                        pos.x + node_width / 2.0,
                        y,
                        rd_config.font_size_content,
                        escape_xml(line)
                    ));
                }

                let step_label = match node.step_range {
                    Some((start, end)) => format!("Steps {}-{}", start, end),
                    None => format!("Step {}", node.source_step_id),
                };
                svg.push_str(&format!(
                    "  <text x=\"{}\" y=\"{}\" fill=\"#666\" font-size=\"{}\" text-anchor=\"middle\">{} | Conf {:.1}</text>\n",
                    pos.x + node_width / 2.0,
                    pos.y + node_height - 10.0,
                    rd_config.font_size_detail,
                    step_label,
                    node.confidence
                ));
            }
        }

        // Place legend in the reserved footer area (canvas was extended by
        // LEGEND_HEIGHT+LEGEND_GAP so the legend cannot overlap nodes).
        let legend_top = max_y + LEGEND_GAP;
        let view_left = min_x - self.margin;
        self.draw_reasoning_dag_legend(&mut svg, legend_top, view_left);

        svg.push_str("</svg>\n");
        svg
    }

    /// Draw the legend in the reserved footer area.
    /// `legend_top` is the Y coordinate below all nodes.
    /// `view_left` is the left edge of the viewBox so the legend is visible.
    fn draw_reasoning_dag_legend(&self, svg: &mut String, legend_top: f64, view_left: f64) {
        let config = get_config();
        let rd_config = &config.rendering.svg.reasoning_dag;

        let legend_x = view_left + 10.0;
        let legend_y = legend_top;

        svg.push_str(&format!(
            "  <rect x=\"{}\" y=\"{}\" width=\"270\" height=\"320\" fill=\"white\" stroke=\"#999\" stroke-width=\"1\" rx=\"5\" opacity=\"0.95\"/>\n",
            legend_x, legend_y
        ));

        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" fill=\"#222\" font-size=\"{}\" font-weight=\"bold\">Legend</text>\n",
            legend_x + 10.0, legend_y + 20.0, rd_config.font_size_legend_title
        ));

        // Node type fills
        let node_types = [
            ("Ground Truth", COLOR_GROUND_TRUTH, COLOR_BORDER_VERIFIED),
            (
                "Hypothesis (verified)",
                COLOR_HYPOTHESIS_VERIFIED,
                COLOR_BORDER_VERIFIED,
            ),
            (
                "Hypothesis (falsified)",
                COLOR_HYPOTHESIS_FALSED,
                COLOR_BORDER_FALSED,
            ),
            (
                "Hypothesis (unverified)",
                COLOR_HYPOTHESIS_UNVERIFIED,
                COLOR_BORDER_DEFAULT,
            ),
            ("Insight", COLOR_INSIGHT, COLOR_BORDER_DEFAULT),
        ];

        for (i, (label, fill, border)) in node_types.iter().enumerate() {
            let y = legend_y + 40.0 + (i as f64 * 20.0);
            svg.push_str(&format!(
                "  <rect x=\"{}\" y=\"{}\" width=\"16\" height=\"12\" fill=\"{}\" stroke=\"{}\" stroke-width=\"1\" rx=\"2\"/>\n",
                legend_x + 10.0, y - 9.0, fill, border
            ));
            svg.push_str(&format!(
                "  <text x=\"{}\" y=\"{}\" fill=\"#222\" font-size=\"9\">{}</text>\n",
                legend_x + 32.0,
                y,
                label
            ));
        }

        // Trajectory source frames
        let frame_y = legend_y + 148.0;
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" fill=\"#222\" font-size=\"9\" font-weight=\"bold\">Trajectory source (outer frame):</text>\n",
            legend_x + 10.0, frame_y
        ));
        let frames = [
            ("Solved trajectory", "#2e7d32"),
            ("Failed trajectory", "#c62828"),
            ("Shared (both)", "#1565c0"),
        ];
        for (i, (label, color)) in frames.iter().enumerate() {
            let y = frame_y + 15.0 + (i as f64 * 20.0);
            svg.push_str(&format!(
                "  <rect x=\"{}\" y=\"{}\" width=\"18\" height=\"13\" fill=\"none\" stroke=\"{}\" stroke-width=\"2.5\" rx=\"3\" stroke-dasharray=\"5,2\"/>\n",
                legend_x + 10.0, y - 10.0, color
            ));
            svg.push_str(&format!(
                "  <text x=\"{}\" y=\"{}\" fill=\"#222\" font-size=\"9\">{}</text>\n",
                legend_x + 34.0,
                y,
                label
            ));
        }

        // Edge types
        let edge_y = frame_y + 80.0;
        let edge_types = [
            ("Infers", COLOR_EDGE_INFERS, "arrow-infers", ""),
            (
                "Contradicts/Falsifies",
                COLOR_EDGE_CONTRADICTS,
                "arrow-contradicts",
                "6,4",
            ),
            (
                "Supersedes",
                COLOR_EDGE_SUPERSEDES,
                "arrow-supersedes",
                "6,4",
            ),
        ];

        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" fill=\"#222\" font-size=\"9\" font-weight=\"bold\">Edges:</text>\n",
            legend_x + 10.0, edge_y - 5.0
        ));

        for (i, (label, color, marker, dash)) in edge_types.iter().enumerate() {
            let y = edge_y + 10.0 + (i as f64 * 18.0);
            let dash_attr = if dash.is_empty() {
                String::new()
            } else {
                format!(" stroke-dasharray=\"{}\"", dash)
            };
            svg.push_str(&format!(
                "  <line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-width=\"1.5\" marker-end=\"url(#{})\"{}/>",
                legend_x + 10.0, y, legend_x + 30.0, y, color, marker, dash_attr
            ));
            svg.push('\n');
            svg.push_str(&format!(
                "  <text x=\"{}\" y=\"{}\" fill=\"#222\" font-size=\"9\">{}</text>\n",
                legend_x + 35.0,
                y + 4.0,
                label
            ));
        }

        // Junction dot
        let jy = edge_y + 10.0 + 3.0 * 18.0;
        svg.push_str(&format!(
            "  <circle cx=\"{}\" cy=\"{}\" r=\"4\" fill=\"{}\" />\n",
            legend_x + 20.0,
            jy,
            COLOR_EDGE_INFERS
        ));
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" fill=\"#222\" font-size=\"9\">Joint premise (N-to-1)</text>\n",
            legend_x + 35.0, jy + 4.0
        ));

        // Divergence marker
        let dy = jy + 20.0;
        svg.push_str(&format!(
            "  <rect x=\"{}\" y=\"{}\" width=\"18\" height=\"13\" fill=\"none\" stroke=\"#ff6f00\" stroke-width=\"3\" rx=\"3\"/>\n",
            legend_x + 10.0, dy - 9.0
        ));
        svg.push_str(&format!(
            "  <polygon points=\"{},{} {},{} {},{}\" fill=\"#ff6f00\" stroke=\"#fff\" stroke-width=\"0.5\"/>\n",
            legend_x + 7.0, dy - 13.0, legend_x + 3.0, dy - 6.0, legend_x + 11.0, dy - 6.0
        ));
        svg.push_str(&format!(
            "  <text x=\"{}\" y=\"{}\" fill=\"#222\" font-size=\"9\">Divergence point</text>\n",
            legend_x + 35.0,
            dy
        ));
    }
}

fn node_colors(node: &crate::models::ReasoningArtifactNode) -> (&'static str, &'static str) {
    match node.node_type {
        ReasoningNodeType::GroundTruth => (COLOR_GROUND_TRUTH, COLOR_BORDER_VERIFIED),
        ReasoningNodeType::Insight => match &node.status {
            Some(InsightStatus::Verified) => (COLOR_HYPOTHESIS_VERIFIED, COLOR_BORDER_VERIFIED),
            Some(InsightStatus::SelfFalsed) => (COLOR_HYPOTHESIS_FALSED, COLOR_BORDER_FALSED),
            Some(InsightStatus::Unverified) => (COLOR_INSIGHT, COLOR_BORDER_DEFAULT),
            None => (COLOR_INSIGHT, COLOR_BORDER_DEFAULT),
        },
    }
}

/// Returns outer frame color based on node_id prefix encoding trajectory source.
/// S_ = solved (green), F_ = failed (red), SHARED_ = both (blue), else None.
fn trajectory_frame_color(node_id: &str) -> Option<&'static str> {
    if node_id.starts_with("S_") {
        Some("#2e7d32")
    } else if node_id.starts_with("F_") {
        Some("#c62828")
    } else if node_id.starts_with("SHARED_") {
        Some("#1565c0")
    } else {
        None
    }
}

fn node_badge(node: &crate::models::ReasoningArtifactNode) -> &'static str {
    match node.node_type {
        ReasoningNodeType::GroundTruth => "FACT",
        ReasoningNodeType::Insight => match &node.status {
            Some(InsightStatus::Verified) => "VERIFIED",
            Some(InsightStatus::SelfFalsed) => "FALSIFIED",
            Some(InsightStatus::Unverified) => "HYPOTHESIS",
            None => "INSIGHT",
        },
    }
}
