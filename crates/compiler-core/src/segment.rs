use std::num::NonZero;

use pelt::{Pelt, SegmentCostFunction};

use crate::{
    analyze::SensitivityProfile,
    features::{mean, robust_scale, std_dev},
};
use compiler_schema::{NormalizedSeries, Regime, Statistics};

pub fn segment_series(
    series: &NormalizedSeries,
    stats: &Statistics,
    sensitivity: SensitivityProfile,
) -> Vec<Regime> {
    let values: Vec<f64> = series.points.iter().map(|point| point.value).collect();
    if values.len() < 5 {
        return single_regime(series, stats);
    }

    let minimum_segment_length = derive_minimum_segment_length(series);
    if values.len() < minimum_segment_length.saturating_mul(2) {
        return single_regime(series, stats);
    }

    let standardized = standardize_values(&values, stats);
    let penalty = derive_penalty(series, &standardized, sensitivity);
    let changepoints = Pelt::new()
        .with_jump(NonZero::new(minimum_segment_length).expect("jump must be non-zero"))
        .with_minimum_segment_length(
            NonZero::new(minimum_segment_length).expect("minimum segment length must be non-zero"),
        )
        .with_segment_cost_function(SegmentCostFunction::L1)
        .predict(&standardized, penalty)
        .unwrap_or_default();
    let changepoints: Vec<usize> = changepoints
        .into_iter()
        .filter(|index| *index > 0 && *index < values.len())
        .collect();

    let regimes = build_regimes(series, &changepoints);
    if regimes.is_empty() {
        single_regime(series, stats)
    } else {
        regimes
    }
}

fn single_regime(series: &NormalizedSeries, stats: &Statistics) -> Vec<Regime> {
    vec![Regime {
        start_ts_secs: series
            .points
            .first()
            .map(|point| point.ts_secs)
            .unwrap_or(0),
        end_ts_secs: series.points.last().map(|point| point.ts_secs).unwrap_or(0),
        mean: stats.mean,
        delta_from_prev: None,
    }]
}

fn derive_minimum_segment_length(series: &NormalizedSeries) -> usize {
    let interval_secs = series.interval_secs.max(1);
    let target_duration_secs = (series.window_secs / 4).max(interval_secs * 4);
    let target_points = ((target_duration_secs + interval_secs - 1) / interval_secs) as usize;
    let upper_bound = (series.points.len() / 2).max(3);
    target_points.clamp(2, upper_bound)
}

fn standardize_values(values: &[f64], stats: &Statistics) -> Vec<f64> {
    let scale = robust_scale(
        stats.std_dev,
        stats.mad,
        (stats.max - stats.min).abs(),
        stats.mean.abs(),
    );
    values
        .iter()
        .map(|value| (*value - stats.median) / scale)
        .collect()
}

fn derive_penalty(
    series: &NormalizedSeries,
    standardized: &[f64],
    sensitivity: SensitivityProfile,
) -> f64 {
    let sample_count = series.points.len().max(2) as f64;
    let sampling_factor = sample_count.ln().max(1.0);
    let diff_noise = normalized_diff_noise(standardized);
    let head_tail_shift = head_tail_shift(standardized);
    let sensitivity_multiplier = match sensitivity {
        SensitivityProfile::Conservative => 1.25,
        SensitivityProfile::Balanced => 1.0,
        SensitivityProfile::Aggressive => 0.75,
    };

    ((sampling_factor * 1.1) + (diff_noise * 0.7) - (head_tail_shift * 0.6)).max(1.0)
        * sensitivity_multiplier
}

fn normalized_diff_noise(values: &[f64]) -> f64 {
    let diffs: Vec<f64> = values
        .windows(2)
        .map(|window| window[1] - window[0])
        .collect();
    std_dev(&diffs).clamp(0.0, 3.0)
}

fn head_tail_shift(values: &[f64]) -> f64 {
    if values.len() < 6 {
        return 0.0;
    }

    let segment_len = (values.len() / 3).max(2);
    let head = &values[..segment_len];
    let tail = &values[values.len() - segment_len..];
    (mean(head) - mean(tail)).abs().clamp(0.0, 4.0)
}

fn build_regimes(series: &NormalizedSeries, changepoints: &[usize]) -> Vec<Regime> {
    let mut boundaries = Vec::with_capacity(changepoints.len() + 2);
    boundaries.push(0);
    boundaries.extend(changepoints.iter().copied().filter(|idx| *idx > 0));
    boundaries.push(series.points.len());

    let mut regimes = Vec::new();
    let mut previous_mean = None;
    for window in boundaries.windows(2) {
        let start = window[0];
        let end = window[1];
        if end <= start {
            continue;
        }
        let slice = &series.points[start..end];
        let mean = slice.iter().map(|point| point.value).sum::<f64>() / slice.len() as f64;
        let delta_from_prev = previous_mean.map(|previous| mean - previous);
        previous_mean = Some(mean);
        regimes.push(Regime {
            start_ts_secs: slice.first().map(|point| point.ts_secs).unwrap_or(0),
            end_ts_secs: slice.last().map(|point| point.ts_secs).unwrap_or(0),
            mean,
            delta_from_prev,
        });
    }
    regimes
}

#[cfg(test)]
mod tests {
    use crate::analyze::SensitivityProfile;
    use compiler_schema::{LogicalSeries, TimeSeriesPoint};

    use crate::{features::compute_statistics, normalize::normalize_series};

    use super::segment_series;

    #[test]
    fn detects_regime_shift_for_step_change() {
        let series = LogicalSeries {
            metric_id: "metric".into(),
            entity_id: "entity".into(),
            group_id: "group".into(),
            labels: vec![],
            points: [10.0, 10.5, 11.0, 10.8, 18.0, 18.2, 18.4, 18.3]
                .into_iter()
                .enumerate()
                .map(|(idx, value)| TimeSeriesPoint {
                    ts_secs: idx as i64 * 60,
                    value,
                })
                .collect(),
        };

        let normalized = normalize_series(&series).expect("normalized");
        let stats = compute_statistics(&normalized.points, 4);
        let regimes = segment_series(&normalized, &stats, SensitivityProfile::Balanced);
        assert!(regimes.len() >= 2);
    }

    #[test]
    fn keeps_oscillation_as_single_regime() {
        let series = LogicalSeries {
            metric_id: "metric".into(),
            entity_id: "entity".into(),
            group_id: "group".into(),
            labels: vec![],
            points: [
                10.0, 15.0, 9.0, 18.0, 8.0, 15.5, 9.5, 16.5, 8.5, 15.0, 9.0, 16.0,
            ]
            .into_iter()
            .enumerate()
            .map(|(idx, value)| TimeSeriesPoint {
                ts_secs: idx as i64 * 60,
                value,
            })
            .collect(),
        };

        let normalized = normalize_series(&series).expect("normalized");
        let stats = compute_statistics(&normalized.points, 4);
        let regimes = segment_series(&normalized, &stats, SensitivityProfile::Balanced);
        assert_eq!(regimes.len(), 1);
    }
}
