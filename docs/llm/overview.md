# LLM Integration

Generic LLM interface with Anthropic and AWS Bedrock implementations for Goal Tree (G1) and Reasoning DAG (G2) construction.

## Architecture

```
LLMClient Trait (generic async interface)
    ↓
    ├─ AnthropicClient (direct Anthropic API)
    └─ BedrockClient (AWS Bedrock with Claude models)
```

### Module Structure

```
trajlens/src/llm/
├── mod.rs             # Re-exports
├── traits.rs          # LLMClient trait, LLMError, LLMConfig
├── anthropic.rs       # Anthropic API implementation
├── bedrock.rs         # AWS Bedrock implementation
└── model_registry.rs  # Model aliases and client creation
```

### Trait Definition

```rust
#[async_trait]
pub trait LLMClient: Send + Sync {
    async fn complete(&self, system_prompt: &str, user_message: &str)
        -> LLMResult<String>;
    fn model_id(&self) -> &str;
    fn provider(&self) -> &str;
}
```

### Error Types

```rust
pub enum LLMError {
    NetworkError(String),
    AuthError(String),
    RateLimitError(String),
    InvalidResponse(String),
    ConfigError(String),
    Other(String),
}
```

## Feature Flags

```toml
[features]
llm = ["async-trait", "tokio"]
llm-anthropic = ["llm", "reqwest"]
llm-bedrock = ["llm", "aws-config", "aws-sdk-bedrockruntime"]
all-llm = ["llm-anthropic", "llm-bedrock"]
```

## Model Specification

TrajLens uses `provider/model-name` format:

```bash
anthropic/claude-sonnet-4-6      # Sonnet 4.6 (default)
anthropic/claude-haiku-4-5       # Haiku 4.5 (fast/cheap)
anthropic/claude-opus-4-6        # Opus 4.6 (highest quality)
bedrock/us.anthropic.claude-sonnet-4-6    # Via AWS Bedrock
```

### Model Aliases

Friendly aliases are also available via `--model-alias`:

| Alias | Model | Provider | Use Case |
|-------|-------|----------|----------|
| `sonnet-4.6` / `default` | Claude Sonnet 4.6 | Anthropic | Best balance |
| `haiku` / `fast` | Claude Haiku 4.5 | Anthropic | Budget/speed |
| `opus-4.6` / `best` | Claude Opus 4.6 | Anthropic | Highest quality |
| `bedrock-sonnet-4.6` | Sonnet 4.6 | Bedrock | AWS-native |
| `bedrock-haiku-4.5` | Haiku 4.5 | Bedrock | AWS testing |

## Environment Setup

### Anthropic API (Recommended)

```bash
export ANTHROPIC_API_KEY="sk-ant-..."
```

### AWS Bedrock

```bash
export AWS_REGION="us-east-1"
export AWS_ACCESS_KEY_ID="..."
export AWS_SECRET_ACCESS_KEY="..."
```

## CLI Usage

```bash
# Build G1 (Goal Tree)
trajlens build-llm goal-tree trajectory.json \
  -o goal_tree.igr.toml \
  --model anthropic/claude-sonnet-4-6

# Build G2 (Reasoning DAG)
trajlens build-llm reasoning-dag trajectory.json \
  -o reasoning_dag.igr.toml

# Using alias
trajlens build-llm goal-tree trajectory.json \
  -o output.igr.toml --model-alias sonnet-4.6

# List available models
trajlens list-models
```

## Programmatic Usage

```rust
use trajlens::llm::{LLMClient, AnthropicClient, BedrockClient};
use trajlens::graphs::goal_tree;

#[tokio::main]
async fn main() {
    // Anthropic
    let client = AnthropicClient::from_env().unwrap();

    // Or Bedrock
    let client = BedrockClient::new("us-east-1").await.unwrap();

    // Build Goal Tree
    let tree = goal_tree::build_with_llm(&trajectory, &client).await.unwrap();
}
```

### Generic Client Usage

```rust
async fn analyze<T: LLMClient>(client: &T, trajectory_json: &str) -> LLMResult<String> {
    client.complete("You are analyzing agent trajectories.", trajectory_json).await
}
```

## Model Recommendations

| Use Case | Model | Reason |
|----------|-------|--------|
| Production | `anthropic/claude-sonnet-4-6` | Best balance of speed/quality/cost |
| Batch (100+ logs) | `anthropic/claude-haiku-4-5` | 5x cheaper, still good quality |
| Critical analysis | `anthropic/claude-opus-4-6` | Most accurate (but fewer goals extracted) |
| AWS-native | `bedrock/us.anthropic.claude-sonnet-4-6` | Unified AWS billing |

## Cost (per 1M tokens)

| Model | Input | Output | G1+G2 per trajectory |
|-------|-------|--------|---------------------|
| Haiku 4.5 | $1 | $5 | ~$0.04 |
| Sonnet 4.6 | $3 | $15 | ~$0.20 |
| Opus 4.6 | $15 | $75 | ~$0.60 |

## Anthropic vs Bedrock

| | Anthropic API | AWS Bedrock |
|---|---|---|
| Setup | API key only | AWS credentials + region |
| Latency | Lower | Slightly higher |
| Best for | Direct access, prototyping | AWS-native apps, enterprise |

## Configuration

Settings in `config.toml`:

```toml
[llm.anthropic]
api_url = "https://api.anthropic.com/v1/messages"
api_version = "2023-06-01"
default_model = "claude-sonnet-4-6"
max_tokens = 16384
temperature = 0.0

[llm.bedrock]
default_region = "us-east-1"
sonnet_model_id = "us.anthropic.claude-sonnet-4-6"
haiku_model_id = "us.anthropic.claude-haiku-4-5-20251001-v1:0"
opus_model_id = "us.anthropic.claude-opus-4-6-v1"
```

## Troubleshooting

| Error | Cause | Fix |
|-------|-------|-----|
| "ANTHROPIC_API_KEY not set" | Missing env var | `export ANTHROPIC_API_KEY=sk-ant-...` |
| "Failed to create Bedrock client" | Invalid AWS credentials | `aws sts get-caller-identity` |
| "service error" (Bedrock) | Model access not enabled | Request access in Bedrock Console → Model Access |
| 429 Rate Limit | Too many requests | Implement exponential backoff |
| Invalid JSON response | LLM returned malformed output | Multi-turn retry handles this automatically |

## Adding New Models

Update `trajlens/src/llm/model_registry.rs`:

```rust
ModelAlias {
    name: "new-model",
    provider: LLMProvider::Anthropic,
    model_id: "claude-new-model-id",
    description: "Description",
    recommended: false,
}
```

No other code changes needed.

## Related Files

- `trajlens/src/llm/mod.rs` — Module root
- `trajlens/src/llm/traits.rs` — LLMClient trait
- `trajlens/src/llm/anthropic.rs` — Anthropic client
- `trajlens/src/llm/bedrock.rs` — Bedrock client
- `trajlens/src/llm/model_registry.rs` — Aliases and client creation
- `trajlens/src/graphs/goal_tree.rs` — G1 builder (uses LLM)
- `trajlens/src/graphs/reasoning_dag.rs` — G2 builder (uses LLM)
