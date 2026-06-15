/// TrajLens CLI: parse agent logs, build graphs, render visualizations.
///
/// Commands:
/// - analyze: end-to-end (RECOMMENDED) — parse + split by agent + build all graphs + render
/// - parse: raw log → Trajectory JSON (atomic primitive)
/// - build: Trajectory → IGR TOML (single graph; LLM or deterministic by graph_type)
/// - render: IGR TOML → SVG (atomic primitive)
/// - generate-parser: LLM-generated fingerprint + parser script for a new log format
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use trajlens::compilers::GraphCompiler;
use trajlens::graphs::{activity_graph, cost_map};
use trajlens::igr;
use trajlens::models::{GraphEnum, Trajectory};
use trajlens::parsing::cost_estimator;

#[cfg(feature = "svg-rust")]
use trajlens::compilers::SVGCompiler;

#[cfg(feature = "svg-python")]
use trajlens::compilers::SVGPythonCompiler;

#[derive(Parser)]
#[command(name = "trajlens")]
#[command(about = "Transform agent execution logs into structured graph visualizations")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Analyze one or more log files end-to-end: parse, split by agent, and
    /// generate all graphs (G1 goal-tree, G2 reasoning-dag, G3 activity, G4 cost-map).
    ///
    /// This is the recommended entry point. Accepts file paths and/or glob patterns;
    /// each input is processed in parallel into its own subdirectory under --output.
    /// Each agent within a multi-agent log gets its own subdirectory with up to 4 graphs.
    ///
    /// Examples:
    ///   trajlens analyze trace.log -o out/
    ///   trajlens analyze "logs/**/*.log" -o batch/
    ///   trajlens analyze trace.log -o out/ --graphs activity,cost      # skip LLM graphs
    ///   trajlens analyze trace.log -o out/ --graphs g1,g2,g3,g4        # all (default)
    #[cfg(feature = "llm")]
    Analyze {
        /// Input log file path(s) or glob pattern(s).
        /// Multiple positional args are accepted; each is either a literal path
        /// or a glob (e.g., "logs/*.log").
        inputs: Vec<String>,

        /// Output directory. One subdirectory is created per input log file.
        #[arg(short, long)]
        output: PathBuf,

        /// Log format. "auto" detects from fingerprints (default).
        #[arg(long, default_value = "auto")]
        format: String,

        /// Comma-separated list of graphs to generate.
        /// Aliases: g1=goal-tree, g2=reasoning-dag, g3=activity, g4=cost.
        /// Default: all four.
        #[arg(long, default_value = "g1,g2,g3,g4")]
        graphs: String,

        /// LLM model spec ("provider/model-name") for G1, G2 and any LLM patcher.
        /// For testing on AWS: bedrock/us.anthropic.claude-sonnet-4-6
        #[arg(long, default_value = "anthropic/claude-sonnet-4-6")]
        model: String,

        /// Maximum estimated LLM cost budget in dollars. If the estimated cost
        /// of running all requested LLM graphs exceeds this, the command aborts
        /// before making any LLM calls. Default: $100.
        /// Use --dangerously-unlimited-budget to bypass.
        #[arg(long, default_value = "100.0")]
        budget: f64,

        /// Bypass the budget check entirely. Use with caution on large batches.
        #[arg(long, default_value = "false")]
        dangerously_unlimited_budget: bool,
    },

    /// Parse one or more log files into per-agent Trajectory JSONs (atomic primitive).
    ///
    /// Output is always a directory: `<output>/<log_stem>/<agent_id>/trajectory.json`.
    /// A single log is just a batch with size=1 — no special-cased flat layout.
    /// Most users want `analyze` instead (this command stops at parsing).
    Parse {
        /// Log format: auto or any registered parser name.
        #[arg(long, default_value = "auto")]
        format: String,

        /// Input log file path(s) or glob pattern(s).
        inputs: Vec<String>,

        /// Output directory (one subdirectory per input log, then one per agent).
        #[arg(short, long)]
        output: PathBuf,

        /// Optional: Process with LLM semantic processor to extract categories/args.
        /// Format: "provider/model-name" (e.g., "anthropic/claude-sonnet-4-6")
        #[arg(long)]
        semantic_model: Option<String>,
    },

    /// Build a single graph from Trajectory JSON.
    ///
    /// Dispatches to LLM-based or deterministic builder based on graph_type:
    ///   - activity-graph, cost-map: deterministic, no LLM call
    ///   - goal-tree, reasoning-dag: requires LLM (specify --model)
    Build {
        /// Graph type: activity-graph | cost-map | goal-tree | reasoning-dag.
        /// Aliases: g1=goal-tree, g2=reasoning-dag, g3=activity-graph, g4=cost-map.
        graph_type: String,

        /// Input Trajectory JSON path
        input: PathBuf,

        /// Output IGR TOML path
        #[arg(short, long)]
        output: PathBuf,

        /// Optional Goal Tree IGR TOML (for cost-map nesting; ignored for other types).
        #[arg(long)]
        goal_tree: Option<PathBuf>,

        /// LLM model spec (required only for goal-tree and reasoning-dag).
        #[cfg(feature = "llm")]
        #[arg(long, default_value = "anthropic/claude-sonnet-4-6")]
        model: String,
    },

    /// Render an IGR TOML file to SVG (atomic primitive).
    Render {
        /// Input IGR TOML path
        input: PathBuf,

        /// Output SVG path
        #[arg(short, long)]
        output: PathBuf,
    },

    /// Generate a parser (fingerprint + script) from example log files
    #[cfg(feature = "llm")]
    GenerateParser {
        /// Input log file(s) to analyze (can specify multiple)
        input: Vec<PathBuf>,

        /// Format name for the generated parser (e.g., "nova2_auditor")
        #[arg(long)]
        name: String,

        /// Model specification in "provider/model-name" format
        /// Default: anthropic/claude-sonnet-4-6
        /// For testing: bedrock/us.anthropic.claude-sonnet-4-6
        #[arg(long, default_value = "anthropic/claude-sonnet-4-6")]
        model: String,

        /// Maximum retries for generation (default: 3)
        #[arg(long, default_value = "3")]
        max_retries: usize,

        /// Optional free-form description of the log batch passed to the LLM
        /// (helps it understand domain semantics — what tool produced this log,
        /// what agents/sub-agents are present, what the trajectory represents).
        #[arg(long)]
        description: Option<String>,

        /// Optional path to a description file (alternative to --description).
        /// The file's contents are passed verbatim to the LLM as context.
        #[arg(long)]
        description_file: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        #[cfg(feature = "llm")]
        Commands::Analyze {
            inputs,
            output,
            format,
            graphs,
            model,
            budget,
            dangerously_unlimited_budget,
        } => cmd_analyze(
            &inputs,
            &output,
            &format,
            &graphs,
            &model,
            budget,
            dangerously_unlimited_budget,
        ),

        Commands::Parse {
            format,
            inputs,
            output,
            semantic_model,
        } => cmd_parse(&inputs, &output, &format, semantic_model.as_deref()),

        Commands::Build {
            graph_type,
            input,
            output,
            goal_tree,
            #[cfg(feature = "llm")]
            model,
        } => {
            #[cfg(feature = "llm")]
            {
                cmd_build(
                    &graph_type,
                    &input,
                    &output,
                    goal_tree.as_deref(),
                    Some(&model),
                )
            }
            #[cfg(not(feature = "llm"))]
            {
                cmd_build(&graph_type, &input, &output, goal_tree.as_deref(), None)
            }
        }

        Commands::Render { input, output } => cmd_render(&input, &output),

        #[cfg(feature = "llm")]
        Commands::GenerateParser {
            input,
            name,
            model,
            max_retries,
            description,
            description_file,
        } => cmd_generate_parser(
            &input,
            &name,
            &model,
            max_retries,
            description.as_deref(),
            description_file.as_deref(),
        ),
    }
}

