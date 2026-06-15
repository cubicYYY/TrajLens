/// Common interface for all TrajLens graph compilers.
///
/// A GraphCompiler transforms a graph (represented as GraphEnum) into some
/// output format chosen by the implementor via the associated type:
///
/// - SVG compiler → Output = String (XML document)
/// - React Flow compiler → Output = serde_json::Value (positioned nodes/edges)
/// - Neo4j compiler → Output = Vec<String> (Cypher statements)
/// - Custom compilers → whatever they need
///
/// Graph compilers are stateless: same input always produces same output.
/// Any layout computation (Sugiyama, treemap, force-directed) is internal
/// to the compiler, using shared utilities from `compilers::layout` if needed.
use crate::models::GraphEnum;

/// The core graph compiler trait. Implement this for each output backend.
pub trait GraphCompiler {
    /// The type produced by this compiler (SVG string, JSON, Cypher, etc.).
    type Output;

    /// Transform a graph into the compiler's output format.
    fn compile(&self, graph: &GraphEnum) -> Self::Output;

    /// Human-readable name for this compiler (e.g. "svg", "reactflow", "neo4j").
    fn name(&self) -> &'static str;
}
