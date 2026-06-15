import { useCallback, useMemo, useState } from "react";
import {
  ReactFlow,
  MiniMap,
  Background,
  useNodesState,
  useEdgesState,
  type Node,
  type Edge,
  type NodeMouseHandler,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import type { GoalTransitionTree } from "../types";
import { applyDagreLayout } from "../layout";
import { GoalNodeComponent } from "./nodes/GoalNode";
import { DetailPanel } from "./DetailPanel";

const nodeTypes = { goalNode: GoalNodeComponent };

interface Props {
  graph: GoalTransitionTree;
}

export function GoalTreeView({ graph }: Props) {
  const [selectedNode, setSelectedNode] = useState<Node | null>(null);

  const { initialNodes, initialEdges } = useMemo(() => {
    const nextTargets = new Set(
      graph.edges.filter((e) => e.edge_type === "next").map((e) => e.target_id)
    );

    const parentOf: Record<string, string> = {};
    for (const e of graph.edges) {
      if (e.edge_type === "sub") {
        parentOf[e.target_id] = e.source_id;
      }
    }
    let changed = true;
    while (changed) {
      changed = false;
      for (const e of graph.edges) {
        if (e.edge_type === "next" && parentOf[e.source_id] && !parentOf[e.target_id]) {
          parentOf[e.target_id] = parentOf[e.source_id];
          changed = true;
        }
      }
    }

    const hasOutgoingNext = new Set(
      graph.edges.filter((e) => e.edge_type === "next").map((e) => e.source_id)
    );
    const nodeById = Object.fromEntries(graph.nodes.map((n) => [n.node_id, n]));

    const nodes: Node[] = graph.nodes.map((n) => ({
      id: n.node_id,
      type: "goalNode",
      position: { x: 0, y: 0 },
      data: n as unknown as Record<string, unknown>,
      width: Math.min(Math.max(n.label.length * 7 + 24, 160), 280),
      height: 72,
    }));

    const edges: Edge[] = [];

    for (const e of graph.edges) {
      if (e.edge_type === "sub") {
        const isStructural = nextTargets.has(e.target_id);
        edges.push({
          id: `sub-${e.source_id}-${e.target_id}`,
          source: e.source_id,
          target: e.target_id,
          label: isStructural ? undefined : e.label || undefined,
          style: isStructural
            ? { stroke: "#333", strokeWidth: 1.2, strokeDasharray: "6 4" }
            : { stroke: "#111", strokeWidth: 2.5 },
          animated: true,
        });
      }
    }

    for (const e of graph.edges) {
      if (e.edge_type === "next") {
        edges.push({
          id: `next-${e.source_id}-${e.target_id}`,
          source: e.source_id,
          target: e.target_id,
          label: e.label || undefined,
          style: { stroke: "#333", strokeWidth: 1.5 },
          animated: true,
        });
      }
    }

    for (const [childId, parId] of Object.entries(parentOf)) {
      const node = nodeById[childId];
      if (!node || (node.status !== "failed" && node.status !== "abandoned")) continue;
      edges.push({
        id: `fail-${childId}-${parId}`,
        source: childId,
        target: parId,
        label: node.status === "failed" ? "failed" : "abandoned",
        style: { stroke: "#d32f2f", strokeWidth: 2, strokeDasharray: "6 4" },
        type: "smoothstep",
        animated: true,
      });
    }

    const childrenMap: Record<string, string[]> = {};
    for (const [childId, parId] of Object.entries(parentOf)) {
      if (!childrenMap[parId]) childrenMap[parId] = [];
      childrenMap[parId].push(childId);
    }
    for (const [parId, kids] of Object.entries(childrenMap)) {
      const terminalDone = kids.filter(
        (k) => !hasOutgoingNext.has(k) && nodeById[k]?.status === "done"
      );
      if (terminalDone.length === 0) continue;
      const lastDone = terminalDone.reduce((a, b) =>
        (nodeById[a]?.step_range[1] ?? 0) >= (nodeById[b]?.step_range[1] ?? 0) ? a : b
      );
      edges.push({
        id: `done-${lastDone}-${parId}`,
        source: lastDone,
        target: parId,
        label: "completed",
        style: { stroke: "#2e7d32", strokeWidth: 2, strokeDasharray: "6 4" },
        type: "smoothstep",
        animated: true,
      });
    }

    const laid = applyDagreLayout(nodes, edges, { rankdir: "TB", nodesep: 60, ranksep: 80 });
    return { initialNodes: laid.nodes, initialEdges: laid.edges };
  }, [graph]);

  const [nodes, , onNodesChange] = useNodesState(initialNodes);
  const [rfEdges, , onEdgesChange] = useEdgesState(initialEdges);

  const onNodeClick: NodeMouseHandler = useCallback((_, node) => {
    setSelectedNode(node);
  }, []);

  return (
    <div style={{ width: "100%", height: "100%", position: "relative" }}>
      <ReactFlow
        nodes={nodes}
        edges={rfEdges}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onNodeClick={onNodeClick}
        nodeTypes={nodeTypes}
        fitView
        minZoom={0.2}
        maxZoom={3}
      >
        <Background />
        <MiniMap />
      </ReactFlow>
      <DetailPanel node={selectedNode} onClose={() => setSelectedNode(null)} />
    </div>
  );
}
