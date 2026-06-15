/// Cost Map SVG rendering using squarified treemap layout.
///
/// Uses the `treemap` crate (squarified algorithm) for optimal aspect ratios.
/// Each hierarchy level is laid out independently within its parent's bounds,
/// producing nested rectangles with area proportional to cost.
use crate::models::{CostMap, CostMapNode};
use treemap::{MapItem, Mappable, Rect, TreemapLayout};

use super::SVGCompiler;

const HEADER_HEIGHT: f64 = 22.0;
const PADDING: f64 = 2.0;

impl SVGCompiler {
    pub(super) fn render_cost_map(&self, cost_map: &CostMap) -> String {
        let canvas_w = 1200.0;
        let canvas_h = 800.0;

        let mut svg = String::new();
        self.svg_header(&mut svg, canvas_w, canvas_h);

        let root_bounds = Rect::from_points(0.0, 0.0, canvas_w, canvas_h);
        self.render_treemap_node(&mut svg, &cost_map.root, root_bounds, 0);

        self.svg_footer(&mut svg);
        svg
    }

    fn render_treemap_node(
        &self,
        svg: &mut String,
        node: &CostMapNode,
        bounds: Rect,
        depth: usize,
    ) {
        if bounds.w < 3.0 || bounds.h < 3.0 {
            return;
        }

        let fill = node_fill(node, depth);
        let stroke_width = format!("{:.1}", (1.8 - depth as f64 * 0.4).max(0.4));
        self.draw_rect(
            svg,
            bounds.x,
            bounds.y,
            bounds.w,
            bounds.h,
            fill,
            "#444",
            &stroke_width,
        );

        let has_header_space = bounds.w > 30.0 && bounds.h > HEADER_HEIGHT;
        if has_header_space {
            // Build label with step range inline (only for multi-step ranges on goal nodes)
            let base_label = format_label(&node.node_id, &node.label, bounds.w);
            let label = if let Some((start, end)) = node.step_range {
                if start != end && !node.children.is_empty() {
                    format!("{} [steps {}-{}]", base_label, start, end)
                } else {
                    base_label
                }
            } else {
                base_label
            };
            self.draw_text(
                svg,
                bounds.x + 4.0,
                bounds.y + 14.0,
                &label,
                11,
                "start",
                "#111",
            );

            // Show cost on the right side of the header
            let cost_str = format_cost(&node.cost);
            if bounds.w > 80.0 {
                self.draw_text(
                    svg,
                    bounds.x + bounds.w - 4.0,
                    bounds.y + 14.0,
                    &cost_str,
                    9,
                    "end",
                    "#555",
                );
            }
        }

        if node.children.is_empty() {
            return;
        }

        // Compute inner bounds for children (below header)
        let inner_y = if has_header_space {
            bounds.y + HEADER_HEIGHT
        } else {
            bounds.y + PADDING
        };
        let inner_x = bounds.x + PADDING;
        let inner_w = bounds.w - 2.0 * PADDING;
        let inner_h = (bounds.y + bounds.h - PADDING) - inner_y;

        if inner_w < 4.0 || inner_h < 4.0 {
            return;
        }

        // Use squarified treemap layout for children
        let children_with_cost: Vec<(usize, f64)> = node
            .children
            .iter()
            .enumerate()
            .filter(|(_, c)| c.cost.dollar_cost > 0.0 || c.cost.input_tokens > 0)
            .map(|(i, c)| {
                let size = if c.cost.dollar_cost > 0.0 {
                    c.cost.dollar_cost
                } else {
                    c.cost.input_tokens as f64
                };
                (i, size.max(0.001))
            })
            .collect();

        if children_with_cost.is_empty() {
            return;
        }

        let mut items: Vec<MapItem> = children_with_cost
            .iter()
            .map(|(_, size)| MapItem::with_size(*size))
            .collect();

        let layout = TreemapLayout::new();
        let layout_bounds = Rect::from_points(inner_x, inner_y, inner_w, inner_h);
        layout.layout_items(&mut items, layout_bounds);

        // Items get sorted by the layout, so we need to match by size
        // Instead, do a second pass: layout preserves order after sorting,
        // so we sort children_with_cost the same way (descending by size)
        let mut sorted_children: Vec<(usize, f64)> = children_with_cost;
        sorted_children.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        for (item_idx, (child_idx, _)) in sorted_children.iter().enumerate() {
            if item_idx >= items.len() {
                break;
            }
            let child_bounds = *items[item_idx].bounds();
            let child = &node.children[*child_idx];
            self.render_treemap_node(svg, child, child_bounds, depth + 1);
        }
    }
}

fn node_fill(node: &CostMapNode, depth: usize) -> &'static str {
    if let Some(cat) = &node.category {
        match cat.as_str() {
            "read" => "#bbdefb",
            "write" => "#f8bbd0",
            "edit" => "#ffe0b2",
            "run" => "#c8e6c9",
            "think" => "#e1bee7",
            "event" => "#fff9c4",
            _ => "#eeeeee",
        }
    } else {
        match depth % 4 {
            0 => "#f5f5f5",
            1 => "#e8eaf6",
            2 => "#ede7f6",
            _ => "#e0f2f1",
        }
    }
}

fn format_cost(cost: &crate::models::Cost) -> String {
    if cost.dollar_cost >= 1.0 {
        format!("${:.2}", cost.dollar_cost)
    } else if cost.dollar_cost >= 0.01 {
        format!("${:.3}", cost.dollar_cost)
    } else if cost.dollar_cost > 0.0 {
        format!("${:.4}", cost.dollar_cost)
    } else if cost.input_tokens > 0 {
        format!("{}tok", cost.input_tokens)
    } else {
        String::new()
    }
}

fn format_label(_node_id: &str, label: &str, available_width: f64) -> String {
    let max_chars = ((available_width - 8.0) / 6.5) as usize;
    let full = label.to_string();

    if full.len() <= max_chars {
        full
    } else if max_chars > 3 {
        format!("{}...", &full[..max_chars - 3])
    } else {
        String::new()
    }
}
