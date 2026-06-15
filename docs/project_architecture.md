# TrajLens Architecture

## Technology Stack

| Layer | Technology |
|-------|-----------|
| Core Library | Rust (single crate with feature flags) |
| CLI | Rust + clap (feature: `cli`) |
| LLM Integration | Generic trait + Anthropic/Bedrock (features: `llm-anthropic`, `llm-bedrock`) |
| Python Bindings | PyO3 + maturin (feature: `python`) |
| WASM Module | wasm-bindgen (feature: `wasm`) |
| Web Frontend | Vite + React + React Flow |
| Renderers | SVG (Rust/Python), React Flow JSON, Neo4j Cypher (all optional) |
| Testing | Rust (cargo test), insta for snapshots |
| Serialization | TOML (IGR format) |

## Directory Layout

```
TrajLens/
├── Cargo.toml              # Workspace root
├── trajlens/               # Main crate (lib + bin + features)
│   ├── Cargo.toml          # Feature flags: cli, wasm, python, llm-*, renderer-*
│   ├── src/
│   │   ├── lib.rs          # Public library API
│   │   ├── bin/
│   │   │   └── cli.rs      # CLI binary (feature = "cli")
│   │   ├── models.rs       # All data structures
│   │   ├── igr.rs          # IGR TOML serialization
│   │   ├── parsing/        # Log parsers
│   │   │   ├── mod.rs
│   │   │   ├── claude_code.rs
│   │   │   ├── pocgen.rs
│   │   │   └── cost_estimator.rs
│   │   ├── graphs/         # Graph builders
│   │   │   ├── mod.rs
│   │   │   ├── activity_graph.rs  # G3 (deterministic)
│   │   │   ├── cost_map.rs        # G4 (deterministic)
│   │   │   ├── goal_tree.rs       # G1 (LLM-based) [TODO]
│   │   │   └── reasoning_dag.rs   # G2 (LLM-based) [TODO]
│   │   ├── llm/            # LLM integration (optional)
│   │   │   ├── mod.rs
│   │   │   ├── traits.rs
│   │   │   ├── anthropic.rs
│   │   │   └── bedrock.rs
│   │   ├── compilers/      # Graph compiler plugins
│   │   │   ├── mod.rs
│   │   │   ├── traits.rs
│   │   │   ├── layout.rs          # Sugiyama layout
│   │   │   ├── svg_rust/          # Pure Rust SVG
│   │   │   ├── svg_python/        # Python subprocess SVG
│   │   │   ├── reactflow/         # React Flow JSON
│   │   │   └── neo4j/             # Neo4j Cypher
│   │   ├── wasm.rs         # WASM exports (feature = "wasm")
│   │   └── python.rs       # Python bindings (feature = "python")
│   ├── tests/              # Integration tests
│   │   ├── test_parsers.rs
│   │   ├── test_builders.rs
│   │   └── test_igr.rs
├── trajlens-py/            # Python package (maturin build)
│   ├── trajlens/
│   │   └── __init__.py
│   ├── tests/
│   │   └── test_trajlens.py
│   └── README.md
├── trajlens-web/           # React frontend (Vite + React Flow)
│   ├── package.json
│   ├── src/
│   │   ├── components/
│   │   └── wasm/           # Generated WASM artifacts
│   └── public/
└── example_trajectories/   # Test fixtures
    ├── G4_architecture/
    └── ...
```

## Core Data Models (`models.rs`)

### Item

```rust
pub struct Item {
    pub item_id: String,
    pub category: ItemCategory,  // Read, Write, Edit, Run, Think, Other, Unknown
    pub detail: String,
    pub primary_object: String,
    pub cost: Cost,
}
```

### Cost

```rust
pub struct Cost {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub dollar_cost: f64,
}
```

### Turn

```rust
pub struct Turn {
    pub turn_id: String,
    pub items: Vec<Item>,
}
```

### Trajectory

```rust
pub struct Trajectory {
    pub turns: Vec<Turn>,
    pub outcome: String,  // "success", "failed", etc.
    pub total_cost: Cost,
}
```

## Graph Types

### Goal Transition Tree (G1) - LLM Required

```rust
pub struct GoalTransitionTree {
    pub nodes: Vec<GoalNode>,
    pub edges: Vec<GoalEdge>,
}

pub struct GoalNode {
    pub node_id: String,
    pub label: String,
    pub status: String,  // "achieved", "failed", etc.
    pub cost: Cost,
}
```

### Reasoning Artifact DAG (G2) - LLM Required

