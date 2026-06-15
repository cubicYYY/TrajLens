/// Goal Transition Tree (G1) builder - LLM-based goal extraction.
///
/// Analyzes trajectory to extract:
/// - Hierarchical goal structure
/// - Goal transitions (next, backtrack, refinement)
/// - Achievement status
/// - Cost per goal
use crate::models::{GoalEdge, GoalNode, GoalTransitionTree, Trajectory};

#[cfg(feature = "llm")]
use crate::llm::traits::LLMResult;

/// Build a Goal Transition Tree from a trajectory using an LLM.
///
/// # Arguments
/// * `trajectory` - The parsed trajectory
/// * `model` - Model specification in "provider/model-name" format
///   - "anthropic/claude-sonnet-4-6"
///   - "bedrock/us.anthropic.claude-sonnet-4-6"
///
/// # Returns
/// Result containing the GoalTransitionTree or an error message
///
/// # Example
/// ```rust,no_run
/// use trajlens::graphs::goal_tree;
///
/// #[tokio::main]
/// async fn main() {
///     let tree = goal_tree::build_with_llm(&trajectory, "anthropic/claude-sonnet-4-6")
///         .await.unwrap();
/// }
/// ```
#[cfg(feature = "llm")]
pub async fn build_with_llm(trajectory: &Trajectory, model: &str) -> LLMResult<GoalTransitionTree> {
    build_with_llm_retries(trajectory, model, 3).await
}