// ============ Command Implementations ============

/// Resolve a parser config from format name (or auto-detect).
fn resolve_parser_config(
    format: &str,
    raw: &str,
    registry: &trajlens::parsing::parser_registry::ParserRegistry,
) -> Result<trajlens::parsing::parser_config::ParserConfig> {
    let config = match format {
        "auto" => {
            println!("Auto-detecting format...");
            let detected = registry.detect_format(raw)
                .ok_or_else(|| anyhow::anyhow!(
                    "Could not detect log format. Available: {:?}",
                    registry.list_formats()
                ))?;
            println!("Detected format: {}", detected);
            registry.get(&detected)
                .ok_or_else(|| anyhow::anyhow!("Format config not found"))?
                .clone()
        }
        format_name => {
            registry.get(format_name)
                .ok_or_else(|| anyhow::anyhow!(
                    "Unknown format: {}. Available: {:?}\nHint: Add a config to parsers/configs/{}.toml",
                    format_name, registry.list_formats(), format_name
                ))?
                .clone()
        }
    };
    Ok(config)
}

/// Infer outcome from filename suffix, or from trajectory content.
///
/// Priority:
/// 1. If outcome is already set (not PARSED/UNKNOWN), keep it.
/// 2. Check filename for SOLVED/FAILED.
/// 3. Check last few steps for outcome events (type="event", sub_type="outcome").
/// 4. Check last step content for success/failure keywords.
fn infer_outcome_from_filename(input: &Path, current: &str) -> String {
    if current != "PARSED" && current != "UNKNOWN" {
        return current.to_string();
    }
    let filename = input
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_uppercase();
    if filename.contains("SOLVED") || filename.contains("SUCCESS") {
        "SOLVED".to_string()
    } else if filename.contains("FAILED") || filename.contains("FAILURE") {
        "FAILED".to_string()
    } else {
        current.to_string()
    }
}

