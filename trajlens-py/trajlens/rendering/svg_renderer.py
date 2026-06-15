"""
SVG Renderer for TrajLens IGR files (Python reference implementation).

Reads an IGR TOML file and produces SVG to stdout.
Supports all 4 graph types: goal_transition_tree, reasoning_artifact_dag,
activity_graph, cost_map.

Usage:
    python -m trajlens.rendering.svg_renderer <input.igr.toml>
"""

import sys
import tomllib
from pathlib import Path
from xml.sax.saxutils import escape


def wrap_text(text: str, max_chars: int = 28) -> list[str]:
    """Word-wrap text into lines no longer than max_chars."""
    if len(text) <= max_chars:
        return [text]
    lines = []
    remaining = text
    while remaining:
        if len(remaining) <= max_chars:
            lines.append(remaining)
            break
        pos = remaining[:max_chars].rfind(" ")
        if pos > 0:
            lines.append(remaining[:pos])
            remaining = remaining[pos + 1 :]
        else:
            lines.append(remaining[:max_chars])
            remaining = remaining[max_chars:]
    return lines


def render_goal_tree(data: dict) -> str:
    """Render Goal Transition Tree as hierarchical SVG."""
    nodes = data.get("nodes", [])
    edges = data.get("edges", [])
    root_id = data.get("root_id", nodes[0]["node_id"] if nodes else "")

    node_width, node_height = 240, 90
    level_spacing, node_spacing = 150, 50

    # Assign levels via BFS
    node_levels = {root_id: 0}
    queue = [(root_id, 0)]
    visited = set()
    while queue:
        nid, level = queue.pop(0)
        if nid in visited:
            continue
        visited.add(nid)
        for e in edges:
            if e["source_id"] == nid:
                et = e["edge_type"]
                tl = (
                    level + 1
                    if et == "sub"
                    else (level if et == "next" else max(0, level - 1))
                )
                if e["target_id"] not in node_levels:
                    node_levels[e["target_id"]] = tl
                    queue.append((e["target_id"], tl))

    # Build parent map
    parent_ids = {root_id: None}
    changed = True
    while changed:
        changed = False
        for e in edges:
            if e["edge_type"] == "sub" and e["target_id"] not in parent_ids:
                parent_ids[e["target_id"]] = e["source_id"]
                changed = True
            elif e["edge_type"] == "next":
                if e["source_id"] in parent_ids and e["target_id"] not in parent_ids:
                    parent_ids[e["target_id"]] = parent_ids[e["source_id"]]
                    changed = True

    # Assign hierarchical IDs
    h_ids = {root_id: "1"}
    counters = {}
    queue = [root_id]
    while queue:
        curr = queue.pop(0)
        first_child = next(
            (
                e["target_id"]
                for e in edges
                if e["source_id"] == curr and e["edge_type"] == "sub"
            ),
            None,
        )
        if first_child:
            ordered = [first_child]
            cur = first_child
            while True:
                nxt = next(
                    (
                        e["target_id"]
                        for e in edges
                        if e["source_id"] == cur and e["edge_type"] == "next"
                    ),
                    None,
                )
                if nxt:
                    ordered.append(nxt)
                    cur = nxt
                else:
                    break
            for child in ordered:
                if child not in h_ids:
                    counters.setdefault(curr, 0)
                    counters[curr] += 1
                    h_ids[child] = f"{h_ids[curr]}.{counters[curr]}"
                    queue.append(child)

    # Group and sort nodes by level
    levels = {}
    for n in nodes:
        lv = node_levels.get(n["node_id"], 0)
        levels.setdefault(lv, []).append(n)

    for lv in levels:
        levels[lv].sort(
            key=lambda n: [int(x) for x in h_ids.get(n["node_id"], "0").split(".")]
        )

    # Position nodes
    positions = {}
    for lv, lv_nodes in levels.items():
        y = lv * level_spacing + 50
        total_w = len(lv_nodes) * (node_width + node_spacing) - node_spacing
        start_x = -total_w / 2 + node_width / 2
        for i, n in enumerate(lv_nodes):
            positions[n["node_id"]] = (start_x + i * (node_width + node_spacing), y)

    # Canvas bounds
    xs = [p[0] for p in positions.values()]
    ys = [p[1] for p in positions.values()]
    margin = 60
    legend_h = 160
    cw = max(max(xs) - min(xs) + node_width + 2 * margin, 800)
    ch = max(max(ys) - min(ys) + node_height + 2 * margin + legend_h, 600)
    ox = margin - min(xs) + node_width / 2
    oy = margin - min(ys)
    positions = {k: (v[0] + ox, v[1] + oy) for k, v in positions.items()}

    # Build SVG
    svg = [f'<?xml version="1.0" encoding="UTF-8"?>']
    svg.append(
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{cw:.0f}" height="{ch:.0f}" viewBox="0 0 {cw:.0f} {ch:.0f}">'
    )
    svg.append(
        '<defs><marker id="arrowhead" markerWidth="10" markerHeight="10" refX="9" refY="3" orient="auto">'
    )
    svg.append('<polygon points="0 0, 10 3, 0 6" fill="#666"/></marker></defs>')

    # Edges
    node_status = {n["node_id"]: n.get("status", "") for n in nodes}
    arrow_offset = 8
    for e in edges:
        s, t = e["source_id"], e["target_id"]
        if s in positions and t in positions:
            sx, sy = positions[s]
            tx, ty = positions[t]

            # Color backtrack by source node status: green if done, red if failed
            if e["edge_type"] == "backtrack":
                if node_status.get(s) == "done":
                    c = "#4CAF50"
                    label = "success"
                else:
                    c = "#F44336"
                    label = "backtrack"
                dash = ' stroke-dasharray="6,4"'
            elif e["edge_type"] == "next":
                c, label, dash = "#4CAF50", "next", ""
            else:
                c, label, dash = "#2196F3", "new plan", ""

            # Line ends 1px before target border; arrowhead tip (refX=9, 10px marker)
            # extends 1px past line end, landing exactly at the node frame.
            tip_offset = 1
            if e["edge_type"] == "next":
                x1, y1 = sx + node_width / 2, sy + node_height / 2
                x2, y2 = tx - node_width / 2 - tip_offset, ty + node_height / 2
            elif e["edge_type"] == "sub":
                x1, y1 = sx, sy + node_height
                x2, y2 = tx, ty - tip_offset
            else:
                x1, y1 = sx, sy
                x2, y2 = tx, ty + node_height + tip_offset

            svg.append(
                f'  <line x1="{x1}" y1="{y1}" x2="{x2}" y2="{y2}" stroke="{c}" stroke-width="2"{dash} marker-end="url(#arrowhead)"/>'
            )
            mx, my = (x1 + x2) / 2, (y1 + y2) / 2
            label = e.get("label") or label
            svg.append(
                f'  <text x="{mx}" y="{my-5}" fill="{c}" font-size="9" text-anchor="middle" font-style="italic">{escape(label)}</text>'
            )

    # Nodes
    status_colors = {
        "done": "#C8E6C9",
        "failed": "#FFCDD2",
        "abandoned": "#E0E0E0",
        "wip": "#FFF9C4",
    }
    for n in nodes:
        nid = n["node_id"]
        if nid not in positions:
            continue
        x, y = positions[nid]
        fill = status_colors.get(n.get("status", ""), "#FFFFFF")
        svg.append(
            f'  <rect x="{x-node_width/2}" y="{y}" width="{node_width}" height="{node_height}" fill="{fill}" stroke="#666" stroke-width="1.5" rx="5"/>'
        )
        hid = h_ids.get(nid, nid)
        svg.append(
            f'  <text x="{x}" y="{y+14}" fill="#666" font-size="10" font-weight="bold" text-anchor="middle">{escape(hid)}</text>'
        )
        lines = wrap_text(n.get("label", ""), 28)[:3]
        for i, line in enumerate(lines):
            svg.append(
                f'  <text x="{x}" y="{y+28+i*14}" fill="#000" font-size="10" font-weight="bold" text-anchor="middle">{escape(line)}</text>'
            )
        sr = n.get("step_range", [0, 0])
        svg.append(
            f'  <text x="{x}" y="{y+node_height-10}" fill="#666" font-size="9" text-anchor="middle">Steps: {sr[0]}-{sr[1]}</text>'
        )

    # Legend
    lx, ly = 10, ch - 150
    svg.append(
        f'  <rect x="{lx}" y="{ly}" width="200" height="140" fill="white" stroke="#999" stroke-width="1" rx="5" opacity="0.95"/>'
    )
    svg.append(
        f'  <text x="{lx+10}" y="{ly+20}" fill="#000" font-size="12" font-weight="bold">Legend</text>'
    )
    for i, (label, color) in enumerate(
        [
            ("Done", "#C8E6C9"),
            ("In Progress", "#FFF9C4"),
            ("Failed", "#FFCDD2"),
            ("Abandoned", "#E0E0E0"),
        ]
    ):
        ey = ly + 40 + i * 25
        svg.append(
            f'  <rect x="{lx+10}" y="{ey-10}" width="20" height="15" fill="{color}" stroke="#666" stroke-width="1" rx="2"/>'
        )
        svg.append(
            f'  <text x="{lx+35}" y="{ey}" fill="#000" font-size="10">{label}</text>'
        )

    svg.append("</svg>")
    return "\n".join(svg)


