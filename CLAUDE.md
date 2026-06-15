# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

TrajLens transforms agent execution logs into structured multi-graph visualizations (IGR format) and reports. It parses raw console logs from AI agents (Claude Code, PoCGen, CyberAgent, etc.) into per-agent trajectories, then builds four graph types: Goal Transition Tree (G1, LLM), Reasoning Artifact DAG (G2, LLM), Activity Graph (G3, deterministic), and Cost Map (G4, deterministic).

## Configuration

TrajLens uses `config.toml` for all configurable parameters (colors, dimensions, pricing, etc.). The config module (`trajlens/src/config.rs`) loads from:
1. `./config.toml` (current directory, highest priority)
2. `~/.config/trajlens/config.toml` (user-specific)
3. Hardcoded defaults (if no config file found)

**Key principle:** NO magic numbers in code. All constants must be in `config.toml` with descriptions and defaults, unless moving them would make code messy/redundant.

See `config.example.toml` for all 90+ parameters with detailed comments and usage examples.

## Build & Run Commands

### Rust (trajlens/ — library, CLI, WASM)

```bash
# Build library + CLI (default features: cli + svg-rust)
cargo build

# Build with LLM support (required for G1, G2, analyze, generate-parser)
cargo build --features "cli,svg-rust,llm-bedrock"

# Run tests
cargo test
cargo test --features "cli,svg-rust,llm-bedrock" --tests    # full suite with LLM
cargo test --test test_render                                # single test file
cargo test test_name                                         # single test by name

# Accept insta snapshots
cargo insta review

# CLI — recommended entry point
cargo run --features "cli,svg-rust,llm-bedrock" --bin trajlens -- analyze \
  "logs/*.log" -o output/ --model "bedrock/us.anthropic.claude-sonnet-4-6"

# CLI — atomic primitives
cargo run --bin trajlens -- parse input.log -o out/
cargo run --bin trajlens -- build g3 out/main/trajectory.json -o out/main/activity-graph.igr.toml
cargo run --bin trajlens -- render out/main/activity-graph.igr.toml -o out/main/activity-graph.svg

# Generate a parser for a new log format
cargo run --features "cli,svg-rust,llm-bedrock" --bin trajlens -- generate-parser \
  sample1.log sample2.log --name my_format --model "bedrock/us.anthropic.claude-sonnet-4-6"

# Build WASM for browser
wasm-pack build --target web --no-default-features --features wasm
```

### Web (trajlens-web/ — Vite + React viewer)

```bash
cd trajlens-web
npm install
npm run dev      # dev server
npm run build    # production build
```

## Architecture

### Single Rust Crate

**`trajlens/`** — One Rust crate with multiple build targets:
- **Library** (`src/lib.rs`): Core models, parsers, graph builders, IGR serialization
- **CLI Binary** (`src/bin/cli.rs`): Command-line interface (feature: `cli`)
- **WASM Module** (`src/wasm.rs`): Browser bindings (feature: `wasm`)
- **Graph Compiler Plugins** (`src/compilers/`): Optional renderers via feature flags

### Pipeline Flow

```
Raw Log → Fingerprint Matching → Parser Script (Python) → agent_id Rules → LLM Patching (optional)
    → Per-Agent Trajectories → Cost Estimator → Graph Builders → IGR (TOML) → Renderer (SVG)
```

### CLI Commands

- **`analyze`** (recommended) — End-to-end: 1+ logs → split by agent → build G1-G4 per agent. Accepts file paths and globs. `--graphs g1,g2,g3,g4` to subset.
- **`parse`** — Raw log → per-agent Trajectory JSONs (stops before graph building).
- **`build`** — Trajectory JSON → single IGR TOML. Graph type determines if LLM is needed (`g1`/`g2` = LLM, `g3`/`g4` = deterministic). Aliases: g1=goal-tree, g2=reasoning-dag, g3=activity-graph, g4=cost-map.
- **`render`** — IGR TOML → SVG + metrics.json sidecar.
- **`generate-parser`** — LLM agents create fingerprint + parser script + agent_id rules for a new log format.

### Parser System (docs/universal_parser.md)

Format knowledge lives OUTSIDE the Rust code:
- `parsers/configs/*.toml` — fingerprint patterns (ALL must match) + parser script path + `[[agent_id_rules]]`
- `parsers/scripts/*.py` — Python scripts that do all extraction (receives log path via argv[1], outputs Vec<StepInfo> as JSON to stdout)

**StepInfo schema:** `{step_id, agent_id, content, start_time, end_time, metrics{input_token, output_token, cache_read, cache_write, time, cost, line_range}, operations[{type, sub_type, args}]}`

**agent_id rules** (config-driven, regex-based): applied after the script runs. Separates multi-agent logs into per-agent trajectories. Each agent's trajectory represents what was in that agent's context window.

**Parser generation agents** (`src/parsing/parser_agents.rs`):
1. `FingerprintGenAgent` — proposes fingerprint patterns from samples
2. `AgentIdGenAgent` — detects multi-agent markers, proposes extraction rules
3. `ParserGenAgent` — generates the Python parser script