/// Build goal tree with multi-turn LLM correction.
/// If the LLM produces structural anomalies, they are reported back and
/// the LLM gets a chance to fix them. Retries up to `max_retries` times.
#[cfg(feature = "llm")]
pub async fn build_with_llm_retries(
    trajectory: &Trajectory,
    model: &str,
    max_retries: usize,
) -> LLMResult<GoalTransitionTree> {
    use crate::llm::model_registry;
    let llm_client = model_registry::create_client(model).await?;

    let step_count = trajectory.steps.len();
    let total_cost = trajectory.total_cost.dollar_cost;
    let outcome = &trajectory.outcome;
    let system_prompt = r#"You are an expert at analyzing AI agent execution trajectories and extracting goal hierarchies.
Agent's "current goal" is frequently changing or backtracking; you need to find these goals and reveal how the agent's goal transits.
This can help people better understand why some steps failed or why the agent pivot to another way after something failed.
The key is to find the critical result or decision in each "current goal", which decides the agent's next step.

Your task: build a Goal Transition Tree showing what goals the agent pursued and how they relate.

# CRITICAL: This must be a TREE with depth, NOT a flat chain.

The tree MUST have at least 2 levels of hierarchy. A flat chain (ROOT → 1 → 2 → 3 → ... all via "next" edges) is WRONG.

Correct structure: ROOT has 2-4 children (phases). Each phase has 2-4 sub-children (actions).

# LOOP INVARIANT (most important rule)

Every parent node forms a CLOSED LOOP with its children:
  parent --sub--> first_child --next--> ... --next--> last_child --backtrack--> parent

This means:
- Parent has one "sub" edge per plan it launches (to the first child of that plan)
- Children within a plan are chained by "next" edges (first→second→...→last)
- The LAST child of each chain has a "backtrack" edge returning to the parent
- A node CANNOT have both "next" AND "backtrack" — it's either mid-chain or end-of-chain

Single plan (most common):
  3 --sub--> 3.1 --next--> 3.2 --next--> 3.3 --backtrack--> 3

Multiple plans (parent retries after first plan fails):
  3 --sub--> 3.1 --next--> 3.2 --backtrack--> 3   (first plan failed)
  3 --sub--> 3.3 --next--> 3.4 --backtrack--> 3   (second plan, new approach)

WRONG (node has both next AND backtrack — REJECTED):
  3.2 --next--> 3.3       ← means 3.2 is mid-chain
  3.2 --backtrack--> 3    ← means 3.2 is end-of-chain
  CONTRADICTION: a node is either middle or end, never both.

If 3.2 failed and the parent pivots, 3.2 should be the END of its chain (backtrack),
and the parent launches a NEW sub edge to 3.3 (start of next plan).

Example with full edge set:
  ROOT --sub--> 1
  1 --next--> 2
  2 --next--> 3
  3 --backtrack--> ROOT        ← closes the ROOT loop

  1 --sub--> 1.1
  1.1 --next--> 1.2
  1.2 --backtrack--> 1         ← closes node 1's loop

  3 --sub--> 3.1
  3.1 --next--> 3.2
  3.2 --backtrack--> 3         ← closes node 3's loop

Tree shape:
  ROOT
  ├── 1 (Reconnaissance)
  │   ├── 1.1 (read files)
  │   └── 1.2 (probe endpoints)
  ├── 2 (Analysis)
  └── 3 (Exploitation)
      ├── 3.1 (write exploit)
      └── 3.2 (execute & verify)

WRONG (flat chain — REJECTED):
  ROOT → 1 → 2 → 3 → 4 → 5 (all "next", no sub edges, no loops)

WRONG (missing backtrack — REJECTED):
  ROOT --sub--> 1, 1 --next--> 2, 2 --next--> 3  (no backtrack to ROOT)

# Node ID Convention

- Root: "ROOT" (level 0 — the only node at this level)
- Children of ROOT: "1", "2", "3" (level 1 — major phases, 2-4 nodes)
- Children of "2": "2.1", "2.2" (level 2 — actions within phase)
- Children of "2.1": "2.1.1", "2.1.2" (level 3 — rare, only if needed)

# Edge Types

- "sub": parent → first child (exactly ONE per parent that has children)
- "next": sibling → next sibling (chains children left-to-right)
- "backtrack": last child → parent (closes the loop; every parent gets exactly one)

The validator checks:
- Every node with children has exactly 1 outgoing "sub" edge
- Every parent receives exactly 1 incoming "backtrack" edge from its last child
- ROOT's loop is closed (last phase backtracks to ROOT)

# Sibling Differentiation

When the agent retries or refines an approach, each sibling node MUST state what CHANGED
from the previous attempt. Do NOT use vague labels like "Refine exploit" or "Try again".

BAD siblings (indistinguishable):
  3.1: "Write exploit script"
  3.2: "Refine exploit script"
  3.3: "Try exploit again"

GOOD siblings (each states the concrete difference):
  3.1: "Exploit via SQL injection on /search endpoint"
  3.2: "Exploit via file upload with path traversal"
  3.3: "Exploit via SSRF targeting internal PostgreSQL"

BAD (vague refinement):
  3.1: "Attempt authentication bypass"
  3.2: "Retry with different approach"

GOOD (states what changed):
  3.1: "Bypass auth via case-insensitive email registration"
  3.2: "Bypass auth via JWT secret brute-force (wordlist)"

The label must answer: "what is DIFFERENT about THIS attempt vs the previous one?"

# Output Format (strict JSON)

{
  "nodes": [
    {
      "node_id": "ROOT",
      "label": "Short goal statement, max 15 words",
      "goal_type": "explore|think|act",
      "status": "done|failed|abandoned|partial",
      "result": "Brief outcome (max 20 words)",
      "details": "Key evidence from the trajectory: exact commands, error messages, server responses. 2-4 sentences with specifics.",
      "step_range": [start_step, end_step]
    }
  ],
  "edges": [
    {
      "source_id": "ROOT",
      "target_id": "1",
      "edge_type": "sub",
      "label": ""
    },
    {
      "source_id": "3",
      "target_id": "2",
      "edge_type": "backtrack",
      "label": "SQL injection blocked by ORM parameterization; no raw queries in codebase"
    }
  ]
}

NOTE on edge labels:
- "sub" and "next" edges: label = "" (empty)
- "backtrack" from a FAILED node: label MUST be a non-empty sentence explaining the failure.
  The LLM validator REJECTS empty labels on failed backtrack edges. You MUST fill them.

# Node Labels & Examples

Each node: label (max 15 words, the GOAL) + goal_type + result (what happened).

## EXPLORE examples (gathering information):

  { "node_id": "1", "label": "Map target application structure and endpoints",
    "goal_type": "explore", "status": "done",
    "result": "Flask app with 6 REST endpoints, nginx proxy, PostgreSQL DB" }

  { "node_id": "2.1", "label": "Read authentication source code",
    "goal_type": "explore", "status": "done",
    "result": "JWT-based auth using PyJWT; tokens expire after 1h; secret from env var" }

  { "node_id": "3.1", "label": "Probe API endpoints for access control gaps",
    "goal_type": "explore", "status": "done",
    "result": "GET /api/users returns 200 without auth; PUT requires admin JWT" }

## THINK examples (reasoning and deciding):

  { "node_id": "2", "label": "Identify most promising attack vector",
    "goal_type": "think", "status": "done",
    "result": "Best vector: unauthenticated GET leaks user IDs; chain with IDOR on PUT" }

  { "node_id": "3", "label": "Decide between SQL injection vs API abuse",
    "goal_type": "think", "status": "done",
    "result": "SQLAlchemy ORM blocks injection; pivot to mass-assignment via REST" }

  { "node_id": "4", "label": "Plan next approach after first exploit failed",
    "goal_type": "think", "status": "done",
    "result": "JWT forgery seems infeasible; will try session fixation via cookie handling" }

## ACT examples (executing actions):

  { "node_id": "5", "label": "Craft SQL injection payload for search endpoint",
    "goal_type": "act", "status": "failed",
    "result": "Server returned 500; PostgreSQL rejects is_template=0 (needs FALSE)",
    "details": "Ran: curl -X GET '.../surveys?search=x' UNION SELECT...' Response: HTTP 500. Source at routes.py:45 shows raw SQL with f-string interpolation but PostgreSQL requires boolean literals (TRUE/FALSE not 0/1). All UNION variants produce same 500." }

  { "node_id": "4.2", "label": "Write and run authentication bypass script",
    "goal_type": "act", "status": "done",
    "result": "Got valid admin token via case-insensitive email duplicate registration",
    "details": "Discovered /api/auth/me leaks hashed_password field. Registered ADMIN@target.com (uppercase) which created new user but get_or_create matched existing admin. Login as new user returned admin's JWT token." }

  { "node_id": "6", "label": "Submit proof of exploitation to validation service",
    "goal_type": "act", "status": "failed",
    "result": "Validator rejected: extracted value doesn't match expected format",
    "details": "Ran: python3 submit_proof.py data_exfiltration '{\"data\": \"...\"}'. Validator response: 'Expected format: raw string value, not JSON object'. The extracted DB value contains quotes that break the submission format." }

# Details Field

The "details" field captures the EVIDENCE that supports the result. It's not shown in the
graph node (too long) but available in a click-to-expand panel. It should contain:

For FAILED nodes (most important — explains WHY something failed):
  - The exact command/request that was tried
  - The exact error message or unexpected response
  - What specific mechanism blocked the approach (code line, config, server behavior)

For DONE nodes:
  - The key command that worked
  - The specific output/response that confirmed success

CRITICAL: Do not write vague details. Every detail MUST reference something SPECIFIC
from the trajectory: a URL, a function name, an error code, a file path, a response body.

BAD details: "Tried multiple approaches but none worked"
GOOD details: "Ran exploit.py targeting /api/upload with Content-Type: multipart/form-data. Server returned 403 'Forbidden: upload requires admin role'. Checked middleware at auth.py:23 — @require_role('admin') decorator blocks all non-admin uploads."

# Edge Labels

- "sub" and "next" edges: label = "" (empty)
- "backtrack" from DONE nodes: label = "" (empty)
- "backtrack" from FAILED nodes: label MUST state the CONCRETE ROOT CAUSE.
  Not "it failed" or "approach didn't work" — the SPECIFIC technical blocker.

  BAD:  "Exploit attempt failed"
  BAD:  "Could not bypass authentication"
  BAD:  "Approach unsuccessful"
  GOOD: "ORM parameterizes all queries; no raw SQL paths exist for injection"
  GOOD: "secure_filename() strips '../'; uploaded file always lands in /uploads/"
  GOOD: "PostgreSQL port 5432 not exposed; only reachable from container network"
  GOOD: "JWT secret is 32-byte random; brute-force infeasible within time budget"

  The root cause is a TECHNICAL FACT observed from the codebase or server response.
  It answers: "what specific mechanism prevented this from working?"

# Rules

- Root node "ROOT" has exactly one outgoing "sub" edge to its first child
- Every non-root leaf node (no children) that is last in its plan must have a "backtrack" edge to its parent
- Siblings are connected by "next" edges in order: 1 → 2 → 3
- Parent connects to first child via "sub": ROOT → 1, or 2 → 2.1
- A child's step_range MUST be within its parent's step_range
- Node IDs MUST follow the convention above (no "g0", "g1" etc.)
- At most 1 "next" edge per node
- status: "done" = fully achieved, "failed" = attempted but not achieved, "partial" = some progress but not complete, "abandoned" = gave up
- goal_type (3 categories):
    "explore" = gather information (read, search, probe, list, enumerate)
    "think"   = reason and decide (analyze findings, form plan, choose approach)
    "act"     = execute (write code, run command, submit, test, modify state)
- result: SHORT outcome (max 20 words). Must state CONCRETE facts, not vague summaries.
    For explore: what SPECIFIC thing was found. "Flask app uses SQLAlchemy ORM; no raw SQL queries exist"
    For think:   what SPECIFIC decision was made. "Will try mass-assignment via PATCH /api/players/{id}"
    For act (done): what SPECIFIC result occurred. "Got valid session token for user admin@target.com"
    For act (failed): the CONCRETE root cause of failure — not just "it failed" but WHY and WHAT.
      BAD:  "Exploit failed"
      BAD:  "Authentication bypass not achieved"
      BAD:  "Script returned error"
      BAD: "Grep finds candidate dangerous calls in source"
      GOOD: "Server returned 403: endpoint requires valid JWT with admin role"
      GOOD: "SQLAlchemy parameterizes all queries; UNION injection impossible"
      GOOD: "secure_filename() strips path separators; traversal blocked"
      GOOD: "Grep finds candidate dangerous calls to dangerousFunc() in xxx.py"
    The root cause must be a TECHNICAL FACT the agent observed, not a restatement of the goal.
- CRITICAL: If the trajectory Outcome is "FAILED", then the ROOT node MUST have status "failed"
- CRITICAL: backtrack edges from failed nodes MUST have a non-empty label with the CONCRETE root cause
"#;

    let initial_message = format!(
        r#"Analyze this agent trajectory and extract the goal tree:

Trajectory Summary:
- Total steps: {}
- Outcome: {}
- Total cost: ${:.4}

Sample Steps (key moments):
{}

Extract all goals, sub-goals, and transitions. Return ONLY valid JSON, no additional text."#,
        step_count,
        outcome,
        total_cost,
        sample_trajectory_steps(trajectory, 30)
    );

    // [LLM_CALL: cached] system_prompt is fixed; user_message includes trajectory + corrections
    let mut user_message = initial_message.clone();
    #[allow(unused_assignments)]
    let mut last_response = String::new();

    for attempt in 0..=max_retries {
        let response = llm_client
            .as_ref()
            .complete(system_prompt, &user_message)
            .await?;
        last_response = response.clone();

        let mut tree = match parse_goal_tree_response(&response) {
            Ok(t) => t,
            Err(e) => {
                if attempt < max_retries {
                    eprintln!("Attempt {}: parse error, retrying...", attempt + 1);
                    user_message = format!(
                        "{}\n\n---\n\n\
                         Your previous response could not be parsed: {}\n\n\
                         Please output ONLY valid JSON matching the format above.",
                        initial_message, e
                    );
                    continue;
                }
                return Err(e);
            }
        };

        add_missing_backtrack_edges(&mut tree);
        propagate_status(&mut tree);
        let anomalies = collect_anomalies(&tree);

        if anomalies.is_empty() {
            eprintln!("Goal tree built successfully (attempt {})", attempt + 1);
            return Ok(tree);
        }

        if attempt < max_retries {
            eprintln!(
                "Attempt {}: {} anomalies found, asking LLM to fix...",
                attempt + 1,
                anomalies.len()
            );
            user_message = format!(
                "{}\n\n---\n\n\
                 Your previous output had these structural problems:\n{}\n\n\
                 Your previous (broken) JSON was:\n```json\n{}\n```\n\n\
                 Fix the anomalies above and return the corrected FULL JSON. No explanation, just JSON.",
                initial_message,
                anomalies.iter().map(|a| format!("- {}", a)).collect::<Vec<_>>().join("\n"),
                last_response.chars().take(3000).collect::<String>()
            );
        } else {
            for a in &anomalies {
                eprintln!("ANOMALY: {}", a);
            }
            // Fill empty backtrack labels from the source node's own label as fallback
            fill_missing_backtrack_labels(&mut tree);
            return Ok(tree);
        }
    }

    unreachable!()
}

