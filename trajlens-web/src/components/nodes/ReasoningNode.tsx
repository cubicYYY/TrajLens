import { Handle, Position, type NodeProps } from "@xyflow/react";
import type { ReasoningArtifactNode } from "../../types";

const TYPE_FILLS: Record<string, string> = {
  ground_truth: "#e8f5e9",
  insight: "#e3f2fd",
};

const STATUS_BORDERS: Record<string, string> = {
  verified: "#2e7d32",
  "self-falsed": "#c62828",
  unverified: "#757575",
};

type Props = NodeProps & { data: ReasoningArtifactNode };

export function ReasoningNodeComponent({ data }: Props) {
  const fill = TYPE_FILLS[data.node_type] ?? "#f5f5f5";
  const border = data.status ? (STATUS_BORDERS[data.status] ?? "#333") : "#333";

  return (
    <div
      style={{
        background: fill,
        border: `2px solid ${border}`,
        borderRadius: 8,
        minWidth: 150,
        maxWidth: 320,
        fontFamily: "Helvetica, sans-serif",
        boxShadow: "0 2px 8px rgba(0,0,0,0.1)",
        cursor: "pointer",
        overflow: "hidden",
      }}
    >
      <Handle type="target" position={Position.Top} style={{ opacity: 0 }} />
      <div
        style={{
          padding: "4px 12px",
          background: "rgba(0,0,0,0.04)",
          borderBottom: `1px solid ${border}`,
          fontWeight: "bold",
          fontSize: 9,
          color: "#666",
          letterSpacing: 0.5,
        }}
      >
        {data.node_id}
      </div>
      <div style={{ padding: "8px 12px" }}>
        <div style={{ fontSize: 11, color: "#222", marginBottom: 4 }}>{data.content}</div>
        <div style={{ fontSize: 9, color: "#666" }}>
          [{data.node_type}] conf={data.confidence.toFixed(2)} |{" "}
          {data.status || "source"}
        </div>
      </div>
      <Handle type="source" position={Position.Bottom} style={{ opacity: 0 }} />
    </div>
  );
}