```rust
pub struct ReasoningArtifactDAG {
    pub nodes: Vec<ReasoningArtifactNode>,
    pub edges: Vec<ReasoningEdge>,
}

pub struct ReasoningArtifactNode {
    pub node_id: String,
    pub artifact_type: String,
    pub content: String,
    pub confidence: f64,
}
```

### Activity Graph (G3) - Deterministic

```rust
pub struct ActivityGraph {
    pub nodes: Vec<ActivityNode>,
    pub edges: Vec<ActivityEdge>,
}

pub struct ActivityNode {
    pub node_id: String,
    pub label: String,
    pub goal_category: GoalCategory,  // Read, Write, Edit, etc.
    pub primary_object: String,
    pub parent_id: Option<String>,
    pub operations: Vec<Operation>,
    pub total_cost: Cost,
}
```

### Cost Map (G4) - Deterministic

```rust
pub struct CostMap {
    pub root: CostMapNode,
}

pub struct CostMapNode {
    pub node_id: String,
    pub label: String,
    pub cost: Cost,
    pub children: Vec<CostMapNode>,
    pub category: Option<String>,
}
```

## Interface Definitions

### Parser Trait (`parsing/mod.rs`)

```rust
pub trait Parser {
    fn parse(&self, raw_text: &str) -> Trajectory;
}

pub struct ClaudeCodeParser;
pub struct PocgenParser;
```

Auto-detection:
```rust
pub fn detect_parser(log_lines: &[&str]) -> Box<dyn Parser>;
```

### LLM Client Trait (`llm/traits.rs`)

```rust
#[async_trait]
pub trait LLMClient: Send + Sync {
    async fn complete(&self, system_prompt: &str, user_message: &str) 
        -> LLMResult<String>;
    fn model_id(&self) -> &str;
    fn provider(&self) -> &str;
}

pub struct AnthropicClient { /* ... */ }
pub struct BedrockClient { /* ... */ }
```

### Renderer Trait (`compilers/traits.rs`)

```rust
pub trait Renderer {
    type Output;
    fn render(&self, graph: &GraphEnum) -> Self::Output;
    fn name(&self) -> &'static str;
}

pub struct SVGCompiler;      // Output = String
pub struct SVGPythonCompiler;    // Output = Result<String, String>
pub struct ReactFlowCompiler;    // Output = serde_json::Value
pub struct Neo4jCompiler;        // Output = Vec<String>
```

### IGR Serialization (`igr.rs`)

```rust
pub fn serialize(graph: &GraphEnum) -> Result<String, String>;
pub fn deserialize(toml: &str) -> Result<GraphEnum, String>;

pub enum GraphEnum {
    ActivityGraph(ActivityGraph),
    CostMap(CostMap),
    GoalTree(GoalTransitionTree),
    ReasoningDAG(ReasoningArtifactDAG),
}
```

## Pipeline Flow

```
Raw Log Text
     │
     ▼
┌─────────────────┐
│ Format Detection│ ── detect_parser(lines)
└────────┬────────┘
         │ Box<dyn Parser>
         ▼
┌─────────────────┐
│   Log Parser    │ ── parse(raw_text) → Trajectory
└────────┬────────┘
         │ Trajectory
         ▼
┌─────────────────┐
│ Cost Estimator  │ ── estimate_costs(&trajectory)
└────────┬────────┘
         │ Trajectory (with costs)
         ▼
    ┌────┴──────────────────┐
    │              │         │
    ▼              ▼         ▼
┌─────────┐  ┌─────────┐  ┌─────────┐
│G3 Build │  │G4 Build │  │G1 Build │ (async, LLM)
│(sync)   │  │(sync)   │  │         │
└────┬────┘  └────┬────┘  └────┬────┘
     │            │             │
     └────────────┼─────────────┘
                  ▼
           ┌─────────────┐
           │ IGR (TOML)  │
           └──────┬──────┘
                  │
                  ▼
           ┌─────────────┐
           │  Renderer   │
           └──────┬──────┘
                  │
                  ▼
              SVG / JSON / Cypher
```

## Feature Flag Architecture

```toml
[features]
default = ["cli", "svg-rust"]

# Core features
cli = ["clap", "rayon", "anyhow", "glob"]
wasm = ["wasm-bindgen"]
python = ["pyo3", "svg-rust"]

# LLM providers (optional, for G1/G2)
llm = ["async-trait", "tokio"]
llm-anthropic = ["llm", "reqwest"]
llm-bedrock = ["llm", "aws-config", "aws-sdk-bedrockruntime"]
all-llm = ["llm-anthropic", "llm-bedrock"]

# Graph compiler plugins (optional)
svg-rust = []
svg-python = []
reactflow = []
neo4j = []
all-compilers = ["svg-rust", "svg-python", 
                 "reactflow", "neo4j"]
```

