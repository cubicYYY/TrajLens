/// Reasoning Artifact DAG (G2) builder - LLM-based reasoning extraction.
///
/// Analyzes trajectory to extract:
/// - Ground truths vs insights
/// - Inference relationships
/// - Contradictions and supersessions
/// - Confidence scores
use crate::models::{
    InsightStatus, ReasoningArtifactDAG, ReasoningArtifactNode, ReasoningEdge, Trajectory,
};

#[cfg(feature = "llm")]
use crate::llm::traits::LLMResult;

/// Build a Reasoning Artifact DAG from a trajectory using an LLM.
///
/// # Arguments
/// * `trajectory` - The parsed trajectory
/// * `model` - Model specification in "provider/model-name" format
///   - "anthropic/claude-sonnet-4-6"
///   - "bedrock/us.anthropic.claude-sonnet-4-6"
///
/// # Returns
/// Result containing the ReasoningArtifactDAG or an error message
///
/// # Example
/// ```rust,no_run
/// use trajlens::graphs::reasoning_dag;
///
/// #[tokio::main]
/// async fn main() {
///     let dag = reasoning_dag::build_with_llm(&trajectory, "anthropic/claude-sonnet-4-6")
///         .await.unwrap();
/// }
/// ```
#[cfg(feature = "llm")]
pub async fn build_with_llm(
    trajectory: &Trajectory,
    model: &str,
) -> LLMResult<ReasoningArtifactDAG> {
    // Create LLM client from provider/model spec
    use crate::llm::model_registry;
    let llm_client = model_registry::create_client(model).await?;
    let reasoning_content = extract_reasoning_content(trajectory);

    let system_prompt = r#"You are reconstructing an AI agent's internal reasoning as a directed acyclic graph.

Write ALL node content from the agent's FIRST-PERSON perspective — what it noticed, believed, attempted, and concluded. Never use third-person descriptions like "the agent got stuck" or "the agent failed to identify". Instead write what the agent actually believed or observed at that moment.

# Node Types

1. **ground_truth** — A fact the agent observed via tool output (confidence always 1.0)
   - Write as: what was seen/read/confirmed
   - e.g., "gp_file_name_sizeof = 4096 (defined in gp.h:382)"
   - e.g., "Running PoC produced exit_code=1 with stack-buffer-overflow at pdf_font.c:332"
   - e.g., "PoC attempt produced exit_code=0 — no crash triggered"

2. **hypothesis** — A belief the agent held about how to solve the problem
   - Write as: the agent's stated strategy/approach at that moment
   - e.g., "The overflow should be triggerable via PostScript concatstrings path"
   - e.g., "Switching to PDF input format should reach the pdfi C-level fallback code"
   - Status must be one of:
     - "verified": hypothesis led to success (crash triggered, test passed, etc.)
     - "disproven": hypothesis is FUNDAMENTALLY WRONG — evidence shows the approach CANNOT work regardless of implementation (e.g., "PS path doesn't reach the vulnerable C function at all")
     - "self-falsed": hypothesis is CORRECT in principle but agent abandoned it — either because implementation was buggy (bad PDF structure, wrong offsets) or budget ran out before it could be properly tested
     - "unverified": hypothesis was never tested or is still open
   - CRITICAL DISTINCTION: A failed PoC attempt (exit_code=0) does NOT prove a hypothesis wrong. It only proves THAT SPECIFIC IMPLEMENTATION didn't work. Mark as "disproven" ONLY when evidence shows the approach itself is impossible, not when the attempt was just badly executed.
   - step_range: [when_formed, when_abandoned_or_verified]

3. **insight** — A realization that shifted the agent's understanding
   - Write as: the inference the agent drew
   - e.g., "pdfi_fmap_file_exists has a bounds check, so the unguarded overflow must be in pdfi_open_CIDFont_substitute_file"
   - e.g., "The font name buffer is only 47 bytes — Registry + '-' + Ordering just needs to exceed that"
   - Confidence 0.0-1.0

# Edge Types

- **infers**: Observation A led to belief B
- **contradicts**: A and B cannot both be true
- **supersedes**: Newer hypothesis B replaced older hypothesis A
- **falsifies**: Observation A disproved hypothesis B

# Output Format (strict JSON)

{
  "nodes": [
    {
      "node_id": "r0",
      "artifact_type": "ground_truth|hypothesis|insight",
      "content": "First-person description of what was observed/believed/realized",
      "confidence": 0.95,
      "step_index": 5,
      "step_range": [5, 25],
      "status": "verified|self-falsed|disproven|unverified",
      "source": "Brief pointer to evidence"
    }
  ],
  "edges": [
    {
      "source_ids": ["r0", "r1"],
      "target_id": "r2",
      "relationship_type": "infers|contradicts|supersedes|falsifies",
      "description": "Brief explanation"
    }
  ]
}

# Critical Guidelines

- ALL content must be FIRST-PERSON perspective of the agent. Describe what it believed, observed, tried, and concluded. Never write meta-commentary about the agent from outside.

- HYPOTHESIS LIFECYCLE is the most important output. For each major approach:
  1. What did the agent believe would work? (the hypothesis)
  2. How long was it pursued? (step_range)
  3. What observation disproved it? (falsifies edge from a ground_truth)
  4. What replaced it? (supersedes edge to next hypothesis)

- Every abandoned hypothesis must have either:
  - A "falsifies" incoming edge from evidence that PROVES the approach cannot work (only for "disproven" status), OR
  - A "supersedes" outgoing edge to the hypothesis that replaced it (for "self-falsed" — agent moved on despite the approach being potentially viable)
  - Do NOT use "falsifies" when a PoC attempt simply returned exit_code=0. That's just "the implementation didn't work this time", not proof the approach is wrong. Use "supersedes" instead.

- Track PIVOTS explicitly via supersedes edges between consecutive hypotheses.

- step_range is REQUIRED for hypothesis nodes.

- Group steps into 15-30 reasoning artifacts that explain the trajectory's outcome. Prefer fewer meaningful nodes over many trivial ones. Each node should reflect the major insight or belief of the agent affecting agent's decision after the step. Make sure critical diverge points are captured as nodes.

- Use MULTI-SOURCE edges (source_ids with 2+ entries) when a conclusion was derived from MULTIPLE premises jointly. For example, if the agent combined "buffer is 4096 bytes" (r0) AND "no bounds check in function X" (r1) to conclude "overflow is in function X" (r2), write: {"source_ids": ["r0", "r1"], "target_id": "r2", ...}. This is semantically different from two separate 1-to-1 edges — it means the conclusion required BOTH premises together, not either alone.
"#;

    // Extract outcome evidence: search final steps from the END of items
    // (verification/crash results appear at the end of long steps)
    let final_content = {
        let mut evidence_items = Vec::new();
        for step in trajectory
            .steps
            .iter()
            .rev()
            .take(2)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
        {
            for item in step.items.iter().rev() {
                let c = item.content.to_lowercase();
                if c.contains("verify")
                    || c.contains("success")
                    || c.contains("failed")
                    || c.contains("crash")
                    || c.contains("exit code: 1")
                    || c.contains("stack-buffer-overflow")
                    || c.contains("solved")
                    || c.contains("submit")
                    || c.contains("successstate")
                {
                    // Extract clean content from tool_result JSON blobs
                    let clean = extract_tool_content(&item.content);
                    evidence_items.push(truncate(&clean, 400));
                    if evidence_items.len() >= 3 {
                        break;
                    }
                }
            }
        }
        evidence_items.reverse();
        evidence_items.join("\n---\n")
    };

    // Classify trajectory's terminal state.
    // We deliberately differentiate three cases:
    //   ConcludedSuccess — explicit success marker found
    //   ConcludedFailure — explicit failure marker found
    //   Unknown          — trajectory just ends; no conclusion event
    //
    // The "Unknown" case is what tripped up earlier G2 generation: the LLM
    // would default to optimism (status=verified) for the final informative
    // step, hallucinating success when really the agent just timed out.
    let outcome_class = classify_trajectory_outcome(trajectory, &final_content);

    let user_message = format!(
        r#"Analyze this agent's reasoning process and extract the reasoning artifact DAG:

Trajectory Summary:
- Total steps: {}
- Trajectory outcome label (filename hint): {}
- Detected terminal state: {}

Final Step (outcome evidence):
{}

Reasoning Content (Think items and key observations):
{}

# TERMINAL NODE REQUIREMENT

The DAG MUST include a terminal node (the LAST node you emit) representing the
final state of the trajectory. The terminal node's `status` is determined by
the **Detected terminal state** above, NOT by the filename hint:

- If terminal state is "concluded_success": status="verified", content describes
  the explicit success evidence (e.g., "Server confirmed exploit succeeded with proof X").

- If terminal state is "concluded_failure": status="self-falsed" (or "disproven"
  if applicable), content describes the explicit failure (e.g., "Verifier rejected
  exploit; final attempt produced no crash").

- If terminal state is "unknown": the agent's trajectory simply STOPPED without
  reporting a conclusion. The terminal node's status MUST be "unverified" and
  its content MUST explicitly say so — e.g., "Trajectory ended without an
  explicit conclusion event (no done/submit/result marker reached)".
  DO NOT mark the terminal node as "verified" in this case. The agent did
  NOT verify success — the trajectory simply ran out.

Extract reasoning artifacts (ground truths and insights) and their relationships.
Return ONLY valid JSON, no additional text."#,
        trajectory.steps.len(),
        trajectory.outcome,
        outcome_class.as_label(),
        truncate(&final_content, 600),
        reasoning_content
    );

    // [LLM_CALL: cached] system_prompt is a fixed inline string literal
    let response = llm_client
        .as_ref()
        .complete(system_prompt, &user_message)
        .await?;

    // Parse JSON response
    let mut parsed = parse_reasoning_dag_response(&response)?;

    // Enforce the terminal-node contract for unknown-outcome trajectories.
    // If the LLM hallucinated a "verified" terminal node despite no conclusion
    // being detected, force-correct it: change status to Unverified and prepend
    // a notice to the content so reviewers can see the trajectory was incomplete.
    if outcome_class == TrajectoryOutcomeClass::Unknown {
        if let Some(terminal) = parsed.nodes.last_mut() {
            let was_verified = matches!(terminal.status, Some(InsightStatus::Verified));
            terminal.status = Some(InsightStatus::Unverified);
            if was_verified
                || !terminal.content.to_lowercase().contains("ended without")
                    && !terminal.content.to_lowercase().contains("no conclusion")
                    && !terminal.content.to_lowercase().contains("trajectory ended")
            {
                terminal.content = format!(
                    "[OUTCOME UNKNOWN] Trajectory ended without an explicit conclusion event \
                     (no done/submit/result marker observed). Final observation: {}",
                    truncate(&terminal.content, 300)
                );
            }
        }
    }

    Ok(parsed)
}

/// Trajectory's terminal classification.
///
/// Recorded in the rendered SVG metrics so reviewers can tell when a graph
/// is showing "the agent gave up" vs. "the agent succeeded".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrajectoryOutcomeClass {
    /// Explicit success marker present (server verified, crash triggered, etc.)
    ConcludedSuccess,
    /// Explicit failure marker present (verifier rejected, no crash, budget exhausted with confirmation)
    ConcludedFailure,
    /// Trajectory just ends — no terminal marker. Most dangerous case for downstream analysis.
    Unknown,
}