/// Infer outcome from a trajectory's step content.
///
/// Scans the last N steps for explicit success/failure markers. Handles multiple
/// formats: operation sub_type="outcome", structured fields, natural language.
fn infer_outcome_from_trajectory(traj: &Trajectory) -> Option<String> {
    // Scan last 8 steps (conclusion may be a few steps before the end)
    let tail_content: String = traj
        .steps
        .iter()
        .rev()
        .take(8)
        .flat_map(|s| s.items.iter())
        .map(|item| item.content.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let upper = tail_content.to_uppercase();

    // Check operation-level outcome markers
    for step in traj.steps.iter().rev().take(5) {
        for item in &step.items {
            if item.sub_category.as_deref() == Some("outcome") {
                let args_str = item
                    .args
                    .values()
                    .map(|v| v.to_uppercase())
                    .collect::<Vec<_>>()
                    .join(" ");
                if args_str.contains("SUCCESS") || args_str.contains("SOLVED") {
                    return Some("SOLVED".to_string());
                }
                if args_str.contains("FAIL") {
                    return Some("FAILED".to_string());
                }
            }
        }
    }

    // Broad content-based markers (order matters — check success first to avoid
    // false negatives from "Success: False" containing "Success")
    let success_patterns = [
        "SUCCESS: TRUE",
        "VERIFIED: TRUE",
        "\"SUCCESS\": TRUE",
        "\"VERIFIED\": TRUE",
        "EXPLOIT WAS VALIDATED SUCCESSFULLY",
        "SUCCESSFULLY COMPLETED AND VALIDATED",
        "EXPLOIT SUCCEEDED",
        "SUCCESSFULLY EXPLOITED",
    ];
    let failure_patterns = [
        "SUCCESS: FALSE",
        "\"SUCCESS\": FALSE",
        "RESULT: FAIL",
        "ALL ATTEMPTS FAILED",
        "EXPLOIT FAILED",
        "TIMED OUT",
        "TIMEOUT REACHED",
        "BUDGET EXHAUSTED",
    ];

    for pat in &success_patterns {
        if upper.contains(pat) {
            return Some("SOLVED".to_string());
        }
    }
    for pat in &failure_patterns {
        if upper.contains(pat) {
            return Some("FAILED".to_string());
        }
    }

    None
}

/// Infer outcome from the source file's top-level metadata.
///
/// Many structured log formats (JSON trajectories) have top-level fields like
/// `"success": true/false` or `"verified": true/false` that definitively state
/// the outcome. This function reads the source file and checks for these fields.
/// Works for any JSON file with top-level success/failure indicators.
fn infer_outcome_from_source_metadata(input: &Path) -> Option<String> {
    if input.is_dir() {
        return None;
    }
    let content = std::fs::read_to_string(input).ok()?;
    // Only attempt JSON parsing on JSON-like files
    if !content.trim_start().starts_with('{') {
        return None;
    }
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    let obj = parsed.as_object()?;

    // Check common outcome field patterns
    // "success": true/false
    if let Some(val) = obj.get("success") {
        if val.as_bool() == Some(true) {
            return Some("SOLVED".to_string());
        } else if val.as_bool() == Some(false) {
            return Some("FAILED".to_string());
        }
    }
    // "verified": true/false (stronger than success in some formats)
    if let Some(val) = obj.get("verified") {
        if val.as_bool() == Some(true) {
            return Some("SOLVED".to_string());
        }
    }
    // "result": "success"/"failure"/"pass"/"fail"
    if let Some(val) = obj.get("result").and_then(|v| v.as_str()) {
        let lower = val.to_lowercase();
        if lower == "success" || lower == "pass" || lower == "solved" {
            return Some("SOLVED".to_string());
        }
        if lower == "failure" || lower == "fail" || lower == "failed" {
            return Some("FAILED".to_string());
        }
    }
    // "outcome": "success"/"failure"
    if let Some(val) = obj.get("outcome").and_then(|v| v.as_str()) {
        let lower = val.to_lowercase();
        if lower.contains("success") || lower.contains("solved") || lower.contains("pass") {
            return Some("SOLVED".to_string());
        }
        if lower.contains("fail") {
            return Some("FAILED".to_string());
        }
    }
    None
}

/// Parse a log file into one or more trajectories (split by agent_id).
///
/// Output behavior:
/// - Single agent (one trajectory): writes a single JSON at `output`.
/// - Multi-agent: treats `output` as a directory; writes
///   `<output>/<agent_id>/trajectory.json` per agent.
///
/// Returns the list of (agent_id, trajectory_json_path) so callers
/// (e.g. cmd_run) can build per-agent graphs.
/// Parse a raw agent execution log into structured trajectory JSON file(s).
///
/// This command identifies the log format via fingerprinting, runs the appropriate
/// parser script (from `parsers/scripts/`), and outputs one trajectory JSON per agent.
///
/// # Arguments
/// * `format` - Log format identifier (e.g., "claude-code", "pocgen"), or "auto" for fingerprinting
/// * `input` - Path to the raw log file
/// * `output` - Output path/directory for trajectory JSON file(s)
/// * `semantic_model` - Optional LLM model ID for semantic patching (e.g., fixing malformed tool calls)
///
/// # Output
/// - Single-agent logs: writes `<output>/trajectory.json`
/// - Multi-agent logs: writes `<output>/agent_<id>/trajectory.json` per agent
///
/// # Example
/// ```bash
/// trajlens parse input.log -o trajectory.json
/// trajlens parse --format claude-code input.log -o output_dir/
/// ```
/// `parse` command: parse one or more logs into per-agent trajectories.
///
/// Treats every invocation as a batch (single log = batch of size 1). For each
/// resolved input log, creates `<output>/<log_stem>/<agent_id>/trajectory.json`.
fn cmd_parse(
    inputs: &[String],
    output: &Path,
    format: &str,
    semantic_model: Option<&str>,
) -> Result<()> {
    let resolved = expand_inputs(inputs)?;
    fs::create_dir_all(output)
        .with_context(|| format!("Failed to create output directory: {}", output.display()))?;

    println!(
        "Parsing {} log file(s) → {}",
        resolved.len(),
        output.display()
    );

    let mut succeeded = 0usize;
    let mut failed: Vec<(PathBuf, String)> = Vec::new();
    for log_path in &resolved {
        let stem = if log_path.is_dir() {
            log_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
        } else {
            log_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
        };
        let log_out = output.join(stem);
        println!("--- log: {} → {}", log_path.display(), log_out.display());
        match parse_split(format, log_path, &log_out, semantic_model) {
            Ok(_) => succeeded += 1,
            Err(e) => {
                eprintln!("    ✗ failed: {}", e);
                failed.push((log_path.clone(), e.to_string()));
            }
        }
    }

    println!("\n=== Parse summary ===");
    println!("  {} log(s) succeeded", succeeded);
    if !failed.is_empty() {
        println!("  {} log(s) failed:", failed.len());
        for (p, e) in &failed {
            println!("    - {}: {}", p.display(), e);
        }
        anyhow::bail!("{} log(s) failed to parse", failed.len());
    }
    Ok(())
}

/// Internal helper: parse and return per-agent trajectory paths.
///
/// Input can be a file OR a directory (folder-based logs).
fn parse_split(
    format: &str,
    input: &Path,
    output: &Path,
    semantic_model: Option<&str>,
) -> Result<Vec<(String, PathBuf)>> {
    use trajlens::parsing::{parser_registry, script_runner};

    let registry = parser_registry::ParserRegistry::load_default()
        .map_err(|e| anyhow::anyhow!("Failed to load parser registry: {}", e))?;

    // For directories: use path-based format detection.
    // For files: read content for fingerprint matching.
    let (config, raw) = if input.is_dir() {
        let detected_format = if format == "auto" {
            registry.detect_format_from_path(input).ok_or_else(|| {
                anyhow::anyhow!(
                    "Could not auto-detect format for directory: {}. Use --format to specify.",
                    input.display()
                )
            })?
        } else {
            format.to_string()
        };
        let config = registry
            .get(&detected_format)
            .ok_or_else(|| anyhow::anyhow!("Unknown format: {}", detected_format))?
            .clone();
        (config, None)
    } else {
        let raw = fs::read_to_string(input)
            .with_context(|| format!("Failed to read log file: {}", input.display()))?;
        let config = resolve_parser_config(format, &raw, &registry)?;
        (config, Some(raw))
    };

    println!("Using parser script: {}", config.parser);
    let scripts_dir = script_runner::find_scripts_dir();
    let parser = script_runner::ScriptParser::new(config, scripts_dir);

    // Always split — single-agent logs return exactly one trajectory.
    let mut trajectories = parser
        .parse_file_split(input)
        .map_err(|e| anyhow::anyhow!("Parsing failed: {}", e))?;

    if trajectories.is_empty() {
        anyhow::bail!("Parser produced no trajectories");
    }

    let total_steps: usize = trajectories.iter().map(|t| t.steps.len()).sum();
    println!(
        "Parsed {} step(s) into {} trajectories",
        total_steps,
        trajectories.len()
    );

    // Apply outcome inference + cost estimation to each
    for traj in &mut trajectories {
        traj.outcome = infer_outcome_from_filename(input, &traj.outcome);
        // Source metadata is authoritative (it's the ground truth field from the
        // framework that ran the agent). Check it before content heuristics.
        if traj.outcome == "PARSED" || traj.outcome == "UNKNOWN" {
            if let Some(outcome) = infer_outcome_from_source_metadata(input) {
                traj.outcome = outcome;
            }
        }
        // Content heuristics as fallback (for formats without top-level metadata)
        if traj.outcome == "PARSED" || traj.outcome == "UNKNOWN" {
            if let Some(outcome) = infer_outcome_from_trajectory(traj) {
                traj.outcome = outcome;
            }
        }
        *traj = cost_estimator::estimate_costs(traj);
    }

    // Optional LLM semantic processing per agent
    if let Some(_model) = semantic_model {
        #[cfg(feature = "llm")]
        {
            use trajlens::parsing::llm_semantic_processor;
            println!("Processing with LLM semantic processor: {}", _model);
            let runtime = tokio::runtime::Runtime::new()?;
            for traj in &mut trajectories {
                *traj = runtime.block_on(async {
                    llm_semantic_processor::process_trajectory(traj, _model)
                        .await
                        .map_err(|e| anyhow::anyhow!("Semantic processing failed: {}", e))
                })?;
            }
            println!("✓ Semantic processing complete");
        }
        #[cfg(not(feature = "llm"))]
        {
            anyhow::bail!("Semantic processing requires LLM feature. Rebuild with --features llm-anthropic or llm-bedrock");
        }
    }

    let mut written = Vec::new();

    // Pre-split the original log into lines for per-agent log slices.
    // For directories, log slicing is not applicable (source is multi-file).
    let log_lines: Option<Vec<&str>> = raw.as_ref().map(|r| r.lines().collect());

    // Single uniform output layout, regardless of agent count.
    let base_dir = output.to_path_buf();
    fs::create_dir_all(&base_dir)
        .with_context(|| format!("Failed to create directory: {}", base_dir.display()))?;

    for traj in &trajectories {
        let agent_id = if traj.label.is_empty() {
            "main".to_string()
        } else {
            traj.label.clone()
        };
        let agent_dir = base_dir.join(safe_agent_dir(&agent_id));
        fs::create_dir_all(&agent_dir)
            .with_context(|| format!("Failed to create agent dir: {}", agent_dir.display()))?;
        let traj_path = agent_dir.join("trajectory.json");
        let file = File::create(&traj_path)
            .with_context(|| format!("Failed to create: {}", traj_path.display()))?;
        serde_json::to_writer_pretty(BufWriter::new(file), traj)
            .context("Failed to serialize Trajectory to JSON")?;

        // Each agent gets its own slice of the original log (file-based only).
        if let Some(ref lines) = log_lines {
            let slice_path = agent_dir.join("log_slice.log");
            let line_count = write_agent_log_slice(lines, traj, &slice_path)?;
            if line_count > 0 {
                println!(
                    "    log slice ({} lines) → {}",
                    line_count,
                    slice_path.display()
                );
            }
        }

        println!(
            "  ✓ agent='{}' steps={} cost=${:.4} → {}",
            agent_id,
            traj.steps.len(),
            traj.total_cost.dollar_cost,
            traj_path.display()
        );
        written.push((agent_id, traj_path));
    }

    Ok(written)
}

/// Sanitize an agent_id for use as a directory name.
fn safe_agent_dir(agent_id: &str) -> String {
    agent_id
        .chars()
        .map(|c| match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => c,
            _ => '_',
        })
        .collect()
}

