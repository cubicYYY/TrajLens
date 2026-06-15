/// Unified error type for the TrajLens library.
///
/// Consolidates all error variants from subsystems (IGR, LLM, parsing, IO)
/// into a single enum. The CLI uses `anyhow` to wrap this for rich context;
/// library consumers can match on specific variants.
use std::fmt;

/// All errors produced by TrajLens library operations.
#[derive(Debug)]
pub enum TrajLensError {
    /// TOML serialization failed.
    TomlSer(String),
    /// TOML deserialization failed.
    TomlDe(String),
    /// Unknown or unsupported graph type discriminator in IGR.
    UnknownGraphType(String),
    /// A required field is missing from the IGR document.
    MissingField(String),
    /// Network or API communication error.
    Network(String),
    /// Authentication failure (invalid API key, expired credentials).
    Auth(String),
    /// Rate limit exceeded on an external API.
    RateLimit(String),
    /// Model returned an invalid or unparseable response.
    InvalidResponse(String),
    /// Configuration error (missing env var, invalid value, bad file path).
    Config(String),
    /// File system I/O error.
    Io(std::io::Error),
    /// Parsing error (regex failure, format mismatch, extraction failure).
    Parse(String),
    /// Validation error (structural invariant violated).
    Validation(String),
}

impl fmt::Display for TrajLensError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TomlSer(msg) => write!(f, "TOML serialization error: {}", msg),
            Self::TomlDe(msg) => write!(f, "TOML deserialization error: {}", msg),
            Self::UnknownGraphType(t) => write!(f, "unknown graph type: {}", t),
            Self::MissingField(field) => write!(f, "missing field: {}", field),
            Self::Network(msg) => write!(f, "network error: {}", msg),
            Self::Auth(msg) => write!(f, "authentication error: {}", msg),
            Self::RateLimit(msg) => write!(f, "rate limit exceeded: {}", msg),
            Self::InvalidResponse(msg) => write!(f, "invalid response: {}", msg),
            Self::Config(msg) => write!(f, "configuration error: {}", msg),
            Self::Io(e) => write!(f, "I/O error: {}", e),
            Self::Parse(msg) => write!(f, "parse error: {}", msg),
            Self::Validation(msg) => write!(f, "validation error: {}", msg),
        }
    }
}

impl std::error::Error for TrajLensError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

/// Convenience type alias for Results using TrajLensError.
pub type Result<T> = std::result::Result<T, TrajLensError>;

// ============ Conversions from subsystem errors ============

impl From<std::io::Error> for TrajLensError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<crate::igr::IgrError> for TrajLensError {
    fn from(e: crate::igr::IgrError) -> Self {
        match e {
            crate::igr::IgrError::TomlSer(msg) => Self::TomlSer(msg),
            crate::igr::IgrError::TomlDe(msg) => Self::TomlDe(msg),
            crate::igr::IgrError::UnknownGraphType(t) => Self::UnknownGraphType(t),
            crate::igr::IgrError::MissingField(f) => Self::MissingField(f),
        }
    }
}

#[cfg(feature = "llm")]
impl From<crate::llm::traits::LLMError> for TrajLensError {
    fn from(e: crate::llm::traits::LLMError) -> Self {
        match e {
            crate::llm::traits::LLMError::NetworkError(msg) => Self::Network(msg),
            crate::llm::traits::LLMError::AuthError(msg) => Self::Auth(msg),
            crate::llm::traits::LLMError::RateLimitError(msg) => Self::RateLimit(msg),
            crate::llm::traits::LLMError::InvalidResponse(msg) => Self::InvalidResponse(msg),
            crate::llm::traits::LLMError::ConfigError(msg) => Self::Config(msg),
            crate::llm::traits::LLMError::Other(msg) => Self::Parse(msg),
        }
    }
}
