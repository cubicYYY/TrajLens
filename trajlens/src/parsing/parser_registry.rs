/// Parser registry: loads parser configs and matches logs to parsers.
///
/// The registry loads TOML configs from:
/// 1. Built-in configs (embedded at compile time from parsers/configs/)
/// 2. User-custom configs from ~/.config/trajlens/parsers/configs/
///
/// Matching: for each config, ALL fingerprint regexes must match the log sample.
/// If multiple configs match, the one with the most fingerprint patterns wins.
use super::parser_config::ParserConfig;
use crate::error::TrajLensError;
use regex::Regex;
use std::collections::HashMap;
use std::path::PathBuf;

/// Registry of all known parser configurations.
pub struct ParserRegistry {
    configs: HashMap<String, ParserConfig>,
}

impl ParserRegistry {
    /// Create empty registry.
    pub fn new() -> Self {
        Self {
            configs: HashMap::new(),
        }
    }

    /// Load default registry with built-in parser configs.
    pub fn load_default() -> Result<Self, TrajLensError> {
        let mut registry = Self::new();

        // Load built-in parser configs (embedded at compile time)
        registry.register_builtin(include_str!(
            "../../../parsers/configs/claude_code_history_jsonl.toml"
        ))?;
        registry.register_builtin(include_str!(
            "../../../parsers/configs/cairn_project_yaml.toml"
        ))?;
        registry.register_builtin(include_str!("../../../parsers/configs/cyberagent_log.toml"))?;
        registry.register_builtin(include_str!(
            "../../../parsers/configs/claude_code_text.toml"
        ))?;
        registry.register_builtin(include_str!("../../../parsers/configs/pocgen_text.toml"))?;

        // Load any additional configs from parsers/configs/ at runtime (dynamic parsers)
        let dynamic_dir = PathBuf::from("parsers/configs");
        if dynamic_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dynamic_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                        continue;
                    }
                    match ParserConfig::from_file(&path) {
                        Ok(config) => {
                            if !registry.configs.contains_key(&config.log_type_name) {
                                registry
                                    .configs
                                    .insert(config.log_type_name.clone(), config);
                            }
                        }
                        Err(_) => {}
                    }
                }
            }
        }

        // Load user-custom parser configs
        if let Err(e) = registry.load_user_configs() {
            eprintln!("Warning: Could not load user parser configs: {}", e);
        }

        Ok(registry)
    }

    /// Register a parser config from embedded TOML string.
    fn register_builtin(&mut self, toml_content: &str) -> Result<(), TrajLensError> {
        let config = ParserConfig::from_toml(toml_content).map_err(|e| {
            TrajLensError::Config(format!("Failed to parse built-in parser config: {}", e))
        })?;
        self.configs.insert(config.log_type_name.clone(), config);
        Ok(())
    }

    /// Load user-custom parser configs from ~/.config/trajlens/parsers/configs/.
    fn load_user_configs(&mut self) -> Result<(), TrajLensError> {
        let config_dir = self.get_user_configs_dir()?;
        if !config_dir.exists() {
            return Ok(());
        }

        let entries = std::fs::read_dir(&config_dir)?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("toml") {
                continue;
            }
            match ParserConfig::from_file(&path) {
                Ok(config) => {
                    self.configs.insert(config.log_type_name.clone(), config);
                }
                Err(e) => {
                    eprintln!(
                        "Warning: Failed to load parser config {}: {}",
                        path.display(),
                        e
                    );
                }
            }
        }
        Ok(())
    }

    /// Get path to user parser configs directory.
    fn get_user_configs_dir(&self) -> Result<PathBuf, TrajLensError> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| TrajLensError::Config("Could not determine config directory".into()))?;
        Ok(config_dir.join("trajlens").join("parsers").join("configs"))
    }

    /// Match a log against all registered fingerprints.
    ///
    /// Returns the log_type_name of the best matching parser, or None.
    /// ALL fingerprint patterns must match for a config to be considered.
    /// Ties broken by number of fingerprint patterns (more specific wins).
    pub fn detect_format(&self, log: &str) -> Option<String> {
        // Sample first 2000 lines for detection
        let sample: String = log.lines().take(2000).collect::<Vec<_>>().join("\n");

        let mut best: Option<(String, usize)> = None;

        for (name, config) in &self.configs {
            if self.all_fingerprints_match(&sample, &config.fingerprint) {
                let score = config.fingerprint.len();
                match &best {
                    None => best = Some((name.clone(), score)),
                    Some((_, prev_score)) if score > *prev_score => {
                        best = Some((name.clone(), score));
                    }
                    _ => {}
                }
            }
        }

        best.map(|(name, _)| name)
    }

    /// Check if ALL fingerprint patterns match the sample.
    fn all_fingerprints_match(&self, sample: &str, patterns: &[String]) -> bool {
        patterns.iter().all(|pattern| {
            let multiline_pattern = format!("(?m){}", pattern);
            Regex::new(&multiline_pattern)
                .ok()
                .map(|re| re.is_match(sample))
                .unwrap_or(false)
        })
    }

    /// Match a log path (file or directory) against all registered fingerprints.
    ///
    /// For files: reads first 2000 lines and tests fingerprints.
    /// For directories: collects samples from representative files in the tree
    /// (first few .json, .log, .yaml files found) and tests fingerprints against
    /// the combined content.
    pub fn detect_format_from_path(&self, path: &std::path::Path) -> Option<String> {
        let sample = if path.is_dir() {
            self.read_dir_sample(path)
        } else {
            std::fs::read_to_string(path)
                .ok()
                .map(|c| c.lines().take(2000).collect::<Vec<_>>().join("\n"))
                .unwrap_or_default()
        };

        if sample.is_empty() {
            return None;
        }

        self.detect_format(&sample)
    }

    /// Collect a fingerprinting sample from a directory by reading a few
    /// representative files (prioritizes .json and .log files).
    fn read_dir_sample(&self, dir: &std::path::Path) -> String {
        let mut samples = Vec::new();
        let mut visited = 0;

        fn walk(dir: &std::path::Path, samples: &mut Vec<String>, visited: &mut usize) {
            if *visited >= 5 {
                return;
            }
            let entries = match std::fs::read_dir(dir) {
                Ok(e) => e,
                Err(_) => return,
            };
            let mut entries: Vec<_> = entries.flatten().collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                if *visited >= 5 {
                    break;
                }
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, samples, visited);
                } else {
                    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                    if matches!(ext, "json" | "log" | "yaml" | "yml" | "jsonl" | "txt") {
                        if let Ok(content) = std::fs::read_to_string(&path) {
                            let head: String =
                                content.lines().take(200).collect::<Vec<_>>().join("\n");
                            samples.push(head);
                            *visited += 1;
                        }
                    }
                }
            }
        }

        walk(dir, &mut samples, &mut visited);
        samples.join("\n\n--- NEXT FILE ---\n\n")
    }

    /// Get parser config by log_type_name.
    pub fn get(&self, name: &str) -> Option<&ParserConfig> {
        self.configs.get(name)
    }

    /// List all registered log type names.
    pub fn list_formats(&self) -> Vec<String> {
        let mut names: Vec<String> = self.configs.keys().cloned().collect();
        names.sort();
        names
    }

    /// Check if a format is registered.
    pub fn has_format(&self, name: &str) -> bool {
        self.configs.contains_key(name)
    }
}

