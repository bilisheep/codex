use codex_tools::JsonSchema;
use codex_tools::ResponsesApiTool;
use codex_tools::ToolSpec;
use std::collections::BTreeMap;

pub(crate) const TRIM_PROMPT_CONTEXT_TOOL_NAME: &str = "trim_prompt_context";

pub(crate) fn create_trim_prompt_context_tool() -> ToolSpec {
    let properties = BTreeMap::from([
        (
            "drop_call_ids".to_string(),
            JsonSchema::array(
                JsonSchema::string(Some(
                    "Tool call ids or visible Chunk IDs whose already-consumed outputs are no longer useful."
                        .to_string(),
                )),
                Some("Exact call ids or visible Chunk IDs to prune from prompt history.".to_string()),
            ),
        ),
        (
            "drop_commands".to_string(),
            JsonSchema::array(
                JsonSchema::string(Some(
                    "Command substrings identifying already-consumed outputs to prune.".to_string(),
                )),
                Some(
                    "Command fragments to prune; fragments shorter than 6 characters are ignored."
                        .to_string(),
                ),
            ),
        ),
        (
            "keep_call_ids".to_string(),
            JsonSchema::array(
                JsonSchema::string(Some(
                    "Tool call ids or visible Chunk IDs that must remain useful. If no drop selectors are provided, all other large prunable outputs become prune candidates."
                        .to_string(),
                )),
                Some(
                    "Exact call ids or visible Chunk IDs to preserve; keep-only calls prune other large outputs."
                        .to_string(),
                ),
            ),
        ),
        (
            "keep_commands".to_string(),
            JsonSchema::array(
                JsonSchema::string(Some(
                    "Command substrings that must remain useful. If no drop selectors are provided, all other large prunable outputs become prune candidates."
                        .to_string(),
                )),
                Some("Command fragments to preserve; keep-only calls prune other large outputs.".to_string()),
            ),
        ),
        (
            "reason".to_string(),
            JsonSchema::string(Some(
                "Short reason for the pruning decision; do not include hidden reasoning."
                    .to_string(),
            )),
        ),
    ]);

    ToolSpec::Function(ResponsesApiTool {
        name: TRIM_PROMPT_CONTEXT_TOOL_NAME.to_string(),
        description:
            "Conditionally prune already-consumed large tool outputs from prompt history. Use only when at least one large output is no longer needed; provide keep selectors for evidence still needed."
                .to_string(),
        strict: false,
        defer_loading: None,
        parameters: JsonSchema::object(properties, /*required*/ None, Some(false.into())),
        output_schema: None,
    })
}

#[cfg(test)]
#[path = "trim_prompt_context_spec_tests.rs"]
mod tests;
