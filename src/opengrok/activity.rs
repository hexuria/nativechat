//! Live bot status, ported from OpenGrok `sand-activity.ts` + `agent-activity.ts`.
//! AG-UI events are the NativeChat input; the desktop paints the same facts from `/events`.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BotActivity {
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityTick {
    Set(BotActivity),
    Keep,
    Clear,
}

/// The footer says "{name} is working" from `bot_status`. That label is
/// per in-flight coworker — switching to another bot must not inherit it.
pub fn visible_bot_status(
    active_coworker_id: Option<&str>,
    responding_coworker_id: Option<&str>,
    bot_status: Option<&str>,
) -> Option<String> {
    if active_coworker_id.is_some() && active_coworker_id == responding_coworker_id {
        bot_status
            .filter(|label| !label.is_empty())
            .map(str::to_string)
    } else {
        None
    }
}

/// Per-call memory for the frames AG-UI splits: `TOOL_CALL_START` names the
/// tool but has no arguments, `TOOL_CALL_ARGS` has arguments but no name.
/// Fed every frame in order, each one labels correctly.
#[derive(Default)]
pub struct ToolCallTracker {
    names: std::collections::HashMap<String, String>,
    args: std::collections::HashMap<String, String>,
}

impl ToolCallTracker {
    /// The status one frame implies, given the frames before it.
    pub fn tick(&mut self, event: &Value) -> ActivityTick {
        let call_id = event.get("toolCallId").and_then(Value::as_str);
        if let Some(id) = call_id {
            if let Some(name) = event.get("toolCallName").and_then(Value::as_str) {
                self.names.insert(id.to_string(), name.to_string());
            }
            if let Some(arguments) = event.get("arguments") {
                self.args.insert(id.to_string(), arguments.to_string());
            } else if let Some(delta) = event.get("delta").and_then(Value::as_str) {
                self.args.entry(id.to_string()).or_default().push_str(delta);
            }
        }
        let args = call_id.and_then(|id| self.args.get(id)).map(String::as_str);
        let remembered = if event.get("toolCallName").is_none() {
            call_id.and_then(|id| self.names.get(id))
        } else {
            None
        };
        match remembered {
            Some(name) => {
                let mut named = event.clone();
                if let Some(object) = named.as_object_mut() {
                    object.insert("toolCallName".into(), name.clone().into());
                }
                activity_from_agui(&named, args)
            }
            None => activity_from_agui(event, args),
        }
    }
}

/// Where a journaled run is right now: the last frame that set a status.
pub fn activity_from_replay(events: &[Value]) -> Option<BotActivity> {
    let mut tracker = ToolCallTracker::default();
    let mut current = None;
    for event in events {
        match tracker.tick(event) {
            ActivityTick::Keep => {}
            ActivityTick::Clear => current = None,
            ActivityTick::Set(activity) => current = Some(activity),
        }
    }
    current
}

pub fn activity_from_agui(event: &Value, tool_args: Option<&str>) -> ActivityTick {
    let Some(kind) = event.get("type").and_then(Value::as_str) else {
        return ActivityTick::Keep;
    };
    match kind {
        "RUN_STARTED"
        | "REASONING_START"
        | "REASONING_MESSAGE_START"
        | "REASONING_MESSAGE_CONTENT"
        | "REASONING_MESSAGE_CHUNK" => ActivityTick::Set(BotActivity {
            label: "Thinking".into(),
        }),
        "TEXT_MESSAGE_START" | "TEXT_MESSAGE_CONTENT" | "TEXT_MESSAGE_CHUNK" => {
            ActivityTick::Set(BotActivity {
                label: "Writing".into(),
            })
        }
        "TOOL_CALL_START" | "TOOL_CALL_ARGS" => {
            let name = event
                .get("toolCallName")
                .and_then(Value::as_str)
                .unwrap_or("");
            ActivityTick::Set(BotActivity {
                label: describe_tool(name, tool_args),
            })
        }
        "TOOL_CALL_END" => ActivityTick::Set(BotActivity {
            label: "Thinking".into(),
        }),
        "RUN_FINISHED" | "RUN_ERROR" => ActivityTick::Clear,
        "CUSTOM" => {
            if event.get("name").and_then(Value::as_str) == Some("run-awaiting-approval") {
                ActivityTick::Set(BotActivity {
                    label: "Waiting for approval".into(),
                })
            } else {
                ActivityTick::Keep
            }
        }
        _ => ActivityTick::Keep,
    }
}

fn file_basename(path: &str) -> Option<&str> {
    path.split(['/', '\\'])
        .filter(|s| !s.is_empty())
        .next_back()
}

/// First line of the command, cut so the status line stays one line.
fn short_command(command: &str) -> String {
    const MAX: usize = 48;
    let line = command.lines().next().unwrap_or("").trim();
    let mut out: String = line.chars().take(MAX).collect();
    if line.chars().count() > MAX || command.lines().nth(1).is_some() {
        out.push('…');
    }
    out
}

