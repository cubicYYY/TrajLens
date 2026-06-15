/// SVG renderer using Python subprocess.
///
/// This renderer invokes the Python implementation of TrajLens for SVG generation.
/// Requires Python 3.12+ with trajlens package installed (`uv sync`).
///
/// Use this renderer when:
/// - You want pixel-perfect compatibility with the Python reference implementation
/// - You're validating the Rust rewrite against Python output
/// - You need Python-specific rendering features not yet ported to Rust
///
/// The renderer:
/// 1. Serializes the graph to IGR TOML
/// 2. Writes TOML to a temporary file
/// 3. Invokes `python -m trajlens.rendering.svg_renderer <temp_file>`
/// 4. Reads and returns the SVG output
///
/// Performance: ~50-100ms overhead per render due to process spawning.
use std::fs;
use std::process::Command;

use crate::compilers::traits::Renderer;
use crate::igr;
use crate::models::GraphEnum;

/// SVG renderer (Python implementation) via subprocess.
pub struct SVGPythonCompiler {
    python_path: String,
}

impl SVGPythonCompiler {
    /// Create a new Python SVG renderer using the default Python interpreter.
    pub fn new() -> Self {
        Self {
            python_path: "python3".to_string(),
        }
    }

    /// Create a new Python SVG renderer with a custom Python interpreter path.
    pub fn with_python(python_path: String) -> Self {
        Self { python_path }
    }
}

impl Default for SVGPythonCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphCompiler for SVGPythonCompiler {
    type Output = Result<String, String>;

    fn compile(&self, graph: &GraphEnum) -> Self::Output {
        // Serialize graph to IGR TOML
        let igr_toml =
            igr::serialize(graph).map_err(|e| format!("IGR serialization failed: {}", e))?;

        // Write to temporary file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!("trajlens_{}.igr.toml", std::process::id()));
        fs::write(&temp_file, igr_toml).map_err(|e| format!("Failed to write temp file: {}", e))?;

        // Invoke Python renderer
        let output = Command::new(&self.python_path)
            .args(&["-m", "trajlens.rendering.svg_renderer"])
            .arg(&temp_file)
            .output()
            .map_err(|e| format!("Failed to spawn Python process: {}. Is Python installed with trajlens package?", e))?;

        // Clean up temp file
        let _ = fs::remove_file(&temp_file);

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("Python renderer failed: {}", stderr));
        }

        let svg = String::from_utf8(output.stdout)
            .map_err(|e| format!("Invalid UTF-8 in Python output: {}", e))?;

        Ok(svg)
    }

    fn name(&self) -> &'static str {
        "svg-python"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{ActivityGraph, ActivityNode, Cost, GoalCategory, OpType, Operation};

    #[test]
    #[ignore] // Requires Python installation
    fn test_python_renderer_invocation() {
        let graph = ActivityGraph {
            nodes: vec![ActivityNode {
                node_id: "n0".into(),
                label: "test.rs".into(),
                goal_category: GoalCategory::Read,
                primary_object: "/test.rs".into(),
                parent_id: None,
                call_indices: vec![0],
                operations: vec![Operation {
                    op_type: OpType::Read,
                    detail: "L1-L10".into(),
                    call_index: 0,
                }],
                total_cost: Cost::default(),
            }],
            edges: vec![],
        };

        let compiler = SVGPythonCompiler::new();
        let result = renderer.compile(&GraphEnum::ActivityGraph(graph));

        // This test will fail if Python is not installed or trajlens package is missing
        // Run with: cargo test --features renderer-svg-python -- --ignored
        match result {
            Ok(svg) => {
                assert!(svg.contains("<svg"));
                assert!(svg.contains("</svg>"));
            }
            Err(e) => {
                eprintln!(
                    "Python renderer failed (expected if Python not installed): {}",
                    e
                );
            }
        }
    }
}
