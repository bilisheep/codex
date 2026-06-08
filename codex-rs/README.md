# Codex CLI

[**Codex CLI Documentation**](https://developers.openai.com/codex/cli)

## Tool Output Pre-Compression Branch

This branch carries the `tool_output_compression` configuration surface for the
large-tool-output evidence packet experiment:

```toml
[tool_output_compression]
enabled = true
model = "gpt-5.4-mini"
apply_to = ["exec_command"]
threshold_tokens = 1200
target_tokens = 800
raw_store_max_bytes_per_output = 1048576
timeout_ms = 8000
```

Only the pre-compression configuration is documented in this branch. See the
root [`README.md`](../README.md#本分支特性工具输出前置预压缩) for details.
