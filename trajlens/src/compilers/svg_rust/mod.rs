/// SVG renderer for TrajLens graphs.
///
/// Dispatches to graph-specific sub-modules for rendering. Each graph type
/// has its own file with layout and drawing logic:
/// - `activity_graph.rs`: hierarchical containers with operation tables
/// - `cost_map.rs`: recursive treemap with area proportional to cost
/// - `goal_tree.rs`: hierarchical tree with transition edges
/// - `reasoning_dag.rs`: directed graph with inference relationships
///
/// Shared SVG primitives (rect, text, edge, header/footer) live in this module.
mod activity_graph;
mod cost_map;
mod goal_tree;
mod reasoning_dag;

use crate::compilers::traits::GraphCompiler;
use crate::models::GraphEnum;

/// SVG renderer (Rust implementation) implementing the Renderer trait with String output.
pub struct SVGCompiler {
    margin: f64,
}

impl SVGCompiler {
    pub fn new() -> Self {
        Self { margin: 20.0 }
    }

    pub fn with_margin(margin: f64) -> Self {
        Self { margin }
    }
}

impl Default for SVGCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphCompiler for SVGCompiler {
    type Output = String;

    fn compile(&self, graph: &GraphEnum) -> Self::Output {
        match graph {
            GraphEnum::ActivityGraph(ag) => self.render_activity_graph(ag),
            GraphEnum::CostMap(cm) => self.render_cost_map(cm),
            GraphEnum::GoalTree(gt) => self.render_goal_tree(gt),
            GraphEnum::ReasoningDAG(rd) => self.render_reasoning_dag(rd),
        }
    }

    fn name(&self) -> &'static str {
        "svg-rust"
    }
}

// ============ Shared SVG Primitives ============

impl SVGCompiler {
    pub(crate) fn svg_header(&self, svg: &mut String, width: f64, height: f64) {
        let total_w = width + 2.0 * self.margin;
        let total_h = height + 2.0 * self.margin;
        svg.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        svg.push_str(&format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"{} {} {} {}\">\n",
            total_w as i32,
            total_h as i32,
            -self.margin,
            -self.margin,
            total_w,
            total_h
        ));
        svg.push_str("  <defs>\n");
        svg.push_str("    <marker id=\"arrowhead\" markerWidth=\"10\" markerHeight=\"10\" refX=\"9\" refY=\"3\" orient=\"auto\">\n");
        svg.push_str("      <polygon points=\"0 0, 10 3, 0 6\" fill=\"#666\" />\n");
        svg.push_str("    </marker>\n");
        svg.push_str("  </defs>\n");
    }

    pub(crate) fn svg_footer(&self, svg: &mut String) {
        svg.push_str("</svg>\n");
    }

    pub(crate) fn draw_rect(
        &self,
        svg: &mut String,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        fill: &str,
        stroke: &str,
        stroke_width: &str,
    ) {
        svg.push_str(&format!(
            "  <rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{}\" stroke=\"{}\" stroke-width=\"{}\"/>\n",
            x, y, w, h, fill, stroke, stroke_width
        ));
    }

    pub(crate) fn draw_text(
        &self,
        svg: &mut String,
        x: f64,
        y: f64,
        text: &str,
        font_size: i32,
        anchor: &str,
        fill: &str,
    ) {
        let escaped = escape_xml(text);
        svg.push_str(&format!(
            "  <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"{}\" font-family=\"Helvetica, sans-serif\" text-anchor=\"{}\" fill=\"{}\">{}</text>\n",
            x, y, font_size, anchor, fill, escaped
        ));
    }

    pub(crate) fn draw_edge(
        &self,
        svg: &mut String,
        src_pos: (f64, f64),
        src_dim: (f64, f64),
        tgt_pos: (f64, f64),
        tgt_dim: (f64, f64),
        color: &str,
        dashed: bool,
        arrowhead: bool,
    ) {
        let src_pt = edge_endpoint_on_box(src_pos, src_dim, tgt_pos);
        let tgt_pt = edge_endpoint_on_box(tgt_pos, tgt_dim, src_pos);

        let dash_attr = if dashed {
            " stroke-dasharray=\"6,4\""
        } else {
            ""
        };
        let marker_attr = if arrowhead {
            " marker-end=\"url(#arrowhead)\""
        } else {
            ""
        };

        svg.push_str(&format!(
            "  <line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"2\"{}{}/>\n",
            src_pt.0, src_pt.1, tgt_pt.0, tgt_pt.1, color, dash_attr, marker_attr
        ));
    }
}

// ============ Utility Functions ============

/// Word-wrap text into lines no longer than max_chars.
pub(crate) fn wrap_text(text: &str, max_chars: usize) -> Vec<String> {
    if text.chars().count() <= max_chars {
        return vec![text.to_string()];
    }

    let mut lines = Vec::new();
    let mut remaining = text;

    while !remaining.is_empty() {
        if remaining.chars().count() <= max_chars {
            lines.push(remaining.to_string());
            break;
        }

        // Find byte offset of the max_chars-th character
        let byte_limit = remaining
            .char_indices()
            .nth(max_chars)
            .map(|(i, _)| i)
            .unwrap_or(remaining.len());

        if let Some(pos) = remaining[..byte_limit].rfind(' ') {
            lines.push(remaining[..pos].to_string());
            remaining = &remaining[pos + 1..];
        } else {
            lines.push(remaining[..byte_limit].to_string());
            remaining = &remaining[byte_limit..];
        }
    }

    lines
}

/// Truncate text to max_chars, adding ellipsis if truncated.
pub(crate) fn truncate(text: &str, max_chars: usize) -> String {
    if text.chars().count() <= max_chars {
        text.to_string()
    } else {
        let end: String = text.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", end)
    }
}

/// Compute point where line from 'other' to 'center' intersects rectangle border.
fn edge_endpoint_on_box(center: (f64, f64), (w, h): (f64, f64), other: (f64, f64)) -> (f64, f64) {
    let (cx, cy) = center;
    let (ox, oy) = other;
    let dx = ox - cx;
    let dy = oy - cy;

    if dx.abs() < 0.01 && dy.abs() < 0.01 {
        return center;
    }

    let half_w = w / 2.0;
    let half_h = h / 2.0;

    let sx = if dx.abs() > 0.01 {
        half_w / dx.abs()
    } else {
        f64::INFINITY
    };
    let sy = if dy.abs() > 0.01 {
        half_h / dy.abs()
    } else {
        f64::INFINITY
    };
    let s = sx.min(sy);

    (cx + dx * s, cy + dy * s)
}

/// Escape XML special characters.
pub(crate) fn escape_xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wrap_text_long() {
        let result = wrap_text("hello world this is a test", 10);
        assert!(result.len() >= 3);
        assert!(result.iter().all(|line| line.len() <= 11));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("a<b>c&d"), "a&lt;b&gt;c&amp;d");
    }
}
