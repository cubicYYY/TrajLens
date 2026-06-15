# Cost Map (G4)

Deterministic builder that produces a recursive treemap where area is proportional to token cost.

**File:** `trajlens/src/graphs/cost_map.rs`

## Purpose

Shows where the budget was spent, broken down by goal/category, like SpaceSniffer's treemap. No LLM required — fully deterministic.

## Construction Modes

### Flat Mode (no Goal Tree)

Groups costs by action category as top-level children of a single root node.

```
Root (total cost)
├── Read ($X)
├── Write ($Y)
├── Run ($Z)
├── Think ($W)
└── Other ($V)
```

### Goal-Hierarchical Mode (with Goal Tree)

When a `GoalTransitionTree` is provided, mirrors its hierarchy — each goal becomes a cost map node with matching parent-child structure.

```
Root Task ($5.00, steps 1-50)
├── Goal 1 ($0.50, steps 1-10)
├── Others ($0.25, steps 11-14)
├── Goal 2 ($1.00, steps 15-30)
│   ├── Goal 2.1 ($0.40, steps 15-22)
│   ├── Goal 2.2 ($0.40, steps 23-26)
│   └── Goal 2.3 ($0.20, steps 27-30)
├── Others ($0.25, steps 31-40)
└── Goal 3 ($1.00, steps 41-50)
```

## Nodes

| Attribute | Description |
|-----------|-------------|
| `node_id` | Matches goal tree ID (hierarchical mode) or `cat_<category>` (flat) |
| `label` | Goal name or capitalized category |
| `cost` | `{ dollar_cost, input_tokens, output_tokens }` |
| `children` | Nested child nodes (recursive tree) |
| `category` | (leaf only) Category for color coding |

### Category Classification

| Sub-category patterns | Category |
|----------------------|----------|
| `read` | read |
| `write` | write |
| `bash`, `run` | run |
| `think`, `reason` | think |
| `event`, `error` | event |
| Everything else | other |

## Edges

No explicit edges. Hierarchy is represented by nested `children` arrays.

## Structural Invariants

- Strictly hierarchical (no cycles, no shared children)
- Parent cost ≥ sum of children costs (parent may include overhead)
- When synced with Goal Tree: node IDs match goal tree node IDs
- Children within a parent sorted by descending cost (largest first for treemap layout)

## Visual Rendering

- Recursive treemap: area proportional to `dollar_cost`
- Alternating horizontal/vertical splits based on aspect ratio
- Color by category + depth gradient
- Each rectangle shows: goal ID, goal name, step range, cost

## CLI Usage

```bash
# Build from trajectory (flat mode)
trajlens build cost-map trajectory.json -o cost-map.igr.toml

# All-in-one pipeline (includes cost map)
trajlens run input.log -o output/
```

## Programmatic Usage

```rust
use trajlens::graphs::cost_map;
use trajlens::models::{Trajectory, GoalTransitionTree};

let trajectory: Trajectory = /* parsed */;

// Flat mode (no goal tree)
let map = cost_map::build(&trajectory, None);

// Hierarchical mode (with goal tree)
let goal_tree: GoalTransitionTree = /* from LLM */;
let map = cost_map::build(&trajectory, Some(&goal_tree));
```

## Key Implementation Details

- Total cost is computed from trajectory; if zero, sums children
- Zero-cost categories are filtered out (no empty nodes)
- Children sorted descending by dollar_cost for optimal treemap layout
- "Others" nodes capture cost from steps not covered by any goal

