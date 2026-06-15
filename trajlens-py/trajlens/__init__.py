"""
TrajLens: Transform agent execution logs into structured multi-graph visualizations.

This is a Python wrapper around the Rust implementation of TrajLens, providing
a Pythonic API for parsing logs, building graphs, and rendering visualizations.

# Quick Start

```python
import trajlens

# Parse a log file
with open("agent.log") as f:
    trajectory = trajlens.parse_log("auto", f.read())

# Build graphs
activity_graph = trajlens.build_activity_graph(trajectory)
cost_map = trajlens.build_cost_map(trajectory)

# Render to SVG
svg = trajlens.render_svg(activity_graph)
with open("graph.svg", "w") as f:
    f.write(svg)

# Or work with IGR format
igr_toml = trajlens.to_igr_toml(activity_graph)
graph_back = trajlens.from_igr_toml(igr_toml)
```

# API Reference

## Parsing
- `parse_log(format, content)` - Parse raw log into Trajectory JSON

## Graph Building
- `build_activity_graph(trajectory_json)` - Build Activity Graph (G3)
- `build_cost_map(trajectory_json, goal_tree_json=None)` - Build Cost Map (G4)

## Rendering
- `render_svg(graph_json)` - Render graph to SVG string

## IGR Interchange
- `to_igr_toml(graph_json)` - Serialize graph to IGR TOML
- `from_igr_toml(igr_toml)` - Deserialize IGR TOML to graph
"""

# Import the Rust extension module
from .trajlens import (
    parse_log,
    build_activity_graph,
    build_cost_map,
    to_igr_toml,
    from_igr_toml,
    render_svg,
    __version__,
)

__all__ = [
    "parse_log",
    "build_activity_graph",
    "build_cost_map",
    "to_igr_toml",
    "from_igr_toml",
    "render_svg",
    "__version__",
]
