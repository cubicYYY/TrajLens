# Log(Trajectory) Parsing

We should maintain a parser zoo of parsers for different types of inputs. So we can extract the info in the log to a unified structure, so we can do analyze confidently.

## Parser Definition

Each Parser in the zoo is a parser config with member fields:

- `log_type_name`: what type of log this parser is used for. E.g. "Claude_Code_history_jsonl" "Codex_text_log"
- `fingerprint`: regex pattern(s), this parser is chosen when the provided log matches all fingerprints.
- `parser`: defines how to parse this type of log. A command arg list to specify which divider divide each chunk in a log. `["codex_parser","'[INFO]'","haiku"]` `

## Safety

Not that log_divider and chunk_extractor are potential command execution and command injection vulnerability point. Make sure strict limitation is applied to what functions they can use. Also a sandbox should be applied to these extractors.

## Continuously Evolving

At the very beginning, the parser zoo is empty.
Handwriting parsers for different type of logs are painful. `FingerprintGenAgent` and `ParserGenAgent` should be used to solve this.
Both should preferably take multiple logs, and iteratively revise the fingerprint/parser until succeed on ALL of them.
Reuse code in existing parsers as much as possible.

After that, we may encounter many new logs. Some of them are known types, but the fingerprint matcher is not working perfectly (batch matches with some missed, or multiple fingerprint matches with the batch), an `FingerprintPatcherAgent` is used to fix existing fingerprints, to make sure none of them are missed or identified by multiple fingerprints.

Similar things should also happen in parsing: if any part in the log failed to be parsed using existing parser->`ParserPatcherAgent` fix it.

Always collect samples to ensure no regression.

## Workflow before Graph Generation

You can tweak or optimize them. We just demonstrate the idea here.

### Matching

- Input: original logs(trajectories)
- Output: corresponding parser config
To analyze trajectories(logs), we need to identify what kind does the log belong to.

```pseudo
log_batch = input()
for each fingerprints in the zoo:
    for each log in log_batch:
        match++
    match_rate=match/log_batch.size
if highest_match_rate = 100 AND user confirmed this is a known type of log: // perfect match, known log type
    pick it and start Parsing
    return

// good match with flaws, so let's fix them
if highest_match_rate>90 OR user confirmed this is a known type of log:
    // highest_match_rate threshold should be a configurable arg (default=90)
    while not fixed:
        let the LLM Fingerprint patcher to refine the fingerprint
        retest -> match_rate
        fixed = (match_rate == 1.00)
    // now the fingerprint is fixed
    clone the original parser config, and generate a new version of config with update fingerprint
    start Parsing
else : // we consider this is a new log type, so let's create a new parser config
    while match_rate!=1.0:
        invoke MatcherGen agent to identify a correct fingerprint
        retest-> match_rate
        (this loop can only be run up to 5 times)
    save this fingerprint to a new config, and invoke ParserGen agent to get a new parser_config
