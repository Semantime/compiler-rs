use compiler_schema::{Statistics, TimeSeriesPoint};
use linreg::linear_regression;
use statrs::statistics::Statistics as StatsExt;

pub fn compute_statistics(points: &[TimeSeriesPoint], max_paa_segments: usize) -> Statistics {
    let values: Vec<f64> = points.iter().map(|point| point.value).collect();
    let mean = mean(&values);
    let std_dev = std_dev(&values);
    let min = if values.is_empty() {
        0.0
    } else {
        sanitize_stat(StatsExt::min(values.as_slice()))
    };
    let max = if values.is_empty() {
        0.0
    } else {
        sanitize_stat(StatsExt::max(values.as_slice()))
    };
    let median = median(&values);
    let mad = median_abs_deviation(&values, median);
    let slope = linear_slope(points);
    let paa = paa(&values, max_paa_segments.max(1));

    Statistics {
        mean,
        std_dev,
        min,
        max,
        median,
        mad,
        slope,
        paa,
    }
}

pub fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    sanitize_stat(StatsExt::mean(values))
}

pub fn std_dev(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    sanitize_stat(StatsExt::population_std_dev(values))
}

pub fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    if sorted.len() % 2 == 0 {
        let upper = sorted[sorted.len() / 2];
        let lower = sorted[sorted.len() / 2 - 1];
        (upper + lower) / 2.0
    } else {
        sorted[sorted.len() / 2]
    }
}

pub fn quantile(values: &[f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|left, right| left.total_cmp(right));
    if sorted.len() == 1 {
        return sorted[0];
    }
    let clamped = q.clamp(0.0, 1.0);
    let index = clamped * (sorted.len() - 1) as f64;
    let lower = index.floor() as usize;
    let upper = index.ceil() as usize;
    if lower == upper {
        sorted[lower]
    } else {
        let weight = index - lower as f64;
        sorted[lower] * (1.0 - weight) + sorted[upper] * weight
    }
}

pub fn median_abs_deviation(values: &[f64], center: f64) -> f64 {
    let deviations: Vec<f64> = values.iter().map(|value| (*value - center).abs()).collect();
    median(&deviations)
}

pub fn robust_scale(std_dev: f64, mad: f64, range: f64, mean_abs: f64) -> f64 {
    let mad_scale = mad * 1.4826;
    let range_scale = range * 0.15;
    let mean_scale = mean_abs * 0.08;
    std_dev
        .max(mad_scale)
        .max(range_scale)
        .max(mean_scale)
        .max(1e-6)
}

pub fn linear_slope(points: &[TimeSeriesPoint]) -> f64 {
    if points.len() < 2 {
        return 0.0;
    }

    let xs: Vec<f64> = points.iter().map(|point| point.ts_secs as f64).collect();
    let ys: Vec<f64> = points.iter().map(|point| point.value).collect();

    match linear_regression(&xs, &ys) {
        Ok((slope, _intercept)) => sanitize_stat(slope),
        Err(_) => 0.0,
    }
}

pub fn paa(values: &[f64], segments: usize) -> Vec<f64> {
    if values.is_empty() {
        return Vec::new();
    }
    let segments = segments.min(values.len()).max(1);
    let mut output = Vec::with_capacity(segments);
    for segment in 0..segments {
        let start = segment * values.len() / segments;
        let end = ((segment + 1) * values.len() / segments).max(start + 1);
        output.push(mean(&values[start..end]));
    }
    output
}

pub fn format_duration(seconds: i64) -> String {
    if seconds >= 3600 && seconds % 3600 == 0 {
        format!("{}h", seconds / 3600)
    } else if seconds >= 60 {
        format!("{}m", seconds / 60)
    } else {
        format!("{}s", seconds)
    }
}

fn sanitize_stat(value: f64) -> f64 {
    if value.is_finite() { value } else { 0.0 }
}

#[cfg(test)]
mod tests {
    use compiler_schema::TimeSeriesPoint;

    use super::{compute_statistics, quantile};

    #[test]
    fn compute_stats_and_paa() {
        let points = vec![
            TimeSeriesPoint {
                ts_secs: 0,
                value: 1.0,
            },
            TimeSeriesPoint {
                ts_secs: 60,
                value: 2.0,
            },
            TimeSeriesPoint {
                ts_secs: 120,
                value: 3.0,
            },
            TimeSeriesPoint {
                ts_secs: 180,
                value: 4.0,
            },
        ];
        let stats = compute_statistics(&points, 2);
        assert_eq!(stats.paa, vec![1.5, 3.5]);
        assert!((stats.slope - (1.0 / 60.0)).abs() < 1e-9);
    }

    #[test]
    fn quantile_interpolates() {
        let values = vec![1.0, 2.0, 3.0, 4.0];
        assert_eq!(quantile(&values, 0.75), 3.25);
    }
}
