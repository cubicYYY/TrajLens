# Compiler Feature Matrix

TrajLens supports multiple renderer backends via optional feature flags. This allows you to build only the renderers you need, reducing binary size and compile times.

## Available Renderers

| Renderer | Feature Flag | Output Type | Use Case |
|----------|-------------|-------------|----------|
| SVG (Rust) | `svg-rust` | `String` | Static images, pure Rust, no dependencies |
| SVG (Python) | `svg-python` | `Result<String, String>` | Python subprocess, requires Python 3.12+ |
| React Flow | `reactflow` | `serde_json::Value` | Interactive web visualization |
| Neo4j | `neo4j` | `Vec<String>` | Graph database import (Cypher statements) |

## Build Examples

### Library Only (No Renderers)
```bash
cargo build --no-default-features
```
Builds: Core models, parsers, graph builders, IGR serialization only.

### Default Build (CLI + Rust SVG)
```bash
cargo build
```
Builds: Library + CLI binary with Rust SVG renderer (default features).

### All Renderers
```bash
cargo build --features all-compilers
```
Builds: CLI with SVG, React Flow, and Neo4j renderers.

### Custom Renderer Combination
```bash
cargo build --no-default-features --features cli,reactflow,neo4j
```
Builds: CLI with only React Flow and Neo4j renderers (no SVG).

### WASM Build
```bash
wasm-pack build --target web --no-default-features --features wasm
```
Builds: WASM bindings without CLI or renderer dependencies.

## Feature Flags

### Core Features
- `cli`: Enables CLI binary with clap, rayon, anyhow, glob dependencies
- `wasm`: Enables WASM bindings with wasm-bindgen

### Compiler Features
- `svg-rust`: SVG string output (pure Rust, no external dependencies)
- `svg-python`: SVG string output (Python subprocess, requires `uv sync`)
- `renderer-svg`: Alias for `svg-rust` (backwards compatibility)
- `reactflow`: JSON output for React Flow library (positioned nodes/edges)
- `neo4j`: Cypher statement output for Neo4j database import
- `all-compilers`: Meta-feature enabling all graph compiler plugins (both SVG + React Flow + Neo4j)

## Usage in Code

### Using a Specific Renderer

**Rust SVG Renderer:**
```rust
use trajlens::rendering::{Renderer, SVGCompiler};
use trajlens::models::GraphEnum;

let graph = GraphEnum::ActivityGraph(/* ... */);
let renderer = SVGCompiler::new();
let svg_string = renderer.render(&graph);
```

**Python SVG Renderer:**
```rust
use trajlens::rendering::{Renderer, SVGPythonCompiler};
use trajlens::models::GraphEnum;

let graph = GraphEnum::ActivityGraph(/* ... */);
let renderer = SVGPythonCompiler::new();
let svg_result = renderer.render(&graph); // Returns Result<String, String>
match svg_result {
    Ok(svg) => println!("Success: {}", svg),
    Err(e) => eprintln!("Python renderer failed: {}", e),
}
```

### Conditional Compilation

The renderer modules are only available when their corresponding features are enabled:

```rust
#[cfg(feature = "svg-rust")]
use trajlens::rendering::SVGCompiler;

#[cfg(feature = "svg-python")]
use trajlens::rendering::SVGPythonCompiler;

#[cfg(feature = "reactflow")]
use trajlens::rendering::ReactFlowCompiler;

#[cfg(feature = "neo4j")]
use trajlens::rendering::Neo4jCompiler;
```

## CLI Binary Requirements

The CLI binary (`src/bin/cli.rs`) requires the `cli` feature and at least one SVG renderer feature (`svg-rust` or `svg-python`). The binary will not be built with `--no-default-features` unless you explicitly add `--features cli,svg-rust` (or `svg-python`).

When both SVG renderers are enabled, the Rust renderer takes precedence (faster, no subprocess overhead).

## Test Coverage

All renderer features have unit tests. Run tests for all renderers:

```bash
cargo test --features all-compilers
```

## Architecture

All renderers implement the `GraphCompiler` trait:

```rust
pub trait Renderer {
    type Output;
    fn render(&self, graph: &GraphEnum) -> Self::Output;
    fn name(&self) -> &'static str;
}
```

Each renderer chooses its own output type:
- SVG: `Output = String` (XML markup)
- React Flow: `Output = serde_json::Value` (positioned JSON)
- Neo4j: `Output = Vec<String>` (Cypher statements)

This design allows easy extension: new renderers can be added by:
1. Creating a new module in `src/compilers/`
2. Implementing the `GraphCompiler` trait
3. Adding a feature flag in `Cargo.toml`
4. Adding conditional compilation in `src/compilers/mod.rs`

## Binary Size Comparison

Approximate binary sizes (release build):

| Configuration | Binary Size |
|--------------|-------------|
| Library only | N/A (no binary) |
| CLI + Rust SVG only | ~4.2 MB |
| CLI + Python SVG only | ~3.8 MB (no rendering code, just subprocess call) |
| CLI + All renderers | ~4.6 MB |
| WASM bundle | ~1.5 MB |

The size difference between renderer combinations is minimal since the renderers are lightweight (mostly string formatting and JSON serialization). The Python renderer is smaller because it delegates all rendering logic to an external Python process.
Se