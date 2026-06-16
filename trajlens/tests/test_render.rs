/// Integration tests dedicated to IGR → SVG rendering.
///
/// Focus on edge cases that have caused real bugs in the past:
///   - DAG cycles / back-edges that broke Sugiyama layer assignment
///   - Empty / single-node graphs
///   - Disconnected components
///   - Multi-source (N-to-1) edges
///   - Long labels and Unicode
///   - Extreme node counts (stress test)
///   - Self-loops
///   - Missing edge endpoints (dangling references)
///
/// We assert structural properties (node/edge counts, layer count, canvas
/// dimensions, presence of marker definitions) rather than byte-identical
/// SVG output — the latter is brittle to cosmetic changes.
use trajlens::compilers::{GraphCompiler, SVGCompiler};
use trajlens::models::{
    ActivityEdge, ActivityGraph, ActivityNode, Cost, GoalCategory, GoalEdge, GoalEdgeType,
    GoalNode, GoalStatus, GoalTransitionTree, GoalType, GraphEnum, InsightStatus, OpType,
    Operation, ReasoningArtifactDAG, ReasoningArtifactNode, ReasoningEdge, ReasoningEdgeType,
    ReasoningNodeType,
};

// ============ Test helpers ============

fn render(graph: &GraphEnum) -> String {
    SVGCompiler::new().compile(graph)
}

/// Width threshold to distinguish node rects from legend swatches/markers.
/// Node rects in all our renderers are ≥ ~150px wide; legend swatches are
/// typically 16-20px. We pick 100 as a safe boundary.
const NODE_WIDTH_THRESHOLD: f64 = 100.0;

/// Count distinct y-coordinates among "node-sized" rects.
/// Layout collapses (cycle bug) show up as `count_distinct_y_levels == 1`.
fn count_distinct_y_levels(svg: &str) -> usize {
    let mut ys: Vec<i64> = node_rect_positions(svg)
        .iter()
        .map(|(_, y, _, _)| y.round() as i64)
        .collect();
    ys.sort_unstable();
    ys.dedup();
    ys.len()
}

fn count_node_rects(svg: &str) -> usize {
    node_rect_positions(svg).len()
}

