use crate::llm::model_registry;
/// LLM-based semantic processor: Extract semantic meaning from raw trajectory data.
///
/// The regex-based parsers only extract structural information (turns, raw content).
/// This processor does the actual understanding:
/// - Categorize items (Think, Action, Input, Event, Unknown)
/// - Extract arguments (file paths, tool names, etc.)
/// - Infer sub-categories (bash_action, read_file, etc.)
///
/// The pre-parsed content from regex parsers is treated as a hint, not ground truth.
/// Everything starts as "unknown" and the LLM processor handles the real work.
///
/// # Example
///
/// ```bash
/// trajlens parse --format claude-code example.log -o trajectory.json --semantic-model anthropic/claude-sonnet-4-6
/// ```
use crate::models::{Item, ItemCategory, Trajectory};

/// Process a trajectory with LLM to extract semantic meaning.
///
/// # Arguments
///
/// * `trajectory` - Raw trajectory (items may be "unknown" category)
/// * `model` - Model specification in "provider/model-name" format
///
/// # Returns
///
/// New trajectory with properly categorized items and extracted arguments
///
/// # Example
/// ```rust,no_run
/// use trajlens::parsing::llm_semantic_processor;
///
/// #[tokio::main]
/// async fn main() {
///     let processed = llm_semantic_processor::process_trajectory(
///         &raw_trajectory,
///         "anthropic/claude-sonnet-4-6"
///     ).await.unwrap();
/// }
/// ```
#[cfg(feature = "llm")]
pub async fn process_trajectory(
    trajectory: &Trajectory,
    model: &str,
) -> crate::error::Result<Trajectory> {
    use crate::error::TrajLensError;

    let llm_client = model_registry::create_client(model)
        .await
        .map_err(TrajLensError::from)?;

    let system_prompt = build_processor_prompt();

    // Process in batches (avoid token limits)
    let batch_size = 50;
    let mut processed_steps = Vec::new();

    for (batch_idx, batch) in trajectory.steps.chunks(batch_size).enumerate() {
        println!(
            "Processing batch {}/{}...",
            batch_idx + 1,
            (trajectory.steps.len() + batch_size - 1) / batch_size
        );

        let user_message = format_batch_message(batch, batch_idx * batch_size);

        // [LLM_CALL: cached] system_prompt is fixed (build_processor_prompt)
        let response = llm_client
            .as_ref()
            .complete(&system_prompt, &user_message)
            .await
            .map_err(TrajLensError::from)?;

        let processed_items = parse_response(&response)?;

        // Merge processed items back into steps
        let mut item_idx = 0;
        for step in batch {
            let mut new_items = Vec::new();
            for _ in 0..step.items.len() {
                if item_idx < processed_items.len() {
                    new_items.push(processed_items[item_idx].clone());
                    item_idx += 1;
                } else {
                    new_items.push(step.items[new_items.len()].clone());
                }
            }

            let mut new_step = step.clone();
            new_step.items = new_items;
            processed_steps.push(new_step);
        }
    }

    Ok(Trajectory {
        label: trajectory.label.clone(),
        steps: processed_steps,
        total_cost: trajectory.total_cost.clone(),
        outcome: trajectory.outcome.clone(),
    })
}

fn build_processor_prompt() -> String {
    r#"You are a semantic processor for agent execution logs.

Your task: Analyze raw log items and extract semantic meaning.

# Item Categories

Classify each item into one of these categories:

- **Think**: LLM reasoning, planning, analysis, thoughts, internal monologue
- **Action**: Tool calls, file operations, commands, API calls (actual operations that change state)
- **Input**: User input, system prompts, external messages
- **Event**: System events, observations, notifications, results from actions
- **Unknown**: Cannot determine (use sparingly)

# Extraction Rules

1. **Category**: Primary classification (Think/Action/Input/Event/Unknown)
2. **Sub-category**: Specific type within category
   - For Action: "bash_action", "read_file", "write_file", "edit_file", "web_fetch", "tool_call"
   - For Think: "reasoning", "planning", "analysis"
   - For Event: "observation", "result", "notification", "error"
   - For Input: "user_message", "system_prompt"

3. **Arguments**: Extracted structured data
   - For file operations: `file_path` (string)
   - For bash: `command` (string)
   - For tool calls: `tool_name` (string), `args` (string)

4. **Content**: Preserve original text, trimmed to max 1024 chars

# Output Format

Return ONLY valid JSON array, no additional text:

```json
[
  {
    "category": "think",
    "sub_category": "reasoning",
    "args": {},
    "content": "Original content here..."
  },
  {
    "category": "action",
    "sub_category": "read_file",
    "args": {
      "file_path": "/path/to/file.rs"
    },
    "content": "Reading file /path/to/file.rs"
  },
  {
    "category": "event",
    "sub_category": "result",
    "args": {},
    "content": "File contents: ..."
  }
]
```

# Guidelines

- Default to "unknown" only if truly ambiguous
- Think items often contain words like: "I think", "Let me", "To", "We should", "Planning"
- Action items contain: "Executing", "Running", "Calling", "Writing", "Reading"
- Event items contain: "Result", "Output", "Error", "Success", "Observation"
- Extract file paths from ANY mention in content (look for /path/to/file patterns)
- Extract commands from bash/shell operations
- Be consistent: similar patterns should get similar categorization"#
        .to_string()
}

