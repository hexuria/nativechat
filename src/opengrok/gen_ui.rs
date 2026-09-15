//! Generative UI on an AG-UI stream.
//!
//! Text (`TEXT_MESSAGE_*`) paints as it arrives. A bar chart or form is a
//! *different* event (tool call, CUSTOM, or a complete `{"ui":...}` object)
//! and mounts only when its JSON is whole. Incomplete JSON is held, not
//! rendered as markdown, and does not stall the prose around it.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum ChatPart {
    Text(String),
    Ui(UiSpec),
}

#[derive(Debug, Clone, PartialEq)]
pub enum UiSpec {
    BarChart(BarChartSpec),
    Form(FormSpec),
}

#[derive(Debug, Clone, PartialEq)]
pub struct BarChartSpec {
    pub title: Option<String>,
    pub bars: Vec<BarItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BarItem {
    pub label: String,
    pub value: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormSpec {
    pub title: Option<String>,
    pub prompt: Option<String>,
    pub fields: Vec<FormField>,
    pub submit: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FormField {
    pub id: String,
    pub label: String,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedUiTool {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Default)]
pub struct TurnAssembler {
    committed: Vec<ChatPart>,
    text: String,
    tool: Option<OpenTool>,
    waiting_approval: bool,
    completed_ui: Vec<CompletedUiTool>,
}

#[derive(Debug)]
struct OpenTool {
    id: String,
    name: String,
    args: String,
}

impl TurnAssembler {
    pub fn push_event(&mut self, event: &Value) {
        let kind = event.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "TEXT_MESSAGE_CONTENT" | "TEXT_MESSAGE_CHUNK" => {
                if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                    self.text.push_str(delta);
                    if self.tool.is_none() {
                        self.flush_text();
                    }
                }
            }
            "TOOL_CALL_START" => {
                let name = event
                    .get("toolCallName")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                if is_ui_tool(name) {
                    self.flush_text();
                    let id = event
                        .get("toolCallId")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    self.tool = Some(OpenTool {
                        id,
                        name: name.to_string(),
                        args: String::new(),
                    });
                }
            }
            "TOOL_CALL_ARGS" => {
                if let Some(tool) = self.tool.as_mut() {
                    if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                        tool.args.push_str(delta);
                    }
                }
            }
            "TOOL_CALL_END" => {
                self.flush_text();
                if let Some(tool) = self.tool.take() {
                    if let Ok(value) = serde_json::from_str::<Value>(&tool.args) {
                        if let Some(spec) = UiSpec::from_tool(&tool.name, &value) {
                            self.push_ui(spec, tool.id, tool.name);
                        }
                    }
                }
            }
            "CUSTOM" => {
                let name = event.get("name").and_then(Value::as_str).unwrap_or("");
                if name == "run-awaiting-approval" {
                    self.flush_text();
                    let tool = event
                        .get("tool")
                        .and_then(Value::as_str)
                        .unwrap_or("a tool");
                    let why = event.get("why").and_then(Value::as_str).unwrap_or("");
                    let line = if why.is_empty() {
                        format!("Waiting for approval to run {tool}.")
                    } else {
                        format!("Waiting for approval to run {tool}: {why}")
                    };
                    self.committed.push(ChatPart::Text(format!("\n\n{line}")));
                    self.waiting_approval = true;
                } else {
                    let value = event.get("value").cloned().unwrap_or(Value::Null);
                    if let Some(spec) = UiSpec::from_custom(name, &value) {
                        self.flush_text();
                        self.committed.push(ChatPart::Ui(spec));
                    }
                }
            }
            _ => {}
        }
    }

    pub fn snapshot(&self) -> (String, Vec<ChatPart>) {
        if self.holding_ui() {
            let plain = plain_text(&self.committed);
            return (plain, self.committed.clone());
        }
        let mut parts = self.committed.clone();
        if !self.text.is_empty() {
            parts.push(ChatPart::Text(self.text.clone()));
        }
        let plain = plain_text(&parts);
        (plain, parts)
    }

    pub fn waiting_approval(&self) -> bool {
        self.waiting_approval
    }

