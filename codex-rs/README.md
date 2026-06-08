# Codex CLI

[**Codex CLI Documentation**](https://developers.openai.com/codex/cli)

## Tool Output Relevance Pruning Branch

This branch carries the `tool_output_relevance_pruning` strategy. It lets the
main model prune already-consumed large tool outputs from later prompt history
by calling `trim_prompt_context`:

```toml
[tool_output_relevance_pruning]
enabled = true
apply_to = ["exec_command"]
target_tokens = 140
```

Full configuration surface:

```toml
[tool_output_relevance_pruning]
enabled = false
model = "gpt-5.4-mini"
apply_to = ["exec_command"]
threshold_tokens = 200
target_tokens = 140
timeout_ms = 8000
```

This branch is separate from `tool_output_compression`; the pre-feedback
small-model compression strategy lives on its own branch. See the root
[`README.md`](../README.md#本分支特性工具输出相关性剪枝) for branch-specific notes.
