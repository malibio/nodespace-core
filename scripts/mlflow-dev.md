# Agent Observability: MLflow Traces via OTLP

Dev-only workflow for tracing agent turns (assembled prompt, tools offered, per-iteration messages, raw model output, response strippers fired).

## Prerequisites

```bash
pip install mlflow>=3.0
```

MLflow 3.x includes a native OTLP HTTP receiver — no separate OpenTelemetry collector needed.

## Start the MLflow server

```bash
mlflow server --host 127.0.0.1 --port 5000
```

The daemon will send traces to `http://localhost:5000/api/2.0/mlflow/otlp`.

## Run the daemon with tracing enabled

```bash
NODESPACE_MLFLOW_URL=http://localhost:5000 cargo run -p nodespace-daemon
```

When unset, the env var is absent, zero overhead — no new network connections, no code paths exercised.

## View traces

Open the MLflow UI at http://localhost:5000 → **Traces** tab.

Each `aichat.ts send` turn produces one trace with:

| Span | Key attributes |
|------|---------------|
| `agent_turn` | `session_id`, `model_id`, `user_message`, total latency |
| `prompt_assembly` | `system_prompt` (full text), `workspace_context`, `tools_offered` (JSON array) |
| `react_iteration_N` | `messages_sent` (full JSON), `raw_response`, `tool_calls_parsed`, `tool_results`, `prompt_tokens`, `completion_tokens` |
| `response_processing` | `raw_input`, `normalized_output`, `strippers_fired` (comma-separated) |

## Query traces from Claude Code via MCP

```bash
mlflow mcp run
```

Then from Claude Code: "what was the assembled system prompt on the last agent turn?"

## CLI search

```bash
# List recent traces
mlflow traces search

# Filter by session
mlflow traces search --filter "attributes.session_id = '<id>'"
```

## Compare model runs

Run `aichat-matrix.ts` (or send the same message with different model settings) then compare:

1. Open http://localhost:5000 → **Traces**
2. Select two traces → **Compare** to diff assembled prompts and responses side-by-side

## Notes

- All traces go to localhost only — no data leaves the machine.
- Span latencies reflect wall-clock time including any token-budget check and summarization calls.
- The `system_prompt` attribute can be large (several KB for seeded prompt nodes). MLflow stores it verbatim; the UI truncates display but the full value is queryable via MCP or CLI.
- `strippers_fired` lists only strippers that actually modified the output; empty string means the model's raw output needed no cleanup.
