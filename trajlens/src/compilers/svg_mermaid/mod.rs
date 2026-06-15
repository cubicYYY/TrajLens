/// SVG renderer via Mermaid CLI (mmdc).
///
/// Reuses the Mermaid renderer to produce diagram syntax, then invokes
/// `mmdc` (mermaid-cli) to convert it to SVG. Requires `mmdc` in PATH.
///
/// Install mermaid-cli: `npm install -g @mermaid-js/mermaid-cli`
///
/// Use this renderer when you want Mermaid's layout engine (dagre/elk)
/// to handle positioning instead of our Sugiyama implementation.
use std::fs;
use std::process::Command;

use crate::compilers::mermaid::MermaidCompiler;
use crate::compilers::traits::Renderer;
use crate::models::GraphEnum;

/// SVG renderer that pipes Mermaid syntax through `mmdc`.
pub struct SVGMermaidCompiler {
    mmdc_path: String,
}

impl SVGMermaidCompiler {
    pub fn new() -> Self {
        Self {
            mmdc_path: "mmdc".to_string(),
        }
    }

    /// Use a custom path to the mmdc binary.
    pub fn with_mmdc_path(path: String) -> Self {
        Self { mmdc_path: path }
    }
}

impl Default for SVGMermaidCompiler {
    fn default() -> Self {
        Self::new()
    }
}

impl GraphCompiler for SVGMermaidCompiler {
    type Output = Result<String, String>;

    fn compile(&self, graph: &GraphEnum) -> Self::Output {
        let mermaid_compiler = MermaidCompiler::new();
        let mmd_content = mermaid_compiler.compile(graph);

        let temp_dir = std::env::temp_dir();
        let input_path = temp_dir.join(format!("trajlens_{}.mmd", std::process::id()));
        let output_path = temp_dir.join(format!("trajlens_{}.svg", std::process::id()));

        fs::write(&input_path, &mmd_content)
            .map_err(|e| format!("Failed to write temp .mmd file: {}", e))?;

        let result = Command::new(&self.mmdc_path)
            .args([
                "-i", input_path.to_str().unwrap_or("input.mmd"),
                "-o", output_path.to_str().unwrap_or("output.svg"),
                "--quiet",
            ])
            .output()
            .map_err(|e| format!(
                "Failed to run mmdc: {}. Is mermaid-cli installed? (npm install -g @mermaid-js/mermaid-cli)",
                e
            ))?;

        let _ = fs::remove_file(&input_path);

        if !result.status.success() {
            let _ = fs::remove_file(&output_path);
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(format!("mmdc failed: {}", stderr));
        }

        let svg = fs::read_to_string(&output_path)
            .map_err(|e| format!("Failed to read mmdc output: {}", e))?;

        let _ = fs::remove_file(&output_path);

        Ok(svg)
    }

    fn name(&self) -> &'static str {
        "svg-mermaid"
    }
}
