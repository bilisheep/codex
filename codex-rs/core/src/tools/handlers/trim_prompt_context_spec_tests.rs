use super::*;
use codex_tools::ToolSpec;
use pretty_assertions::assert_eq;

#[test]
fn trim_prompt_context_tool_declares_expected_name() {
    let ToolSpec::Function(tool) = create_trim_prompt_context_tool() else {
        panic!("trim_prompt_context should be a function tool");
    };

    assert_eq!(tool.name, TRIM_PROMPT_CONTEXT_TOOL_NAME);
    assert!(
        tool.description
            .contains("Conditionally prune already-consumed large tool outputs")
    );
}