fn format_batch_message(batch: &[crate::models::Step], start_step_id: usize) -> String {
    let mut items = Vec::new();

    for (step_idx, step) in batch.iter().enumerate() {
        for (item_idx, item) in step.items.iter().enumerate() {
            items.push(format!(
                r#"Item {} (Step {}, Item {}):
Content: {}
Hint category: {}
Hint sub_category: {}"#,
                items.len(),
                start_step_id + step_idx,
                item_idx,
                if item.content.len() > 500 {
                    format!("{}...", &item.content[..500])
                } else {
                    item.content.clone()
                },
                format!("{:?}", item.category).to_lowercase(),
                item.sub_category.as_ref().unwrap_or(&"none".to_string())
            ));
        }
    }

    format!(
        r#"Process the following {} items and return JSON array with categorization and extracted arguments.

Items to process:
{}

Return ONLY the JSON array, no additional text."#,
        items.len(),
        items.join("\n\n")
    )
}

fn parse_response(response: &str) -> crate::error::Result<Vec<Item>> {
    use crate::error::TrajLensError;
    // Try to extract JSON array from response (may be wrapped in markdown)
    let json_str = if let Some(start) = response.find("```json") {
        let start_content = start + 7; // Skip "```json"
                                       // Skip newline after opening fence
        let start_content = if response[start_content..].starts_with('\n') {
            start_content + 1
        } else if response[start_content..].starts_with("\r\n") {
            start_content + 2
        } else {
            start_content
        };

        // Find closing fence
        if let Some(end_offset) = response[start_content..].find("```") {
            let end_content = start_content + end_offset;
            response[start_content..end_content].trim()
        } else {
            response
        }
    } else if let Some(start) = response.find('[') {
        // Find matching closing bracket
        &response[start..]
    } else {
        response
    };

    let parsed: serde_json::Value = serde_json::from_str(json_str)
        .map_err(|e| TrajLensError::InvalidResponse(format!("Failed to parse JSON: {}", e)))?;

    let array = parsed
        .as_array()
        .ok_or_else(|| TrajLensError::InvalidResponse("Response is not a JSON array".into()))?;

    let mut items = Vec::new();
    for item_json in array {
        let category_str = item_json["category"]
            .as_str()
            .ok_or_else(|| TrajLensError::InvalidResponse("Missing 'category' field".into()))?;

        let category = match category_str.to_lowercase().as_str() {
            "think" => ItemCategory::Think,
            "action" => ItemCategory::Action,
            "input" => ItemCategory::Input,
            "event" => ItemCategory::Event,
            _ => ItemCategory::Unknown,
        };

        let sub_category = item_json["sub_category"].as_str().map(|s| s.to_string());

        let content = item_json["content"].as_str().unwrap_or("").to_string();

        let mut args = std::collections::HashMap::new();
        if let Some(args_obj) = item_json["args"].as_object() {
            for (key, value) in args_obj {
                if let Some(val_str) = value.as_str() {
                    args.insert(key.clone(), val_str.to_string());
                }
            }
        }

        items.push(Item {
            category,
            sub_category,
            args,
            content,
            cost: crate::models::Cost::default(), // Will be re-estimated later
        });
    }

    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_response_basic() {
        let response = r#"[
            {
                "category": "think",
                "sub_category": "reasoning",
                "args": {},
                "content": "Let me analyze this"
            },
            {
                "category": "action",
                "sub_category": "read_file",
                "args": {
                    "file_path": "/test/file.rs"
                },
                "content": "Reading file"
            }
        ]"#;

        let items = parse_response(response).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].category, ItemCategory::Think);
        assert_eq!(items[1].category, ItemCategory::Action);
        assert_eq!(
            items[1].args.get("file_path"),
            Some(&"/test/file.rs".to_string())
        );
    }

    #[test]
    fn test_parse_response_markdown() {
        let response = r#"Here's the result:

```json
[
    {
        "category": "action",
        "sub_category": "bash_action",
        "args": {
            "command": "ls -la"
        },
        "content": "Listing files"
    }
]
```"#;

        let items = parse_response(response).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].category, ItemCategory::Action);
    }
}
