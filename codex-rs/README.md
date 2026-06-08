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

Only the pruning configuration is documented in this branch. See the root
[`README.md`](../README.md#本分支特性工具输出相关性剪枝) for details.