    /// Stream ended. Mount a UI tool if its args already parse; otherwise release held text.
    pub fn finish(&mut self) {
        self.flush_text();
        if let Some(tool) = self.tool.take() {
            if let Ok(value) = serde_json::from_str::<Value>(&tool.args) {
                if let Some(spec) = UiSpec::from_tool(&tool.name, &value) {
                    self.push_ui(spec, tool.id, tool.name);
                }
            }
        }
    }

    pub fn take_completed_ui_tools(&mut self) -> Vec<CompletedUiTool> {
        std::mem::take(&mut self.completed_ui)
    }

    fn push_ui(&mut self, spec: UiSpec, id: String, name: String) {
        let chart = matches!(spec, UiSpec::BarChart(_));
        self.committed.retain(|part| match part {
            ChatPart::Ui(UiSpec::BarChart(_)) => !chart,
            ChatPart::Ui(UiSpec::Form(_)) => chart,
            ChatPart::Text(_) => true,
        });
        self.committed.push(ChatPart::Ui(spec));
        self.completed_ui.retain(|tool| tool.name != name);
        self.completed_ui.push(CompletedUiTool { id, name });
    }

    fn holding_ui(&self) -> bool {
        self.tool.is_some() || ui_object_incomplete(&self.text)
    }

    fn flush_text(&mut self) {
        drain_complete_ui(&mut self.text, &mut self.committed);
    }
}

impl UiSpec {
    pub fn from_value(value: &Value) -> Option<Self> {
        let kind = ui_kind(value)?;
        match kind.as_str() {
            "bar-chart" | "barchart" => Some(Self::BarChart(BarChartSpec::from_value(value)?)),
            "form" => Some(Self::Form(FormSpec::from_value(value)?)),
            _ => None,
        }
    }

    fn from_tool(name: &str, value: &Value) -> Option<Self> {
        if let Some(spec) = Self::from_value(value) {
            return Some(spec);
        }
        match normalize_name(name).as_str() {
            "bar-chart" | "barchart" => Some(Self::BarChart(BarChartSpec::from_value(value)?)),
            "form" => Some(Self::Form(FormSpec::from_value(value)?)),
            _ => None,
        }
    }

    fn from_custom(name: &str, value: &Value) -> Option<Self> {
        if normalize_name(name) == "ui" || name.is_empty() {
            return Self::from_value(value);
        }
        Self::from_tool(name, value)
    }
}

impl BarChartSpec {
    fn from_value(value: &Value) -> Option<Self> {
        let data = value.get("data").unwrap_or(value);
        let rows = data
            .get("bars")
            .or_else(|| data.get("values"))
            .or_else(|| value.get("bars"))?;
        let bars = parse_bars(rows);
        if bars.is_empty() {
            return None;
        }
        Some(Self {
            title: string_field(data, "title").or_else(|| string_field(value, "title")),
            bars,
        })
    }
}

impl FormSpec {
    fn from_value(value: &Value) -> Option<Self> {
        let data = value.get("data").unwrap_or(value);
        let fields = data
            .get("fields")
            .or_else(|| value.get("fields"))
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .enumerate()
                    .filter_map(|(i, row)| parse_field(row, i))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if fields.is_empty() {
            return None;
        }
        Some(Self {
            title: string_field(data, "title").or_else(|| string_field(value, "title")),
            prompt: string_field(data, "prompt").or_else(|| string_field(value, "prompt")),
            fields,
            submit: string_field(data, "submit")
                .or_else(|| string_field(value, "submit"))
                .unwrap_or_else(|| "Send".to_string()),
        })
    }
}

fn parse_field(row: &Value, index: usize) -> Option<FormField> {
    let label = string_field(row, "label").or_else(|| string_field(row, "name"))?;
    let id = string_field(row, "id").unwrap_or_else(|| format!("field-{index}"));
    let options = row
        .get("options")
        .and_then(Value::as_array)
        .map(|opts| {
            opts.iter()
                .filter_map(|opt| {
                    opt.as_str()
                        .map(str::to_string)
                        .or_else(|| string_field(opt, "label"))
                        .or_else(|| string_field(opt, "value"))
                })
                .collect()
        })
        .unwrap_or_default();
    Some(FormField { id, label, options })
}

