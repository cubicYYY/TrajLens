# Configuration System

All configurable parameters centralized in `config.toml` — no magic numbers in code.

## Overview

TrajLens loads configuration from:
1. `./config.toml` (project-specific, highest priority)
2. `~/.config/trajlens/config.toml` (user-specific)
3. Hardcoded defaults (if no config file found)

## Quick Start

```bash
cp config.example.toml config.toml
# Edit desired sections, run normally
trajlens run input.log -o output/
```

## Config Hierarchy

```
Config (root)
├── llm: LLMConfig (Anthropic + Bedrock settings)
├── cost: CostConfig (Model pricing per 1M tokens)
├── rendering: RenderingConfig
│   ├── svg: SvgConfig (dimensions, fonts, colors)
│   ├── layout: LayoutConfig (spacing, iterations)
│   └── cost_map: CostMapConfig (area, padding)
├── parsing: ParsingConfig (estimation params)
├── cli: CliConfig (output paths, workers)
├── graph: GraphConfig (build parameters)
├── igr: IgrConfig (TOML formatting)
├── logging: LoggingConfig (level, file output)
├── performance: PerformanceConfig (buffers, cache)
└── validation: ValidationConfig (strict mode)
```

**Total: 91 parameters, all documented in `config.example.toml`.**

## Usage in Code

```rust
use crate::config::get_config;

fn my_function() {
    let config = get_config();
    let width = config.rendering.svg.node_width;
    let color = &config.rendering.svg.colors.read;
}
```

Config is lazy-loaded via `OnceLock` — first call loads from disk, subsequent calls are instant (thread-safe).

## Key Sections

### SVG Rendering

```toml
[rendering.svg]
node_width = 200.0
node_height_base = 80.0
node_height_per_operation = 20.0
font_size_label = 12.0
font_size_detail = 10.0
text_wrap_max_chars = 30
stroke_width = 1.0
edge_stroke_width = 2.0

[rendering.svg.colors]
read = "#e3f2fd"
write = "#fce4ec"
edit = "#fff3e0"
run = "#e8f5e9"
list = "#f3e5f5"
other = "#eeeeee"
edge = "#666666"
text = "#000000"
border = "#cccccc"
```

### Cost Estimation

```toml
[parsing.estimation]
chars_per_token = 4
default_model = "sonnet"  # Options: haiku, sonnet, opus

[cost.models.sonnet]
input_per_million = 3.0
output_per_million = 15.0
```

### Layout

```toml
[rendering.layout]
x_spacing = 80.0
y_spacing = 120.0
node_separation = 30.0
max_iterations = 20
```

## Adding New Parameters

1. Add struct field to `trajlens/src/config.rs`
2. Add default in `Config::default()`
3. Document in `config.example.toml`
4. Use via `get_config().section.parameter`

## Design Principles

- **No magic numbers**: Every constant in config with documentation
- **Zero breaking changes**: Application works without config file
- **No recompilation**: Edit TOML, restart — that's it
- **Type-safe**: Serde deserialization catches errors at load time

## Related Files

- `trajlens/src/config.rs` — Config module (520 lines)
- `config.toml` — Active configuration
- `config.example.toml` — All 91 parameters with comments and examples
