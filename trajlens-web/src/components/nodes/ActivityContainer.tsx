import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { ActivityNode as ActivityNodeData } from "../../types";

type Props = NodeProps & { data: ActivityNodeData };

/**
 * Container node for hierarchical activity graph.
 * Renders as a card with title header that holds child nodes inside it.
 */
export function ActivityContainerComponent({ data }: Props) {
  return (
    <div
      style={{
        background: "#fafafa",
        border: "2px solid #666",
        borderRadius: 6,
        width: "100%",
        height: "100%",
        fontFamily: "Helvetica, sans-serif",
        fontSize: 10,
        boxShadow: "0 2px 8px rgba(0,0,0,0.08)",
        cursor: "pointer",
        overflow: "hidden",
      }}
    >
      <Handle type="target" position={Position.Top} style={{ opacity: 0 }} />
      <div
        style={{
          padding: "3px 10px",
          background: "rgba(0,0,0,0.04)",
          borderBottom: "1px solid #ddd",
          fontWeight: "bold",
          fontSize: 9,
          color: "#888",
          letterSpacing: 0.5,
        }}
      >
        {data.node_id}
      </div>
      <div
        style={{
          padding: "4px 10px",
          fontWeight: "bold",
          fontSize: 10,
          color: "#555",
          borderBottom: "1px solid #ccc",
        }}
      >
        {data.label} [{data.goal_category}]
      </div>
      {data.operations.length > 0 && (
        <div style={{ fontFamily: "Courier, monospace", fontSize: 9, padding: "2px 10px" }}>
          {data.operations.map((op, i) => (
            <div
              key={i}
              style={{
                padding: "2px 0",
                background: i % 2 === 1 ? "rgba(0,0,0,0.03)" : "transparent",
              }}
            >
              #{String(op.call_index).padStart(2, "0")} [{op.op_type}] {op.detail}
            </div>
          ))}
        </div>
      )}
      <Handle type="source" position={Position.Bottom} style={{ opacity: 0 }} />
    </div>
  );
}
