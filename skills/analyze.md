---
name: analyze
description: Analyze agent logs into goal trees and graphs. Use for self-reflection on agent behavior.
match:
  - "analyze"
  - "generate graphs"
  - "goal tree"
  - "what happened"
  - "why did it fail"
  - "visualize trajectory"
  - "self-reflect"
---

# Analyze Agent Trajectory

One command to understand what an agent did, why it failed, and what to improve.

## Quick Usage

```bash
source .env && cargo run --features "cli,svg-rust,llm-bedrock" --bin trajlens -- analyze "<input>" -o output/<name> --model "bedrock/us.anthropic.claude-sonnet-4-6"
```

Then show the text tree:

```bash
python3 -c "
import toml
gt = toml.loads(open('output/<name>/<agent>/goal-tree.igr.toml').read())
children_of = {}
for e in gt['edges']:
    if e['edge_type'] == 'sub':
        chain = [e['target_id']]
        cur = e['target_id']
        while (nxt := next((x for x in gt['edges'] if x['source_id'] == cur and x['edge_type'] == 'next'), None)):
            chain.append(nxt['target_id']); cur = nxt['target_id']
        children_of[e['source_id']] = chain
node_map = {n['node_id']: n for n in gt['nodes']}
def show(nid, pre, root, last):
    n = node_map.get(nid)
    if not n: return
    cat = {'explore':'🔍','think':'💭','act':'⚡'}.get(n.get('goal_type',''),'')
    st = n.get('status','?')
    res = f\" → {n['result']}\" if n.get('result') else ''
    if root: print(f\"{n['node_id']} {cat}({st}) {n['label']}{res}\")
    else: print(f\"{pre}{'└── ' if last else '├── '}{n['node_id']} {cat}({st}) {n['label']}{res}\")
    kids = children_of.get(nid, [])
    for i, k in enumerate(kids):
        show(k, '' if root else pre+('    ' if last else '│   '), False, i==len(kids)-1)
show(gt.get('root_id','ROOT'), '', True, True)
"
```

## What It Produces

| Graph | What it shows | Cost |
|-------|--------------|------|
| G1 Goal Tree | What the agent tried, in what order, what succeeded/failed | ~$0.21 |
| G2 Reasoning DAG | Hypotheses formed, verified, or falsified | ~$0.21 |
| G3 Activity Graph | Files/endpoints touched, operations performed | Free |
| G4 Cost Map | Token cost breakdown by category | Free |

## Common Patterns

**"Why did this agent fail?"**
```bash
# Generate goal tree only (cheapest useful graph)
source .env && cargo run --features "cli,svg-rust,llm-bedrock" --bin trajlens -- analyze "path/to/log" -o output/debug --graphs g1
```
Then read the text tree — failed nodes show the root cause in their `result` field.

**"What files did the agent touch?"**
```bash
# Activity graph only (free, no LLM)
cargo run --features "cli,svg-rust" --bin trajlens -- analyze "path/to/log" -o output/activity --graphs g3
```

**"Compare success vs failure on same task"**
```bash
source .env && cargo run --features "cli,svg-rust,llm-bedrock" --bin trajlens -- analyze "success.json" "failure.json" -o output/compare --graphs g1,g2
```
Then compare text trees side by side.

**"Batch analyze without spending money"**
```bash
cargo run --features "cli,svg-rust" --bin trajlens -- analyze "logs/*.log" -o output/batch --graphs g3,g4
```

## Input Formats

Auto-detected. Supports:
- Single log file (any supported format)
- Directory (folder-based logs like poc-agent-codex)
- Glob pattern (`"logs/*.log"`)

## Output

```
output/<name>/<agent_id>/
├── trajectory.json
├── goal-tree.igr.toml + .svg
├── reasoning-dag.igr.toml + .svg
├── activity-graph.igr.toml + .svg
└── cost-map.igr.toml + .svg
```

## Budget Safety

Default budget: $100. Aborts if estimated LLM cost exceeds it.
```bash
--budget 10          # strict limit
--graphs g3,g4       # free graphs only
```