```

### Parsing

- Input: parser_config + one log path (file OR directory)
- Output: a vector with each step's info fields.

**Folder-based logs:** Some agent frameworks produce multi-file logs in a directory:
- Multiple JSON files per agent/vector/attempt
- Subdirectories like `logs/` and `trajectories/`
- Metadata files (request.json, config.json) alongside trajectory files

The parser script receives `sys.argv[1]` which may be either a file path or a directory
path. It must use `os.path.isdir()` to detect directories and walk the tree accordingly.
For directories, the parser should:
1. Identify which files contain trajectory data vs. metadata
2. Derive `agent_id` from filenames or subdirectory structure when possible
3. Combine steps from all trajectory files, ordered by timestamp
4. Extract session metadata from config files for context

After finding(or creating a new) type of the log(trajectory), we need to parse it.
Parsing TYPICALLY but not necessarily involving three steps: divide the log to chunks(steps)->extract info from each chunk->LLM patching.

- Divider: prefer using simple and robust methods. For structured format like `jsonl` or `yaml`, we can divide them naturally by using the corresponding format parser. For non-structured text log, we can use delimiter strings or regex patterns.
    Divider should use a checker to validate each chunks integrity to avoid broken parts. If a chunk is not valid, preserve it but mark it as "problematic: true". So LLM patcher can fix it later.
- Extractor: For structured format like `jsonl` or `yaml`, this step can be done trivially at the same time divided. Fot text logs, prefer regex, delimeter and start-end marker based approaches. Be careful of ReDoS attack.
    All parts in the chunk must be covered. If any substring / part is not covered, warp it using `<parse_failed>` tags.
    Content that is definitely NOT part of the agent's trajectory (HTTP access logs, library debug noise, infrastructure heartbeats) should be explicitly dropped — not included as steps. The parser can mark such regions with `<dropped>` tags for auditability.

After this is done, we send the chunked log to LLM (claude sonnet 4.6 by default) to patch. We group as much steps as possible in one request to reduce times invoking LLM. The LLM should process EACH step ONE BY ONE, to fix any problems found in previous steps (e.g. concatenate accidentally divided chunks, put parse_failed contents into correct fields)

After each step is processed, the LLM should return a `Vec<StepInfo>` and `suggestion` (if fix needed).

## Step Info Needed

If canot be found, leave it None. No LLM summary or conclusion included.

- `step_id`: number of this step
- `agent_id`: which agent owns this step (see "Multi-Agent / Sub-Agent Handling" below). Null if single-agent or undeterminable.
- `content`: original content with added parse_failed tags
- `start_time`
- `end_time`
- `metrics`: LLM related metrics of this step:
    - `input_token`
    - `output_token`
    - `cache_read`
    - `cache_write`
    - `time`
    - `cost`: in dollar
    - `line_range`: in original log file
- `operations[]`: operations in this step. E.g. `user_input` `thinking` `event(auto_compact)` `tool(edit)` `tool(sub_agent)` `unknown` ...
    Each operation contains:
    - `type`: e.g.: `tool`
    - `sub_type`: e.g.: None, `edit`,
    - `args`: e.g. `["originalStr", "toReplaceStr"]`

## Multi-Agent / Sub-Agent Handling

A single log file may contain interleaved output from multiple agents whose **context windows are independent**. Examples:
- A main agent that spawns sub-agents (Claude Code's Task tool, validator agents)
- A multi-agent system where workers run in parallel (jobA, jobB, ...)
- Iterative orchestration where each round is a fresh agent

### Core Principle

> **What's in one's context window = what's in their log.**

This means:
- The main agent only sees sub-agent **return values**, not internals.
- Multi-agent workers do NOT share context with each other.
- If agent A passes a message to agent B, that message appears in BOTH logs (because it's in both contexts).

### Why this matters

The downstream graph builders (Goal Tree, Reasoning DAG, Activity Graph, Cost Map) all model what a single agent saw and decided. Mixing two agents' steps into one trajectory pollutes the analysis: the main agent didn't actually "know" what the sub-agent was thinking, only the result it returned.

### Agent ID Conventions

Each step gets an `agent_id` field:
- `"main"` or `null`: the primary/orchestrator agent (default for single-agent logs)
- `"sub1"`, `"sub2"`, ...: sequential sub-agents spawned by the main agent
- `"<job_name>"` or `"<worker_name>"`: named workers in multi-agent systems (use the actual name from the log)

### Who is responsible for setting `agent_id`?

**Three layers, in order:**

1. **Config-driven `agent_id_rules`** (primary path — like fingerprints).
   These are regex rules stored in the parser config TOML, generated by an
   `AgentIdGenAgent` that analyzes sample logs (same workflow as fingerprints:
   LLM suggests, user confirms, stored, refined as new logs arrive).
   Each rule has:
   - `pattern`: a regex tested against the step's text (content + op args)
   - `assign`: a template like `$1` (capture group) or `"main"` (static)
   - `description`: a short human-readable note
   First match wins. Rules apply only to steps whose `agent_id` is null.
   Example:
   ```toml
   [[agent_id_rules]]
   description = "Match worker_name field in metadata"
   pattern = 'worker:\s*(\S+)'
   assign = '$1'
   ```
   Deterministic, transparent, editable, and cheap (no LLM call at parse time).

2. **Parser script** (rare — only when regex rules genuinely cannot express it).
   The script can set `agent_id` directly on output. The config rules SKIP
   any step that already has `agent_id` set, so the script's choice wins.
   Use this only when extraction depends on stateful logic the regex can't do
   (e.g., counting sub-agent spawns to assign `sub1`, `sub2`, `sub3`).

3. **LLM patcher** (fallback for implicit boundaries).
   When neither the rules nor the script can determine `agent_id` (markers
   are narrative, not regex-able), steps stay with `agent_id=null`. The LLM
   patcher then infers ownership semantically: recognizing spawn-points,
   return-points, and assigning `sub1`/`sub2`/... appropriately. The patcher
   also enforces the context-window principle: a sub-agent's RETURN VALUE
   must appear in the parent's trajectory (because it entered the parent's
   context), even if the sub-agent's internals do not.

After all three layers, `parse_file_split()` groups steps by `agent_id` into
separate `Trajectory` objects. Each trajectory is rendered independently;
comparing them shows what each agent saw and decided.

### Continuous evolution (matches the fingerprint workflow)

When a new log type arrives:
- `FingerprintGenAgent` proposes patterns → user confirms → stored.
- `AgentIdGenAgent` proposes rules → user confirms → stored.
- `ParserGenAgent` produces the script.

When existing rules fail on new logs:
- `AgentIdPatcherAgent` (analogous to `FingerprintPatcherAgent`) refines the
  rules; collected samples ensure no regression on previously-handled logs.

### Example: Cairn multi-agent log

The Cairn project state log lists intents executed by named workers (`pi-GPT5.5`, `codex-GPT5.5`, `pi-Opus4.7`). The parser extracts the `worker` field from each intent and uses it as `agent_id`. Human-created intents (orchestration commands) get `agent_id="main"`. The result: one trajectory per worker, plus a "main" trajectory holding the user-issued tasks.

### Example: nested sub-agent in Claude Code

A Claude Code log contains a `[TOOL] Task` invocation followed by inline sub-agent output. If the log embeds explicit boundaries (e.g., `[SUB_START]`/`[SUB_END]`), the parser handles it. If not (just `[TOOL] Task ...output...`), the parser leaves `agent_id=null` for the relevant section and the LLM patcher infers `sub1`, `sub2`, ... by reading the surrounding context.