def render_reasoning_dag(data: dict) -> str:
    """Render Reasoning Artifact DAG as directed graph SVG."""
    nodes = data.get("nodes", [])
    edges = data.get("edges", [])

    node_width, node_height = 220, 90
    x_spacing, y_spacing = 200, 120

    # Simple layered layout: assign layers by longest path from sources
    in_degree = {n["node_id"]: 0 for n in nodes}
    for e in edges:
        in_degree[e["target_id"]] = in_degree.get(e["target_id"], 0) + 1

    layers = {}
    queue = [nid for nid, deg in in_degree.items() if deg == 0]
    for nid in queue:
        layers[nid] = 0
    visited = set()
    while queue:
        nid = queue.pop(0)
        if nid in visited:
            continue
        visited.add(nid)
        for e in edges:
            for sid in e.get("source_ids", []):
                if sid == nid:
                    tid = e["target_id"]
                    layers[tid] = max(layers.get(tid, 0), layers[nid] + 1)
                    if tid not in visited:
                        queue.append(tid)

    # Position by layer
    level_nodes = {}
    for n in nodes:
        lv = layers.get(n["node_id"], 0)
        level_nodes.setdefault(lv, []).append(n)

    positions = {}
    for lv, lv_nodes in level_nodes.items():
        for i, n in enumerate(lv_nodes):
            x = i * (node_width + 50)
            y = lv * (node_height + y_spacing)
            positions[n["node_id"]] = (x, y)

    # Canvas
    if not positions:
        return '<svg xmlns="http://www.w3.org/2000/svg" width="400" height="300"></svg>'
    xs = [p[0] for p in positions.values()]
    ys = [p[1] for p in positions.values()]
    margin = 40
    cw = max(xs) - min(xs) + node_width + 2 * margin
    ch = max(ys) - min(ys) + node_height + 2 * margin + 160
    ox = margin - min(xs)
    oy = margin - min(ys)
    positions = {k: (v[0] + ox, v[1] + oy) for k, v in positions.items()}

    svg = [f'<?xml version="1.0" encoding="UTF-8"?>']
    svg.append(
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{cw:.0f}" height="{ch:.0f}" viewBox="0 0 {cw:.0f} {ch:.0f}">'
    )
    svg.append(
        '<defs><marker id="arrowhead" markerWidth="10" markerHeight="10" refX="9" refY="3" orient="auto">'
    )
    svg.append('<polygon points="0 0, 10 3, 0 6" fill="#666"/></marker></defs>')

    # Edges
    edge_colors = {
        "infers": "#2196F3",
        "contradicts": "#F44336",
        "supersedes": "#FF9800",
    }
    for e in edges:
        tid = e["target_id"]
        color = edge_colors.get(e.get("edge_type", "infers"), "#666")
        for sid in e.get("source_ids", []):
            if sid in positions and tid in positions:
                sx, sy = positions[sid]
                tx, ty = positions[tid]
                scx, scy = sx + node_width / 2, sy + node_height / 2
                tcx, tcy = tx + node_width / 2, ty + node_height / 2
                svg.append(
                    f'  <line x1="{scx}" y1="{scy}" x2="{tcx}" y2="{tcy}" stroke="{color}" stroke-width="2" marker-end="url(#arrowhead)"/>'
                )

    # Nodes
    type_colors = {"ground_truth": "#E3F2FD", "insight": "#FFF3E0"}
    for n in nodes:
        nid = n["node_id"]
        if nid not in positions:
            continue
        x, y = positions[nid]
        fill = type_colors.get(n.get("node_type", ""), "#FFF")
        svg.append(
            f'  <rect x="{x}" y="{y}" width="{node_width}" height="{node_height}" fill="{fill}" stroke="#666" stroke-width="1.5" rx="5"/>'
        )
        lines = wrap_text(n.get("content", ""), 28)[:3]
        for i, line in enumerate(lines):
            svg.append(
                f'  <text x="{x+node_width/2}" y="{y+22+i*14}" fill="#000" font-size="10" text-anchor="middle">{escape(line)}</text>'
            )
        svg.append(
            f'  <text x="{x+node_width/2}" y="{y+node_height-15}" fill="#666" font-size="9" text-anchor="middle">Turn: {n.get("source_turn_id",0)} | Conf: {n.get("confidence",0):.1f}</text>'
        )

    # Legend
    lx, ly = 10, ch - 150
    svg.append(
        f'  <rect x="{lx}" y="{ly}" width="220" height="140" fill="white" stroke="#999" stroke-width="1" rx="5" opacity="0.95"/>'
    )
    svg.append(
        f'  <text x="{lx+10}" y="{ly+20}" fill="#000" font-size="12" font-weight="bold">Legend</text>'
    )
    for i, (label, color) in enumerate(
        [("Ground Truth", "#E3F2FD"), ("Insight", "#FFF3E0")]
    ):
        ey = ly + 45 + i * 25
        svg.append(
            f'  <rect x="{lx+10}" y="{ey-10}" width="20" height="15" fill="{color}" stroke="#666" stroke-width="1" rx="2"/>'
        )
        svg.append(
            f'  <text x="{lx+35}" y="{ey}" fill="#000" font-size="10">{label}</text>'
        )

    svg.append("</svg>")
    return "\n".join(svg)


