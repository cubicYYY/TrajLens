/// Python bindings for TrajLens using PyO3.
///
/// Exposes Rust functionality to Python via native extension module.
/// Build with: `maturin develop` or `maturin build --release`
///
/// # Python API
///
/// ```python
/// import trajlens
///
/// # Parse a log file
/// trajectory = trajlens.parse_log("claude-code", log_content)
///
/// # Build graphs
/// activity_graph = trajlens.build_activity_graph(trajectory)
/// cost_map = trajlens.build_cost_map(trajectory)
///
/// # Render to SVG
/// svg = trajlens.render_svg(activity_graph)
///
/// # Serialize/deserialize IGR
/// igr_toml = trajlens.to_igr_toml(activity_graph)
/// graph = trajlens.from_igr_toml(igr_toml)
/// ```
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;

use crate::graphs::{activity_graph, cost_map};
use crate::igr;
use crate::models::{GraphEnum, Trajectory};
use crate::parsing::{self, cost_estimator, parser_registry, script_runner};

#[cfg(feature = "svg-rust")]
use crate::compilers::{GraphCompiler, SVGCompiler};

/// Parse a log file into a Trajectory.
///
/// Args:
///     format (str): Log format name or "auto" for auto-detection
///     content (str): Raw log content
///
/// Returns:
///     str: Trajectory as JSON string
///
/// Raises:
///     ValueError: If format is unknown or parsing fails
///
/// Note: This writes content to a temp file and invokes the parser script.
/// For best performance, use the CLI directly.
#[pyfunction]
fn parse_log(format: &str, content: &str) -> PyResult<String> {
    let registry = parser_registry::ParserRegistry::load_default()
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to load parser registry: {}", e)))?;

    let config = match format {
        "auto" => {
            let detected = registry.detect_format(content).ok_or_else(|| {
                PyValueError::new_err(format!(
                    "Could not auto-detect format. Available: {:?}",
                    registry.list_formats()
                ))
            })?;
            registry
                .get(&detected)
                .ok_or_else(|| PyRuntimeError::new_err("Config not found"))?
                .clone()
        }
        name => registry
            .get(name)
            .ok_or_else(|| {
                PyValueError::new_err(format!(
                    "Unknown format: {}. Available: {:?}",
                    name,
                    registry.list_formats()
                ))
            })?
            .clone(),
    };

    // Write content to temp file for script execution
    let tmp = tempfile::NamedTempFile::new()
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to create temp file: {}", e)))?;
    std::fs::write(tmp.path(), content)
        .map_err(|e| PyRuntimeError::new_err(format!("Failed to write temp file: {}", e)))?;

    let scripts_dir = script_runner::find_scripts_dir();
    let parser = script_runner::ScriptParser::new(config, scripts_dir);
    let mut trajectory = parser
        .parse_file(tmp.path())
        .map_err(|e| PyRuntimeError::new_err(format!("Parsing failed: {}", e)))?;

    trajectory = cost_estimator::estimate_costs(&trajectory);

    let json = serde_json::to_string(&trajectory)
        .map_err(|e| PyRuntimeError::new_err(format!("JSON serialization failed: {}", e)))?;

    Ok(json)
}

/// Build an Activity Graph from a Trajectory.
///
/// Args:
///     trajectory_json (str): Trajectory as JSON string (from parse_log)
///
/// Returns:
///     str: Activity Graph as JSON string
///
/// Raises:
///     ValueError: If JSON is invalid
#[pyfunction]
fn build_activity_graph(trajectory_json: &str) -> PyResult<String> {
    let trajectory: Trajectory = serde_json::from_str(trajectory_json)
        .map_err(|e| PyValueError::new_err(format!("Invalid Trajectory JSON: {}", e)))?;

    let graph = activity_graph::build(&trajectory);
    let json = serde_json::to_string(&graph)
        .map_err(|e| PyRuntimeError::new_err(format!("JSON serialization failed: {}", e)))?;

    Ok(json)
}

/// Build a Cost Map from a Trajectory.
///
/// Args:
///     trajectory_json (str): Trajectory as JSON string (from parse_log)
///     goal_tree_json (str | None): Optional Goal Tree JSON for categorization
///
/// Returns:
///     str: Cost Map as JSON string
///
/// Raises:
///     ValueError: If JSON is invalid
#[pyfunction]
#[pyo3(signature = (trajectory_json, goal_tree_json=None))]
fn build_cost_map(trajectory_json: &str, goal_tree_json: Option<&str>) -> PyResult<String> {
    let trajectory: Trajectory = serde_json::from_str(trajectory_json)
        .map_err(|e| PyValueError::new_err(format!("Invalid Trajectory JSON: {}", e)))?;

    let goal_tree_opt = if let Some(gt_json) = goal_tree_json {
        let tree: crate::models::GoalTransitionTree = serde_json::from_str(gt_json)
            .map_err(|e| PyValueError::new_err(format!("Invalid Goal Tree JSON: {}", e)))?;
        Some(tree)
    } else {
        None
    };

    let cost_map = cost_map::build(&trajectory, goal_tree_opt.as_ref());
    let json = serde_json::to_string(&cost_map)
        .map_err(|e| PyRuntimeError::new_err(format!("JSON serialization failed: {}", e)))?;

    Ok(json)
}

