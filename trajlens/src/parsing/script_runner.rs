/// Script-based parser: executes an external parser script and converts output.
///
/// The script receives a log file path as argv[1] and outputs Vec<StepInfo> as JSON
/// to stdout. This runner invokes the script, parses the JSON output, and converts
/// it into the internal Trajectory model.
///
/// Safety: parser scripts are sandboxed via nono (Landlock on Linux, Seatbelt on
/// macOS). The child process can only read the log file, parser scripts dir, and
/// Python stdlib — no network, no write access. This isolates each sample in a batch.
use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;

use crate::error::TrajLensError;
use crate::models::{Cost, Item, ItemCategory, Step, Trajectory};

use super::parser_config::{AgentIdRule, ParserConfig, StepInfo, StepMetrics};
use super::Parser;

use chrono::NaiveDateTime;

#[cfg(feature = "nono")]
use std::os::unix::process::CommandExt;

/// Parser that delegates to an external script.
pub struct ScriptParser {
    config: ParserConfig,
    scripts_dir: PathBuf,
}

impl ScriptParser {
    /// Create a ScriptParser from a config and scripts directory path.
    pub fn new(config: ParserConfig, scripts_dir: PathBuf) -> Self {
        Self {
            config,
            scripts_dir,
        }
    }

    /// Resolve the full path to the parser script.
    fn script_path(&self) -> PathBuf {
        self.scripts_dir.join(&self.config.parser)
    }

    /// Apply config-defined agent_id rules to steps that don't already have one.
    ///
    /// For each step with agent_id=None, test rules in order. The first matching
    /// rule's `assign` template (with capture group substitution) becomes the
    /// agent_id. Steps that already have an agent_id from the script are preserved.
    fn apply_agent_id_rules(&self, steps: &mut [StepInfo]) {
        if self.config.agent_id_rules.is_empty() {
            return;
        }

        // Pre-compile regexes once
        let compiled: Vec<(Regex, &AgentIdRule)> = self
            .config
            .agent_id_rules
            .iter()
            .filter_map(|rule| Regex::new(&rule.pattern).ok().map(|re| (re, rule)))
            .collect();

        for step in steps.iter_mut() {
            if step.agent_id.is_some() {
                continue;
            }

            // Build the searchable text: content + concatenated op args
            let mut search_text = step.content.clone();
            for op in &step.operations {
                search_text.push('\n');
                search_text.push_str(&op.op_type);
                if let Some(sub) = &op.sub_type {
                    search_text.push(':');
                    search_text.push_str(sub);
                }
                for arg in &op.args {
                    search_text.push('\n');
                    search_text.push_str(arg);
                }
            }

            for (re, rule) in &compiled {
                if let Some(caps) = re.captures(&search_text) {
                    let mut assigned = rule.assign.clone();
                    // Substitute $1, $2, ... with capture groups
                    for i in 1..caps.len() {
                        if let Some(cap) = caps.get(i) {
                            assigned = assigned.replace(&format!("${}", i), cap.as_str());
                        }
                    }
                    step.agent_id = Some(assigned);
                    break;
                }
            }
        }
    }

