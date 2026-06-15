"""
Render an IGR TOML file to PNG via React Flow in a headless browser.

Uses @xyflow/react v12 with dagre layout. Modern React 18 JSX-like rendering
with custom node components for each graph type.

Usage:
    python dev_utils/render_reactflow.py <input.igr.toml> [output.png]
    python dev_utils/render_reactflow.py output/poc_analysis/exploit_rce_a3/main/goal-tree.igr.toml
    python dev_utils/render_reactflow.py --all output/poc_analysis/  # render all IGR files

Requirements:
    pip install playwright
    playwright install chromium
"""

import argparse
import json
import sys
import tempfile
import tomllib
from pathlib import Path


def igr_to_reactflow_json(igr_path: str) -> str:
    """Convert IGR TOML to React Flow nodes + edges JSON."""
    with open(igr_path, "rb") as f:
        data = tomllib.load(f)

    graph_type = data.get("graph_type", "")
    if graph_type == "goal_transition_tree":
        return goal_tree_to_rf(data)
    elif graph_type == "reasoning_artifact_dag":
        return reasoning_dag_to_rf(data)
    elif graph_type == "activity_graph":
        return activity_graph_to_rf(data)
    elif graph_type == "cost_map":
        return cost_map_to_rf(data)
    return json.dumps({"nodes": [], "edges": [], "graphType": graph_type})


def goal_tree_to_rf(data):
    nodes = data.get("nodes", [])
    edges = data.get("edges", [])

    status_colors = {
        "done": "#dcfce7",
        "failed": "#fecaca",
        "partial": "#fef9c3",
        "abandoned": "#e5e7eb",
    }
    cat_icons = {"explore": "🔍", "think": "💭", "act": "⚡"}

    rf_nodes = []
    for n in nodes:
        cat = n.get("goal_type", "explore")
        icon = cat_icons.get(cat, "")
        status = n.get("status", "done")
        result = n.get("result", "")
        label_parts = [
            f"{icon} {n.get('label', '')}",
        ]
        if result:
            label_parts.append(f"→ {result}")

        rf_nodes.append(
            {
                "id": n["node_id"],
                "type": "goalNode",
                "data": {
                    "label": n.get("label", ""),
                    "nodeId": n["node_id"],
                    "category": cat.upper(),
                    "status": status,
                    "result": result,
                    "stepRange": n.get("step_range", [0, 0]),
                },
                "style": {
                    "background": status_colors.get(status, "#fff"),
                    "border": f"2px solid {'#dc2626' if status == 'failed' else '#16a34a' if status == 'done' else '#666'}",
                    "borderRadius": "12px",
                    "padding": "0",
                    "width": 280,
                },
                "position": {"x": 0, "y": 0},
            }
        )

    rf_edges = []
    for i, e in enumerate(edges):
        etype = e.get("edge_type", "next")
        style = {
            "sub": {"stroke": "#3b82f6", "strokeWidth": 2},
            "next": {"stroke": "#22c55e", "strokeWidth": 2},
            "backtrack": {
                "stroke": "#ef4444",
                "strokeWidth": 1.5,
                "strokeDasharray": "6 3",
            },
        }.get(etype, {"stroke": "#666", "strokeWidth": 1})
        rf_edges.append(
            {
                "id": f"e{i}",
                "source": e["source_id"],
                "target": e["target_id"],
                "style": style,
                "animated": etype == "backtrack",
                "type": "smoothstep",
            }
        )

    return json.dumps({"nodes": rf_nodes, "edges": rf_edges, "graphType": "goal_tree"})


