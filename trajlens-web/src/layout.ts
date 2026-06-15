import dagre from "dagre";
import type { Node, Edge } from "@xyflow/react";

interface LayoutOptions {
  rankdir?: "TB" | "LR";
  nodesep?: number;
  ranksep?: number;
}

/**
 * Apply dagre hierarchical layout to React Flow nodes and edges.
 * Mutates node positions in-place and returns the positioned arrays.
 */
export function applyDagreLayout(
  nodes: Node[],
  edges: Edge[],
  options: LayoutOptions = {}
): { nodes: Node[]; edges: Edge[] } {
  const { rankdir = "TB", nodesep = 60, ranksep = 80 } = options;

  const g = new dagre.graphlib.Graph();
  g.setDefaultEdgeLabel(() => ({}));
  g.setGraph({ rankdir, nodesep, ranksep });

  for (const node of nodes) {
    const width = node.measured?.width ?? node.width ?? 200;
    const height = node.measured?.height ?? node.height ?? 60;
    g.setNode(node.id, { width, height });
  }

  for (const edge of edges) {
    g.setEdge(edge.source, edge.target);
  }

  dagre.layout(g);

  const layoutedNodes = nodes.map((node) => {
    const dagreNode = g.node(node.id);
    const width = node.measured?.width ?? node.width ?? 200;
    const height = node.measured?.height ?? node.height ?? 60;
    return {
      ...node,
      position: {
        x: dagreNode.x - width / 2,
        y: dagreNode.y - height / 2,
      },
    };
  });

  return { nodes: layoutedNodes, edges };
}