fn describe_tool(name: &str, args: Option<&str>) -> String {
    let parsed = args.and_then(|raw| serde_json::from_str::<Value>(raw).ok());
    let path = parsed
        .as_ref()
        .and_then(|v| v.get("path").and_then(Value::as_str));
    let command = parsed
        .as_ref()
        .and_then(|v| v.get("command").and_then(Value::as_str));
    match name {
        "Read" | "ExternalRead" | "BoxRead" | "readToolCall" => path
            .and_then(file_basename)
            .map(|n| format!("Reading {n}"))
            .unwrap_or_else(|| "Reading file".into()),
        "WebSearch" | "webSearchToolCall" => "Searching the web".into(),
        "WebFetch" | "webFetchToolCall" => "Reading the web".into(),
        "GenerateImage" | "generateImageToolCall" => "Generating a photo".into(),
        "Shell" | "BoxShell" | "shellToolCall" | "ExternalShell" => match command {
            Some(c) if c.contains('>') || c.contains("tee ") || c.contains("sed ") => {
                "Drafting the file".into()
            }
            Some(c) => format!("Running `{}`", short_command(c)),
            None => "Running commands".into(),
        },
        super::gen_ui::USER_MACHINE_SHELL => command
            .map(|c| format!("On your machine: {}", short_command(c)))
            .unwrap_or_else(|| "On your machine".into()),
        "Computer" | "Screenshot" | "computerUseToolCall" => "On its computer".into(),
        "" => "Working".into(),
        other => format!("Using {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn thinking_and_writing() {
        let ev = json!({"type":"REASONING_MESSAGE_CONTENT","delta":"hmm"});
        assert_eq!(
            activity_from_agui(&ev, None),
            ActivityTick::Set(BotActivity {
                label: "Thinking".into()
            })
        );
        let ev = json!({"type":"TEXT_MESSAGE_CONTENT","delta":"Hi"});
        assert_eq!(
            activity_from_agui(&ev, None),
            ActivityTick::Set(BotActivity {
                label: "Writing".into()
            })
        );
        let ev = json!({"type":"RUN_FINISHED"});
        assert_eq!(activity_from_agui(&ev, None), ActivityTick::Clear);
    }

    #[test]
    fn read_and_shell_labels() {
        let ev = json!({"type":"TOOL_CALL_START","toolCallName":"Read"});
        assert_eq!(
            activity_from_agui(&ev, Some(r#"{"path":"/tmp/foo.rs"}"#)),
            ActivityTick::Set(BotActivity {
                label: "Reading foo.rs".into()
            })
        );
        let ev = json!({"type":"TOOL_CALL_START","toolCallName":"Shell"});
        assert_eq!(
            activity_from_agui(&ev, Some(r#"{"command":"cat > notes.md"}"#)),
            ActivityTick::Set(BotActivity {
                label: "Drafting the file".into()
            })
        );
        assert_eq!(
            activity_from_agui(&ev, Some(r#"{"command":"cargo test"}"#)),
            ActivityTick::Set(BotActivity {
                label: "Running `cargo test`".into()
            })
        );
        assert_eq!(
            activity_from_agui(&ev, None),
            ActivityTick::Set(BotActivity {
                label: "Running commands".into()
            })
        );
    }

    #[test]
    fn args_frame_is_labelled_by_the_name_from_its_start_frame() {
        let mut tracker = ToolCallTracker::default();
        let start = json!({"type":"TOOL_CALL_START","toolCallId":"c1","toolCallName":"Shell"});
        assert_eq!(
            tracker.tick(&start),
            ActivityTick::Set(BotActivity {
                label: "Running commands".into()
            })
        );
        let args = json!({"type":"TOOL_CALL_ARGS","toolCallId":"c1","delta":"{\"command\":\"cargo test\"}"});
        assert_eq!(
            tracker.tick(&args),
            ActivityTick::Set(BotActivity {
                label: "Running `cargo test`".into()
            })
        );
    }

    #[test]
    fn replay_status_is_the_last_frame_that_set_one() {
        let events = vec![
            json!({"type":"RUN_STARTED"}),
            json!({"type":"TOOL_CALL_START","toolCallId":"c1","toolCallName":"Shell"}),
            json!({"type":"TOOL_CALL_ARGS","toolCallId":"c1","arguments":{"command":"date"}}),
        ];
        assert_eq!(
            activity_from_replay(&events),
            Some(BotActivity {
                label: "Running `date`".into()
            })
        );
        let mut finished = events.clone();
        finished.push(json!({"type":"RUN_FINISHED"}));
        assert_eq!(activity_from_replay(&finished), None);
    }

    #[test]
    fn short_command_keeps_the_status_line_to_one_line() {
        assert_eq!(short_command("ls -la"), "ls -la");
        assert_eq!(short_command("  echo hi\nls"), "echo hi…");
        let long = "x".repeat(60);
        let cut = short_command(&long);
        assert_eq!(cut.chars().count(), 49);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn working_status_stays_on_the_bot_that_is_running() {
        assert_eq!(
            visible_bot_status(Some("cw_example"), Some("cw_example"), Some("Thinking")),
            Some("Thinking".into())
        );
        assert_eq!(
            visible_bot_status(Some("cw_new"), Some("cw_example"), Some("Thinking")),
            None
        );
        assert_eq!(
            visible_bot_status(Some("cw_new"), None, Some("Thinking")),
            None
        );
    }

    #[test]
    fn awaiting_approval_is_a_status() {
        let ev = json!({"type":"CUSTOM","name":"run-awaiting-approval"});
        assert_eq!(
            activity_from_agui(&ev, None),
            ActivityTick::Set(BotActivity {
                label: "Waiting for approval".into()
            })
        );
    }
}