### Key Modules

- `src/models.rs` — Core data types (Trajectory, Step, Item, Cost, all 4 graph types)
- `src/parsing/` — Parser registry, script runner, agent_id rule application, cost estimator, LLM-based parser generation agents
- `src/graphs/` — Graph builders: `activity_graph.rs` (G3), `cost_map.rs` (G4), `goal_tree.rs` (G1, LLM), `reasoning_dag.rs` (G2, LLM)
- `src/igr.rs` — IGR TOML serialization/deserialization
- `src/compilers/` — `layout.rs` (Sugiyama), `svg_rust/` (SVG string generation per graph type)
- `src/llm/` — LLM client abstraction (Anthropic + Bedrock providers), model registry
- `src/bin/cli.rs` — CLI: `analyze`, `parse`, `build`, `render`, `generate-parser`

### Output Layout

`analyze` produces a uniform directory structure regardless of agent count:
```
output/<log_stem>/<agent_id>/
├── trajectory.json       # parsed structured trajectory
├── log_slice.log         # original log lines for this agent only
├── activity-graph.{igr.toml,svg,metrics.json}   (G3)
├── cost-map.{igr.toml,svg,metrics.json}         (G4)
├── goal-tree.{igr.toml,svg,metrics.json}        (G1)
└── reasoning-dag.{igr.toml,svg,metrics.json}    (G2)
```

### Render Metrics

Every `render` call writes a `.metrics.json` sidecar with: node_count, edge_count, canvas_size, distinct_y_levels, distinct_x_columns, truncation_ratio, terminal_outcome (G2 only), and warnings (layout collapse, extreme aspect ratio, etc.).

### IGR Format

Intermediate Graph Representation uses TOML (`.igr.toml`). Every graph passes through IGR before rendering. Contains `graph_type` discriminator + nodes/edges. No position info — layout is computed at render time.

## LLM Provider Configuration

Model spec format: `"provider/model-name"` (e.g., `"bedrock/us.anthropic.claude-sonnet-4-6"`, `"anthropic/claude-sonnet-4-6"`).

For testing (AWS Bedrock): set `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `AWS_DEFAULT_REGION_NAME` in `.env`.
For production (Anthropic): set `ANTHROPIC_API_KEY`.

## Test Fixtures

Example trajectories in `example_trajectories/`.
Rust tests use `insta` for snapshot testing. Integration tests in `trajlens/tests/`.

## CRITICAL: Dev Rules

- CRITICAL: If user ask you to run something, always estimate cost first, and ask confirmation from the user.
- Mocking should be transparent to modules: no special handling for mocking is allowed in production code.
- Do NOT trim any string in logging unless it exceeds 1024 characters.
- Simplicity beats complexity. Keep it simple and stupid.
- "Extensibility" is not a reason to abuse abstraction. Add only proven to be neccessary.
- Explicitness beats ambiguity: NO default params, and configurable args should be managed in few places.
- Specification first, then unit tests, code finally.
- Always make the execution path clear: code is the Standard Operation Procedure itself.
- If you misused something, that means the usage of the components may be confusing: report to the user.
- If you find something wrong, find the root cause and fix it instead of monkey patching.
- Use a comment paragraph > 5 lines to mark every workaround, temporary implementation or unnatural, inelegant code smell. The comment graph must elaborate WHY this have to be done like that, so we can review and fix later.
- Sandwich rule: complexity should be near input and output, the core should be kept simple.
- Find the root cause and meta cause of a problem, report it and fix it.
- Spec first, unit tests second, and then final coding.
- Always write detailed inline doc string/comments, especially for classes, interfaces, structs and traits.
- Always us uv to manage Python envs, not conda.
- All docs should be put in @docs/ to make the project structure cleaner.
- Always keep docs and unit tests up-to-date after changes to avoid misleading info pollution.
- Maintain configurable args in one config file and comment every arg's description, default value. There should be NO magic number and constants in the code unless moving it to the config will make the code messy and redundant.
- [CRITICAL] Record all changes or key decisions not described in specification to @docs/DEV_NOTES.md .
- Always show the project-relative or absolute path of artifacts desired by the user.
- Heuristic rules must be SOFT: they may be violated in rare conditions but the program should still go fine.
- **Spec first, tests second, code last.**
- **Simplicity beats complexity.** "Extensibility" is not a reason to abuse abstraction.
- **No default parameters.** All configurable args explicit, managed in config.toml.
- **Sandwich rule:** complexity at input/output boundaries; core stays simple.
- **Heuristic rules must be SOFT:** they may be violated in rare conditions but the program should still work.
- **Workarounds require a >5 line comment** explaining WHY.
- **Always write detailed docstrings** for structs, traits, and public interfaces.
- **Find root causes**, not monkey patches. If something confuses you, report the usability problem.
- **[CRITICAL] Record key decisions** not in spec to `docs/dev/DEV_NOTES.md`.
- **Use uv** for Python environments, never conda.
- **All docs in docs/** to keep project structure clean.
- **Keep docs and tests up-to-date** — no misleading stale info.