# Python Wrapper for TrajLens

High-performance Python bindings to the Rust implementation using PyO3.

## Installation

### Prerequisites

- Python 3.12+
- Rust toolchain (for building from source)
- [maturin](https://github.com/PyO3/maturin) (Python-Rust build tool)

### Install from Source

```bash
# Clone the repository
git clone https://github.com/user/TrajLens
cd TrajLens

# Install maturin
pip install maturin

# Development install (editable)
maturin develop --features python

# Or build a wheel
maturin build --release --features python
pip install target/wheels/trajlens-*.whl
```

### With UV

```bash
uv pip install maturin
uv run maturin develop --features python
```

## Quick Start

```python
import trajlens
import json

# Parse a log file
with open("agent.log") as f:
    trajectory_json = trajlens.parse_log("auto", f.read())

# Build Activity Graph
activity_graph_json = trajlens.build_activity_graph(trajectory_json)

# Render to SVG
svg = trajlens.render_svg(activity_graph_json)
with open("graph.svg", "w") as f:
    f.write(svg)

print("Graph rendered successfully!")
```

## API Documentation

All data is exchanged as JSON strings. Use Python's `json` module to inspect structures.

### `parse_log(format: str, content: str) -> str`

Parse a raw agent log into a Trajectory.

**Parameters:**
- `format` (str): Log format - `"auto"`, `"claude-code"`, or `"pocgen"`
- `content` (str): Raw log content

**Returns:** Trajectory as JSON string

**Raises:** `ValueError` if format is unknown or parsing fails

```python
trajectory_json = trajlens.parse_log("auto", log_content)
trajectory = json.loads(trajectory_json)
print(f"Parsed {len(trajectory['turns'])} turns")
```

### `build_activity_graph(trajectory_json: str) -> str`

Build an Activity Graph (G3) from a Trajectory.

**Parameters:**
- `trajectory_json` (str): Trajectory JSON from `parse_log()`

**Returns:** Activity Graph as JSON string

**Raises:** `ValueError` if JSON is invalid

```python
activity_graph_json = trajlens.build_activity_graph(trajectory_json)
graph = json.loads(activity_graph_json)
print(f"Graph: {len(graph['nodes'])} nodes, {len(graph['edges'])} edges")
```

### `build_cost_map(trajectory_json: str, goal_tree_json: str | None = None) -> str`

Build a Cost Map (G4) from a Trajectory.

**Parameters:**
- `trajectory_json` (str): Trajectory JSON
- `goal_tree_json` (str | None): Optional Goal Tree JSON for categorization

**Returns:** Cost Map as JSON string

**Raises:** `ValueError` if JSON is invalid

```python
cost_map_json = trajlens.build_cost_map(trajectory_json)
cost_map = json.loads(cost_map_json)
print(f"Total cost: ${cost_map['root']['cost']['dollar_cost']:.4f}")
```

### `render_svg(graph_json: str) -> str`

Render a graph to SVG markup.

**Parameters:**
- `graph_json` (str): Graph JSON (ActivityGraph, CostMap, GoalTree, or ReasoningDAG)

**Returns:** SVG markup as string

**Raises:** 
- `ValueError` if JSON is invalid
- `RuntimeError` if renderer not available (requires `renderer-svg-rust` feature)

```python
svg_string = trajlens.render_svg(graph_json)
with open("output.svg", "w") as f:
    f.write(svg_string)
```

### `to_igr_toml(graph_json: str) -> str`

Serialize a graph to IGR TOML format.

**Parameters:**
- `graph_json` (str): Graph JSON

**Returns:** IGR TOML string

**Raises:** `ValueError` if JSON is invalid or serialization fails

```python
igr_toml = trajlens.to_igr_toml(graph_json)
with open("graph.igr.toml", "w") as f:
    f.write(igr_toml)
```

### `from_igr_toml(igr_toml: str) -> str`

Deserialize an IGR TOML string into a graph.

**Parameters:**
- `igr_toml` (str): IGR TOML content

**Returns:** Graph as JSON string

**Raises:** `ValueError` if TOML is invalid

```python
with open("graph.igr.toml") as f:
    igr_toml = f.read()
graph_json = trajlens.from_igr_toml(igr_toml)
```

## Complete Example

```python
#!/usr/bin/env python3
"""Complete TrajLens pipeline example."""
import trajlens
import json
from pathlib import Path

def process_log(log_path: str, output_dir: str):
    """Process a single log file through the complete pipeline."""
    output = Path(output_dir)
    output.mkdir(parents=True, exist_ok=True)
    
    # 1. Parse log
    print(f"Parsing {log_path}...")
    with open(log_path) as f:
        log_content = f.read()
    
    trajectory_json = trajlens.parse_log("auto", log_content)
    trajectory = json.loads(trajectory_json)
    print(f"  ✓ {len(trajectory['turns'])} turns, outcome={trajectory['outcome']}")
    
    # Save trajectory
    with open(output / "trajectory.json", "w") as f:
        f.write(trajectory_json)
    
    # 2. Build Activity Graph
    print("Building Activity Graph...")
    activity_graph_json = trajlens.build_activity_graph(trajectory_json)
    activity_graph = json.loads(activity_graph_json)
    print(f"  ✓ {len(activity_graph['nodes'])} nodes, {len(activity_graph['edges'])} edges")
    
    # Render and save
    svg = trajlens.render_svg(activity_graph_json)
    with open(output / "activity_graph.svg", "w") as f:
        f.write(svg)
    
    igr = trajlens.to_igr_toml(activity_graph_json)
    with open(output / "activity_graph.igr.toml", "w") as f:
        f.write(igr)
    print(f"  ✓ Saved SVG and IGR")
    
    # 3. Build Cost Map
    print("Building Cost Map...")
    cost_map_json = trajlens.build_cost_map(trajectory_json)
    cost_map = json.loads(cost_map_json)
    print(f"  ✓ Total cost: ${cost_map['root']['cost']['dollar_cost']:.4f}")
    
    # Render and save
    svg = trajlens.render_svg(cost_map_json)
    with open(output / "cost_map.svg", "w") as f:
        f.write(svg)
    
    igr = trajlens.to_igr_toml(cost_map_json)
    with open(output / "cost_map.igr.toml", "w") as f:
        f.write(igr)
    print(f"  ✓ Saved SVG and IGR")
    
    print(f"\n✅ Complete! All outputs in {output_dir}/")

if __name__ == "__main__":
    import sys
    if len(sys.argv) != 3:
        print("Usage: python example.py <log_file> <output_dir>")
        sys.exit(1)
    
    process_log(sys.argv[1], sys.argv[2])
```

## Performance

The Python bindings provide near-native Rust performance:

| Operation | Performance | Notes |
|-----------|------------|-------|
| Parsing (1000 lines) | ~10-20ms | Zero-copy string handling |
| Activity Graph build | ~1-3ms | Pure Rust, no Python overhead |
| Cost Map build | ~1-2ms | Deterministic algorithm |
| SVG rendering | ~1-2ms | Layout + string assembly |

**vs Pure Python:** 10-100x faster depending on operation.

## Architecture

```
Python Code
    ↓
PyO3 Bindings (trajlens/src/python.rs)
    ↓
Rust Library (trajlens/src/lib.rs)
    ↓
    ├─ Parsing (trajlens/src/parsing/)
    ├─ Graph Building (trajlens/src/graphs/)
    ├─ IGR Serialization (trajlens/src/igr.rs)
    └─ SVG Rendering (trajlens/src/rendering/svg_rust/)
```

Data flows as JSON strings between Python and Rust:
- **Advantage:** Simple API, no complex type marshaling
- **Trade-off:** Small JSON serialization overhead (~0.1-0.5ms)
- **Alternative:** For zero-overhead, use the Rust library directly

## Development

### Running Tests

```bash
# Build the extension
maturin develop --features python

# Run Python tests
pytest trajlens-py/tests/

# Or with UV
uv run pytest trajlens-py/tests/
```

### Type Checking

```bash
pip install mypy
mypy trajlens-py/trajlens/
```

### Building for Distribution

```bash
# Build wheels for current platform
maturin build --release --features python

# Build for multiple Python versions
maturin build --release --features python --interpreter python3.12 python3.13

# Wheels are created in target/wheels/
ls target/wheels/
```

## Comparison: Python Bindings vs CLI vs Rust Library

| Feature | Python Bindings | CLI | Rust Library |
|---------|----------------|-----|--------------|
| Installation | `pip install` | Single binary | `cargo add trajlens` |
| Scripting | ✅ Native Python | ⚠️ Shell scripts | ⚠️ Rust code |
| Performance | ⚡ Near-native | ⚡ Native | ⚡ Native |
| Integration | ✅ Python ecosystem | ⚠️ Subprocess | ✅ Rust ecosystem |
| Batch Processing | Manual loop | ✅ Built-in parallel | Manual with rayon |
| Flexibility | High | Medium | Highest |
| Deployment | Wheel + Python | Single file | Compile target |

**Use Python Bindings when:**
- You're already in a Python codebase
- You want to integrate with Python ML/data tools
- You need programmatic control (vs CLI)

**Use CLI when:**
- You want standalone tool (no runtime)
- You're processing logs in CI/CD pipelines
- You need parallel batch processing

**Use Rust Library when:**
- Building a Rust application
- Need absolute maximum performance
- Customizing graph builders or renderers

## Troubleshooting

### Import Error

```
ImportError: cannot import name 'trajlens' from 'trajlens'
```

**Solution:** Rebuild the extension:
```bash
maturin develop --features python
```

### Feature Not Available

```
RuntimeError: SVG renderer not available. Rebuild with --features renderer-svg-rust
```

**Solution:** The Python feature automatically includes `renderer-svg-rust`. If you built with custom features, add it:
```bash
maturin develop --features python,renderer-svg-rust
```

### Segfault or Crash

**Possible causes:**
- Mismatched Python version (build vs runtime)
- Corrupted JSON data

**Solution:**
1. Rebuild: `maturin develop --features python`
2. Check Python version matches: `python --version`
3. Validate JSON before passing to functions

## License

MIT