fn parse_bars(rows: &Value) -> Vec<BarItem> {
    let Some(rows) = rows.as_array() else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            if let Some(arr) = row.as_array() {
                let label = arr.first().and_then(Value::as_str)?.to_string();
                let value = arr.get(1).and_then(json_f32)?;
                return Some(BarItem { label, value });
            }
            let label = string_field(row, "label")
                .or_else(|| string_field(row, "name"))
                .unwrap_or_default();
            let value = row
                .get("value")
                .or_else(|| row.get("y"))
                .and_then(json_f32)?;
            if label.is_empty() {
                return None;
            }
            Some(BarItem { label, value })
        })
        .collect()
}

fn json_f32(value: &Value) -> Option<f32> {
    value
        .as_f64()
        .map(|n| n as f32)
        .or_else(|| value.as_i64().map(|n| n as f32))
        .or_else(|| value.as_u64().map(|n| n as f32))
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}

fn ui_kind(value: &Value) -> Option<String> {
    if let Some(kind) = value
        .get("ui")
        .or_else(|| value.get("component"))
        .and_then(Value::as_str)
    {
        return Some(normalize_name(kind));
    }
    let ty = value.get("type").and_then(Value::as_str)?;
    if ty.eq_ignore_ascii_case("ui") {
        return value
            .get("name")
            .and_then(Value::as_str)
            .map(normalize_name);
    }
    Some(normalize_name(ty))
}

fn is_ui_tool(name: &str) -> bool {
    matches!(
        normalize_name(name).as_str(),
        "bar-chart"
            | "barchart"
            | "show-bar-chart"
            | "render-bar-chart"
            | "form"
            | "show-form"
            | "render-form"
    )
}

fn normalize_name(name: &str) -> String {
    name.trim()
        .to_ascii_lowercase()
        .replace('_', "-")
        .replace(' ', "-")
}

fn drain_complete_ui(buf: &mut String, out: &mut Vec<ChatPart>) {
    loop {
        let Some(start) = find_ui_object(buf) else {
            if !buf.is_empty() {
                out.push(ChatPart::Text(std::mem::take(buf)));
            }
            return;
        };
        let Some(len) = complete_json_len(&buf[start..]) else {
            return;
        };
        if start > 0 {
            out.push(ChatPart::Text(buf[..start].to_string()));
        }
        let end = start + len;
        let raw = buf[start..end].to_string();
        buf.replace_range(..end, "");
        match serde_json::from_str::<Value>(&raw)
            .ok()
            .and_then(|v| UiSpec::from_value(&v))
        {
            Some(spec) => out.push(ChatPart::Ui(spec)),
            None => out.push(ChatPart::Text(raw)),
        }
    }
}

fn ui_object_incomplete(buf: &str) -> bool {
    match find_ui_object(buf) {
        None => false,
        Some(start) => complete_json_len(&buf[start..]).is_none(),
    }
}

fn find_ui_object(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let rest = s[i..].trim_start();
            if rest.starts_with("{\"ui\"")
                || rest.starts_with("{ \"ui\"")
                || rest.starts_with("{\"component\"")
                || rest.starts_with("{ \"component\"")
            {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn complete_json_len(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'{') {
        return None;
    }
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escape = false;
    for (i, &b) in bytes.iter().enumerate() {
        if in_str {
            if escape {
                escape = false;
                continue;
            }
            if b == b'\\' {
                escape = true;
                continue;
            }
            if b == b'"' {
                in_str = false;
            }
            continue;
        }
        match b {
            b'"' => in_str = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i + 1);
                }
            }
            _ => {}
        }
    }
    None
}

fn plain_text(parts: &[ChatPart]) -> String {
    parts
        .iter()
        .filter_map(|part| match part {
            ChatPart::Text(text) => Some(text.as_str()),
            ChatPart::Ui(_) => None,
        })
        .collect()
}

/// What the next `/ag-ui` run is told after NativeChat paints a frontend tool.
pub const UI_TOOL_RESULT: &str = "It is now on screen for the person.";

pub const MAX_TURN_CONTINUES: usize = 8;