/// Collect structural anomalies without modifying the tree.
/// Returns a list of human-readable anomaly descriptions.
fn collect_anomalies(tree: &GoalTransitionTree) -> Vec<String> {
    use std::collections::HashMap;

    let mut anomalies = Vec::new();

    let mut outgoing_counts: HashMap<&str, usize> = HashMap::new();
    for edge in &tree.edges {
        *outgoing_counts.entry(&edge.source_id).or_insert(0) += 1;
    }

    // Orphan nodes
    let mut reachable = std::collections::HashSet::new();
    let mut queue = vec![tree.root_id.as_str()];
    while let Some(nid) = queue.pop() {
        if !reachable.insert(nid) {
            continue;
        }
        for edge in &tree.edges {
            if edge.source_id == nid
                && (edge.edge_type == crate::models::GoalEdgeType::Sub
                    || edge.edge_type == crate::models::GoalEdgeType::Next)
            {
                queue.push(&edge.target_id);
            }
        }
    }
    for node in &tree.nodes {
        if !reachable.contains(node.node_id.as_str()) {
            anomalies.push(format!(
                "Node '{}' is orphaned (unreachable from root)",
                node.node_id
            ));
        }
    }

    // Missing outgoing edges
    for node in &tree.nodes {
        let count = outgoing_counts
            .get(node.node_id.as_str())
            .copied()
            .unwrap_or(0);
        if node.node_id != tree.root_id && count == 0 {
            anomalies.push(format!("Node '{}' has no outgoing edge", node.node_id));
        }
    }

    // Multiple Next edges
    for node in &tree.nodes {
        let next_count = tree
            .edges
            .iter()
            .filter(|e| {
                e.source_id == node.node_id && e.edge_type == crate::models::GoalEdgeType::Next
            })
            .count();
        if next_count > 1 {
            anomalies.push(format!(
                "Node '{}' has {} Next edges (max 1)",
                node.node_id, next_count
            ));
        }
    }

    // Opaque IDs: valid IDs are "ROOT" or digits/dots (e.g., "1", "2.3", "2.1.4")
    for node in &tree.nodes {
        let id = &node.node_id;
        let is_valid = id == "ROOT" || id.chars().all(|c| c.is_ascii_digit() || c == '.');
        if !is_valid {
            anomalies.push(format!(
                "Node '{}' uses invalid ID (must be ROOT or N.N.N format)",
                id
            ));
        }
    }

    // Backtrack edges from failed nodes must have a label explaining why
    {
        let failed_ids: std::collections::HashSet<&str> = tree
            .nodes
            .iter()
            .filter(|n| {
                n.status == crate::models::GoalStatus::Failed
                    || n.status == crate::models::GoalStatus::Abandoned
            })
            .map(|n| n.node_id.as_str())
            .collect();

        for edge in &tree.edges {
            if edge.edge_type == crate::models::GoalEdgeType::Backtrack
                && failed_ids.contains(edge.source_id.as_str())
                && edge.label.trim().is_empty()
            {
                anomalies.push(format!(
                    "Backtrack edge from failed node '{}' has no label — must explain WHY it failed",
                    edge.source_id
                ));
            }
        }
    }

    // Tree shape checker: enforce hierarchical structure and loop invariant.
    {
        use crate::models::GoalEdgeType;
        let sub_edges: Vec<_> = tree.edges.iter().filter(|e| e.edge_type == GoalEdgeType::Sub).collect();
        let next_edges: Vec<_> = tree.edges.iter().filter(|e| e.edge_type == GoalEdgeType::Next).collect();
        let bt_edges: Vec<_> = tree.edges.iter().filter(|e| e.edge_type == GoalEdgeType::Backtrack).collect();
        let root_id = &tree.root_id;

        // Rule 1: ROOT is level 0 — no sub/next edge points TO ROOT.
        let root_is_child = tree.edges.iter().any(|e| {
            e.target_id == *root_id && (e.edge_type == GoalEdgeType::Sub || e.edge_type == GoalEdgeType::Next)
        });
        if root_is_child {
            anomalies.push("SHAPE-1: ROOT must be level 0. No sub/next edge should target ROOT.".into());
        }

        // Rule 2: ROOT must have children (at least 2 phases).
        let root_sub: Vec<_> = sub_edges.iter().filter(|e| e.source_id == *root_id).collect();
        let mut root_child_count = 0;
        if let Some(first) = root_sub.first() {
            root_child_count = 1;
            let mut current = first.target_id.as_str();
            while let Some(next) = next_edges.iter().find(|e| e.source_id == current) {
                root_child_count += 1;
                current = &next.target_id;
            }
        }
        if root_child_count < 2 && tree.nodes.len() >= 5 {
            anomalies.push(format!(
                "SHAPE-2: ROOT has only {} child(ren). Need 2-4 phases. \
                 Structure: ROOT --sub--> 1 --next--> 2 --next--> 3 --backtrack--> ROOT.",
                root_child_count
            ));
        }

        // Rule 3: Loop invariant — every parent with children must have:
        //   exactly 1 incoming backtrack from its last child.
        // Find all parents (nodes that have an outgoing sub edge).
        let parents: Vec<&str> = sub_edges.iter().map(|e| e.source_id.as_str()).collect();
        for parent_id in &parents {
            // Find children: first child via sub, rest via next chain
            let first_child = sub_edges.iter().find(|e| e.source_id == *parent_id);
            if first_child.is_none() { continue; }

            let mut last_child = first_child.unwrap().target_id.as_str();
            while let Some(next) = next_edges.iter().find(|e| e.source_id == last_child) {
                last_child = &next.target_id;
            }

            // Check: last_child must have backtrack → parent
            let has_backtrack = bt_edges.iter().any(|e| e.source_id == last_child && e.target_id == *parent_id);
            if !has_backtrack {
                anomalies.push(format!(
                    "SHAPE-3: Loop not closed for '{}'. Last child '{}' must have backtrack edge → '{}'. \
                     Add: {{\"source_id\": \"{}\", \"target_id\": \"{}\", \"edge_type\": \"backtrack\", \"label\": \"\"}}",
                    parent_id, last_child, parent_id, last_child, parent_id
                ));
            }
        }

        // Also check ROOT's loop
        if root_child_count >= 2 {
            let root_has_backtrack = bt_edges.iter().any(|e| e.target_id == *root_id);
            if !root_has_backtrack {
                anomalies.push(
                    "SHAPE-3: ROOT loop not closed. Last phase must have backtrack → ROOT.".into()
                );
            }
        }

        // Rule 3b: Each sub-plan must be a closed chain ending in backtrack.
        // A node with a backtrack edge must NOT also have a "next" edge (it's the end of a chain).
        // A node with a "next" edge must NOT also backtrack (it's in the middle).
        for bt in &bt_edges {
            let source = bt.source_id.as_str();
            let also_has_next = next_edges.iter().any(|e| e.source_id == source);
            if also_has_next {
                let next_target = next_edges.iter().find(|e| e.source_id == source).unwrap();
                anomalies.push(format!(
                    "SHAPE-3b: Node '{}' has BOTH a backtrack (to '{}') AND a next (to '{}'). \
                     A node is either the end of a chain (backtrack only) or in the middle (next only), never both. \
                     If '{}' failed and the parent started a new plan, use a SEPARATE sub edge from the parent.",
                    source, bt.target_id, next_target.target_id, source
                ));
            }
        }

        // Rule 4: Depth — at least one phase must have sub-children.
        let phase_has_sub = parents.iter().any(|p| *p != root_id.as_str());
        if !phase_has_sub && tree.nodes.len() >= 5 {
            anomalies.push(
                "SHAPE-4: No depth — no phase has sub-children. \
                 At least one phase must have sub edges to actions (e.g., 1 --sub--> 1.1)."
                    .into(),
            );
        }

        // Rule 5: No sibling chain longer than 5 (too flat within a level).
        for sub in &sub_edges {
            let mut length = 1;
            let mut current = sub.target_id.as_str();
            while let Some(next) = next_edges.iter().find(|e| e.source_id == current) {
                length += 1;
                current = &next.target_id;
                if length > 6 { break; }
            }
            if length > 5 {
                anomalies.push(format!(
                    "SHAPE-5: Chain under '{}' has {} siblings (max 5). Group into sub-phases.",
                    sub.source_id, length
                ));
                break;
            }
        }
    }

    // Step range containment: derive parent from ID.
    // "2.1" → parent "2", "3" → parent "ROOT"
    let node_map: HashMap<&str, &crate::models::GoalNode> =
        tree.nodes.iter().map(|n| (n.node_id.as_str(), n)).collect();

    for node in &tree.nodes {
        if node.node_id == tree.root_id {
            continue;
        }
        let parent_id = if let Some(dot_pos) = node.node_id.rfind('.') {
            &node.node_id[..dot_pos]
        } else {
            "ROOT"
        };
        if let Some(parent) = node_map.get(parent_id) {
            let (ps, pe) = parent.step_range;
            let (cs, ce) = node.step_range;
            if cs < ps || ce > pe {
                anomalies.push(format!(
                    "Node '{}' step_range [{}-{}] exceeds parent '{}' range [{}-{}]",
                    node.node_id, cs, ce, parent.node_id, ps, pe
                ));
            }
        }
    }

    anomalies
}

