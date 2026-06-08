# Sample configuration

This branch only documents the tool output pre-compression strategy.

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