pub fn agui_tools() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "name": "bar_chart",
            "description": "Render an interactive bar chart in the chat instead of markdown or ASCII.",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "bars": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "label": { "type": "string" },
                                "value": { "type": "number" }
                            },
                            "required": ["label", "value"]
                        }
                    }
                },
                "required": ["bars"]
            }
        }),
        serde_json::json!({
            "name": "form",
            "description": "Render an interactive choice form in the chat instead of a bullet list.",
            "parameters": {
                "type": "object",
                "properties": {
                    "title": { "type": "string" },
                    "prompt": { "type": "string" },
                    "submit": { "type": "string" },
                    "fields": {
                        "type": "array",
                        "items": {
                            "type": "object",
                            "properties": {
                                "id": { "type": "string" },
                                "label": { "type": "string" },
                                "options": { "type": "array", "items": { "type": "string" } }
                            },
                            "required": ["label", "options"]
                        }
                    }
                },
                "required": ["fields"]
            }
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn text(delta: &str) -> Value {
        json!({"type":"TEXT_MESSAGE_CONTENT","delta":delta})
    }

    #[test]
    fn text_deltas_stream_before_the_run_ends() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&text("Hello "));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "Hello ");
        assert_eq!(parts, vec![ChatPart::Text("Hello ".into())]);
    }

    #[test]
    fn incomplete_ui_json_is_held_not_painted() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&text("See this {\"ui\":\"bar-chart\",\"bars\":["));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "");
        assert!(parts.is_empty());
    }

    #[test]
    fn a_ui_tool_in_flight_does_not_stream() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type":"TOOL_CALL_START",
            "toolCallId":"c1",
            "toolCallName":"bar_chart"
        }));
        turn.push_event(&text("should not appear yet"));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "");
        assert!(parts.is_empty());
        turn.push_event(&json!({
            "type":"TOOL_CALL_ARGS",
            "toolCallId":"c1",
            "delta":"{\"title\":\"Q3\",\"bars\":[{\"label\":\"A\",\"value\":3}]}"
        }));
        turn.push_event(&json!({"type":"TOOL_CALL_END","toolCallId":"c1"}));
        let (_, parts) = turn.snapshot();
        assert!(matches!(parts.first(), Some(ChatPart::Text(t)) if t == "should not appear yet"));
        assert!(matches!(parts.get(1), Some(ChatPart::Ui(UiSpec::BarChart(_)))));
        let done = turn.take_completed_ui_tools();
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].id, "c1");
    }

    #[test]
    fn a_complete_bar_chart_object_mounts_without_waiting_for_later_text() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&text(
            "See this {\"ui\":\"bar-chart\",\"title\":\"Q3\",\"bars\":[{\"label\":\"A\",\"value\":10}]} and more",
        ));
        let (plain, parts) = turn.snapshot();
        assert_eq!(plain, "See this  and more");
        assert!(matches!(parts[0], ChatPart::Text(ref t) if t == "See this "));
        assert!(matches!(parts[1], ChatPart::Ui(UiSpec::BarChart(_))));
        assert!(matches!(parts[2], ChatPart::Text(ref t) if t == " and more"));
    }

    #[test]
    fn a_tool_call_chart_mounts_when_args_are_complete() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&text("Here:"));
        turn.push_event(&json!({
            "type":"TOOL_CALL_START",
            "toolCallId":"c1",
            "toolCallName":"bar_chart"
        }));
        turn.push_event(&json!({
            "type":"TOOL_CALL_ARGS",
            "toolCallId":"c1",
            "delta":"{\"title\":\"Q3\",\"bars\":[{\"label\":\"A\",\"value\":3}]}"
        }));
        let (_, mid) = turn.snapshot();
        assert!(mid.iter().all(|p| !matches!(p, ChatPart::Ui(_))));
        turn.push_event(&json!({"type":"TOOL_CALL_END","toolCallId":"c1"}));
        let (_, parts) = turn.snapshot();
        assert!(matches!(
            parts.last(),
            Some(ChatPart::Ui(UiSpec::BarChart(_)))
        ));
    }

    #[test]
    fn a_custom_form_event_mounts_as_a_widget() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&json!({
            "type":"CUSTOM",
            "name":"ui",
            "value":{
                "component":"form",
                "title":"Next",
                "fields":[{"id":"go","label":"Go","options":["Yes","No"]}],
                "submit":"Send"
            }
        }));
        let (_, parts) = turn.snapshot();
        assert!(matches!(parts.as_slice(), [ChatPart::Ui(UiSpec::Form(_))]));
    }

    #[test]
    fn curly_braces_in_prose_are_not_held() {
        let mut turn = TurnAssembler::default();
        turn.push_event(&text("The set {a, b} is finite."));
        let (plain, _) = turn.snapshot();
        assert_eq!(plain, "The set {a, b} is finite.");
    }
}
