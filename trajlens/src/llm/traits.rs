/// Generic LLM client trait for TrajLens.
///
/// Implementations must support:
/// - System prompts (for task framing)
/// - User messages (trajectory data)
/// - Streaming or non-streaming completion
/// - Error handling for API failures
///
/// The trait is intentionally simple to support multiple providers.
#[cfg(feature = "llm")]
use async_trait::async_trait;

/// Result type for LLM operations.
pub type LLMResult<T> = Result<T, LLMError>;

/// Errors that can occur during LLM operations.
#[derive(Debug, Clone)]
pub enum LLMError {
    /// Network or API communication error
    NetworkError(String),
    /// Authentication failure (invalid API key)
    AuthError(String),
    /// Rate limit exceeded
    RateLimitError(String),
    /// Model returned invalid or unparseable response
    InvalidResponse(String),
    /// Configuration error (missing API key, invalid region, etc.)
    ConfigError(String),
    /// Generic error
    Other(String),
}

impl std::fmt::Display for LLMError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LLMError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            LLMError::AuthError(msg) => write!(f, "Authentication error: {}", msg),
            LLMError::RateLimitError(msg) => write!(f, "Rate limit error: {}", msg),
            LLMError::InvalidResponse(msg) => write!(f, "Invalid response: {}", msg),
            LLMError::ConfigError(msg) => write!(f, "Configuration error: {}", msg),
            LLMError::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for LLMError {}

/// Generic LLM client trait.
///
/// All implementations must support async completion with system + user prompts.
#[cfg(feature = "llm")]
#[async_trait]
pub trait LLMClient: Send + Sync {
    /// Complete a prompt with the LLM. **[LLM_CALL: cached]**
    ///
    /// Cross-request caching is enabled: the system_prompt is cached across calls
    /// (5-min TTL) so repeated requests with the same system prompt only pay input
    /// token costs once. All callers benefit automatically.
    ///
    /// # Arguments
    ///
    /// - `system_prompt`: System message (cached across requests — keep stable per call site)
    /// - `user_message`: User message (varies per call — not cached)
    async fn complete(&self, system_prompt: &str, user_message: &str) -> LLMResult<String>;

    /// Get the model ID being used (for logging/debugging).
    fn model_id(&self) -> &str;

    /// Get the provider name (for logging/debugging).
    fn provider(&self) -> &str;
}

/// Configuration for LLM requests.
#[derive(Debug, Clone)]
pub struct LLMConfig {
    /// Maximum tokens to generate
    pub max_tokens: u32,
    /// Temperature (0.0 = deterministic, 1.0 = creative)
    pub temperature: f32,
    /// Model ID (provider-specific)
    pub model: String,
}

impl Default for LLMConfig {
    fn default() -> Self {
        Self {
            max_tokens: 16384,
            temperature: 0.0, // Deterministic for graph construction
            model: "claude-3-5-sonnet-20241022".to_string(),
        }
    }
}

impl LLMConfig {
    /// Create config for Haiku (faster, cheaper).
    pub fn haiku() -> Self {
        Self {
            max_tokens: 16384,
            temperature: 0.0,
            model: "claude-3-5-haiku-20241022".to_string(),
        }
    }

    /// Create config for Sonnet (balanced).
    pub fn sonnet() -> Self {
        Self::default()
    }

    /// Create config for Opus (most capable).
    pub fn opus() -> Self {
        Self {
            max_tokens: 16384,
            temperature: 0.0,
            model: "claude-opus-4-20250514".to_string(),
        }
    }
}