impl Default for ParserRegistry {
    fn default() -> Self {
        Self::load_default().unwrap_or_else(|e| {
            eprintln!("Warning: Failed to load default parser configs: {}", e);
            Self::new()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_builtin_configs() {
        let registry = ParserRegistry::load_default().unwrap();
        assert!(registry.has_format("claude_code_history_jsonl"));
        assert!(registry.has_format("cairn_project_yaml"));
        assert!(registry.has_format("cyberagent_log"));
    }

    #[test]
    fn test_detect_jsonl() {
        let registry = ParserRegistry::load_default().unwrap();
        let sample = r#"{"display":"/model","pastedContents":{},"timestamp":1779550689229,"sessionId":"ed8bf593-89c1-4b1a-b495-b0583dfbdfdd"}"#;
        let detected = registry.detect_format(sample);
        assert_eq!(detected, Some("claude_code_history_jsonl".to_string()));
    }

    #[test]
    fn test_detect_cyberagent() {
        let registry = ParserRegistry::load_default().unwrap();
        let sample = "2026-05-26 03:05:00 [INFO] [cyberagent.orchestrator] Phase 1\n\
                      2026-05-26 03:05:01 [INFO] [cyberagent.vector.rce] test\n\
                      Score: 1 | RCE=False non-RCE=True | elapsed=10s remaining=100s\n";
        let detected = registry.detect_format(sample);
        assert_eq!(detected, Some("cyberagent_log".to_string()));
    }

    #[test]
    fn test_no_match_returns_none() {
        let registry = ParserRegistry::load_default().unwrap();
        let sample = "this is just random text\nwith nothing matching\n";
        assert_eq!(registry.detect_format(sample), None);
    }
}
