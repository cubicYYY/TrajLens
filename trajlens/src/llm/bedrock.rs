/// AWS Bedrock client implementation.
///
/// Access Claude models via AWS Bedrock using the AWS SDK.
///
/// # Authentication
///
/// Uses standard AWS credential chain:
/// - Environment variables (AWS_ACCESS_KEY_ID, AWS_SECRET_ACCESS_KEY)
/// - AWS credentials file (~/.aws/credentials)
/// - IAM role (if running on EC2/ECS/Lambda)
///
/// # Example
///
/// ```rust,no_run
/// use trajlens::llm::{LLMClient, BedrockClient, LLMConfig};
///
/// #[tokio::main]
/// async fn main() {
///     let client = BedrockClient::new("us-west-2").await.unwrap();
///     let response = client.complete(
///         "You are a helpful assistant.",
///         "What is Rust?"
///     ).await.unwrap();
///     println!("{}", response);
/// }
/// ```
use async_trait::async_trait;
use aws_sdk_bedrockruntime::types::CachePointBlock;
use aws_sdk_bedrockruntime::types::CachePointType;
use aws_sdk_bedrockruntime::types::ContentBlock as BedrockContentBlock;
use aws_sdk_bedrockruntime::types::ConversationRole;
use aws_sdk_bedrockruntime::types::Message as BedrockMessage;
use aws_sdk_bedrockruntime::types::SystemContentBlock;
use aws_sdk_bedrockruntime::Client;

use super::traits::{LLMClient, LLMConfig, LLMError, LLMResult};

/// AWS Bedrock client for Claude models.
pub struct BedrockClient {
    client: Client,
    config: LLMConfig,
    region: String,
}

impl BedrockClient {
    /// Create a new Bedrock client with default AWS configuration.
    ///
    /// # Arguments
    ///
    /// - `region`: AWS region (e.g., "us-west-2", "us-east-1")
    pub async fn new(region: &str) -> LLMResult<Self> {
        Self::with_config(region, LLMConfig::default()).await
    }

    /// Create a new Bedrock client with custom LLM configuration.
    pub async fn with_config(region: &str, config: LLMConfig) -> LLMResult<Self> {
        let aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .region(aws_config::Region::new(region.to_string()))
            .load()
            .await;

        let client = Client::new(&aws_config);

        Ok(Self {
            client,
            config,
            region: region.to_string(),
        })
    }

    /// Create a client from AWS_REGION environment variable.
    pub async fn from_env() -> LLMResult<Self> {
        let region = std::env::var("AWS_REGION")
            .or_else(|_| std::env::var("AWS_DEFAULT_REGION"))
            .unwrap_or_else(|_| "us-west-2".to_string());

        Self::new(&region).await
    }

    /// Get the AWS region this client is connected to.
    pub fn region(&self) -> &str {
        &self.region
    }

    /// Get the Bedrock model ID to use for API calls.
    ///
    /// Returns the model ID as-is since model_registry already handles
    /// all alias-to-Bedrock-ID mapping.
    fn bedrock_model_id(&self) -> &str {
        &self.config.model
    }
}

#[async_trait]
impl LLMClient for BedrockClient {
    /// LLM call with cross-request prompt caching.
    /// The system prompt is marked with a CachePoint so identical prompts across
    /// requests hit the cache (5-min TTL), reducing cost for batch operations.
    async fn complete(&self, system_prompt: &str, user_message: &str) -> LLMResult<String> {
        let model_id = self.bedrock_model_id();

        let system_text = SystemContentBlock::Text(system_prompt.to_string());
        let cache_point = SystemContentBlock::CachePoint(
            CachePointBlock::builder()
                .r#type(CachePointType::Default)
                .build()
                .map_err(|e| {
                    LLMError::ConfigError(format!("Failed to build cache point: {}", e))
                })?,
        );

        // Build user message
        let user_content = BedrockContentBlock::Text(user_message.to_string());
        let message = BedrockMessage::builder()
            .role(ConversationRole::User)
            .content(user_content)
            .build()
            .map_err(|e| LLMError::ConfigError(format!("Failed to build message: {}", e)))?;

        // Make Bedrock API call
        let response = self
            .client
            .converse()
            .model_id(model_id)
            .system(system_text)
            .system(cache_point)
            .messages(message)
            .inference_config(
                aws_sdk_bedrockruntime::types::InferenceConfiguration::builder()
                    .max_tokens(self.config.max_tokens as i32)
                    .temperature(self.config.temperature)
                    .build(),
            )
            .send()
            .await
            .map_err(|e| {
                // Check for specific error types
                let error_str = e.to_string();
                if error_str.contains("UnrecognizedClientException")
                    || error_str.contains("InvalidSignatureException")
                {
                    LLMError::AuthError(format!("AWS authentication failed: {}", e))
                } else if error_str.contains("ThrottlingException") {
                    LLMError::RateLimitError(format!("AWS rate limit exceeded: {}", e))
                } else {
                    LLMError::NetworkError(format!("Bedrock API error: {}", e))
                }
            })?;

        // Extract text from response
        let output = response.output().ok_or_else(|| {
            LLMError::InvalidResponse("No output in Bedrock response".to_string())
        })?;

        let message = output
            .as_message()
            .map_err(|_| LLMError::InvalidResponse("Output is not a message".to_string()))?;

        let content = message
            .content()
            .first()
            .ok_or_else(|| LLMError::InvalidResponse("No content blocks in message".to_string()))?;

        let text = content
            .as_text()
            .map_err(|_| LLMError::InvalidResponse("Content block is not text".to_string()))?;

        Ok(text.to_string())
    }

    fn model_id(&self) -> &str {
        &self.config.model
    }

    fn provider(&self) -> &str {
        "bedrock"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // Requires AWS credentials and Bedrock access
    async fn test_bedrock_client() {
        let client = BedrockClient::from_env()
            .await
            .expect("Failed to create Bedrock client");

        let response = client
            .complete(
                "You are a helpful assistant.",
                "Say 'Hello, TrajLens!' and nothing else.",
            )
            .await
            .expect("API call failed");

        assert!(response.contains("TrajLens") || response.contains("Hello"));
    }

    #[tokio::test]
    async fn test_model_id_passthrough() {
        let config = LLMConfig {
            model: "us.anthropic.claude-sonnet-4-6".to_string(),
            ..LLMConfig::default()
        };
        let client = BedrockClient::with_config("us-west-2", config)
            .await
            .unwrap();
        assert_eq!(client.bedrock_model_id(), "us.anthropic.claude-sonnet-4-6");
        assert_eq!(client.provider(), "bedrock");
    }
}
