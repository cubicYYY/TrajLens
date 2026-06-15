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
import type { ActivityGraph } from "../types";
import { applyDagreLayout } from "../layout";
import { ActivityNodeComponent } from "./nodes/ActivityNode";
import { ActivityContainerComponent } from "./nodes/ActivityContainer";
import { DetailPanel } from "./DetailPanel";

const nodeTypes = {
  activityNode: ActivityNodeComponent,
  activityContainer: ActivityContainerComponent,
};

interface Props {
  graph: ActivityGraph;
}

export function ActivityGraphView({ graph }: Props) {
  const [selectedNode, setSelectedNode] = useState<Node | null>(null);

  const { initialNodes, initialEdges } = useMemo(() => {
    const childrenOf: Record<string, string[]> = {};
    for (const n of graph.nodes) {
      if (n.parent_id) {
        if (!childrenOf[n.parent_id]) childrenOf[n.parent_id] = [];
        childrenOf[n.parent_id].push(n.node_id);
      }
    }

    const isContainer = (id: string) => !!childrenOf[id];

    const nodes: Node[] = [];

    for (const n of graph.nodes) {
      const container = isContainer(n.node_id);
      const opHeight = n.operations.length * 22 + 8;
      const baseHeight = 40 + opHeight;

      if (container) {
        const kids = graph.nodes.filter(
          (k) => k.parent_id === n.node_id
        );
        const kidsHeight = kids.reduce(
          (sum, k) => sum + 40 + k.operations.length * 22 + 8 + 10,
          0
        );
        const width = 340;
        const height = 42 + opHeight + kidsHeight + 20;

        nodes.push({
          id: n.node_id,
          type: "activityContainer",
          position: { x: 0, y: 0 },
          data: n as unknown as Record<string, unknown>,
          width,
          height,
          style: { width, height },
        });
      } else if (n.parent_id) {
        nodes.push({
          id: n.node_id,
          type: "activityNode",
          position: { x: 10, y: 0 },
          data: n as unknown as Record<string, unknown>,
          parentId: n.parent_id,
          extent: "parent" as const,
          width: 300,
          height: baseHeight,
        });
      } else {
        nodes.push({
          id: n.node_id,
          type: "activityNode",
          position: { x: 0, y: 0 },
          data: n as unknown as Record<string, unknown>,
          width: 300,
          height: baseHeight,
        });
      }
    }

    for (const [parentId, kidIds] of Object.entries(childrenOf)) {
      const parentNode = nodes.find((nd) => nd.id === parentId);
      if (!parentNode) continue;
      const parentData = graph.nodes.find((n) => n.node_id === parentId);
      const parentOpsHeight = parentData
        ? parentData.operations.length * 22 + 8
        : 0;
      let yOffset = 42 + parentOpsHeight + 10;
      for (const kidId of kidIds) {
        const kidNode = nodes.find((nd) => nd.id === kidId);
        if (!kidNode) continue;
        kidNode.position = { x: 10, y: yOffset };
        yOffset += (kidNode.height ?? 50) + 10;
      }
    }

    const edges: Edge[] = graph.edges.map((e, i) => ({
      id: `e-${i}-${e.source_id}-${e.target_id}`,
      source: e.source_id,
      target: e.target_id,
      style: { stroke: "#666", strokeWidth: 1.5 },
      animated: true,
    }));

    const layoutNodes = nodes.filter((n) => !n.parentId);
    const laid = applyDagreLayout(layoutNodes, edges, {
      rankdir: "TB",
      nodesep: 50,
      ranksep: 60,
    });

    for (const laidNode of laid.nodes) {
      const original = nodes.find((n) => n.id === laidNode.id);
      if (original) {
        original.position = laidNode.position;
      }
    }

    return { initialNodes: nodes, initialEdges: edges };
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