/// Enforce: each node has at most one outgoing edge.
/// If a node has both Sub and Next outgoing, keep only the Sub (Sub defines hierarchy).
/// If a node has multiple Next outgoing, keep only the first.
#[allow(dead_code)]
/// Fill empty backtrack labels from failed nodes using the node's result field.
/// The result field contains the concrete root cause of failure.
/// Called as a fallback after max retries — ensures the graph is always complete.
fn fill_missing_backtrack_labels(tree: &mut GoalTransitionTree) {
    use std::collections::HashMap;
    let node_info: HashMap<String, (String, String, crate::models::GoalStatus)> = tree
        .nodes
        .iter()
        .map(|n| {
            (
                n.node_id.clone(),
                (n.result.clone(), n.label.clone(), n.status.clone()),
            )
        })
        .collect();

    for edge in &mut tree.edges {
        if edge.edge_type == crate::models::GoalEdgeType::Backtrack && edge.label.trim().is_empty()
        {
            if let Some((result, label, status)) = node_info.get(&edge.source_id) {
                if *status == crate::models::GoalStatus::Failed
                    || *status == crate::models::GoalStatus::Abandoned
                {
                    // Prefer result (has the root cause), fall back to label
                    edge.label = if !result.is_empty() {
                        result.chars().take(120).collect::<String>()
                    } else {
                        format!("Failed: {}", label.chars().take(80).collect::<String>())
                    };
                }
            }
        }
    }
}

