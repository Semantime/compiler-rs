use compiler_schema::{CanonicalAnalysis, LlmOutput};
use crate::features::format_duration;
use time::{OffsetDateTime, UtcOffset};

pub fn project_output(analysis: &CanonicalAnalysis) -> LlmOutput {
    let top_events = analysis
        .top_events
        .iter()
        .map(|event| event.kind.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let problem_range = summarize_problem_range(analysis);
    let problem_time = summarize_problem_time(analysis);
    let event_times = summarize_event_times(analysis);
    let evidence = analysis
        .evidence
        .iter()
        .take(3)
        .map(|item| format!("{}:{}", item.label, item.value))
        .collect::<Vec<_>>()
        .join(", ");

    let mut fields = vec![
        format!("window={}", format_duration(analysis.window_secs.max(0))),
        format!("problem_range={problem_range}"),
        format!("problem_time={problem_time}"),
        format!("state={}", analysis.state.as_str()),
        format!("trend={}", analysis.trend.as_str()),
        format!("top_events=[{top_events}]"),
    ];

    if !event_times.is_empty() {
        fields.push(format!("event_times=[{event_times}]"));
    }

    if let Some(peer_context) = &analysis.peer_context {
        fields.push(format!("rank={}/{}", peer_context.rank, peer_context.total));
        fields.push(format!("percentile={}", peer_context.percentile));
    }

    if !evidence.is_empty() {
        fields.push(format!("evidence=[{evidence}]"));
    }

    LlmOutput {
        schema_version: analysis.schema_version,
        metric_id: analysis.metric_id.clone(),
        scope: analysis.scope,
        subject_id: analysis.subject_id.clone(),
        description: fields.join("; "),
    }
}

fn summarize_problem_range(analysis: &CanonicalAnalysis) -> String {
    let (start, end) = infer_problem_range_bounds(analysis);
    format!("{}->{}", format_offset(start), format_offset(end))
}

fn summarize_problem_time(analysis: &CanonicalAnalysis) -> String {
    let Some(window_start_ts_secs) = analysis.window_start_ts_secs else {
        return summarize_problem_range(analysis);
    };
    let (start, end) = infer_problem_range_bounds(analysis);
    format_absolute_range(window_start_ts_secs + start, window_start_ts_secs + end)
}

fn summarize_event_times(analysis: &CanonicalAnalysis) -> String {
    analysis
        .top_events
        .iter()
        .map(|event| {
            let timing = if let Some(window_start_ts_secs) = analysis.window_start_ts_secs {
                if !event.timepoints_ts_secs.is_empty() {
                    format_absolute_points(window_start_ts_secs, &event.timepoints_ts_secs)
                } else {
                    format_absolute_range(
                        window_start_ts_secs + event.start_ts_secs,
                        window_start_ts_secs + event.end_ts_secs,
                    )
                }
            } else if !event.timepoints_ts_secs.is_empty() {
                format_relative_points(&event.timepoints_ts_secs)
            } else {
                format!("{}->{}", format_offset(event.start_ts_secs), format_offset(event.end_ts_secs))
            };
            format!("{}@{}", event.kind.as_str(), timing)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn infer_problem_range_bounds(analysis: &CanonicalAnalysis) -> (i64, i64) {
    let focused_events: Vec<_> = analysis
        .top_events
        .iter()
        .filter(|event| event.start_ts_secs < event.end_ts_secs)
        .filter(|event| {
            !matches!(
                event.kind,
                compiler_schema::EventKind::IncreasingTrend
                    | compiler_schema::EventKind::DecreasingTrend
            )
        })
        .collect();

    if let Some((start, end)) = focused_events
        .iter()
        .map(|event| (event.start_ts_secs, event.end_ts_secs))
        .reduce(|(left_start, left_end), (right_start, right_end)| {
            (left_start.min(right_start), left_end.max(right_end))
        })
    {
        return (start, end);
    }

    if let Some(event) = analysis.top_events.first() {
        return (event.start_ts_secs, event.end_ts_secs);
    }

    (0, analysis.window_secs.max(0))
}

fn format_offset(ts_secs: i64) -> String {
    let ts_secs = ts_secs.max(0);
    if ts_secs == 0 {
        "0m".into()
    } else {
        format_duration(ts_secs)
    }
}

fn format_absolute_range(start_ts_secs: i64, end_ts_secs: i64) -> String {
    let Ok(start) = OffsetDateTime::from_unix_timestamp(start_ts_secs) else {
        return format!("{}s->{}s", start_ts_secs, end_ts_secs);
    };
    let Ok(end) = OffsetDateTime::from_unix_timestamp(end_ts_secs) else {
        return format!("{}s->{}s", start_ts_secs, end_ts_secs);
    };
    let start = start.to_offset(UtcOffset::UTC);
    let end = end.to_offset(UtcOffset::UTC);

    if start.date() == end.date() {
        return format!(
            "{:04}-{:02}-{:02} {:02}:{:02}->{:02}:{:02} UTC",
            start.year(),
            u8::from(start.month()),
            start.day(),
            start.hour(),
            start.minute(),
            end.hour(),
            end.minute()
        );
    }

    format!(
        "{:04}-{:02}-{:02} {:02}:{:02} UTC->{:04}-{:02}-{:02} {:02}:{:02} UTC",
        start.year(),
        u8::from(start.month()),
        start.day(),
        start.hour(),
        start.minute(),
        end.year(),
        u8::from(end.month()),
        end.day(),
        end.hour(),
        end.minute()
    )
}

fn format_absolute_points(window_start_ts_secs: i64, timepoints_ts_secs: &[i64]) -> String {
    let mut labels = Vec::new();
    for timepoint_ts_secs in timepoints_ts_secs {
        let Ok(point) = OffsetDateTime::from_unix_timestamp(window_start_ts_secs + *timepoint_ts_secs) else {
            labels.push(format!("{timepoint_ts_secs}s"));
            continue;
        };
        let point = point.to_offset(UtcOffset::UTC);
        labels.push(format!("{:02}:{:02}", point.hour(), point.minute()));
    }
    format!("{} UTC", labels.join(","))
}

fn format_relative_points(timepoints_ts_secs: &[i64]) -> String {
    timepoints_ts_secs
        .iter()
        .map(|timepoint_ts_secs| format_offset(*timepoint_ts_secs))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use compiler_schema::{
        CanonicalAnalysis, Event, EventKind, EvidenceItem, Scope, StateKind, TrendKind,
    };

    use super::project_output;

    #[test]
    fn projection_uses_single_description_field() {
        let analysis = CanonicalAnalysis {
            schema_version: "v1",
            scope: Scope::Line,
            metric_id: "metric".into(),
            subject_id: "subject".into(),
            window_start_ts_secs: Some(1_774_951_200),
            window_secs: 600,
            state: StateKind::Elevated,
            trend: TrendKind::UpThenFlat,
            top_events: vec![Event {
                kind: EventKind::SustainedHigh,
                score: 8.0,
                start_ts_secs: 0,
                end_ts_secs: 600,
                timepoints_ts_secs: vec![],
                evidence: vec![EvidenceItem {
                    label: "duration".into(),
                    value: "10m".into(),
                }],
                impacted_members: 1,
            }],
            peer_context: None,
            regimes: vec![],
            evidence: vec![EvidenceItem {
                label: "duration".into(),
                value: "10m".into(),
            }],
        };

        let output = project_output(&analysis);
        assert!(output.description.contains("window=10m"));
        assert!(output.description.contains("problem_range=0m->10m"));
        assert!(output.description.contains("problem_time=2026-03-31 10:00->10:10 UTC"));
        assert!(output.description.contains("top_events=[sustained_high]"));
        assert!(output.description.contains("event_times=[sustained_high@2026-03-31 10:00->10:10 UTC]"));
    }
}
