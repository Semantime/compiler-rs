use compiler_schema::{LogicalSeries, NormalizedSeries, TimeSeriesPoint};

pub fn normalize_series(series: &LogicalSeries) -> Option<NormalizedSeries> {
    let mut points: Vec<TimeSeriesPoint> = series
        .points
        .iter()
        .filter(|point| point.value.is_finite())
        .cloned()
        .collect();
    points.sort_by_key(|point| point.ts_secs);

    let mut deduped: Vec<TimeSeriesPoint> = Vec::with_capacity(points.len());
    for point in points {
        if let Some(last) = deduped.last_mut()
            && last.ts_secs == point.ts_secs
        {
            *last = point;
            continue;
        }
        deduped.push(point);
    }

    if deduped.len() < 3 {
        return None;
    }

    let base_ts_secs = deduped.first()?.ts_secs;
    let window_start_ts_secs = looks_like_unix_timestamp(base_ts_secs).then_some(base_ts_secs);
    for point in &mut deduped {
        point.ts_secs -= base_ts_secs;
    }

    let mut deltas: Vec<i64> = deduped
        .windows(2)
        .map(|window| window[1].ts_secs - window[0].ts_secs)
        .filter(|delta| *delta > 0)
        .collect();
    deltas.sort_unstable();

    let interval_secs = if deltas.is_empty() {
        60
    } else if deltas.len() % 2 == 0 {
        let upper = deltas[deltas.len() / 2];
        let lower = deltas[deltas.len() / 2 - 1];
        (upper + lower) / 2
    } else {
        deltas[deltas.len() / 2]
    };

    let window_secs = deduped.last()?.ts_secs - deduped.first()?.ts_secs;

    Some(NormalizedSeries {
        metric_id: series.metric_id.clone(),
        entity_id: series.entity_id.clone(),
        group_id: series.group_id.clone(),
        labels: series.labels.clone(),
        points: deduped,
        window_start_ts_secs,
        interval_secs,
        window_secs,
    })
}

fn looks_like_unix_timestamp(ts_secs: i64) -> bool {
    (946_684_800..=4_102_444_800).contains(&ts_secs)
}

#[cfg(test)]
mod tests {
    use compiler_schema::{LogicalSeries, TimeSeriesPoint};

    use super::normalize_series;

    #[test]
    fn normalize_sorts_and_dedups_points() {
        let series = LogicalSeries {
            metric_id: "metric".into(),
            entity_id: "entity".into(),
            group_id: "group".into(),
            labels: vec![],
            points: vec![
                TimeSeriesPoint {
                    ts_secs: 120,
                    value: 2.0,
                },
                TimeSeriesPoint {
                    ts_secs: 60,
                    value: 1.0,
                },
                TimeSeriesPoint {
                    ts_secs: 120,
                    value: 3.0,
                },
                TimeSeriesPoint {
                    ts_secs: 180,
                    value: 4.0,
                },
            ],
        };

        let normalized = normalize_series(&series).expect("normalized");
        let values: Vec<f64> = normalized.points.iter().map(|point| point.value).collect();
        assert_eq!(values, vec![1.0, 3.0, 4.0]);
        assert_eq!(normalized.points[0].ts_secs, 0);
        assert_eq!(normalized.points[1].ts_secs, 60);
        assert_eq!(normalized.interval_secs, 60);
    }

    #[test]
    fn normalize_rebases_absolute_timestamps() {
        let series = LogicalSeries {
            metric_id: "metric".into(),
            entity_id: "entity".into(),
            group_id: "group".into(),
            labels: vec![],
            points: vec![
                TimeSeriesPoint {
                    ts_secs: 1_743_415_200,
                    value: 100.0,
                },
                TimeSeriesPoint {
                    ts_secs: 1_743_415_260,
                    value: 101.0,
                },
                TimeSeriesPoint {
                    ts_secs: 1_743_415_320,
                    value: 102.0,
                },
            ],
        };

        let normalized = normalize_series(&series).expect("normalized");
        let offsets: Vec<i64> = normalized.points.iter().map(|point| point.ts_secs).collect();
        assert_eq!(offsets, vec![0, 60, 120]);
        assert_eq!(normalized.window_secs, 120);
    }
}