/// Write the original log lines belonging to one agent's trajectory.
///
/// We collect every step's `raw_line_range` (0-based, end-EXCLUSIVE — see
/// `StepMetrics.line_range`) into a sorted set of line numbers, then dump
/// those lines verbatim. This preserves the exact bytes the agent "saw" —
/// useful for debugging, audit, and feeding the slice back to a re-parse
/// if the parser is updated.
///
/// IMPORTANT: end-EXCLUSIVE. A step with line_range=(10, 13) covers lines
/// at indices 10, 11, 12. This matches Python/Rust slicing convention and
/// avoids the bug where step N's range touched step N+1's first line.
///
/// Steps with line_range == (0, 0) are treated as "no source span" and skipped.
fn write_agent_log_slice(
    log_lines: &[&str],
    trajectory: &trajlens::models::Trajectory,
    out_path: &Path,
) -> Result<usize> {
    use std::collections::BTreeSet;

    let mut wanted: BTreeSet<usize> = BTreeSet::new();
    for step in &trajectory.steps {
        let (start, end) = step.raw_line_range;
        // Skip degenerate (0,0) ranges — they mean the parser didn't track this.
        if start == 0 && end == 0 {
            continue;
        }
        // Defensive: support both legacy (inclusive) and current (exclusive)
        // by treating start==end as "single line" (inclusive degenerate).
        let lo = start.min(end);
        let hi = start.max(end);
        let range_end = if lo == hi { hi + 1 } else { hi }; // end-exclusive
        for i in lo..range_end {
            if i < log_lines.len() {
                wanted.insert(i);
            }
        }
    }

    let mut content = String::new();
    let mut prev: Option<usize> = None;
    for &i in &wanted {
        // Insert a marker line if there's a gap (so reviewers can tell that
        // unrelated lines were dropped).
        if let Some(p) = prev {
            if i > p + 1 {
                content.push_str(&format!(
                    "\n[... gap: lines {}..{} omitted ...]\n",
                    p + 1,
                    i - 1
                ));
            }
        }
        content.push_str(log_lines[i]);
        content.push('\n');
        prev = Some(i);
    }

    fs::write(out_path, &content)
        .with_context(|| format!("Failed to write log slice: {}", out_path.display()))?;

    Ok(wanted.len())
}

/// Build a specific graph type from a trajectory JSON and serialize to IGR TOML.
///
/// This command constructs one of the four graph types (G1-G4) from a parsed trajectory
/// and writes it to the Intermediate Graph Representation (IGR) format.
///
/// # Arguments
/// * `graph_type` - One of: "activity-graph" (G3), "cost-map" (G4), "goal-tree" (G1), "reasoning-dag" (G2)
/// * `input` - Path to trajectory JSON file
/// * `output` - Path for output IGR TOML file (e.g., `graph.igr.toml`)
/// * `goal_tree_path` - Optional path to G1 IGR file (required for cost-map to link nodes)
///
/// # Graph Types
/// - **activity-graph (G3)**: Deterministic action sequence graph
/// - **cost-map (G4)**: Deterministic hierarchical cost breakdown tree
/// - **goal-tree (G1)**: Requires LLM — use `build-llm` command instead
/// - **reasoning-dag (G2)**: Requires LLM — use `build-llm` command instead
///
/// # Example
/// ```bash
/// trajlens build activity-graph trajectory.json -o activity.igr.toml
/// trajlens build cost-map trajectory.json --goal-tree goal.igr.toml -o cost.igr.toml
/// ```
/// Normalize a graph-type name to the canonical form.
/// Accepts aliases: g1, g2, g3, g4, activity, cost.
fn canonical_graph_type(s: &str) -> &str {
    match s.trim().to_lowercase().as_str() {
        "g1" | "goal-tree" | "goal_tree" | "goaltree" => "goal-tree",
        "g2" | "reasoning-dag" | "reasoning_dag" | "reasoning" | "dag" => "reasoning-dag",
        "g3" | "activity-graph" | "activity_graph" | "activity" => "activity-graph",
        "g4" | "cost-map" | "cost_map" | "cost" | "treemap" => "cost-map",
        _ => s,
    }
    // Note: returning &str of original on no match would break lifetimes; we
    // accept the static literal forms above and pass through unrecognized
    // values via the caller's bail.
}

/// Build a graph from a trajectory JSON.
///
/// Dispatches to deterministic or LLM-based builders based on graph_type:
///   - activity-graph, cost-map: no LLM call
///   - goal-tree, reasoning-dag: requires LLM (provide `model`)
///
/// `model` may be None when llm feature is disabled at compile time, in which
/// case requesting goal-tree/reasoning-dag will error.
fn cmd_build(
    graph_type: &str,
    input: &Path,
    output: &Path,
    goal_tree_path: Option<&Path>,
    #[allow(unused_variables)] model: Option<&str>,
) -> Result<()> {
    let traj_json = fs::read_to_string(input)
        .with_context(|| format!("Failed to read trajectory file: {}", input.display()))?;
    let trajectory: Trajectory =
        serde_json::from_str(&traj_json).context("Failed to parse Trajectory JSON")?;

    let canonical = canonical_graph_type(graph_type);

    let graph = match canonical {
        "activity-graph" => GraphEnum::ActivityGraph(activity_graph::build(&trajectory)),

        "cost-map" => {
            let goal_tree_opt = if let Some(path) = goal_tree_path {
                let igr_toml = fs::read_to_string(path)
                    .with_context(|| format!("Failed to read goal tree: {}", path.display()))?;
                let graph = igr::deserialize(&igr_toml)
                    .map_err(|e| anyhow::anyhow!("Failed to deserialize goal tree: {}", e))?;
                if let GraphEnum::GoalTree(tree) = graph {
                    Some(tree)
                } else {
                    anyhow::bail!("Goal tree file is not a GoalTransitionTree");
                }
            } else {
                None
            };
            GraphEnum::CostMap(cost_map::build(&trajectory, goal_tree_opt.as_ref()))
        }

        #[cfg(feature = "llm")]
        "goal-tree" => {
            let m = model.ok_or_else(|| anyhow::anyhow!(
                "goal-tree requires an LLM model — pass --model"))?;
            println!("Building goal-tree using model '{}'...", m);
            let runtime = tokio::runtime::Runtime::new()?;
            let tree = runtime.block_on(async {
                trajlens::graphs::goal_tree::build_with_llm(&trajectory, m)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to build goal tree: {}", e))
            })?;
            GraphEnum::GoalTree(tree)
        }

        #[cfg(feature = "llm")]
        "reasoning-dag" => {
            let m = model.ok_or_else(|| anyhow::anyhow!(
                "reasoning-dag requires an LLM model — pass --model"))?;
            println!("Building reasoning-dag using model '{}'...", m);
            let runtime = tokio::runtime::Runtime::new()?;
            let dag = runtime.block_on(async {
                trajlens::graphs::reasoning_dag::build_with_llm(&trajectory, m)
                    .await
                    .map_err(|e| anyhow::anyhow!("Failed to build reasoning DAG: {}", e))
            })?;
            GraphEnum::ReasoningDAG(dag)
        }

        #[cfg(not(feature = "llm"))]
        "goal-tree" | "reasoning-dag" => {
            anyhow::bail!(
                "{} requires the LLM feature. Rebuild with `--features llm-anthropic` or `--features llm-bedrock`.",
                canonical
            );
        }

        _ => anyhow::bail!(
            "Unknown graph type: {}. Use one of: activity-graph (g3), cost-map (g4), goal-tree (g1), reasoning-dag (g2).",
            graph_type
        ),
    };

    let igr_toml =
        igr::serialize(&graph).map_err(|e| anyhow::anyhow!("Failed to serialize to IGR: {}", e))?;

    fs::write(output, igr_toml)
        .with_context(|| format!("Failed to write IGR file: {}", output.display()))?;

    println!("✓ Built {} → {}", canonical, output.display());

    Ok(())
}

