use crate::snapshot::EvidenceStrength;
use crate::snapshot::OptimizationAction;
use crate::snapshot::OptimizationFinding;
use crate::snapshot::PromptCriticality;
use crate::snapshot::ToolExposure;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalysisFormat {
    Markdown,
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenWaterfallRow {
    pub id: String,
    pub label: String,
    pub category: String,
    pub token_count_estimate: usize,
    pub occurrences: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentSummary {
    pub segment_id: String,
    pub category: String,
    pub criticality: PromptCriticality,
    pub token_count_estimate: usize,
    pub occurrences: usize,
    pub evidence: EvidenceStrength,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolSummary {
    pub tool_id: String,
    pub name: String,
    pub namespace: Option<String>,
    pub exposure: ToolExposure,
    pub token_count_estimate: usize,
    pub exposed_count: usize,
    pub called_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeferredSearchSummary {
    pub name: String,
    pub namespace: Option<String>,
    pub search_result_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainCoverageSummary {
    pub chain_id: String,
    pub entered: usize,
    pub skipped: usize,
    pub fallback: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceAnalysisReport {
    pub bundles_analyzed: usize,
    pub snapshots_analyzed: usize,
    pub token_waterfall: Vec<TokenWaterfallRow>,
    pub top_token_heavy_segments: Vec<SegmentSummary>,
    pub low_evidence_segments: Vec<SegmentSummary>,
    pub exposed_but_never_called_tools: Vec<ToolSummary>,
    pub deferred_but_searched_tools: Vec<DeferredSearchSummary>,
    pub chain_coverage: Vec<ChainCoverageSummary>,
    pub findings: Vec<OptimizationFinding>,
}

pub fn render_analysis_report(report: &TraceAnalysisReport, format: AnalysisFormat) -> String {
    match format {
        AnalysisFormat::Markdown => render_markdown(report),
        AnalysisFormat::Json => serde_json::to_string_pretty(report).unwrap_or_default(),
    }
}

fn render_markdown(report: &TraceAnalysisReport) -> String {
    let mut out = String::new();
    out.push_str("# Prompt Observability Trace Analysis\n\n");
    out.push_str(&format!(
        "- Bundles analyzed: {}\n- Snapshots analyzed: {}\n- Token counts: estimate\n\n",
        report.bundles_analyzed, report.snapshots_analyzed
    ));

    out.push_str("## Token Waterfall\n\n");
    push_table(
        &mut out,
        &["ID", "Category", "Tokens", "Occurrences"],
        report.token_waterfall.iter().take(20).map(|row| {
            vec![
                row.id.clone(),
                row.category.clone(),
                row.token_count_estimate.to_string(),
                row.occurrences.to_string(),
            ]
        }),
    );

    out.push_str("\n## Low Evidence Segments\n\n");
    push_table(
        &mut out,
        &["Segment", "Category", "Criticality", "Tokens", "Evidence"],
        report.low_evidence_segments.iter().take(20).map(|segment| {
            vec![
                segment.segment_id.clone(),
                segment.category.clone(),
                format!("{:?}", segment.criticality).to_lowercase(),
                segment.token_count_estimate.to_string(),
                format!("{:?}", segment.evidence).to_lowercase(),
            ]
        }),
    );

    out.push_str("\n## Exposed But Never Called Tools\n\n");
    push_table(
        &mut out,
        &["Tool", "Exposure", "Tokens", "Exposed"],
        report
            .exposed_but_never_called_tools
            .iter()
            .take(20)
            .map(|tool| {
                vec![
                    display_tool_name(tool.namespace.as_deref(), &tool.name),
                    format!("{:?}", tool.exposure).to_lowercase(),
                    tool.token_count_estimate.to_string(),
                    tool.exposed_count.to_string(),
                ]
            }),
    );

    out.push_str("\n## Deferred But Searched Tools\n\n");
    push_table(
        &mut out,
        &["Tool", "Search Results"],
        report
            .deferred_but_searched_tools
            .iter()
            .take(20)
            .map(|tool| {
                vec![
                    display_tool_name(tool.namespace.as_deref(), &tool.name),
                    tool.search_result_count.to_string(),
                ]
            }),
    );

    out.push_str("\n## Chain Coverage\n\n");
    push_table(
        &mut out,
        &["Chain", "Entered", "Skipped", "Fallback"],
        report.chain_coverage.iter().map(|chain| {
            vec![
                chain.chain_id.clone(),
                chain.entered.to_string(),
                chain.skipped.to_string(),
                chain.fallback.to_string(),
            ]
        }),
    );

    out.push_str("\n## Candidate Actions\n\n");
    push_table(
        &mut out,
        &["Target", "Action", "Savings", "Evidence", "Reason"],
        report.findings.iter().take(20).map(|finding| {
            vec![
                finding.target_id.clone(),
                display_action(finding.action).to_string(),
                finding.token_savings_estimate.to_string(),
                format!("{:?}", finding.evidence).to_lowercase(),
                finding.reason.clone(),
            ]
        }),
    );

    out
}

fn push_table(out: &mut String, headers: &[&str], rows: impl IntoIterator<Item = Vec<String>>) {
    out.push('|');
    for header in headers {
        out.push(' ');
        out.push_str(header);
        out.push_str(" |");
    }
    out.push('\n');
    out.push('|');
    for _ in headers {
        out.push_str(" --- |");
    }
    out.push('\n');

    let mut wrote = false;
    for row in rows {
        wrote = true;
        out.push('|');
        for value in row {
            out.push(' ');
            out.push_str(&escape_markdown_table_cell(&value));
            out.push_str(" |");
        }
        out.push('\n');
    }

    if !wrote {
        out.push('|');
        for _ in headers {
            out.push_str(" none |");
        }
        out.push('\n');
    }
}

fn display_tool_name(namespace: Option<&str>, name: &str) -> String {
    match namespace {
        Some(namespace) => format!("{namespace}.{name}"),
        None => name.to_string(),
    }
}

fn display_action(action: OptimizationAction) -> &'static str {
    match action {
        OptimizationAction::Keep => "keep",
        OptimizationAction::Compress => "compress",
        OptimizationAction::LazyLoad => "lazy_load",
        OptimizationAction::RouteConditionally => "route_conditionally",
        OptimizationAction::DeleteCandidate => "delete_candidate",
    }
}

fn escape_markdown_table_cell(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn renders_markdown_report() {
        let report = TraceAnalysisReport {
            bundles_analyzed: 1,
            snapshots_analyzed: 1,
            token_waterfall: vec![TokenWaterfallRow {
                id: "context:skills:v1".to_string(),
                label: "context:skills:v1".to_string(),
                category: "capabilities".to_string(),
                token_count_estimate: 80,
                occurrences: 1,
            }],
            top_token_heavy_segments: Vec::new(),
            low_evidence_segments: Vec::new(),
            exposed_but_never_called_tools: Vec::new(),
            deferred_but_searched_tools: Vec::new(),
            chain_coverage: Vec::new(),
            findings: Vec::new(),
        };

        let rendered = render_analysis_report(&report, AnalysisFormat::Markdown);

        assert_eq!(
            rendered.lines().take(8).collect::<Vec<_>>(),
            vec![
                "# Prompt Observability Trace Analysis",
                "",
                "- Bundles analyzed: 1",
                "- Snapshots analyzed: 1",
                "- Token counts: estimate",
                "",
                "## Token Waterfall",
                "",
            ]
        );
    }
}