/// Extract (x, y, width, height) for every "node-sized" rect in the SVG.
///
/// We must distinguish node rects from:
///  - Header bars (full width, low height — exclude by min height)
///  - Legend backgrounds (have opacity attribute or fill="white")
///  - Activity graph header sub-rect (stroke="none")
///  - Cost-map background container (fill="#f5f5f5", first rect)
fn node_rect_positions(svg: &str) -> Vec<(f64, f64, f64, f64)> {
    use regex::Regex;
    let rect_re = Regex::new(r#"<rect\s+([^>]*?)\s*/?>"#).unwrap();
    let attr_re = Regex::new(r#"([a-zA-Z_-]+)="([^"]*)""#).unwrap();
    let mut out = Vec::new();
    for rect in rect_re.captures_iter(svg) {
        let attrs_str = &rect[1];
        let mut x = None;
        let mut y = None;
        let mut w = None;
        let mut h = None;
        let mut stroke = String::new();
        let mut opacity = String::new();
        let mut fill = String::new();
        for cap in attr_re.captures_iter(attrs_str) {
            let val = cap[2].parse::<f64>().ok();
            match &cap[1] {
                "x" => x = val,
                "y" => y = val,
                "width" => w = val,
                "height" => h = val,
                "stroke" => stroke = cap[2].to_string(),
                "opacity" => opacity = cap[2].to_string(),
                "fill" => fill = cap[2].to_string(),
                _ => {}
            }
        }
        if let (Some(x), Some(y), Some(w), Some(h)) = (x, y, w, h) {
            // Filters in priority order:
            //  - too small to be a node body
            //  - styled as a header (stroke="none")
            //  - styled as a legend (opacity attribute set, typically 0.95)
            //  - white-filled containers (legend backgrounds)
            if w < NODE_WIDTH_THRESHOLD || h < 40.0 {
                continue;
            }
            if stroke == "none" {
                continue;
            }
            if !opacity.is_empty() {
                continue;
            }
            if fill == "white" || fill == "#fff" || fill == "#ffffff" {
                continue;
            }
            out.push((x, y, w, h));
        }
    }
    out
}

fn count_edge_lines(svg: &str) -> usize {
    svg.matches("<line").count() + svg.matches("<path").count()
}

fn canvas_dims(svg: &str) -> (u32, u32) {
    use regex::Regex;
    let re = Regex::new(r#"<svg[^>]*\bwidth="(\d+)"[^>]*\bheight="(\d+)""#).unwrap();
    re.captures(svg)
        .map(|c| (c[1].parse().unwrap_or(0), c[2].parse().unwrap_or(0)))
        .unwrap_or((0, 0))
}

fn cost(d: f64) -> Cost {
    Cost {
        input_tokens: 0,
        output_tokens: 0,
        cache_read_tokens: 0,
        cache_write_tokens: 0,
        dollar_cost: d,
    }
}

fn rnode(id: &str, content: &str, status: Option<InsightStatus>) -> ReasoningArtifactNode {
    ReasoningArtifactNode {
        node_id: id.into(),
        node_type: ReasoningNodeType::Insight,
        content: content.into(),
        source_step_id: 0,
        confidence: 0.7,
        status,
        step_range: Some((0, 1)),
    }
}

fn redge(kind: ReasoningEdgeType, sources: &[&str], target: &str) -> ReasoningEdge {
    ReasoningEdge {
        edge_type: kind,
        source_ids: sources.iter().map(|s| s.to_string()).collect(),
        target_id: target.into(),
    }
}

fn anode(id: &str, label: &str, ops: usize) -> ActivityNode {
    ActivityNode {
        node_id: id.into(),
        label: label.into(),
        goal_category: GoalCategory::Read,
        primary_object: format!("/path/{}", id),
        parent_id: None,
        call_indices: (0..ops).collect(),
        operations: (0..ops)
            .map(|i| Operation {
                op_type: OpType::Read,
                detail: format!("op{}", i),
                call_index: i,
            })
            .collect(),
        total_cost: cost(0.01),
    }
}

fn aedge(src: &str, tgt: &str) -> ActivityEdge {
    ActivityEdge {
        edge_type: "next".into(),
        source_id: src.into(),
        source_operation_index: 0,
        target_id: tgt.into(),
        target_operation_index: 0,
    }
}

fn gnode(id: &str, label: &str, level: usize) -> GoalNode {
    GoalNode {
        node_id: id.into(),
        label: label.into(),
        goal_type: GoalType::Explore,
        status: GoalStatus::Done,
        result: String::new(),
        details: String::new(),
        level,
        step_range: (0, 5),
        cost: cost(0.0),
        reasoning_artifacts: Vec::new(),
    }
}

fn gedge(kind: GoalEdgeType, src: &str, tgt: &str) -> GoalEdge {
    GoalEdge {
        edge_type: kind,
        source_id: src.into(),
        target_id: tgt.into(),
        label: String::new(),
    }
}

// ============ Reasoning DAG edge cases ============

/// Regression: in-degree-0 seed bug. When every node has at least one
/// incoming edge (back-edge from a verified ground-truth node refuting
/// earlier hypotheses), Kahn's algorithm previously stalled and all nodes
/// collapsed into layer 0 (one row).
///
/// This is the actual r0..r6 / contradicts pattern from the pocgen rce log.
#[test]
fn reasoning_dag_with_back_edges_does_not_collapse() {
    let nodes = vec![
        rnode("r0", "early hypothesis A", Some(InsightStatus::SelfFalsed)),
        rnode("r1", "early hypothesis B", Some(InsightStatus::SelfFalsed)),
        rnode("r2", "early hypothesis C", Some(InsightStatus::SelfFalsed)),
        rnode("r3", "early hypothesis D", Some(InsightStatus::SelfFalsed)),
        rnode(
            "r4",
            "synthesized conclusion",
            Some(InsightStatus::Verified),
        ),
        ReasoningArtifactNode {
            node_id: "r5".into(),
            node_type: ReasoningNodeType::GroundTruth,
            content: "exploit ran, no crash".into(),
            source_step_id: 100,
            confidence: 1.0,
            status: Some(InsightStatus::Verified),
            step_range: Some((100, 100)),
        },
        rnode("r6", "final summary", Some(InsightStatus::Verified)),
    ];
    let edges = vec![
        redge(ReasoningEdgeType::Infers, &["r0"], "r1"),
        redge(ReasoningEdgeType::Supersedes, &["r1"], "r2"),
        redge(ReasoningEdgeType::Supersedes, &["r2"], "r3"),
        redge(ReasoningEdgeType::Infers, &["r1", "r2", "r3"], "r4"),
        redge(ReasoningEdgeType::Infers, &["r4"], "r5"),
        redge(ReasoningEdgeType::Infers, &["r5"], "r6"),
        // Back-edges: ground truth contradicts every early hypothesis.
        // These are what break naive Kahn's algorithm.
        redge(ReasoningEdgeType::Contradicts, &["r5"], "r0"),
        redge(ReasoningEdgeType::Contradicts, &["r5"], "r1"),
        redge(ReasoningEdgeType::Contradicts, &["r5"], "r2"),
        redge(ReasoningEdgeType::Contradicts, &["r5"], "r3"),
    ];
    let svg = render(&GraphEnum::ReasoningDAG(ReasoningArtifactDAG {
        nodes,
        edges,
    }));

    let levels = count_distinct_y_levels(&svg);
    assert!(
        levels >= 3,
        "DAG with back-edges should still produce a hierarchy (>=3 layers); got {} (Sugiyama collapse?)",
        levels
    );
    assert_eq!(count_node_rects(&svg), 7, "all 7 nodes should be rendered");
}

/// Self-loop: a node whose insight infers itself. Must not panic and must
/// still produce a valid SVG.
#[test]
fn reasoning_dag_with_self_loop() {
    let nodes = vec![
        rnode("a", "self-referential", None),
        rnode("b", "downstream", None),
    ];
    let edges = vec![
        redge(ReasoningEdgeType::Infers, &["a"], "a"),
        redge(ReasoningEdgeType::Infers, &["a"], "b"),
    ];
    let svg = render(&GraphEnum::ReasoningDAG(ReasoningArtifactDAG {
        nodes,
        edges,
    }));
    assert_eq!(count_node_rects(&svg), 2);
    let (w, h) = canvas_dims(&svg);
    assert!(w > 0 && h > 0, "canvas dims must be positive: {}x{}", w, h);
}

/// Multi-source (N-to-1) edge — needs a junction point, not N independent arrows.
#[test]
fn reasoning_dag_multi_source_edge_renders_junction() {
    let nodes = vec![
        rnode("s1", "source 1", None),
        rnode("s2", "source 2", None),
        rnode("s3", "source 3", None),
        rnode("t", "target inferred from all 3", None),
    ];
    let edges = vec![redge(ReasoningEdgeType::Infers, &["s1", "s2", "s3"], "t")];
    let svg = render(&GraphEnum::ReasoningDAG(ReasoningArtifactDAG {
        nodes,
        edges,
    }));
    assert_eq!(count_node_rects(&svg), 4);
    // A junction means at least 3 line segments (one per source) for the one edge.
    let lines = count_edge_lines(&svg);
    assert!(
        lines >= 3,
        "expected >=3 edge segments for junction; got {}",
        lines
    );
}

/// Dangling source: an edge references a node that doesn't exist.
/// Must not panic; the edge should be silently dropped.
#[test]
fn reasoning_dag_with_dangling_edge_does_not_panic() {
    let nodes = vec![rnode("a", "real", None)];
    let edges = vec![redge(ReasoningEdgeType::Infers, &["nonexistent"], "a")];
    let svg = render(&GraphEnum::ReasoningDAG(ReasoningArtifactDAG {
        nodes,
        edges,
    }));
    assert_eq!(count_node_rects(&svg), 1);
}

/// Regression: legend must NOT overlap any node.
/// On tall vertical layouts (≥7 stacked nodes), the legend used to be placed at
/// `canvas_height - 310` which fell back into the node column.
/// Now the canvas is extended to reserve a footer area for the legend.
#[test]
fn reasoning_dag_legend_does_not_overlap_nodes() {
    use regex::Regex;
    // Build a 7-node vertical chain (1 column, 7 rows) — same shape as the
    // pocgen_2/rce DAG that exposed the bug.
    let nodes: Vec<ReasoningArtifactNode> = (0..7)
        .map(|i| rnode(&format!("r{}", i), &format!("step {}", i), None))
        .collect();
    let edges: Vec<ReasoningEdge> = (0..6)
        .map(|i| {
            redge(
                ReasoningEdgeType::Infers,
                &[&format!("r{}", i)],
                &format!("r{}", i + 1),
            )
        })
        .collect();

    let svg = render(&GraphEnum::ReasoningDAG(ReasoningArtifactDAG {
        nodes,
        edges,
    }));

    // Find all node rects (width=220, height=190).
    let node_re = Regex::new(r#"<rect x="(\d+)" y="(\d+)" width="220" height="190""#).unwrap();
    let nodes_bottom: i64 = node_re
        .captures_iter(&svg)
        .map(|c| c[2].parse::<i64>().unwrap_or(0) + 190)
        .max()
        .expect("at least one node rect should be present");

    // Find the legend background (width=270, height=320, fill=white).
    // x position is relative to viewBox left edge, not fixed at 10.
    let legend_re = Regex::new(r#"<rect x="[^"]*" y="([^"]*)" width="270" height="320""#).unwrap();
    let legend_top: i64 = legend_re
        .captures(&svg)
        .map(|c| c[1].parse::<f64>().unwrap_or(0.0) as i64)
        .expect("legend background rect should be present");

    assert!(
        legend_top >= nodes_bottom,
        "legend (top={}) overlaps bottom-most node (bottom={})",
        legend_top,
        nodes_bottom
    );
}

/// Trajectory outcome classifier: explicit success markers → ConcludedSuccess.
#[test]
fn classify_trajectory_outcome_detects_success() {
    use chrono::NaiveDateTime;
    use trajlens::graphs::reasoning_dag::{classify_trajectory_outcome, TrajectoryOutcomeClass};
    use trajlens::models::{Cost, Item, ItemCategory, Step, Trajectory};

    let traj = Trajectory {
        label: String::new(),
        steps: vec![Step {
            step_id: 0,
            items: vec![Item {
                category: ItemCategory::Event,
                sub_category: Some("submit".into()),
                args: Default::default(),
                content: "Server confirmed exploit succeeded with proof X".into(),
                cost: Cost::default(),
            }],
            timestamp_start: NaiveDateTime::MIN,
            timestamp_end: NaiveDateTime::MIN,
            raw_line_range: (0, 0),
        }],
        total_cost: Cost::default(),
        outcome: "PARSED".into(),
    };
    let class = classify_trajectory_outcome(&traj, "verified by server");
    assert_eq!(class, TrajectoryOutcomeClass::ConcludedSuccess);
}

/// Trajectory outcome classifier: trajectory just ends → Unknown.
#[test]
fn classify_trajectory_outcome_detects_unknown_when_no_conclusion() {
    use chrono::NaiveDateTime;
    use trajlens::graphs::reasoning_dag::{classify_trajectory_outcome, TrajectoryOutcomeClass};
    use trajlens::models::{Cost, Item, ItemCategory, Step, Trajectory};

    let traj = Trajectory {
        label: String::new(),
        steps: vec![Step {
            step_id: 0,
            items: vec![Item {
                category: ItemCategory::Action,
                sub_category: Some("read".into()),
                args: Default::default(),
                content: "cat survey.py — file content read successfully".into(),
                cost: Cost::default(),
            }],
            timestamp_start: NaiveDateTime::MIN,
            timestamp_end: NaiveDateTime::MIN,
            raw_line_range: (0, 0),
        }],
        total_cost: Cost::default(),
        outcome: "PARSED".into(),
    };
    let class = classify_trajectory_outcome(&traj, "");
    assert_eq!(
        class,
        TrajectoryOutcomeClass::Unknown,
        "trajectory with only file-read content should be classified Unknown — \
         no explicit conclusion event present"
    );
}

/// Trajectory outcome classifier: failure markers → ConcludedFailure.
#[test]
fn classify_trajectory_outcome_detects_failure() {
    use chrono::NaiveDateTime;
    use trajlens::graphs::reasoning_dag::{classify_trajectory_outcome, TrajectoryOutcomeClass};
    use trajlens::models::{Cost, Item, ItemCategory, Step, Trajectory};

    let traj = Trajectory {
        label: String::new(),
        steps: vec![Step {
            step_id: 0,
            items: vec![Item {
                category: ItemCategory::Event,
                sub_category: None,
                args: Default::default(),
                content: "All attempts failed; verifier rejected the submission".into(),
                cost: Cost::default(),
            }],
            timestamp_start: NaiveDateTime::MIN,
            timestamp_end: NaiveDateTime::MIN,
            raw_line_range: (0, 0),
        }],
        total_cost: Cost::default(),
        outcome: "PARSED".into(),
    };
    let class = classify_trajectory_outcome(&traj, "verify failed");
    assert_eq!(class, TrajectoryOutcomeClass::ConcludedFailure);
}

/// Empty DAG: zero nodes. Must produce a valid SVG (with legend), not panic.
#[test]
fn reasoning_dag_empty() {
    let svg = render(&GraphEnum::ReasoningDAG(ReasoningArtifactDAG {
        nodes: Vec::new(),
        edges: Vec::new(),
    }));
    assert!(svg.starts_with("<?xml"));
    assert!(svg.contains("</svg>"));
}

/// Long label with Unicode — must not panic on multi-byte char boundaries
/// (this caused a real panic with em-dash on byte slicing).
#[test]
fn reasoning_dag_long_unicode_label() {
    let long_label: String = "测试 — em-dash and unicode characters 中文 ".repeat(20);
    let nodes = vec![rnode("a", &long_label, None)];
    let svg = render(&GraphEnum::ReasoningDAG(ReasoningArtifactDAG {
        nodes,
        edges: Vec::new(),
    }));
    assert_eq!(count_node_rects(&svg), 1);
}

// ============ Activity Graph edge cases ============

/// Linear chain of activity nodes with edges between specific operations.
#[test]
fn activity_graph_linear_chain() {
    let nodes = vec![
        anode("n0", "first", 3),
        anode("n1", "second", 2),
        anode("n2", "third", 1),
    ];
    let edges = vec![aedge("n0", "n1"), aedge("n1", "n2")];
    let svg = render(&GraphEnum::ActivityGraph(ActivityGraph { nodes, edges }));
    assert_eq!(count_node_rects(&svg), 3);
    let levels = count_distinct_y_levels(&svg);
    assert!(
        levels >= 2,
        "linear chain must have >=2 layers; got {}",
        levels
    );
}

/// Activity graph with operations exceeding the truncation cap (10 + "...and N more").
#[test]
fn activity_graph_node_with_many_operations_truncates() {
    let nodes = vec![anode("big", "lots-of-ops", 50)];
    let svg = render(&GraphEnum::ActivityGraph(ActivityGraph {
        nodes,
        edges: Vec::new(),
    }));
    assert!(
        svg.contains("more"),
        "expected '...and N more' truncation marker"
    );
    let (_, h) = canvas_dims(&svg);
    assert!(
        h < 1000,
        "node height should be capped despite 50 ops; got {}",
        h
    );
}

/// Edges referencing non-existent nodes should not panic.
#[test]
fn activity_graph_dangling_edge() {
    let nodes = vec![anode("a", "only-real-node", 1)];
    let edges = vec![aedge("a", "ghost"), aedge("phantom", "a")];
    let svg = render(&GraphEnum::ActivityGraph(ActivityGraph { nodes, edges }));
    assert_eq!(count_node_rects(&svg), 1);
}

/// Single isolated node with no edges.
#[test]
fn activity_graph_single_node_no_edges() {
    let nodes = vec![anode("solo", "isolated", 1)];
    let svg = render(&GraphEnum::ActivityGraph(ActivityGraph {
        nodes,
        edges: Vec::new(),
    }));
    assert_eq!(count_node_rects(&svg), 1);
    let (w, h) = canvas_dims(&svg);
    assert!(w > 0 && h > 0);
}

/// Empty activity graph: no nodes. Should render without panicking and
/// produce a valid SVG (we expect the renderer not to crash on zero-size canvases).
#[test]
fn activity_graph_empty() {
    let svg = render(&GraphEnum::ActivityGraph(ActivityGraph {
        nodes: Vec::new(),
        edges: Vec::new(),
    }));
    assert!(svg.starts_with("<?xml"));
    let (w, h) = canvas_dims(&svg);
    assert!(
        w > 0 && h > 0,
        "even empty graph needs positive canvas; got {}x{}",
        w,
        h
    );
}

/// All activity-graph categories rendered together — make sure each gets
/// its color and no fill is missing.
#[test]
fn activity_graph_all_categories() {
    let cats = [
        GoalCategory::Read,
        GoalCategory::Write,
        GoalCategory::Edit,
        GoalCategory::List,
        GoalCategory::Run,
        GoalCategory::Other,
    ];
    let nodes: Vec<ActivityNode> = cats
        .iter()
        .enumerate()
        .map(|(i, c)| ActivityNode {
            node_id: format!("n{}", i),
            label: format!("{:?}", c),
            goal_category: c.clone(),
            primary_object: format!("/x/{}", i),
            parent_id: None,
            call_indices: vec![i],
            operations: vec![Operation {
                op_type: OpType::Read,
                detail: "x".into(),
                call_index: i,
            }],
            total_cost: cost(0.0),
        })
        .collect();
    let svg = render(&GraphEnum::ActivityGraph(ActivityGraph {
        nodes,
        edges: Vec::new(),
    }));
    assert_eq!(count_node_rects(&svg), 6);
}

// ============ Goal Transition Tree edge cases ============

/// Single-root-only tree (no children).
#[test]
fn goal_tree_root_only() {
    let nodes = vec![gnode("1", "root only", 0)];
    let svg = render(&GraphEnum::GoalTree(GoalTransitionTree {
        nodes,
        edges: Vec::new(),
        root_id: "1".into(),
    }));
    assert_eq!(count_node_rects(&svg), 1);
}

/// Goal tree with sub-goals and backtrack edges (the typical hierarchy).
#[test]
fn goal_tree_with_subgoals_and_backtrack() {
    let nodes = vec![
        gnode("1", "main", 0),
        gnode("1.1", "sub-a", 1),
        gnode("1.2", "sub-b", 1),
        gnode("1.1.1", "deeper", 2),
    ];
    let edges = vec![
        gedge(GoalEdgeType::Sub, "1", "1.1"),
        gedge(GoalEdgeType::Next, "1.1", "1.2"),
        gedge(GoalEdgeType::Sub, "1.1", "1.1.1"),
        gedge(GoalEdgeType::Backtrack, "1.2", "1"),
    ];
    let svg = render(&GraphEnum::GoalTree(GoalTransitionTree {
        nodes,
        edges,
        root_id: "1".into(),
    }));
    assert_eq!(count_node_rects(&svg), 4);
    let levels = count_distinct_y_levels(&svg);
    assert!(
        levels >= 3,
        "3-level tree should produce >=3 layers; got {}",
        levels
    );
}

/// Wide tree: one root, 30 children. Tests the "extreme aspect ratio" case
/// that surfaced as a render warning on real pocgen logs.
#[test]
fn goal_tree_wide_fanout_keeps_canvas_bounded() {
    let mut nodes = vec![gnode("0", "root", 0)];
    let mut edges = Vec::new();
    for i in 1..=30 {
        nodes.push(gnode(&format!("{}", i), &format!("child {}", i), 1));
        edges.push(gedge(GoalEdgeType::Sub, "0", &format!("{}", i)));
    }
    let svg = render(&GraphEnum::GoalTree(GoalTransitionTree {
        nodes,
        edges,
        root_id: "0".into(),
    }));
    assert_eq!(count_node_rects(&svg), 31);
    let (w, h) = canvas_dims(&svg);
    // Canvas can be wide but must not be insane (>50000px would mean overflow).
    assert!(w > 0 && w < 50_000, "canvas width sanity: {}", w);
    assert!(h > 0 && h < 50_000, "canvas height sanity: {}", h);
}

// ============ Cost Map edge cases ============

/// Zero-cost root: every node has 0 cost. Treemap algorithm should not
/// divide by zero or produce NaN positions.
#[test]
fn cost_map_zero_cost() {
    use trajlens::models::{CostMap, CostMapNode};
    let cm = CostMap {
        root: CostMapNode {
            node_id: "root".into(),
            label: "root".into(),
            cost: cost(0.0),
            children: vec![CostMapNode {
                node_id: "a".into(),
                label: "child".into(),
                cost: cost(0.0),
                children: Vec::new(),
                category: Some("read".into()),
                step_range: None,
            }],
            category: None,
            step_range: None,
        },
    };
    let svg = render(&GraphEnum::CostMap(cm));
    assert!(svg.starts_with("<?xml"));
    let (w, h) = canvas_dims(&svg);
    assert!(w > 0 && h > 0);
}

/// Deeply nested cost map (5 levels). Recursion must not blow the stack
/// or render off-screen.
#[test]
fn cost_map_deep_nesting() {
    use trajlens::models::{CostMap, CostMapNode};
    fn build(depth: usize, id_prefix: &str) -> CostMapNode {
        if depth == 0 {
            return CostMapNode {
                node_id: format!("{}-leaf", id_prefix),
                label: "leaf".into(),
                cost: cost(0.5),
                children: Vec::new(),
                category: Some("run".into()),
                step_range: None,
            };
        }
        CostMapNode {
            node_id: format!("{}-l{}", id_prefix, depth),
            label: format!("level-{}", depth),
            cost: cost(depth as f64),
            children: vec![
                build(depth - 1, &format!("{}-a", id_prefix)),
                build(depth - 1, &format!("{}-b", id_prefix)),
            ],
            category: None,
            step_range: None,
        }
    }
    let cm = CostMap {
        root: build(5, "r"),
    };
    let svg = render(&GraphEnum::CostMap(cm));
    assert!(svg.starts_with("<?xml"));
    let (w, h) = canvas_dims(&svg);
    assert!(w > 0 && h > 0);
}

/// Single-leaf cost map: only the root, no children.
#[test]
fn cost_map_root_only() {
    use trajlens::models::{CostMap, CostMapNode};
    let cm = CostMap {
        root: CostMapNode {
            node_id: "root".into(),
            label: "only".into(),
            cost: cost(1.0),
            children: Vec::new(),
            category: Some("read".into()),
            step_range: None,
        },
    };
    let svg = render(&GraphEnum::CostMap(cm));
    assert!(svg.contains("</svg>"));
}

// ============ Cross-graph properties ============

/// Every renderer must produce a self-contained, well-formed SVG document.
/// (Cheap structural check, not a strict XML parse.)
#[test]
fn all_graph_types_produce_valid_svg_envelope() {
    let cases: Vec<GraphEnum> = vec![
        GraphEnum::ReasoningDAG(ReasoningArtifactDAG {
            nodes: vec![rnode("a", "x", None)],
            edges: Vec::new(),
        }),
        GraphEnum::ActivityGraph(ActivityGraph {
            nodes: vec![anode("a", "x", 1)],
            edges: Vec::new(),
        }),
        GraphEnum::GoalTree(GoalTransitionTree {
            nodes: vec![gnode("1", "x", 0)],
            edges: Vec::new(),
            root_id: "1".into(),
        }),
        GraphEnum::CostMap({
            use trajlens::models::{CostMap, CostMapNode};
            CostMap {
                root: CostMapNode {
                    node_id: "root".into(),
                    label: "x".into(),
                    cost: cost(1.0),
                    children: Vec::new(),
                    category: Some("read".into()),
                    step_range: None,
                },
            }
        }),
    ];

    for graph in &cases {
        let svg = render(graph);
        assert!(svg.starts_with("<?xml"), "missing XML declaration");
        assert!(svg.contains("<svg"), "missing <svg>");
        assert!(
            svg.ends_with("</svg>\n") || svg.ends_with("</svg>"),
            "missing </svg>"
        );
        assert!(svg.contains("xmlns="), "missing xmlns");
    }
}

/// Text tree renderer produces readable indented output.
#[test]
fn text_tree_renders_goal_tree() {
    use trajlens::compilers::{text_tree::TextTreeCompiler, GraphCompiler};

    let tree = GoalTransitionTree {
        root_id: "ROOT".into(),
        nodes: vec![
            gnode("ROOT", "Achieve RCE on target", 0),
            gnode("1", "Reconnaissance", 1),
            gnode("1.1", "Read source files", 2),
            gnode("1.2", "Probe endpoints", 2),
            gnode("2", "Exploit attempt", 1),
            gnode("2.1", "Write exploit", 2),
        ],
        edges: vec![
            gedge(GoalEdgeType::Sub, "ROOT", "1"),
            gedge(GoalEdgeType::Next, "1", "2"),
            gedge(GoalEdgeType::Backtrack, "2", "ROOT"),
            gedge(GoalEdgeType::Sub, "1", "1.1"),
            gedge(GoalEdgeType::Next, "1.1", "1.2"),
            gedge(GoalEdgeType::Backtrack, "1.2", "1"),
            gedge(GoalEdgeType::Sub, "2", "2.1"),
            gedge(GoalEdgeType::Backtrack, "2.1", "2"),
        ],
    };

    let compiler = TextTreeCompiler::new();
    let output = compiler.compile(&GraphEnum::GoalTree(tree));
    println!("{}", output);

    assert!(output.contains("ROOT"));
    assert!(output.contains("├──") || output.contains("└──"));
    assert!(output.contains("1.1"));
    assert!(output.contains("2.1"));
}
