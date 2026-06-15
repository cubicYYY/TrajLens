import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { GoalNode as GoalNodeData } from "../../types";

const TYPE_FILLS: Record<string, string> = {
  explore: "#e3f2fd",
  write: "#fff3e0",
  verify: "#f3e5f5",
};

const STATUS_BORDERS: Record<string, string> = {
  done: "#2e7d32",
  failed: "#c62828",
  abandoned: "#f9a825",
  wip: "#1565c0",
};

type Props = NodeProps & { data: GoalNodeData };

export function GoalNodeComponent({ data }: Props) {
  const fill = TYPE_FILLS[data.goal_type] ?? "#e0e0e0";
  const border = STATUS_BORDERS[data.status] ?? "#333";

  return (
    <div
      style={{
        background: fill,
        border: `2px solid ${border}`,
        borderRadius: 8,
        minWidth: 160,
        maxWidth: 300,
        fontFamily: "Helvetica, sans-serif",
        boxShadow: "0 2px 8px rgba(0,0,0,0.1)",
        cursor: "pointer",
        overflow: "hidden",
      }}
    >
      <Handle type="target" position={Position.Top} style={{ opacity: 0 }} />
      <div
        style={{
          padding: "6px 12px",
          background: "rgba(0,0,0,0.05)",
          borderBottom: `1px solid ${border}`,
          fontWeight: "bold",
          fontSize: 10,
          color: "#555",
          letterSpacing: 0.5,
        }}
      >
        {data.node_id}
      </div>
      <div style={{ padding: "8px 12px" }}>
        <div style={{ fontWeight: "bold", fontSize: 12, color: "#222", marginBottom: 4 }}>
          {data.label}
        </div>
        <div style={{ fontSize: 9, color: "#666" }}>
          [{data.goal_type}:{data.status}] steps {data.step_range[0]}-{data.step_range[1]} | $
          {data.cost.dollar_cost.toFixed(4)}
        </div>
      </div>
      <Handle type="source" position={Position.Bottom} style={{ opacity: 0 }} />
    </div>
  );
}
