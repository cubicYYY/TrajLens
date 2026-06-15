/// Log parsing module.
///
/// Architecture (from docs/universal_parser.md):
///   1. Matching: fingerprint regex patterns identify the log type
///   2. Parsing: a parser script (Python) does all extraction, outputs Vec<StepInfo> JSON
///   3. LLM Patching: fixes <parse_failed> sections in batch (optional)
///
/// NO format-specific code lives in this Rust module. All format knowledge is in
/// external parser scripts (parsers/scripts/) and configs (parsers/configs/).
pub mod cost_estimator;
pub mod parser_config;
pub mod parser_registry;
pub mod script_runner;

// LLM-based components (require llm feature)
#[cfg(feature = "llm")]
pub mod llm_semantic_processor;

#[cfg(feature = "llm")]
pub mod parser_agents;

// Legacy parser generator (superseded by parser_agents)
#[cfg(feature = "llm")]
pub mod parser_generator;

use crate::models::{Cost, Item, ItemCategory, Step, Trajectory};
use chrono::NaiveDateTime;

/// Parser trait. Implementations parse raw log text into a Trajectory.
/// Content that cannot be classified deterministically should be wrapped
/// in an Item with category=Unknown. The LLM fixer handles the rest.
pub trait Parser {
    fn parse(&self, raw_text: &str) -> Trajectory;
}

/// Default parser that performs no actual parsing.
/// Wraps the entire log as a single Step with a single Unknown Item.
pub struct NoopParser;

impl Parser for NoopParser {
    fn parse(&self, raw_text: &str) -> Trajectory {
        if raw_text.trim().is_empty() {
            return Trajectory {
                label: String::new(),
                steps: Vec::new(),
                total_cost: Cost::default(),
                outcome: "UNKNOWN".into(),
            };
        }

        let line_count = raw_text.chars().filter(|&c| c == '\n').count();
        let item = Item {
            category: ItemCategory::Unknown,
            sub_category: None,
            args: Default::default(),
            content: raw_text.to_string(),
            cost: Cost::default(),
        };
        let step = Step {
            step_id: 0,
            items: vec![item],
            timestamp_start: NaiveDateTime::MIN,
            timestamp_end: NaiveDateTime::MIN,
            raw_line_range: (0, line_count),
        };
        Trajectory {
            label: String::new(),
            steps: vec![step],
            total_cost: Cost::default(),
            outcome: "UNKNOWN".into(),
        }
    }
}

/// Auto-detect the log format using the parser registry.
///
/// Tests all registered fingerprints and returns the matching log_type_name.
/// Returns None if no format matches.
pub fn detect_format(log: &str) -> Option<String> {
    let registry = parser_registry::ParserRegistry::load_default().ok()?;
    registry.detect_format(log)
}
