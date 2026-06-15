import type { IGRGraph } from "../types";
import { GoalTreeView } from "./GoalTreeView";
import { ReasoningDAGView } from "./ReasoningDAGView";
import { ActivityGraphView } from "./ActivityGraphView";
import { CostMapView } from "./CostMapView";

interface Props {
  graph: IGRGraph;
}

/**
 * Main viewer component. Auto-detects graph type and renders the appropriate view.
 * This is the primary export for embedding in any React application.
 */
export function TrajLensViewer({ graph }: Props) {
  switch (graph.graph_type) {
    case "goal_transition_tree":
      return <GoalTreeView graph={graph} />;
    case "reasoning_artifact_dag":
      return <ReasoningDAGView graph={graph} />;
    case "activity_graph":
      return <ActivityGraphView graph={graph} />;
    case "cost_map":
      return <CostMapView graph={graph} />;
    default:
      return <div style={{ padding: 20 }}>Unknown graph type</div>;
  }
}
