use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs::File;
use std::io::BufRead;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;

use anyhow::Context;
use anyhow::Result;
use serde::Deserialize;
use serde_json::Value;

use crate::PROMPT_OBSERVABILITY_SNAPSHOT_EVENT_KIND;
use crate::report::ChainCoverageSummary;
use crate::report::DeferredSearchSummary;
use crate::report::SegmentSummary;
use crate::report::TokenWaterfallRow;
use crate::report::ToolSummary;
use crate::report::TraceAnalysisReport;
use crate::snapshot::ChainStatus;
use crate::snapshot::EvidenceStrength;
use crate::snapshot::OptimizationAction;
use crate::snapshot::OptimizationFinding;
use crate::snapshot::PromptCriticality;
use crate::snapshot::PromptObservabilitySnapshot;
use crate::snapshot::PromptSegmentTrace;
use crate::snapshot::ToolExposure;
use crate::snapshot::ToolExposureTrace;

const MANIFEST_FILE_NAME: &str = "manifest.json";
const RAW_EVENT_LOG_FILE_NAME: &str = "trace.jsonl";

pub fn analyze_trace_path(path: impl AsRef<Path>) -> Result<TraceAnalysisReport> {
    let bundles = discover_trace_bundles(path.as_ref())?;
    let mut accumulator = AnalysisAccumulator::default();
    for bundle in &bundles {
        analyze_bundle(bundle, &mut accumulator)?;
    }
    Ok(accumulator.into_report(bundles.len()))
}

fn discover_trace_bundles(path: &Path) -> Result<Vec<PathBuf>> {
    let path = if path.is_file() {
        path.parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        path.to_path_buf()
    };

    if path.join(MANIFEST_FILE_NAME).is_file() && path.join(RAW_EVENT_LOG_FILE_NAME).is_file() {
        return Ok(vec![path]);
    }

    let mut bundles = Vec::new();
    for entry in std::fs::read_dir(&path).with_context(|| format!("read {}", path.display()))? {
        let entry = entry?;
        let candidate = entry.path();
        if candidate.join(MANIFEST_FILE_NAME).is_file()
            && candidate.join(RAW_EVENT_LOG_FILE_NAME).is_file()
        {
            bundles.push(candidate);
        }
    }
    bundles.sort();
    Ok(bundles)
}

