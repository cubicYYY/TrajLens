import type { Node } from "@xyflow/react";

interface Props {
  node: Node | null;
  onClose: () => void;
}

/**
 * Side panel that appears when a node is clicked.
 * Displays node ID as header and "TODO" as placeholder content.
 */
export function DetailPanel({ node, onClose }: Props) {
  if (!node) return null;

  return (
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
        <span style={{ fontWeight: "bold", fontSize: 13 }}>{node.id}</span>
        <button
          onClick={onClose}
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
  );
}
