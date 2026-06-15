/// Model specification using "provider/model-name" format.
///
/// Instead of using aliases, users specify provider and model explicitly:
/// - "anthropic/claude-sonnet-4-6"
/// - "bedrock/us.anthropic.claude-sonnet-4-6"
///
/// # Example
///
/// ```rust,no_run
/// use trajlens::llm::model_registry;
///
/// #[tokio::main]
/// async fn main() {
///     // Create client from provider/model string
///     let client = model_registry::create_client("anthropic/claude-sonnet-4-6")
///         .await
///         .unwrap();
///
///     let response = client.complete("System prompt", "User message").await.unwrap();
/// }
/// ```
use super::traits::{LLMClient, LLMConfig, LLMError, LLMResult};

#[cfg(feature = "llm-anthropic")]
use super::anthropic::AnthropicClient;

#[cfg(feature = "llm-bedrock")]
use super::bedrock::BedrockClient;

/// LLM provider type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LLMProvider {
    /// Anthropic API (direct)
    Anthropic,
    /// AWS Bedrock
    Bedrock,
}

/// Model alias definition
#[derive(Debug, Clone)]
pub struct ModelAlias {
    /// Short, friendly name (e.g., "sonnet-4.6")
    pub name: &'static str,

    /// Provider to use
    pub provider: LLMProvider,

    /// Provider-specific model ID
    pub model_id: &'static str,

    /// Human-readable description
    pub description: &'static str,

    /// Recommended for production use
    pub recommended: bool,
}

/// Get the complete model registry
///
/// This is the single source of truth for model aliases.
/// Update this list when new models are released or IDs change.
///
/// VERIFIED WORKING (2026-05-24, us-east-1):
/// - Sonnet 4.6: us.anthropic.claude-sonnet-4-6
/// - Haiku 4.5: us.anthropic.claude-haiku-4-5-20251001-v1:0
/// - Opus 4.6: us.anthropic.claude-opus-4-6-v1
pub fn get_model_registry() -> Vec<ModelAlias> {
    vec![
        // ============ Sonnet 4.6 (Recommended - DEFAULT) ============
        ModelAlias {
            name: "sonnet-4.6",
            provider: LLMProvider::Anthropic,
            model_id: "claude-sonnet-4-6",
            description: "Claude Sonnet 4.6 - Best balance of speed/quality for log parsing",
            recommended: true,
        },
        ModelAlias {
            name: "sonnet",
            provider: LLMProvider::Anthropic,
            model_id: "claude-sonnet-4-6",
            description: "Alias for sonnet-4.6",
            recommended: true,
        },
        // ============ Haiku 4.5 (Fast & Budget-Friendly) ============
        ModelAlias {
            name: "haiku-4.5",
            provider: LLMProvider::Anthropic,
            model_id: "claude-haiku-4-5",
            description: "Claude Haiku 4.5 - Fast, budget-friendly (5x cheaper than Sonnet)",
            recommended: true,
        },
        ModelAlias {
            name: "haiku",
            provider: LLMProvider::Anthropic,
            model_id: "claude-haiku-4-5",
            description: "Alias for haiku-4.5",
            recommended: true,
        },
        // ============ Opus 4.6 (Highest Quality) ============
        ModelAlias {
            name: "opus-4.6",
            provider: LLMProvider::Anthropic,
            model_id: "claude-opus-4-6",
            description: "Claude Opus 4.6 - Highest quality, slowest (3x cost of Sonnet)",
            recommended: false,
        },
        ModelAlias {
            name: "opus",
            provider: LLMProvider::Anthropic,
            model_id: "claude-opus-4-6",
            description: "Alias for opus-4.6",
            recommended: false,
        },
        // ============ Convenience Aliases ============
        ModelAlias {
            name: "default",
            provider: LLMProvider::Anthropic,
            model_id: "claude-sonnet-4-6",
            description: "Default (Sonnet 4.6 via Anthropic API) - Recommended for production",
            recommended: true,
        },
        ModelAlias {
            name: "fast",
            provider: LLMProvider::Anthropic,
            model_id: "claude-haiku-4-5",
            description: "Fast mode (Haiku 4.5 via Anthropic API) - Quick iterations, low cost",
            recommended: true,
        },
        ModelAlias {
            name: "best",
            provider: LLMProvider::Anthropic,
            model_id: "claude-opus-4-6",
            description: "Best quality (Opus 4.6 via Anthropic API) - Highest accuracy, expensive",
            recommended: false,
        },
        // ============ Bedrock Variants (For Testing) ============
        ModelAlias {
            name: "bedrock-sonnet-4.6",
            provider: LLMProvider::Bedrock,
            model_id: "us.anthropic.claude-sonnet-4-6",
            description: "Sonnet 4.6 via AWS Bedrock (testing)",
            recommended: false,
        },
        ModelAlias {
            name: "bedrock-haiku-4.5",
            provider: LLMProvider::Bedrock,
            model_id: "us.anthropic.claude-haiku-4-5-20251001-v1:0",
            description: "Haiku 4.5 via AWS Bedrock (testing)",
            recommended: false,
        },
        ModelAlias {
            name: "bedrock-opus-4.6",
            provider: LLMProvider::Bedrock,
            model_id: "us.anthropic.claude-opus-4-6-v1",
            description: "Opus 4.6 via AWS Bedrock (testing)",
            recommended: false,
        },
    ]
}