**Build Combinations:**

```bash
# Library only (minimal)
cargo build --no-default-features

# CLI with Rust SVG (default)
cargo build

# CLI with all renderers
cargo build --features all-compilers

# CLI with LLM support (for G1/G2)
cargo build --features llm-anthropic

# Python bindings
maturin develop --features python

# WASM for web
wasm-pack build --features wasm --target web
```

## Key Design Decisions

1. **Single Crate Architecture**
   - One `trajlens` crate with multiple build targets
   - Feature flags for optional functionality
   - No artificial separation (trajlens-core, trajlens-cli, etc.)

2. **Generic LLM Interface**
   - Provider-agnostic `LLMClient` trait
   - Async/await for non-blocking I/O
   - Implementations for Anthropic and Bedrock
   - Future: OpenAI, Google, etc.

3. **Graph Compiler Plugins**
   - Trait-based with associated `Output` type
   - Each renderer chooses its own output format
   - Feature-gated to avoid unnecessary dependencies

4. **IGR-First**
   - Every graph serializes to TOML (Intermediate Graph Representation)
   - Renderers consume IGR, not raw graph objects
   - Enables language/tool interop

5. **Deterministic Core**
   - G3 (Activity Graph) and G4 (Cost Map) are fully deterministic
   - No LLM calls, no randomness
   - Fast batch processing

6. **Optional LLM**
   - G1 (Goal Tree) and G2 (Reasoning DAG) require LLM
   - LLM features are opt-in via feature flags
   - ~20MB dependency savings when not needed

7. **Python Interop**
   - PyO3 bindings for Python users
   - Near-native Rust performance
   - Simple JSON-based API

8. **WASM for Web**
   - Browser-native parsing and graph building
   - No server required for deterministic graphs
   - React Flow for interactive visualization

## Comparison: Rust vs Python Implementation

| Aspect | Rust | Python (legacy) |
|--------|------|-----------------|
| Parsing | 10-50ms | 200-1000ms |
| Graph Building | 1-5ms | 50-200ms |
| Memory | 5-10MB | 50-100MB |
| Distribution | Single binary | Python + deps |
| Type Safety | Compile-time | Runtime |
| Async LLM | Tokio | asyncio |
| Batch Processing | rayon (parallel) | Sequential |

## Testing Strategy

### Unit Tests

```bash
cargo test                    # All unit tests
cargo test --features all-llm # Including LLM tests (ignored)
```

### Integration Tests

Located in `trajlens/tests/`:
- `test_parsers.rs` - Parser output validation
- `test_builders.rs` - Graph construction correctness
- `test_igr.rs` - Serialization roundtrips

Uses `insta` for snapshot testing:
```bash
cargo insta review  # Review new snapshots
cargo insta accept  # Accept all
```

### Test Fixtures

`example_trajectories/` contains real agent logs:
- Claude Code format
- PoCGen format
- Various outcomes (success, failed, timeout)

## Distribution

| Target | Build Command | Output |
|--------|--------------|--------|
| CLI Binary | `cargo build --release` | Single executable |
| Python Package | `maturin build --release` | Wheel file |
| WASM Module | `wasm-pack build --features wasm` | .wasm + JS bindings |
| Web App | `cd trajlens-web && npm run build` | Static site |

## Documentation

- `CLAUDE.md` — Developer guide for Claude Code
- `docs/project_specification.md` — Original requirements and graph definitions
- `docs/project_architecture.md` — This file: tech stack, models, pipeline
- `docs/graphs/goal_tree.md` — Goal Transition Tree (G1)
- `docs/graphs/reasoning_dag.md` — Reasoning Artifact DAG (G2)
- `docs/graphs/activity_graph.md` — Activity Graph (G3)
- `docs/graphs/cost_map.md` — Cost Map (G4)
- `docs/llm/overview.md` — LLM integration (providers, models, usage)
- `docs/llm/bedrock.md` — AWS Bedrock setup and verified model IDs
- `docs/config.md` — Configuration system (91 parameters)
- `docs/parser_architecture.md` — Format-agnostic parser design
- `docs/python_wrapper.md` — Python bindings (PyO3)
- `docs/renderer_features.md` — Renderer feature matrix
- `docs/dev_rules.md` — Development principles