/// Serialize an Activity Graph to IGR TOML format.
///
/// Args:
///     graph_json (str): ActivityGraph or CostMap as JSON string
///
/// Returns:
///     str: IGR TOML string
///
/// Raises:
///     ValueError: If JSON is invalid or serialization fails
#[pyfunction]
fn to_igr_toml(graph_json: &str) -> PyResult<String> {
    // Try to parse as ActivityGraph first
    if let Ok(ag) = serde_json::from_str::<crate::models::ActivityGraph>(graph_json) {
        let toml = igr::serialize(&GraphEnum::ActivityGraph(ag))
            .map_err(|e| PyRuntimeError::new_err(format!("IGR serialization failed: {}", e)))?;
        return Ok(toml);
    }

    // Try CostMap
    if let Ok(cm) = serde_json::from_str::<crate::models::CostMap>(graph_json) {
        let toml = igr::serialize(&GraphEnum::CostMap(cm))
            .map_err(|e| PyRuntimeError::new_err(format!("IGR serialization failed: {}", e)))?;
        return Ok(toml);
    }

    Err(PyValueError::new_err(
        "Invalid graph JSON: must be ActivityGraph or CostMap",
    ))
}

/// Deserialize an IGR TOML string into a graph.
///
/// Args:
///     igr_toml (str): IGR TOML string
///
/// Returns:
///     str: Graph as JSON string
///
/// Raises:
///     ValueError: If TOML is invalid or deserialization fails
#[pyfunction]
fn from_igr_toml(igr_toml: &str) -> PyResult<String> {
    let graph = igr::deserialize(igr_toml)
        .map_err(|e| PyValueError::new_err(format!("IGR deserialization failed: {}", e)))?;

    let json = match graph {
        GraphEnum::ActivityGraph(ag) => serde_json::to_string(&ag),
        GraphEnum::CostMap(cm) => serde_json::to_string(&cm),
        GraphEnum::GoalTree(gt) => serde_json::to_string(&gt),
        GraphEnum::ReasoningDAG(dag) => serde_json::to_string(&dag),
    }
    .map_err(|e| PyRuntimeError::new_err(format!("JSON serialization failed: {}", e)))?;

    Ok(json)
}

/// Render a graph to SVG using the Rust renderer.
///
/// Args:
///     graph_json (str): Graph as JSON string
///
/// Returns:
///     str: SVG markup
///
/// Raises:
///     ValueError: If JSON is invalid
///     RuntimeError: If renderer is not available (requires svg-rust feature)
#[pyfunction]
#[cfg(feature = "svg-rust")]
fn render_svg(graph_json: &str) -> PyResult<String> {
    // Try to parse as different graph types
    let graph = if let Ok(ag) = serde_json::from_str::<crate::models::ActivityGraph>(graph_json) {
        GraphEnum::ActivityGraph(ag)
    } else if let Ok(cm) = serde_json::from_str::<crate::models::CostMap>(graph_json) {
        GraphEnum::CostMap(cm)
    } else if let Ok(gt) = serde_json::from_str::<crate::models::GoalTransitionTree>(graph_json) {
        GraphEnum::GoalTree(gt)
    } else if let Ok(dag) = serde_json::from_str::<crate::models::ReasoningArtifactDAG>(graph_json)
    {
        GraphEnum::ReasoningDAG(dag)
    } else {
        return Err(PyValueError::new_err(
            "Invalid graph JSON: unrecognized format",
        ));
    };

    let compiler = SVGCompiler::new();
    let svg = renderer.compile(&graph);

    Ok(svg)
}

#[cfg(not(feature = "svg-rust"))]
#[pyfunction]
fn render_svg(_graph_json: &str) -> PyResult<String> {
    Err(PyRuntimeError::new_err(
        "SVG renderer not available. Rebuild with --features svg-rust",
    ))
}

/// TrajLens: Transform agent execution logs into structured multi-graph visualizations.
///
/// This module provides Python bindings to the Rust implementation of TrajLens.
///
/// # Basic Usage
///
/// ```python
/// import trajlens
///
/// # Parse a log
/// with open("example.log") as f:
///     trajectory = trajlens.parse_log("auto", f.read())
///
/// # Build and render an activity graph
/// activity_graph = trajlens.build_activity_graph(trajectory)
/// svg = trajlens.render_svg(activity_graph)
///
/// # Save SVG
/// with open("graph.svg", "w") as f:
///     f.write(svg)
///
/// # Or use IGR format for interchange
/// igr_toml = trajlens.to_igr_toml(activity_graph)
/// with open("graph.igr.toml", "w") as f:
///     f.write(igr_toml)
/// ```
#[pymodule]
fn trajlens(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse_log, m)?)?;
    m.add_function(wrap_pyfunction!(build_activity_graph, m)?)?;
    m.add_function(wrap_pyfunction!(build_cost_map, m)?)?;
    m.add_function(wrap_pyfunction!(to_igr_toml, m)?)?;
    m.add_function(wrap_pyfunction!(from_igr_toml, m)?)?;
    m.add_function(wrap_pyfunction!(render_svg, m)?)?;

    // Module metadata
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    m.add("__author__", "TrajLens Contributors")?;

    Ok(())
}