def render_activity_graph(data: dict) -> str:
    """Render Activity Graph as hierarchical SVG."""
    nodes = data.get("nodes", [])
    edges = data.get("edges", [])

    node_width = 180
    cat_colors = {
        "read": "#e3f2fd",
        "write": "#fce4ec",
        "edit": "#fff3e0",
        "run": "#e8f5e9",
        "list": "#f3e5f5",
        "other": "#eeeeee",
    }

    # Simple grid layout by category
    categories = {}
    for n in nodes:
        cat = n.get("goal_category", "other")
        categories.setdefault(cat, []).append(n)

    positions = {}
    x_offset = 20
    for cat, cat_nodes in categories.items():
        for i, n in enumerate(cat_nodes):
            h = 30 + len(n.get("operations", [])) * 16
            positions[n["node_id"]] = (
                x_offset,
                i * (h + 20) + 40,
                node_width,
                max(h, 50),
            )
        x_offset += node_width + 40

    if not positions:
        return '<svg xmlns="http://www.w3.org/2000/svg" width="400" height="300"><text x="200" y="150" text-anchor="middle">No activity nodes</text></svg>'

    cw = max(p[0] + p[2] for p in positions.values()) + 40
    ch = max(p[1] + p[3] for p in positions.values()) + 200

    svg = [f'<?xml version="1.0" encoding="UTF-8"?>']
    svg.append(
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{cw:.0f}" height="{ch:.0f}" viewBox="0 0 {cw:.0f} {ch:.0f}">'
    )
    svg.append(
        '<defs><marker id="arrowhead" markerWidth="10" markerHeight="10" refX="9" refY="3" orient="auto">'
    )
    svg.append('<polygon points="0 0, 10 3, 0 6" fill="#666"/></marker></defs>')

    # Nodes
    for n in nodes:
        nid = n["node_id"]
        if nid not in positions:
            continue
        x, y, w, h = positions[nid]
        cat = n.get("goal_category", "other")
        fill = cat_colors.get(cat, "#eee")
        svg.append(
            f'  <rect x="{x}" y="{y}" width="{w}" height="{h}" fill="{fill}" stroke="#666" stroke-width="1.5" rx="3"/>'
        )
        svg.append(
            f'  <text x="{x+w/2}" y="{y+16}" fill="#000" font-size="10" font-weight="bold" text-anchor="middle">{escape(n.get("label","")[:25])}</text>'
        )
        for j, op in enumerate(n.get("operations", [])[:8]):
            oy = y + 30 + j * 16
            detail = op.get("detail", "")[:20]
            svg.append(
                f'  <text x="{x+5}" y="{oy+11}" fill="#333" font-size="9">#{op.get("call_index",0)} {op.get("op_type","")}: {escape(detail)}</text>'
            )

    # Legend
    lx, ly = 10, ch - 180
    svg.append(
        f'  <rect x="{lx}" y="{ly}" width="180" height="170" fill="white" stroke="#999" stroke-width="1" rx="5" opacity="0.95"/>'
    )
    svg.append(
        f'  <text x="{lx+10}" y="{ly+20}" fill="#000" font-size="12" font-weight="bold">Legend</text>'
    )
    for i, (label, color) in enumerate(
        [
            ("Read", "#e3f2fd"),
            ("Write", "#fce4ec"),
            ("Edit", "#fff3e0"),
            ("Run", "#e8f5e9"),
            ("List", "#f3e5f5"),
            ("Other", "#eeeeee"),
        ]
    ):
        ey = ly + 40 + i * 22
        svg.append(
            f'  <rect x="{lx+10}" y="{ey-10}" width="20" height="15" fill="{color}" stroke="#999" stroke-width="1" rx="2"/>'
        )
        svg.append(
            f'  <text x="{lx+35}" y="{ey}" fill="#000" font-size="10">{label}</text>'
        )

    svg.append("</svg>")
    return "\n".join(svg)


