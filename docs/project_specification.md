# TrajLens Specification

A toolset that transforms agent execution logs into structured multi-graph visualizations and reports, making long agent trajectories readable and debuggable.
The goal is to transform trajectories into LLM-friendly intermediate representations, and human-readable graphs. These artifacts show how the agent reasoning process, goal transition process during run, or tool use pattern found. They may be used for agent's self reflection and even agentic RL of models.

TrajLens can be used as a standalone CLI util, but it also has a simple web interface.

---

## 1. Overview

TrajLens takes a raw agent console log as input and produces distinct output artifacts (intermediate graph representation and then corresponding graphs), each show an aspect of the trajectory.

**Parsing Strategy:** The current implementation uses deterministic regex-based parsing (format-specific parsers for Claude Code, PoCGen, etc.) for speed and zero cost. LLMs are only used for G1 (Goal Tree) and G2 (Reasoning DAG) construction, not for initial log parsing.
Turns are steps in Read-Eval-Print loop, containing one or more items:

- Item Category: `input` `think` `action(tool_name)` `event(event_name)`
- Args: filename, command, ...
- Cost: I/O token, cache hit

An example of an agent's trajectory:

```
[input] -> 
[think, action(read), action(read), action(read), action(web_search), event(context_compact)] -> 
[think, action(bash), action(write), action(submit_and_verify)] ->
[think, FINISH]
```

Note that sometimes the agent will add something (like system reminder) into the message and send it back to the LLM: this should be considered as an extra `input`.

---

## 2. Graph Catalog

### Intermediate Graph Representation (IGR)

IGRs are equivalent text of final output graphs. They contain the full info of a graph: nodes, edges, node text, edge text, node type, edge type, and so on.
It should NOT contain position info for the graph, since position does NOT contain info related to the problem itself.

