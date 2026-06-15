/// WASM bindings for browser usage.
///
/// Thin wrappers that expose library functions to JavaScript via wasm-bindgen.
/// Enable with the "wasm" feature flag.
///
/// # Known Limitation (TODO: solve in future)
///
/// WASM cannot execute Python subprocess parser scripts — the browser sandbox
/// forbids spawning processes. This means `parse_log` is a no-op stub that wraps
/// the entire input as a single Unknown step.
///
/// Current workaround: use the CLI to parse logs into trajectory.json, then load
/// that file in the web viewer (which uses WASM only for graph building + rendering).
///
/// Future solutions to explore:
/// - Pyodide (Python in WASM) to run parser scripts in-browser
/// - Re-implement the most common parsers in pure Rust (no subprocess)
/// - Server-side parsing API that the web viewer calls
use wasm_bindgen::prelude::*;

use crate::graphs::{activity_graph, cost_map};
use crate::igr;
use crate::models::{GoalTransitionTree, GraphEnum, Trajectory};
use crate::parsing::{self, cost_estimator, Parser};

/// Parse a log file into a Trajectory JSON string.
///
/// # Arguments
/// * `format` - Format hint (currently unused; WASM cannot run parser scripts)
/// * `content` - Raw log file content as string
///
/// # Returns
/// Trajectory as JSON string, or error message
///
/// # Limitations
/// WASM cannot execute subprocess parser scripts. This uses NoopParser which wraps
/// the entire log as a single Unknown step. Use the CLI for full parsing, then load
/// the resulting trajectory.json in the web viewer.
#[wasm_bindgen]
pub fn parse_log(format: &str, content: &str) -> Result<JsValue, JsValue> {
    let _ = format; // WASM cannot run parser scripts; format detection is informational only
    let parser = parsing::NoopParser;
    let mut trajectory = parser.parse(content);
    trajectory = cost_estimator::estimate_costs(&trajectory);

    let json = serde_json::to_string(&trajectory)
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;

    Ok(JsValue::from_str(&json))
}

/// Build an Activity Graph from Trajectory JSON.
///
/// # Arguments
/// * `trajectory_json` - Trajectory as JSON string (from parse_log)
///
/// # Returns
/// IGR TOML string for ActivityGraph
#[wasm_bindgen]
pub fn build_activity_graph(trajectory_json: &str) -> Result<JsValue, JsValue> {
    let trajectory: Trajectory = serde_json::from_str(trajectory_json)
        .map_err(|e| JsValue::from_str(&format!("Parse error: {}", e)))?;

    let graph = activity_graph::build(&trajectory);
    let igr_toml = igr::serialize(&GraphEnum::ActivityGraph(graph))
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;

    Ok(JsValue::from_str(&igr_toml))
}

/// Build a Cost Map from Trajectory JSON.
///
/// # Arguments
/// * `trajectory_json` - Trajectory as JSON string
/// * `goal_tree_json` - Optional GoalTransitionTree as JSON string
///
/// # Returns
/// IGR TOML string for CostMap
#[wasm_bindgen]
pub fn build_cost_map(
    trajectory_json: &str,
    goal_tree_json: Option<String>,
) -> Result<JsValue, JsValue> {
    let trajectory: Trajectory = serde_json::from_str(trajectory_json)
        .map_err(|e| JsValue::from_str(&format!("Parse trajectory error: {}", e)))?;

    let goal_tree_opt = if let Some(json) = goal_tree_json {
        let tree: GoalTransitionTree = serde_json::from_str(&json)
            .map_err(|e| JsValue::from_str(&format!("Parse goal tree error: {}", e)))?;
        Some(tree)
    } else {
        None
    };

    let graph = cost_map::build(&trajectory, goal_tree_opt.as_ref());
    let igr_toml = igr::serialize(&GraphEnum::CostMap(graph))
        .map_err(|e| JsValue::from_str(&format!("Serialization error: {}", e)))?;

    Ok(JsValue::from_str(&igr_toml))
}

/// Convert IGR TOML to graph object JSON for consumption by JavaScript.
///
/// # Arguments
/// * `igr_toml` - IGR TOML string
///
/// # Returns
/// Graph object as JSON (type-specific structure)
#[wasm_bindgen]
pub fn igr_to_json(igr_toml: &str) -> Result<JsValue, JsValue> {
    let graph = igr::deserialize(igr_toml)
        .map_err(|e| JsValue::from_str(&format!("Deserialization error: {}", e)))?;

    let json = match graph {
        GraphEnum::ActivityGraph(ag) => serde_json::to_string(&ag),
        GraphEnum::CostMap(cm) => serde_json::to_string(&cm),
        GraphEnum::GoalTree(gt) => serde_json::to_string(&gt),
        GraphEnum::ReasoningDAG(dag) => serde_json::to_string(&dag),
    }
    .map_err(|e| JsValue::from_str(&format!("JSON serialization error: {}", e)))?;

    Ok(JsValue::from_str(&json))
}
