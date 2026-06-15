import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { ActivityNode as ActivityNodeData } from "../../types";

const CATEGORY_FILLS: Record<string, string> = {
  read: "#e3f2fd",
  write: "#fce4ec",
  edit: "#fff3e0",
  list: "#f3e5f5",
  run: "#e8f5e9",
  other: "#f5f5f5",
};

type Props = NodeProps & { data: ActivityNodeData };

export function ActivityNodeComponent({ data }: Props) {
  const bg = CATEGORY_FILLS[data.goal_category] ?? "#f5f5f5";

  return (
    <div
      style={{
        background: bg,
        border: "1.5px solid #333",
        borderRadius: 6,
        minWidth: 220,
        maxWidth: 420,
        fontFamily: "Courier, monospace",
        fontSize: 10,
        boxShadow: "0 2px 8px rgba(0,0,0,0.1)",
        cursor: "pointer",
        overflow: "hidden",
      }}
    >
      <Handle type="target" position={Position.Top} style={{ opacity: 0 }} />
      <div
        style={{
          padding: "4px 10px",
          background: "rgba(0,0,0,0.04)",
          borderBottom: "1px solid #ccc",
          fontFamily: "Helvetica, sans-serif",
          fontWeight: "bold",
          fontSize: 9,
          color: "#666",
          letterSpacing: 0.5,
        }}
      >
        {data.node_id}
      </div>
      <div
        style={{
          padding: "6px 10px",
          fontFamily: "Helvetica, sans-serif",
          fontWeight: "bold",
          fontSize: 11,
          borderBottom: "1px solid #999",
        }}
      >
        {data.label} [{data.goal_category}] — {data.operations.length} ops, $
        {data.total_cost.dollar_cost.toFixed(4)}
      </div>
      {data.operations.map((op, i) => (
        <div
          key={i}
          style={{
            padding: "3px 10px",
            background: i % 2 === 1 ? "rgba(0,0,0,0.03)" : "transparent",
          }}
        >
          #{String(op.call_index).padStart(2, "0")} [{op.op_type}] {op.detail}
        </div>
      ))}
      <Handle type="source" position={Position.Bottom} style={{ opacity: 0 }} />
    </div>
  );
}
