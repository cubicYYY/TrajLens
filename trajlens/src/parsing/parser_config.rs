/// Parser configuration loaded from TOML files.
///
/// New design (universal_parser.md):
/// - Each config defines a `log_type_name`, `fingerprint` patterns, and a `parser` script path.
/// - The parser script handles ALL parsing logic (divide + extract in one step).
/// - Script reads a log file path from argv[1], outputs Vec<StepInfo> as JSON to stdout.
/// - LLM patching fixes <parse_failed> sections after script execution.
use serde::{Deserialize, Serialize};

/// A parser configuration entry in the parser zoo.
///
/// Each entry identifies a log type and points to a script that can parse it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParserConfig {
    /// Unique name identifying this log type. E.g. "claude_code_history_jsonl".
    pub log_type_name: String,

    /// Regex patterns that identify this log type.
    /// ALL patterns must match for this parser to be selected.
    pub fingerprint: Vec<String>,

    /// Path to the parser script (relative to parsers/scripts/ directory).
    /// The script receives the log file path as argv[1] and outputs JSON to stdout.
    pub parser: String,

    /// Rules for extracting `agent_id` from each step's content/args.
    ///
    /// Treated like fingerprints: LLM suggests them from sample logs, user
    /// confirms/edits, they're stored here, and refined as new logs arrive.
    ///
    /// Applied AFTER the parser script runs:
    /// - For each step where `agent_id` is None, test rules in order.
    /// - If a rule matches, assign its `assign` template (with capture group
    ///   substitution) as the agent_id.
    /// - If no rule matches, leave agent_id as None (LLM patcher will fill in).
    /// - Steps where the script already set agent_id are NOT overwritten.
    #[serde(default)]
    pub agent_id_rules: Vec<AgentIdRule>,
}

/// One rule for extracting agent_id from a step.
///
/// The `pattern` regex is matched against the step's content + concatenated
/// operation args. If it matches, `assign` is used as the agent_id, with
/// capture group references like `$1`, `$2`, etc. substituted.
///
/// Examples:
/// - `pattern = "creator:\\s*Human"`, `assign = "main"` → static assignment
/// - `pattern = "worker:\\s*(\\S+)"`, `assign = "$1"` → dynamic from match
/// - `pattern = "\\[sub-agent\\s+(\\d+)\\]"`, `assign = "sub$1"` → templated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentIdRule {
    /// Human-readable description of what this rule captures.
    #[serde(default)]
    pub description: String,

    /// Regex pattern tested against each step's text (content + args).
    pub pattern: String,

    /// Template for the assigned agent_id. Supports `$1`, `$2`, ... for
    /// capture groups from `pattern`. Use static string for fixed assignment.
    pub assign: String,
}

/// Output of a parser script: one step in the trajectory.
///
/// This is the schema that every parser script must produce as JSON.
/// Fields that cannot be found should be None (null in JSON).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepInfo {
    /// Sequential step number.
    pub step_id: usize,

    /// Identifier of the agent (or sub-agent) that owns this step.
    ///
    /// Why this matters: a step's content represents what was IN that agent's
    /// context window at execution time. Different agents have different windows:
    /// - Main agent only sees sub-agent RETURN values, not their internals.
    /// - Multi-agent workers don't share context at all.
    /// Each unique agent_id forms an independent trajectory.
    ///
    /// Conventions:
    /// - "main" or None: the primary/orchestrator agent
    /// - "sub1", "sub2", ...: sequential sub-agents spawned by the main agent
    /// - "<job_name>" or "<worker_name>": named workers in multi-agent systems
    ///
    /// If the parser cannot determine this from log markers, set to None and
    /// the LLM patcher will infer it semantically.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,

    /// Original content of this step, with <parse_failed> tags wrapping
    /// any substrings the parser could not classify.
    pub content: String,

    /// ISO 8601 datetime string (e.g. "2026-05-26 03:05:00") or null.
    pub start_time: Option<String>,

    /// ISO 8601 datetime string or null.
    pub end_time: Option<String>,

    /// LLM and execution metrics for this step.
    pub metrics: StepMetrics,

    /// Operations performed in this step.
    pub operations: Vec<OperationInfo>,
}

/// Metrics associated with a single step.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct StepMetrics {
    pub input_token: Option<i64>,
    pub output_token: Option<i64>,
    pub cache_read: Option<i64>,
    pub cache_write: Option<i64>,

    /// Wall-clock execution time in seconds.
    pub time: Option<f64>,

    /// Dollar cost of this step.
    pub cost: Option<f64>,

    /// Line range [start, end) in the original log file.
    /// Convention: 0-based, end-EXCLUSIVE (Python-style slicing).
    /// So `[10, 15]` covers lines at indices 10, 11, 12, 13, 14 (5 lines).
    /// This avoids off-by-one bugs where step boundaries touch each other.
    pub line_range: Option<(usize, usize)>,
}

/// A single operation within a step.
///
/// Examples: tool("edit"), tool("bash"), user_input, thinking, event("auto_compact").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationInfo {
    /// Operation type. E.g. "tool", "user_input", "thinking", "event", "unknown".
    #[serde(rename = "type")]
    pub op_type: String,

    /// Sub-type qualifier. E.g. "edit", "bash", "read_file", "auto_compact".
    pub sub_type: Option<String>,

    /// Positional arguments or key=value strings relevant to this operation.
    pub args: Vec<String>,
}

/// Marker on steps indicating problematic parsing that needs LLM attention.
///
/// A step is "problematic" if its content contains `<parse_failed>` tags,
/// meaning some portion could not be classified by the parser script.
impl StepInfo {
    /// Returns true if this step has content that failed to parse.
    pub fn has_parse_failures(&self) -> bool {
        self.content.contains("<parse_failed>")
    }
}

impl ParserConfig {
    /// Load parser config from TOML string.
    pub fn from_toml(content: &str) -> Result<Self, toml::de::Error> {
        toml::from_str(content)
    }

    /// Load parser config from file path.
    pub fn from_file(path: &std::path::Path) -> std::io::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Self::from_toml(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_new_config_format() {
        let toml = r#"
log_type_name = "test_format"
fingerprint = ["^\\[TEST\\]", "test_marker"]
parser = "test_parser.py"
        "#;
        let config = ParserConfig::from_toml(toml).unwrap();
        assert_eq!(config.log_type_name, "test_format");
        assert_eq!(config.fingerprint.len(), 2);
        assert_eq!(config.parser, "test_parser.py");
    }

    #[test]
    fn test_step_info_parse_failure_detection() {
        let step = StepInfo {
            step_id: 0,
            agent_id: None,
            content: "some good content <parse_failed>broken part</parse_failed> more content"
                .into(),
            start_time: None,
            end_time: None,
            metrics: StepMetrics::default(),
            operations: vec![],
        };
        assert!(step.has_parse_failures());

        let clean_step = StepInfo {
            step_id: 1,
            agent_id: None,
            content: "all good content".into(),
            start_time: None,
            end_time: None,
            metrics: StepMetrics::default(),
            operations: vec![],
        };
        assert!(!clean_step.has_parse_failures());
    }
}
