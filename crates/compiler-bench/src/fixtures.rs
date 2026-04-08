use std::{
    error::Error,
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use compiler_core::{CompilerPolicy, GroupInput, analyze_groups, analyze_lines, project_output};
use compiler_schema::{CanonicalAnalysis, EventKind, LlmOutput, LogicalSeries, Scope};

use crate::regression::{RegressionFailure, RegressionReport, ScopeRegressionSummary};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum AnalyzeRequest {
    Line {
        #[serde(default)]
        policy: Option<CompilerPolicy>,
        series: Vec<LogicalSeries>,
    },
    Group {
        #[serde(default)]
        policy: Option<CompilerPolicy>,
        groups: Vec<GroupInput>,
    },
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct AnalyzeRecord {
    pub canonical: CanonicalAnalysis,
    pub llm: LlmOutput,
}

#[derive(Clone, Debug, Serialize, PartialEq)]
pub struct AnalyzeResponse {
    pub outputs: Vec<AnalyzeRecord>,
}

impl AnalyzeResponse {
    pub fn to_pretty_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum RegressionCase {
    Line {
        name: String,
        subject_id: String,
        series: Vec<LogicalSeries>,
        expected_top_events: Vec<EventKind>,
    },
    Group {
        name: String,
        subject_id: String,
        groups: Vec<GroupInput>,
        expected_top_events: Vec<EventKind>,
    },
}

pub fn analyze_request_file(
    path: &Path,
    default_policy: &CompilerPolicy,
) -> Result<AnalyzeResponse, Box<dyn Error>> {
    let request = load_json_file::<AnalyzeRequest>(path)?;
    Ok(analyze_request(&request, default_policy))
}

pub fn analyze_request(
    request: &AnalyzeRequest,
    default_policy: &CompilerPolicy,
) -> AnalyzeResponse {
    let analyses = match request {
        AnalyzeRequest::Line { policy, series } => {
            let effective_policy = policy.clone().unwrap_or_else(|| default_policy.clone());
            analyze_lines(series, &effective_policy)
        }
        AnalyzeRequest::Group { policy, groups } => {
            let effective_policy = policy.clone().unwrap_or_else(|| default_policy.clone());
            analyze_groups(groups, &effective_policy)
        }
    };

    AnalyzeResponse {
        outputs: analyses
            .into_iter()
            .map(|canonical| AnalyzeRecord {
                llm: project_output(&canonical),
                canonical,
            })
            .collect(),
    }
}

pub fn load_regression_cases_from_dir(dir: &Path) -> Result<Vec<RegressionCase>, Box<dyn Error>> {
    let paths = json_files_in_dir(dir)?;
    let mut cases = Vec::with_capacity(paths.len());
    for path in paths {
        cases.push(load_json_file::<RegressionCase>(&path)?);
    }
    Ok(cases)
}

pub fn load_analyze_requests_from_dir(dir: &Path) -> Result<Vec<AnalyzeRequest>, Box<dyn Error>> {
    let paths = json_files_in_dir(dir)?;
    let mut requests = Vec::with_capacity(paths.len());
    for path in paths {
        requests.push(load_json_file::<AnalyzeRequest>(&path)?);
    }
    Ok(requests)
}

pub fn run_regression_cases(
    cases: &[RegressionCase],
    default_policy: &CompilerPolicy,
) -> RegressionReport {
    let mut total = 0;
    let mut passed = 0;
    let mut failed = Vec::new();
    let mut line_level_top3 = ScopeRegressionSummary {
        total: 0,
        passed: 0,
        failed: 0,
    };
    let mut group_level_top3 = ScopeRegressionSummary {
        total: 0,
        passed: 0,
        failed: 0,
    };

    for case in cases {
        total += 1;
        match case {
            RegressionCase::Line {
                name,
                subject_id,
                series,
                expected_top_events,
            } => {
                line_level_top3.total += 1;
                let analyses = analyze_lines(series, default_policy);
                let actual: Vec<EventKind> = analyses
                    .iter()
                    .find(|analysis| analysis.subject_id == *subject_id)
                    .map(|analysis| {
                        analysis
                            .top_events
                            .iter()
                            .map(|event| event.kind)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if actual == *expected_top_events {
                    passed += 1;
                    line_level_top3.passed += 1;
                } else {
                    line_level_top3.failed += 1;
                    failed.push(RegressionFailure {
                        case_name: name.clone(),
                        scope: Scope::Line,
                        subject_id: subject_id.clone(),
                        expected: expected_top_events.clone(),
                        missing: missing_events(expected_top_events, &actual),
                        unexpected: unexpected_events(expected_top_events, &actual),
                        actual,
                    });
                }
            }
            RegressionCase::Group {
                name,
                subject_id,
                groups,
                expected_top_events,
            } => {
                group_level_top3.total += 1;
                let analyses = analyze_groups(groups, default_policy);
                let actual: Vec<EventKind> = analyses
                    .iter()
                    .find(|analysis| analysis.subject_id == *subject_id)
                    .map(|analysis| {
                        analysis
                            .top_events
                            .iter()
                            .map(|event| event.kind)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if actual == *expected_top_events {
                    passed += 1;
                    group_level_top3.passed += 1;
                } else {
                    group_level_top3.failed += 1;
                    failed.push(RegressionFailure {
                        case_name: name.clone(),
                        scope: Scope::Group,
                        subject_id: subject_id.clone(),
                        expected: expected_top_events.clone(),
                        missing: missing_events(expected_top_events, &actual),
                        unexpected: unexpected_events(expected_top_events, &actual),
                        actual,
                    });
                }
            }
        }
    }

    RegressionReport {
        total,
        passed,
        line_level_top3,
        group_level_top3,
        failed,
    }
}

pub fn default_regression_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/regression")
}

pub fn default_demo_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../cases/demo")
}

fn json_files_in_dir(dir: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut entries = fs::read_dir(dir)?
        .map(|entry| entry.map(|item| item.path()))
        .collect::<Result<Vec<_>, _>>()?;
    entries.retain(|path| path.extension() == Some(OsStr::new("json")));
    entries.sort();
    Ok(entries)
}

fn load_json_file<T>(path: &Path) -> Result<T, Box<dyn Error>>
where
    T: for<'de> Deserialize<'de>,
{
    let raw = fs::read_to_string(path)?;
    let value = serde_json::from_str(&raw)?;
    Ok(value)
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

    use super::{
        AnalyzeRequest, default_demo_dir, default_regression_dir, load_analyze_requests_from_dir,
        load_regression_cases_from_dir, run_regression_cases,
    };

    #[test]
    fn default_fixture_dirs_load() {
        let demo = load_analyze_requests_from_dir(&default_demo_dir()).expect("demo cases");
        let regression =
            load_regression_cases_from_dir(&default_regression_dir()).expect("regression cases");

        assert_eq!(demo.len(), 2);
        assert_eq!(regression.len(), 52);
        assert!(matches!(demo[0], AnalyzeRequest::Line { .. }));
    }

    #[test]
    fn default_regression_cases_pass() {
        let cases = load_regression_cases_from_dir(&default_regression_dir()).expect("cases");
        let report = run_regression_cases(&cases, &CompilerPolicy::default());
        assert!(
            report.failed.is_empty(),
            "regression failed: {}",
            report.render_text()
        );
    }
}
