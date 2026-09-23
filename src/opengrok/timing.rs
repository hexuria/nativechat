//! Per-turn wall-clock from the OpenGrok harness, when the server sends it.
//!
//! NativeChat does not invent the numbers. It reads a CUSTOM frame and paints
//! what is there. A server that never emits one is silent here: the bubble
//! still wears how long the run took (start → finish on this machine), and
//! the phase list is simply absent.
//!
//! # CUSTOM payload (`v: 1`)
//!
//! The harness names the frame `run-timing` (alias `turn-timeline`). The
//! object lives in `value`, or on the event itself if `value` is missing.
//!
//! ```json
//! {
//!   "type": "CUSTOM",
//!   "name": "run-timing",
//!   "value": {
//!     "v": 1,
//!     "total_ms": 372123,
//!     "model_ms": 12000,
//!     "auto_review_ms": 8000,
//!     "rounds": [
//!       { "model_ms": 4000 },
//!       { "model_ms": 8000 }
//!     ],
//!     "tools": [
//!       { "name": "profile.list", "ms": 350000 }
//!     ]
//!   }
//! }
//! ```
//!
//! `opengrok-harness` sends no `v` and no `rounds`: its `model_ms` is an
//! array with one number per model round, beside `tool_wait_ms` and
//! `tool_rounds`. Both shapes read.
//!
//! Unknown fields are ignored. `v` greater than 1 is still read for the
//! keys this build knows. Absence of every timing field is not a payload.
//! CamelCase aliases (`totalMs`, `modelMs`, `autoReviewMs`, `durationMs`)
//! are accepted so a server PR can land either spelling.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::Duration;

/// CUSTOM `name` the harness is asked to use.
pub const RUN_TIMING_CUSTOM: &str = "run-timing";
/// Alias while the server PR settles on a name.
pub const TURN_TIMELINE_CUSTOM: &str = "turn-timeline";

/// Schema version this client writes and prefers to read.
pub const TIMING_SCHEMA_V: u32 = 1;

/// One tool's wall clock inside a turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolTiming {
    pub name: String,
    pub ms: u64,
}

/// One model round's wall clock. A turn that called tools twice has two.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoundTiming {
    pub model_ms: u64,
}

/// Phases of one run, as the harness measured them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TurnTiming {
    pub v: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_review_ms: Option<u64>,
    /// Wall clock of the tool batches, which run side by side: not the sum of `tools`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_wait_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_rounds: Option<u32>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rounds: Vec<RoundTiming>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolTiming>,
}

impl TurnTiming {
    /// The CUSTOM frame, if it is a timing event this build can read.
    pub fn from_event(event: &Value) -> Option<Self> {
        if !is_timing_event(event) {
            return None;
        }
        let value = event.get("value").unwrap_or(event);
        match value {
            Value::String(raw) => serde_json::from_str(raw)
                .ok()
                .as_ref()
                .and_then(Self::from_value),
            other => Self::from_value(other),
        }
    }

    /// The last timing frame in a journal, if any. Earlier ones are drafts.
    pub fn from_events(events: &[Value]) -> Option<Self> {
        events.iter().rev().find_map(Self::from_event)
    }

    /// The object inside `value` (or the event itself).
    pub fn from_value(value: &Value) -> Option<Self> {
        let v = u32_at(value, &["v", "version"]).unwrap_or(TIMING_SCHEMA_V);
        if v == 0 {
            return None;
        }
        let total_ms = ms_at(value, &["total_ms", "totalMs"]);
        // The harness sends `model_ms` as one number per model round; the
        // documented shape is a total beside a `rounds` list.
        let model = ["model_ms", "modelMs"]
            .iter()
            .find_map(|key| value.get(*key));
        let model_ms = model.and_then(as_ms);
        let auto_review_ms = ms_at(value, &["auto_review_ms", "autoReviewMs"]);
        let tool_wait_ms = ms_at(value, &["tool_wait_ms", "toolWaitMs"]);
        let tool_rounds = u32_at(value, &["tool_rounds", "toolRounds"]);
        let tools = parse_tools(value.get("tools"));
        let rounds = parse_rounds(value.get("rounds").or(model.filter(|m| m.is_array())));
        if total_ms.is_none()
            && model_ms.is_none()
            && auto_review_ms.is_none()
            && tool_wait_ms.is_none()
            && tools.is_empty()
            && rounds.is_empty()
        {
            return None;
        }
        Some(Self {
            v,
            total_ms,
            model_ms,
            auto_review_ms,
            tool_wait_ms,
            tool_rounds,
            rounds,
            tools,
        })
    }