/// Render an IGR TOML file to SVG using the configured graph compiler.
///
/// This command loads a graph from Intermediate Graph Representation (IGR) format,
/// performs layout computation (Sugiyama hierarchical layout), and generates an SVG file.
///
/// # Arguments
/// * `input` - Path to IGR TOML file (e.g., `graph.igr.toml`)
/// * `output` - Path for output SVG file (e.g., `graph.svg`)
///
/// # Compiler Selection
/// Uses the first available renderer (build-time feature flags):
/// 1. `svg-rust` (default) — Pure Rust SVG generation
/// 2. `svg-python` — Python subprocess renderer (requires Python 3.12+)
///
/// # Example
/// ```bash
/// trajlens render activity-graph.igr.toml -o activity.svg
/// ```
fn cmd_render(input: &Path, output: &Path) -> Result<()> {
    let igr_toml = fs::read_to_string(input)
        .with_context(|| format!("Failed to read IGR file: {}", input.display()))?;

    let graph = igr::deserialize(&igr_toml)
        .map_err(|e| anyhow::anyhow!("Failed to deserialize IGR: {}", e))?;

    // Use the available SVG renderer (prefer Rust, fallback to Python)
    #[cfg(feature = "svg-rust")]
    let svg = {
        let compiler = SVGCompiler::new();
        compiler.compile(&graph)
    };

    #[cfg(all(feature = "svg-python", not(feature = "svg-rust")))]
    let svg = {
        let compiler = SVGPythonCompiler::new();
        compiler
            .compile(&graph)
            .map_err(|e| anyhow::anyhow!("Python SVG compiler failed: {}", e))?
    };

    #[cfg(not(any(feature = "svg-rust", feature = "svg-python", feature = "svg-mermaid")))]
    compile_error!("CLI requires at least one SVG compiler: svg-rust, svg-python, or svg-mermaid");

    fs::write(output, &svg)
        .with_context(|| format!("Failed to write SVG file: {}", output.display()))?;

    // Compute graph-rendering metrics for self-reflection.
    // Writes a `<output>.metrics.json` sidecar with stats + warnings.
    // This catches regressions like "all nodes on one row", "huge truncation",
    // "negative canvas dimensions", etc. without needing visual inspection.
    let metrics = compute_render_metrics(&graph, &svg);
    let metrics_path = output.with_extension("metrics.json");
    if let Ok(json) = serde_json::to_string_pretty(&metrics) {
        let _ = fs::write(&metrics_path, json);
    }

    println!("✓ Rendered {} → {}", input.display(), output.display());
    if !metrics.warnings.is_empty() {
        println!("  ⚠ {} warning(s):", metrics.warnings.len());
        for w in &metrics.warnings {
            println!("      - {}", w);
        }
    }

    Ok(())
}

/// Graph render-quality metrics.
///
/// Computed after SVG generation by parsing the SVG back. This is intentional —
/// we want to measure what was actually drawn, not what we intended to draw.
/// Catches bugs like "Sugiyama layout collapsed all nodes onto one row" that
/// don't surface as errors but produce useless graphs.
#[derive(serde::Serialize)]
struct RenderMetrics {
    /// Graph type name from the IGR.
    graph_type: String,
    /// Number of nodes in the source IGR.
    node_count: usize,
    /// Number of edges in the source IGR.
    edge_count: usize,
    /// SVG dimensions: (width, height) in px.
    canvas_size: (u32, u32),
    /// Distinct node y-coordinates (a proxy for layer/level count).
    distinct_y_levels: usize,
    /// Distinct node x-coordinates.
    distinct_x_columns: usize,
    /// Mean and stdev of node positions (helps detect collapsed layouts).
    x_spread: (f64, f64),
    y_spread: (f64, f64),
    /// Fraction of node labels that contain truncation markers ("..." or "…").
    truncation_ratio: f64,
    /// Bytes written to the SVG file.
    svg_bytes: usize,
    /// Reasoning DAG only: terminal-node outcome classification.
    /// One of "concluded_success", "concluded_failure", "unknown", or null
    /// for non-DAG graphs. Detected by inspecting the terminal node's content
    /// for the [OUTCOME UNKNOWN] sentinel that the builder forces when no
    /// conclusion event was found in the trajectory.
    #[serde(skip_serializing_if = "Option::is_none")]
    terminal_outcome: Option<String>,
    /// Warnings about likely-bad output.
    warnings: Vec<String>,
}

