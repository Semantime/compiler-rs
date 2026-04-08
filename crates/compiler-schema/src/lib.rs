use serde::{Deserialize, Deserializer, Serialize, de};
use std::fmt;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

pub const SCHEMA_VERSION: &str = "v1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Scope {
    Line,
    Group,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::Group => "group",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateKind {
    Stable,
    Elevated,
    Depressed,
    Volatile,
}

impl StateKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Elevated => "elevated",
            Self::Depressed => "depressed",
            Self::Volatile => "volatile",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrendKind {
    Increasing,
    Decreasing,
    Flat,
    UpThenFlat,
    DownThenFlat,
}

impl TrendKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Increasing => "increasing",
            Self::Decreasing => "decreasing",
            Self::Flat => "flat",
            Self::UpThenFlat => "up_then_flat",
            Self::DownThenFlat => "down_then_flat",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    SustainedHigh,
    SustainedLow,
    Spike,
    Drop,
    RegimeShift,
    Oscillation,
    IncreasingTrend,
    DecreasingTrend,
    PeerImbalance,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SustainedHigh => "sustained_high",
            Self::SustainedLow => "sustained_low",
            Self::Spike => "spike",
            Self::Drop => "drop",
            Self::RegimeShift => "regime_shift",
            Self::Oscillation => "oscillation",
            Self::IncreasingTrend => "increasing_trend",
            Self::DecreasingTrend => "decreasing_trend",
            Self::PeerImbalance => "peer_imbalance",
        }
    }

    pub fn priority(self) -> usize {
        match self {
            Self::SustainedHigh => 0,
            Self::SustainedLow => 1,
            Self::Spike => 2,
            Self::Drop => 3,
            Self::RegimeShift => 4,
            Self::Oscillation => 5,
            Self::IncreasingTrend => 6,
            Self::DecreasingTrend => 7,
            Self::PeerImbalance => 8,
        }
    }
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str((*self).as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub label: String,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TimeSeriesPoint {
    pub ts_secs: i64,
    pub value: f64,
}

impl<'de> Deserialize<'de> for TimeSeriesPoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum PointRepr {
            Relative { ts_secs: i64, value: f64 },
            Timestamp { ts: String, value: f64 },
            IsoTuple(String, f64),
            SecondsTuple(i64, f64),
        }

        let point = match PointRepr::deserialize(deserializer)? {
            PointRepr::Relative { ts_secs, value } => Self { ts_secs, value },
            PointRepr::Timestamp { ts, value } => Self {
                ts_secs: parse_rfc3339(&ts).map_err(de::Error::custom)?,
                value,
            },
            PointRepr::IsoTuple(ts, value) => Self {
                ts_secs: parse_rfc3339(&ts).map_err(de::Error::custom)?,
                value,
            },
            PointRepr::SecondsTuple(ts_secs, value) => Self { ts_secs, value },
        };

        Ok(point)
    }
}

fn parse_rfc3339(ts: &str) -> Result<i64, String> {
    OffsetDateTime::parse(ts, &Rfc3339)
        .map(|datetime| datetime.unix_timestamp())
        .map_err(|err| format!("invalid RFC3339 timestamp `{ts}`: {err}"))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LogicalSeries {
    pub metric_id: String,
    pub entity_id: String,
    pub group_id: String,
    #[serde(default)]
    pub labels: Vec<(String, String)>,
    pub points: Vec<TimeSeriesPoint>,
}

#[derive(Clone, Debug, Serialize)]
pub struct NormalizedSeries {
    pub metric_id: String,
    pub entity_id: String,
    pub group_id: String,
    pub labels: Vec<(String, String)>,
    pub points: Vec<TimeSeriesPoint>,
    pub window_start_ts_secs: Option<i64>,
    pub interval_secs: i64,
    pub window_secs: i64,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Statistics {
    pub mean: f64,
    pub std_dev: f64,
    pub min: f64,
    pub max: f64,
    pub median: f64,
    pub mad: f64,
    pub slope: f64,
    pub paa: Vec<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Regime {
    pub start_ts_secs: i64,
    pub end_ts_secs: i64,
    pub mean: f64,
    pub delta_from_prev: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Event {
    pub kind: EventKind,
    pub score: f64,
    pub start_ts_secs: i64,
    pub end_ts_secs: i64,
    #[serde(default)]
    pub timepoints_ts_secs: Vec<i64>,
    pub evidence: Vec<EvidenceItem>,
    pub impacted_members: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PeerContext {
    pub rank: usize,
    pub total: usize,
    pub percentile: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CanonicalAnalysis {
    pub schema_version: &'static str,
    pub scope: Scope,
    pub metric_id: String,
    pub subject_id: String,
    pub window_start_ts_secs: Option<i64>,
    pub window_secs: i64,
    pub state: StateKind,
    pub trend: TrendKind,
    pub top_events: Vec<Event>,
    pub peer_context: Option<PeerContext>,
    pub regimes: Vec<Regime>,
    pub evidence: Vec<EvidenceItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LlmOutput {
    pub schema_version: &'static str,
    pub metric_id: String,
    pub scope: Scope,
    pub subject_id: String,
    pub description: String,
}

impl LlmOutput {
    pub fn to_json_string(&self) -> String {
        format!(
            "{{\"schema_version\":\"{}\",\"metric_id\":\"{}\",\"scope\":\"{}\",\"subject_id\":\"{}\",\"description\":\"{}\"}}",
            escape_json(self.schema_version),
            escape_json(&self.metric_id),
            escape_json(self.scope.as_str()),
            escape_json(&self.subject_id),
            escape_json(&self.description)
        )
    }
}

fn escape_json(value: &str) -> String {
    value
        .chars()
        .flat_map(|ch| match ch {
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            _ => vec![ch],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::TimeSeriesPoint;

    #[test]
    fn point_deserializes_from_relative_object() {
        let point: TimeSeriesPoint =
            serde_json::from_str(r#"{"ts_secs":60,"value":42.0}"#).expect("point");
        assert_eq!(point.ts_secs, 60);
        assert_eq!(point.value, 42.0);
    }

    #[test]
    fn point_deserializes_from_rfc3339_tuple() {
        let point: TimeSeriesPoint =
            serde_json::from_str(r#"["2026-03-31T10:04:00Z",116.0]"#).expect("point");
        assert_eq!(point.ts_secs, 1_774_951_440);
        assert_eq!(point.value, 116.0);
    }
}