impl TrajectoryOutcomeClass {
    pub fn as_label(&self) -> &'static str {
        match self {
            Self::ConcludedSuccess => "concluded_success",
            Self::ConcludedFailure => "concluded_failure",
            Self::Unknown => "unknown",
        }
    }
}

/// Classify a trajectory's terminal state from explicit conclusion markers.
///
/// Generic across log formats: looks for any of these in the final 5 steps:
/// - Success markers: "stack-buffer-overflow", "successstate", "verified",
///   "exploit succeeded", "crash confirmed"
/// - Failure markers: "verify failed", "no crash", "verifier rejected",
///   "all attempts failed", "budget exhausted"
/// - Conclusion markers (regardless of pass/fail): "done", "submit_exploit_proof",
///   "Result: Score", "completed"
///
/// If neither success nor failure is detected AND no conclusion marker exists,
/// returns Unknown — meaning the agent likely timed out or was killed without
/// reporting an outcome.
pub fn classify_trajectory_outcome(
    trajectory: &Trajectory,
    final_evidence: &str,
) -> TrajectoryOutcomeClass {
    // Priority 1: trajectory.outcome field (set by parser or outcome inference)
    let outcome_upper = trajectory.outcome.to_uppercase();
    if outcome_upper == "SOLVED" || outcome_upper == "SUCCESS" {
        return TrajectoryOutcomeClass::ConcludedSuccess;
    }
    if outcome_upper == "FAILED" || outcome_upper == "FAILURE" {
        return TrajectoryOutcomeClass::ConcludedFailure;
    }

    let evidence = final_evidence.to_lowercase();

    // Aggregate text from the last 5 steps
    let mut tail_text = String::new();
    for step in trajectory.steps.iter().rev().take(5) {
        for item in &step.items {
            tail_text.push_str(&item.content);
            tail_text.push('\n');
        }
    }
    let tail = tail_text.to_lowercase();

    let success_markers = [
        "stack-buffer-overflow",
        "successstate",
        "exploit succeeded",
        "crash confirmed",
        "server confirmed",
        "verified by server",
        "score 1/",
        "success: true",
        "verified: true",
        "successfully completed",
        "validated successfully",
        "exploit was validated",
        "result: pass",
    ];
    let failure_markers = [
        "verify failed",
        "verifier rejected",
        "no crash was triggered",
        "all attempts failed",
        "budget exhausted",
        "exploit_failure",
        "success: false",
        "result: fail",
        "result: false",
        "timed out",
        "timeout reached",
    ];
    let conclusion_markers = [
        "submit_exploit_proof",
        "result: score",
        "trajectory complete",
        "agent completed",
        "submit_proof",
    ];

    let has_any = |text: &str, markers: &[&str]| markers.iter().any(|m| text.contains(m));

    if has_any(&evidence, &success_markers) || has_any(&tail, &success_markers) {
        return TrajectoryOutcomeClass::ConcludedSuccess;
    }
    if has_any(&evidence, &failure_markers) || has_any(&tail, &failure_markers) {
        return TrajectoryOutcomeClass::ConcludedFailure;
    }

    if has_any(&tail, &conclusion_markers) {
        TrajectoryOutcomeClass::ConcludedFailure
    } else {
        TrajectoryOutcomeClass::Unknown
    }
}