def reasoning_dag_to_rf(data):
    nodes = data.get("nodes", [])
    edges = data.get("edges", [])

    type_colors = {
        "ground_truth": "#dbeafe",
        "insight": "#fef3c7",
    }
    status_border = {
        "verified": "#16a34a",
        "self-falsed": "#dc2626",
        "unverified": "#9ca3af",
    }

    rf_nodes = []
    for n in nodes:
        ntype = n.get("node_type", "insight")
        status = n.get("status", "")
        content = n.get("content", "")[:80]
        border_color = status_border.get(status, "#6b7280")

        rf_nodes.append(
            {
                "id": n["node_id"],
                "type": "reasoningNode",
                "data": {
                    "content": content,
                    "nodeType": ntype,
                    "status": status,
                    "confidence": n.get("confidence", 0),
                    "stepRange": n.get("step_range", [0, 0]),
                },
                "style": {
                    "background": type_colors.get(ntype, "#f3f4f6"),
                    "border": f"2px solid {border_color}",
                    "borderRadius": "8px",
                    "padding": "0",
                    "width": 240,
                },
                "position": {"x": 0, "y": 0},
            }
        )

    edge_colors = {
        "infers": "#3b82f6",
        "contradicts": "#ef4444",
        "supersedes": "#f59e0b",
    }
    rf_edges = []
    idx = 0
    for e in edges:
        etype = e.get("edge_type", "infers")
        color = edge_colors.get(etype, "#6b7280")
        sources = e.get("source_ids", [e.get("source_id", "")])
        for sid in sources:
            if not sid:
                continue
            rf_edges.append(
                {
                    "id": f"e{idx}",
                    "source": sid,
                    "target": e["target_id"],
                    "style": {
                        "stroke": color,
                        "strokeWidth": 1.5,
                        **({} if etype == "infers" else {"strokeDasharray": "5 3"}),
                    },
                    "type": "smoothstep",
                    "animated": etype == "contradicts",
                }
            )
            idx += 1

    return json.dumps(
        {"nodes": rf_nodes, "edges": rf_edges, "graphType": "reasoning_dag"}
    )


def activity_graph_to_rf(data):
    nodes = data.get("nodes", [])
    edges = data.get("edges", [])
    cat_colors = {
        "read": "#dbeafe",
        "write": "#fce7f3",
        "edit": "#fff7ed",
        "run": "#dcfce7",
        "list": "#f3e8ff",
        "other": "#f3f4f6",
    }

    rf_nodes = []
    for n in nodes:
        cat = n.get("goal_category", "other")
        ops = n.get("operations", [])
        detail_lines = [op.get("detail", "")[:40] for op in ops[:4]]

        rf_nodes.append(
            {
                "id": n["node_id"],
                "type": "activityNode",
                "data": {
                    "label": n.get("label", ""),
                    "category": cat,
                    "opsCount": len(ops),
                    "details": detail_lines,
                },
                "style": {
                    "background": cat_colors.get(cat, "#f3f4f6"),
                    "border": "1.5px solid #6b7280",
                    "borderRadius": "8px",
                    "padding": "0",
                    "width": 200,
                },
                "position": {"x": 0, "y": 0},
            }
        )

    seen = set()
    rf_edges = []
    for i, e in enumerate(edges):
        key = (e["source_id"], e["target_id"])
        if key not in seen:
            seen.add(key)
            rf_edges.append(
                {
                    "id": f"e{i}",
                    "source": e["source_id"],
                    "target": e["target_id"],
                    "type": "smoothstep",
                    "style": {"stroke": "#6b7280", "strokeWidth": 1},
                }
            )

    return json.dumps(
        {"nodes": rf_nodes, "edges": rf_edges, "graphType": "activity_graph"}
    )


def cost_map_to_rf(data):
    root = data.get("root", {})
    rf_nodes = []
    rf_edges = []
    idx = [0]

    def add_node(node, depth):
        nid = node["node_id"]
        cost = node.get("cost", {}).get("dollar_cost", 0)
        rf_nodes.append(
            {
                "id": nid,
                "type": "costNode",
                "data": {
                    "label": node.get("label", ""),
                    "cost": cost,
                    "category": node.get("category", ""),
                },
                "style": {
                    "background": "#f8fafc" if depth == 0 else "#e2e8f0",
                    "border": "1.5px solid #475569",
                    "borderRadius": "6px",
                    "padding": "0",
                    "width": 180,
                },
                "position": {"x": 0, "y": 0},
            }
        )
        for child in node.get("children", []):
            add_node(child, depth + 1)
            rf_edges.append(
                {
                    "id": f"e{idx[0]}",
                    "source": nid,
                    "target": child["node_id"],
                    "type": "smoothstep",
                    "style": {"stroke": "#64748b", "strokeWidth": 1},
                }
            )
            idx[0] += 1

    if root:
        add_node(root, 0)
    return json.dumps({"nodes": rf_nodes, "edges": rf_edges, "graphType": "cost_map"})