fn analyze_bundle(bundle: &Path, accumulator: &mut AnalysisAccumulator) -> Result<()> {
    let event_log_path = bundle.join(RAW_EVENT_LOG_FILE_NAME);
    let event_log = File::open(&event_log_path)
        .with_context(|| format!("open trace event log {}", event_log_path.display()))?;
    for (line_index, line) in BufReader::new(event_log).lines().enumerate() {
        let line = line.with_context(|| format!("read trace event line {}", line_index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: RawTraceEvent = serde_json::from_str(&line)
            .with_context(|| format!("parse trace event line {}", line_index + 1))?;
        analyze_event(bundle, event, accumulator)?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct RawTraceEvent {
    payload: RawTraceEventPayload,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum RawTraceEventPayload {
    Other {
        kind: String,
        payloads: Vec<RawPayloadRef>,
    },
    ToolCallStarted {
        kind: ToolCallKind,
        invocation_payload: Option<RawPayloadRef>,
    },
    ToolCallEnded {
        result_payload: Option<RawPayloadRef>,
    },
    #[serde(other)]
    Ignored,
}

#[derive(Debug, Deserialize)]
struct RawPayloadRef {
    raw_payload_id: String,
    path: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
enum ToolCallKind {
    ExecCommand,
    WriteStdin,
    ApplyPatch,
    Mcp {
        server: String,
        tool: String,
    },
    Web,
    ImageGeneration,
    SpawnAgent,
    AssignAgentTask,
    SendMessage,
    WaitAgent,
    CloseAgent,
    Other {
        name: String,
    },
    #[serde(other)]
    Unknown,
}

fn analyze_event(
    bundle: &Path,
    event: RawTraceEvent,
    accumulator: &mut AnalysisAccumulator,
) -> Result<()> {
    match event.payload {
        RawTraceEventPayload::Other { kind, payloads, .. }
            if kind == PROMPT_OBSERVABILITY_SNAPSHOT_EVENT_KIND =>
        {
            for payload in payloads {
                let json = read_payload_json(bundle, &payload)?;
                let snapshot: PromptObservabilitySnapshot = serde_json::from_value(json)
                    .with_context(|| format!("parse {}", payload.raw_payload_id))?;
                accumulator.record_snapshot(snapshot);
            }
        }
        RawTraceEventPayload::ToolCallStarted {
            kind,
            invocation_payload,
            ..
        } => {
            if let Some(payload) = invocation_payload {
                let json = read_payload_json(bundle, &payload)?;
                accumulator.record_tool_invocation(&json);
            } else {
                accumulator.record_tool_kind(&kind);
            }
        }
        RawTraceEventPayload::ToolCallEnded { result_payload, .. } => {
            if let Some(payload) = result_payload {
                let json = read_payload_json(bundle, &payload)?;
                accumulator.record_tool_result(&json);
            }
        }
        RawTraceEventPayload::Other { .. } | RawTraceEventPayload::Ignored => {}
    }
    Ok(())
}

fn read_payload_json(bundle: &Path, payload: &RawPayloadRef) -> Result<Value> {
    let payload_path = bundle.join(&payload.path);
    let file = File::open(&payload_path)
        .with_context(|| format!("open payload {}", payload_path.display()))?;
    serde_json::from_reader(file)
        .with_context(|| format!("parse payload {}", payload_path.display()))
}

#[derive(Default)]
struct AnalysisAccumulator {
    snapshots_analyzed: usize,
    segments: BTreeMap<String, SegmentAggregate>,
    tools: BTreeMap<String, ToolAggregate>,
    chains: BTreeMap<String, ChainAggregate>,
    called_tools: BTreeMap<String, usize>,
    deferred_search_results: BTreeMap<String, DeferredSearchAggregate>,
}

#[derive(Clone)]
struct SegmentAggregate {
    category: String,
    criticality: PromptCriticality,
    token_count_estimate: usize,
    occurrences: usize,
}

#[derive(Clone)]
struct ToolAggregate {
    name: String,
    namespace: Option<String>,
    exposure: ToolExposure,
    token_count_estimate: usize,
    exposed_count: usize,
}

#[derive(Default)]
struct ChainAggregate {
    entered: usize,
    skipped: usize,
    fallback: usize,
}

struct DeferredSearchAggregate {
    name: String,
    namespace: Option<String>,
    search_result_count: usize,
}

impl AnalysisAccumulator {
    fn record_snapshot(&mut self, snapshot: PromptObservabilitySnapshot) {
        self.snapshots_analyzed += 1;
        for segment in snapshot.prompt_segments {
            self.record_segment(segment);
        }
        for tool in snapshot.tools {
            self.record_tool(tool);
        }
        for chain in snapshot.chains {
            let entry = self.chains.entry(chain.chain_id).or_default();
            match chain.status {
                ChainStatus::Entered => entry.entered += 1,
                ChainStatus::Skipped => entry.skipped += 1,
                ChainStatus::Fallback => entry.fallback += 1,
            }
        }
    }

    fn record_segment(&mut self, segment: PromptSegmentTrace) {
        let entry = self
            .segments
            .entry(segment.segment_id)
            .or_insert_with(|| SegmentAggregate {
                category: segment.category,
                criticality: segment.criticality,
                token_count_estimate: 0,
                occurrences: 0,
            });
        entry.token_count_estimate = entry
            .token_count_estimate
            .saturating_add(segment.token_count_estimate);
        entry.occurrences += 1;
    }

    fn record_tool(&mut self, tool: ToolExposureTrace) {
        let entry = self
            .tools
            .entry(tool.tool_id)
            .or_insert_with(|| ToolAggregate {
                name: tool.name,
                namespace: tool.namespace,
                exposure: tool.exposure,
                token_count_estimate: 0,
                exposed_count: 0,
            });
        entry.token_count_estimate = entry
            .token_count_estimate
            .saturating_add(tool.token_count_estimate);
        entry.exposed_count += 1;
    }

    fn record_tool_invocation(&mut self, invocation: &Value) {
        let name = invocation.get("tool_name").and_then(Value::as_str);
        let namespace = invocation.get("tool_namespace").and_then(Value::as_str);
        if let Some(name) = name {
            self.record_called_tool(namespace, name);
        }
        if invocation
            .get("payload")
            .and_then(|payload| payload.get("type"))
            .and_then(Value::as_str)
            == Some("tool_search")
        {
            self.record_called_tool(None, "tool_search");
        }
    }

    fn record_tool_kind(&mut self, kind: &ToolCallKind) {
        match kind {
            ToolCallKind::ExecCommand => self.record_called_tool(None, "local_shell"),
            ToolCallKind::WriteStdin => self.record_called_tool(None, "write_stdin"),
            ToolCallKind::ApplyPatch => self.record_called_tool(None, "apply_patch"),
            ToolCallKind::Mcp { server, tool } => {
                self.record_called_tool(Some(server), tool);
                self.record_called_tool(None, tool);
            }
            ToolCallKind::Web => self.record_called_tool(None, "web_search"),
            ToolCallKind::ImageGeneration => self.record_called_tool(None, "image_generation"),
            ToolCallKind::SpawnAgent => self.record_called_tool(None, "spawn_agent"),
            ToolCallKind::AssignAgentTask => self.record_called_tool(None, "assign_agent_task"),
            ToolCallKind::SendMessage => self.record_called_tool(None, "send_message"),
            ToolCallKind::WaitAgent => self.record_called_tool(None, "wait_agent"),
            ToolCallKind::CloseAgent => self.record_called_tool(None, "close_agent"),
            ToolCallKind::Other { name } => self.record_called_tool(None, name),
            ToolCallKind::Unknown => {}
        }
    }

    fn record_tool_result(&mut self, result: &Value) {
        let Some(response_item) = result.get("response_item") else {
            return;
        };
        if response_item.get("type").and_then(Value::as_str) != Some("tool_search_output") {
            return;
        }
        let Some(tools) = response_item.get("tools").and_then(Value::as_array) else {
            return;
        };
        for tool in tools {
            for (namespace, name) in loadable_tool_names(tool) {
                let key = tool_key(namespace.as_deref(), &name);
                let entry = self.deferred_search_results.entry(key).or_insert_with(|| {
                    DeferredSearchAggregate {
                        name: name.clone(),
                        namespace: namespace.clone(),
                        search_result_count: 0,
                    }
                });
                entry.search_result_count += 1;
            }
        }
    }

    fn record_called_tool(&mut self, namespace: Option<&str>, name: &str) {
        let mut keys = BTreeSet::new();
        keys.insert(tool_key(namespace, name));
        keys.insert(tool_key(None, name));
        if let Some(namespace) = namespace {
            keys.insert(format!("{namespace}.{name}"));
            keys.insert(format!("{namespace}{name}"));
        }
        for key in keys {
            *self.called_tools.entry(key).or_default() += 1;
        }
    }

    fn into_report(self, bundles_analyzed: usize) -> TraceAnalysisReport {
        let any_tools_called = !self.called_tools.is_empty();
        let mut segment_summaries = self
            .segments
            .iter()
            .map(|(segment_id, aggregate)| SegmentSummary {
                segment_id: segment_id.clone(),
                category: aggregate.category.clone(),
                criticality: aggregate.criticality,
                token_count_estimate: aggregate.token_count_estimate,
                occurrences: aggregate.occurrences,
                evidence: segment_evidence(&aggregate.category, any_tools_called),
            })
            .collect::<Vec<_>>();
        segment_summaries.sort_by(|left, right| {
            right
                .token_count_estimate
                .cmp(&left.token_count_estimate)
                .then_with(|| left.segment_id.cmp(&right.segment_id))
        });

        let mut tool_summaries = self
            .tools
            .iter()
            .map(|(tool_id, aggregate)| {
                let called_count = called_count_for(
                    &self.called_tools,
                    aggregate.namespace.as_deref(),
                    &aggregate.name,
                );
                ToolSummary {
                    tool_id: tool_id.clone(),
                    name: aggregate.name.clone(),
                    namespace: aggregate.namespace.clone(),
                    exposure: aggregate.exposure,
                    token_count_estimate: aggregate.token_count_estimate,
                    exposed_count: aggregate.exposed_count,
                    called_count,
                }
            })
            .collect::<Vec<_>>();
        tool_summaries.sort_by(|left, right| {
            right
                .token_count_estimate
                .cmp(&left.token_count_estimate)
                .then_with(|| left.tool_id.cmp(&right.tool_id))
        });

        let low_evidence_segments = segment_summaries
            .iter()
            .filter(|segment| {
                matches!(
                    segment.evidence,
                    EvidenceStrength::None | EvidenceStrength::Weak
                ) && matches!(
                    segment.criticality,
                    PromptCriticality::Medium | PromptCriticality::Low
                ) && segment.token_count_estimate >= 64
            })
            .cloned()
            .collect::<Vec<_>>();

        let exposed_but_never_called_tools = tool_summaries
            .iter()
            .filter(|tool| tool.called_count == 0)
            .cloned()
            .collect::<Vec<_>>();

        let mut deferred_but_searched_tools = self
            .deferred_search_results
            .into_values()
            .map(|aggregate| DeferredSearchSummary {
                name: aggregate.name,
                namespace: aggregate.namespace,
                search_result_count: aggregate.search_result_count,
            })
            .collect::<Vec<_>>();
        deferred_but_searched_tools.sort_by(|left, right| {
            right
                .search_result_count
                .cmp(&left.search_result_count)
                .then_with(|| left.name.cmp(&right.name))
        });

        let mut chain_coverage = self
            .chains
            .into_iter()
            .map(|(chain_id, aggregate)| ChainCoverageSummary {
                chain_id,
                entered: aggregate.entered,
                skipped: aggregate.skipped,
                fallback: aggregate.fallback,
            })
            .collect::<Vec<_>>();
        chain_coverage.sort_by(|left, right| left.chain_id.cmp(&right.chain_id));

        let token_waterfall = token_waterfall(&segment_summaries, &tool_summaries);
        let findings = findings(&segment_summaries, &tool_summaries);

        TraceAnalysisReport {
            bundles_analyzed,
            snapshots_analyzed: self.snapshots_analyzed,
            token_waterfall,
            top_token_heavy_segments: segment_summaries,
            low_evidence_segments,
            exposed_but_never_called_tools,
            deferred_but_searched_tools,
            chain_coverage,
            findings,
        }
    }
}

fn token_waterfall(segments: &[SegmentSummary], tools: &[ToolSummary]) -> Vec<TokenWaterfallRow> {
    let mut rows = segments
        .iter()
        .map(|segment| TokenWaterfallRow {
            id: segment.segment_id.clone(),
            label: segment.segment_id.clone(),
            category: segment.category.clone(),
            token_count_estimate: segment.token_count_estimate,
            occurrences: segment.occurrences,
        })
        .collect::<Vec<_>>();
    rows.extend(tools.iter().map(|tool| TokenWaterfallRow {
        id: tool.tool_id.clone(),
        label: display_tool_name(tool.namespace.as_deref(), &tool.name),
        category: "tool_schema".to_string(),
        token_count_estimate: tool.token_count_estimate,
        occurrences: tool.exposed_count,
    }));
    rows.sort_by(|left, right| {
        right
            .token_count_estimate
            .cmp(&left.token_count_estimate)
            .then_with(|| left.id.cmp(&right.id))
    });
    rows
}

fn findings(segments: &[SegmentSummary], tools: &[ToolSummary]) -> Vec<OptimizationFinding> {
    let mut findings = Vec::new();
    for segment in segments {
        if matches!(
            segment.criticality,
            PromptCriticality::Critical | PromptCriticality::High
        ) && segment.token_count_estimate >= 500
        {
            findings.push(OptimizationFinding {
                target_id: segment.segment_id.clone(),
                target_kind: "prompt_segment".to_string(),
                action: OptimizationAction::Compress,
                evidence: segment.evidence,
                reason: "High-criticality prompt segment is expensive; report compression only."
                    .to_string(),
                token_savings_estimate: segment.token_count_estimate / 3,
            });
            continue;
        }
        if matches!(
            segment.evidence,
            EvidenceStrength::None | EvidenceStrength::Weak
        ) && matches!(
            segment.criticality,
            PromptCriticality::Medium | PromptCriticality::Low
        ) && segment.token_count_estimate >= 64
        {
            let action = if segment.criticality == PromptCriticality::Low {
                OptimizationAction::DeleteCandidate
            } else {
                OptimizationAction::Compress
            };
            findings.push(OptimizationFinding {
                target_id: segment.segment_id.clone(),
                target_kind: "prompt_segment".to_string(),
                action,
                evidence: segment.evidence,
                reason: "Segment has low behavioral evidence in analyzed traces.".to_string(),
                token_savings_estimate: segment.token_count_estimate / 2,
            });
        }
    }
    for tool in tools {
        if tool.called_count == 0 && tool.token_count_estimate > 0 {
            findings.push(OptimizationFinding {
                target_id: tool.tool_id.clone(),
                target_kind: "tool_schema".to_string(),
                action: match tool.exposure {
                    ToolExposure::Direct => OptimizationAction::LazyLoad,
                    ToolExposure::Deferred => OptimizationAction::Keep,
                    ToolExposure::Hosted | ToolExposure::Search => {
                        OptimizationAction::RouteConditionally
                    }
                },
                evidence: EvidenceStrength::Strong,
                reason: "Tool schema was exposed but no matching dispatch was observed."
                    .to_string(),
                token_savings_estimate: tool.token_count_estimate,
            });
        }
    }
    findings.sort_by(|left, right| {
        right
            .token_savings_estimate
            .cmp(&left.token_savings_estimate)
            .then_with(|| left.target_id.cmp(&right.target_id))
    });
    findings
}

fn segment_evidence(category: &str, any_tools_called: bool) -> EvidenceStrength {
    match category {
        "tool_result" => EvidenceStrength::Strong,
        "tools" | "capabilities" if any_tools_called => EvidenceStrength::Medium,
        "tools" | "capabilities" => EvidenceStrength::Weak,
        "output_format" => EvidenceStrength::Medium,
        "user_input" | "project_instructions" | "runtime_context" => EvidenceStrength::Weak,
        "permissions" | "base_instructions" | "developer_instructions" => EvidenceStrength::Weak,
        "behavior" | "mode_context" | "conversation_history" | "media" => EvidenceStrength::None,
        _ => EvidenceStrength::None,
    }
}

fn called_count_for(
    called_tools: &BTreeMap<String, usize>,
    namespace: Option<&str>,
    name: &str,
) -> usize {
    let keys = [
        tool_key(namespace, name),
        tool_key(None, name),
        namespace
            .map(|namespace| format!("{namespace}.{name}"))
            .unwrap_or_default(),
        namespace
            .map(|namespace| format!("{namespace}{name}"))
            .unwrap_or_default(),
    ];
    keys.iter()
        .filter_map(|key| called_tools.get(key))
        .copied()
        .max()
        .unwrap_or_default()
}

fn loadable_tool_names(value: &Value) -> Vec<(Option<String>, String)> {
    match value.get("type").and_then(Value::as_str) {
        Some("function") => value
            .get("name")
            .and_then(Value::as_str)
            .map(|name| vec![(None, name.to_string())])
            .unwrap_or_default(),
        Some("namespace") => {
            let namespace = value
                .get("name")
                .and_then(Value::as_str)
                .map(str::to_string);
            value
                .get("tools")
                .and_then(Value::as_array)
                .map(|tools| {
                    tools
                        .iter()
                        .filter_map(|tool| {
                            tool.get("name")
                                .and_then(Value::as_str)
                                .map(|name| (namespace.clone(), name.to_string()))
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        }
        Some(_) | None => Vec::new(),
    }
}

fn display_tool_name(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(namespace) => format!("{namespace}.{name}"),
        None => name.to_string(),
    }
}

fn tool_key(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(namespace) => format!("{namespace}/{name}"),
        None => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snapshot::ChainCoverageTrace;
    use crate::snapshot::PromptObservabilitySnapshot;
    use crate::snapshot::PromptSegmentTrace;
    use crate::snapshot::ToolExposureTrace;
    use pretty_assertions::assert_eq;
    use serde_json::json;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn report_sorts_token_waterfall_and_findings() {
        let mut accumulator = AnalysisAccumulator::default();
        accumulator.record_snapshot(PromptObservabilitySnapshot {
            schema_version: 1,
            thread_id: Some("thread".to_string()),
            turn_id: Some("turn".to_string()),
            inference_call_id: Some("inference".to_string()),
            model: "model".to_string(),
            prompt_segments: vec![
                PromptSegmentTrace {
                    segment_id: "context:skills:v1".to_string(),
                    category: "capabilities".to_string(),
                    role: "developer".to_string(),
                    source: "message.text".to_string(),
                    version: "v1".to_string(),
                    token_count_estimate: 200,
                    byte_count: 800,
                    criticality: PromptCriticality::Medium,
                    trigger_condition: "test".to_string(),
                    expected_behavior: None,
                    dependencies: Vec::new(),
                    content_hash: "hash".to_string(),
                },
                PromptSegmentTrace {
                    segment_id: "base:model:gpt:hash".to_string(),
                    category: "base_instructions".to_string(),
                    role: "instructions".to_string(),
                    source: "responses.instructions".to_string(),
                    version: "v1".to_string(),
                    token_count_estimate: 100,
                    byte_count: 400,
                    criticality: PromptCriticality::High,
                    trigger_condition: "always".to_string(),
                    expected_behavior: None,
                    dependencies: Vec::new(),
                    content_hash: "hash".to_string(),
                },
            ],
            tools: vec![ToolExposureTrace {
                tool_id: "tool:global:run_tests:hash".to_string(),
                name: "run_tests".to_string(),
                namespace: None,
                schema_hash: "hash".to_string(),
                token_count_estimate: 300,
                byte_count: 1200,
                exposure: ToolExposure::Direct,
                source: "responses.tools".to_string(),
                called: None,
            }],
            chains: vec![ChainCoverageTrace {
                chain_id: "chain:responses_request:v1".to_string(),
                status: ChainStatus::Entered,
                evidence: EvidenceStrength::Strong,
                reason: None,
            }],
            input_token_count_estimate: 0,
            prompt_token_count_estimate: 300,
            tool_token_count_estimate: 300,
            total_token_count_estimate: 600,
            output_schema_present: false,
            output_schema_strict: true,
            parallel_tool_calls: false,
            notes: Vec::new(),
        });

        let report = accumulator.into_report(1);

        assert_eq!(
            report
                .token_waterfall
                .iter()
                .map(|row| row.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "tool:global:run_tests:hash",
                "context:skills:v1",
                "base:model:gpt:hash"
            ]
        );
        assert_eq!(report.findings[0].target_id, "tool:global:run_tests:hash");
    }

    #[test]
    fn analyze_reads_snapshot_from_other_payload() -> anyhow::Result<()> {
        let temp = TempDir::new()?;
        fs::write(temp.path().join(MANIFEST_FILE_NAME), "{}")?;
        let payloads_dir = temp.path().join("payloads");
        fs::create_dir_all(&payloads_dir)?;
        fs::write(
            payloads_dir.join("1.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version": 1,
                "thread_id": "thread-root",
                "turn_id": "turn-1",
                "inference_call_id": "inference-1",
                "model": "gpt-test",
                "prompt_segments": [{
                    "segment_id": "context:skills:v1",
                    "category": "capabilities",
                    "role": "developer",
                    "source": "message.text",
                    "version": "v1",
                    "token_count_estimate": 80,
                    "byte_count": 320,
                    "criticality": "medium",
                    "trigger_condition": "test",
                    "expected_behavior": null,
                    "dependencies": [],
                    "content_hash": "hash"
                }],
                "tools": [],
                "chains": [],
                "input_token_count_estimate": 0,
                "prompt_token_count_estimate": 80,
                "tool_token_count_estimate": 0,
                "total_token_count_estimate": 80,
                "output_schema_present": false,
                "output_schema_strict": true,
                "parallel_tool_calls": false,
                "notes": []
            }))?,
        )?;
        fs::write(
            temp.path().join(RAW_EVENT_LOG_FILE_NAME),
            serde_json::to_string(&json!({
                "schema_version": 1,
                "seq": 1,
                "wall_time_unix_ms": 0,
                "rollout_id": "rollout-1",
                "thread_id": null,
                "codex_turn_id": null,
                "payload": {
                    "type": "other",
                    "kind": PROMPT_OBSERVABILITY_SNAPSHOT_EVENT_KIND,
                    "summary": "snapshot",
                    "payloads": [{
                        "raw_payload_id": "raw_payload:1",
                        "kind": {"type": "prompt_observability"},
                        "path": "payloads/1.json"
                    }],
                    "metadata": {}
                }
            }))? + "\n",
        )?;

        let report = analyze_trace_path(temp.path())?;

        assert_eq!(report.bundles_analyzed, 1);
        assert_eq!(report.snapshots_analyzed, 1);
        assert_eq!(
            report.top_token_heavy_segments[0].segment_id,
            "context:skills:v1"
        );
        Ok(())
    }
}
