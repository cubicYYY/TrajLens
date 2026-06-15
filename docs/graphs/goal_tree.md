# Goal Transition Tree (G1)

LLM-based builder that extracts a hierarchical goal decomposition from agent trajectories.

**File:** `trajlens/src/graphs/goal_tree.rs`

## Purpose

Shows the agent's intent hierarchy over time — what it was trying to accomplish, how it decomposed tasks, and when it abandoned a plan and started a new one.

## Construction Flow

1. Build system prompt defining goal extraction task and JSON schema
2. Sample trajectory turns (first 3 + evenly spaced + last 3 + high-cost turns, max 20)
3. LLM produces hierarchical goal structure with nodes and edges
4. Post-processing: `add_missing_backtrack_edges`, `propagate_status`
5. Validation: `collect_anomalies` checks structural invariants
6. If anomalies found → multi-turn correction (retry with anomaly list, max 3 retries)

## Hierarchical ID Scheme

- Root: `"ROOT"`
- Children of ROOT: `"1"`, `"2"`, `"3"`, ...
- Grandchildren: `"2.1"`, `"2.2"`, ...
- Great-grandchildren: `"2.1.1"`, `"2.1.2"`, ...

No opaque IDs (g0, g1, ...) — LLM produces hierarchical IDs from the start.

## Nodes

| Attribute | Description |
|-----------|-------------|
| `node_id` | Hierarchical ID (e.g. "ROOT", "2.1") |
| `label` | Verb phrase describing intent |
| `status` | `done` / `failed` / `abandoned` / `wip` |
| `level` | Depth in tree (root=0) |
| `step_range` | Which trajectory steps this goal spans |
| `cost` | Total cost of steps in this range |

### Status Propagation

- If ALL children `failed`/`abandoned` → parent becomes `failed`
- If parent is `done` but has `failed` children → parent becomes `wip` (partial success)
- Leaf nodes keep LLM-assigned status unchanged

## Edges

| Type | From → To | Meaning | Visual |
|------|-----------|---------|--------|
| **Sub** ("new plan") | parent → child | Creates a sub-plan | Vertical solid arrow |
| **Next** | sibling → sibling | Sequential steps in a plan | Horizontal solid arrow |
| **Backtrack** | last-in-plan → parent | Returns control to parent | Dashed vertical (color by status) |

### Edge Constraints

- A node may have MULTIPLE outgoing Sub edges (multiple sub-plans)
- A node can have at most ONE outgoing Next edge
- Only the LAST node in a plan (no outgoing Next) has a Backtrack edge
- Sub+Next on same node is VALID
- Sub+Backtrack is VALID

## Multi-Turn Correction

When `collect_anomalies` detects issues, the builder:

1. Sends original trajectory context + broken JSON + anomaly list to LLM
2. LLM produces corrected output
3. Re-validates; repeat up to `max_retries` (default: 3, configurable)

Anomalies checked:
- Orphan nodes not reachable from ROOT
- Missing backtrack edges on last-in-plan nodes
- Multiple outgoing Next edges (max 1 allowed)
- Invalid IDs (must be "ROOT" or digits+dots)
- Step range containment violations (child outside parent's range)

## CLI Usage

```bash
trajlens build-llm goal-tree trajectory.json \
  -o goal-tree.igr.toml --model anthropic/claude-sonnet-4-6

# Default model
trajlens build-llm goal-tree trajectory.json -o goal-tree.igr.toml
```

## Programmatic Usage

```rust
use trajlens::llm::AnthropicClient;
use trajlens::graphs::goal_tree;

#[tokio::main]
async fn main() {
    let client = AnthropicClient::from_env().unwrap();
    let tree = goal_tree::build_with_llm(&trajectory, &client).await.unwrap();
    println!("{} goals extracted", tree.nodes.len());
}
```

## Performance

For typical 100-turn trajectory:
- Input: ~15,000 tokens
- Output: ~2,000 tokens
- Time: 5-15s (Sonnet 4.6)
- Cost: ~$0.10

## Structural Invariants

- The tree is rooted: one root node with no incoming Sub/Next edges.
- Every non-root node is reachable from the root via Sub/Next edges (forward direction).
- Every leaf node (no outgoing Sub) that is last-in-plan (no outgoing Next) must have a Backtrack edge to its parent.
- Nodes at the same level with the same parent are connected by a Next chain.
- The graph is fully connected: no orphan nodes.
- **Step range containment**: a child's `step_range` must be within its parent's range.

## Layout Rules (for rendering)

- Root at top, leaves at bottom.
- Sub edges are vertical (top → bottom).
- Next edges are horizontal (left → right), connecting siblings at the same Y level.
- Backtrack edges are vertical upward (dashed), from child top to parent bottom.
- Nodes within a level are sorted left-to-right by hierarchical ID.