    /// Canonical JSON for sqlite. Unknown wire aliases are not written back.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".into())
    }

    pub fn from_json(raw: &str) -> Option<Self> {
        serde_json::from_str::<Value>(raw)
            .ok()
            .as_ref()
            .and_then(Self::from_value)
    }

    /// Lines under the bubble when Settings → Show turn timing is on.
    ///
    /// A phase at zero is left out: the harness always sends `auto_review_ms`,
    /// and a `0ms` line reads as a step that ran.
    pub fn debug_lines(&self) -> Vec<String> {
        let spent = |ms: Option<u64>| ms.filter(|ms| *ms > 0);
        let mut lines = Vec::new();
        if let Some(ms) = self.total_ms {
            lines.push(format!("{} total", format_ms(ms)));
        }
        if self.rounds.len() > 1 {
            for (i, round) in self.rounds.iter().enumerate() {
                lines.push(format!("round {}  {}", i + 1, format_ms(round.model_ms)));
            }
        } else if let Some(ms) =
            spent(self.model_ms).or_else(|| spent(self.rounds.first().map(|r| r.model_ms)))
        {
            lines.push(format!("model  {}", format_ms(ms)));
        }
        for tool in &self.tools {
            let name = if tool.name.is_empty() {
                "tool"
            } else {
                tool.name.as_str()
            };
            lines.push(format!("{name}  {}", format_ms(tool.ms)));
        }
        if let Some(ms) = spent(self.tool_wait_ms) {
            let over = match self.tool_rounds {
                Some(n) if n > 1 => format!(" over {n} rounds"),
                _ => String::new(),
            };
            lines.push(format!("tool wait  {}{over}", format_ms(ms)));
        }
        if let Some(ms) = spent(self.auto_review_ms) {
            lines.push(format!("auto-review  {}", format_ms(ms)));
        }
        lines
    }
}

pub fn is_timing_name(name: &str) -> bool {
    name == RUN_TIMING_CUSTOM || name == TURN_TIMELINE_CUSTOM
}

fn is_timing_event(event: &Value) -> bool {
    let name = event.get("name").and_then(Value::as_str).unwrap_or("");
    if !is_timing_name(name) {
        return false;
    }
    match event.get("type").and_then(Value::as_str) {
        Some("CUSTOM") | Some("custom") | None => true,
        Some(_) => false,
    }
}

/// Compact wall clock: `12s`, `6m12s`, `1h2m`. Sub-second is milliseconds.
pub fn format_ms(ms: u64) -> String {
    format_duration(Duration::from_millis(ms))
}

pub fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 1000 {
        return format!("{ms}ms");
    }
    let secs = d.as_secs();
    if secs < 60 {
        format!("{secs}s")
    } else if secs < 3600 {
        let m = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{m}m")
        } else {
            format!("{m}m{s}s")
        }
    } else {
        let h = secs / 3600;
        let m = (secs % 3600) / 60;
        if m == 0 {
            format!("{h}h")
        } else {
            format!("{h}h{m}m")
        }
    }
}

/// How long a finished run took, for the peek stamp. Sub-second waits are
/// omitted so a fast reply still looks like a clock, not `400ms`.
pub fn stamp_duration(start: std::time::SystemTime, end: std::time::SystemTime) -> Option<String> {
    let d = end.duration_since(start).ok()?;
    (d.as_secs() >= 1).then(|| format_duration(d))
}

fn ms_at(value: &Value, keys: &[&str]) -> Option<u64> {
    keys.iter().find_map(|key| as_ms(value.get(*key)?))
}

fn u32_at(value: &Value, keys: &[&str]) -> Option<u32> {
    keys.iter().find_map(|key| {
        value
            .get(*key)
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
    })
}

/// Longer than any turn a person waits on. The number is the server's, and
/// past this it would run a finish clock beyond what a row or a clock label
/// can hold.
const LONGEST_TURN_MS: u64 = 30 * 24 * 60 * 60 * 1000;