fn compute_render_metrics(graph: &GraphEnum, svg: &str) -> RenderMetrics {
    use regex::Regex;

    let (graph_type, node_count, edge_count) = match graph {
        GraphEnum::GoalTree(g) => ("goal_tree", g.nodes.len(), g.edges.len()),
        GraphEnum::ReasoningDAG(g) => ("reasoning_dag", g.nodes.len(), g.edges.len()),
        GraphEnum::ActivityGraph(g) => ("activity_graph", g.nodes.len(), g.edges.len()),
        GraphEnum::CostMap(_) => ("cost_map", 0, 0),
    };

    // Parse canvas dims from <svg width="..." height="...">
    let canvas_size = {
        let re = Regex::new(r#"<svg[^>]*\bwidth="(\d+)"[^>]*\bheight="(\d+)""#).unwrap();
        re.captures(svg)
            .and_then(|c| {
                let w = c.get(1)?.as_str().parse().ok()?;
                let h = c.get(2)?.as_str().parse().ok()?;
                Some((w, h))
            })
            .unwrap_or((0, 0))
    };

    // Collect node rect positions (x, y, w, h). We treat any rect with width >= 100
    // as a "node" rect (legend boxes are smaller).
    let rect_re = Regex::new(r#"<rect[^>]*\bx="(-?\d+(?:\.\d+)?)"[^>]*\by="(-?\d+(?:\.\d+)?)"[^>]*\bwidth="(\d+(?:\.\d+)?)"[^>]*\bheight="(\d+(?:\.\d+)?)""#).unwrap();
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    let mut node_rects: Vec<(f64, f64, f64, f64)> = Vec::new(); // (x, y, w, h)
    for cap in rect_re.captures_iter(svg) {
        let w: f64 = cap[3].parse().unwrap_or(0.0);
        if w < 100.0 {
            continue;
        }
        let x: f64 = cap[1].parse().unwrap_or(0.0);
        let y: f64 = cap[2].parse().unwrap_or(0.0);
        let h: f64 = cap[4].parse().unwrap_or(0.0);
        xs.push(x);
        ys.push(y);
        node_rects.push((x, y, w, h));
    }

    let distinct_y_levels = {
        let mut s: Vec<i64> = ys.iter().map(|y| y.round() as i64).collect();
        s.sort_unstable();
        s.dedup();
        s.len()
    };
    let distinct_x_columns = {
        let mut s: Vec<i64> = xs.iter().map(|x| x.round() as i64).collect();
        s.sort_unstable();
        s.dedup();
        s.len()
    };

    let mean_stdev = |v: &[f64]| -> (f64, f64) {
        if v.is_empty() {
            return (0.0, 0.0);
        }
        let m = v.iter().sum::<f64>() / v.len() as f64;
        let var = v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64;
        (m, var.sqrt())
    };

    let x_spread = mean_stdev(&xs);
    let y_spread = mean_stdev(&ys);

    // Truncation ratio: count text elements containing "..." or "…"
    let text_re = Regex::new(r#"<text[^>]*>([^<]*)</text>"#).unwrap();
    let mut total_texts = 0usize;
    let mut truncated = 0usize;
    for cap in text_re.captures_iter(svg) {
        total_texts += 1;
        let t = &cap[1];
        if t.contains("...") || t.contains("…") {
            truncated += 1;
        }
    }
    let truncation_ratio = if total_texts == 0 {
        0.0
    } else {
        truncated as f64 / total_texts as f64
    };

    // Warnings
    let mut warnings = Vec::new();

    if node_count >= 3 && distinct_y_levels <= 1 {
        warnings.push(format!(
            "all {} nodes are on a single y-level — layout likely collapsed (Sugiyama cycle bug?)",
            node_count
        ));
    }
    if node_count >= 3 && distinct_x_columns <= 1 {
        warnings.push(format!(
            "all {} nodes share the same x-column — layout collapsed",
            node_count
        ));
    }
    if canvas_size.0 > 5000 || canvas_size.1 > 5000 {
        warnings.push(format!(
            "canvas {}x{} px is very large — text may be unreadable when scaled to fit",
            canvas_size.0, canvas_size.1
        ));
    }
    if truncation_ratio > 0.5 {
        warnings.push(format!(
            "{:.0}% of text is truncated — labels may be unreadable",
            truncation_ratio * 100.0
        ));
    }
    if node_count > 0 && xs.is_empty() {
        warnings
            .push("no node rects detected in SVG — render may have failed silently".to_string());
    }
    if edge_count > 0 && !svg.contains("<line") && !svg.contains("<path") {
        warnings.push("no edge lines/paths in SVG even though IGR has edges".to_string());
    }

    // Node overlap detection: check if any two node rects intersect.
    if node_rects.len() >= 2 {
        let mut overlap_count = 0;
        for i in 0..node_rects.len() {
            for j in (i + 1)..node_rects.len() {
                let (x1, y1, w1, h1) = node_rects[i];
                let (x2, y2, w2, h2) = node_rects[j];
                // Two rects overlap if they intersect in both X and Y
                let x_overlap = x1 < x2 + w2 && x2 < x1 + w1;
                let y_overlap = y1 < y2 + h2 && y2 < y1 + h1;
                if x_overlap && y_overlap {
                    overlap_count += 1;
                }
            }
        }
        if overlap_count > 0 {
            warnings.push(format!(
                "{} node pair(s) overlap — labels may be unreadable or graph layout is broken",
                overlap_count
            ));
        }
    }

    // Check for content clipped by the canvas boundary.
    // Parse viewBox to get the visible area, then check if any element extends beyond.
    {
        let vb_re = Regex::new(r#"viewBox="(-?\d+(?:\.\d+)?)\s+(-?\d+(?:\.\d+)?)\s+(\d+(?:\.\d+)?)\s+(\d+(?:\.\d+)?)""#).unwrap();
        if let Some(cap) = vb_re.captures(svg) {
            let vb_x: f64 = cap[1].parse().unwrap_or(0.0);
            let vb_y: f64 = cap[2].parse().unwrap_or(0.0);
            let vb_w: f64 = cap[3].parse().unwrap_or(0.0);
            let vb_h: f64 = cap[4].parse().unwrap_or(0.0);
            let vb_right = vb_x + vb_w;
            let vb_bottom = vb_y + vb_h;

            // Check all rects (including legend) for overflow
            let all_rect_re = Regex::new(r#"<rect[^>]*\bx="(-?\d+(?:\.\d+)?)"[^>]*\by="(-?\d+(?:\.\d+)?)"[^>]*\bwidth="(\d+(?:\.\d+)?)"[^>]*\bheight="(\d+(?:\.\d+)?)""#).unwrap();
            let mut clipped = false;
            for cap in all_rect_re.captures_iter(svg) {
                let rx: f64 = cap[1].parse().unwrap_or(0.0);
                let ry: f64 = cap[2].parse().unwrap_or(0.0);
                let rw: f64 = cap[3].parse().unwrap_or(0.0);
                let rh: f64 = cap[4].parse().unwrap_or(0.0);
                if rx + rw > vb_right + 5.0
                    || ry + rh > vb_bottom + 5.0
                    || rx < vb_x - 5.0
                    || ry < vb_y - 5.0
                {
                    clipped = true;
                    break;
                }
            }
            if clipped {
                warnings.push("content (legend or nodes) extends beyond canvas boundary — elements may be clipped".to_string());
            }
        }
    }

    let aspect = if canvas_size.1 > 0 {
        canvas_size.0 as f64 / canvas_size.1 as f64
    } else {
        0.0
    };
    if aspect > 4.0 || (aspect > 0.0 && aspect < 0.25) {
        warnings.push(format!(
            "extreme aspect ratio {:.1}:1 — graph likely too wide or too tall",
            aspect
        ));
    }

    // Reasoning DAG: classify terminal-node outcome.
    //
    // Detection priority:
    //   1. Explicit [OUTCOME UNKNOWN] sentinel (force-injected by the builder
    //      when classify_trajectory_outcome returned Unknown).
    //   2. Natural-language hints in content ("trajectory ended without",
    //      "no conclusion", "no done/submit") — the LLM sometimes paraphrases.
    //   3. Status field: Verified → success; SelfFalsed → failure; Unverified → indeterminate.
    let terminal_outcome = match graph {
        GraphEnum::ReasoningDAG(dag) => {
            let last = dag.nodes.last();
            if let Some(node) = last {
                use trajlens::models::InsightStatus;
                let lc = node.content.to_lowercase();
                let unknown_phrases = [
                    "[outcome unknown]",
                    "trajectory ended without",
                    "ended without an explicit conclusion",
                    "no conclusion event",
                    "no done/submit",
                    "no terminal marker",
                    "agent did not report",
                ];
                let is_unknown = unknown_phrases.iter().any(|p| lc.contains(p));

                let class = if is_unknown {
                    "unknown"
                } else if matches!(node.status, Some(InsightStatus::Verified)) {
                    "concluded_success"
                } else if matches!(node.status, Some(InsightStatus::SelfFalsed)) {
                    "concluded_failure"
                } else {
                    "indeterminate"
                };
                if class == "unknown" {
                    warnings.push(
                        "trajectory ended without an explicit conclusion event — \
                         terminal node marked as outcome unknown (the agent did not \
                         report success or failure; it likely timed out or was killed)"
                            .to_string(),
                    );
                }
                Some(class.to_string())
            } else {
                None
            }
        }
        _ => None,
    };

    RenderMetrics {
        graph_type: graph_type.to_string(),
        node_count,
        edge_count,
        canvas_size,
        distinct_y_levels,
        distinct_x_columns,
        x_spread,
        y_spread,
        truncation_ratio,
        terminal_outcome,
        svg_bytes: svg.len(),
        warnings,
    }
}

/// End-to-end pipeline: parse a single log file, build all deterministic graphs, and render them.
///
/// This is the primary high-level command that executes the full TrajLens pipeline:
/// 1. Parse raw log → extract trajectory per agent (JSON)
/// 2. Build deterministic graphs per agent: Activity Graph (G3), Cost Map (G4)
/// 3. Render graphs to SVG
///
/// **Note:** G1 (Goal Tree) and G2 (Reasoning DAG) require LLM calls and must be built
/// separately using the `build-llm` command.
///
/// # Arguments
/// * `format` - Log format identifier for parser selection (e.g., "claude-code", "pocgen")
/// * `input` - Path to the input log file
/// * `output_dir` - Directory where all outputs will be written
///
/// # Output Structure
/// ```text
/// output_dir/
///   ├── agent_<id>/
///   │   ├── trajectory.json        (parsed trajectory)
///   │   ├── activity-graph.igr.toml (G3 intermediate representation)
///   │   ├── activity-graph.svg     (G3 rendered)
///   │   ├── cost-map.igr.toml      (G4 intermediate representation)
///   │   └── cost-map.svg           (G4 rendered)
///   └── ...                        (additional agents if multi-agent log)
/// ```
///
/// # Multi-Agent Support
/// If the log contains multiple agents, each gets its own subdirectory.
/// One agent's graph failure does not block others from being processed.
///
/// # Example
/// ```bash
/// trajlens run example_trajectories/G4_architecture/arvo_57672_cc_FAILED.log -o output/
/// ```
/// Resolve a list of input strings (paths and/or glob patterns) to concrete file paths.
fn expand_inputs(inputs: &[String]) -> Result<Vec<PathBuf>> {
    let mut out: Vec<PathBuf> = Vec::new();
    for raw in inputs {
        // If it contains glob meta-characters, expand; otherwise treat as a literal path.
        if raw.contains('*') || raw.contains('?') || raw.contains('[') {
            let matches: Vec<PathBuf> = glob::glob(raw)
                .with_context(|| format!("Invalid glob: {}", raw))?
                .filter_map(Result::ok)
                .collect();
            if matches.is_empty() {
                eprintln!("  ⚠ glob matched no files: {}", raw);
            }
            out.extend(matches);
        } else {
            let p = PathBuf::from(raw);
            if !p.exists() {
                anyhow::bail!("Input not found: {}", p.display());
            }
            out.push(p);
        }
    }
    if out.is_empty() {
        anyhow::bail!("No input log files resolved from: {:?}", inputs);
    }
    Ok(out)
}

/// Parse the `--graphs` CSV flag into the canonical set of graph-type names to build.
/// Aliases: g1, g2, g3, g4, activity, cost. Returns the list in canonical order.
fn parse_graphs_selector(graphs: &str) -> Result<Vec<&'static str>> {
    let mut want_g1 = false;
    let mut want_g2 = false;
    let mut want_g3 = false;
    let mut want_g4 = false;

    for token in graphs
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        match canonical_graph_type(token) {
            "goal-tree" => want_g1 = true,
            "reasoning-dag" => want_g2 = true,
            "activity-graph" => want_g3 = true,
            "cost-map" => want_g4 = true,
            other => anyhow::bail!(
                "Unknown graph type in --graphs: '{}'. Valid: g1/goal-tree, g2/reasoning-dag, \
                 g3/activity, g4/cost.",
                other
            ),
        }
    }

    let mut sel = Vec::new();
    if want_g1 {
        sel.push("goal-tree");
    }
    if want_g2 {
        sel.push("reasoning-dag");
    }
    if want_g3 {
        sel.push("activity-graph");
    }
    if want_g4 {
        sel.push("cost-map");
    }

    if sel.is_empty() {
        anyhow::bail!("--graphs is empty — specify at least one graph type");
    }
    Ok(sel)
}

/// `analyze` — the recommended end-to-end command.
///
/// For each input log:
///   1. Parse and split by agent_id into per-agent trajectories.
///   2. For each agent, build every graph in `selected_graphs` (LLM-based or not).
///   3. Render each IGR to SVG with metrics sidecar.
///
/// Inputs may be literal paths or glob patterns. Each input log gets its own
/// subdirectory under `output_dir`.
#[cfg(feature = "llm")]
fn cmd_analyze(
    inputs: &[String],
    output_dir: &Path,
    format: &str,
    graphs: &str,
    model: &str,
    budget: f64,
    unlimited: bool,
) -> Result<()> {
    let resolved = expand_inputs(inputs)?;
    let selected = parse_graphs_selector(graphs)?;

    let needs_llm = selected
        .iter()
        .any(|g| *g == "goal-tree" || *g == "reasoning-dag");

    // Budget check: estimate LLM cost before starting.
    // G1 + G2 each require one LLM call per agent. Estimate ~60K input + 2K output tokens per call.
    // Sonnet pricing: ~$3/1M input, ~$15/1M output.
    if needs_llm && !unlimited {
        let llm_graphs_count = selected
            .iter()
            .filter(|g| **g == "goal-tree" || **g == "reasoning-dag")
            .count();
        // Rough estimate: each input resolves to ~15 agents on average (from poc-agent-codex data).
        // Conservative: assume 20 agents per input for budget estimation.
        let estimated_agents = resolved.len() * 20;
        let estimated_calls = estimated_agents * llm_graphs_count;
        let cost_per_call = 60_000.0 * 3.0 / 1_000_000.0 + 2_000.0 * 15.0 / 1_000_000.0; // $0.21/call
        let estimated_cost = estimated_calls as f64 * cost_per_call;

        println!("Budget check:");
        println!(
            "  Inputs: {} log(s), ~{} agents (est.)",
            resolved.len(),
            estimated_agents
        );
        println!(
            "  LLM calls needed: ~{} ({} LLM graphs × {} agents)",
            estimated_calls, llm_graphs_count, estimated_agents
        );
        println!("  Estimated LLM cost: ${:.2}", estimated_cost);
        println!("  Budget limit: ${:.2}", budget);

        if estimated_cost > budget {
            anyhow::bail!(
                "Estimated LLM cost (${:.2}) exceeds budget (${:.2}). \
                 Options:\n  \
                 1. Use --graphs g3,g4 (deterministic, no LLM cost)\n  \
                 2. Increase --budget {:.0}\n  \
                 3. Use --dangerously-unlimited-budget to bypass\n  \
                 4. Reduce input set (fewer logs)",
                estimated_cost,
                budget,
                estimated_cost * 1.2
            );
        }
        println!("  ✓ Within budget. Proceeding.");
        println!();
    }

    fs::create_dir_all(output_dir).with_context(|| {
        format!(
            "Failed to create output directory: {}",
            output_dir.display()
        )
    })?;

    println!(
        "Analyzing {} log file(s) → {}",
        resolved.len(),
        output_dir.display()
    );
    println!("Graphs to generate: {:?}", selected);
    if needs_llm {
        println!("Model: {}", model);
    }
    println!();

    let mut overall_succeeded = 0usize;
    let mut overall_failed: Vec<(PathBuf, String)> = Vec::new();

    // Process logs sequentially (per-log work already parallelizes internally
    // via parser/script execution; LLM calls also serialize on the same model
    // anyway). Could be made rayon-parallel later if needed.
    for log_path in &resolved {
        let stem = if log_path.is_dir() {
            log_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
        } else {
            log_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
        };
        let log_out = output_dir.join(stem);
        println!("--- log: {} → {}", log_path.display(), log_out.display());

        match analyze_one_log(log_path, &log_out, format, &selected, model) {
            Ok(stats) => {
                overall_succeeded += 1;
                println!(
                    "    {} agent(s), {} graphs built, {} skipped",
                    stats.agent_count, stats.graphs_built, stats.graphs_skipped
                );
            }
            Err(e) => {
                eprintln!("    ✗ failed: {}", e);
                overall_failed.push((log_path.clone(), e.to_string()));
            }
        }
    }

    println!("\n=== Analyze summary ===");
    println!("  {} log(s) succeeded", overall_succeeded);
    if !overall_failed.is_empty() {
        println!("  {} log(s) failed:", overall_failed.len());
        for (p, e) in &overall_failed {
            println!("    - {}: {}", p.display(), e);
        }
    }

    Ok(())
}

#[cfg(feature = "llm")]
struct AnalyzeStats {
    agent_count: usize,
    graphs_built: usize,
    graphs_skipped: usize,
}

#[cfg(feature = "llm")]
fn analyze_one_log(
    log_path: &Path,
    log_out: &Path,
    format: &str,
    selected: &[&str],
    model: &str,
) -> Result<AnalyzeStats> {
    fs::create_dir_all(log_out)
        .with_context(|| format!("Failed to create dir: {}", log_out.display()))?;

    // Parse (split by agent_id) → per-agent trajectory.json paths.
    let agent_trajs = parse_split(format, log_path, log_out, None)?;

    let mut graphs_built = 0usize;
    let mut graphs_skipped = 0usize;

    for (agent_id, traj_path) in &agent_trajs {
        let agent_dir = traj_path.parent().unwrap_or(log_out);
        println!("    agent='{}' → {}", agent_id, agent_dir.display());

        // Build graphs in dependency-friendly order: goal-tree first (cost-map
        // can use it), reasoning-dag, activity, cost. Skip those not requested.
        let ordered: Vec<&&str> = ["goal-tree", "reasoning-dag", "activity-graph", "cost-map"]
            .iter()
            .filter(|g| selected.contains(g))
            .collect();

        for graph_type in ordered {
            let igr = agent_dir.join(format!("{}.igr.toml", graph_type));
            let svg = agent_dir.join(format!("{}.svg", graph_type));

            let goal_tree_for_cost = if *graph_type == "cost-map" {
                let gt = agent_dir.join("goal-tree.igr.toml");
                if gt.exists() {
                    Some(gt)
                } else {
                    None
                }
            } else {
                None
            };

            let result: Result<()> = (|| {
                cmd_build(
                    graph_type,
                    traj_path,
                    &igr,
                    goal_tree_for_cost.as_deref(),
                    Some(model),
                )?;
                cmd_render(&igr, &svg)?;
                Ok(())
            })();

            match result {
                Ok(()) => graphs_built += 1,
                Err(e) => {
                    eprintln!("      ⚠ skipping {} for '{}': {}", graph_type, agent_id, e);
                    graphs_skipped += 1;
                }
            }
        }
    }

    Ok(AnalyzeStats {
        agent_count: agent_trajs.len(),
        graphs_built,
        graphs_skipped,
    })
}

#[cfg(feature = "llm")]
fn cmd_generate_parser(
    inputs: &[PathBuf],
    format_name: &str,
    model: &str,
    max_retries: usize,
    description: Option<&str>,
    description_file: Option<&Path>,
) -> Result<()> {
    use trajlens::parsing::parser_agents::{self, ParserGenConfig};

    if inputs.is_empty() {
        anyhow::bail!("At least one log file is required");
    }

    for p in inputs {
        if !p.exists() {
            anyhow::bail!("Log file not found: {}", p.display());
        }
    }

    // Resolve the optional batch description.
    // Sources: --description (inline) or --description-file (path).
    // Both empty → None (no extra context).
    let resolved_description: Option<String> = match (description, description_file) {
        (Some(d), None) => Some(d.to_string()),
        (None, Some(f)) => Some(
            fs::read_to_string(f)
                .with_context(|| format!("Failed to read description file: {}", f.display()))?,
        ),
        (Some(_), Some(_)) => {
            anyhow::bail!("Use either --description or --description-file, not both")
        }
        (None, None) => None,
    };

    println!("Generating parser for format: {}", format_name);
    println!("Input logs: {} file(s)", inputs.len());
    println!("Model: {}", model);
    if let Some(desc) = &resolved_description {
        let preview: String = desc.chars().take(120).collect();
        println!(
            "Batch description: {}{}",
            preview,
            if desc.len() > preview.len() {
                "..."
            } else {
                ""
            }
        );
    }

    let config = ParserGenConfig {
        model: model.to_string(),
        max_retries,
        configs_dir: PathBuf::from("parsers/configs"),
        scripts_dir: PathBuf::from("parsers/scripts"),
        batch_description: resolved_description,
    };

    let runtime = tokio::runtime::Runtime::new()?;
    let result = runtime
        .block_on(async { parser_agents::generate_parser(inputs, format_name, &config).await })
        .map_err(|e| anyhow::anyhow!("Parser generation failed: {}", e))?;

    println!("\n✓ Parser generated successfully!");
    println!("  Config: {}", result.config_path.display());
    println!("  Script: {}", result.script_path.display());
    println!("  Fingerprint: {:?}", result.fingerprint);
    println!("\nTest it:");
    println!(
        "  cargo run --bin trajlens -- parse --format {} {} -o traj.json",
        format_name,
        inputs[0].display()
    );

    Ok(())
}