    /// Execute the parser script on a log path (file or directory) and return parsed
    /// StepInfo records.
    ///
    /// The path may be:
    /// - A single file (traditional): parser reads one file
    /// - A directory (folder-based logs): parser walks the tree and combines data
    ///
    /// When the `nono` feature is enabled, the child process is sandboxed via
    /// Landlock (Linux) or Seatbelt (macOS): read-only access to the log path,
    /// scripts dir, and Python stdlib; no network. This isolates each sample.
    pub fn run_script(&self, log_path: &Path) -> Result<Vec<StepInfo>, TrajLensError> {
        let script = self.script_path();
        if !script.exists() {
            return Err(TrajLensError::Config(format!(
                "Parser script not found: {} (resolved: {})",
                self.config.parser,
                script.display()
            )));
        }

        // Validate the log path contains no shell metacharacters
        let log_str = log_path.to_string_lossy();
        if log_str.contains(';')
            || log_str.contains('|')
            || log_str.contains('&')
            || log_str.contains('`')
            || log_str.contains('$')
        {
            return Err(TrajLensError::Config(format!(
                "Log path contains disallowed characters: {}",
                log_str
            )));
        }

        let mut cmd = Command::new("python3");
        cmd.arg(&script).arg(log_path);

        #[cfg(feature = "nono")]
        {
            // For sandboxing: allow read access to the log path (or its parent if file)
            let log_dir = if log_path.is_dir() {
                log_path.to_path_buf()
            } else {
                log_path.parent().unwrap_or(Path::new(".")).to_path_buf()
            };
            let scripts_dir = self.scripts_dir.clone();

            if nono::Sandbox::is_supported() {
                unsafe {
                    cmd.pre_exec(move || {
                        let caps = nono::CapabilitySet::new();
                        let caps = match caps.allow_path(&log_dir, nono::AccessMode::Read) {
                            Ok(c) => c,
                            Err(_) => return Ok(()),
                        };
                        let caps = match caps.allow_path(&scripts_dir, nono::AccessMode::Read) {
                            Ok(c) => c,
                            Err(_) => return Ok(()),
                        };
                        let caps = match caps.allow_path("/usr", nono::AccessMode::Read) {
                            Ok(c) => c,
                            Err(_) => return Ok(()),
                        };
                        let caps = match caps.allow_path("/lib", nono::AccessMode::Read) {
                            Ok(c) => c,
                            Err(_) => return Ok(()),
                        };
                        let caps = if Path::new("/lib64").exists() {
                            match caps.allow_path("/lib64", nono::AccessMode::Read) {
                                Ok(c) => c,
                                Err(_) => return Ok(()),
                            }
                        } else {
                            caps
                        };
                        let caps = match caps.allow_path("/etc", nono::AccessMode::Read) {
                            Ok(c) => c,
                            Err(_) => return Ok(()),
                        };
                        let caps = match caps.allow_path("/bin", nono::AccessMode::Read) {
                            Ok(c) => c,
                            Err(_) => return Ok(()),
                        };
                        let caps = match caps.allow_path("/tmp", nono::AccessMode::ReadWrite) {
                            Ok(c) => c,
                            Err(_) => return Ok(()),
                        };
                        let caps = caps.block_network();
                        let _ = nono::Sandbox::apply(&caps);
                        Ok(())
                    });
                }
            }
        }

        let output = cmd.output().map_err(|e| {
            TrajLensError::Config(format!(
                "Failed to execute parser script {}: {}",
                script.display(),
                e
            ))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(TrajLensError::Config(format!(
                "Parser script {} exited with {}: {}",
                self.config.parser,
                output.status,
                stderr.chars().take(500).collect::<String>()
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let steps: Vec<StepInfo> = serde_json::from_str(&stdout).map_err(|e| {
            TrajLensError::Config(format!(
                "Parser script {} produced invalid JSON: {}",
                self.config.parser, e
            ))
        })?;

        Ok(steps)
    }

    /// Convert Vec<StepInfo> into internal Trajectory model.
    fn steps_to_trajectory(&self, steps: Vec<StepInfo>) -> Trajectory {
        let mut trajectory_steps: Vec<Step> = Vec::new();

        for step_info in &steps {
            let items = self.step_info_to_items(step_info);

            let ts_start = step_info
                .start_time
                .as_ref()
                .and_then(|s| parse_datetime(s))
                .unwrap_or(NaiveDateTime::MIN);
            let ts_end = step_info
                .end_time
                .as_ref()
                .and_then(|s| parse_datetime(s))
                .unwrap_or(ts_start);

            let line_range = step_info.metrics.line_range.unwrap_or((0, 0));

            trajectory_steps.push(Step {
                step_id: step_info.step_id,
                items,
                timestamp_start: ts_start,
                timestamp_end: ts_end,
                raw_line_range: line_range,
            });
        }

        // Compute total cost
        let total_cost = trajectory_steps
            .iter()
            .flat_map(|s| s.items.iter())
            .fold(Cost::default(), |acc, item| acc.add(&item.cost));

        Trajectory {
            label: String::new(),
            steps: trajectory_steps,
            total_cost,
            outcome: "PARSED".into(),
        }
    }

    /// Convert a single StepInfo into one or more Items.
    fn step_info_to_items(&self, step: &StepInfo) -> Vec<Item> {
        if step.operations.is_empty() {
            return vec![Item {
                category: ItemCategory::Unknown,
                sub_category: None,
                args: Default::default(),
                content: step.content.clone(),
                cost: self.metrics_to_cost(&step.metrics),
            }];
        }

        step.operations
            .iter()
            .map(|op| {
                let category = match op.op_type.as_str() {
                    "tool" => ItemCategory::Action,
                    "user_input" => ItemCategory::Input,
                    "thinking" => ItemCategory::Think,
                    "event" => ItemCategory::Event,
                    "command" => ItemCategory::Input,
                    _ => ItemCategory::Unknown,
                };

                let mut args = std::collections::HashMap::new();
                for arg in &op.args {
                    if let Some((key, value)) = arg.split_once('=') {
                        args.insert(key.to_string(), value.to_string());
                    }
                }

                Item {
                    category,
                    sub_category: op.sub_type.clone(),
                    args,
                    content: step.content.clone(),
                    cost: self.metrics_to_cost(&step.metrics),
                }
            })
            .collect()
    }

    /// Convert StepMetrics into internal Cost struct.
    fn metrics_to_cost(&self, metrics: &StepMetrics) -> Cost {
        Cost {
            input_tokens: metrics.input_token.unwrap_or(0),
            output_tokens: metrics.output_token.unwrap_or(0),
            cache_read_tokens: metrics.cache_read.unwrap_or(0),
            cache_write_tokens: metrics.cache_write.unwrap_or(0),
            dollar_cost: metrics.cost.unwrap_or(0.0),
        }
    }
}

impl Parser for ScriptParser {
    fn parse(&self, _raw_text: &str) -> Trajectory {
        // This method exists for trait compatibility but ScriptParser needs a file path.
        // Use parse_file() instead.
        Trajectory {
            label: String::new(),
            steps: Vec::new(),
            total_cost: Cost::default(),
            outcome: "ERROR: ScriptParser requires parse_file(), not parse()".into(),
        }
    }
}

impl ScriptParser {
    /// Parse a log path (file or directory) using the parser script.
    ///
    /// All steps go into a single trajectory regardless of agent_id. To split
    /// by agent_id (multi-agent / sub-agent contexts), use `parse_file_split`.
    pub fn parse_file(&self, log_path: &Path) -> Result<Trajectory, TrajLensError> {
        let mut steps = self.run_script(log_path)?;
        self.apply_agent_id_rules(&mut steps);
        Ok(self.steps_to_trajectory(steps))
    }

    /// Parse a log path (file or directory) and split steps by agent_id into
    /// multiple trajectories.
    ///
    /// This reflects the principle: "what's in one's context = what's in the log".
    /// - Sub-agents have separate context windows from the main agent.
    /// - Multi-agent workers don't share context.
    /// Each unique agent_id becomes its own Trajectory.
    ///
    /// agent_id is determined in this order:
    ///   1. The parser script may set it explicitly.
    ///   2. Config-defined `agent_id_rules` are applied to steps still missing it.
    ///   3. Steps that remain None go into a trajectory labeled "main".
    ///   The trajectory.label field holds the agent_id.
    pub fn parse_file_split(&self, log_path: &Path) -> Result<Vec<Trajectory>, TrajLensError> {
        let mut steps = self.run_script(log_path)?;
        self.apply_agent_id_rules(&mut steps);

        // Group steps by agent_id
        let mut groups: std::collections::BTreeMap<String, Vec<StepInfo>> =
            std::collections::BTreeMap::new();
        for step in steps {
            let key = step.agent_id.clone().unwrap_or_else(|| "main".to_string());
            groups.entry(key).or_insert_with(Vec::new).push(step);
        }

        // Convert each group into a trajectory
        let mut trajectories = Vec::new();
        for (agent_id, group_steps) in groups {
            let mut traj = self.steps_to_trajectory(group_steps);
            traj.label = agent_id;
            trajectories.push(traj);
        }

        Ok(trajectories)
    }
}

/// Parse a datetime string in common formats.
fn parse_datetime(s: &str) -> Option<NaiveDateTime> {
    let formats = [
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.fZ",
        "%Y-%m-%dT%H:%M:%S",
    ];
    for fmt in &formats {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt);
        }
    }
    None
}

/// Resolve the parsers/scripts/ directory.
///
/// Search order:
/// 1. ./parsers/scripts/ (relative to cwd)
/// 2. Executable directory's ../parsers/scripts/
/// 3. Compile-time CARGO_MANIFEST_DIR/../parsers/scripts/
pub fn find_scripts_dir() -> PathBuf {
    // Try relative to cwd
    let cwd_scripts = PathBuf::from("parsers/scripts");
    if cwd_scripts.exists() {
        return cwd_scripts;
    }

    // Try relative to executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            let exe_scripts = exe_dir.join("../parsers/scripts");
            if exe_scripts.exists() {
                return exe_scripts;
            }
        }
    }

    // Fallback: use the compile-time manifest dir (development mode)
    let manifest_scripts = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../parsers/scripts");
    if manifest_scripts.exists() {
        return manifest_scripts;
    }

    // Last resort
    cwd_scripts
}