def render_cost_map(data: dict) -> str:
    """Render Cost Map as treemap SVG."""
    root = data.get("root", {})
    cw, ch = 800, 600

    svg = [f'<?xml version="1.0" encoding="UTF-8"?>']
    svg.append(
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{cw}" height="{ch}" viewBox="0 0 {cw} {ch}">'
    )

    cat_colors = {
        "read": "#bbdefb",
        "write": "#f8bbd0",
        "edit": "#ffe0b2",
        "run": "#c8e6c9",
        "think": "#e1bee7",
    }
    depth_colors = ["#f5f5f5", "#e0e0e0", "#bdbdbd"]

    def draw_node(node, x, y, w, h, depth):
        if w < 5 or h < 5:
            return
        fill = cat_colors.get(node.get("category", ""), depth_colors[depth % 3])
        svg.append(
            f'  <rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" fill="{fill}" stroke="#616161" stroke-width="1"/>'
        )
        if w > 40 and h > 20:
            label = node.get("label", "")[:15]
            cost = f"${node.get('cost', {}).get('dollar_cost', 0):.3f}"
            svg.append(
                f'  <text x="{x+w/2:.1f}" y="{y+15}" fill="#000" font-size="10" text-anchor="middle">{escape(label)}</text>'
            )
            svg.append(
                f'  <text x="{x+w/2:.1f}" y="{y+30}" fill="#424242" font-size="9" text-anchor="middle">{cost}</text>'
            )

        children = node.get("children", [])
        if children:
            pad = 4
            ix, iy, iw, ih = x + pad, y + pad, w - 2 * pad, h - 2 * pad
            total_cost = sum(
                max(c.get("cost", {}).get("dollar_cost", 0), 0) for c in children
            )
            if total_cost > 0:
                cx, cy = ix, iy
                horiz = iw >= ih
                for child in children:
                    prop = (
                        max(child.get("cost", {}).get("dollar_cost", 0), 0) / total_cost
                    )
                    if horiz:
                        cw_child = iw * prop
                        draw_node(child, cx, cy, cw_child, ih, depth + 1)
                        cx += cw_child
                    else:
                        ch_child = ih * prop
                        draw_node(child, cx, cy, iw, ch_child, depth + 1)
                        cy += ch_child

    draw_node(root, 0, 0, cw, ch, 0)
    svg.append("</svg>")
    return "\n".join(svg)


def render(data: dict) -> str:
    """Route to appropriate renderer based on graph_type."""
    graph_type = data.get("graph_type", "")
    if graph_type == "goal_transition_tree":
        return render_goal_tree(data)
    elif graph_type == "reasoning_artifact_dag":
        return render_reasoning_dag(data)
    elif graph_type == "activity_graph":
        return render_activity_graph(data)
    elif graph_type == "cost_map":
        return render_cost_map(data)
    else:
        return f"<!-- Unknown graph type: {graph_type} -->"


def main():
    if len(sys.argv) < 2:
        print(
            "Usage: python -m trajlens.rendering.svg_renderer <input.igr.toml>",
            file=sys.stderr,
        )
        sys.exit(1)

    input_path = Path(sys.argv[1])
    with open(input_path, "rb") as f:
        data = tomllib.load(f)

    svg = render(data)
    print(svg)


if __name__ == "__main__":
    main()
