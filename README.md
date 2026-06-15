# TrajLens

Transform agent execution logs into structured graph visualizations for trajectory analysis.

TrajLens parses logs from AI agents (Claude Code, PoCGen, CyberAgent, Codex, etc.) into per-agent trajectories, then builds four graph types that reveal different aspects of behavior: goal transitions, reasoning chains, file activity, and cost breakdowns.

**Two ways to use TrajLens:**
- **Web interface** (recommended for exploration) — interactive React Flow graphs with click-to-expand details, pan/zoom, and node inspection. Best for understanding agent behavior.
- **CLI tool** (recommended for batch processing) — single binary, generates SVG/IGR files at scale. Best for CI pipelines, large dataset analysis, and automation.

## Quick Start

```bash
# Build
cargo build --features "cli,svg-rust,llm-bedrock"

# Analyze a log (generates all 4 graphs per agent)
trajlens analyze trace.log -o output/ --model "bedrock/us.anthropic.claude-sonnet-4-6"

# Only deterministic graphs (no LLM cost)
trajlens analyze trace.log -o output/ --graphs g3,g4

# Batch with glob
trajlens analyze "logs/*.log" -o output/

# Folder-based logs (directory as input)
trajlens analyze ./session_folder/ -o output/ --format poc_agent_folder
```

## Graph Types

| Graph | Type | Description |
|-------|------|-------------|
| **G1: Goal Tree** | LLM | Hierarchical goal transitions with status (done/failed/partial) |
| **G2: Reasoning DAG** | LLM | Hypothesis formation, verification, and falsification chain |
| **G3: Activity Graph** | Deterministic | File targets accessed, grouped by directory hierarchy |
| **G4: Cost Map** | Deterministic | Treemap of token cost by category |

## CLI Commands

```bash
trajlens analyze <inputs> -o <dir>          # End-to-end: parse + build + render
trajlens parse <input> -o <dir>             # Log -> per-agent trajectory JSONs
trajlens build <type> <traj.json> -o <igr>  # Trajectory -> IGR TOML
trajlens render <igr.toml> -o <svg>         # IGR -> SVG
trajlens generate-parser <samples> --name x # LLM generates a new parser
```

## Parser Zoo

Format detection is automatic via fingerprinting. New formats are added by the LLM parser generator:

```bash
trajlens generate-parser sample1.log sample2.log --name my_format \
  --model "bedrock/us.anthropic.claude-sonnet-4-6"
```

Built-in parsers: `claude_code_history_jsonl`, `claude_code_text`, `pocgen_text`, `cyberagent_log`, `codex_streaming_json`, `poc_agent_folder`, `cairn_project_yaml`, `nova2_auditor`.

## Architecture

```
trajlens/src/
  bin/cli.rs          # CLI (analyze, parse, build, render, generate-parser)
  models.rs           # Core types (Trajectory, Step, Item, all 4 graph types)
  parsing/            # Parser registry, script runner, cost estimator, LLM agents
  graphs/             # Graph builders (activity_graph, cost_map, goal_tree, reasoning_dag)
  compilers/          # Renderers (svg_rust, reactflow, mermaid, neo4j)
  llm/                # LLM clients (Anthropic, Bedrock)
  igr.rs              # IGR TOML serialization

parsers/
  configs/*.toml      # Fingerprint + parser script path + agent_id rules
  scripts/*.py        # Python parser scripts (called via subprocess)

trajlens-web/         # React Flow interactive viewer
dev_utils/            # Screenshot & render utilities
```

## Output Structure

```
output/<log_stem>/<agent_id>/
  trajectory.json
  goal-tree.{igr.toml,svg,metrics.json}
  reasoning-dag.{igr.toml,svg,metrics.json}
  activity-graph.{igr.toml,svg,metrics.json}
  cost-map.{igr.toml,svg,metrics.json}
  log_slice.log
```

## Configuration

```bash
cp config.example.toml config.toml  # All 90+ parameters documented inline
```

LLM provider: `"bedrock/us.anthropic.claude-sonnet-4-6"` or `"anthropic/claude-sonnet-4-6"`.

Environment: `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_DEFAULT_REGION_NAME` (Bedrock) or `ANTHROPIC_API_KEY` (Anthropic).

## Budget Control

```bash
# Default $100 budget for LLM calls — aborts if estimated cost exceeds it
trajlens analyze "logs/*.log" -o out/ --graphs g1,g2 --budget 50

# Bypass budget check
trajlens analyze "logs/*.log" -o out/ --dangerously-unlimited-budget
```

## Web Interface (recommended for readability)

The web viewer provides the best experience for understanding agent trajectories — interactive graphs with expandable details, smooth navigation, and rich node information that static SVGs can't match.

```bash
# Option 1: Full interactive web app
cd trajlens-web && npm install && npm run dev
# Open http://localhost:5173, load .igr.toml files

# Option 2: Render to PNG via headless browser (for reports/docs)
python dev_utils/render_reactflow.py output/goal-tree.igr.toml screenshot.png

# Option 3: Render all IGR files in a directory
python dev_utils/render_reactflow.py --all output/poc_analysis/
```

Features: click nodes to expand details, category badges (EXPLORE/THINK/ACT), color-coded status, smooth bezier edges, minimap navigation.

## Installation

```bash
# End user (CLI only, no LLM)
./install.sh

# Developer (full features + hooks + web viewer)
./dev_install.sh
```

## Development

```bash
cargo build --features "cli,svg-rust,llm-bedrock"   # Full build
cargo test                                           # Unit + integration tests
cargo test --test test_render                        # SVG rendering tests
```

See [CLAUDE.md](CLAUDE.md) for full development guide.

## License

MIT
