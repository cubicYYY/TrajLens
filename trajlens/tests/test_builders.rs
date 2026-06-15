/// Integration tests for graph builders.
/// Tests that building from parsed trajectories produces valid graphs.
use std::path::PathBuf;

use trajlens::graphs::{activity_graph, cost_map};
use trajlens::parsing::{
    cost_estimator,
    parser_registry::ParserRegistry,
    script_runner::{find_scripts_dir, ScriptParser},
};

fn sampled_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("example_trajectories")
        .join("Sampled")
}

fn parse_cyberagent() -> trajlens::models::Trajectory {
    let registry = ParserRegistry::load_default().unwrap();
    let config = registry.get("cyberagent_log").unwrap().clone();
    let scripts_dir = find_scripts_dir();
    let parser = ScriptParser::new(config, scripts_dir);
    let traj = parser
        .parse_file(&sampled_dir().join("run_20260526_030500(1).log"))
        .unwrap();
    cost_estimator::estimate_costs(&traj)
}

fn parse_cairn() -> trajlens::models::Trajectory {
    let registry = ParserRegistry::load_default().unwrap();
    let config = registry.get("cairn_project_yaml").unwrap().clone();
    let scripts_dir = find_scripts_dir();
    let parser = ScriptParser::new(config, scripts_dir);
    let traj = parser
        .parse_file(&sampled_dir().join("2bird2can.yaml"))
        .unwrap();
    cost_estimator::estimate_costs(&traj)
}

#[test]
fn test_activity_graph_from_cyberagent_log() {
    let traj = parse_cyberagent();
    let ag = activity_graph::build(&traj);

    assert!(!ag.nodes.is_empty(), "Expected at least one activity node");
    let total_ops: usize = ag.nodes.iter().map(|n| n.operations.len()).sum();
    assert!(
        total_ops >= 5,
        "Cyberagent log should produce >=5 operations, got {}",
        total_ops
    );
}

#[test]
fn test_activity_graph_from_cairn_yaml() {
    let traj = parse_cairn();
    let ag = activity_graph::build(&traj);

    assert!(!ag.nodes.is_empty());
}

#[test]
fn test_cost_map_from_cyberagent_log() {
    let traj = parse_cyberagent();
    let cm = cost_map::build(&traj, None);

    assert_eq!(cm.root.node_id, "root");
    assert!(!cm.root.children.is_empty(), "Expected cost categories");
}

#[test]
fn test_cost_map_from_cairn_yaml() {
    let traj = parse_cairn();
    let cm = cost_map::build(&traj, None);

    assert_eq!(cm.root.node_id, "root");
    assert!(!cm.root.children.is_empty());
}
