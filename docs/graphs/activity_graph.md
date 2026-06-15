# Activity Graph (G3)

Deterministic builder that shows what filesystem operations the agent performed, organized as a path hierarchy.

**File:** `trajlens/src/graphs/activity_graph.rs`

## Purpose

Shows what filesystem targets the agent operated on, grouped by operation type and path. Repetition is visible from call indices. No LLM required — fully deterministic.

## Construction Flow

1. Extract path targets from each action item (via `file_path` arg or regex on `command`)
2. Classify each item's goal category (`read`, `write`, `edit`, `run`, `list`, `other`)
3. Group operations by `(goal_category, target_path)` — same target+category = one node
4. Build directory hierarchy via longest-prefix path containment (`parent_id`)
5. Create edges between operations in chronological visit order

## Nodes

Each node represents a distinct `(goal_category, target_path)` pair.

| Attribute | Description |
|-----------|-------------|
| `node_id` | Unique identifier |
| `label` | Basename of target file/directory |
| `goal_category` | `read` / `write` / `edit` / `run` / `list` / `other` |
| `primary_object` | Full filesystem path |
| `parent_id` | Containing directory node (longest-prefix match) |
| `operations` | Ordered list: `(op_type, detail, call_index)` |
| `call_indices` | All raw step indices merged into this node |
| `total_cost` | Sum of costs across all visits |

### Category Classification

| Sub-category patterns | GoalCategory |
|----------------------|--------------|
| `read`, `read_file` | Read |
| `write`, `write_file` | Write |
| `edit`, `edit_file`, `patch` | Edit |
| `bash`, `run`, `exec`, `command` | Run |
| `glob`, `find`, `list`, `ls` | List |
| Everything else | Other |

### Path Hierarchy

Nodes form a tree by filesystem containment:
- `ls /workspace` → node "workspace" (root)
- `cat /workspace/src/main.rs` → node "main.rs" with `parent_id` → "src" → "workspace"

The longest-prefix path match among other nodes becomes a node's parent.

## Edges

| Attribute | Description |
|-----------|-------------|
| `source_id` | Source `(node_id, operation_index)` |
| `target_id` | Target `(node_id, operation_index)` |
| `edge_type` | Always `next` (temporal ordering) |

Edges encode chronological sequence between operations: operation N → operation N+1.

## Visual Rendering

- **Container nodes**: Labeled boxes containing child nodes
- **Leaf nodes**: Tables with header row (label, category, op count) + one row per operation
- **Color by category**: read=#e3f2fd, write=#fce4ec, edit=#fff3e0, run=#e8f5e9, list=#f3e5f5
- **No arrows rendered** — call_index on each operation row shows execution order

## CLI Usage

```bash
# Build from trajectory
trajlens build activity-graph trajectory.json -o activity-graph.igr.toml

# Render to SVG
trajlens render activity-graph.igr.toml -o activity-graph.svg

# All-in-one pipeline (includes activity graph)
trajlens run input.log -o output/
```

## Programmatic Usage

```rust
use trajlens::graphs::activity_graph;
use trajlens::models::Trajectory;

let trajectory: Trajectory = /* parsed */;
let graph = activity_graph::build(&trajectory);
// graph.nodes, graph.edges ready for IGR serialization or rendering
```

## Key Implementation Details

- Path extraction: checks `file_path` arg first, falls back to regex on `command` arg
- Only `Action`-category items produce nodes (Think/Event items are skipped)
- Operations within a node are sorted chronologically by `call_index`
- The operation sequence across all nodes forms a single linear chain

## Structural Invariants

- Only `Action`-category items from the trajectory produce nodes. Unknown items must be classified by the semantic processor first.
- The operation sequence forms a single linear chain (one connected component).
- Operations within a node are in chronological order by `call_index`.
