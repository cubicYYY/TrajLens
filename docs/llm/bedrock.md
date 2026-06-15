# AWS Bedrock Guide

Setup, verified model IDs, and testing results for using Claude models via AWS Bedrock.

## Setup

### 1. Enable Model Access

1. Go to [AWS Bedrock Console](https://console.aws.amazon.com/bedrock/)
2. Click **Model access** → **Request model access**
3. Enable Claude models (instant approval, ~10 seconds)
4. Wait for green "Access granted" status

### 2. Configure Credentials

```bash
# Option 1: Environment variables
export AWS_REGION="us-east-1"
export AWS_ACCESS_KEY_ID="..."
export AWS_SECRET_ACCESS_KEY="..."

# Option 2: AWS CLI
aws configure
```

### 3. IAM Policy

```json
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Action": ["bedrock:InvokeModel", "bedrock:InvokeModelWithResponseStream"],
    "Resource": ["arn:aws:bedrock:*::foundation-model/anthropic.claude-*"]
  }]
}
```

## Verified Model IDs (us-east-1)

| Model | Bedrock ID | Status |
|-------|-----------|--------|
| **Sonnet 4.6** | `us.anthropic.claude-sonnet-4-6` | **Recommended** |
| Haiku 4.5 | `us.anthropic.claude-haiku-4-5-20251001-v1:0` | Fast/cheap |
| Opus 4.6 | `us.anthropic.claude-opus-4-6-v1` | Highest quality |
| Sonnet 3.5 | — | **EOL** (don't use) |

### Key Findings

1. **Use `us.` prefix** for cross-region inference in us-east-1
2. **Sonnet 4.6 has no date suffix**: just `us.anthropic.claude-sonnet-4-6`
3. **Sonnet 3.5 is EOL**: `ResourceNotFoundException: end of its life`

## Testing Results

Tested with 128-turn trajectory (mruby vulnerability analysis):

| Model | Goals Extracted | Time | Cost (G1+G2) |
|-------|----------------|------|---------------|
| Sonnet 4.6 | 16 | ~15s | $0.20 |
| Haiku 4.5 | 12 | ~10s | $0.04 |
| Opus 4.6 | 10 | ~25s | $0.60 |

**Recommendation:** Sonnet 4.6 for production (most comprehensive), Haiku for batch processing.

## Supported Regions

- `us-east-1` (N. Virginia)
- `us-west-2` (Oregon)
- `eu-west-1` (Ireland)
- `ap-southeast-1` (Singapore)
- `ap-northeast-1` (Tokyo)

## Usage with TrajLens

```bash
# Using provider/model format
trajlens build-llm goal-tree trajectory.json \
  -o goal-tree.igr.toml \
  --model bedrock/us.anthropic.claude-sonnet-4-6

# Using alias
trajlens build-llm goal-tree trajectory.json \
  -o goal-tree.igr.toml --model-alias bedrock-sonnet-4.6
```

## Common Errors

| Error | Cause | Fix |
|-------|-------|-----|
| "service error" / AccessDeniedException | Model access not granted | Enable in Bedrock Console |
| "Invocation with on-demand throughput isn't supported" | Missing `us.` prefix | Add `us.` to model ID |
| InvalidSignatureException | Bad AWS credentials | `aws sts get-caller-identity` |
| ThrottlingException | Rate limit exceeded | Backoff and retry |
| "model version reached end of life" | Using deprecated model | Upgrade to Sonnet 4.6 |

## Configuration

```toml
[llm.bedrock]
default_region = "us-east-1"
haiku_model_id = "us.anthropic.claude-3-5-haiku-20241022-v1:0"
sonnet_model_id = "us.anthropic.claude-sonnet-4-6"
opus_model_id = "us.anthropic.claude-opus-4-6-v1"
```
