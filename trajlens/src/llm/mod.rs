/// LLM integration for Goal Tree and Reasoning DAG construction.
///
/// Provides a generic `LLMClient` trait with implementations for:
/// - Anthropic API (claude-3-5-sonnet, claude-3-5-haiku)
/// - AWS Bedrock (claude-3-5-sonnet via Bedrock)
///
/// # Architecture
///
/// ```
/// LLMClient trait (generic interface)
///    ↓
///    ├─ AnthropicClient (direct API)
///    └─ BedrockClient (AWS Bedrock)
/// ```
///
/// # Usage
///
/// ```rust,no_run
/// use trajlens::llm::{LLMClient, AnthropicClient};
///
/// #[tokio::main]
/// async fn main() {
///     let client = AnthropicClient::new("api-key".to_string());
///     let response = client.complete("system", "user message").await.unwrap();
///     println!("{}", response);
/// }
/// ```
pub mod traits;

#[cfg(feature = "llm-anthropic")]
pub mod anthropic;

#[cfg(feature = "llm-bedrock")]
pub mod bedrock;

// Model alias registry (available with any LLM feature)
#[cfg(feature = "llm")]
pub mod model_registry;

// Re-exports
pub use traits::{LLMClient, LLMConfig, LLMError, LLMResult};

#[cfg(feature = "llm-anthropic")]
pub use anthropic::AnthropicClient;

#[cfg(feature = "llm-bedrock")]
pub use bedrock::BedrockClient;
