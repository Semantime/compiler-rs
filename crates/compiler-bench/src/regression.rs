use serde::Serialize;
use std::{error::Error, path::Path};

use compiler_core::CompilerPolicy;
use compiler_schema::{EventKind, LlmOutput, Scope};

use crate::fixtures::{
    AnalyzeRecord, AnalyzeRequest, RegressionCase, analyze_request, default_demo_dir,
    default_regression_dir, load_analyze_requests_from_dir, load_regression_cases_from_dir,
    run_regression_cases,
};

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RegressionFailure {
    pub case_name: String,
    pub scope: Scope,
    pub subject_id: String,
    pub expected: Vec<EventKind>,
    pub actual: Vec<EventKind>,
    pub missing: Vec<EventKind>,
    pub unexpected: Vec<EventKind>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ScopeRegressionSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RegressionViewerCase {
    pub case_name: String,
    pub scope: Scope,
    pub subject_id: String,
    pub passed: bool,
    pub expected: Vec<EventKind>,
    pub actual: Vec<EventKind>,
    pub missing: Vec<EventKind>,
    pub unexpected: Vec<EventKind>,
    pub request: AnalyzeRequest,
    pub output: Option<AnalyzeRecord>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RegressionViewerData {
    pub report: RegressionReport,
    pub cases: Vec<RegressionViewerCase>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RegressionReport {
    pub total: usize,
    pub passed: usize,
    pub line_level_top3: ScopeRegressionSummary,
    pub group_level_top3: ScopeRegressionSummary,
    pub failed: Vec<RegressionFailure>,
}

impl RegressionReport {
    pub fn render_text(&self) -> String {
        let mut lines = vec![format!(
            "regression total={} passed={} failed={}",
            self.total,
            self.passed,
            self.failed.len()
        )];
        lines.push(format!(
            "line_level_top3 total={} passed={} failed={}",
            self.line_level_top3.total, self.line_level_top3.passed, self.line_level_top3.failed
        ));
        lines.push(format!(
            "group_level_top3 total={} passed={} failed={}",
            self.group_level_top3.total, self.group_level_top3.passed, self.group_level_top3.failed
        ));
        for failure in &self.failed {
            lines.push(format!(
                "FAIL [{}] {} subject={} expected=[{}] actual=[{}] missing=[{}] unexpected=[{}]",
                failure.scope.as_str(),
                failure.case_name,
                failure.subject_id,
                render_kinds(&failure.expected),
                render_kinds(&failure.actual),
                render_kinds(&failure.missing),
                render_kinds(&failure.unexpected)
            ));
        }
        if self.failed.is_empty() {
            lines.push("all regression cases passed".into());
        }
        lines.join("\n")
    }
}

pub fn run_demo(policy: &CompilerPolicy) -> Result<Vec<LlmOutput>, Box<dyn Error>> {
    run_demo_from_dir(policy, &default_demo_dir())
}

pub fn run_demo_from_dir(
    policy: &CompilerPolicy,
    dir: &Path,
) -> Result<Vec<LlmOutput>, Box<dyn Error>> {
    let requests = load_analyze_requests_from_dir(dir)?;
    let mut outputs = Vec::new();
    for request in requests {
        outputs.extend(
            analyze_request(&request, policy)
                .outputs
                .into_iter()
                .map(|record| record.llm),
        );
    }
    Ok(outputs)
}

pub fn run_regression_suite(policy: &CompilerPolicy) -> Result<RegressionReport, Box<dyn Error>> {
    run_regression_suite_from_dir(policy, &default_regression_dir())
}

pub fn run_regression_suite_from_dir(
    policy: &CompilerPolicy,
    dir: &Path,
) -> Result<RegressionReport, Box<dyn Error>> {
    let cases = load_regression_cases_from_dir(dir)?;
    Ok(run_regression_cases(&cases, policy))
}

pub fn build_regression_viewer_data(
    cases: &[RegressionCase],
    default_policy: &CompilerPolicy,
) -> RegressionViewerData {
    let report = run_regression_cases(cases, default_policy);
    let cases = cases
        .iter()
        .map(|case| build_regression_viewer_case(case, default_policy))
        .collect();
    RegressionViewerData { report, cases }
}

pub fn build_regression_viewer_data_from_dir(
    policy: &CompilerPolicy,
    dir: &Path,
) -> Result<RegressionViewerData, Box<dyn Error>> {
    let cases = load_regression_cases_from_dir(dir)?;
    Ok(build_regression_viewer_data(&cases, policy))
}

fn render_kinds(kinds: &[EventKind]) -> String {
    kinds
        .iter()
        .map(|kind| kind.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn build_regression_viewer_case(
    case: &RegressionCase,
    default_policy: &CompilerPolicy,
) -> RegressionViewerCase {
    match case {
        RegressionCase::Line {
            name,
            subject_id,
            series,
            expected_top_events,
        } => {
            let request = AnalyzeRequest::Line {
                policy: None,
                series: series.clone(),
            };
            let response = analyze_request(&request, default_policy);
            let output = response
                .outputs
                .into_iter()
                .find(|record| record.canonical.subject_id == *subject_id);
            let actual = output
                .as_ref()
                .map(|record| {
                    record
                        .canonical
                        .top_events
                        .iter()
                        .map(|event| event.kind)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let missing = missing_events(expected_top_events, &actual);
            let unexpected = unexpected_events(expected_top_events, &actual);

            RegressionViewerCase {
                case_name: name.clone(),
                scope: Scope::Line,
                subject_id: subject_id.clone(),
                passed: actual == *expected_top_events,
                expected: expected_top_events.clone(),
                actual,
                missing,
                unexpected,
                request,
                output,
            }
        }
        RegressionCase::Group {
            name,
            subject_id,
            groups,
            expected_top_events,
        } => {
            let request = AnalyzeRequest::Group {
                policy: None,
                groups: groups.clone(),
            };
            let response = analyze_request(&request, default_policy);
            let output = response
                .outputs
                .into_iter()
                .find(|record| record.canonical.subject_id == *subject_id);
            let actual = output
                .as_ref()
                .map(|record| {
                    record
                        .canonical
                        .top_events
                        .iter()
                        .map(|event| event.kind)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            let missing = missing_events(expected_top_events, &actual);
            let unexpected = unexpected_events(expected_top_events, &actual);

            RegressionViewerCase {
                case_name: name.clone(),
                scope: Scope::Group,
                subject_id: subject_id.clone(),
                passed: actual == *expected_top_events,
                expected: expected_top_events.clone(),
                actual,
                missing,
                unexpected,
                request,
                output,
            }
        }
    }
}

fn missing_events(expected: &[EventKind], actual: &[EventKind]) -> Vec<EventKind> {
    expected
        .iter()
        .copied()
        .filter(|kind| !actual.contains(kind))
        .collect()
}

fn unexpected_events(expected: &[EventKind], actual: &[EventKind]) -> Vec<EventKind> {
    actual
        .iter()
        .copied()
        .filter(|kind| !expected.contains(kind))
        .collect()
}

#[cfg(test)]
mod tests {
    use compiler_core::CompilerPolicy;

    use super::{build_regression_viewer_data_from_dir, run_demo, run_regression_suite};
    use crate::default_regression_dir;

    #[test]
    fn regression_suite_passes() {
        let report = run_regression_suite(&CompilerPolicy::default()).expect("regression");
        assert!(
            report.failed.is_empty(),
            "regression failed: {}",
            report.render_text()
        );
    }

    #[test]
    fn demo_outputs_are_not_empty() {
        let outputs = run_demo(&CompilerPolicy::default()).expect("demo");
        assert_eq!(outputs.len(), 4);
    }

    #[test]
    fn regression_viewer_data_contains_cases() {
        let data = build_regression_viewer_data_from_dir(
            &CompilerPolicy::default(),
            &default_regression_dir(),
        )
        .expect("viewer data");
        assert_eq!(data.report.total, data.cases.len());
        assert!(data.cases.iter().all(|case| case.output.is_some()));
    }
}