/// Look up a model alias in the registry
pub fn lookup_alias(alias: &str) -> Option<ModelAlias> {
    get_model_registry().into_iter().find(|m| m.name == alias)
}

/// Create an LLM client from provider/model-name string.
///
/// # Arguments
/// * `spec` - Provider and model in "provider/model-name" format
///   - "anthropic/claude-sonnet-4-6"
///   - "bedrock/us.anthropic.claude-sonnet-4-6"
///
/// # Returns
/// Configured LLM client ready to use
///
/// # Errors
/// Returns error if provider is unknown or client creation fails
pub async fn create_client(spec: &str) -> LLMResult<Box<dyn LLMClient>> {
    let parts: Vec<&str> = spec.split('/').collect();

    if parts.len() != 2 {
        return Err(LLMError::ConfigError(format!(
            "Invalid model spec '{}'. Expected format: provider/model-name",
            spec
        )));
    }

    let provider = parts[0];
    let model_id = parts[1];

    match provider {
        #[cfg(feature = "llm-anthropic")]
        "anthropic" => {
            use super::anthropic::AnthropicClient;
            use crate::config::get_config;

            let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
                LLMError::ConfigError("ANTHROPIC_API_KEY environment variable not set".to_string())
            })?;

            let config = LLMConfig {
                model: model_id.to_string(),
                max_tokens: get_config().llm.anthropic.max_tokens,
                temperature: get_config().llm.anthropic.temperature as f32,
            };

            Ok(Box::new(AnthropicClient::with_config(api_key, config)))
        }

        #[cfg(feature = "llm-bedrock")]
        "bedrock" => {
            use super::bedrock::BedrockClient;
            use crate::config::get_config;

            let config_data = get_config();
            let region = std::env::var("AWS_REGION")
                .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
                .unwrap_or_else(|_| config_data.llm.bedrock.default_region.clone());

            let config = LLMConfig {
                model: model_id.to_string(),
                max_tokens: config_data.llm.bedrock.max_tokens,
                temperature: config_data.llm.bedrock.temperature as f32,
            };

            let client = BedrockClient::with_config(&region, config).await?;

            Ok(Box::new(client))
        }

        _ => Err(LLMError::ConfigError(format!(
            "Unknown provider '{}'. Supported: anthropic, bedrock",
            provider
        ))),
    }
}

