# Reasoning Artifact DAG (G2)

LLM-based builder that extracts a directed acyclic graph of knowledge artifacts and hypothesis timelines.

**File:** `trajlens/src/graphs/reasoning_dag.rs`

## Purpose

Reconstructs the agent's **reasoning evolution** — not just final conclusions, but the hypothesis lifecycle: what approaches were tried, when pivots occurred, and why some paths succeeded while others were abandoned. Critical for comparative analysis (why did run A succeed but run B fail?).

## Construction Flow

1. Extract reasoning-heavy content from trajectory (Think items, items >100 chars)
2. Sample top 30 items by content length
3. LLM identifies ground truths vs insights, and relationships between them
4. Parse JSON response into `ReasoningArtifactDAG`

## Nodes

| Type | Definition | Confidence |
|------|-----------|-----------|
| `ground_truth` | Observed fact verified via tool call | 1.0 (always) |
| `hypothesis` | A strategic bet about HOW to solve the problem (has step_range lifecycle) | 0.0–1.0 |
| `insight` | Derived conclusion or realization that changes direction | 0.0–1.0 |

### Node Attributes

| Attribute | Description |
|-----------|-------------|
| `node_id` | Unique ID (r0, r1, ...) |
| `content` | The claim text |
| `node_type` | `ground_truth` or `insight` |
| `source_turn_id` | Which trajectory step produced this |
| `confidence` | Certainty score [0.0, 1.0] |
| `status` | (insight only) `verified` / `self-falsed` / `unverified` |

## Edges

| Type | Meaning | Directionality |
|------|---------|---------------|
| `infers` | A supports/strengthens B | Many-to-one (N sources → 1 target) |
| `contradicts` | A conflicts with B | Unidirectional (stored one way) |
| `supersedes` | A replaces B (newer hypothesis) | Temporal ordering (pivot) |
| `falsifies` | Evidence A disproved hypothesis B | Evidence → hypothesis |

### Structural Rules

- `ground_truth` nodes have NO incoming `infers` edges (they are axioms)
- The graph is a DAG (no cycles)
- Every node is reachable from at least one `ground_truth` via `infers` edges
- No isolated nodes (every node has at least one edge)
- `infers` may be N-to-1: edges 1→3 and 2→3 are distinct from a single {1,2}→3

## CLI Usage

```bash
trajlens build-llm reasoning-dag trajectory.json \
  -o reasoning-dag.igr.toml --model anthropic/claude-sonnet-4-6

# Default model
trajlens build-llm reasoning-dag trajectory.json -o reasoning-dag.igr.toml
```

## Programmatic Usage

```rust
use trajlens::llm::AnthropicClient;
use trajlens::graphs::reasoning_dag;

#[tokio::main]
async fn main() {
    let client = AnthropicClient::from_env().unwrap();
    let dag = reasoning_dag::build_with_llm(&trajectory, &client).await.unwrap();
    println!("{} reasoning artifacts", dag.nodes.len());
}
```

## Performance

For typical 100-turn trajectory:
- Input: ~20,000 tokens
- Output: ~3,000 tokens
- Time: 5-15s (Sonnet 4.6)
- Cost: ~$0.10

## Structural Invariants

- The graph is a DAG (no cycles).
- Every node is reachable from at least one `ground_truth` node via `infers` edges.
- No isolated nodes (every node has at least one edge, incoming or outgoing).
- `ground_truth` nodes have no incoming `infers` edges (they are axioms).
- `contradicts` edges are symmetric in meaning but stored unidirectional.
- `supersedes` edges are temporal: newer insight replaces older one.
