/// Integration tests for IGR serialization/deserialization.
/// Tests roundtrip property and deserialization of existing IGR files.
use std::fs;
use std::path::PathBuf;

use trajlens::igr;
use trajlens::models::GraphEnum;

fn output_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("output")
        .join("cc_arvo_57672")
}

#[test]
fn test_deserialize_activity_graph_igr() {
    let toml_str = fs::read_to_string(output_dir().join("activity-graph.igr.toml")).unwrap();
    let graph = igr::deserialize(&toml_str).unwrap();
    match &graph {
        GraphEnum::ActivityGraph(ag) => {
            assert!(ag.nodes.len() > 0, "Expected at least one node");
            // Check that the "workspace" node exists
            let workspace_node = ag.nodes.iter().find(|n| n.label == "workspace");
            assert!(workspace_node.is_some(), "Expected a 'workspace' node");
        }
        _ => panic!(
            "Expected ActivityGraph, got {:?}",
            std::mem::discriminant(&graph)
        ),
    }
}

#[test]
fn test_deserialize_goal_tree_igr() {
    let toml_str = fs::read_to_string(output_dir().join("goal-tree.igr.toml")).unwrap();
    let graph = igr::deserialize(&toml_str).unwrap();
    match &graph {
        GraphEnum::GoalTree(tree) => {
            assert!(tree.nodes.len() > 0);
            assert!(!tree.root_id.is_empty());
            assert!(tree.edges.len() > 0);
        }
        _ => panic!("Expected GoalTransitionTree"),
    }
}

#[test]
fn test_deserialize_reasoning_dag_igr() {
    let toml_str = fs::read_to_string(output_dir().join("reasoning-dag.igr.toml")).unwrap();
    let graph = igr::deserialize(&toml_str).unwrap();
    match &graph {
        GraphEnum::ReasoningDAG(dag) => {
            assert!(dag.nodes.len() > 0);
            assert!(dag.edges.len() > 0);
        }
        _ => panic!("Expected ReasoningArtifactDAG"),
    }
}

#[test]
fn test_deserialize_cost_map_igr() {
    let toml_str = fs::read_to_string(output_dir().join("cost-map.igr.toml")).unwrap();
    let graph = igr::deserialize(&toml_str).unwrap();
    match &graph {
        GraphEnum::CostMap(cm) => {
            assert_eq!(cm.root.node_id, "root");
            assert!(cm.root.children.len() > 0);
        }
        _ => panic!("Expected CostMap"),
    }
}

#[test]
fn test_roundtrip_goal_tree() {
    let toml_str = fs::read_to_string(output_dir().join("goal-tree.igr.toml")).unwrap();
    let graph = igr::deserialize(&toml_str).unwrap();
    let reserialized = igr::serialize(&graph).unwrap();
    let graph2 = igr::deserialize(&reserialized).unwrap();
    assert_eq!(
        graph, graph2,
        "Roundtrip failed: deserialize(serialize(graph)) != graph"
    );
}

#[test]
fn test_roundtrip_reasoning_dag() {
    let toml_str = fs::read_to_string(output_dir().join("reasoning-dag.igr.toml")).unwrap();
    let graph = igr::deserialize(&toml_str).unwrap();
    let reserialized = igr::serialize(&graph).unwrap();
    let graph2 = igr::deserialize(&reserialized).unwrap();
    assert_eq!(graph, graph2);
}

#[test]
fn test_roundtrip_activity_graph() {
    let toml_str = fs::read_to_string(output_dir().join("activity-graph.igr.toml")).unwrap();
    let graph = igr::deserialize(&toml_str).unwrap();
    let reserialized = igr::serialize(&graph).unwrap();
    let graph2 = igr::deserialize(&reserialized).unwrap();
    assert_eq!(graph, graph2);
}

#[test]
fn test_roundtrip_cost_map() {
    let toml_str = fs::read_to_string(output_dir().join("cost-map.igr.toml")).unwrap();
    let graph = igr::deserialize(&toml_str).unwrap();
    let reserialized = igr::serialize(&graph).unwrap();
    let graph2 = igr::deserialize(&reserialized).unwrap();
    assert_eq!(graph, graph2);
}