/// Create an LLM client from a model alias (DEPRECATED - use create_client instead)
///
/// # Arguments
///
/// * `alias` - Model alias (e.g., "sonnet-4.6", "haiku", "default")
///
/// # Returns
///
/// A boxed LLM client configured for the specified model
///
/// # Errors
///
/// Returns error if:
/// - Alias not found in registry
/// - Provider feature not enabled (e.g., llm-bedrock)
/// - Client creation fails (e.g., missing credentials)
pub async fn create_client_from_alias(alias: &str) -> LLMResult<Box<dyn LLMClient>> {
    let model = lookup_alias(alias)
        .ok_or_else(|| LLMError::ConfigError(format!("Unknown model alias: {}", alias)))?;

    match model.provider {
        #[cfg(feature = "llm-bedrock")]
        LLMProvider::Bedrock => {
            let config = crate::config::get_config();
            let region = std::env::var("AWS_REGION")
                .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
                .unwrap_or_else(|_| config.llm.bedrock.default_region.clone());

            let llm_config = LLMConfig {
                model: model.model_id.to_string(),
                max_tokens: config.llm.bedrock.max_tokens,
                temperature: config.llm.bedrock.temperature as f32,
            };

            let client = BedrockClient::with_config(&region, llm_config).await?;
            Ok(Box::new(client))
        }

        #[cfg(not(feature = "llm-bedrock"))]
        LLMProvider::Bedrock => Err(LLMError::ConfigError(
            "Bedrock provider not enabled. Build with --features llm-bedrock".to_string(),
        )),

        #[cfg(feature = "llm-anthropic")]
        LLMProvider::Anthropic => {
            let api_key = std::env::var("ANTHROPIC_API_KEY")
                .map_err(|_| LLMError::ConfigError("ANTHROPIC_API_KEY not set".to_string()))?;

            let config = crate::config::get_config();
            let llm_config = LLMConfig {
                model: model.model_id.to_string(),
                max_tokens: config.llm.anthropic.max_tokens,
                temperature: config.llm.anthropic.temperature as f32,
            };

            let client = AnthropicClient::with_config(api_key, llm_config);
            Ok(Box::new(client))
        }

        #[cfg(not(feature = "llm-anthropic"))]
        LLMProvider::Anthropic => Err(LLMError::ConfigError(
            "Anthropic provider not enabled. Build with --features llm-anthropic".to_string(),
        )),
    }
}

/// List all available model aliases
///
/// Returns a formatted string with all aliases, providers, and descriptions
pub fn list_aliases() -> String {
    let mut output = String::new();
    output.push_str("Available model aliases:\n\n");

    let registry = get_model_registry();

    // Group by recommended status
    let recommended: Vec<_> = registry.iter().filter(|m| m.recommended).collect();
    let others: Vec<_> = registry.iter().filter(|m| !m.recommended).collect();

    if !recommended.is_empty() {
        output.push_str("Recommended:\n");
        for model in recommended {
            output.push_str(&format!(
                "  {} ({:?})\n    {}\n\n",
                model.name, model.provider, model.description
            ));
        }
    }

    if !others.is_empty() {
        output.push_str("Other options:\n");
        for model in others {
            output.push_str(&format!(
                "  {} ({:?})\n    {}\n\n",
                model.name, model.provider, model.description
            ));
        }
    }

    output
}

/// Get recommended aliases for common use cases
pub fn get_recommendations() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Production log parsing", "sonnet-4.6"),
        ("Quick iterations / budget", "haiku"),
        ("Highest quality analysis", "opus-4.6"),
        ("Default (best balance)", "default"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_alias() {
        let model = lookup_alias("sonnet-4.6").unwrap();
        assert_eq!(model.name, "sonnet-4.6");
        assert_eq!(model.provider, LLMProvider::Anthropic);
        assert_eq!(model.model_id, "claude-sonnet-4-6");
    }

    #[test]
    fn test_lookup_unknown() {
        assert!(lookup_alias("invalid-model").is_none());
    }

    #[test]
    fn test_convenience_aliases() {
        assert!(lookup_alias("default").is_some());
        assert!(lookup_alias("fast").is_some());
        assert!(lookup_alias("best").is_some());
    }

    #[test]
    fn test_list_aliases() {
        let list = list_aliases();
        assert!(list.contains("sonnet-4.6"));
        assert!(list.contains("haiku"));
        assert!(list.contains("Recommended:"));
    }

    #[test]
    fn test_all_aliases_unique() {
        let registry = get_model_registry();
        let mut names: Vec<&str> = registry.iter().map(|m| m.name).collect();
        names.sort();
        let original_len = names.len();
        names.dedup();
        assert_eq!(names.len(), original_len, "Duplicate alias names found");
    }
}
