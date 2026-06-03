use crate::context_manager::RelevancePruningFeedback;
use crate::context_manager::build_relevance_pruning_plan_for_feedback;
use crate::function_tool::FunctionCallError;
use crate::tools::context::FunctionToolOutput;
use crate::tools::context::ToolInvocation;
use crate::tools::context::ToolPayload;
use crate::tools::handlers::parse_arguments;
use crate::tools::handlers::trim_prompt_context_spec::TRIM_PROMPT_CONTEXT_TOOL_NAME;
use crate::tools::handlers::trim_prompt_context_spec::create_trim_prompt_context_tool;
use crate::tools::registry::ToolHandler;
use codex_tools::ToolName;
use codex_tools::ToolSpec;
use serde::Deserialize;
use serde_json::json;

pub struct TrimPromptContextHandler;

#[derive(Debug, Default, Deserialize)]
struct TrimPromptContextArgs {
    #[serde(default)]
    drop_call_ids: Vec<String>,
    #[serde(default)]
    drop_commands: Vec<String>,
    #[serde(default)]
    keep_call_ids: Vec<String>,
    #[serde(default)]
    keep_commands: Vec<String>,
    #[serde(default)]
    reason: Option<String>,
}

impl ToolHandler for TrimPromptContextHandler {
    type Output = FunctionToolOutput;

    fn tool_name(&self) -> ToolName {
        ToolName::plain(TRIM_PROMPT_CONTEXT_TOOL_NAME)
    }

    fn spec(&self) -> Option<ToolSpec> {
        Some(create_trim_prompt_context_tool())
    }

    async fn handle(&self, invocation: ToolInvocation) -> Result<Self::Output, FunctionCallError> {
        let ToolInvocation {
            session,
            turn,
            payload,
            ..
        } = invocation;

        let arguments = match payload {
            ToolPayload::Function { arguments } => arguments,
            _ => {
                return Err(FunctionCallError::RespondToModel(
                    "trim_prompt_context handler received unsupported payload".to_string(),
                ));
            }
        };

        let args: TrimPromptContextArgs = parse_arguments(&arguments)?;
        let feedback = RelevancePruningFeedback::from_tool_args(
            args.drop_call_ids,
            args.keep_call_ids,
            args.drop_commands,
            args.keep_commands,
        );
        let prompt_items = session
            .clone_history()
            .await
            .for_prompt(&turn.model_info.input_modalities);
        let plan = build_relevance_pruning_plan_for_feedback(
            &prompt_items,
            &turn.config.tool_output_relevance_pruning,
            &feedback,
        );
        let plan_stats = plan.stats;
        let stats = session
            .apply_tool_output_relevance_pruning_plan(turn.as_ref(), plan)
            .await;
        let saved_token_estimate = if stats.rewritten_outputs == 0 {
            0
        } else {
            plan_stats
                .original_token_estimate
                .saturating_sub(plan_stats.replacement_token_estimate)
        };

        let output = json!({
            "status": "ok",
            "candidate_outputs": plan_stats.candidate_outputs,
            "matched_hints": stats.matched_hints,
            "rewritten_outputs": stats.rewritten_outputs,
            "skipped_hints": stats.skipped_hints,
            "saved_token_estimate": saved_token_estimate,
            "reason": args.reason.unwrap_or_default(),
        });

        Ok(FunctionToolOutput::from_text(
            output.to_string(),
            Some(true),
        ))
    }
}