fn enforce_single_outgoing_edge(tree: &mut GoalTransitionTree) {
    use std::collections::HashMap;

    let mut outgoing: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, edge) in tree.edges.iter().enumerate() {
        outgoing.entry(edge.source_id.clone()).or_default().push(i);
    }

    let mut to_remove = Vec::new();
    for (_node_id, edge_indices) in &outgoing {
        if edge_indices.len() <= 1 {
            continue;
        }

        let has_sub = edge_indices
            .iter()
            .any(|&i| tree.edges[i].edge_type == crate::models::GoalEdgeType::Sub);

        if has_sub {
            // Keep the Sub edge, remove all Next/Backtrack from this node
            for &i in edge_indices {
                if tree.edges[i].edge_type != crate::models::GoalEdgeType::Sub {
                    to_remove.push(i);
                }
            }
        } else {
            // Multiple Next edges: keep only the first, remove the rest
            for &i in &edge_indices[1..] {
                to_remove.push(i);
            }
        }
    }

    if !to_remove.is_empty() {
        to_remove.sort_unstable();
        to_remove.dedup();
        eprintln!(
            "Removed {} conflicting edge(s) to enforce single-outgoing rule",
            to_remove.len()
        );
        // Remove in reverse order to preserve indices
        for i in to_remove.into_iter().rev() {
            tree.edges.remove(i);
        }
    }
}

/// Ensure every last-in-plan node has a Backtrack edge to its parent.
/// "Last in plan" = no outgoing Next edge AND not the root.
/// This enforces the invariant: every node has exactly one outgoing edge.
fn add_missing_backtrack_edges(tree: &mut GoalTransitionTree) {
    use std::collections::HashMap;

    // Build parent map (Sub priority over Next)
    let mut parent_map: HashMap<String, Option<String>> = HashMap::new();
    parent_map.insert(tree.root_id.clone(), None);
    let mut changed = true;
    while changed {
        changed = false;
        for edge in &tree.edges {
            if edge.edge_type == crate::models::GoalEdgeType::Sub {
                let existing = parent_map.get(&edge.target_id);
                if existing.is_none() || existing == Some(&None) {
                    parent_map.insert(edge.target_id.clone(), Some(edge.source_id.clone()));
                    changed = true;
                }
            }
        }
        for edge in &tree.edges {
            if edge.edge_type == crate::models::GoalEdgeType::Next {
                if let Some(sp) = parent_map.get(&edge.source_id).cloned() {
                    if !parent_map.contains_key(&edge.target_id) {
                        parent_map.insert(edge.target_id.clone(), sp);
                        changed = true;
                    }
                }
            }
        }
    }

    // Find nodes with no outgoing edge and add backtrack to parent
    let mut to_add = Vec::new();
    for node in &tree.nodes {
        if node.node_id == tree.root_id {
            continue;
        }
        let has_outgoing = tree.edges.iter().any(|e| e.source_id == node.node_id);
        if !has_outgoing {
            if let Some(Some(parent_id)) = parent_map.get(&node.node_id) {
                to_add.push(crate::models::GoalEdge {
                    edge_type: crate::models::GoalEdgeType::Backtrack,
                    source_id: node.node_id.clone(),
                    target_id: parent_id.clone(),
                    label: String::new(),
                });
            }
        }
    }

    if !to_add.is_empty() {
        eprintln!("Added {} missing backtrack edge(s)", to_add.len());
        tree.edges.extend(to_add);
    }
}

