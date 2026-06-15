export interface Cost {
  input_tokens: number;
  output_tokens: number;
  cache_read_tokens: number;
  cache_write_tokens: number;
  dollar_cost: number;
}

export interface GoalNode {
  node_id: string;
  label: string;
  goal_type: "explore" | "write" | "verify";
  status: "done" | "failed" | "abandoned" | "wip";
  level: number;
  step_range: [number, number];
  cost: Cost;
  reasoning_artifacts: string[];
}

export interface GoalEdge {
  edge_type: "next" | "backtrack" | "sub";
  source_id: string;
  target_id: string;
  label: string;
}

export interface GoalTransitionTree {
  graph_type: "goal_transition_tree";
  root_id: string;
  nodes: GoalNode[];
  edges: GoalEdge[];
}

export interface ReasoningArtifactNode {
  node_id: string;
  node_type: "ground_truth" | "insight";
  content: string;
  source_step_id: number;
  confidence: number;
  status: string;
}

export interface ReasoningEdge {
  edge_type: "infers" | "contradicts" | "supersedes";
  source_ids: string[];
  target_id: string;
}

export interface ReasoningArtifactDAG {
  graph_type: "reasoning_artifact_dag";
  nodes: ReasoningArtifactNode[];
  edges: ReasoningEdge[];
}

export interface Operation {
  op_type: string;
  detail: string;
  call_index: number;
}

export interface ActivityNode {
  node_id: string;
  label: string;
  goal_category: string;
  primary_object: string;
  parent_id: string | null;
  call_indices: number[];
  operations: Operation[];
  total_cost: Cost;
}

export interface ActivityEdge {
  edge_type: "next";
  source_id: string;
  source_operation_index: number;
  target_id: string;
  target_operation_index: number;
}

export interface ActivityGraph {
  graph_type: "activity_graph";
  nodes: ActivityNode[];
  edges: ActivityEdge[];
}

export interface CostMapNode {
  node_id: string;
  label: string;
  cost: Cost;
  category: string;
  children: CostMapNode[];
}

export interface CostMap {
  graph_type: "cost_map";
  root: CostMapNode;
}

export type IGRGraph = GoalTransitionTree | ReasoningArtifactDAG | ActivityGraph | CostMap;
