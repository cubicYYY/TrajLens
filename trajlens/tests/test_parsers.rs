/// Integration tests for the new script-based parser system.
/// Tests fingerprint matching and parser script execution against sample logs.
use std::fs;
use std::path::PathBuf;

use trajlens::parsing::parser_registry::ParserRegistry;
use trajlens::parsing::script_runner::{find_scripts_dir, ScriptParser};

fn sampled_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("example_trajectories")
        .join("Sampled")
}

#[test]
fn test_detect_claude_code_history_jsonl() {
    let registry = ParserRegistry::load_default().unwrap();
    let text = fs::read_to_string(sampled_dir().join("history.jsonl")).unwrap();
    let format = registry.detect_format(&text);
    assert_eq!(format, Some("claude_code_history_jsonl".to_string()));
}

#[test]
fn test_detect_cairn_project_yaml() {
    let registry = ParserRegistry::load_default().unwrap();
    let text = fs::read_to_string(sampled_dir().join("2bird2can.yaml")).unwrap();
    let format = registry.detect_format(&text);
    assert_eq!(format, Some("cairn_project_yaml".to_string()));
}

#[test]
fn test_detect_cyberagent_log() {
    let registry = ParserRegistry::load_default().unwrap();
    let text = fs::read_to_string(sampled_dir().join("run_20260526_030500(1).log")).unwrap();
    let format = registry.detect_format(&text);
    assert_eq!(format, Some("cyberagent_log".to_string()));
}

#[test]
fn test_parse_jsonl_via_script() {
    let registry = ParserRegistry::load_default().unwrap();
    let config = registry.get("claude_code_history_jsonl").unwrap().clone();
    let scripts_dir = find_scripts_dir();
    let parser = ScriptParser::new(config, scripts_dir);

    let log_path = sampled_dir().join("history.jsonl");
    let traj = parser.parse_file(&log_path).unwrap();

    assert_eq!(
        traj.steps.len(),
        191,
        "Expected 191 steps (one per JSONL line)"
    );
    assert!(!traj.steps[0].items.is_empty());
}

#[test]
fn test_parse_yaml_via_script() {
    let registry = ParserRegistry::load_default().unwrap();
    let config = registry.get("cairn_project_yaml").unwrap().clone();
    let scripts_dir = find_scripts_dir();
    let parser = ScriptParser::new(config, scripts_dir);

    let log_path = sampled_dir().join("2bird2can.yaml");
    let traj = parser.parse_file(&log_path).unwrap();

    assert_eq!(
        traj.steps.len(),
        24,
        "Expected 24 steps (1 meta + 23 intents)"
    );
}

#[test]
fn test_parse_cyberagent_via_script() {
    let registry = ParserRegistry::load_default().unwrap();
    let config = registry.get("cyberagent_log").unwrap().clone();
    let scripts_dir = find_scripts_dir();
    let parser = ScriptParser::new(config, scripts_dir);

    let log_path = sampled_dir().join("run_20260526_030500(1).log");
    let traj = parser.parse_file(&log_path).unwrap();

    assert_eq!(
        traj.steps.len(),
        79,
        "Expected 79 orchestrator action steps"
    );
}

#[test]
#[ignore] // requires generated parser; run with --ignored after generate-parser
fn test_pocgen_multi_action_split() {
    let registry = ParserRegistry::load_default().unwrap();
    let config = match registry.get("pocgen_multi_action") {
        Some(c) => c.clone(),
        None => return, // parser not generated yet
    };
    let scripts_dir = find_scripts_dir();
    let parser = ScriptParser::new(config, scripts_dir);

    let log_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("example_trajectories")
        .join("pocgen")
        .join("1.txt");
    if !log_path.exists() {
        return;
    }

    let trajectories = parser.parse_file_split(&log_path).unwrap();
    eprintln!("Trajectories produced: {}", trajectories.len());
    for t in &trajectories {
        eprintln!("  agent='{}' steps={}", t.label, t.steps.len());
    }
    assert!(
        trajectories.len() >= 5,
        "expected >=5 agents, got {}",
        trajectories.len()
    );
}

#[test]
fn test_cairn_agent_id_rules_split_workers() {
    // The Cairn YAML config defines agent_id_rules that map:
    //   creator=Human         → main
    //   creator=dispatcher    → main
    //   worker=<name>         → <name>
    // With these rules, parse_file_split should produce >=2 trajectories
    // (at least "main" and one worker like "pi-GPT5.5").
    let registry = ParserRegistry::load_default().unwrap();
    let config = registry.get("cairn_project_yaml").unwrap().clone();
    let scripts_dir = find_scripts_dir();
    let parser = ScriptParser::new(config, scripts_dir);

    let log_path = sampled_dir().join("2bird2can.yaml");
    let trajectories = parser.parse_file_split(&log_path).unwrap();

    let labels: Vec<&str> = trajectories.iter().map(|t| t.label.as_str()).collect();
    assert!(
        labels.contains(&"main"),
        "expected 'main' trajectory; got {:?}",
        labels
    );
    assert!(
        labels.iter().any(|l| l.contains("pi-")),
        "expected at least one 'pi-*' worker trajectory; got {:?}",
        labels
    );
    assert!(
        trajectories.len() >= 2,
        "expected multi-agent split (>=2 trajectories), got {}",
        trajectories.len()
    );
}

#[test]
fn test_no_cross_detection() {
    let registry = ParserRegistry::load_default().unwrap();

    // JSONL should NOT match cyberagent or cairn patterns
    let jsonl = fs::read_to_string(sampled_dir().join("history.jsonl")).unwrap();
    let detected = registry.detect_format(&jsonl).unwrap();
    assert_eq!(detected, "claude_code_history_jsonl");

    // Random text should not match anything
    let random = "hello world\nthis is just text\nnothing special here\n";
    assert_eq!(registry.detect_format(random), None);
}