fn as_ms(value: &Value) -> Option<u64> {
    let whole = |f: f64| (f.is_finite() && f >= 0.0).then(|| f.round() as u64);
    let ms = match value {
        Value::Number(n) => n.as_u64().or_else(|| n.as_f64().and_then(whole)),
        Value::String(s) => s.parse::<f64>().ok().and_then(whole),
        _ => None,
    }?;
    (ms <= LONGEST_TURN_MS).then_some(ms)
}

fn parse_tools(value: Option<&Value>) -> Vec<ToolTiming> {
    let Some(value) = value else {
        return Vec::new();
    };
    match value {
        Value::Array(items) => items.iter().filter_map(parse_tool).collect(),
        Value::Object(map) => map
            .iter()
            .filter_map(|(name, ms)| {
                Some(ToolTiming {
                    name: name.clone(),
                    ms: as_ms(ms)?,
                })
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn parse_tool(value: &Value) -> Option<ToolTiming> {
    let name = string_at(value, &["name", "tool", "toolCallName", "tool_name"]).unwrap_or_default();
    let ms = ms_at(value, &["ms", "duration_ms", "durationMs", "elapsed_ms"])?;
    Some(ToolTiming { name, ms })
}

fn parse_rounds(value: Option<&Value>) -> Vec<RoundTiming> {
    let Some(Value::Array(items)) = value else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| {
            let model_ms = as_ms(item).or_else(|| ms_at(item, &["model_ms", "modelMs", "ms"]))?;
            Some(RoundTiming { model_ms })
        })
        .collect()
}

fn string_at(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{Duration, SystemTime};

    fn custom(name: &str, value: Value) -> Value {
        json!({ "type": "CUSTOM", "name": name, "value": value })
    }

    #[test]
    fn the_documented_v1_shape_reads() {
        let event = custom(
            RUN_TIMING_CUSTOM,
            json!({
                "v": 1,
                "total_ms": 372123,
                "model_ms": 12000,
                "auto_review_ms": 8000,
                "rounds": [{ "model_ms": 4000 }, { "model_ms": 8000 }],
                "tools": [{ "name": "profile.list", "ms": 350000 }]
            }),
        );
        let timing = TurnTiming::from_event(&event).expect("v1");
        assert_eq!(timing.v, 1);
        assert_eq!(timing.total_ms, Some(372123));
        assert_eq!(timing.model_ms, Some(12000));
        assert_eq!(timing.auto_review_ms, Some(8000));
        assert_eq!(timing.tools[0].name, "profile.list");
        assert_eq!(timing.tools[0].ms, 350000);
        assert_eq!(timing.rounds.len(), 2);
        assert_eq!(
            timing.debug_lines(),
            vec![
                "6m12s total".to_string(),
                "round 1  4s".to_string(),
                "round 2  8s".to_string(),
                "profile.list  5m50s".to_string(),
                "auto-review  8s".to_string(),
            ]
        );
    }

    /// What `opengrok-harness` `TurnTiming::event` sends, as its
    /// `timing_event_is_custom_run_timing_with_compact_value` test builds it:
    /// `record_model(12)`, then one tool round of `shell` 3ms with 3ms of wait
    /// and no auto-review. No `v`, no `rounds`, `model_ms` per round.
    #[test]
    fn the_frame_the_harness_sends_reads() {
        let event = json!({
            "type": "CUSTOM",
            "timestamp": 7,
            "name": "run-timing",
            "threadId": "t1",
            "runId": "r1",
            "value": {
                "model_ms": [12],
                "tools": [{ "name": "shell", "ms": 3 }],
                "tool_wait_ms": 3,
                "auto_review_ms": 0,
                "total_ms": 15,
                "tool_rounds": 1
            },
            "total_ms": 15,
            "tool_rounds": 1
        });
        let timing = TurnTiming::from_event(&event).expect("the harness frame");
        assert_eq!(timing.total_ms, Some(15));
        assert_eq!(timing.rounds, vec![RoundTiming { model_ms: 12 }]);
        assert_eq!(timing.tool_wait_ms, Some(3));
        assert_eq!(timing.tool_rounds, Some(1));
        assert_eq!(
            timing.debug_lines(),
            vec![
                "15ms total".to_string(),
                "model  12ms".to_string(),
                "shell  3ms".to_string(),
                "tool wait  3ms".to_string(),
            ]
        );
    }

    #[test]
    fn two_model_rounds_from_the_harness_read_as_rounds() {
        let timing = TurnTiming::from_value(&json!({
            "model_ms": [12, 40],
            "tools": [],
            "tool_wait_ms": 0,
            "auto_review_ms": 0,
            "total_ms": 60,
            "tool_rounds": 1
        }))
        .expect("the harness value");
        assert_eq!(
            timing.debug_lines(),
            vec![
                "60ms total".to_string(),
                "round 1  12ms".to_string(),
                "round 2  40ms".to_string(),
            ]
        );
    }

    #[test]
    fn an_absurd_duration_is_no_duration() {
        for total in [json!("1e400"), json!(1e300), json!(u64::MAX)] {
            let timing = TurnTiming::from_value(&json!({ "total_ms": total, "tool_wait_ms": 3 }))
                .expect("the tool wait still reads");
            assert_eq!(timing.total_ms, None, "{total}");
        }
    }

    #[test]
    fn turn_timeline_and_camel_case_and_a_string_value_all_read() {
        let event = json!({
            "type": "CUSTOM",
            "name": TURN_TIMELINE_CUSTOM,
            "value": "{\"v\":1,\"totalMs\":1500,\"modelMs\":900,\"tools\":[{\"tool\":\"Shell\",\"durationMs\":600}]}"
        });
        let timing = TurnTiming::from_event(&event).expect("alias");
        assert_eq!(timing.total_ms, Some(1500));
        assert_eq!(timing.model_ms, Some(900));
        assert_eq!(timing.tools[0].name, "Shell");
        assert_eq!(timing.tools[0].ms, 600);
    }

    #[test]
    fn a_user_form_custom_is_not_timing() {
        let event = custom(
            "user-form",
            json!({ "v": 1, "total_ms": 12, "formRequest": { "title": "Login" } }),
        );
        assert!(TurnTiming::from_event(&event).is_none());
    }

    #[test]
    fn absence_and_an_empty_object_are_silence() {
        assert!(
            TurnTiming::from_event(&json!({"type":"CUSTOM","name":"run-timing","value":{}}))
                .is_none()
        );
        assert!(TurnTiming::from_events(&[]).is_none());
        assert!(TurnTiming::from_event(&json!({"type":"RUN_FINISHED"})).is_none());
    }

    #[test]
    fn later_frames_win_and_unknown_keys_are_ignored() {
        let events = vec![
            custom(RUN_TIMING_CUSTOM, json!({"v":1,"total_ms":10})),
            custom(
                RUN_TIMING_CUSTOM,
                json!({"v":1,"total_ms":99,"future_field": true}),
            ),
        ];
        assert_eq!(TurnTiming::from_events(&events).unwrap().total_ms, Some(99));
    }

    #[test]
    fn duration_wording() {
        assert_eq!(format_ms(400), "400ms");
        assert_eq!(format_ms(12_000), "12s");
        assert_eq!(format_ms(6 * 60 * 1000 + 12 * 1000), "6m12s");
        assert_eq!(format_ms(6 * 60 * 1000), "6m");
        assert_eq!(format_ms(3600 * 1000 + 2 * 60 * 1000), "1h2m");
    }

    #[test]
    fn the_peek_stamp_hides_a_subsecond_wait() {
        let start = SystemTime::UNIX_EPOCH;
        assert_eq!(
            stamp_duration(start, start + Duration::from_millis(400)),
            None
        );
        assert_eq!(
            stamp_duration(start, start + Duration::from_secs(12)),
            Some("12s".into())
        );
        assert_eq!(
            stamp_duration(start, start + Duration::from_secs(6 * 60 + 12)),
            Some("6m12s".into())
        );
    }

    #[test]
    fn json_round_trip_is_canonical_snake_case() {
        let timing = TurnTiming::from_value(&json!({
            "v": 1,
            "totalMs": 1500,
            "tools": { "profile.list": 900 }
        }))
        .unwrap();
        let raw = timing.to_json();
        assert!(raw.contains("total_ms"), "{raw}");
        assert!(!raw.contains("totalMs"), "{raw}");
        let back = TurnTiming::from_json(&raw).unwrap();
        assert_eq!(back.total_ms, Some(1500));
        assert_eq!(back.tools[0].name, "profile.list");
    }
}
