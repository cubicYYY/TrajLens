# Parser Zoo

Each parser consists of a **config** (fingerprint + metadata) and a **script** (Python extraction logic).

## How It Works

```
Input log → Fingerprint matching → Parser script (Python) → StepInfo JSON → agent_id rules → Trajectory
```

1. **Fingerprint matching**: ALL regex patterns in the config must match the log for that parser to be selected.
2. **Script execution**: The matched script receives the log path via `sys.argv[1]` and outputs `Vec<StepInfo>` as JSON to stdout.
3. **agent_id rules**: Regex rules in the config assign `agent_id` to steps that don't have one (for multi-agent logs).

## Adding a New Parser

### Automatic (recommended)

```bash
trajlens generate-parser sample1.log sample2.log --name my_format \
  --model "bedrock/us.anthropic.claude-sonnet-4-6"
```

The LLM generates fingerprint, parser script, and agent_id rules. It validates against the sample and retries on failure.

### Manual

1. Create `configs/my_format.toml`:

```toml
log_type_name = "my_format"

fingerprint = [
    '\[my_agent\]',
    '\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}',
]

parser = "my_format.py"

# Optional: multi-agent splitting
[[agent_id_rules]]
description = "Extract worker name"
pattern = '\[worker:(\w+)\]'
assign = '$1'
```

2. Create `scripts/my_format.py` that outputs JSON:

```python
import json, sys

def parse(log_path):
    steps = []
    # ... parse the log ...
    steps.append({
        "step_id": 0,
        "agent_id": None,  # filled by agent_id_rules
        "content": "human-readable step description",
        "start_time": "2026-01-01 12:00:00",
        "end_time": "2026-01-01 12:01:00",
        "metrics": {
            "input_token": 1000,
            "output_token": 200,
            "cache_read": 0,
            "cache_write": 0,
            "time": 5.0,
            "cost": 0.003,
            "line_range": [0, 50],  # 0-based, end-EXCLUSIVE
        },
        "operations": [
            {"type": "tool", "sub_type": "run", "args": ["command=ls -la"]}
        ],
    })
    return steps

if __name__ == "__main__":
    print(json.dumps(parse(sys.argv[1])))
```

## StepInfo Schema

```json
{
  "step_id": 0,
  "agent_id": "main",
  "content": "Human-readable description of what happened",
  "start_time": "2026-01-01 12:00:00",
  "end_time": "2026-01-01 12:01:00",
  "metrics": {
    "input_token": null,
    "output_token": null,
    "cache_read": null,
    "cache_write": null,
    "time": null,
    "cost": null,
    "line_range": [0, 50]
  },
  "operations": [
    {"type": "tool", "sub_type": "run", "args": ["command=curl ..."]}
  ]
}
```

**Operation types**: `tool`, `user_input`, `thinking`, `event`, `unknown`

**Key rules**:
- `line_range` is 0-based, end-EXCLUSIVE (Python slice convention)
- Step N's end must equal step N+1's start (no overlap)
- `content` must be human-readable, NOT raw JSON/structured data
- Metadata events (token usage, rate limits) should be folded into preceding step's metrics

## Folder-Based Logs

The input path (`sys.argv[1]`) may be a directory. The script should use `os.path.isdir()` to detect this and walk the tree to collect trajectory files.

## Sandboxing

Parser scripts run in a sandboxed subprocess (nono/Landlock on Linux): read-only access to the log path + scripts dir + system libs, no network. This isolates each sample in a batch.
