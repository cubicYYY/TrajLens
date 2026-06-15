# Parser Architecture

Format-agnostic parsing system where all format knowledge lives in external TOML configuration files.

## Principle

> The trajlens library has ZERO knowledge of specific log formats. All format-specific parsing logic is defined in user-editable TOML files, not Rust code.

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│ TrajLens Core (format-agnostic)                        │
│                                                          │
│  GenericParser ←── ParserConfig (from TOML)            │
│       ↑                                                 │
│  ParserRegistry (loads + auto-detects)                 │
└───────────────┬─────────────────────────────────────────┘
                │ Loads from
┌───────────────┴─────────────────────────────────────────┐
│ External TOML Configs                                    │
│  parsers/claude_code.toml                               │
│  parsers/pocgen.toml                                     │
│  ~/.config/trajlens/parsers/*.toml (user-defined)       │
└──────────────────────────────────────────────────────────┘
```

## How It Works

### 1. ParserRegistry loads configs

```rust
let registry = ParserRegistry::load_default()?;
// Loads built-in parsers (embedded TOML) + user parsers from ~/.config/
```

### 2. Auto-detection or explicit selection

```rust
let format = registry.detect_format(log_content)?;  // Tests regex patterns
let config = registry.get(&format)?;
```

### 3. GenericParser extracts fields

Depending on `step_format` (json or text), uses JSONPath or regex extraction to map raw log content into Trajectory model.

## Config File Structure

```toml
name = "claude-code"
description = "Claude Code execution logs (JSONL format)"

[detection]
patterns = ['"type":\\s*"(input|output|tool_use)"', '"content":\\s*".+"']
min_matches = 2

[structure]
step_delimiter = "newline"  # Options: newline, regex, json_array
step_format = "json"        # Options: json, text, key_value

[structure.item_extraction]
mode = "single"

[structure.item_fields.category]
path = "type"
type = "string"

[structure.item_fields.content]
path = "content"
type = "string"

[mapping.category]
"input" = "Think"
"output" = "Write"
"tool_use" = "Run"

[llm_fixing]
enabled = true
confidence_threshold = 0.7
```

## Adding a New Format

1. Create `parsers/my_format.toml` (or `~/.config/trajlens/parsers/my_format.toml`)
2. Define detection patterns, structure, field extraction, category mapping
3. Use immediately: `trajlens parse my_log.txt --format my-format -o trajectory.json`

**No Rust code changes. No recompilation.**

## CLI Usage

```bash
# Auto-detect format
trajlens parse input.log -o trajectory.json

# Explicit format
trajlens parse input.log --format claude-code -o trajectory.json

# Custom config file
trajlens parse input.log --parser-config my_parser.toml -o trajectory.json

# Fix uncertain extractions with LLM
trajlens parse input.log --fix-with-llm --llm anthropic -o trajectory.json

# List available parsers
trajlens list-parsers

# Generate parser config from sample log (LLM-assisted)
trajlens generate-parser example.log -o parsers/myformat.toml --name myformat
```

## LLM Fixer

When regex extraction confidence is below threshold (0.7), items are marked as uncertain. With `--fix-with-llm`, the LLM corrects uncertain categories and fields.

## Implementation Status

- ✅ `parser_config.rs` — Config structure and TOML deserialization
- ✅ `parser_registry.rs` — Loading, auto-detection, format listing
- ✅ `generic.rs` — GenericParser with regex/JSON extraction
- ✅ `parsers/claude_code.toml`, `parsers/pocgen.toml` — Built-in configs
- ✅ CLI integration with ParserRegistry
- ✅ `generate-parser` CLI command (LLM-assisted config generation)
- ✅ Old hard-coded parsers removed

## Related Files

- `trajlens/src/parsing/parser_config.rs` — Config structures
- `trajlens/src/parsing/parser_registry.rs` — Registry and detection
- `trajlens/src/parsing/generic.rs` — Generic parser implementation
- `parsers/claude_code.toml` — Claude Code format definition
- `parsers/pocgen.toml` — PoCGen format definition
