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
        "Shell" | "BoxShell" | "shellToolCall" | "ExternalShell" => {
            if command.is_some_and(|c| c.contains('>') || c.contains("tee ") || c.contains("sed "))
            {
                "Drafting the file".into()
            } else {
                "Running commands".into()
            }
        }
        super::gen_ui::USER_MACHINE_SHELL => command
            .map(|c| format!("On your machine: {c}"))
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
            activity_from_agui(&ev, Some(r#"{"command":"ls"}"#)),
            ActivityTick::Set(BotActivity {
                label: "Running commands".into()
            })
        );
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
