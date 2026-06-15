import { useState } from "react";
import { parseIGR } from "./igr-loader";
import { TrajLensViewer } from "./components/TrajLensViewer";
import type { IGRGraph } from "./types";

const SAMPLE_FILES = [
  "goal_tree.igr.toml",
  "reasoning_dag.igr.toml",
  "activity_graph.igr.toml",
  "cost_map.igr.toml",
  "cc_goal_tree.igr.toml",
  "cc_reasoning_dag.igr.toml",
  "cc_activity_graph.igr.toml",
  "cc_cost_map.igr.toml",
  "pocgen_goal_tree.igr.toml",
  "pocgen_activity_graph.igr.toml",
  "pocgen_cost_map.igr.toml",
  "g1_solved_activity.igr.toml",
  "g1_solved_costmap.igr.toml",
  "g1_failed_activity.igr.toml",
  "g1_failed_costmap.igr.toml",
];

export function App() {
  const [graph, setGraph] = useState<IGRGraph | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [currentFile, setCurrentFile] = useState<string | null>(null);

  async function loadFile(filename: string) {
    setLoading(true);
    setError(null);
    try {
      const resp = await fetch(`/output/${filename}`);
      if (!resp.ok) throw new Error(`Failed to load: ${resp.status}`);
      const text = await resp.text();
      const parsed = parseIGR(text);
      setGraph(parsed);
      setCurrentFile(filename);
    } catch (e) {
      setError(String(e));
      setGraph(null);
    } finally {
      setLoading(false);
    }
  }

  async function handleFileUpload(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    setLoading(true);
    setError(null);
    try {
      const text = await file.text();
      const parsed = parseIGR(text);
      setGraph(parsed);
      setCurrentFile(file.name);
    } catch (err) {
      setError(String(err));
      setGraph(null);
    } finally {
      setLoading(false);
    }
  }

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <header
        style={{
          padding: "8px 16px",
          borderBottom: "1px solid #ddd",
          display: "flex",
          alignItems: "center",
          gap: 12,
          flexShrink: 0,
        }}
      >
        <strong style={{ fontSize: 14 }}>TrajLens</strong>
        <select
          onChange={(e) => e.target.value && loadFile(e.target.value)}
          value={currentFile ?? ""}
          style={{ fontSize: 12, padding: "4px 8px" }}
        >
          <option value="">Select a graph...</option>
          {SAMPLE_FILES.map((f) => (
            <option key={f} value={f}>
              {f}
            </option>
          ))}
        </select>
        <label style={{ fontSize: 12, cursor: "pointer", color: "#1565c0" }}>
          Upload .igr.toml
          <input
            type="file"
            accept=".toml"
            onChange={handleFileUpload}
            style={{ display: "none" }}
          />
        </label>
        {currentFile && (
          <span style={{ fontSize: 11, color: "#666" }}>
            Viewing: {currentFile} ({graph?.graph_type})
          </span>
        )}
      </header>

      <div style={{ flex: 1, position: "relative" }}>
        {loading && (
          <div style={{ padding: 20, color: "#666" }}>Loading...</div>
        )}
        {error && (
          <div style={{ padding: 20, color: "#c62828" }}>{error}</div>
        )}
        {graph && !loading && <TrajLensViewer graph={graph} />}
        {!graph && !loading && !error && (
          <div style={{ padding: 40, color: "#999", textAlign: "center" }}>
            Select a graph file above or upload an .igr.toml file
          </div>
        )}
      </div>
    </div>
  );
}