**Format: TOML** (`.igr.toml`). Each file has a `graph_type` discriminator field, then `nodes` and `edges` arrays (or `root` for CostMap's recursive tree). TOML was chosen over JSON for human-readability and LLM-friendliness; over YAML for unambiguous typing.

Before generating any graph, IGR must be generated first. Graphs are human-readable, but IGRs are what we want ultimately.

### Graph Types

| # | Name | Primary question | LLM required? |
|---|---|---|---|
| G1 | Goal Transition Tree | "How was the agent's short-term goal transits?" | Yes |
| G2 | Reasoning Artifact DAG | "What does the agent observe or believe? Where does new insghts come from?" | Yes |
| G3 | Activity Graph | "What operations did the agent perform, and which repeated?" | No |
| G4 | Cost Map | "Where did the budget go?" | No |

---

## 3. Graph Definitions

### G1 - Goal Transition Tree

**Purpose:** Show the agent's intent hierarchy over time - what it was trying to accomplish, how it decomposed tasks, and when it abandoned a plan and started a new one.

Goal Transition Tree's nodes are linked by both tree edges (goal and sub-goals) and graph edges (transitions).

#### Nodes

| Node type | Definition | Example |
|---|---|---|
| `goal` | A named, purposeful intent the agent pursued. Always a verb phrase describing WHAT, never a sequence label. | "Craft PoC for stack overflow in parse_hdr" |

**Node attributes:**
- `label` - verb phrase, 3-5 lines for text.
- `status` - `done` \| `failed` \| `abandoned` \| `wip` (colored by status)
- `level` - depth of the node (root's level is 0)
- `step_range` - which steps in the raw trace this goal spans
- `cost` - total cost of steps in this range
- `reasoning_artifacts` - a list. See G2.

#### Edges

| Edge type | From → To | Meaning | Visual |
|---|---|---|---|
| `next` | sibling → sibling or parent -> child | "Sub-goal A completed, proceeding to sub-goal B" or "Start a new plan" | Solid arrow |
| `backtrack` | failed child → parent | "This sub-goal failed; control returns to the parent to replan" | Dashed red arc curving back upward |
| `sub` | parent --- child | "This sub-goal is the child of that goal" | Dashed black segment connecting goal and its sub-goals |

**Edge attributes:**
- `label` - cause of the transition (past tense). E.g. "Submission failed: win() not in binary", "Source read: related to ACL handler"

#### Structural invariants
- Exactly one root node (the overall task).
- `backtrack` edges point only from a failed node to its direct parent, consecutive backtracking is possible.
- Children are ordered left-to-right by time (earliest leftmost).
- When a child fails and the parent replans, the new children belong to a new `plan_id`; no `next` edge bridges across plan boundaries.

---

### G2 - Reasoning Artifact DAG

**Purpose:** Show what the agent believes at any point in the trajectory - distinguishing verified ground truths from unverified assumptions and derived insights.

#### Nodes

| Node type | Definition | Example |
|---|---|---|
| `ground_truth` | A fact the agent found or verified via a tool call and confirmed correct. | "bigint.c has memcpy at line 847" |
| `insight` | A conclusion the agent derived by reasoning over other artifacts. | "The offset to saved RIP must be 0x58 to make it work" |

**Node attributes:**
- `content` - the claim text
- `source_turn_id` - which step produced or stated this artifact
- `confidence` - [0, 1], the agent's apparent certainty
- `status` (insight only) - `verified` | `self-falsed` | `unverified`

#### Edges

| Edge type | From → To | Meaning |
|---|---|---|
| `infers` | ground_truth or insight → assumption or next insight | "This evidence strengthens that claim" |
| `contradicts` | ground_truth or contradiction → assumption or insight | "This evidence weakens or falsifies that claim" |
| `supersedes` | new artifact → old artifact | "The agent updated its belief; the new version replaces the old" |

NOTE: `infers` may be N-to-1 relationship. Make sure edges 1->3 and 2->3 are shown different to {1,2}->3 (two single arrow V.S. one 2-to-1 arrow).

#### Structural properties
- `ground_truth` nodes have no incoming edges (they are sources produced in tool use results).
- The final answer's claims should be traceable back through this DAG to either `ground_truth` or `assumption` nodes.

---

### G3 - Activity Graph

**Purpose:** Show what filesystem targets the agent operated on, organized as a path hierarchy. Repetition is visible from `call_indices` and the `#call_index` annotation on each operation row. No arrows needed — chronological order is shown by the call index numbers.

#### Nodes

| Node type | Definition | Example |
|---|---|---|
| `activity` | A distinct operation target, identified by `(goal_category, target_path)`. Multiple raw steps on the same target merge into one node. | "gc.c [run]" (visited 3 times → one node with 3 operations) |

**Node attributes:**
- `label` - basename of the target (file or directory name)
- `goal_category` - `read` \| `write` \| `edit` \| `list` \| `run` \| `other`
- `primary_object` - full filesystem path of the target
- `parent_id` - node_id of the containing directory node (null for root-level nodes)
- `call_indices` - list of raw step indices that merged into this node
- `operations` - list of operations, each with `(op_type, detail, call_index)`. Example: `[(run, "grep -n foo /path/gc.c", 3), (read, "L790-L820", 11)]`
- `total_cost` - sum of costs across all visits

#### Hierarchy

Nodes form a tree based on filesystem path containment:
- `ls /workspace` → node "workspace" (root)
- `ls /workspace/src-vul` → node "src-vul" with `parent_id` pointing to "workspace"
- `cat /workspace/src-vul/gc.c` → node "gc.c" with `parent_id` pointing to "src-vul"

The hierarchy is determined after grouping: for each node, the longest-prefix path match among other nodes becomes its parent.

#### Edges

Edges exist in the data model (encoding chronological visit order between operations) but are **not rendered** — the `#call_index` prefix on each operation row already communicates execution sequence.

#### Visual

Each leaf node renders as a table: header row (label, category, op count, cost) + one row per operation. Container nodes (those with children) render as labeled boxes containing their child nodes. No arrows.

---


### G4 - Cost Map

**Purpose:** Show where the budget was spent, broken down by goal/phase, presented as proportional flow.

This map should show cost like SpaceSniffer's Treemap. You can use this metaphor:

- Folder = Goal
- Sub-folder = Sub-goal
- Files = Turns used in different category
- Size = Cost

Example structure:

- Ultimate goal (5$)
  - Read (0.5$)
  - Write (0.5$)
  - Bash (0.5$)
  - Think (1$)
  - Sub-goal #1 (0.5$)
    - Read (0.5$)
  - Sub-goal #2 (2$)
    - Read (0.5$)
    - Think (1.0$)
    - Write (0.25$)
    - Run (0.25$)

Each node is a rectangle containing sub-items.

---


## 4. Report Output

For each trajectory, TrajLens use LLM produces a **text report** alongside the visualizations. The report contains:

1. **Header:** run label, outcome, total cost, total calls.
2. **Cost breakdown:** one line per turn category (cost, %, call count).
3. **Problem diagnosis**:
   - Anti-patterns in G1 (e.g. dead end or repetitive looping).
   - Unused insights or wrong assumptions in G2.
   - Repeated operation against the same project in G3 (e.g. overlapping read of a file).
   - Others.

---

## 5. Construction Methods

| Graph | Construction approach |
|---|---|
| G1 (Goal Tree) | LLM classifies goals and transitions from a windowed trajectory; deterministic synthesizer enforces plan/backtrack invariants; validator checks structural properties. |
| G2 (Reasoning DAG) | LLM classifies artifact types (ground_truth / insight) and relationships (infers / contradicts / supersedes) from agent's reasoning text. |
| G3 (Activity Graph) | Deterministic: extract target path from each action; group by `(goal_category, target_path)`; assign `parent_id` by longest-prefix path containment; chronological order shown via call_index. |
| G4 (Cost Map) | Deterministic: build treemap tree from G1 goal nodes (or flat categories if no goal tree); area proportional to dollar_cost. |

---

## 6. Webpage

Vite + React static site. Core logic runs as WASM (same Rust crate compiled to `wasm32`). No backend server. Deployable to GitHub Pages.

```
User drops .log file
  → WASM parses log → Trajectory
  → WASM builds G3 (Activity Graph), G4 (Cost Map) → IGR
  → Browser calls Anthropic API (user's key) for G1, G2 → IGR
  → React Flow renders all graphs from IGR
```

Alternatively, user can load pre-built `.igr.toml` files directly (skipping parse + build).

### Interaction

All graphs support:
- **Pan + zoom:** built-in via React Flow (G1, G2, G3) or custom SVG viewport (G4).
- **MiniMap:** navigation overview for large graphs.
- **Hover:** tooltip with full node details (cost breakdown, step range, content).
- **Click:** drill-down or collapse (graph-dependent). Click a node to show detail panel with corresponding turns.
- **Export:** SVG (static), PNG at 2×/4×.

Future:
- **Step-by-step reveal:** nodes appear in temporal order; edges appear when both endpoints are visible.
- **Pause / resume / scrub:** keyboard (`space`, `←`, `→`) and scrubber bar.

### Pages

- `Home`:
  - File picker: accepts `.log` (triggers WASM parse + build) or `.igr.toml` (renders directly)
  - API key input for LLM-dependent graphs (stored in localStorage)
  - Cost estimation shown before LLM calls
- `Result`:
  - Upper nav bar to choose which graph to show
  - Big canvas to show the graph
  - Stored in browser memory (no server storage)

---

## 7. Input Contract

TrajLens accepts any text log. An **optional mapper config** (YAML) declares how to parse lines into structured records. The mapper specifies:
- How to identify trajectory boundaries (session key / trace ID).
- How to classify each line into a step type (tool call, LLM output, observation, etc.).
- Where to find cost, token count, tool name, exit code, and content fields.

Presets ship for common formats. Custom formats require one mapper file.

If the mapper is not specified, a simple model should be used to identify the corresponding mapper config. 

All contents NOT successfully handled by the parser should be marked `unknown` in the output, and a simple fallback model should be used to fix the output and move/modify/categorize them, so no `unknown` entries remain.

---

## 8. Privacy & Deployment

- Runs as a static site (deployable to GitHub Pages).
- Logs are processed in-browser; never uploaded to any server.
- LLM enhancement (G1, G2) calls the user's own API key directly from the browser.
- Cost estimation is shown before every LLM call; a budget meter tracks spend.
- API key stored in localStorage; never transmitted except to the configured LLM provider.

---

## Misc

- You can test with example trajectories: @example_trajectories/ .
- You must design DETERMINISTIC unit tests to make sure log parsing work correctly.

## Usage Example

User: CLI or Web
Renderers: any. As CFG features. User picks from them.

Web interface is just a wrapper of this lib, and the animation is often from the React.js renderer's output, not always! User can choose which render to use in the webpage, though the React.js one is recommended.

Web->invoke lib->React.js web content got->render in a sub area

TrajLensWeb/:
    Csrgo.toml
    src/
     - lib.rs
    public/
     - ...
    package.json
TrajLens/:
    Cargo.toml
    src/:
    - lib.rs
    - bin/
      - cli.rs
    - igr/
    - renderers/
        - mod.rs
        - trait.rs
        - svg_rust_render/
        - reactjs_renderer/
        - xxxlib_renderer/

Web interface should be independent: no wasm or web related thing should be in core lib.
Use workspace to manage multiple members in this Rust monorepo.
NOTE: this is just an example. You can criticize and optimize the structure. But you must ensure there's no web-ish things go inside the core lib.