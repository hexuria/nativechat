//! Live bot status, ported from OpenGrok `sand-activity.ts` + `agent-activity.ts`.
//! AG-UI events are the NativeChat input; the desktop paints the same facts from `/events`.

use serde_json::Value;

/// The label while a turn waits for the coworker's box to wake. The server sends a CUSTOM
/// `box-waking` frame right before the first box-bound tool of a turn starts a sleeping box, and
/// the footer turns it into "Waking {name}'s computer" (`bot_status_line`).
pub const WAKING_COMPUTER: &str = "Waking the computer";

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

/// Per-call memory for the frames AG-UI splits: `TOOL_CALL_START` names the
/// tool but has no arguments, `TOOL_CALL_ARGS` has arguments but no name.
/// Fed every frame in order, each one labels correctly.
#[derive(Default)]
pub struct ToolCallTracker {
    names: std::collections::HashMap<String, String>,
    args: std::collections::HashMap<String, String>,
    /// Call ids as they first appeared: the maps say what each call was, this says in what order,
    /// which is what `deeds` needs to tell a turn's story.
    order: Vec<String>,
}

impl ToolCallTracker {
    /// The status one frame implies, given the frames before it.
    pub fn tick(&mut self, event: &Value) -> ActivityTick {
        let call_id = event.get("toolCallId").and_then(Value::as_str);
        if let Some(id) = call_id {
            if !self.order.iter().any(|seen| seen == id) {
                self.order.push(id.to_string());
            }
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

    /// What this turn did, each tool said once and in the order it was called.
    pub fn deeds(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for id in &self.order {
            let Some(name) = self.names.get(id) else {
                continue;
            };
            let Some(deed) = deed_from_tool(name, self.args.get(id).map(String::as_str)) else {
                continue;
            };
            if !out.contains(&deed) {
                out.push(deed);
            }
        }
        out
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

/// What a journaled run did, for the turns rebuilt from the journal rather than watched live —
/// a run resumed after a permission card comes back this way.
pub fn deeds_from_replay(events: &[Value]) -> Vec<String> {
    let mut tracker = ToolCallTracker::default();
    for event in events {
        tracker.tick(event);
    }
    tracker.deeds()
}

/// How many deeds the stand-in names. A computer-use turn calls twenty tools; the transcript
/// wants a sentence, not a log.
const STANDIN_DEEDS: usize = 3;

/// The line a turn that acted but said nothing leaves in the transcript, e.g.
/// "[took a screenshot of my screen]". The coworker is shown its own transcript on every later
/// turn, so a wordless turn must still leave a memory of what it did.
pub fn tool_standin(deeds: &[String]) -> Option<String> {
    if deeds.is_empty() {
        return None;
    }
    let mut line = deeds
        .iter()
        .take(STANDIN_DEEDS)
        .cloned()
        .collect::<Vec<_>>()
        .join(", then ");
    if deeds.len() > STANDIN_DEEDS {
        line.push('…');
    }
    Some(format!("[{line}]"))
}

/// One tool call in the coworker's own voice, past tense, for the stand-in line. `describe_tool`
/// says what a call is doing now for the status strip; this says what it did, afterwards.
fn deed_from_tool(name: &str, args: Option<&str>) -> Option<String> {
    let parsed = args.and_then(|raw| serde_json::from_str::<Value>(raw).ok());
    let field = |key: &str| {
        parsed
            .as_ref()
            .and_then(|v| v.get(key).and_then(Value::as_str))
            .map(str::to_string)
    };
    let deed = match name {
        "" => return None,
        "Read" | "ExternalRead" | "BoxRead" | "readToolCall" | "read_file" => field("path")
            .as_deref()
            .and_then(file_basename)
            .map(|file| format!("read {file}"))
            .unwrap_or_else(|| "read a file".into()),
        "write_file" => field("path")
            .as_deref()
            .and_then(file_basename)
            .map(|file| format!("wrote {file}"))
            .unwrap_or_else(|| "wrote a file".into()),
        "WebSearch" | "webSearchToolCall" => "searched the web".into(),
        "WebFetch" | "webFetchToolCall" => "read a page on the web".into(),
        "GenerateImage" | "generateImageToolCall" => "made a picture".into(),
        "Shell" | "BoxShell" | "shellToolCall" | "ExternalShell" | "shell" => {
            "ran a shell command".into()
        }
        super::gen_ui::USER_MACHINE_SHELL => "ran a command on your computer".into(),
        "run_recipe" => "ran a recipe".into(),
        "Computer" | "Screenshot" | "computerUseToolCall" => {
            "took a screenshot of my screen".into()
        }
        "computer" => match field("action").as_deref() {
            Some("type") => "typed on my screen".into(),
            Some("key") => "pressed a key on my screen".into(),
            Some("scroll") => "scrolled my screen".into(),
            Some("click" | "double_click" | "right_click" | "drag" | "move") => {
                "clicked on my screen".into()
            }
            _ => "took a screenshot of my screen".into(),
        },
        "open_url" => field("url")
            .map(|url| {
                let rest = url.split("://").nth(1).unwrap_or(&url).to_string();
                let host = rest.split('/').next().unwrap_or(&rest);
                format!("opened {host}")
            })
            .unwrap_or_else(|| "opened a page on my screen".into()),
        "request_user_form" | "request-user-form" | "user-form" => {
            "asked you to fill a form".into()
        }
        "form" | "show_form" | "show-form" | "render_form" | "render-form" => {
            "showed you a form".into()
        }
        "bar_chart" | "bar-chart" | "barchart" | "show_bar_chart" | "show-bar-chart"
        | "render_bar_chart" | "render-bar-chart" => "drew you a chart".into(),
        other => format!("used {other}"),
    };
    Some(deed)
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
            if super::user_form::is_user_form_awaiting(event) {
                ActivityTick::Set(BotActivity {
                    label: super::user_form::WAITING_FOR_YOU.into(),
                })
            } else if event.get("name").and_then(Value::as_str) == Some("run-awaiting-approval") {
                ActivityTick::Set(BotActivity {
                    label: "Waiting for approval".into(),
                })
            } else if event.get("name").and_then(Value::as_str) == Some("box-waking") {
                ActivityTick::Set(BotActivity {
                    label: WAKING_COMPUTER.into(),
                })
            } else if super::user_form::is_user_form_event(
                event.get("name").and_then(Value::as_str).unwrap_or(""),
                event.get("value").unwrap_or(event),
            ) {
                let unresolved = super::user_form::UserFormSpec::from_custom_event(event)
                    .map(|spec| spec.is_unresolved())
                    .unwrap_or(true);
                if unresolved {
                    ActivityTick::Set(BotActivity {
                        label: super::user_form::WAITING_FOR_YOU.into(),
                    })
                } else {
                    // Settled: do not leave "Waiting for you" over the compact pill.
                    ActivityTick::Clear
                }
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
        "computer" => match parsed
            .as_ref()
            .and_then(|v| v.get("action").and_then(Value::as_str))
        {
            Some("screenshot") => "Looking at its screen".into(),
            Some("type") => "Typing on its computer".into(),
            Some("key") => "Pressing a key on its computer".into(),
            Some("scroll") => "Scrolling on its computer".into(),
            Some("click" | "double_click" | "right_click" | "drag" | "move") => {
                "Clicking on its computer".into()
            }
            _ => "On its computer".into(),
        },
        "open_url" => parsed
            .as_ref()
            .and_then(|v| v.get("url").and_then(Value::as_str))
            .and_then(|url| url.split("://").nth(1).or(Some(url)))
            .map(|rest| format!("Opening {}", rest.split('/').next().unwrap_or(rest)))
            .unwrap_or_else(|| "Opening a page on its computer".into()),
        "request_user_form" | "request-user-form" | "user-form" => {
            super::user_form::WAITING_FOR_YOU.into()
        }
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
    fn screen_tools_say_what_the_bot_is_doing_on_its_computer() {
        let ev = json!({"type":"TOOL_CALL_START","toolCallName":"computer"});
        assert_eq!(
            activity_from_agui(&ev, Some(r#"{"action":"screenshot"}"#)),
            ActivityTick::Set(BotActivity {
                label: "Looking at its screen".into()
            })
        );
        assert_eq!(
            activity_from_agui(&ev, Some(r#"{"action":"click","coordinate":[1,2]}"#)),
            ActivityTick::Set(BotActivity {
                label: "Clicking on its computer".into()
            })
        );
        let ev = json!({"type":"TOOL_CALL_START","toolCallName":"open_url"});
        assert_eq!(
            activity_from_agui(&ev, Some(r#"{"url":"https://facebook.com/login"}"#)),
            ActivityTick::Set(BotActivity {
                label: "Opening facebook.com".into()
            })
        );
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
    fn a_turn_that_only_looked_at_its_screen_leaves_that_behind() {
        let mut tracker = ToolCallTracker::default();
        tracker
            .tick(&json!({"type":"TOOL_CALL_START","toolCallId":"c1","toolCallName":"computer"}));
        tracker.tick(
            &json!({"type":"TOOL_CALL_ARGS","toolCallId":"c1","arguments":{"action":"screenshot"}}),
        );
        assert_eq!(
            tool_standin(&tracker.deeds()),
            Some("[took a screenshot of my screen]".into())
        );
    }

    #[test]
    fn a_turn_that_did_nothing_has_nothing_to_stand_in_for() {
        assert_eq!(tool_standin(&[]), None);
    }

    /// The stand-in tells the turn's story: every tool it called, in order, each said once, so a
    /// screen the bot looked at twenty times is still one clause.
    #[test]
    fn deeds_name_each_tool_once_and_in_the_order_it_was_called() {
        let events = vec![
            json!({"type":"TOOL_CALL_START","toolCallId":"c1","toolCallName":"open_url"}),
            json!({"type":"TOOL_CALL_ARGS","toolCallId":"c1","arguments":{"url":"https://example.com/login"}}),
            json!({"type":"TOOL_CALL_START","toolCallId":"c2","toolCallName":"computer"}),
            json!({"type":"TOOL_CALL_ARGS","toolCallId":"c2","arguments":{"action":"screenshot"}}),
            json!({"type":"TOOL_CALL_START","toolCallId":"c3","toolCallName":"computer"}),
            json!({"type":"TOOL_CALL_ARGS","toolCallId":"c3","arguments":{"action":"screenshot"}}),
        ];
        assert_eq!(
            tool_standin(&deeds_from_replay(&events)),
            Some("[opened example.com, then took a screenshot of my screen]".into())
        );
    }

    /// A turn that answered with a generative form and no words used to be written down as
    /// "[used form]" — the fallback for a tool nobody had named — which is what the thread
    /// showed for it ever after (Vamos, 21 Sep 2026). The stand-in says what the coworker did.
    #[test]
    fn a_generative_form_or_chart_is_remembered_as_shown_not_used() {
        let events = vec![
            json!({"type":"TOOL_CALL_START","toolCallId":"c1","toolCallName":"form"}),
            json!({"type":"TOOL_CALL_START","toolCallId":"c2","toolCallName":"bar_chart"}),
        ];
        assert_eq!(
            tool_standin(&deeds_from_replay(&events)),
            Some("[showed you a form, then drew you a chart]".into())
        );
    }

    /// The server says so with one frame before the first box-bound tool of a turn wakes a
    /// sleeping box; the footer must say what the wait is, not "working".
    #[test]
    fn a_box_being_woken_is_a_status() {
        let ev = json!({"type":"CUSTOM","name":"box-waking","coworkerId":"cw_1"});
        assert_eq!(
            activity_from_agui(&ev, None),
            ActivityTick::Set(BotActivity {
                label: WAKING_COMPUTER.into()
            })
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

    #[test]
    fn a_live_user_form_is_waiting_for_you_not_approval() {
        let ev = json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run-1",
            "callId": "call-9",
            "tool": "request_user_form",
            "reason": "user-form",
            "why": "Waiting for you",
            "arguments": {
                "title": "Google account email",
                "fields": [{"id":"email","label":"Email","type":"email","required":true}]
            }
        });
        assert_eq!(
            activity_from_agui(&ev, None),
            ActivityTick::Set(BotActivity {
                label: "Waiting for you".into()
            })
        );
        let fixture = json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": {
                "entryId": "e1",
                "formRequest": {
                    "title": "Google account email",
                    "fields": [{"id":"email","label":"Email","type":"email","required":true}]
                }
            }
        });
        assert_eq!(
            activity_from_agui(&fixture, None),
            ActivityTick::Set(BotActivity {
                label: "Waiting for you".into()
            })
        );
        let settled = json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": { "entryId": "e1", "formResolution": "submitted" }
        });
        assert_eq!(activity_from_agui(&settled, None), ActivityTick::Clear);
        let tool = json!({"type":"TOOL_CALL_START","toolCallName":"request_user_form"});
        assert_eq!(
            activity_from_agui(&tool, None),
            ActivityTick::Set(BotActivity {
                label: "Waiting for you".into()
            })
        );
    }
}