/// Fix inconsistent statuses bottom-up.
/// If a parent is marked "done" but has any failed/abandoned children,
/// mark it as "partial" (Wip) to indicate partial success — some subgoals
/// succeeded while others didn't. This preserves the LLM's intent without
/// claiming full success where failures exist.
fn propagate_status(tree: &mut GoalTransitionTree) {
    use crate::models::GoalStatus;
    use std::collections::HashMap;

    // Build parent→children map from edges
    let mut parent_map: HashMap<String, Option<String>> = HashMap::new();
    let root_id = tree.root_id.clone();
    parent_map.insert(root_id.clone(), None);
    // Sub edges always define parent (higher priority than Next).
    // Process Sub edges first, then Next edges only for nodes not yet assigned.
    let mut changed = true;
    while changed {
        changed = false;
        // Pass 1: Sub edges (definitive parent-child)
        for edge in &tree.edges {
            if edge.edge_type == crate::models::GoalEdgeType::Sub {
                let existing = parent_map.get(&edge.target_id);
                if existing.is_none() || existing == Some(&None) {
                    parent_map.insert(edge.target_id.clone(), Some(edge.source_id.clone()));
                    changed = true;
                }
            }
        }
        // Pass 2: Next edges (sibling inference, only if Sub didn't already set parent)
        for edge in &tree.edges {
            if edge.edge_type == crate::models::GoalEdgeType::Next {
                if let Some(sp) = parent_map.get(&edge.source_id).cloned() {
                    if !parent_map.contains_key(&edge.target_id) {
                        parent_map.insert(edge.target_id.clone(), sp);
                        changed = true;
                    }
                }
            }
        }
    }

    let mut children_of: HashMap<String, Vec<String>> = HashMap::new();
    for (child, parent_opt) in &parent_map {
        if let Some(parent) = parent_opt {
            children_of
                .entry(parent.clone())
                .or_default()
                .push(child.clone());
        }
    }

    let mut status_map: HashMap<String, GoalStatus> = tree
        .nodes
        .iter()
        .map(|n| (n.node_id.clone(), n.status.clone()))
        .collect();

    // Bottom-up pass: deepest nodes first
    let mut node_ids: Vec<String> = status_map.keys().cloned().collect();
    node_ids.sort_by(|a, b| b.matches('.').count().cmp(&a.matches('.').count()));

    for nid in &node_ids {
        if let Some(children) = children_of.get(nid) {
            if children.is_empty() {
                continue;
            }
            let current_status = status_map.get(nid).cloned().unwrap_or(GoalStatus::Partial);
            let has_failed_child = children.iter().any(|c| {
                let s = status_map.get(c);
                s == Some(&GoalStatus::Failed) || s == Some(&GoalStatus::Abandoned)
            });
            let all_children_failed = children.iter().all(|c| {
                let s = status_map.get(c);
                s == Some(&GoalStatus::Failed) || s == Some(&GoalStatus::Abandoned)
            });

            if all_children_failed {
                status_map.insert(nid.clone(), GoalStatus::Failed);
            } else if current_status == GoalStatus::Done && has_failed_child {
                // Some children failed, some succeeded — partial success
                status_map.insert(nid.clone(), GoalStatus::Partial);
            }
        }
    }

    for node in &mut tree.nodes {
        if let Some(status) = status_map.get(&node.node_id) {
            node.status = status.clone();
        }
    }
}

