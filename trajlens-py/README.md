# TrajLens Python Bindings

Python wrapper for TrajLens, providing a Pythonic API to the high-performance Rust implementation.

## Installation

### From Source

```bash
# Install maturin (Python build tool for Rust extensions)
pip install maturin

# Build and install in development mode
maturin develop --features python

# Or build a wheel for distribution
maturin build --release --features python
pip install target/wheels/trajlens-*.whl
```

### With UV

```bash
# Development installation
uv pip install maturin
uv run maturin develop --features python
```

## Quick Start

```python
import trajlens

# Parse an agent log
with open("agent.log") as f:
    trajectory = trajlens.parse_log("auto", f.read())

# Build an Activity Graph
activity_graph = trajlens.build_activity_graph(trajectory)

# Render to SVG
svg = trajlens.render_svg(activity_graph)
with open("graph.svg", "w") as f:
    f.write(svg)
```

## API Reference

### Parsing

**`parse_log(format: str, content: str) -> str`**

Parse a raw agent log into a Trajectory (returned as JSON string).

- `format`: Log format - `"auto"` (auto-detect), `"claude-code"`, or `"pocgen"`
- `content`: Raw log content
- Returns: Trajectory as JSON string
- Raises: `ValueError` if format is unknown or parsing fails

```python
trajectory_json = trajlens.parse_log("auto", log_content)
```

### Graph Building

**`build_activity_graph(trajectory_json: str) -> str`**

Build an Activity Graph (G3) from a Trajectory.

- `trajectory_json`: Trajectory as JSON string (from `parse_log`)
- Returns: Activity Graph as JSON string
- Raises: `ValueError` if JSON is invalid

```python
activity_graph_json = trajlens.build_activity_graph(trajectory_json)
```

**`build_cost_map(trajectory_json: str, goal_tree_json: str | None = None) -> str`**

Build a Cost Map (G4) from a Trajectory.

- `trajectory_json`: Trajectory as JSON string
- `goal_tree_json`: Optional Goal Tree JSON for categorization
- Returns: Cost Map as JSON string
- Raises: `ValueError` if JSON is invalid

```python
cost_map_json = trajlens.build_cost_map(trajectory_json)
```

### Rendering

**`render_svg(graph_json: str) -> str`**

Render a graph to SVG markup using the Rust renderer.

- `graph_json`: Graph as JSON string (from `build_*` functions)
- Returns: SVG markup as string
- Raises: `ValueError` if JSON is invalid, `RuntimeError` if renderer unavailable

```python
svg_string = trajlens.render_svg(graph_json)
```

### IGR Interchange Format

**`to_igr_toml(graph_json: str) -> str`**

Serialize a graph to IGR TOML format for interchange.

- `graph_json`: Graph as JSON string
- Returns: IGR TOML string
- Raises: `ValueError` if JSON is invalid or serialization fails

```python
igr_toml = trajlens.to_igr_toml(graph_json)
with open("graph.igr.toml", "w") as f:
    f.write(igr_toml)
```

**`from_igr_toml(igr_toml: str) -> str`**

Deserialize an IGR TOML string into a graph.

- `igr_toml`: IGR TOML string
- Returns: Graph as JSON string
- Raises: `ValueError` if TOML is invalid or deserialization fails

```python
graph_json = trajlens.from_igr_toml(igr_toml)
```

## Complete Example

```python
import trajlens
import json

# 1. Parse a log file
with open("example_trajectories/G4_architecture/arvo_57672_cc_FAILED.log") as f:
    log_content = f.read()

trajectory_json = trajlens.parse_log("claude-code", log_content)
trajectory = json.loads(trajectory_json)
print(f"Parsed {len(trajectory['turns'])} turns")

# 2. Build Activity Graph
activity_graph_json = trajlens.build_activity_graph(trajectory_json)
activity_graph = json.loads(activity_graph_json)
print(f"Activity Graph: {len(activity_graph['nodes'])} nodes, {len(activity_graph['edges'])} edges")

# 3. Render to SVG
svg = trajlens.render_svg(activity_graph_json)
with open("activity_graph.svg", "w") as f:
    f.write(svg)
print("Saved activity_graph.svg")

# 4. Save as IGR TOML for interchange
igr_toml = trajlens.to_igr_toml(activity_graph_json)
with open("activity_graph.igr.toml", "w") as f:
    f.write(igr_toml)
print("Saved activity_graph.igr.toml")

# 5. Build Cost Map
cost_map_json = trajlens.build_cost_map(trajectory_json)
cost_map = json.loads(cost_map_json)
print(f"Cost Map: {len(cost_map['root']['children'])} categories")

# 6. Render Cost Map
cost_svg = trajlens.render_svg(cost_map_json)
with open("cost_map.svg", "w") as f:
    f.write(cost_svg)
print("Saved cost_map.svg")
```

## Data Format

All data is exchanged as JSON strings for simplicity. Use Python's `json` module to parse:

```python
import json

trajectory_json = trajlens.parse_log("auto", log_content)
trajectory = json.loads(trajectory_json)

# Access fields
print(trajectory["outcome"])  # "success" or "failed"
print(trajectory["total_cost"]["dollar_cost"])
```

## Performance

The Python bindings provide near-native Rust performance:
- **Parsing**: ~10-50ms for typical logs (1000-5000 lines)
- **Graph Building**: ~1-5ms (deterministic, no LLM)
- **SVG Rendering**: ~1-2ms (Rust renderer)

Much faster than pure Python implementations due to zero-copy memory sharing between Rust and Python.

## Development

Run tests:

```bash
# Install dev dependencies
pip install pytest

# Build the extension
maturin develop --features python

# Run tests
pytest trajlens-py/tests/
```

## Comparison: CLI vs Python Bindings

| Feature | CLI | Python Bindings |
|---------|-----|-----------------|
| Parsing | ✅ | ✅ |
| Activity Graph | ✅ | ✅ |
| Cost Map | ✅ | ✅ |
| Goal Tree | ❌ (requires LLM) | ❌ (requires LLM) |
| Reasoning DAG | ❌ (requires LLM) | ❌ (requires LLM) |
| SVG Rendering | ✅ | ✅ |
| Batch Processing | ✅ | ⚠️ (manual loop) |
| Scripting | ⚠️ (shell) | ✅ (Python) |
| Installation | Single binary | `pip install` |

For LLM-based graphs (G1, G2), use the Rust library directly or integrate with Python LLM libraries.

## License

MIT
