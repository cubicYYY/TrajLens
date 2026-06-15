import TOML from "@iarna/toml";
import type { ActivityGraph, IGRGraph } from "./types";

export function parseIGR(tomlString: string): IGRGraph {
  const data = TOML.parse(tomlString) as unknown as IGRGraph;

  if (data.graph_type === "activity_graph") {
    const ag = data as ActivityGraph;
    for (const node of ag.nodes) {
      if (!node.parent_id) {
        node.parent_id = null;
      }
    }
  }

  return data;
}