/// Build a stub Reasoning DAG without LLM (for testing/fallback).
pub fn build_stub(trajectory: &Trajectory) -> ReasoningArtifactDAG {
    use crate::models::ReasoningNodeType;

    // Create simple DAG with trajectory outcome as single insight
    let root_node = ReasoningArtifactNode {
        node_id: "r0".to_string(),
        node_type: ReasoningNodeType::GroundTruth,
        content: format!("Task outcome: {}", trajectory.outcome),
        confidence: 1.0,
        source_step_id: trajectory.steps.len() - 1,
        status: None,
        step_range: None,
    };

    ReasoningArtifactDAG {
        nodes: vec![root_node],
        edges: vec![],
    }
}

/// Extract readable content from items that may contain raw JSON tool_result blobs.
/// Pulls out the 'content' field value from tool_result structures.
#[cfg(feature = "llm")]
fn extract_tool_content(raw: &str) -> String {
    // If it looks like a JSON tool_result blob, try to extract the content field
    if raw.contains("'content':") && raw.contains("tool_result") {
        // Find content strings: 'content': "..." or 'content': '...'
        let mut extracted = Vec::new();
        for part in raw.split("'content':") {
            let trimmed = part.trim_start();
            if let Some(start) = trimmed.find(|c| c == '"' || c == '\'') {
                let delim = trimmed.as_bytes()[start] as char;
                if let Some(end) = trimmed[start + 1..].find(delim) {
                    let content = &trimmed[start + 1..start + 1 + end];
                    if content.len() > 5 {
                        extracted.push(content.replace("\\n", "\n"));
                    }
                }
            }
        }
        if !extracted.is_empty() {
            return extracted.join("\n");
        }
    }
    raw.to_string()
}

