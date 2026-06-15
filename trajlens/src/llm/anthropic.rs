/// Anthropic API client implementation.
///
/// Direct integration with Anthropic's Messages API using reqwest.
///
/// # Authentication
///
/// Requires `ANTHROPIC_API_KEY` environment variable or explicit API key.
///
/// # Example
///
/// ```rust,no_run
/// use trajlens::llm::{LLMClient, AnthropicClient, LLMConfig};
///
/// #[tokio::main]
/// async fn main() {
///     let client = AnthropicClient::from_env().unwrap();
///     let response = client.complete(
///         "You are a helpful assistant.",
///         "What is Rust?"
///     ).await.unwrap();
///     println!("{}", response);
/// }
/// ```
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderValue, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use super::traits::{LLMClient, LLMConfig, LLMError, LLMResult};
use crate::config::get_config;

/// Anthropic API client.
pub struct AnthropicClient {
    api_key: String,
    config: LLMConfig,
    client: reqwest::Client,
}

impl AnthropicClient {
    /// Create a new Anthropic client with an explicit API key.
    pub fn new(api_key: String) -> Self {
        Self::with_config(api_key, LLMConfig::default())
    }

    /// Create a new Anthropic client with custom configuration.
    pub fn with_config(api_key: String, config: LLMConfig) -> Self {
        let client = reqwest::Client::new();
        Self {
            api_key,
            config,
            client,
        }
    }

    /// Create a client from the `ANTHROPIC_API_KEY` environment variable.
    pub fn from_env() -> LLMResult<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            LLMError::ConfigError("ANTHROPIC_API_KEY environment variable not set".to_string())
        })?;
        Ok(Self::new(api_key))
    }

    /// Create a client from environment with custom config.
    pub fn from_env_with_config(config: LLMConfig) -> LLMResult<Self> {
        let api_key = std::env::var("ANTHROPIC_API_KEY").map_err(|_| {
            LLMError::ConfigError("ANTHROPIC_API_KEY environment variable not set".to_string())
        })?;
        Ok(Self::with_config(api_key, config))
    }
}

#[async_trait]
impl LLMClient for AnthropicClient {
    async fn complete(&self, system_prompt: &str, user_message: &str) -> LLMResult<String> {
        // Use prompt caching for system prompt (typically contains examples/context)
        let system_blocks = vec![SystemBlock::Text {
            block_type: "text".to_string(),
            text: system_prompt.to_string(),
            cache_control: Some(CacheControl {
                cache_type: "ephemeral".to_string(),
            }),
        }];

        let request = AnthropicRequest {
            model: self.config.model.clone(),
            max_tokens: self.config.max_tokens,
            temperature: self.config.temperature,
            system: system_blocks,
            messages: vec![Message {
                role: "user".to_string(),
                content: user_message.to_string(),
            }],
        };

        let mut headers = HeaderMap::new();
        headers.insert(
            "x-api-key",
            HeaderValue::from_str(&self.api_key)
                .map_err(|e| LLMError::ConfigError(format!("Invalid API key: {}", e)))?,
        );
        let app_config = get_config();
        headers.insert(
            "anthropic-version",
            HeaderValue::from_str(&app_config.llm.anthropic.api_version)
                .map_err(|e| LLMError::ConfigError(format!("Invalid API version: {}", e)))?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        // Enable prompt caching — system prompts are fixed per call site so the
        // cache_control: {type: "ephemeral"} on system blocks will trigger cache
        // hits across consecutive requests with the same system prompt content.
        headers.insert(
            "anthropic-beta",
            HeaderValue::from_static("prompt-caching-2024-07-31"),
        );

        let response = self
            .client
            .post(&app_config.llm.anthropic.api_url)
            .headers(headers)
            .json(&request)
            .send()
            .await
            .map_err(|e| LLMError::NetworkError(format!("Request failed: {}", e)))?;

        let status = response.status();
        if status == 401 {
            return Err(LLMError::AuthError(
                "Invalid API key or authentication failed".to_string(),
            ));
        } else if status == 429 {
            return Err(LLMError::RateLimitError("Rate limit exceeded".to_string()));
        } else if !status.is_success() {
            let error_text = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(LLMError::NetworkError(format!(
                "API error {}: {}",
                status, error_text
            )));
        }

        let anthropic_response: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| LLMError::InvalidResponse(format!("Failed to parse response: {}", e)))?;

        // Extract text from first content block
        anthropic_response
            .content
            .first()
            .and_then(|block| match block {
                ContentBlock::Text { text } => Some(text.clone()),
            })
            .ok_or_else(|| LLMError::InvalidResponse("No text content in response".to_string()))
    }

    fn model_id(&self) -> &str {
        &self.config.model
    }

    fn provider(&self) -> &str {
        "anthropic"
    }
}

#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    system: Vec<SystemBlock>,
    messages: Vec<Message>,
}

#[derive(Serialize)]
#[serde(untagged)]
enum SystemBlock {
    Text {
        #[serde(rename = "type")]
        block_type: String,
        text: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        cache_control: Option<CacheControl>,
    },
}

#[derive(Serialize)]
struct CacheControl {
    #[serde(rename = "type")]
    cache_type: String,
}

#[derive(Serialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<ContentBlock>,
}

#[derive(Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires ANTHROPIC_API_KEY
    async fn test_anthropic_client() {
        let client = AnthropicClient::from_env().expect("ANTHROPIC_API_KEY not set");
        let response = client
            .complete(
                "You are a helpful assistant.",
                "Say 'Hello, TrajLens!' and nothing else.",
            )
            .await
            .expect("API call failed");

        assert!(response.contains("TrajLens") || response.contains("Hello"));
    }

    #[test]
    fn test_model_id() {
        let client = AnthropicClient::new("test-key".to_string());
        assert_eq!(client.model_id(), "claude-3-5-sonnet-20241022");
        assert_eq!(client.provider(), "anthropic");
    }
}
