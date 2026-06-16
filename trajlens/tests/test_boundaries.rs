//! Integration test for build_with_boundaries.
//! Run with: cargo test --features "cli,svg-rust,llm-bedrock" --test test_boundaries -- --ignored
#[cfg(feature = "llm")]
#[tokio::test]
#[ignore] // requires LLM API keys
async fn test_build_with_consensus_boundaries() {
    use trajlens::graphs::goal_tree;
    use trajlens::models::Trajectory;

    let traj_json = std::fs::read_to_string(
        "../output/variance_test/run_1/exploit_rce_a3/main/trajectory.json",
    )
    .expect("Run variance_test first");

    let trajectory: Trajectory = serde_json::from_str(&traj_json).unwrap();

    let boundaries = vec![
        2, 18, 30, 34, 40, 46, 51, 56, 62, 66, 72, 77, 84, 89, 92, 97, 100, 103, 106,
    ];

    let tree = goal_tree::build_with_boundaries(
        &trajectory,
        &boundaries,
        "bedrock/us.anthropic.claude-sonnet-4-6",
    )
    .await
    .expect("build_with_boundaries failed");

    println!("Nodes: {}", tree.nodes.len());
    for n in &tree.nodes {
        let sr = n.step_range;
        println!(
            "  {:6} [{:7}] [{:7}] steps {:3}-{:<3} | {}",
            n.node_id,
            format!("{:?}", n.goal_type).to_lowercase(),
            format!("{:?}", n.status).to_lowercase(),
            sr.0,
            sr.1,
            n.label
        );
    }

    // Write output
    let igr = trajlens::igr::serialize(&trajlens::models::GraphEnum::GoalTree(tree)).unwrap();
    std::fs::create_dir_all("/data4/ye472/TrajLens/output/variance_test").ok();
    std::fs::write(
        "/data4/ye472/TrajLens/output/variance_test/consensus_goal_tree.igr.toml",
        &igr,
    )
    .unwrap();

    assert!(
        true,
        "Test passed — check output/variance_test/consensus_goal_tree.igr.toml"
    );
}