#[cfg(feature = "llm")]
fn extract_reasoning_content(trajectory: &Trajectory) -> String {
    use crate::models::ItemCategory;

    // Collect reasoning-relevant items, dropping tool results and noise
    let mut all_items: Vec<(usize, &crate::models::Item)> = trajectory
        .steps
        .iter()
        .enumerate()
        .flat_map(|(step_idx, step)| {
            step.items.iter().filter_map(move |item| {
                let c = item.content.to_lowercase();

                // DROP: items that are clearly tool output, not reasoning
                if matches!(item.category, ItemCategory::Action) {
                    // Raw tool results: file contents, grep output, JSON blobs
                    if c.starts_with("rc=")
                        || c.starts_with("{")
                        || c.starts_with("[")
                        || c.starts_with("}]}")
                        || c.starts_with("  ")  // indented code output
                        || c.contains("'role': 'user', 'content': [{'type': 'tool_result'")
                        || c.contains("\"role\": \"user\"")
                        || c.contains("tool_use_id")
                        || c.contains("cache_control")
                        || c.contains("#include")
                        || c.contains("#define")
                    {
                        return None;
                    }
                }

                // DROP: very short items with no signal
                if item.content.len() < 30 {
                    return None;
                }

                // KEEP: explicit reasoning markers (high confidence)
                let has_reasoning_signal = matches!(item.category, ItemCategory::Think)
                    || c.contains("reasoning:")
                    || c.contains("hypothesis")
                    || c.contains("exit code:")
                    || c.contains("crash")
                    || c.contains("stuck")
                    || c.contains("budget")
                    || c.contains("verify")
                    || c.contains("submit")
                    || c.contains("features enabled")
                    || c.contains("iteration");

                if has_reasoning_signal {
                    return Some((step_idx, item));
                }

                // KEEP: medium-length items that aren't raw code/data
                if item.content.len() > 80 && item.content.len() < 1500 {
                    // Drop if it looks like source code or raw data
                    let code_indicators = c.contains("static int")
                        || c.contains("static void")
                        || c.contains("typedef")
                        || c.contains("copyright")
                        || c.contains("license")
                        || c.starts_with("/*")
                        || c.starts_with("//")
                        || c.starts_with("#!/");
                    if !code_indicators {
                        return Some((step_idx, item));
                    }
                }

                None
            })
        })
        .collect();

    if all_items.is_empty() {
        return String::new();
    }

    // Cap at 200 items to stay within model context limits (~100K tokens).
    // Items are already in chronological order.
    if all_items.len() > 200 {
        let total = all_items.len();
        // Keep first 40, last 40, and evenly sample 120 from the middle
        let mut sampled = Vec::new();
        sampled.extend_from_slice(&all_items[..40]);
        let middle = &all_items[40..total - 40];
        let step = middle.len() / 120;
        if step > 0 {
            for i in (0..middle.len()).step_by(step).take(120) {
                sampled.push(middle[i]);
            }
        } else {
            sampled.extend_from_slice(middle);
        }
        sampled.extend_from_slice(&all_items[total - 40..]);
        all_items = sampled;
    }

    all_items
        .iter()
        .map(|(step_idx, item)| {
            format!(
                "Step #{} [{:?}]:\nContent: {}",
                step_idx,
                item.category,
                truncate(&item.content, 500)
            )
        })
        .collect::<Vec<_>>()
        .join("\n\n---\n\n")
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
fn parse_reasoning_dag_response(response: &str) -> LLMResult<ReasoningArtifactDAG> {
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
    let nodes: Vec<ReasoningArtifactNode> = parsed["nodes"]
        .as_array()
        .ok_or_else(|| {
            crate::llm::traits::LLMError::InvalidResponse(
                "Missing 'nodes' array in response".to_string(),
            )
        })?
        .iter()
        .map(|node| {
            use crate::models::{InsightStatus, ReasoningNodeType};

            let artifact_type = node["artifact_type"].as_str().unwrap_or("insight");
            let node_type = match artifact_type {
                "ground_truth" => ReasoningNodeType::GroundTruth,
                _ => ReasoningNodeType::Insight,
            };

            let status = match node["status"].as_str().unwrap_or("") {
                "verified" => Some(InsightStatus::Verified),
                "self-falsed" | "self_falsed" | "disproven" => Some(InsightStatus::SelfFalsed),
                "unverified" => Some(InsightStatus::Unverified),
                _ => {
                    if matches!(node_type, ReasoningNodeType::Insight) {
                        Some(InsightStatus::Unverified)
                    } else {
                        None
                    }
                }
            };

            let step_range = node["step_range"].as_array().and_then(|arr| {
                if arr.len() == 2 {
                    Some((
                        arr[0].as_u64().unwrap_or(0) as usize,
                        arr[1].as_u64().unwrap_or(0) as usize,
                    ))
                } else {
                    None
                }
            });

            ReasoningArtifactNode {
                node_id: node["node_id"].as_str().unwrap_or("r0").to_string(),
                node_type,
                content: node["content"].as_str().unwrap_or("Unknown").to_string(),
                confidence: node["confidence"].as_f64().unwrap_or(0.5),
                source_step_id: node["step_index"].as_u64().unwrap_or(0) as usize,
                status,
                step_range,
            }
        })
        .collect();

    // Extract edges
    let edges: Vec<ReasoningEdge> = parsed["edges"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|edge| {
            use crate::models::ReasoningEdgeType;

            let relationship_type = edge["relationship_type"].as_str().unwrap_or("infers");
            let edge_type = match relationship_type {
                "contradicts" | "falsifies" => ReasoningEdgeType::Contradicts,
                "supersedes" => ReasoningEdgeType::Supersedes,
                _ => ReasoningEdgeType::Infers,
            };

            // Handle both single source_id and array source_ids
            let source_ids = if let Some(arr) = edge["source_ids"].as_array() {
                arr.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            } else if let Some(single) = edge["source_id"].as_str() {
                vec![single.to_string()]
            } else {
                vec!["r0".to_string()]
            };

            ReasoningEdge {
                edge_type,
                source_ids,
                target_id: edge["target_id"].as_str().unwrap_or("r1").to_string(),
            }
        })
        .collect();

    Ok(ReasoningArtifactDAG { nodes, edges })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Cost, Step};

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
            outcome: "SOLVED".to_string(),
        };

        let dag = build_stub(&trajectory);
        assert_eq!(dag.nodes.len(), 1);
        assert_eq!(
            dag.nodes[0].node_type,
            crate::models::ReasoningNodeType::GroundTruth
        );
    }
}
