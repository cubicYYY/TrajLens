import { Handle, Position, type NodeProps } from "@xyflow/react";

export function JunctionNodeComponent(_props: NodeProps) {
  return (
    <div
      style={{
        width: 12,
        height: 12,
        borderRadius: "50%",
        background: "#555",
      }}
    >
      <Handle type="target" position={Position.Top} style={{ opacity: 0 }} />
      <Handle type="source" position={Position.Bottom} style={{ opacity: 0 }} />
    </div>
  );
}
