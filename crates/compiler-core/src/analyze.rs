use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use compiler_schema::{
    CanonicalAnalysis, Event, EventKind, EvidenceItem, LogicalSeries, NormalizedSeries,
    PeerContext, Regime, SCHEMA_VERSION, Scope, StateKind, Statistics, TimeSeriesPoint, TrendKind,
};

use crate::{
    features::{compute_statistics, format_duration, mean, robust_scale, std_dev},
    normalize::normalize_series,
    segment::segment_series,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitivityProfile {
    Conservative,
    Balanced,
    Aggressive,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompilerPolicy {
    #[serde(default = "default_sensitivity")]
    pub sensitivity: SensitivityProfile,
    #[serde(default = "default_max_paa_segments")]
    pub max_paa_segments: usize,
    #[serde(default = "default_enable_peer_context")]
    pub enable_peer_context: bool,
}

impl Default for CompilerPolicy {
    fn default() -> Self {
        Self {
            sensitivity: SensitivityProfile::Balanced,
            max_paa_segments: 4,
            enable_peer_context: true,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GroupInput {
    pub metric_id: String,
    pub group_id: String,
    pub members: Vec<LogicalSeries>,
}

const fn default_sensitivity() -> SensitivityProfile {
    SensitivityProfile::Balanced
}

const fn default_max_paa_segments() -> usize {
    4
}

const fn default_enable_peer_context() -> bool {
    true
}

#[derive(Clone)]
struct WorkingAnalysis {
    analysis: CanonicalAnalysis,
    magnitude_anchor: f64,
}

pub fn analyze_lines(
    series_list: &[LogicalSeries],
    policy: &CompilerPolicy,
) -> Vec<CanonicalAnalysis> {
    let mut working: Vec<WorkingAnalysis> = series_list
        .iter()
        .filter_map(|series| analyze_line_working(series, policy))
        .collect();

    if policy.enable_peer_context {
        attach_peer_context(&mut working);
    }

    working.into_iter().map(|item| item.analysis).collect()
}

pub fn analyze_groups(groups: &[GroupInput], policy: &CompilerPolicy) -> Vec<CanonicalAnalysis> {
    let mut working: Vec<WorkingAnalysis> = groups
        .iter()
        .filter_map(|group| analyze_group_working(group, policy))
        .collect();

    if policy.enable_peer_context {
        attach_peer_context(&mut working);
    }

    working.into_iter().map(|item| item.analysis).collect()
}

fn analyze_line_working(
    series: &LogicalSeries,
    policy: &CompilerPolicy,
) -> Option<WorkingAnalysis> {
    let normalized = normalize_series(series)?;
    let stats = compute_statistics(&normalized.points, policy.max_paa_segments);
    let regimes = segment_series(&normalized, &stats, policy.sensitivity);
    let trend = summarize_trend(&normalized, &stats, &regimes);
    let mut events = detect_line_events(&normalized, &stats, &regimes, trend);
    sort_events(&mut events);
    let state = derive_state(&events);
    let top_events: Vec<Event> = events.into_iter().take(3).collect();
    let evidence = collect_analysis_evidence(&top_events);

    Some(WorkingAnalysis {
        magnitude_anchor: stats.mean,
        analysis: CanonicalAnalysis {
            schema_version: SCHEMA_VERSION,
            scope: Scope::Line,
            metric_id: normalized.metric_id.clone(),
            subject_id: normalized.entity_id.clone(),
            window_start_ts_secs: normalized.window_start_ts_secs,
            window_secs: normalized.window_secs,
            state,
            trend,
            top_events,
            peer_context: None,
            regimes,
            evidence,
        },
    })
}

fn analyze_group_working(group: &GroupInput, policy: &CompilerPolicy) -> Option<WorkingAnalysis> {
    if group.members.is_empty() {
        return None;
    }

    let line_working: Vec<WorkingAnalysis> = group
        .members
        .iter()
        .filter_map(|series| analyze_line_working(series, policy))
        .collect();
    if line_working.is_empty() {
        return None;
    }

    let normalized_members: Vec<NormalizedSeries> =
        group.members.iter().filter_map(normalize_series).collect();
    let aggregate_series =
        aggregate_group_series(&group.metric_id, &group.group_id, &normalized_members)?;
    let aggregate_stats = compute_statistics(&aggregate_series.points, policy.max_paa_segments);
    let regimes = segment_series(&aggregate_series, &aggregate_stats, policy.sensitivity);
    let trend = summarize_trend(&aggregate_series, &aggregate_stats, &regimes);

    let mut by_kind: HashMap<EventKind, Vec<Event>> = HashMap::new();
    for line in &line_working {
        for event in &line.analysis.top_events {
            by_kind.entry(event.kind).or_default().push(event.clone());
        }
    }

    let mut events = Vec::new();
    for (kind, members) in by_kind {
        let max_score = members
            .iter()
            .map(|event| event.score)
            .fold(0.0_f64, f64::max);
        let start_ts_secs = members
            .iter()
            .map(|event| event.start_ts_secs)
            .min()
            .unwrap_or_else(|| {
                aggregate_series
                    .points
                    .first()
                    .map(|point| point.ts_secs)
                    .unwrap_or(0)
            });
        let end_ts_secs = members
            .iter()
            .map(|event| event.end_ts_secs)
            .max()
            .unwrap_or_else(|| {
                aggregate_series
                    .points
                    .last()
                    .map(|point| point.ts_secs)
                    .unwrap_or(0)
            });
        let affected = members.len();
        let member_ratio = affected as f64 / line_working.len() as f64;
        let mut evidence = vec![
            EvidenceItem {
                label: "affected_lines".into(),
                value: format!("{affected}/{}", line_working.len()),
            },
            EvidenceItem {
                label: "max_line_score".into(),
                value: format!("{max_score:.2}"),
            },
        ];
        if let Some(first) = members.first() {
            evidence.extend(first.evidence.iter().take(2).cloned());
        }
        let score = max_score + member_ratio * 2.5;
        let mut timepoints_ts_secs: Vec<i64> = members
            .iter()
            .flat_map(|event| {
                if event.timepoints_ts_secs.is_empty() && event.start_ts_secs == event.end_ts_secs {
                    vec![event.start_ts_secs]
                } else {
                    event.timepoints_ts_secs.clone()
                }
            })
            .collect();
        timepoints_ts_secs.sort_unstable();
        timepoints_ts_secs.dedup();
        events.push(Event {
            kind,
            score,
            start_ts_secs,
            end_ts_secs,
            timepoints_ts_secs,
            evidence,
            impacted_members: affected,
        });
    }

    let mut aggregate_events =
        detect_line_events(&aggregate_series, &aggregate_stats, &regimes, trend);
    for aggregate in aggregate_events.drain(..) {
        if let Some(existing) = events.iter_mut().find(|event| event.kind == aggregate.kind) {
            existing.score += aggregate.score * 0.35;
            existing
                .timepoints_ts_secs
                .extend(aggregate.timepoints_ts_secs.iter().copied());
            existing.timepoints_ts_secs.sort_unstable();
            existing.timepoints_ts_secs.dedup();
            for evidence in aggregate.evidence.into_iter().take(2) {
                if existing
                    .evidence
                    .iter()
                    .all(|item| item.label != evidence.label)
                {
                    existing.evidence.push(evidence);
                }
            }
        } else {
            events.push(Event {
                impacted_members: 1,
                ..aggregate
            });
        }
    }

    if let Some(peer_imbalance) =
        detect_group_peer_imbalance(&aggregate_series, &line_working, line_working.len())
    {
        events.push(peer_imbalance);
    }

    sort_events(&mut events);
    let top_events: Vec<Event> = events.into_iter().take(3).collect();
    let state = derive_state(&top_events);
    let evidence = collect_analysis_evidence(&top_events);

    Some(WorkingAnalysis {
        magnitude_anchor: aggregate_stats.mean,
        analysis: CanonicalAnalysis {
            schema_version: SCHEMA_VERSION,
            scope: Scope::Group,
            metric_id: group.metric_id.clone(),
            subject_id: group.group_id.clone(),
            window_start_ts_secs: aggregate_series.window_start_ts_secs,
            window_secs: aggregate_series.window_secs,
            state,
            trend,
            top_events,
            peer_context: None,
            regimes,
            evidence,
        },
    })
}

fn detect_group_peer_imbalance(
    aggregate_series: &NormalizedSeries,
    line_working: &[WorkingAnalysis],
    member_count: usize,
) -> Option<Event> {
    if member_count < 2 || line_working.len() < 2 {
        return None;
    }

    let stable_member_count = line_working
        .iter()
        .filter(|line| is_stable_member(&line.analysis.top_events))
        .count();
    if stable_member_count * 3 < line_working.len() * 2 {
        return None;
    }

    let mut member_levels: Vec<(String, f64)> = line_working
        .iter()
        .map(|line| (line.analysis.subject_id.clone(), line.magnitude_anchor))
        .collect();
    member_levels.sort_by(|left, right| left.1.total_cmp(&right.1));

    let baseline = median_level(&member_levels);
    let mean_abs = baseline.abs().max(1.0);
    let min_level = member_levels.first()?.1;
    let max_level = member_levels.last()?.1;
    let relative_spread = (max_level - min_level).abs() / mean_abs;

    let (outlier_member, outlier_level, outlier_gap) = member_levels
        .iter()
        .map(|(subject_id, level)| (subject_id.clone(), *level, (*level - baseline).abs() / mean_abs))
        .max_by(|left, right| left.2.total_cmp(&right.2))?;

    if relative_spread < 0.10 || outlier_gap < 0.08 {
        return None;
    }

    let impacted_members = member_levels
        .iter()
        .filter(|(_, level)| ((*level - baseline).abs() / mean_abs) >= 0.08)
        .count()
        .max(1);
    let score = 4.5 + relative_spread * 20.0 + outlier_gap * 8.0;

    Some(Event {
        kind: EventKind::PeerImbalance,
        score,
        start_ts_secs: aggregate_series.points.first().map(|point| point.ts_secs).unwrap_or(0),
        end_ts_secs: aggregate_series.points.last().map(|point| point.ts_secs).unwrap_or(0),
        timepoints_ts_secs: vec![],
        evidence: vec![
            EvidenceItem {
                label: "level_spread".into(),
                value: format_percent(relative_spread),
            },
            EvidenceItem {
                label: "outlier_gap".into(),
                value: format_percent(outlier_gap),
            },
            EvidenceItem {
                label: "top_member".into(),
                value: outlier_member,
            },
            EvidenceItem {
                label: "top_member_mean".into(),
                value: format!("{outlier_level:.1}"),
            },
        ],
        impacted_members,
    })
}

fn is_stable_member(events: &[Event]) -> bool {
    events.iter().all(|event| {
        !matches!(
            event.kind,
            EventKind::Spike
                | EventKind::Drop
                | EventKind::RegimeShift
                | EventKind::Oscillation
                | EventKind::IncreasingTrend
                | EventKind::DecreasingTrend
        )
    })
}

fn median_level(member_levels: &[(String, f64)]) -> f64 {
    let len = member_levels.len();
    if len == 0 {
        return 0.0;
    }
    if len % 2 == 0 {
        let upper = member_levels[len / 2].1;
        let lower = member_levels[len / 2 - 1].1;
        (upper + lower) / 2.0
    } else {
        member_levels[len / 2].1
    }
}

fn aggregate_group_series(
    metric_id: &str,
    group_id: &str,
    members: &[NormalizedSeries],
) -> Option<NormalizedSeries> {
    let first = members.first()?;
    let min_len = members.iter().map(|series| series.points.len()).min()?;
    if min_len < 3 {
        return None;
    }

    let mut points = Vec::with_capacity(min_len);
    for index in 0..min_len {
        let ts = first.points.get(index)?.ts_secs;
        let values: Vec<f64> = members
            .iter()
            .filter_map(|series| series.points.get(index))
            .map(|point| point.value)
            .collect();
        points.push(TimeSeriesPoint {
            ts_secs: ts,
            value: mean(&values),
        });
    }

    Some(NormalizedSeries {
        metric_id: metric_id.to_string(),
        entity_id: group_id.to_string(),
        group_id: group_id.to_string(),
        labels: first.labels.clone(),
        window_start_ts_secs: first.window_start_ts_secs,
        interval_secs: first.interval_secs,
        window_secs: points.last()?.ts_secs - points.first()?.ts_secs,
        points,
    })
}

fn detect_line_events(
    series: &NormalizedSeries,
    stats: &Statistics,
    regimes: &[Regime],
    trend: TrendKind,
) -> Vec<Event> {
    let values: Vec<f64> = series.points.iter().map(|point| point.value).collect();
    let len = values.len();
    let range = (stats.max - stats.min).abs();
    let scale = robust_scale(stats.std_dev, stats.mad, range, stats.mean.abs());
    let leading_baseline = leading_baseline_mean(&values);

    let mut events = Vec::new();

    if let Some((start, end, shift)) = tail_run_above(&values, stats.mean + scale * 0.25) {
        let duration_points = end - start + 1;
        let tail_mean = mean(&values[start..]);
        let baseline_shift = tail_mean - leading_baseline;
        if duration_points >= len.max(4) / 4 && baseline_shift > scale * 0.30 {
            let start_ts = series.points[start].ts_secs;
            let end_ts = series.points[end].ts_secs;
            let duration = end_ts - start_ts + series.interval_secs;
            events.push(Event {
                kind: EventKind::SustainedHigh,
                score: 6.5 + shift / scale * 1.4 + duration_points as f64 / len as f64 * 3.0,
                start_ts_secs: start_ts,
                end_ts_secs: end_ts,
                timepoints_ts_secs: vec![],
                evidence: vec![
                    EvidenceItem {
                        label: "mean_shift".into(),
                        value: format_percent(shift / stats.mean.max(1e-6)),
                    },
                    EvidenceItem {
                        label: "duration".into(),
                        value: format_duration(duration),
                    },
                ],
                impacted_members: 1,
            });
        }
    }

    if let Some((start, end, shift)) = tail_run_below(&values, stats.mean - scale * 0.25) {
        let duration_points = end - start + 1;
        let tail_mean = mean(&values[start..]);
        let baseline_shift = leading_baseline - tail_mean;
        if duration_points >= len.max(4) / 4 && baseline_shift > scale * 0.30 {
            let start_ts = series.points[start].ts_secs;
            let end_ts = series.points[end].ts_secs;
            let duration = end_ts - start_ts + series.interval_secs;
            events.push(Event {
                kind: EventKind::SustainedLow,
                score: 6.5 + shift / scale * 1.4 + duration_points as f64 / len as f64 * 3.0,
                start_ts_secs: start_ts,
                end_ts_secs: end_ts,
                timepoints_ts_secs: vec![],
                evidence: vec![
                    EvidenceItem {
                        label: "mean_shift".into(),
                        value: format_percent(-shift / stats.mean.abs().max(1e-6)),
                    },
                    EvidenceItem {
                        label: "duration".into(),
                        value: format_duration(duration),
                    },
                ],
                impacted_members: 1,
            });
        }
    }

    if let Some((index, prominence, dwell, spike_indices)) = detect_spike(&values, stats.median, scale) {
        let start = index.saturating_sub(dwell / 2);
        let end = (index + dwell / 2).min(len - 1);
        events.push(Event {
            kind: EventKind::Spike,
            score: 4.4 + prominence / scale + (1.0 - dwell as f64 / len as f64) * 0.8,
            start_ts_secs: series.points[start].ts_secs,
            end_ts_secs: series.points[end].ts_secs,
            timepoints_ts_secs: spike_indices
                .into_iter()
                .filter_map(|point_index| series.points.get(point_index))
                .map(|point| point.ts_secs)
                .collect(),
            evidence: vec![
                EvidenceItem {
                    label: "peak".into(),
                    value: format_percent(prominence / stats.mean.max(1e-6)),
                },
                EvidenceItem {
                    label: "dwell".into(),
                    value: format!("{}pt", dwell),
                },
            ],
            impacted_members: 1,
        });
    }

    if let Some((index, prominence, dwell)) = detect_drop(&values, stats.median, scale) {
        let start = index.saturating_sub(dwell / 2);
        let end = (index + dwell / 2).min(len - 1);
        events.push(Event {
            kind: EventKind::Drop,
            score: 4.4 + prominence / scale + (1.0 - dwell as f64 / len as f64) * 0.8,
            start_ts_secs: series.points[start].ts_secs,
            end_ts_secs: series.points[end].ts_secs,
            timepoints_ts_secs: vec![],
            evidence: vec![
                EvidenceItem {
                    label: "trough".into(),
                    value: format_percent(prominence / stats.mean.abs().max(1e-6)),
                },
                EvidenceItem {
                    label: "dwell".into(),
                    value: format!("{}pt", dwell),
                },
            ],
            impacted_members: 1,
        });
    }

    if regimes.len() >= 2 {
        let largest_shift = regimes
            .iter()
            .skip(1)
            .filter_map(|regime| regime.delta_from_prev.map(f64::abs))
            .fold(0.0_f64, f64::max);
        if largest_shift >= scale * 0.8 {
            events.push(Event {
                kind: EventKind::RegimeShift,
                score: 4.8 + largest_shift / scale * 1.4 + regimes.len() as f64 * 0.2,
                start_ts_secs: regimes
                    .get(1)
                    .map(|regime| regime.start_ts_secs)
                    .unwrap_or_else(|| {
                        series
                            .points
                            .first()
                            .map(|point| point.ts_secs)
                            .unwrap_or(0)
                    }),
                end_ts_secs: series.points.last().map(|point| point.ts_secs).unwrap_or(0),
                timepoints_ts_secs: vec![],
                evidence: vec![
                    EvidenceItem {
                        label: "largest_regime_shift".into(),
                        value: format_percent(largest_shift / stats.mean.abs().max(1e-6)),
                    },
                    EvidenceItem {
                        label: "regime_count".into(),
                        value: regimes.len().to_string(),
                    },
                ],
                impacted_members: 1,
            });
        }
    }

    if let Some((density, amplitude)) = oscillation_signature(&values, scale) {
        events.push(Event {
            kind: EventKind::Oscillation,
            score: 3.5 + density * 4.0 + amplitude / scale,
            start_ts_secs: series
                .points
                .first()
                .map(|point| point.ts_secs)
                .unwrap_or(0),
            end_ts_secs: series.points.last().map(|point| point.ts_secs).unwrap_or(0),
            timepoints_ts_secs: vec![],
            evidence: vec![
                EvidenceItem {
                    label: "flip_density".into(),
                    value: format!("{density:.2}"),
                },
                EvidenceItem {
                    label: "swing_std".into(),
                    value: format!("{amplitude:.2}"),
                },
            ],
            impacted_members: 1,
        });
    }

    let slope_strength = normalized_slope_strength(series, stats, scale);
    match trend {
        TrendKind::Increasing | TrendKind::UpThenFlat if slope_strength >= 0.45 => {
            events.push(Event {
                kind: EventKind::IncreasingTrend,
                score: 1.8 + slope_strength * 1.4,
                start_ts_secs: series
                    .points
                    .first()
                    .map(|point| point.ts_secs)
                    .unwrap_or(0),
                end_ts_secs: series.points.last().map(|point| point.ts_secs).unwrap_or(0),
                timepoints_ts_secs: vec![],
                evidence: vec![EvidenceItem {
                    label: "slope".into(),
                    value: format!("{:.4}", stats.slope),
                }],
                impacted_members: 1,
            });
        }
        TrendKind::Decreasing | TrendKind::DownThenFlat if slope_strength >= 0.45 => {
            events.push(Event {
                kind: EventKind::DecreasingTrend,
                score: 1.8 + slope_strength * 1.4,
                start_ts_secs: series
                    .points
                    .first()
                    .map(|point| point.ts_secs)
                    .unwrap_or(0),
                end_ts_secs: series.points.last().map(|point| point.ts_secs).unwrap_or(0),
                timepoints_ts_secs: vec![],
                evidence: vec![EvidenceItem {
                    label: "slope".into(),
                    value: format!("{:.4}", stats.slope),
                }],
                impacted_members: 1,
            });
        }
        _ => {}
    }

    dedup_events(events)
}

fn leading_baseline_mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let window = (values.len() / 3).clamp(3, values.len());
    mean(&values[..window])
}

fn detect_spike(values: &[f64], median: f64, scale: f64) -> Option<(usize, f64, usize, Vec<usize>)> {
    let spike_candidates = detect_spike_points(values, median, scale);
    let (index, prominence, dwell) = spike_candidates
        .iter()
        .copied()
        .max_by(|left, right| left.1.total_cmp(&right.1))?;
    let spike_indices = spike_candidates
        .iter()
        .map(|(candidate_index, _, _)| *candidate_index)
        .collect();
    Some((index, prominence, dwell, spike_indices))
}

fn detect_spike_points(values: &[f64], median: f64, scale: f64) -> Vec<(usize, f64, usize)> {
    if values.len() < 3 {
        return Vec::new();
    }

    let mut candidates = Vec::new();
    for index in 1..values.len() - 1 {
        let peak = values[index];
        if peak < values[index - 1] || peak < values[index + 1] {
            continue;
        }
        let prominence = peak - median;
        let local_prominence = peak - values[index - 1].max(values[index + 1]);
        if prominence < scale * 0.85 || local_prominence < scale * 0.35 {
            continue;
        }
        let dwell = local_run(values, index, median + scale * 0.45, true);
        candidates.push((index, prominence, dwell));
    }

    candidates
}

fn detect_drop(values: &[f64], median: f64, scale: f64) -> Option<(usize, f64, usize)> {
    let (index, trough) = values
        .iter()
        .copied()
        .enumerate()
        .min_by(|left, right| left.1.total_cmp(&right.1))?;
    if index == 0 || index + 1 >= values.len() {
        return None;
    }
    let prominence = median - trough;
    let local_prominence = values[index - 1].min(values[index + 1]) - trough;
    if prominence < scale * 0.85 || local_prominence < scale * 0.35 {
        return None;
    }
    let dwell = local_run(values, index, median - scale * 0.45, false);
    Some((index, prominence, dwell))
}

fn local_run(values: &[f64], index: usize, threshold: f64, above: bool) -> usize {
    let mut start = index;
    while start > 0
        && if above {
            values[start - 1] >= threshold
        } else {
            values[start - 1] <= threshold
        }
    {
        start -= 1;
    }

    let mut end = index;
    while end + 1 < values.len()
        && if above {
            values[end + 1] >= threshold
        } else {
            values[end + 1] <= threshold
        }
    {
        end += 1;
    }

    end - start + 1
}

fn tail_run_above(values: &[f64], threshold: f64) -> Option<(usize, usize, f64)> {
    tail_run(values, threshold, true)
}

fn tail_run_below(values: &[f64], threshold: f64) -> Option<(usize, usize, f64)> {
    tail_run(values, threshold, false)
}

fn tail_run(values: &[f64], threshold: f64, above: bool) -> Option<(usize, usize, f64)> {
    let last = *values.last()?;
    let matches = if above {
        last >= threshold
    } else {
        last <= threshold
    };
    if !matches {
        return None;
    }

    let mut start = values.len() - 1;
    while start > 0
        && if above {
            values[start - 1] >= threshold
        } else {
            values[start - 1] <= threshold
        }
    {
        start -= 1;
    }
    let shift = run_shift(&values[start..], mean(values), above);
    Some((start, values.len() - 1, shift))
}

fn run_shift(values: &[f64], global_mean: f64, above: bool) -> f64 {
    let run_mean = mean(values);
    if above {
        (run_mean - global_mean).max(0.0)
    } else {
        (global_mean - run_mean).max(0.0)
    }
}

fn oscillation_signature(values: &[f64], scale: f64) -> Option<(f64, f64)> {
    if values.len() < 5 {
        return None;
    }
    let mut flips = 0;
    let mut prior_sign = 0_i32;
    let mut diffs = Vec::with_capacity(values.len().saturating_sub(1));

    for window in values.windows(2) {
        let diff = window[1] - window[0];
        diffs.push(diff);
        if diff.abs() < scale * 0.25 {
            continue;
        }
        let sign = if diff.is_sign_positive() { 1 } else { -1 };
        if prior_sign != 0 && sign != prior_sign {
            flips += 1;
        }
        prior_sign = sign;
    }

    let density = flips as f64 / (values.len() - 2) as f64;
    let amplitude = std_dev(&diffs);
    if density >= 0.30 && amplitude >= scale * 0.25 {
        Some((density, amplitude))
    } else {
        None
    }
}

fn normalized_slope_strength(series: &NormalizedSeries, stats: &Statistics, scale: f64) -> f64 {
    let duration = series.window_secs.max(series.interval_secs) as f64;
    (stats.slope.abs() * duration / scale).min(4.0)
}

fn summarize_trend(series: &NormalizedSeries, stats: &Statistics, regimes: &[Regime]) -> TrendKind {
    let slope_strength = normalized_slope_strength(
        series,
        stats,
        robust_scale(
            stats.std_dev,
            stats.mad,
            (stats.max - stats.min).abs(),
            stats.mean.abs(),
        ),
    );

    if regimes.len() >= 2 {
        let first = regimes
            .first()
            .map(|regime| regime.mean)
            .unwrap_or(stats.mean);
        let last = regimes
            .last()
            .map(|regime| regime.mean)
            .unwrap_or(stats.mean);
        let tail = &series.points[series.points.len() / 2..];
        let tail_stats = compute_statistics(tail, 2);
        let tail_range = (tail_stats.max - tail_stats.min).abs();

        if last > first && tail_range <= (stats.max - stats.min).abs() * 0.25 {
            return TrendKind::UpThenFlat;
        }
        if last < first && tail_range <= (stats.max - stats.min).abs() * 0.25 {
            return TrendKind::DownThenFlat;
        }
    }

    if slope_strength < 0.35 {
        TrendKind::Flat
    } else if stats.slope.is_sign_positive() {
        TrendKind::Increasing
    } else {
        TrendKind::Decreasing
    }
}

fn dedup_events(events: Vec<Event>) -> Vec<Event> {
    let mut deduped = HashMap::<EventKind, Event>::new();
    for event in events {
        let entry = deduped.entry(event.kind).or_insert_with(|| event.clone());
        if event.score > entry.score {
            *entry = event;
        }
    }
    deduped.into_values().collect()
}

fn sort_events(events: &mut [Event]) {
    events.sort_by(|left, right| {
        right
            .score
            .total_cmp(&left.score)
            .then_with(|| left.kind.priority().cmp(&right.kind.priority()))
            .then_with(|| left.start_ts_secs.cmp(&right.start_ts_secs))
    });
}

fn derive_state(events: &[Event]) -> StateKind {
    if events
        .iter()
        .any(|event| event.kind == EventKind::SustainedHigh)
    {
        StateKind::Elevated
    } else if events
        .iter()
        .any(|event| event.kind == EventKind::SustainedLow)
    {
        StateKind::Depressed
    } else if events
        .iter()
        .any(|event| event.kind == EventKind::Oscillation)
    {
        StateKind::Volatile
    } else {
        StateKind::Stable
    }
}

fn collect_analysis_evidence(events: &[Event]) -> Vec<EvidenceItem> {
    let mut evidence = Vec::new();
    for event in events {
        for item in event.evidence.iter().take(2) {
            if evidence
                .iter()
                .all(|existing: &EvidenceItem| existing.label != item.label)
            {
                evidence.push(item.clone());
            }
        }
    }
    evidence
}

fn attach_peer_context(items: &mut [WorkingAnalysis]) {
    let mut ordering: Vec<(usize, f64)> = items
        .iter()
        .enumerate()
        .map(|(index, item)| (index, item.magnitude_anchor))
        .collect();
    ordering.sort_by(|left, right| right.1.total_cmp(&left.1));

    let total = ordering.len().max(1);
    for (rank_index, (item_index, _)) in ordering.into_iter().enumerate() {
        let rank = rank_index + 1;
        let percentile = ((total - rank) * 100) / total;
        items[item_index].analysis.peer_context = Some(PeerContext {
            rank,
            total,
            percentile,
        });
    }
}

fn format_percent(value: f64) -> String {
    format!("{:+.0}%", value * 100.0)
}

#[cfg(test)]
mod tests {
    use compiler_schema::{EventKind, LogicalSeries, TimeSeriesPoint};

    use super::{CompilerPolicy, GroupInput, analyze_groups, analyze_lines};
    use crate::project_output;

    fn build_series(
        metric_id: &str,
        entity_id: &str,
        group_id: &str,
        values: &[f64],
    ) -> LogicalSeries {
        LogicalSeries {
            metric_id: metric_id.into(),
            entity_id: entity_id.into(),
            group_id: group_id.into(),
            labels: vec![],
            points: values
                .iter()
                .enumerate()
                .map(|(index, value)| TimeSeriesPoint {
                    ts_secs: index as i64 * 60,
                    value: *value,
                })
                .collect(),
        }
    }

    #[test]
    fn line_analysis_detects_expected_top_events() {
        let series = build_series(
            "tikv_cpu",
            "tikv-1/coprocessor",
            "tikv-1",
            &[
                10.0, 10.2, 10.5, 10.6, 15.0, 18.0, 18.3, 18.5, 28.0, 18.6, 18.4, 18.2,
            ],
        );
        let analyses = analyze_lines(&[series], &CompilerPolicy::default());
        let kinds: Vec<EventKind> = analyses[0]
            .top_events
            .iter()
            .map(|event| event.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![
                EventKind::SustainedHigh,
                EventKind::RegimeShift,
                EventKind::Spike
            ]
        );
    }

    #[test]
    fn group_analysis_aggregates_member_events() {
        let group = GroupInput {
            metric_id: "tikv_cpu".into(),
            group_id: "tikv-1".into(),
            members: vec![
                build_series(
                    "tikv_cpu",
                    "tikv-1/coprocessor",
                    "tikv-1",
                    &[10.0, 10.1, 10.2, 15.0, 18.0, 18.2, 18.4, 26.0, 18.3, 18.1],
                ),
                build_series(
                    "tikv_cpu",
                    "tikv-1/grpc",
                    "tikv-1",
                    &[9.8, 10.0, 10.1, 14.0, 16.0, 16.3, 16.5, 16.7, 16.6, 16.5],
                ),
            ],
        };

        let analyses = analyze_groups(&[group], &CompilerPolicy::default());
        let kinds: Vec<EventKind> = analyses[0]
            .top_events
            .iter()
            .map(|event| event.kind)
            .collect();
        assert_eq!(
            kinds,
            vec![
                EventKind::SustainedHigh,
                EventKind::RegimeShift,
                EventKind::Spike
            ]
        );
        let output = project_output(&analyses[0]);
        assert!(
            output
                .description
                .contains("top_events=[sustained_high, regime_shift, spike]")
        );
    }

    #[test]
    fn line_recovery_does_not_look_like_sustained_high() {
        let series = build_series(
            "qps",
            "select/tidb-1",
            "select",
            &[120.0, 121.0, 120.0, 121.0, 80.0, 78.0, 82.0, 120.0, 121.0, 120.0, 121.0, 120.0],
        );
        let analyses = analyze_lines(&[series], &CompilerPolicy::default());
        let kinds: Vec<EventKind> = analyses[0].top_events.iter().map(|event| event.kind).collect();
        assert!(!kinds.contains(&EventKind::SustainedHigh));
    }

    #[test]
    fn group_analysis_detects_peer_imbalance_for_stable_split() {
        let group = GroupInput {
            metric_id: "qps".into(),
            group_id: "select".into(),
            members: vec![
                build_series("qps", "select/tidb-1", "select", &[140.0; 12]),
                build_series("qps", "select/tidb-2", "select", &[120.0; 12]),
                build_series("qps", "select/tidb-3", "select", &[119.0; 12]),
            ],
        };

        let analyses = analyze_groups(&[group], &CompilerPolicy::default());
        let kinds: Vec<EventKind> = analyses[0].top_events.iter().map(|event| event.kind).collect();
        assert_eq!(kinds, vec![EventKind::PeerImbalance]);
    }
}
