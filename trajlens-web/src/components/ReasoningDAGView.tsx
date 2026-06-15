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
import type { ReasoningArtifactDAG } from "../types";
import { applyDagreLayout } from "../layout";
import { ReasoningNodeComponent } from "./nodes/ReasoningNode";
import { JunctionNodeComponent } from "./nodes/JunctionNode";
import { DetailPanel } from "./DetailPanel";

const nodeTypes = {
  reasoningNode: ReasoningNodeComponent,
  junctionNode: JunctionNodeComponent,
};

const EDGE_STYLES: Record<string, { stroke: string; strokeDasharray?: string }> = {
  infers: { stroke: "#1565c0" },
  contradicts: { stroke: "#d32f2f", strokeDasharray: "6 4" },
  supersedes: { stroke: "#ff8f00", strokeDasharray: "6 4" },
};

interface Props {
  graph: ReasoningArtifactDAG;
}

export function ReasoningDAGView({ graph }: Props) {
  const [selectedNode, setSelectedNode] = useState<Node | null>(null);

  const { initialNodes, initialEdges } = useMemo(() => {
    const nodes: Node[] = graph.nodes.map((n) => ({
      id: n.node_id,
      type: "reasoningNode",
      position: { x: 0, y: 0 },
      data: n as unknown as Record<string, unknown>,
      width: Math.min(Math.max(n.content.length * 6, 180), 320),
      height: 72,
    }));

    const edges: Edge[] = [];
    let junctionCount = 0;

    for (const edge of graph.edges) {
      const style = EDGE_STYLES[edge.edge_type] ?? { stroke: "#333" };
      const validSources = edge.source_ids.filter((sid) =>
        graph.nodes.some((n) => n.node_id === sid)
      );
      if (validSources.length === 0) continue;

      if (validSources.length === 1) {
        edges.push({
          id: `e-${validSources[0]}-${edge.target_id}`,
          source: validSources[0],
          target: edge.target_id,
          style: { ...style, strokeWidth: 1.5 },
          animated: true,
        });
      } else {
        const jId = `_junction_${junctionCount++}`;
        nodes.push({
          id: jId,
          type: "junctionNode",
          position: { x: 0, y: 0 },
          data: {} as Record<string, unknown>,
          width: 12,
          height: 12,
        });
        for (const sid of validSources) {
          edges.push({
            id: `e-${sid}-${jId}`,
            source: sid,
            target: jId,
            style: { ...style, strokeWidth: 1.5 },
            animated: true,
            markerEnd: undefined,
          });
        }
        edges.push({
          id: `e-${jId}-${edge.target_id}`,
          source: jId,
          target: edge.target_id,
          style: { ...style, strokeWidth: 2 },
          animated: true,
        });
      }
    }

    const laid = applyDagreLayout(nodes, edges, { rankdir: "TB", nodesep: 50, ranksep: 70 });
    return { initialNodes: laid.nodes, initialEdges: laid.edges };
  }, [graph]);

  const [nodes, , onNodesChange] = useNodesState(initialNodes);
  const [rfEdges, , onEdgesChange] = useEdgesState(initialEdges);

  const onNodeClick: NodeMouseHandler = useCallback((_, node) => {
    if (node.type === "junctionNode") return;
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