/// Replace opaque node IDs (g0, g1, ...) with hierarchical IDs (1, 1.1, 1.2, ...).
/// Follows Sub→Next edge chains to determine ordering within each level.
#[allow(dead_code)]
fn assign_hierarchical_ids(tree: &mut GoalTransitionTree) {
    use std::collections::HashMap;

    let root_id = if !tree.root_id.is_empty() {
        tree.root_id.clone()
    } else if let Some(first) = tree.nodes.first() {
        first.node_id.clone()
    } else {
        return;
    };

    // Build parent map from Sub/Next edges
    let mut parent_map: HashMap<String, Option<String>> = HashMap::new();
    parent_map.insert(root_id.clone(), None);
    let mut changed = true;
    while changed {
        changed = false;
        for edge in &tree.edges {
            match edge.edge_type {
                crate::models::GoalEdgeType::Sub => {
                    if !parent_map.contains_key(&edge.target_id) {
                        parent_map.insert(edge.target_id.clone(), Some(edge.source_id.clone()));
                        changed = true;
                    }
                }
                crate::models::GoalEdgeType::Next => {
                    if let Some(source_parent) = parent_map.get(&edge.source_id).cloned() {
                        if !parent_map.contains_key(&edge.target_id) {
                            parent_map.insert(edge.target_id.clone(), source_parent);
                            changed = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }

    // BFS assigning hierarchical IDs. For each node, collect all children
    // (nodes whose parent in the parent_map is this node) and assign in edge order.
    let mut id_map: HashMap<String, String> = HashMap::new();
    let mut counters: HashMap<String, usize> = HashMap::new();
    id_map.insert(root_id.clone(), "ROOT".to_string());

    let mut queue = vec![root_id.clone()];
    while let Some(current) = queue.pop() {
        // Collect all children of this node (from parent_map)
        let children: Vec<String> = parent_map
            .iter()
            .filter_map(|(child, parent_opt)| {
                if let Some(parent) = parent_opt {
                    if parent == &current {
                        Some(child.clone())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Order children: start with Sub target, then follow Next chain,
        // then append any remaining children not in the chain
        let mut ordered: Vec<String> = Vec::new();
        let first_child = tree
            .edges
            .iter()
            .find(|e| e.source_id == current && e.edge_type == crate::models::GoalEdgeType::Sub)
            .map(|e| e.target_id.clone());

        if let Some(first) = first_child {
            ordered.push(first.clone());
            let mut cur = first;
            loop {
                let next = tree.edges.iter().find(|e| {
                    e.source_id == cur
                        && e.edge_type == crate::models::GoalEdgeType::Next
                        && !ordered.contains(&e.target_id)
                });
                match next {
                    Some(e) => {
                        ordered.push(e.target_id.clone());
                        cur = e.target_id.clone();
                    }
                    None => break,
                }
            }
        }
        // Append any children not yet in the ordered list
        for child in &children {
            if !ordered.contains(child) {
                ordered.push(child.clone());
            }
        }

        for child_id in ordered {
            if !id_map.contains_key(&child_id) {
                let counter = counters.entry(current.clone()).or_insert(0);
                *counter += 1;
                let parent_hid = id_map.get(&current).unwrap();
                let child_hid = format!("{}.{}", parent_hid, counter);
                id_map.insert(child_id.clone(), child_hid);
                queue.push(child_id);
            }
        }
    }

    // Apply rename: update all node_ids and edge source/target references
    tree.root_id = id_map
        .get(&tree.root_id)
        .cloned()
        .unwrap_or_else(|| tree.root_id.clone());
    for node in &mut tree.nodes {
        if let Some(new_id) = id_map.get(&node.node_id) {
            node.node_id = new_id.clone();
        }
    }
    for edge in &mut tree.edges {
        if let Some(new_id) = id_map.get(&edge.source_id) {
            edge.source_id = new_id.clone();
        }
        if let Some(new_id) = id_map.get(&edge.target_id) {
            edge.target_id = new_id.clone();
        }
    }
}

/// Remove nodes unreachable from root and edges referencing them.
/// The LLM sometimes generates orphan nodes only referenced by backtrack edges
/// but not connected into the tree via Sub/Next from root.
#[allow(dead_code)]
fn prune_orphan_nodes(tree: &mut GoalTransitionTree) {
    use std::collections::HashSet;

    let root_id = if !tree.root_id.is_empty() {
        tree.root_id.clone()
    } else if let Some(first) = tree.nodes.first() {
        first.node_id.clone()
    } else {
        return;
    };

    // BFS from root following Sub and Next edges (forward direction only)
    let mut reachable = HashSet::new();
    let mut queue = vec![root_id.clone()];
    while let Some(nid) = queue.pop() {
        if !reachable.insert(nid.clone()) {
            continue;
        }
        for edge in &tree.edges {
            if edge.source_id == nid
                && (edge.edge_type == crate::models::GoalEdgeType::Sub
                    || edge.edge_type == crate::models::GoalEdgeType::Next)
            {
                queue.push(edge.target_id.clone());
            }
        }
    }

    let before = tree.nodes.len();
    tree.nodes.retain(|n| reachable.contains(&n.node_id));
    tree.edges
        .retain(|e| reachable.contains(&e.source_id) && reachable.contains(&e.target_id));

    if tree.nodes.len() < before {
        eprintln!(
            "Pruned {} orphan node(s) unreachable from root",
            before - tree.nodes.len()
        );
    }
}

/// Build a stub Goal Transition Tree without LLM (for testing/fallback).
pub fn build_stub(trajectory: &Trajectory) -> GoalTransitionTree {
    use crate::models::{GoalStatus, GoalType};

    // Create a simple single-node tree based on trajectory outcome
    let status = if trajectory.outcome.to_lowercase().contains("solved") {
        GoalStatus::Done
    } else if trajectory.outcome.to_lowercase().contains("failed") {
        GoalStatus::Failed
    } else {
        GoalStatus::Abandoned
    };

    let root_node = GoalNode {
        node_id: "g0".to_string(),
        label: format!("Complete task ({})", trajectory.outcome),
        goal_type: GoalType::Explore,
        status,
        result: String::new(),
        details: String::new(),
        level: 0,
        step_range: (0, trajectory.steps.len() - 1),
        cost: trajectory.total_cost.clone(),
        reasoning_artifacts: vec![],
    };

    GoalTransitionTree {
        nodes: vec![root_node],
        edges: vec![],
        root_id: "g0".to_string(),
    }
}

#[cfg(feature = "llm")]
fn sample_trajectory_steps(trajectory: &Trajectory, max_samples: usize) -> String {
    let step_count = trajectory.steps.len();

    if step_count <= max_samples {
        return trajectory
            .steps
            .iter()
            .enumerate()
            .map(|(idx, step)| format_step_summary(idx, step))
            .collect::<Vec<_>>()
            .join("\n\n");
    }

    // Sample strategy: biased toward the end (where failures/conclusions happen).
    // First 3 + sparse middle + dense last 40% (captures failure details).
    let mut sampled_indices = vec![0, 1, 2]; // First 3

    // Sparse middle (first 60% of trajectory)
    let middle_end = (step_count * 6) / 10;
    let middle_count = max_samples / 3;
    for i in 1..=middle_count {
        let idx = 3 + ((middle_end - 3) * i) / (middle_count + 1);
        sampled_indices.push(idx);
    }

    // Dense tail (last 40% — where exploit attempts and failures live)
    let tail_start = middle_end;
    let tail_count = max_samples - middle_count - 3;
    for i in 0..tail_count {
        let idx = tail_start + ((step_count - tail_start) * i) / tail_count;
        sampled_indices.push(idx);
    }

    // Always include the very last steps
    if step_count >= 3 {
        sampled_indices.extend(vec![step_count - 3, step_count - 2, step_count - 1]);
    }

    // Deduplicate and sort
    sampled_indices.sort_unstable();
    sampled_indices.dedup();

    sampled_indices
        .iter()
        .filter_map(|&idx| trajectory.steps.get(idx).map(|step| (idx, step)))
        .map(|(idx, step)| format_step_summary(idx, step))
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(feature = "llm")]
fn format_step_summary(idx: usize, step: &crate::models::Step) -> String {
    let item_summaries: Vec<String> = step
        .items
        .iter()
        .take(5)
        .map(|item| {
            let primary_obj = item
                .args
                .get("file_path")
                .or_else(|| item.args.get("command"))
                .map(|s| s.as_str())
                .unwrap_or("N/A");

            format!(
                "  - {:?} [{}]: {}",
                item.category,
                primary_obj,
                truncate(&item.content, 80)
            )
        })
        .collect();

    let cost = step.items.iter().map(|i| i.cost.dollar_cost).sum::<f64>();

    format!(
        "Step #{} (${:.4}):\n{}{}",
        idx,
        cost,
        item_summaries.join("\n"),
        if step.items.len() > 5 {
            format!("\n  ... and {} more items", step.items.len() - 5)
        } else {
            String::new()
        }
    )
}

#[cfg(feature = "llm")]
fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

#[cfg(feature = "llm")]
fn parse_goal_tree_response(response: &str) -> LLMResult<GoalTransitionTree> {
    use serde_json;

    // Try to extract JSON from response (might have markdown code blocks)
    let json_str = if response.contains("```json") {
        response
            .split("```json")
            .nth(1)
            .and_then(|s| s.split("```").next())
            .unwrap_or(response)
    } else if response.contains("```") {
        response.split("```").nth(1).unwrap_or(response)
    } else {
        response
    }
    .trim();

    // Parse JSON
    let parsed: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
        crate::llm::traits::LLMError::InvalidResponse(format!(
            "Failed to parse JSON response: {}",
            e
        ))
    })?;

    // Extract nodes
    let nodes: Vec<GoalNode> = parsed["nodes"]
        .as_array()
        .ok_or_else(|| {
            crate::llm::traits::LLMError::InvalidResponse(
                "Missing 'nodes' array in response".to_string(),
            )
        })?
        .iter()
        .map(|node| {
            use crate::models::{GoalStatus, GoalType};

            let step_range = node["step_range"]
                .as_array()
                .and_then(|arr| Some((arr[0].as_u64()? as usize, arr[1].as_u64()? as usize)))
                .unwrap_or((0, 0));

            let status_str = node["status"].as_str().unwrap_or("partial");
            let status = match status_str {
                "achieved" | "done" => GoalStatus::Done,
                "failed" => GoalStatus::Failed,
                "abandoned" => GoalStatus::Abandoned,
                _ => GoalStatus::Partial,
            };

            let label = node["label"].as_str().unwrap_or("Unknown goal");
            let result = node["result"].as_str().unwrap_or("").to_string();

            // Parse goal_type from LLM output, fall back to inference from label
            let goal_type_str = node["goal_type"].as_str().unwrap_or("");
            let goal_type = match goal_type_str {
                "explore" => GoalType::Explore,
                "think" | "analyze" | "plan" => GoalType::Think,
                "act" | "verify" | "report" | "write" | "execute" => GoalType::Act,
                _ => {
                    let l = label.to_lowercase();
                    if l.contains("write")
                        || l.contains("craft")
                        || l.contains("create")
                        || l.contains("run")
                        || l.contains("execute")
                        || l.contains("submit")
                        || l.contains("test")
                        || l.contains("verify")
                        || l.contains("attempt")
                    {
                        GoalType::Act
                    } else if l.contains("analyze")
                        || l.contains("decide")
                        || l.contains("plan")
                        || l.contains("identify")
                        || l.contains("determine")
                    {
                        GoalType::Think
                    } else {
                        GoalType::Explore
                    }
                }
            };

            let details = node["details"].as_str().unwrap_or("").to_string();

            GoalNode {
                node_id: node["node_id"].as_str().unwrap_or("g0").to_string(),
                label: label.to_string(),
                goal_type,
                status,
                result,
                details,
                level: 0,
                step_range,
                cost: crate::models::Cost::default(),
                reasoning_artifacts: vec![],
            }
        })
        .collect();

    // Extract edges (accept both "edge_type" and legacy "transition_type" field names)
    let edges: Vec<GoalEdge> = parsed["edges"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|edge| {
            use crate::models::GoalEdgeType;

            let type_str = edge["edge_type"]
                .as_str()
                .or_else(|| edge["transition_type"].as_str())
                .unwrap_or("next");
            let edge_type = match type_str {
                "backtrack" => GoalEdgeType::Backtrack,
                "sub" | "refinement" => GoalEdgeType::Sub,
                _ => GoalEdgeType::Next,
            };

            GoalEdge {
                edge_type,
                source_id: edge["source_id"].as_str().unwrap_or("ROOT").to_string(),
                target_id: edge["target_id"].as_str().unwrap_or("1").to_string(),
                label: String::new(),
            }
        })
        .collect();

    // Root is "ROOT" or the first node
    let root_id = nodes
        .first()
        .map(|n| n.node_id.clone())
        .unwrap_or_else(|| "ROOT".to_string());

    Ok(GoalTransitionTree {
        nodes,
        edges,
        root_id,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Cost, Item, ItemCategory, Step};

    #[test]
    fn test_build_stub() {
        use chrono::NaiveDateTime;
        let epoch = NaiveDateTime::default();

        let trajectory = Trajectory {
            label: "test".to_string(),
            steps: vec![Step {
                step_id: 0,
                items: vec![],
                timestamp_start: epoch,
                timestamp_end: epoch,
                raw_line_range: (0, 0),
            }],
            total_cost: Cost::default(),
            outcome: "FAILED".to_string(),
        };

        let tree = build_stub(&trajectory);
        assert_eq!(tree.nodes.len(), 1);
        assert_eq!(tree.nodes[0].status, crate::models::GoalStatus::Failed);
    }
}
