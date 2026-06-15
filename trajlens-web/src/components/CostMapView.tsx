import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import type { CostMap, CostMapNode } from "../types";

interface Props {
  graph: CostMap;
}

interface SelectedData {
  node: CostMapNode;
}

/**
 * Cost Map rendered as a nested treemap using d3-hierarchy's squarified tiling.
 * Based on https://observablehq.com/@d3/treemap-component
 */
export function CostMapView({ graph }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<SelectedData | null>(null);
  const [tooltip, setTooltip] = useState<{ node: CostMapNode; x: number; y: number } | null>(null);

  useEffect(() => {
    if (!svgRef.current || !containerRef.current) return;

    const container = containerRef.current;
    const width = container.clientWidth || 1200;
    const height = container.clientHeight || 700;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();
    svg.attr("width", width).attr("height", height).attr("viewBox", `0 0 ${width} ${height}`);

    // Convert CostMapNode tree into d3 hierarchy format
    const hierarchyData = toD3Hierarchy(graph.root);
    const root = d3.hierarchy(hierarchyData)
      .sum((d) => d.value ?? 0)
      .sort((a, b) => (b.value ?? 0) - (a.value ?? 0));

    d3.treemap<D3Node>()
      .size([width, height])
      .paddingTop(24)
      .paddingRight(2)
      .paddingBottom(2)
      .paddingLeft(2)
      .paddingInner(2)
      .round(true)
      .tile(d3.treemapSquarify)(root);

    const color = d3.scaleOrdinal<string>()
      .domain(["read", "write", "edit", "run", "think", "event", "other", ""])
      .range(["#bbdefb", "#f8bbd0", "#ffe0b2", "#b2dfdb", "#d1c4e9", "#fff9c4", "#e0e0e0", "#e8eaf6"]);

    const depthOpacity = (depth: number) => Math.max(1.0 - depth * 0.15, 0.4);

    // Render cells
    const cell = svg.selectAll("g")
      .data(root.descendants())
      .join("g")
      .attr("transform", (d) => `translate(${d.x0},${d.y0})`);

    // Rectangles
    cell.append("rect")
      .attr("width", (d) => Math.max(d.x1 - d.x0, 0))
      .attr("height", (d) => Math.max(d.y1 - d.y0, 0))
      .attr("fill", (d) => {
        const cat = d.data.category || "";
        return color(cat);
      })
      .attr("fill-opacity", (d) => depthOpacity(d.depth))
      .attr("stroke", "#555")
      .attr("stroke-width", (d) => Math.max(1.5 - d.depth * 0.4, 0.3))
      .style("cursor", "pointer")
      .on("click", (_, d) => {
        if (d.data.original) {
          setSelected({ node: d.data.original });
        }
      })
      .on("mouseenter", (event, d) => {
        if (d.data.original) {
          setTooltip({ node: d.data.original, x: event.clientX, y: event.clientY });
        }
      })
      .on("mouseleave", () => setTooltip(null));

    // Labels: node_id + label
    cell.append("clipPath")
      .attr("id", (d) => `clip-${d.data.id}`)
      .append("rect")
      .attr("width", (d) => Math.max(d.x1 - d.x0, 0))
      .attr("height", (d) => Math.max(d.y1 - d.y0, 0));

    // Title text (node_id + label) — only for nodes with enough space
    cell.filter((d) => (d.x1 - d.x0) > 40 && (d.y1 - d.y0) > 20)
      .append("text")
      .attr("clip-path", (d) => `url(#clip-${d.data.id})`)
      .attr("x", 4)
      .attr("y", 14)
      .attr("font-size", "11px")
      .attr("font-weight", "bold")
      .attr("font-family", "sans-serif")
      .attr("fill", "#222")
      .style("pointer-events", "none")
      .text((d) => {
        const nodeId = d.data.nodeId;
        const label = d.data.label;
        if (!nodeId || nodeId === "root") return label;
        return `[${nodeId}] ${label}`;
      });

    // Cost text — only for nodes with enough space
    cell.filter((d) => (d.x1 - d.x0) > 60 && (d.y1 - d.y0) > 34)
      .append("text")
      .attr("clip-path", (d) => `url(#clip-${d.data.id})`)
      .attr("x", 4)
      .attr("y", 26)
      .attr("font-size", "9px")
      .attr("font-family", "sans-serif")
      .attr("fill", "#555")
      .style("pointer-events", "none")
      .text((d) => `$${d.data.cost.toFixed(4)}`);

  }, [graph]);

  return (
    <div
      ref={containerRef}
      style={{ position: "relative", width: "100%", height: "100%", overflow: "auto" }}
    >
      <svg ref={svgRef} />
      {tooltip && (
        <div
          style={{
            position: "fixed",
            left: tooltip.x + 12,
            top: tooltip.y + 12,
            background: "white",
            border: "1px solid #ccc",
            borderRadius: 4,
            padding: "6px 10px",
            fontSize: 11,
            boxShadow: "0 2px 8px rgba(0,0,0,0.15)",
            pointerEvents: "none",
            zIndex: 100,
          }}
        >
          <div style={{ fontWeight: "bold" }}>[{tooltip.node.node_id}] {tooltip.node.label}</div>
          <div>${tooltip.node.cost.dollar_cost.toFixed(4)}</div>
          <div>
            {tooltip.node.cost.input_tokens + tooltip.node.cost.output_tokens} tokens
          </div>
          {tooltip.node.category && <div>Category: {tooltip.node.category}</div>}
        </div>
      )}
      {selected && (
        <div
          style={{
            position: "absolute",
            top: 0,
            right: 0,
            width: 320,
            height: "100%",
            background: "#fff",
            borderLeft: "1px solid #ddd",
            boxShadow: "-4px 0 12px rgba(0,0,0,0.08)",
            zIndex: 200,
            display: "flex",
            flexDirection: "column",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              padding: "12px 16px",
              borderBottom: "1px solid #eee",
              display: "flex",
              justifyContent: "space-between",
              alignItems: "center",
            }}
          >
            <span style={{ fontWeight: "bold", fontSize: 13 }}>{selected.node.node_id}</span>
            <button
              onClick={() => setSelected(null)}
              style={{
                background: "none",
                border: "none",
                fontSize: 18,
                cursor: "pointer",
                color: "#666",
                lineHeight: 1,
              }}
            >
              ×
            </button>
          </div>
          <div style={{ padding: 16, flex: 1, overflowY: "auto", fontSize: 13, color: "#444" }}>
            TODO
          </div>
        </div>
      )}
    </div>
  );
}

interface D3Node {
  id: string;
  nodeId: string;
  label: string;
  category: string;
  cost: number;
  value?: number;
  original: CostMapNode;
  children?: D3Node[];
}

function toD3Hierarchy(node: CostMapNode): D3Node {
  const hasChildren = node.children && node.children.length > 0;
  return {
    id: node.node_id,
    nodeId: node.node_id,
    label: node.label,
    category: node.category || "",
    cost: node.cost.dollar_cost,
    original: node,
    value: hasChildren ? undefined : node.cost.dollar_cost,
    children: hasChildren
      ? node.children.map((child) => toD3Hierarchy(child))
      : undefined,
  };
}
