use codex_utils_string::approx_token_count;
use codex_utils_string::approx_tokens_from_byte_count;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use sha1::Digest;
use sha1::Sha1;

const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PromptCriticality {
    Critical,
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolExposure {
    Direct,
    Deferred,
    Hosted,
    Search,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChainStatus {
    Entered,
    Skipped,
    Fallback,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStrength {
    Strong,
    Medium,
    Weak,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OptimizationAction {
    Keep,
    Compress,
    LazyLoad,
    RouteConditionally,
    DeleteCandidate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptSegmentTrace {
    pub segment_id: String,
    pub category: String,
    pub role: String,
    pub source: String,
    pub version: String,
    pub token_count_estimate: usize,
    pub byte_count: usize,
    pub criticality: PromptCriticality,
    pub trigger_condition: String,
    pub expected_behavior: Option<String>,
    pub dependencies: Vec<String>,
    pub content_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolExposureTrace {
    pub tool_id: String,
    pub name: String,
    pub namespace: Option<String>,
    pub schema_hash: String,
    pub token_count_estimate: usize,
    pub byte_count: usize,
    pub exposure: ToolExposure,
    pub source: String,
    pub called: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainCoverageTrace {
    pub chain_id: String,
    pub status: ChainStatus,
    pub evidence: EvidenceStrength,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptimizationFinding {
    pub target_id: String,
    pub target_kind: String,
    pub action: OptimizationAction,
    pub evidence: EvidenceStrength,
    pub reason: String,
    pub token_savings_estimate: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptObservabilitySnapshot {
    pub schema_version: u32,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub inference_call_id: Option<String>,
    pub model: String,
    pub prompt_segments: Vec<PromptSegmentTrace>,
    pub tools: Vec<ToolExposureTrace>,
    pub chains: Vec<ChainCoverageTrace>,
    pub input_token_count_estimate: usize,
    pub prompt_token_count_estimate: usize,
    pub tool_token_count_estimate: usize,
    pub total_token_count_estimate: usize,
    pub output_schema_present: bool,
    pub output_schema_strict: bool,
    pub parallel_tool_calls: bool,
    pub notes: Vec<String>,
}

pub struct PromptSnapshotInput<'a> {
    pub thread_id: Option<&'a str>,
    pub turn_id: Option<&'a str>,
    pub inference_call_id: Option<&'a str>,
    pub model: &'a str,
    pub base_instructions: &'a str,
    pub input_json: &'a [Value],
    pub tools_json: &'a [Value],
    pub output_schema_json: Option<&'a Value>,
    pub output_schema_present: bool,
    pub output_schema_strict: bool,
    pub parallel_tool_calls: bool,
}

#[derive(Clone, Copy)]
struct SegmentMarker {
    marker: &'static str,
    name: &'static str,
    category: &'static str,
    criticality: PromptCriticality,
    expected_behavior: Option<&'static str>,
}

const SEGMENT_MARKERS: &[SegmentMarker] = &[
    SegmentMarker {
        marker: "<permissions instructions>",
        name: "permissions",
        category: "permissions",
        criticality: PromptCriticality::Critical,
        expected_behavior: Some("Model follows configured approval and sandbox constraints."),
    },
    SegmentMarker {
        marker: "<apps_instructions>",
        name: "apps",
        category: "tools",
        criticality: PromptCriticality::Medium,
        expected_behavior: Some("Model understands app connector tool discovery behavior."),
    },
    SegmentMarker {
        marker: "<skills_instructions>",
        name: "skills",
        category: "capabilities",
        criticality: PromptCriticality::Medium,
        expected_behavior: Some("Model loads skills only when relevant."),
    },
    SegmentMarker {
        marker: "<plugins_instructions>",
        name: "plugins",
        category: "capabilities",
        criticality: PromptCriticality::Medium,
        expected_behavior: Some("Model understands enabled plugin capabilities."),
    },
    SegmentMarker {
        marker: "<collaboration_mode>",
        name: "collaboration_mode",
        category: "behavior",
        criticality: PromptCriticality::Medium,
        expected_behavior: Some("Model follows the active collaboration mode."),
    },
    SegmentMarker {
        marker: "<personality_spec>",
        name: "personality",
        category: "behavior",
        criticality: PromptCriticality::Low,
        expected_behavior: Some("Model follows configured personality guidance."),
    },
    SegmentMarker {
        marker: "<environment_context>",
        name: "environment_context",
        category: "runtime_context",
        criticality: PromptCriticality::High,
        expected_behavior: Some(
            "Model uses current cwd, sandbox, shell, and environment metadata.",
        ),
    },
    SegmentMarker {
        marker: "# AGENTS.md instructions for ",
        name: "agents_instructions",
        category: "project_instructions",
        criticality: PromptCriticality::High,
        expected_behavior: Some("Model follows project-local instructions."),
    },
    SegmentMarker {
        marker: "<realtime",
        name: "realtime",
        category: "mode_context",
        criticality: PromptCriticality::Medium,
        expected_behavior: Some("Model follows realtime-mode instructions."),
    },
];

pub fn build_prompt_observability_snapshot(
    input: PromptSnapshotInput<'_>,
) -> PromptObservabilitySnapshot {
    let mut prompt_segments = Vec::new();
    prompt_segments.push(base_instruction_segment(
        input.model,
        input.base_instructions,
    ));

    for item in input.input_json {
        prompt_segments.extend(prompt_segments_for_response_item(item));
    }
    if let Some(output_schema) = input.output_schema_json {
        prompt_segments.push(output_schema_segment(output_schema));
    }

    let tools = tool_traces_from_json(input.tools_json);
    let prompt_token_count_estimate: usize = prompt_segments
        .iter()
        .map(|segment| segment.token_count_estimate)
        .sum();
    let tool_token_count_estimate: usize = tools.iter().map(|tool| tool.token_count_estimate).sum();
    let input_token_count_estimate: usize = input
        .input_json
        .iter()
        .map(estimate_response_item_tokens)
        .sum();
    let total_token_count_estimate =
        prompt_token_count_estimate.saturating_add(tool_token_count_estimate);
    let chains = chain_traces(&tools);

    PromptObservabilitySnapshot {
        schema_version: SNAPSHOT_SCHEMA_VERSION,
        thread_id: input.thread_id.map(str::to_string),
        turn_id: input.turn_id.map(str::to_string),
        inference_call_id: input.inference_call_id.map(str::to_string),
        model: input.model.to_string(),
        prompt_segments,
        tools,
        chains,
        input_token_count_estimate,
        prompt_token_count_estimate,
        tool_token_count_estimate,
        total_token_count_estimate,
        output_schema_present: input.output_schema_present,
        output_schema_strict: input.output_schema_strict,
        parallel_tool_calls: input.parallel_tool_calls,
        notes: vec![
            "Token counts are byte-based estimates; provider usage remains authoritative."
                .to_string(),
            "Prompt text is not stored in this compact snapshot; hashes identify content."
                .to_string(),
        ],
    }
}

pub fn stable_hash(value: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(value.as_bytes());
    let digest = hasher.finalize();
    digest
        .iter()
        .take(4)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn estimate_json_tokens(value: &Value) -> usize {
    let byte_count = serde_json::to_vec(value).map_or(0, |bytes| bytes.len());
    usize::try_from(approx_tokens_from_byte_count(byte_count)).unwrap_or(usize::MAX)
}

pub fn utility_score(
    behavior_impact_score: f64,
    usage_frequency: f64,
    quality_delta: f64,
    token_cost: usize,
) -> f64 {
    if token_cost == 0 {
        return behavior_impact_score * usage_frequency * quality_delta;
    }
    behavior_impact_score * usage_frequency * quality_delta / token_cost as f64
}

fn base_instruction_segment(model: &str, text: &str) -> PromptSegmentTrace {
    let content_hash = stable_hash(text);
    PromptSegmentTrace {
        segment_id: format!("base:model:{}:{content_hash}", slugify(model)),
        category: "base_instructions".to_string(),
        role: "instructions".to_string(),
        source: "responses.instructions".to_string(),
        version: "v1".to_string(),
        token_count_estimate: approx_token_count(text),
        byte_count: text.len(),
        criticality: PromptCriticality::High,
        trigger_condition: "always".to_string(),
        expected_behavior: Some("Model follows the base Codex agent contract.".to_string()),
        dependencies: Vec::new(),
        content_hash,
    }
}

fn prompt_segments_for_response_item(item: &Value) -> Vec<PromptSegmentTrace> {
    match item.get("type").and_then(Value::as_str) {
        Some("message") => {
            let role = item
                .get("role")
                .and_then(Value::as_str)
                .unwrap_or("unknown");
            item.get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .flat_map(|content| prompt_segments_for_content_item(role, content))
                .collect()
        }
        Some("function_call_output") | Some("custom_tool_call_output") => item
            .get("output")
            .and_then(text_from_output_value)
            .map(|text| {
                vec![context_segment(ContextSegmentInput {
                    name: "tool_output",
                    category: "tool_result",
                    role: "tool",
                    source: "message.tool_output",
                    text: &text,
                    criticality: PromptCriticality::Medium,
                    trigger_condition: "tool result retained in history",
                    expected_behavior: Some("Model can use previous tool results."),
                })]
            })
            .unwrap_or_default(),
        Some("tool_search_output") => vec![context_segment(ContextSegmentInput {
            name: "tool_search_output",
            category: "tool_result",
            role: "tool",
            source: "message.tool_search_output",
            text: &serde_json::to_string(item.get("tools").unwrap_or(&Value::Null))
                .unwrap_or_default(),
            criticality: PromptCriticality::Medium,
            trigger_condition: "tool_search result retained in history",
            expected_behavior: Some("Model can use lazy-loaded tool search results."),
        })],
        Some(_) | None => Vec::new(),
    }
}

fn prompt_segments_for_content_item(role: &str, content: &Value) -> Vec<PromptSegmentTrace> {
    match content.get("type").and_then(Value::as_str) {
        Some("input_text") | Some("output_text") => content
            .get("text")
            .and_then(Value::as_str)
            .map(|text| prompt_segments_for_text(role, text))
            .unwrap_or_default(),
        Some("input_image") => content
            .get("image_url")
            .and_then(Value::as_str)
            .map(|image_url| {
                vec![context_segment(ContextSegmentInput {
                    name: "image_context",
                    category: "media",
                    role,
                    source: "message.input_image",
                    text: image_url,
                    criticality: PromptCriticality::Medium,
                    trigger_condition: "image attached to prompt",
                    expected_behavior: Some("Model can inspect attached image context."),
                })]
            })
            .unwrap_or_default(),
        Some(_) | None => Vec::new(),
    }
}

fn text_from_output_value(output: &Value) -> Option<String> {
    if let Some(text) = output.as_str() {
        return Some(text.to_string());
    }
    if let Some(items) = output.as_array() {
        let text = items
            .iter()
            .filter_map(|item| item.get("text").and_then(Value::as_str))
            .collect::<Vec<_>>()
            .join("\n");
        return (!text.is_empty()).then_some(text);
    }
    None
}

fn prompt_segments_for_text(role: &str, text: &str) -> Vec<PromptSegmentTrace> {
    let mut positions = SEGMENT_MARKERS
        .iter()
        .filter_map(|marker| text.find(marker.marker).map(|index| (index, *marker)))
        .collect::<Vec<_>>();
    positions.sort_by_key(|(index, _)| *index);

    if positions.is_empty() {
        return vec![fallback_text_segment(role, text)];
    }

    let mut segments = Vec::new();
    if positions[0].0 > 0 {
        let prefix = text[..positions[0].0].trim();
        if !prefix.is_empty() {
            segments.push(fallback_text_segment(role, prefix));
        }
    }

    for index in 0..positions.len() {
        let (start, marker) = positions[index];
        let end = positions
            .get(index + 1)
            .map(|(next_start, _)| *next_start)
            .unwrap_or(text.len());
        let slice = text[start..end].trim();
        if slice.is_empty() {
            continue;
        }
        segments.push(context_segment(ContextSegmentInput {
            name: marker.name,
            category: marker.category,
            role,
            source: "message.text",
            text: slice,
            criticality: marker.criticality,
            trigger_condition: "injected when corresponding runtime context is enabled",
            expected_behavior: marker.expected_behavior,
        }));
    }

    segments
}

fn fallback_text_segment(role: &str, text: &str) -> PromptSegmentTrace {
    let (name, category, criticality, trigger_condition, expected_behavior) = match role {
        "developer" => (
            "developer_instructions",
            "developer_instructions",
            PromptCriticality::High,
            "developer message present",
            Some("Model follows developer-provided instructions."),
        ),
        "user" => (
            "user_input_or_context",
            "user_input",
            PromptCriticality::High,
            "user/context message present",
            Some("Model responds to user-visible task context."),
        ),
        "assistant" => (
            "assistant_history",
            "conversation_history",
            PromptCriticality::Medium,
            "assistant history retained",
            Some("Model can use prior assistant outputs."),
        ),
        _ => (
            "conversation_history",
            "conversation_history",
            PromptCriticality::Medium,
            "conversation history retained",
            Some("Model can use prior conversation context."),
        ),
    };
    context_segment(ContextSegmentInput {
        name,
        category,
        role,
        source: "message.text",
        text,
        criticality,
        trigger_condition,
        expected_behavior,
    })
}

struct ContextSegmentInput<'a> {
    name: &'a str,
    category: &'a str,
    role: &'a str,
    source: &'a str,
    text: &'a str,
    criticality: PromptCriticality,
    trigger_condition: &'a str,
    expected_behavior: Option<&'a str>,
}

fn context_segment(input: ContextSegmentInput<'_>) -> PromptSegmentTrace {
    let content_hash = stable_hash(input.text);
    PromptSegmentTrace {
        segment_id: format!("context:{}:v1", slugify(input.name)),
        category: input.category.to_string(),
        role: input.role.to_string(),
        source: input.source.to_string(),
        version: "v1".to_string(),
        token_count_estimate: approx_token_count(input.text),
        byte_count: input.text.len(),
        criticality: input.criticality,
        trigger_condition: input.trigger_condition.to_string(),
        expected_behavior: input.expected_behavior.map(str::to_string),
        dependencies: Vec::new(),
        content_hash,
    }
}

fn output_schema_segment(schema: &Value) -> PromptSegmentTrace {
    let schema_text = serde_json::to_string(schema).unwrap_or_default();
    let content_hash = stable_hash(&schema_text);
    PromptSegmentTrace {
        segment_id: "context:output_schema:v1".to_string(),
        category: "output_format".to_string(),
        role: "instructions".to_string(),
        source: "responses.text.format.schema".to_string(),
        version: "v1".to_string(),
        token_count_estimate: estimate_json_tokens(schema),
        byte_count: schema_text.len(),
        criticality: PromptCriticality::High,
        trigger_condition: "output schema configured".to_string(),
        expected_behavior: Some(
            "Provider and model constrain output to the configured schema.".to_string(),
        ),
        dependencies: Vec::new(),
        content_hash,
    }
}

fn estimate_response_item_tokens(item: &Value) -> usize {
    serde_json::to_string(item)
        .map(|value| approx_token_count(&value))
        .unwrap_or_default()
}

fn tool_traces_from_json(tools_json: &[Value]) -> Vec<ToolExposureTrace> {
    tools_json
        .iter()
        .flat_map(tool_traces_for_value)
        .collect::<Vec<_>>()
}

fn tool_traces_for_value(value: &Value) -> Vec<ToolExposureTrace> {
    let Some(tool_type) = value.get("type").and_then(Value::as_str) else {
        return vec![tool_trace(
            "unknown",
            None,
            value,
            ToolExposure::Direct,
            "responses.tools",
        )];
    };

    match tool_type {
        "namespace" => {
            let namespace = value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("namespace");
            value
                .get("tools")
                .and_then(Value::as_array)
                .map(|tools| {
                    tools
                        .iter()
                        .map(|tool| {
                            let name = tool
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("namespace_tool");
                            tool_trace(
                                name,
                                Some(namespace),
                                tool,
                                exposure_for_tool(tool),
                                "responses.tools.namespace",
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| {
                    vec![tool_trace(
                        namespace,
                        None,
                        value,
                        ToolExposure::Direct,
                        "responses.tools.namespace",
                    )]
                })
        }
        "function" | "custom" => {
            let name = value
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or(tool_type);
            vec![tool_trace(
                name,
                None,
                value,
                exposure_for_tool(value),
                "responses.tools",
            )]
        }
        "tool_search" => vec![tool_trace(
            "tool_search",
            None,
            value,
            ToolExposure::Search,
            "responses.tools",
        )],
        "local_shell" | "web_search" | "image_generation" => {
            vec![tool_trace(
                tool_type,
                None,
                value,
                ToolExposure::Hosted,
                "responses.tools",
            )]
        }
        other => vec![tool_trace(
            other,
            None,
            value,
            ToolExposure::Direct,
            "responses.tools",
        )],
    }
}

fn exposure_for_tool(value: &Value) -> ToolExposure {
    if value
        .get("defer_loading")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        ToolExposure::Deferred
    } else {
        ToolExposure::Direct
    }
}

fn tool_trace(
    name: &str,
    namespace: Option<&str>,
    schema: &Value,
    exposure: ToolExposure,
    source: &str,
) -> ToolExposureTrace {
    let schema_text = serde_json::to_string(schema).unwrap_or_default();
    let schema_hash = stable_hash(&schema_text);
    let namespace_id = namespace.unwrap_or("global");
    ToolExposureTrace {
        tool_id: format!(
            "tool:{}:{}:{schema_hash}",
            slugify(namespace_id),
            slugify(name)
        ),
        name: name.to_string(),
        namespace: namespace.map(str::to_string),
        schema_hash,
        token_count_estimate: estimate_json_tokens(schema),
        byte_count: schema_text.len(),
        exposure,
        source: source.to_string(),
        called: None,
    }
}

fn chain_traces(tools: &[ToolExposureTrace]) -> Vec<ChainCoverageTrace> {
    let tool_search_visible = tools.iter().any(|tool| tool.name == "tool_search");
    let mut chains = vec![ChainCoverageTrace {
        chain_id: "chain:responses_request:v1".to_string(),
        status: ChainStatus::Entered,
        evidence: EvidenceStrength::Strong,
        reason: Some("Responses request was built.".to_string()),
    }];
    chains.push(ChainCoverageTrace {
        chain_id: "chain:tool_search:v1".to_string(),
        status: if tool_search_visible {
            ChainStatus::Entered
        } else {
            ChainStatus::Skipped
        },
        evidence: if tool_search_visible {
            EvidenceStrength::Medium
        } else {
            EvidenceStrength::Weak
        },
        reason: Some(if tool_search_visible {
            "tool_search is model-visible for deferred tools.".to_string()
        } else {
            "tool_search is not model-visible on this request.".to_string()
        }),
    });
    chains
}

fn slugify(value: &str) -> String {
    let slug = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .to_string();
    if slug.is_empty() {
        "unknown".to_string()
    } else {
        slug
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;
    use serde_json::json;

    #[test]
    fn stable_hash_is_short_and_stable() {
        assert_eq!(stable_hash("abc"), "a9993e36");
    }

    #[test]
    fn segment_ids_use_expected_shapes() {
        let snapshot = build_prompt_observability_snapshot(PromptSnapshotInput {
            thread_id: Some("thread-1"),
            turn_id: Some("turn-1"),
            inference_call_id: Some("call-1"),
            model: "gpt-5.4",
            base_instructions: "base",
            input_json: &[json!({
                "type": "message",
                "role": "developer",
                "content": [{
                    "type": "input_text",
                    "text": "<permissions instructions>be careful</permissions instructions>"
                }]
            })],
            tools_json: &[json!({
                "type": "function",
                "name": "run_tests",
                "description": "Run tests",
                "parameters": {"type": "object"}
            })],
            output_schema_json: None,
            output_schema_present: false,
            output_schema_strict: true,
            parallel_tool_calls: true,
        });

        assert_eq!(
            snapshot
                .prompt_segments
                .iter()
                .map(|segment| segment.segment_id.as_str())
                .collect::<Vec<_>>(),
            vec!["base:model:gpt_5_4:1405df66", "context:permissions:v1"]
        );
        assert_eq!(snapshot.tools[0].tool_id, "tool:global:run_tests:581ec80c");
    }

    #[test]
    fn output_schema_is_tracked_as_a_prompt_segment() {
        let output_schema = json!({
            "type": "object",
            "properties": {"answer": {"type": "string"}},
            "required": ["answer"],
            "additionalProperties": false
        });
        let snapshot = build_prompt_observability_snapshot(PromptSnapshotInput {
            thread_id: None,
            turn_id: None,
            inference_call_id: None,
            model: "gpt-5.4",
            base_instructions: "base",
            input_json: &[],
            tools_json: &[],
            output_schema_json: Some(&output_schema),
            output_schema_present: true,
            output_schema_strict: true,
            parallel_tool_calls: false,
        });

        let output_schema_segment = snapshot
            .prompt_segments
            .iter()
            .find(|segment| segment.segment_id == "context:output_schema:v1")
            .expect("output schema segment");
        assert_eq!(output_schema_segment.category, "output_format");
        assert_eq!(
            output_schema_segment.token_count_estimate,
            estimate_json_tokens(&output_schema)
        );
    }

    #[test]
    fn token_estimate_uses_existing_byte_based_counter() {
        assert_eq!(estimate_json_tokens(&json!({"abcd": "efgh"})), 4);
    }

    #[test]
    fn utility_score_penalizes_token_cost() {
        assert_eq!(utility_score(1.0, 0.5, 0.5, 10), 0.025);
        assert_eq!(utility_score(1.0, 0.5, 0.5, 0), 0.25);
    }
}