HTML_TEMPLATE = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>TrajLens Graph</title>
<link rel="stylesheet" href="https://esm.sh/@xyflow/react@12.6.0/dist/style.css">
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
html, body, #root { width: 100vw; height: 100vh; font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif; }
body { background: #fafafa; }

.goal-node { padding: 12px 14px; }
.goal-node .header { display: flex; align-items: center; gap: 6px; margin-bottom: 4px; }
.goal-node .header .badge { font-size: 9px; font-weight: 700; letter-spacing: 0.5px; padding: 2px 6px; border-radius: 4px; background: rgba(0,0,0,0.08); }
.goal-node .header .node-id { font-size: 10px; color: #6b7280; }
.goal-node .label { font-size: 12px; font-weight: 600; color: #1f2937; line-height: 1.3; margin-bottom: 4px; }
.goal-node .result { font-size: 10px; color: #4b5563; font-style: italic; line-height: 1.3; }
.goal-node .footer { font-size: 9px; color: #9ca3af; margin-top: 6px; }

.reasoning-node { padding: 10px 12px; }
.reasoning-node .type-badge { font-size: 9px; font-weight: 700; letter-spacing: 0.5px; padding: 2px 5px; border-radius: 3px; margin-bottom: 4px; display: inline-block; }
.reasoning-node .type-badge.ground_truth { background: #bfdbfe; color: #1e40af; }
.reasoning-node .type-badge.insight { background: #fde68a; color: #92400e; }
.reasoning-node .content { font-size: 11px; color: #374151; line-height: 1.3; }
.reasoning-node .meta { font-size: 9px; color: #6b7280; margin-top: 4px; }

.activity-node { padding: 10px 12px; }
.activity-node .title { font-size: 11px; font-weight: 600; margin-bottom: 4px; }
.activity-node .ops { font-size: 9px; color: #4b5563; }
.activity-node .ops div { padding: 1px 0; white-space: nowrap; overflow: hidden; text-overflow: ellipsis; }

.cost-node { padding: 10px 12px; }
.cost-node .label { font-size: 11px; font-weight: 600; }
.cost-node .cost { font-size: 13px; font-weight: 700; color: #059669; }
</style>
</head>
<body>
<div id="root"></div>
<script type="module">
import React from 'https://esm.sh/react@18.3.1';
import { createRoot } from 'https://esm.sh/react-dom@18.3.1/client?deps=react@18.3.1';
import { ReactFlow, Background, MiniMap, ReactFlowProvider, useNodesState, useEdgesState, Handle, Position } from 'https://esm.sh/@xyflow/react@12.6.0?deps=react@18.3.1,react-dom@18.3.1';
import dagre from 'https://esm.sh/dagre@0.8.5';

const inputData = __DATA_JSON__;

// Custom node components
function GoalNodeComponent({ data }) {
  const catIcons = { EXPLORE: '🔍', THINK: '💭', ACT: '⚡' };
  return React.createElement('div', { className: 'goal-node' },
    React.createElement(Handle, { type: 'target', position: Position.Top, id: 'top', style: { background: '#555' } }),
    React.createElement(Handle, { type: 'target', position: Position.Left, id: 'left', style: { background: '#22c55e', width: 6, height: 6, opacity: 0 } }),
    React.createElement('div', { className: 'header' },
      React.createElement('span', { className: 'badge' }, `${catIcons[data.category] || ''} ${data.category}`),
      React.createElement('span', { className: 'node-id' }, data.nodeId),
    ),
    React.createElement('div', { className: 'label' }, data.label),
    data.result && React.createElement('div', { className: 'result' }, `→ ${data.result}`),
    React.createElement('div', { className: 'footer' }, `Steps ${data.stepRange[0]}–${data.stepRange[1]}`),
    React.createElement(Handle, { type: 'source', position: Position.Bottom, id: 'bottom', style: { background: '#555' } }),
    React.createElement(Handle, { type: 'source', position: Position.Right, id: 'right', style: { background: '#22c55e', width: 6, height: 6, opacity: 0 } }),
  );
}

function ReasoningNodeComponent({ data }) {
  return React.createElement('div', { className: 'reasoning-node' },
    React.createElement(Handle, { type: 'target', position: Position.Top, style: { background: '#555' } }),
    React.createElement('span', { className: `type-badge ${data.nodeType}` },
      data.nodeType === 'ground_truth' ? 'FACT' : 'HYPOTHESIS'),
    React.createElement('div', { className: 'content' }, data.content),
    React.createElement('div', { className: 'meta' },
      `${data.status || 'pending'} · conf: ${(data.confidence * 100).toFixed(0)}%`),
    React.createElement(Handle, { type: 'source', position: Position.Bottom, style: { background: '#555' } }),
  );
}

function ActivityNodeComponent({ data }) {
  return React.createElement('div', { className: 'activity-node' },
    React.createElement(Handle, { type: 'target', position: Position.Top, style: { background: '#555' } }),
    React.createElement('div', { className: 'title' }, `${data.label} (${data.opsCount} ops)`),
    React.createElement('div', { className: 'ops' },
      ...data.details.filter(d => d).map((d, i) =>
        React.createElement('div', { key: i }, d)
      )
    ),
    React.createElement(Handle, { type: 'source', position: Position.Bottom, style: { background: '#555' } }),
  );
}

function CostNodeComponent({ data }) {
  return React.createElement('div', { className: 'cost-node' },
    React.createElement(Handle, { type: 'target', position: Position.Top, style: { background: '#555' } }),
    React.createElement('div', { className: 'label' }, data.label),
    React.createElement('div', { className: 'cost' }, `$${data.cost.toFixed(4)}`),
    React.createElement(Handle, { type: 'source', position: Position.Bottom, style: { background: '#555' } }),
  );
}

const nodeTypes = {
  goalNode: GoalNodeComponent,
  reasoningNode: ReasoningNodeComponent,
  activityNode: ActivityNodeComponent,
  costNode: CostNodeComponent,
};

// Layout: for goal trees, build a proper tree layout using only sub edges for hierarchy.
// For other graph types, use standard dagre.
function applyLayout(nodes, edges) {
  if (inputData.graphType === 'goal_tree') {
    return treeLayout(nodes, edges);
  }
  // Standard dagre for DAGs
  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir: 'TB', nodesep: 50, ranksep: 80, marginx: 40, marginy: 40 });
  nodes.forEach(n => { g.setNode(n.id, { width: n.style?.width || 240, height: 100 }); });
  edges.forEach(e => { g.setEdge(e.source, e.target); });
  dagre.layout(g);
  return nodes.map(n => {
    const pos = g.node(n.id);
    if (!pos) return n;
    return { ...n, position: { x: pos.x - (n.style?.width || 240) / 2, y: pos.y - 50 } };
  });
}

// Tree layout for goal trees: parent-child via "sub" edges, siblings ordered by "next" edges.
function treeLayout(nodes, edges) {
  const NODE_W = 260;
  const NODE_H = 120;
  const H_GAP = 60;
  const V_GAP = 80;

  // Build parent→children map from "sub" edges
  const children = {};  // parentId → [childId, ...]
  const nextMap = {};   // nodeId → nextNodeId (sibling order)
  edges.forEach(e => {
    if (e.data?.edgeType === 'sub' || e.id?.includes('sub') ||
        inputData.edges.find(ie => ie.id === e.id)?.style?.stroke === '#3b82f6') {
      // This is a sub edge (blue)
      if (!children[e.source]) children[e.source] = [];
      children[e.source].push(e.target);
    }
  });

  // Parse edge types by color (set during goal_tree_to_rf)
  const subEdges = inputData.edges.filter(e => !e.animated && e.style?.stroke === '#3b82f6');
  const nextEdges = inputData.edges.filter(e => !e.animated && e.style?.stroke === '#22c55e');

  // Build tree: "sub" edges define parent→first_child.
  // "next" edges define sibling chains UNDER the same parent.
  // So parent's children = first_child + all nodes reachable via "next" from first_child.
  const nextOf = {};
  nextEdges.forEach(e => { nextOf[e.source] = e.target; });

  // For each sub edge, follow the next chain to find ALL children of that parent
  const childrenOf = {};
  subEdges.forEach(e => {
    const parent = e.source;
    if (!childrenOf[parent]) childrenOf[parent] = [];
    // Follow next chain from this first child
    let current = e.target;
    while (current) {
      if (!childrenOf[parent].includes(current)) {
        childrenOf[parent].push(current);
      }
      current = nextOf[current] || null;
    }
  });

  function orderSiblings(siblingIds) {
    if (siblingIds.length <= 1) return siblingIds;
    // Find the first sibling (not a target of any next edge within this set)
    const targets = new Set(siblingIds.filter(id => nextOf[id]).map(id => nextOf[id]));
    const allIds = new Set(siblingIds);
    let first = siblingIds.find(id => {
      // First = not pointed to by any next edge from this sibling set
      return !siblingIds.some(other => nextOf[other] === id);
    }) || siblingIds[0];

    const ordered = [first];
    let current = first;
    while (nextOf[current] && allIds.has(nextOf[current])) {
      current = nextOf[current];
      ordered.push(current);
    }
    // Add any remaining not in chain
    siblingIds.forEach(id => { if (!ordered.includes(id)) ordered.push(id); });
    return ordered;
  }

  // Compute subtree widths (number of leaves)
  const subtreeWidth = {};
  function computeWidth(nodeId) {
    const kids = childrenOf[nodeId] || [];
    if (kids.length === 0) {
      subtreeWidth[nodeId] = 1;
      return 1;
    }
    let total = 0;
    kids.forEach(kid => { total += computeWidth(kid); });
    subtreeWidth[nodeId] = total;
    return total;
  }

  // Find root
  const root = nodes.find(n => n.id === 'ROOT') ? 'ROOT' : nodes[0]?.id;
  computeWidth(root);

  // Position nodes
  const positions = {};
  function layout(nodeId, x, y) {
    positions[nodeId] = { x, y };
    const kids = orderSiblings(childrenOf[nodeId] || []);
    if (kids.length === 0) return;

    // Total width of children subtrees
    const totalLeaves = kids.reduce((sum, kid) => sum + (subtreeWidth[kid] || 1), 0);
    const totalWidth = totalLeaves * (NODE_W + H_GAP) - H_GAP;

    let childX = x - totalWidth / 2;
    kids.forEach(kid => {
      const kidWidth = (subtreeWidth[kid] || 1) * (NODE_W + H_GAP) - H_GAP;
      const kidCenterX = childX + kidWidth / 2;
      layout(kid, kidCenterX, y + NODE_H + V_GAP);
      childX += kidWidth + H_GAP;
    });
  }

  layout(root, 0, 0);

  return nodes.map(n => {
    const pos = positions[n.id] || { x: 0, y: 0 };
    return { ...n, position: { x: pos.x - NODE_W / 2, y: pos.y } };
  });
}

const layoutNodes = applyLayout(inputData.nodes, inputData.edges);

// For goal trees:
// - sub (blue): parent→child, bottom to top (default bezier)
// - next (green): sibling→sibling, right to left (straight/short)
// - backtrack: hidden
const displayEdges = inputData.graphType === 'goal_tree'
  ? inputData.edges.filter(e => !e.animated).map(e => {
      if (e.style?.stroke === '#22c55e') {
        // Sibling edge: right anchor → left anchor (shortest horizontal connection)
        return { ...e, type: 'straight', sourceHandle: 'right', targetHandle: 'left',
                 style: { stroke: '#9ca3af', strokeWidth: 1.5 }, markerEnd: { type: 'arrowclosed', color: '#9ca3af' } };
      }
      // Parent→child: default bezier, bottom → top
      return { ...e, type: 'default' };
    })
  : inputData.edges;

function App() {
  const [nodes] = useNodesState(layoutNodes);
  const [edges] = useEdgesState(displayEdges);

  return React.createElement(ReactFlowProvider, null,
    React.createElement('div', { style: { width: '100vw', height: '100vh' } },
      React.createElement(ReactFlow, {
        nodes, edges, nodeTypes, fitView: true,
        minZoom: 0.1, maxZoom: 2,
        nodesDraggable: false, nodesConnectable: false, elementsSelectable: false,
        defaultEdgeOptions: { type: 'smoothstep' },
        proOptions: { hideAttribution: true },
      },
        React.createElement(Background, { color: '#e5e7eb', gap: 20 }),
        React.createElement(MiniMap, { style: { bottom: 10, right: 10 }, nodeColor: n => n.style?.background || '#fff' }),
      )
    )
  );
}

const root = createRoot(document.getElementById('root'));
root.render(React.createElement(App));

// Signal ready for screenshot
setTimeout(() => { window.__READY__ = true; }, 2000);
</script>
</body>
</html>"""


def render(
    igr_path: str,
    output_path: str,
    width: int = 1600,
    height: int = 1000,
    wait_ms: int = 5000,
):
    """Render IGR file to PNG via React Flow in headless browser."""
    try:
        from playwright.sync_api import sync_playwright
    except ImportError:
        print(
            "ERROR: playwright not installed. Run: pip install playwright && playwright install chromium",
            file=sys.stderr,
        )
        sys.exit(1)

    data_json = igr_to_reactflow_json(igr_path)
    html = HTML_TEMPLATE.replace("__DATA_JSON__", data_json)

    with tempfile.NamedTemporaryFile(suffix=".html", mode="w", delete=False) as f:
        f.write(html)
        html_path = f.name

    try:
        with sync_playwright() as p:
            browser = p.chromium.launch()
            page = browser.new_page(viewport={"width": width, "height": height})
            page.goto(f"file://{html_path}")
            page.wait_for_timeout(wait_ms)
            page.screenshot(path=output_path)
            browser.close()
    finally:
        Path(html_path).unlink(missing_ok=True)

    size_kb = Path(output_path).stat().st_size // 1024
    print(f"✓ {output_path} ({size_kb}KB)")


def render_all(directory: str, output_dir: str = None, **kwargs):
    """Render all .igr.toml files in a directory tree."""
    dir_path = Path(directory)
    if output_dir:
        out_base = Path(output_dir)
    else:
        out_base = dir_path

    igr_files = list(dir_path.rglob("*.igr.toml"))
    print(f"Found {len(igr_files)} IGR files in {directory}")

    for igr in igr_files:
        rel = igr.relative_to(dir_path)
        png_path = out_base / rel.with_suffix(".rf.png")
        png_path.parent.mkdir(parents=True, exist_ok=True)
        try:
            render(str(igr), str(png_path), **kwargs)
        except Exception as e:
            print(f"  ✗ {igr}: {e}", file=sys.stderr)


def main():
    parser = argparse.ArgumentParser(
        description="Render IGR TOML to PNG via React Flow (modern @xyflow/react v12)"
    )
    parser.add_argument("input", help="Input .igr.toml file or directory (with --all)")
    parser.add_argument(
        "output", nargs="?", help="Output PNG path (auto-generated if omitted)"
    )
    parser.add_argument(
        "--all", action="store_true", help="Render all IGR files in the directory"
    )
    parser.add_argument(
        "--width", type=int, default=1600, help="Viewport width (default: 1600)"
    )
    parser.add_argument(
        "--height", type=int, default=1000, help="Viewport height (default: 1000)"
    )
    parser.add_argument(
        "--wait", type=int, default=5000, help="Wait ms for rendering (default: 5000)"
    )
    parser.add_argument("--output-dir", help="Output directory for --all mode")
    args = parser.parse_args()

    if args.all:
        render_all(
            args.input,
            args.output_dir,
            width=args.width,
            height=args.height,
            wait_ms=args.wait,
        )
    else:
        output = args.output or str(Path(args.input).with_suffix(".rf.png"))
        render(args.input, output, args.width, args.height, args.wait)


if __name__ == "__main__":
    main()
