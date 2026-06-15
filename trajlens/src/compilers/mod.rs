/// Graph compiler module: layout algorithms and the GraphCompiler trait.
///
/// Graph compilers transform graph data (via GraphEnum / IGR) into output formats.
/// Each compiler picks its own Output type.
///
/// # Feature Flags
///
/// - `svg-rust`: SVG via pure Rust layout + string assembly
/// - `svg-python`: SVG via Python subprocess
/// - `svg-mermaid`: SVG via mermaid-cli (mmdc)
/// - `mermaid`: Mermaid.js diagram text (.mmd)
/// - `reactflow`: React Flow JSON
/// - `neo4j`: Neo4j Cypher statements
pub mod layout;
pub mod traits;

#[cfg(feature = "svg-rust")]
pub mod svg_rust;

#[cfg(feature = "svg-python")]
pub mod svg_python;

#[cfg(feature = "svg-mermaid")]
pub mod svg_mermaid;

#[cfg(feature = "mermaid")]
pub mod mermaid;

#[cfg(feature = "reactflow")]
pub mod reactflow;

#[cfg(feature = "neo4j")]
pub mod neo4j;

// Re-exports
pub use traits::GraphCompiler;

#[cfg(feature = "svg-rust")]
pub use svg_rust::SVGCompiler;

#[cfg(feature = "svg-python")]
pub use svg_python::SVGPythonCompiler;

#[cfg(feature = "svg-mermaid")]
pub use svg_mermaid::SVGMermaidCompiler;

#[cfg(feature = "mermaid")]
pub use mermaid::MermaidCompiler;

#[cfg(feature = "reactflow")]
pub use reactflow::ReactFlowCompiler;

#[cfg(feature = "neo4j")]
pub use neo4j::Neo4jCompiler;
