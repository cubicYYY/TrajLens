/// LLM-based parser generator: Analyze example logs and generate a parser config + script.
///
/// This module uses LLMs to automatically create:
/// 1. A TOML config (log_type_name, fingerprint, parser script path)
/// 2. A Python parser script that extracts Vec<StepInfo> from the log
///
/// # Example
///
/// ```bash
/// trajlens generate-parser example.log -o parsers/configs/new_format.toml \
///   --name new_format --model anthropic/claude-sonnet-4-6
/// ```

/// Generate a parser config TOML and parser script from example log content.
///
/// Returns (config_toml, script_content) tuple.
#[cfg(feature = "llm")]
pub async fn generate_parser_config(
    log_sample: &str,
    format_name: &str,
    model: &str,
) -> Result<String, String> {
    use crate::llm::model_registry;
    let llm_client = model_registry::create_client(model)
        .await
        .map_err(|e| format!("Failed to create LLM client: {}", e))?;
    let system_prompt = build_generator_prompt();
    let user_message = format_user_message(log_sample, format_name);

    // [LLM_CALL: cached] system_prompt is fixed (build_generator_prompt)
    let response = llm_client
        .complete(&system_prompt, &user_message)
        .await
        .map_err(|e| format!("LLM error: {}", e))?;

    // Extract TOML from response
    let toml_config = extract_toml_from_response(&response)?;
    validate_generated_config(&toml_config)?;

    Ok(toml_config)
}

fn build_generator_prompt() -> String {
    r#"You are an expert log parser configuration generator for TrajLens.

Your task: Analyze example log content and generate:
1. A TOML config file (fingerprint + parser script reference)
2. A Python parser script

# TOML Config Format

```toml
log_type_name = "format_name"
fingerprint = ["regex_pattern_1", "regex_pattern_2", "regex_pattern_3"]
parser = "format_name.py"
```

- `fingerprint`: ALL patterns must match for this config to be selected. Use 3-5 patterns.
- `parser`: filename of the Python script in parsers/scripts/

# Parser Script Requirements

The Python script:
- Receives log file path as sys.argv[1]
- Outputs JSON array of StepInfo objects to stdout
- Each StepInfo has: step_id, content, start_time, end_time, metrics, operations[]
- metrics: {input_token, output_token, cache_read, cache_write, time, cost, line_range}
- operations[]: [{type, sub_type, args}]
- Operation types: "tool", "user_input", "thinking", "event", "unknown"
- Uncovered content must be wrapped: <parse_failed>...</parse_failed>

# Output Format

Return a TOML config wrapped in ```toml ... ``` followed by a Python script in ```python ... ```"#
        .to_string()
}

fn format_user_message(log_sample: &str, format_name: &str) -> String {
    let sample = if log_sample.len() > 3000 {
        &log_sample[..3000]
    } else {
        log_sample
    };

    format!(
        "Generate a parser for the following log format.\n\n\
         Format name: {}\n\n\
         Log sample:\n```\n{}\n```\n\n\
         Generate the TOML config and Python parser script.",
        format_name, sample
    )
}

fn extract_toml_from_response(response: &str) -> Result<String, String> {
    if let Some(start) = response.find("```toml") {
        let start_content = start + 7;
        let start_content = if response[start_content..].starts_with('\n') {
            start_content + 1
        } else {
            start_content
        };
        if let Some(end_offset) = response[start_content..].find("```") {
            return Ok(response[start_content..start_content + end_offset]
                .trim()
                .to_string());
        }
    }

    if response.trim().starts_with("log_type_name") {
        return Ok(response.trim().to_string());
    }

    Err("Could not extract TOML config from LLM response.".to_string())
}

fn validate_generated_config(toml_content: &str) -> Result<(), String> {
    use super::parser_config::ParserConfig;
    ParserConfig::from_toml(toml_content)
        .map_err(|e| format!("Generated TOML is invalid: {}", e))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_toml_from_markdown() {
        let response = "Here's the config:\n\n```toml\nlog_type_name = \"test\"\nfingerprint = [\"a\"]\nparser = \"test.py\"\n```\n";
        let result = extract_toml_from_response(response).unwrap();
        assert!(result.contains("log_type_name"));
    }

    #[test]
    fn test_validate_valid_config() {
        let toml = "log_type_name = \"test\"\nfingerprint = [\"test\"]\nparser = \"test.py\"\n";
        assert!(validate_generated_config(toml).is_ok());
    }
}
