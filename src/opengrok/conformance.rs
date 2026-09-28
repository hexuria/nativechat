//! Wire conformance: the frames and bodies opengrok-server sends, read by this app's own code.
//!
//! This app transcribes the server's wire by hand, and nothing else checks that the two still
//! agree. `fixtures/wire/` is the server's side of that: AG-UI frames exactly as `POST
//! /ag-ui` and the replays send them, and REST bodies exactly as the routes this app reads answer
//! them, in the layout of opengrok-server#255 (`agui/<type>/<slug>.json`, a CUSTOM under
//! `agui/custom/<name>/`, and `rest/<METHOD>_<route>/<status>-<slug>.json`). `MANIFEST.json`
//! names the server commit the corpus was taken from, the server test or builder behind every
//! file, and every `type`, CUSTOM `name`, approval `reason` and `formResolution` word the server's
//! code can send.
//!
//! Until the server's recorder lands the corpus is `recorded_by: "hand-copied"`: each file was
//! copied from the test or builder its manifest entry names. Ids and clocks those tests mint at
//! run time are placeholders in the server's own formats; everything a test fixes is as written.
//! The recorded corpus is meant to drop in over this one and be read by these tests unchanged.
//!
//! What is held here:
//!
//! - every frame is fed to the code that handles its type and name, and every body is parsed
//!   with the type this app reads that route with, and each must come out the way the app means
//!   to read it, not merely without a panic;
//! - the ledger, every wire word this app branches on, is either a word the manifest says the
//!   server sends or excused in [`NOT_SENT_BY_SERVER`], with the evidence;
//! - every word the server sends is either in the ledger or excused in [`CLIENT_IGNORES`], so a
//!   name the server starts sending fails here before a person finds it missing on screen;
//! - [`KNOWN_DRIFT`] names the fixtures this app still reads wrongly. Their checks must fail, so
//!   an entry cannot outlive its fix.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::activity::{
    ActivityTick, BOX_WAKING, BotActivity, ToolCallTracker, WAKING_COMPUTER, activity_from_agui,
};
use super::client::{
    AnswerReply, AsyncRunResponse, LocalExecMode, LocalExecPolicy, QueuedApproval, RecipeDetail,
    RecipeList, RecipeParameterKind, RecipeRunResult, RunCause, ScheduleKind, ScheduleRow,
    ScheduleRun, ScheduleRunStarted, ScheduleRunStatus, SkillDetail, SkillSummary, ThreadReplay,
};
use super::credential::{CREDENTIAL_OFFER_SAVE, SaveLoginSpec};
use super::gen_ui::{
    BAR_CHART_NAMES, ChatPart, EGRESS_TUNNEL_ASK_REASON, FORM_NAMES, REVIEW_AN_ACTION_REASONS,
    RUN_AWAITING_APPROVAL, StepSpec, TurnAssembler, UI_CUSTOM_NAME, USER_MACHINE_SHELL,
    approval_from_event, command_from_replay_events, is_ui_tool, step_arguments,
};
use super::pending::{CUSTOM_NAME as PENDING_CUSTOM, PendingCustom, PendingOp};
use super::timing::{RUN_TIMING_CUSTOM, TURN_TIMELINE_CUSTOM, TurnTiming};
use super::types::{
    Account, Coworker, ThreadListing, assistant_text_from_sse, error_message_from_body,
};
use super::user_form::{
    BoxHandoffReply, COMPUTER_HANDOFF_NAMES, ComputerHandoffStatus, FORM_ENTRY_MISSING,
    FORM_RESOLUTION_WORDS, FormResolution, USER_FORM_CUSTOM, USER_FORM_CUSTOM_NAMES,
    USER_FORM_REASON, UserFormActionReply, UserFormSpec, WAITING_FOR_YOU,
    box_handoff_action_from_http, is_user_form_awaiting, is_user_form_tool,
    user_form_action_from_http,
};
use super::visibility::ImageVisibility;

/// Where the corpus lives, from the package root.
const CORPUS: &str = "fixtures/wire";

/// Where a wire word sits on what the server sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    /// A frame's `type`.
    AguiType,
    /// A CUSTOM frame's `name`.
    CustomName,
    /// `reason` on `run-awaiting-approval`, and on `GET /ag-ui/approvals` rows.
    ApprovalReason,
    /// `formResolution` on a settled user-form card.
    FormResolution,
}

impl Slot {
    const ALL: [Slot; 4] = [
        Slot::AguiType,
        Slot::CustomName,
        Slot::ApprovalReason,
        Slot::FormResolution,
    ];

    fn field(self) -> &'static str {
        match self {
            Slot::AguiType => "frame type",
            Slot::CustomName => "CUSTOM name",
            Slot::ApprovalReason => "approval reason",
            Slot::FormResolution => "formResolution",
        }
    }
}

/// Every AG-UI `type` this app branches on anywhere (`TurnAssembler::push_event`,
/// `activity_from_agui`, `command_from_replay_events`, the SSE readers in `client.rs` and
/// `types.rs`, `PendingCustom::from_agui`, `TurnTiming::from_event`). Those match arms are
/// literals, so [`the_ledger_types_are_the_types_the_source_matches`] holds this list to the
/// source: a type matched anywhere and missing here, or listed here and matched nowhere, fails.
const AGUI_TYPES: &[&str] = &[
    "RUN_STARTED",
    "RUN_FINISHED",
    "RUN_ERROR",
    "TEXT_MESSAGE_START",
    "TEXT_MESSAGE_CONTENT",
    "TEXT_MESSAGE_CHUNK",
    "TOOL_CALL_START",
    "TOOL_CALL_ARGS",
    "TOOL_CALL_END",
    "TOOL_CALL_CHUNK",
    "TOOL_CALL_RESULT",
    "REASONING_START",
    "REASONING_MESSAGE_START",
    "REASONING_MESSAGE_CONTENT",
    "REASONING_MESSAGE_END",
    "REASONING_MESSAGE_CHUNK",
    "CUSTOM",
    "custom",
];

/// Every `type` in AG-UI 0.0.57 (`@ag-ui/core` `EventType`, as opengrok-wire `agui.rs`
/// transcribes it), and the one lowercase spelling this app accepts. The source scan looks for
/// these, so it catches a new match arm for any type the protocol has.
const AGUI_SPEC_TYPES: &[&str] = &[
    "TEXT_MESSAGE_START",
    "TEXT_MESSAGE_CONTENT",
    "TEXT_MESSAGE_END",
    "TEXT_MESSAGE_CHUNK",
    "TOOL_CALL_START",
    "TOOL_CALL_ARGS",
    "TOOL_CALL_END",
    "TOOL_CALL_CHUNK",
    "TOOL_CALL_RESULT",
    "THINKING_START",
    "THINKING_END",
    "THINKING_TEXT_MESSAGE_START",
    "THINKING_TEXT_MESSAGE_CONTENT",
    "THINKING_TEXT_MESSAGE_END",
    "STATE_SNAPSHOT",
    "STATE_DELTA",
    "MESSAGES_SNAPSHOT",
    "ACTIVITY_SNAPSHOT",
    "ACTIVITY_DELTA",
    "RAW",
    "CUSTOM",
    "RUN_STARTED",
    "RUN_FINISHED",
    "RUN_ERROR",
    "STEP_STARTED",
    "STEP_FINISHED",
    "REASONING_START",
    "REASONING_MESSAGE_START",
    "REASONING_MESSAGE_CONTENT",
    "REASONING_MESSAGE_END",
    "REASONING_MESSAGE_CHUNK",
    "REASONING_END",
    "REASONING_ENCRYPTED_VALUE",
    "custom",
];

/// The ledger: every wire word this app branches on, by where it sits. The names, reasons and
/// resolutions are the very constants and lists the matching code reads, so a word added there
/// lands here; the types are held to the source by a scan (see [`AGUI_TYPES`]).
fn ledger() -> Vec<(Slot, &'static str)> {
    let mut words: Vec<(Slot, &'static str)> = AGUI_TYPES
        .iter()
        .map(|word| (Slot::AguiType, *word))
        .collect();
    let names = [
        RUN_AWAITING_APPROVAL,
        BOX_WAKING,
        PENDING_CUSTOM,
        RUN_TIMING_CUSTOM,
        TURN_TIMELINE_CUSTOM,
        CREDENTIAL_OFFER_SAVE,
        UI_CUSTOM_NAME,
        // `UiSpec::from_custom` reads a CUSTOM with no name as a widget that names itself.
        "",
    ]
    .into_iter()
    .chain(USER_FORM_CUSTOM_NAMES.iter().copied())
    .chain(COMPUTER_HANDOFF_NAMES.iter().copied())
    .chain(BAR_CHART_NAMES.iter().copied())
    .chain(FORM_NAMES.iter().copied());
    words.extend(names.map(|word| (Slot::CustomName, word)));
    words.extend(
        REVIEW_AN_ACTION_REASONS
            .iter()
            .chain([USER_FORM_REASON].iter())
            .map(|word| (Slot::ApprovalReason, *word)),
    );
    words.extend(
        FORM_RESOLUTION_WORDS
            .iter()
            .map(|(word, _)| (Slot::FormResolution, *word)),
    );
    words
}

/// Words this app matches that the server does not send, each checked against the server at
/// the manifest's commit (every `*.rs`, `*.ts`, `*.json` and the docs, with the snake_case,
/// camelCase and hyphenated spellings, as a `type`, a `name`, a `reason`, a field). The client
/// code for them stays until somebody decides what to do with it; this list is the record of why
/// the server never sends each.
const NOT_SENT_BY_SERVER: &[(Slot, &str, &str)] = &[
    (
        Slot::AguiType,
        "TEXT_MESSAGE_CHUNK",
        "AG-UI's shorthand for a text delta. The server's projection opens, fills and closes every \
         message (TEXT_MESSAGE_START/CONTENT/END, opengrok-harness projection.rs) and never \
         sends a chunk; it is read so another AG-UI producer still paints.",
    ),
    (
        Slot::AguiType,
        "TOOL_CALL_CHUNK",
        "AG-UI's shorthand for a tool call. The server sends TOOL_CALL_START/ARGS/END \
         (projection.rs) and never a chunk.",
    ),
    (
        Slot::AguiType,
        "REASONING_START",
        "The server opens reasoning with REASONING_MESSAGE_START and never sends \
         REASONING_START (projection.rs).",
    ),
    (
        Slot::AguiType,
        "REASONING_MESSAGE_CHUNK",
        "The server's reasoning is REASONING_MESSAGE_START/CONTENT/END (projection.rs); it sends \
         no chunk.",
    ),
    (
        Slot::AguiType,
        "custom",
        "The lowercase spelling timing.rs accepts for a run-timing frame. opengrok-wire \
         serialises EventType in SCREAMING_SNAKE_CASE, so the server only ever says CUSTOM.",
    ),
    (
        Slot::CustomName,
        "turn-timeline",
        "The name first proposed for run-timing. The harness names the frame run-timing \
         (opengrok-harness timing.rs RUN_TIMING_NAME); no server file says turn-timeline.",
    ),
    (
        Slot::CustomName,
        "form-request",
        "No frame is named form-request or form_request. formRequest is a field: the user-form \
         card's message.formRequest (cards.rs user_form_card) and its alias on the stamped \
         run-awaiting-approval (agui/resume.rs apply_user_form_stamp), which this app reads.",
    ),
    (
        Slot::CustomName,
        "form-resolution",
        "No frame is named form-resolution or form_resolution. formResolution is a field on a \
         settled card and on the user-form CUSTOM (agui/user_form.rs agui_user_form_frame).",
    ),
    (
        Slot::CustomName,
        "request-user-form",
        "No CUSTOM is named request-user-form or request_user_form. request_user_form is the \
         tool (opengrok-tools user_form.rs REQUEST_USER_FORM): it arrives as \
         TOOL_CALL_START.toolCallName and as run-awaiting-approval.tool, where this app reads it \
         through is_user_form_tool. The live card is run-awaiting-approval with reason \
         user-form, the settled one a user-form CUSTOM.",
    ),
    (
        Slot::CustomName,
        "computer-handoff-card",
        "The Computer handoff is a gateway transcript entry (cards.rs computer_handoff_card: \
         message.type attachment, url sand://box, boxRequestId, boxInstruction), appended to the \
         transcript and never sent as a frame; cards.rs says there is no computer-handoff message \
         type. This app learns of it from the dismiss reply's handoffEntryId.",
    ),
    (
        Slot::CustomName,
        "computer-handoff",
        "No frame carries a handoff; see computer-handoff-card.",
    ),
    (
        Slot::CustomName,
        "sand://box",
        "The url on the handoff entry's attachment message (cards.rs), never a frame name.",
    ),
    (
        Slot::CustomName,
        "sand:box",
        "No spelling of sand://box names a frame.",
    ),
    (
        Slot::CustomName,
        "box-handoff",
        "Only in the route /ag-ui/box-handoff/resolve and the functions behind it \
         (agui/user_form.rs resolve_box_handoff, start_box_handoff); no frame is named \
         box-handoff, box_handoff or boxHandoff. The handoff is a transcript entry.",
    ),
    (
        Slot::CustomName,
        "box-handoff-card",
        "Never sent; see box-handoff.",
    ),
    (
        Slot::CustomName,
        "ui",
        "The server paints charts and forms through the bar_chart and form tools it offers the \
         model (agui/chat_ui.rs, \"NativeChat paints them from the TOOL_CALL frames\") and sends \
         no generative-UI CUSTOM.",
    ),
    (
        Slot::CustomName,
        "bar-chart",
        "A tool name on the server (bar_chart, agui/chat_ui.rs), read here from TOOL_CALL \
         frames; never a CUSTOM name.",
    ),
    (
        Slot::CustomName,
        "barchart",
        "Never sent under any name; see bar-chart.",
    ),
    (
        Slot::CustomName,
        "form",
        "A tool name on the server (agui/chat_ui.rs form_schema), never a CUSTOM name.",
    ),
    (
        Slot::CustomName,
        "",
        "A CUSTOM with no name. Every CUSTOM the server builds is named (projection.rs, \
         agui/pending.rs, agui/user_form.rs, opengrok-tools credential.rs).",
    ),
    (
        Slot::ApprovalReason,
        "review-an-action",
        "The name of Grok Bot's card chrome, in server comments only (opengrok-tools review.rs \
         EGRESS_TUNNEL_ASK_REASON's doc). That card is raised with reason auto-review, and the \
         tunnel's with EGRESS_TUNNEL_ASK_REASON as its why (opengrok-tools lib.rs); no \
         spelling of review-an-action is a reason.",
    ),
    (
        Slot::ApprovalReason,
        "computer-action",
        "A Grok Bot desktop gateway event channel (the server's docs/research/client-grok-bot.md \
         and docs/archive/client-versions-0.18-0.30.md), in no server code. The server's \
         reasons are exec-consent, policy-approval, auto-review and user-form (opengrok-core \
         run.rs SuspendReason, opengrok-tools review.rs AwaitingReason).",
    ),
    (
        Slot::ApprovalReason,
        "egress",
        "The tunnel's card arrives as reason auto-review with EGRESS_TUNNEL_ASK_REASON as its \
         why, and is_egress_tunnel reads that sentence. No server reason is the word egress.",
    ),
    (
        Slot::FormResolution,
        "sending",
        "This app's own pill between Continue and the reply. The server settles a form as \
         submitted, fill_failed, dismissed or escalated and nothing else (opengrok-tools \
         user_form.rs FormResolution::as_str).",
    ),
    (
        Slot::FormResolution,
        "submitting",
        "Another spelling of this app's Sending pill; never on the wire.",
    ),
    (
        Slot::FormResolution,
        "fill-failed",
        "The server spells it fill_failed (FormResolution::as_str); the hyphenated word is in no \
         server file. fillFailed is a different field, on formFieldOutcomes rows.",
    ),
    (
        Slot::FormResolution,
        "not_filled",
        "Not filled is this app's pill for fill_failed. No server file spells a resolution \
         not_filled, not-filled or notFilled.",
    ),
    (
        Slot::FormResolution,
        "not-filled",
        "Never sent; see not_filled.",
    ),
    (
        Slot::FormResolution,
        "on_screen",
        "No server file spells a resolution on_screen, on-screen, on_the_computer or \
         on-the-computer. The server's word for Open the screen is escalated \
         (agui/user_form.rs dismiss_user_form), which this app reads too.",
    ),
    (
        Slot::FormResolution,
        "on-screen",
        "Never sent; see on_screen.",
    ),
    (
        Slot::FormResolution,
        "on_the_computer",
        "Never sent; see on_screen.",
    ),
    (
        Slot::FormResolution,
        "on-the-computer",
        "Never sent; see on_screen.",
    ),
    (
        Slot::FormResolution,
        "skipped",
        "Skip settles the form here (UserFormSpec::settle_form_from_box, declined to Skipped). \
         The server writes boxResolution declined on the handoff entry and never a skipped \
         formResolution.",
    ),
    (Slot::FormResolution, "skip", "Never sent; see skipped."),
    (
        Slot::FormResolution,
        "superseded",
        "This app paints it when a later message moved the thread on. The server closes that \
         card as dismissed (agui/user_form.rs settle_dead_holds).",
    ),
];

/// Words the server sends that this app has no arm for, each with what happens instead.
const CLIENT_IGNORES: &[(Slot, &str, &str)] = &[
    (
        Slot::AguiType,
        "TEXT_MESSAGE_END",
        "Words are painted delta by delta and a turn ends on RUN_FINISHED or RUN_ERROR, so the \
         end of one message changes nothing on screen.",
    ),
    (
        Slot::CustomName,
        "run-stopped",
        "The RUN_FINISHED the server always sends right after it (projection.rs stopped) ends \
         the turn, so on the stream a stop reads as a finish.",
    ),
    (
        Slot::ApprovalReason,
        "exec-consent",
        "The default card. approval_from_event fills this word in when a frame has none, and \
         which card shows is decided by the tool (user_machine_shell or not), not by the word.",
    ),
    (
        Slot::ApprovalReason,
        "policy-approval",
        "Shown as the default permission card, allow or deny once; nothing branches on the word.",
    ),
];

/// Fixtures this app still reads wrongly, with the words their check fails with and why. The
/// check has to fail with those words: one that passes means the drift is fixed and the entry
/// goes, and one that fails some other way is a new problem, not this one.
const KNOWN_DRIFT: &[(&str, &str, &str)] = &[];

// ---- the corpus ----

#[derive(Debug, Deserialize)]
struct Manifest {
    server_sha: String,
    recorded_by: String,
    emits: Emits,
    entries: Vec<Entry>,
    /// Names the server can send that no test exercises yet (#255 asks the recorder to say so).
    #[serde(default)]
    unrecorded: Vec<String>,
}

/// What the server's code can send. #255 names the first two lists; the reasons and the
/// resolutions are this app's addition, because four of the eight dead words were words in
/// those two fields rather than names.
#[derive(Debug, Deserialize)]
struct Emits {
    agui_types: Vec<String>,
    custom_names: Vec<String>,
    approval_reasons: Vec<String>,
    form_resolutions: Vec<String>,
}

impl Emits {
    fn words(&self, slot: Slot) -> &[String] {
        match slot {
            Slot::AguiType => &self.agui_types,
            Slot::CustomName => &self.custom_names,
            Slot::ApprovalReason => &self.approval_reasons,
            Slot::FormResolution => &self.form_resolutions,
        }
    }
}

#[derive(Debug, Deserialize)]
struct Entry {
    file: String,
    source: String,
}

struct Corpus {
    root: PathBuf,
    manifest: Manifest,
    /// Every AG-UI frame, by its path under the corpus root.
    frames: BTreeMap<String, Value>,
    /// Every REST fixture, by its path under the corpus root.
    bodies: BTreeMap<String, Value>,
}

impl Corpus {
    fn load() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(CORPUS);
        let manifest: Manifest = serde_json::from_value(read_json(&root, "MANIFEST.json"))
            .unwrap_or_else(|error| {
                panic!(
                    "{CORPUS}/MANIFEST.json is not #255's manifest with emits.approval_reasons \
                     and emits.form_resolutions beside the types and names: {error}"
                )
            });
        let frames = json_files(&root, "agui")
            .into_iter()
            .map(|file| {
                let frame = read_json(&root, &file);
                (file, frame)
            })
            .collect();
        let bodies = json_files(&root, "rest")
            .into_iter()
            .map(|file| {
                let body = read_json(&root, &file);
                (file, body)
            })
            .collect();
        Self {
            root,
            manifest,
            frames,
            bodies,
        }
    }

    /// Every frame and every REST fixture.
    fn every_value(&self) -> impl Iterator<Item = &Value> {
        self.frames.values().chain(self.bodies.values())
    }

    fn frames_of<'a>(&'a self, kind: &'a str) -> impl Iterator<Item = &'a Value> {
        self.frames
            .values()
            .filter(move |frame| str_at(frame, "type") == kind)
    }

    fn customs_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Value> {
        self.frames_of("CUSTOM")
            .filter(move |frame| str_at(frame, "name") == name)
    }

    /// A permission card from the corpus, re-aimed at `call_id`, for the frames whose meaning is
    /// what they do to a card.
    fn approval_card_for(&self, call_id: &str) -> Option<Value> {
        let mut card = self
            .customs_named(RUN_AWAITING_APPROVAL)
            .find(|frame| str_at(frame, "reason") != USER_FORM_REASON)?
            .clone();
        card["callId"] = Value::String(call_id.to_string());
        Some(card)
    }

    /// The call's own opening when the corpus has it, else the corpus's opening re-aimed at
    /// `call_id`, for the frames whose meaning is what they do to a step.
    fn tool_call_start_for(&self, call_id: &str) -> Option<Value> {
        let own = self
            .frames_of("TOOL_CALL_START")
            .find(|start| str_at(start, "toolCallId") == call_id);
        let mut start = own
            .or_else(|| self.frames_of("TOOL_CALL_START").next())?
            .clone();
        start["toolCallId"] = Value::String(call_id.to_string());
        Some(start)
    }

    /// Every frame of the reasoning message `message_id`, in the order a run sends them.
    fn reasoning_frames(&self, message_id: &str) -> Vec<&Value> {
        [
            "REASONING_MESSAGE_START",
            "REASONING_MESSAGE_CONTENT",
            "REASONING_MESSAGE_END",
        ]
        .into_iter()
        .flat_map(|kind| {
            self.frames_of(kind)
                .filter(move |frame| str_at(frame, "messageId") == message_id)
        })
        .collect()
    }
}

/// Every `.json` under `dir`, relative to the corpus root and with `/` between parts.
fn json_files(root: &Path, dir: &str) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let entries = std::fs::read_dir(dir)
            .unwrap_or_else(|error| panic!("read {}: {error}", dir.display()));
        for entry in entries {
            let path = entry.expect("a directory entry").path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if path.extension().is_some_and(|ext| ext == "json") {
                let rel = path.strip_prefix(root).expect("under the corpus root");
                let parts: Vec<String> = rel
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push(parts.join("/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, &root.join(dir), &mut out);
    out.sort();
    out
}

fn read_json(root: &Path, rel: &str) -> Value {
    let path = root.join(rel);
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    serde_json::from_str(&text)
        .unwrap_or_else(|error| panic!("{} is not JSON: {error}", path.display()))
}

fn str_at<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or("")
}

/// Whether `word`, in `slot`, is on `list`. Keyed by slot: `user-form` is both a CUSTOM name and
/// an approval reason, and excusing it in one place must not excuse it in the other.
fn is_excused(list: &[(Slot, &str, &str)], slot: Slot, word: &str) -> bool {
    list.iter()
        .any(|(listed_slot, listed, _)| *listed_slot == slot && *listed == word)
}

/// One check's verdict: `Err` says what came out wrong.
type Check = Result<(), String>;

/// Fail the check with a sentence when `$cond` does not hold.
macro_rules! must {
    ($cond:expr, $($why:tt)+) => {
        if !$cond {
            return Err(format!($($why)+));
        }
    };
}

/// Runs `check` over `items` and says what went wrong, holding [`KNOWN_DRIFT`] entries to failing
/// the way they are known to.
fn verdicts<'a>(
    items: impl Iterator<Item = (&'a String, &'a Value)>,
    check: impl Fn(&str, &Value) -> Check,
) -> Vec<String> {
    let mut problems = Vec::new();
    for (file, value) in items {
        let symptom = KNOWN_DRIFT
            .iter()
            .find(|(drifted, _, _)| drifted == file)
            .map(|(_, symptom, _)| *symptom);
        match (check(file, value), symptom) {
            (Ok(()), None) => {}
            (Err(why), Some(symptom)) if why.contains(symptom) => {}
            (Err(why), None) => problems.push(format!("{file}: {why}")),
            (Err(why), Some(_)) => problems.push(format!(
                "{file} fails other than its known drift says: {why}"
            )),
            (Ok(()), Some(_)) => problems.push(format!(
                "{file} now reads as intended; take it off KNOWN_DRIFT"
            )),
        }
    }
    problems
}

// ---- what each frame is meant to do ----

fn tick(frame: &Value) -> ActivityTick {
    activity_from_agui(frame, None)
}

fn label(text: &str) -> ActivityTick {
    ActivityTick::Set(BotActivity {
        label: text.to_string(),
    })
}

fn assembled(frames: &[&Value]) -> TurnAssembler {
    let mut assembler = TurnAssembler::default();
    for frame in frames {
        assembler.push_event(frame);
    }
    assembler
}

/// Frames as the `POST /ag-ui` stream carries them, one `data:` line each.
fn sse(frames: &[&Value]) -> String {
    frames
        .iter()
        .map(|frame| format!("data: {frame}\n\n"))
        .collect()
}

fn check_frame(corpus: &Corpus, frame: &Value) -> Check {
    match str_at(frame, "type") {
        "RUN_STARTED" => {
            must!(
                tick(frame) == label("Thinking"),
                "RUN_STARTED should say Thinking, not {:?}",
                tick(frame)
            );
            Ok(())
        }
        "RUN_FINISHED" | "RUN_ERROR" => run_ended(corpus, frame),
        "TEXT_MESSAGE_START" => {
            must!(
                tick(frame) == label("Writing"),
                "TEXT_MESSAGE_START should say Writing, not {:?}",
                tick(frame)
            );
            Ok(())
        }
        "TEXT_MESSAGE_CONTENT" => text_content(corpus, frame),
        "TOOL_CALL_START" => tool_call_start(frame),
        "TOOL_CALL_ARGS" => tool_call_args(corpus, frame),
        "TOOL_CALL_END" => tool_call_end(corpus, frame),
        "TOOL_CALL_RESULT" => tool_call_result(corpus, frame),
        "REASONING_MESSAGE_START" | "REASONING_MESSAGE_CONTENT" | "REASONING_MESSAGE_END" => {
            reasoning(corpus, frame)
        }
        "CUSTOM" => custom(frame),
        kind if is_excused(CLIENT_IGNORES, Slot::AguiType, kind) => ignored(frame),
        kind => Err(format!(
            "no check for a {kind:?} frame: say here what this app does with one"
        )),
    }
}

fn run_ended(corpus: &Corpus, frame: &Value) -> Check {
    must!(
        tick(frame) == ActivityTick::Clear,
        "a run's end should clear the status line, not {:?}",
        tick(frame)
    );
    if let Some(card) = corpus.approval_card_for("c-ended") {
        let assembler = assembled(&[&card, frame]);
        must!(
            !assembler.waiting_approval(),
            "the stream is over, so it is not waiting on the card any more"
        );
    }
    if str_at(frame, "type") == "RUN_ERROR" {
        let said = assistant_text_from_sse(&sse(&[frame]));
        must!(
            said == Err(str_at(frame, "message").to_string()),
            "RUN_ERROR should end the turn with the server's sentence, got {said:?}"
        );
    }
    Ok(())
}

fn text_content(corpus: &Corpus, frame: &Value) -> Check {
    let delta = str_at(frame, "delta");
    must!(!delta.is_empty(), "a text frame with no delta");
    must!(
        tick(frame) == label("Writing"),
        "text should say Writing, not {:?}",
        tick(frame)
    );
    // The message's own opening says whose words these are.
    let start = corpus
        .frames_of("TEXT_MESSAGE_START")
        .find(|start| start.get("messageId") == frame.get("messageId"));
    let persons = start.is_some_and(|start| str_at(start, "role") == "user");
    let mut frames: Vec<&Value> = start.into_iter().collect();
    frames.push(frame);
    let (plain, _) = assembled(&frames).snapshot();
    if persons {
        must!(
            !plain.contains(delta),
            "the person's own words {delta:?} were painted as the coworker's reply: {plain:?}"
        );
        return Ok(());
    }
    must!(
        plain == delta,
        "the reply should read {delta:?}, not {plain:?}"
    );
    let streamed = assistant_text_from_sse(&sse(&frames));
    must!(
        streamed == Ok(delta.to_string()),
        "the stream reader should keep {delta:?}, got {streamed:?}"
    );
    Ok(())
}

fn tool_call_start(frame: &Value) -> Check {
    let mut tracker = ToolCallTracker::default();
    let status = tracker.tick(frame);
    let tool = str_at(frame, "toolCallName");
    must!(
        matches!(&status, ActivityTick::Set(activity)
            if activity.label != "Working" && activity.label != format!("Using {tool}")),
        "TOOL_CALL_START should name what is being done, not {status:?}"
    );
    must!(
        tracker.deeds().len() == 1,
        "the call should be one thing the turn did, got {:?}",
        tracker.deeds()
    );
    // A call is a step of the reply from the moment it starts, unless it is drawn as itself.
    let call_id = str_at(frame, "toolCallId");
    let (_, parts) = assembled(&[frame]).snapshot();
    let expected = if is_ui_tool(tool) || is_user_form_tool(tool) {
        Vec::new()
    } else {
        vec![ChatPart::Step(StepSpec {
            call_id: call_id.to_string(),
            tool: tool.to_string(),
            arguments: String::new(),
            result: None,
            ok: None,
        })]
    };
    must!(
        parts == expected,
        "TOOL_CALL_START for {tool:?} should draw {expected:?}, got {parts:?}"
    );
    Ok(())
}

fn tool_call_args(corpus: &Corpus, frame: &Value) -> Check {
    let call_id = str_at(frame, "toolCallId");
    let delta = str_at(frame, "delta");
    let command = command_from_replay_events(std::slice::from_ref(frame), call_id);
    let sent = serde_json::from_str::<Value>(delta).ok().and_then(|args| {
        args.get("command")
            .and_then(Value::as_str)
            .map(str::to_string)
    });
    must!(
        sent.is_some(),
        "the fixture's delta should be a shell call's arguments with a command: {delta:?}"
    );
    must!(
        Some(&command) == sent.as_ref(),
        "the command of {call_id:?} should read back as {sent:?}, got {command:?}"
    );
    // The step holds none of the text the arguments come as while they are coming: what it
    // keeps of them is decided when they end (see `tool_call_end`).
    let start = corpus
        .tool_call_start_for(call_id)
        .ok_or("the corpus has no TOOL_CALL_START to open the call")?;
    let (_, parts) = assembled(&[&start, frame]).snapshot();
    must!(
        parts.iter().any(
            |part| matches!(part, ChatPart::Step(step) if step.call_id == call_id && step.arguments.is_empty())
        ),
        "the step for {call_id:?} should hold none of its argument text before its end: {parts:?}"
    );
    Ok(())
}

/// The end of a call says Thinking, and is where its step keeps its arguments: as the
/// approval card's rules let it say them (`step_arguments`), which for this shell call is the
/// arguments as they came.
fn tool_call_end(corpus: &Corpus, frame: &Value) -> Check {
    must!(
        tick(frame) == label("Thinking"),
        "TOOL_CALL_END should say Thinking, not {:?}",
        tick(frame)
    );
    let call_id = str_at(frame, "toolCallId");
    let start = corpus
        .tool_call_start_for(call_id)
        .ok_or("the corpus has no TOOL_CALL_START to open the call")?;
    let args = corpus
        .frames_of("TOOL_CALL_ARGS")
        .find(|args| str_at(args, "toolCallId") == call_id)
        .ok_or("the corpus has no TOOL_CALL_ARGS for the call")?;
    let delta = str_at(args, "delta");
    let tool = str_at(&start, "toolCallName");
    let sent: Value = serde_json::from_str(delta)
        .map_err(|error| format!("the fixture's arguments are not JSON: {error}"))?;
    let kept = step_arguments(tool, &sent);
    must!(
        kept == sent,
        "a {tool} call's arguments are what its card shows, as sent: {kept}"
    );
    let kept = kept.to_string();
    let (_, parts) = assembled(&[&start, args, frame]).snapshot();
    must!(
        parts.iter().any(
            |part| matches!(part, ChatPart::Step(step) if step.call_id == call_id && step.arguments == kept)
        ),
        "the step for {call_id:?} should keep {kept:?} once its arguments end: {parts:?}"
    );
    Ok(())
}

fn tool_call_result(corpus: &Corpus, frame: &Value) -> Check {
    let call_id = str_at(frame, "toolCallId");
    let content = str_at(frame, "content");
    let card = corpus
        .approval_card_for(call_id)
        .ok_or("the corpus has no permission card to answer")?;
    let assembler = assembled(&[&card, frame]);
    must!(
        !assembler.waiting_approval(),
        "a result means the card was answered"
    );
    let (_, parts) = assembler.snapshot();
    let ok = frame.get("ok").and_then(Value::as_bool);
    let answered = |spec: &super::gen_ui::ApprovalSpec| {
        spec.call_id == call_id && spec.output.as_deref() == Some(content) && spec.ok == ok
    };
    must!(
        parts
            .iter()
            .any(|part| matches!(part, ChatPart::Approval(spec) if answered(spec))),
        "the result should land on the card for {call_id:?}: {parts:?}"
    );
    // The result lands on the call's step, as sent. While the card waits the call is drawn as
    // the card alone; once its result is in, it is a step again.
    let start = corpus
        .tool_call_start_for(call_id)
        .ok_or("the corpus has no TOOL_CALL_START to open the call")?;
    let came_back = |step: &StepSpec| {
        step.call_id == call_id && step.result.as_deref() == Some(content) && step.ok == ok
    };
    let (_, stepped) = assembled(&[&start, frame]).snapshot();
    must!(
        stepped
            .iter()
            .any(|part| matches!(part, ChatPart::Step(step) if came_back(step))),
        "the result should land on the step for {call_id:?}: {stepped:?}"
    );
    let (_, waiting) = assembled(&[&start, &card]).snapshot();
    must!(
        !waiting
            .iter()
            .any(|part| matches!(part, ChatPart::Step(step) if step.call_id == call_id)),
        "a call waiting on its card is drawn as the card and not also as a step: {waiting:?}"
    );
    let (_, answered) = assembled(&[&start, &card, frame]).snapshot();
    must!(
        answered
            .iter()
            .any(|part| matches!(part, ChatPart::Step(step) if came_back(step))),
        "a call whose card was answered is a step with its result: {answered:?}"
    );
    if let Some(image) = frame.get("image") {
        let shot = assembler
            .latest_screenshot()
            .ok_or("the picture on the result was not kept")?;
        must!(
            Some(u64::from(shot.width)) == image.get("width").and_then(Value::as_u64)
                && Some(u64::from(shot.height)) == image.get("height").and_then(Value::as_u64),
            "the picture is {}x{}, not what the frame says",
            shot.width,
            shot.height
        );
        let visibility = ImageVisibility::parse(str_at(image, "visibility"));
        must!(
            shot.visibility == visibility,
            "the picture should be {visibility:?}, not {:?}",
            shot.visibility
        );
        let pinned = parts
            .iter()
            .any(|part| matches!(part, ChatPart::Screenshot(pinned) if pinned.call_id == call_id));
        must!(
            pinned != (visibility == Some(ImageVisibility::Agent)),
            "an agent picture stays in the Computer pane and any other goes in the feed"
        );
    }
    Ok(())
}

/// A reasoning frame puts Thinking on the status line while the coworker thinks and leaves it
/// alone when the thought ends, and the message it belongs to is one thought in the reply that
/// reads as the fixture's words and is none of the reply's own. The end is what closes it.
fn reasoning(corpus: &Corpus, frame: &Value) -> Check {
    let kind = str_at(frame, "type");
    let status = if kind == "REASONING_MESSAGE_END" {
        ActivityTick::Keep
    } else {
        label("Thinking")
    };
    must!(
        tick(frame) == status,
        "{kind} should leave the status line at {status:?}, not {:?}",
        tick(frame)
    );
    let message = corpus.reasoning_frames(str_at(frame, "messageId"));
    let said: String = message
        .iter()
        .filter(|frame| str_at(frame, "type") == "REASONING_MESSAGE_CONTENT")
        .map(|frame| str_at(frame, "delta"))
        .collect();
    must!(
        !said.trim().is_empty(),
        "the corpus has no words for the reasoning message of {kind}"
    );
    let (plain, parts) = assembled(&message).snapshot();
    must!(
        parts == vec![ChatPart::Reasoning(said.trim().to_string())],
        "the reasoning should be one thought reading {said:?}, got {parts:?}"
    );
    must!(
        plain.is_empty(),
        "a thought is not the reply's words: {plain:?}"
    );
    if kind == "REASONING_MESSAGE_END" {
        let still_open: Vec<&Value> = message
            .iter()
            .copied()
            .filter(|frame| str_at(frame, "type") != "REASONING_MESSAGE_END")
            .collect();
        must!(
            assembled(&still_open).snapshot().1.is_empty(),
            "a thought is drawn once its message ends, not while it is still being said"
        );
    }
    Ok(())
}

fn custom(frame: &Value) -> Check {
    match str_at(frame, "name") {
        RUN_AWAITING_APPROVAL => awaiting(frame),
        BOX_WAKING => {
            must!(
                tick(frame) == label(WAKING_COMPUTER),
                "box-waking should say {WAKING_COMPUTER:?}, not {:?}",
                tick(frame)
            );
            nothing_painted(frame)
        }
        RUN_TIMING_CUSTOM | TURN_TIMELINE_CUSTOM => run_timing(frame),
        PENDING_CUSTOM => pending(frame),
        USER_FORM_CUSTOM => settled_form(frame),
        CREDENTIAL_OFFER_SAVE => offer_save(frame),
        name if is_excused(CLIENT_IGNORES, Slot::CustomName, name) => ignored(frame),
        name => Err(format!(
            "no check for a CUSTOM {name:?}: say here what this app does with one"
        )),
    }
}

/// A frame this app has no arm for leaves the turn as it was.
fn ignored(frame: &Value) -> Check {
    must!(
        tick(frame) == ActivityTick::Keep,
        "an ignored frame should leave the status line alone, not {:?}",
        tick(frame)
    );
    nothing_painted(frame)
}

fn nothing_painted(frame: &Value) -> Check {
    let assembler = assembled(&[frame]);
    let (plain, parts) = assembler.snapshot();
    must!(
        plain.is_empty() && parts.is_empty(),
        "nothing should be painted, got {plain:?} {parts:?}"
    );
    must!(
        !assembler.waiting_approval() && !assembler.waiting_user_form(),
        "nothing should be waiting"
    );
    Ok(())
}

fn awaiting(frame: &Value) -> Check {
    let reason = str_at(frame, "reason");
    let assembler = assembled(&[frame]);
    let (_, parts) = assembler.snapshot();
    if reason == USER_FORM_REASON {
        must!(is_user_form_awaiting(frame), "a user-form park is a form");
        let spec = UserFormSpec::from_awaiting_event(frame, None)
            .ok_or("the form did not parse from its arguments")?;
        must!(
            spec.entry_id == str_at(frame, "entryId")
                && spec.call_id == str_at(frame, "callId")
                && spec.run_id == str_at(frame, "runId"),
            "the card should carry the frame's entryId, callId and runId: {spec:?}"
        );
        let fields = frame
            .pointer("/arguments/fields")
            .and_then(Value::as_array)
            .ok_or("a form with no fields")?;
        let secret = fields
            .iter()
            .filter(|field| {
                matches!(str_at(field, "type"), "password" | "otp")
                    || field.get("secret").and_then(Value::as_bool) == Some(true)
            })
            .count();
        must!(
            spec.fields.len() == fields.len()
                && spec.fields.iter().filter(|field| field.masked()).count() == secret,
            "every field should show, and every secret one masked: {:?}",
            spec.fields
        );
        must!(
            spec.title == str_at(&frame["arguments"], "title") && spec.is_unresolved(),
            "an open card titled as the form: {spec:?}"
        );
        let card = |part: &ChatPart| matches!(part, ChatPart::UserForm(card) if card.entry_id == spec.entry_id);
        must!(
            assembler.waiting_user_form()
                && parts.iter().any(card)
                && !parts
                    .iter()
                    .any(|part| matches!(part, ChatPart::Approval(_))),
            "a form card, and not a permission card: {parts:?}"
        );
        must!(
            tick(frame) == label(WAITING_FOR_YOU),
            "a form should say {WAITING_FOR_YOU:?}, not {:?}",
            tick(frame)
        );
        return Ok(());
    }
    let spec = approval_from_event(frame).ok_or("the card did not parse")?;
    must!(
        spec.run_id == str_at(frame, "runId")
            && spec.call_id == str_at(frame, "callId")
            && spec.tool == str_at(frame, "tool")
            && spec.reason == reason
            && spec.why == str_at(frame, "why")
            && spec.thread_id.as_deref() == frame.get("threadId").and_then(Value::as_str),
        "the card should carry the frame's run, call, tool, reason and why: {spec:?}"
    );
    if let Some(command) = frame.pointer("/arguments/command").and_then(Value::as_str) {
        must!(
            spec.command == command,
            "the card should show {command:?}, not {:?}",
            spec.command
        );
    }
    let arguments = frame.get("arguments").unwrap_or(&Value::Null);
    if arguments.is_object() && !matches!(spec.tool.as_str(), "shell" | USER_MACHINE_SHELL) {
        must!(
            !spec.summary.is_empty(),
            "a {} call should say what it would do",
            spec.tool
        );
    }
    must!(
        assembler.waiting_approval()
            && parts.iter().any(
                |part| matches!(part, ChatPart::Approval(card) if card.call_id == spec.call_id)
            ),
        "a permission card should be up and waiting: {parts:?}"
    );
    must!(
        tick(frame) == label("Waiting for approval"),
        "a card should say Waiting for approval, not {:?}",
        tick(frame)
    );
    match reason {
        "auto-review" => {
            must!(
                spec.is_review_an_action(),
                "auto-review is the Review-an-action card"
            );
            let tunnel = spec.why.trim() == EGRESS_TUNNEL_ASK_REASON && !spec.runs_on_this_mac();
            must!(
                spec.is_egress_tunnel() == tunnel,
                "only the tunnel's own sentence makes the tunnel's card"
            );
        }
        "exec-consent" | "policy-approval" => must!(
            !spec.is_review_an_action() && !spec.is_egress_tunnel(),
            "{reason} is the plain permission card"
        ),
        other => return Err(format!("no expectation for a card with reason {other:?}")),
    }
    Ok(())
}

fn run_timing(frame: &Value) -> Check {
    let timing = TurnTiming::from_event(frame).ok_or("the timing did not parse")?;
    let value = frame.get("value").unwrap_or(frame);
    let rounds = value
        .get("tool_rounds")
        .and_then(Value::as_u64)
        .and_then(|rounds| u32::try_from(rounds).ok());
    let tools = value
        .get("tools")
        .and_then(Value::as_array)
        .map_or(0, Vec::len);
    must!(
        timing.tool_rounds == rounds
            && timing.tools.len() == tools
            && timing.total_ms == value.get("total_ms").and_then(Value::as_u64),
        "the timing should keep its rounds, tools and total: {timing:?}"
    );
    must!(
        tick(frame) == ActivityTick::Keep,
        "timing is not a status, got {:?}",
        tick(frame)
    );
    nothing_painted(frame)
}

fn pending(frame: &Value) -> Check {
    let custom = PendingCustom::from_agui(frame).ok_or("the pending frame did not parse")?;
    let value = &frame["value"];
    must!(
        Some(custom.op) == PendingOp::parse(str_at(value, "op"))
            && custom.thread_id == str_at(value, "threadId"),
        "the op and the thread should come through: {custom:?}"
    );
    match (value.get("message"), &custom.message) {
        (Some(raw), Some(message)) => must!(
            message.id == str_at(raw, "id")
                && message.content == str_at(raw, "content")
                && message.bubble_id() == str_at(raw, "clientMessageId"),
            "the row should come through: {message:?}"
        ),
        (None, None) => {}
        (raw, message) => {
            return Err(format!(
                "the row and its parse disagree: {raw:?} against {message:?}"
            ));
        }
    }
    Ok(())
}

fn settled_form(frame: &Value) -> Check {
    let spec = UserFormSpec::from_custom_event(frame).ok_or("the settled card did not parse")?;
    must!(
        spec.entry_id == str_at(frame, "entryId") && spec.call_id == str_at(frame, "callId"),
        "the card should keep its entryId and callId: {spec:?}"
    );
    let word = str_at(frame, "formResolution");
    let resolution = FormResolution::parse(word);
    if resolution == FormResolution::Escalated {
        must!(
            spec.computer_handoff == Some(ComputerHandoffStatus::ActionNeeded),
            "escalated is a live Computer card beside the form: {spec:?}"
        );
    } else {
        must!(
            spec.effective_resolution() == Some(resolution),
            "the card should settle as {word:?}: {spec:?}"
        );
    }
    let settled = spec.effective_resolution().is_some();
    let expected = if settled {
        ActivityTick::Clear
    } else {
        label(WAITING_FOR_YOU)
    };
    must!(
        tick(frame) == expected,
        "a settled card clears the status line, got {:?}",
        tick(frame)
    );
    let (_, parts) = assembled(&[frame]).snapshot();
    let painted = |card: &UserFormSpec| {
        card.entry_id == spec.entry_id && card.effective_resolution() == spec.effective_resolution()
    };
    must!(
        parts
            .iter()
            .any(|part| matches!(part, ChatPart::UserForm(card) if painted(card))),
        "the settled card should paint: {parts:?}"
    );
    Ok(())
}

fn offer_save(frame: &Value) -> Check {
    let spec = SaveLoginSpec::from_event(frame).ok_or("the save offer did not parse")?;
    let value = &frame["value"];
    must!(
        spec.origin == str_at(value, "origin")
            && spec.username == str_at(value, "username")
            && spec.form_entry_id == str_at(value, "formEntryId"),
        "the offer should name the site, the account and its form: {spec:?}"
    );
    let (_, parts) = assembled(&[frame]).snapshot();
    must!(
        parts.contains(&ChatPart::SaveLogin(spec.clone())),
        "the offer should paint: {parts:?}"
    );
    Ok(())
}

// ---- what each REST body is read as ----

/// A route's fixtures, by the directory #255 files them under, and how this app reads them.
type RestCheck = fn(u16, &Value) -> Check;

const REST_ROUTES: &[(&str, RestCheck)] = &[
    ("GET__ag-ui_threads", thread_list),
    ("GET__ag-ui_threads__thread_id_", thread_replay),
    ("GET__ag-ui_approvals", approvals),
    ("POST__ag-ui_runs__run_id__answer", answer),
    ("GET__local-exec_policy", local_exec_policy),
    ("GET__coworkers", coworkers),
    ("POST__coworkers", hired),
    ("GET__recipes", recipes),
    ("GET__recipes__id_", recipe_detail),
    ("POST__recipes__id__run", recipe_run),
    ("GET__schedules", schedules),
    ("GET__schedules__id__runs", schedule_runs),
    ("POST__schedules__id__run", schedule_run_started),
    ("PATCH__schedules__id_", schedule_edited),
    ("GET__skills", skills),
    ("GET__skills__id_", skill_detail),
    ("GET__account", account),
    ("POST__auth_login", login),
    ("POST__ag-ui_user-form_submit", user_form_answer),
    ("POST__ag-ui_user-form_dismiss", user_form_answer),
    ("POST__ag-ui_box-handoff_resolve", box_handoff),
];

fn parse<T: DeserializeOwned>(body: &Value) -> Result<T, String> {
    serde_json::from_value(body.clone())
        .map_err(|error| format!("does not parse as {}: {error}", std::any::type_name::<T>()))
}

fn rows(body: &Value) -> Result<&Vec<Value>, String> {
    body.as_array()
        .ok_or_else(|| "a list route answered something other than an array".to_string())
}

fn same_len<T>(parsed: &[T], raw: &[Value]) -> Check {
    must!(
        parsed.len() == raw.len(),
        "{} rows parsed out of {}",
        parsed.len(),
        raw.len()
    );
    Ok(())
}

fn opt_str<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn thread_list(_: u16, body: &Value) -> Check {
    let listed: Vec<ThreadListing> = parse(body)?;
    let raw = rows(body)?;
    same_len(&listed, raw)?;
    for (row, raw) in listed.iter().zip(raw) {
        must!(
            row.thread_id == str_at(raw, "threadId")
                && row.coworker_id.as_deref() == opt_str(raw, "coworkerId")
                && row.origin == str_at(raw, "origin")
                && row.title.as_deref() == opt_str(raw, "title")
                && row.last_run_id == str_at(raw, "lastRunId")
                && row.last_status == str_at(raw, "lastStatus")
                && Some(row.updated_at_ms) == raw.get("updatedAtMs").and_then(Value::as_i64),
            "a thread row came through changed: {row:?}"
        );
    }
    Ok(())
}

fn thread_replay(_: u16, body: &Value) -> Check {
    let replay: ThreadReplay = parse(body)?;
    must!(
        replay.thread_id == str_at(body, "threadId"),
        "the thread id changed"
    );
    let raw_runs = body
        .get("runs")
        .and_then(Value::as_array)
        .ok_or("a replay with no runs array")?;
    same_len(&replay.runs, raw_runs)?;
    for (run, raw) in replay.runs.iter().zip(raw_runs) {
        let status = str_at(raw, "status");
        must!(
            run.run_id == str_at(raw, "runId")
                && run.status == status
                && Some(run.started_at_ms) == raw.get("startedAtMs").and_then(Value::as_i64)
                && Some(run.updated_at_ms) == raw.get("updatedAtMs").and_then(Value::as_i64)
                && run.failure.as_deref() == opt_str(raw, "failure")
                && run.is_live() == matches!(status, "running" | "awaiting-approval"),
            "a run came through changed: {:?} {status}",
            run.run_id
        );
        // The frames read back to the words the coworker said, as the transcript rebuilds them.
        let persons: BTreeSet<&str> = run
            .events
            .iter()
            .filter(|frame| {
                str_at(frame, "type") == "TEXT_MESSAGE_START" && str_at(frame, "role") == "user"
            })
            .map(|frame| str_at(frame, "messageId"))
            .collect();
        let said: String = run
            .events
            .iter()
            .filter(|frame| str_at(frame, "type") == "TEXT_MESSAGE_CONTENT")
            .filter(|frame| !persons.contains(str_at(frame, "messageId")))
            .map(|frame| str_at(frame, "delta"))
            .collect();
        let mut assembler = TurnAssembler::default();
        for frame in &run.events {
            assembler.push_event(frame);
        }
        assembler.finish();
        let (plain, _) = assembler.snapshot();
        // Blank lines are the transcript's own, put between the words a card or a picture splits.
        // Compared word by word: a space lost between two deltas joins two words and fails.
        let words =
            |text: &str| -> Vec<String> { text.split_whitespace().map(str::to_string).collect() };
        must!(
            words(&plain) == words(&said),
            "run {:?} should read {said:?}, not {plain:?}",
            run.run_id
        );
    }
    let hidden: Vec<&str> = body
        .get("hiddenRunIds")
        .and_then(Value::as_array)
        .map(|ids| ids.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    must!(
        replay.hidden_run_ids == hidden,
        "the hidden runs should come through: {:?}",
        replay.hidden_run_ids
    );
    if let Some(events) = body.get("pendingEvents").and_then(Value::as_array) {
        let live = replay
            .live_pending_messages()
            .ok_or("pendingEvents is there, so the live queue is known")?;
        must!(
            live.len() == events.len(),
            "every pending snapshot should be a live row: {live:?}"
        );
    }
    Ok(())
}

fn approvals(_: u16, body: &Value) -> Check {
    let queue: Vec<QueuedApproval> = parse(body)?;
    let raw = rows(body)?;
    same_len(&queue, raw)?;
    for (item, raw) in queue.iter().zip(raw) {
        must!(
            item.run_id == str_at(raw, "runId")
                && item.thread_id == str_at(raw, "threadId")
                && item.call_id == str_at(raw, "callId")
                && item.tool == str_at(raw, "tool")
                && Some(&item.arguments) == raw.get("arguments")
                && item.reason.as_deref() == opt_str(raw, "reason")
                && item.why.as_deref() == opt_str(raw, "why"),
            "a queued card came through changed: {item:?}"
        );
    }
    Ok(())
}

fn answer(_: u16, body: &Value) -> Check {
    let reply: AnswerReply = parse(body)?;
    let flag = |key: &str| body.get(key).and_then(Value::as_bool).unwrap_or(false);
    must!(
        reply.already_answered == flag("alreadyAnswered") && reply.continuing == flag("continuing"),
        "the answer should say whether it was already answered and whether the run goes on: \
         {reply:?}"
    );
    Ok(())
}

fn local_exec_policy(_: u16, body: &Value) -> Check {
    let view: LocalExecPolicy = parse(body)?;
    must!(
        view.mode == str_at(body, "mode") && LocalExecMode::parse(&view.mode).is_some(),
        "the machine's mode should be a word this app knows, not {:?}",
        view.mode
    );
    must!(
        view.machine_id == str_at(body, "machineId"),
        "the listing should name its machine: {view:?}"
    );
    // The lists Settings → Computer shows (#93): each rule exactly as kept, and every inert
    // pattern is one of the allows, with the server's reason.
    let listed = |key: &str| -> Vec<String> {
        body.get(key)
            .and_then(Value::as_array)
            .map(|rows| {
                rows.iter()
                    .filter_map(|r| r.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    must!(
        view.allow == listed("allow") && view.deny == listed("deny"),
        "the rule lists should read as sent: {view:?}"
    );
    // This fixture is here to lock `inert` (#93, server #246): it must carry some, and they must
    // read back exactly, pattern and reason, so a parse that drops the field (it defaults to
    // empty) fails rather than agreeing with an empty list.
    let sent: Vec<(String, String)> = body
        .get("inert")
        .and_then(Value::as_array)
        .map(|rows| {
            rows.iter()
                .map(|row| {
                    (
                        str_at(row, "pattern").to_string(),
                        str_at(row, "reason").to_string(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();
    must!(
        !sent.is_empty(),
        "the policy fixture should carry inert rules to lock: {body}"
    );
    let read: Vec<(String, String)> = view
        .inert
        .iter()
        .map(|rule| (rule.pattern.clone(), rule.reason.clone()))
        .collect();
    must!(
        read == sent,
        "the inert rules should read as sent: {read:?} vs {sent:?}"
    );
    must!(
        view.inert
            .iter()
            .all(|rule| view.allow.contains(&rule.pattern)),
        "every inert rule should be one of the allows: {view:?}"
    );
    Ok(())
}

fn coworker_matches(coworker: &Coworker, raw: &Value) -> Check {
    must!(
        coworker.id == str_at(raw, "id")
            && coworker.name == str_at(raw, "name")
            && coworker.model == str_at(raw, "model")
            && coworker.role.as_deref() == opt_str(raw, "role")
            && coworker.title.as_deref() == opt_str(raw, "title")
            && coworker.avatar_shape.as_deref() == opt_str(raw, "avatarShape")
            && coworker.avatar_color.as_deref() == opt_str(raw, "avatarColor")
            && Some(coworker.updated_at_ms) == raw.get("updatedAtMs").and_then(Value::as_i64)
            && Some(coworker.hidden_from_sidebar)
                == raw.get("hiddenFromSidebar").and_then(Value::as_bool)
            && coworker.box_id.as_deref() == opt_str(raw, "boxId"),
        "a coworker came through changed: {coworker:?}"
    );
    Ok(())
}

fn coworkers(_: u16, body: &Value) -> Check {
    let roster: Vec<Coworker> = parse(body)?;
    let raw = rows(body)?;
    same_len(&roster, raw)?;
    roster
        .iter()
        .zip(raw)
        .try_for_each(|(coworker, raw)| coworker_matches(coworker, raw))
}

fn hired(_: u16, body: &Value) -> Check {
    coworker_matches(&parse(body)?, body)
}

fn parameter_kind(word: &str) -> RecipeParameterKind {
    match word {
        "number" => RecipeParameterKind::Number,
        "boolean" => RecipeParameterKind::Boolean,
        _ => RecipeParameterKind::Text,
    }
}

fn recipes(_: u16, body: &Value) -> Check {
    let list: RecipeList = parse(body)?;
    let raw = body
        .get("recipes")
        .and_then(Value::as_array)
        .ok_or("the rows sit under recipes")?;
    same_len(&list.recipes, raw)?;
    for (recipe, raw) in list.recipes.iter().zip(raw) {
        let parameters = raw
            .get("parameters")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        must!(
            recipe.id == str_at(raw, "id")
                && recipe.name == str_at(raw, "name")
                && recipe.is_workflow() == (str_at(raw, "kind") == "workflow")
                && recipe.is_mine() == (str_at(raw, "relation") == "mine")
                && Some(u64::from(recipe.latest_version))
                    == raw.get("latestVersion").and_then(Value::as_u64)
                && recipe.parameters.len() == parameters.len(),
            "a recipe came through changed: {recipe:?}"
        );
        for (parameter, raw) in recipe.parameters.iter().zip(&parameters) {
            must!(
                parameter.name == str_at(raw, "name")
                    && Some(parameter.required) == raw.get("required").and_then(Value::as_bool)
                    && parameter.kind == parameter_kind(str_at(raw, "kind")),
                "a parameter came through changed: {parameter:?}"
            );
        }
    }
    Ok(())
}

fn recipe_detail(_: u16, body: &Value) -> Check {
    let detail: RecipeDetail = parse(body)?;
    must!(
        detail.recipe.id == str_at(&body["recipe"], "id"),
        "the recipe changed"
    );
    let versions = body["versions"].as_array().ok_or("no versions array")?;
    same_len(&detail.versions, versions)?;
    for (version, raw) in detail.versions.iter().zip(versions) {
        let steps = raw
            .pointer("/body/steps")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        must!(
            Some(u64::from(version.version)) == raw.get("version").and_then(Value::as_u64)
                && version.kind == str_at(raw, "kind")
                && version.body.steps.len() == steps,
            "a version came through changed: {version:?}"
        );
    }
    let runs = body["runs"].as_array().ok_or("no runs array")?;
    same_len(&detail.runs, runs)?;
    for (run, raw) in detail.runs.iter().zip(runs) {
        let state = str_at(raw, "state");
        must!(
            run.id == str_at(raw, "id")
                && run.state == state
                && run.is_running() == (state == "running")
                && Some(run.ok) == raw.get("ok").and_then(Value::as_bool),
            "a run came through changed: {run:?}"
        );
    }
    let bots = body["myBots"].as_array().ok_or("no myBots array")?;
    same_len(&detail.my_bots, bots)?;
    for (bot, raw) in detail.my_bots.iter().zip(bots) {
        must!(
            detail.bot_name(&bot.id) == str_at(raw, "name"),
            "a bot's name changed: {bot:?}"
        );
    }
    Ok(())
}

fn recipe_run(status: u16, body: &Value) -> Check {
    match status {
        202 => {
            let accepted: AsyncRunResponse = parse(body)?;
            must!(
                accepted.run_id == str_at(body, "runId"),
                "the run id should come through"
            );
        }
        200 => {
            let result: RecipeRunResult = parse(body)?;
            must!(
                Some(result.ok) == body.get("ok").and_then(Value::as_bool),
                "the outcome should come through: {result:?}"
            );
        }
        other => return Err(format!("no reading for a {other} from a run")),
    }
    Ok(())
}

/// Every line of a routine's history comes through, and every word in it is one this app has
/// a name for: a cause or a status read as `Other` is the server saying something the editor
/// cannot say back.
fn schedule_runs(_: u16, body: &Value) -> Check {
    let listed: Vec<ScheduleRun> = parse(body)?;
    let raw = rows(body)?;
    same_len(&listed, raw)?;
    for (run, raw) in listed.iter().zip(raw) {
        must!(
            run.run_id == str_at(raw, "runId")
                && Some(run.started_at_ms) == raw.get("startedAtMs").and_then(Value::as_i64)
                && run.ended_at_ms == raw.get("endedAtMs").and_then(Value::as_i64),
            "a run came through changed: {run:?}"
        );
        must!(
            run.cause != RunCause::Other && run.status != ScheduleRunStatus::Other,
            "a word this app has no name for: {raw}"
        );
    }
    Ok(())
}

fn schedule_run_started(_: u16, body: &Value) -> Check {
    let started: ScheduleRunStarted = parse(body)?;
    must!(
        started.run_id == str_at(body, "runId") && !started.run_id.is_empty(),
        "the run id should come through: {started:?}"
    );
    Ok(())
}

/// An edit answers with the routine as it now is, which is what the editor draws next.
fn schedule_edited(status: u16, body: &Value) -> Check {
    schedules(status, &Value::Array(vec![body.clone()]))
}

fn schedules(_: u16, body: &Value) -> Check {
    let listed: Vec<ScheduleRow> = parse(body)?;
    let raw = rows(body)?;
    same_len(&listed, raw)?;
    for (row, raw) in listed.iter().zip(raw) {
        let webhook = raw.get("webhook").filter(|webhook| !webhook.is_null());
        let kind = if str_at(raw, "kind") == "webhook" {
            ScheduleKind::Webhook
        } else {
            ScheduleKind::Cron
        };
        must!(
            row.id == str_at(raw, "id")
                && row.coworker_id == str_at(raw, "coworkerId")
                && row.kind == kind
                && row.cron.as_deref() == opt_str(raw, "cron")
                && row.prompt == str_at(raw, "prompt")
                && row.name.as_deref() == opt_str(raw, "name")
                && Some(row.active) == raw.get("active").and_then(Value::as_bool)
                && row.next_due_ms == raw.get("nextDueMs").and_then(Value::as_i64)
                && row.webhook.is_some() == webhook.is_some(),
            "a routine came through changed: {row:?}"
        );
        if let (Some(info), Some(raw)) = (&row.webhook, webhook) {
            must!(
                info.url == str_at(raw, "url")
                    && info.key == str_at(raw, "key")
                    && info.header == str_at(raw, "header"),
                "the webhook's url, key and header should come through: {info:?}"
            );
        }
    }
    Ok(())
}

fn skill_matches(skill: &SkillSummary, raw: &Value) -> Check {
    must!(
        skill.id == str_at(raw, "id")
            && skill.name == str_at(raw, "name")
            && skill.description == str_at(raw, "description")
            && skill.source.word() == str_at(raw, "source")
            && Some(u64::from(skill.version_count))
                == raw.get("versionCount").and_then(Value::as_u64)
            && Some(skill.draft) == raw.get("draft").and_then(Value::as_bool)
            && Some(skill.enabled) == raw.get("enabled").and_then(Value::as_bool)
            && skill.approved_at_ms == raw.get("approvedAtMs").and_then(Value::as_i64),
        "a skill came through changed: {skill:?}"
    );
    Ok(())
}

fn skills(_: u16, body: &Value) -> Check {
    let listed: Vec<SkillSummary> = parse(body)?;
    let raw = rows(body)?;
    same_len(&listed, raw)?;
    listed
        .iter()
        .zip(raw)
        .try_for_each(|(skill, raw)| skill_matches(skill, raw))
}

fn skill_detail(_: u16, body: &Value) -> Check {
    let detail: SkillDetail = parse(body)?;
    skill_matches(&detail.skill, body)?;
    let files = body["files"].as_array().map_or(0, Vec::len);
    must!(
        detail.body == str_at(body, "body")
            && Some(u64::from(detail.version)) == body.get("version").and_then(Value::as_u64)
            && detail.files.len() == files,
        "the prose, its version and its files should come through: {detail:?}"
    );
    Ok(())
}

fn account(_: u16, body: &Value) -> Check {
    let me: Account = parse(body)?;
    must!(
        me.id == str_at(body, "id")
            && me.email == str_at(body, "email")
            && me.first_name == str_at(body, "firstName")
            && me.last_name == str_at(body, "lastName")
            && me.org_id.as_deref() == opt_str(body, "orgId")
            && Some(me.verified) == body.get("verified").and_then(Value::as_bool)
            && Some(me.enabled) == body.get("enabled").and_then(Value::as_bool)
            && me.is_admin == body.get("isAdmin").and_then(Value::as_bool),
        "the account came through changed: {me:?}"
    );
    Ok(())
}

/// A sign-in answers with the session in cookies, and this app reads only the status off a
/// success. A refusal is read for the server's own sentence.
fn login(status: u16, body: &Value) -> Check {
    if status == 200 {
        for token in [
            "accessToken",
            "refreshToken",
            "access_token",
            "refresh_token",
        ] {
            must!(
                body.get(token).is_none(),
                "a sign-in body should not carry {token}; the session is in the cookies"
            );
        }
        return Ok(());
    }
    let text = match body {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    let said = error_message_from_body(&text);
    let expected = opt_str(body, "error").or(body.as_str()).unwrap_or("");
    must!(
        said == expected,
        "a refused sign-in should read {expected:?}, not {said:?}"
    );
    Ok(())
}

fn user_form_answer(status: u16, body: &Value) -> Check {
    let reply = user_form_action_from_http(status, body);
    match status {
        200 => {
            let UserFormActionReply::Settled(spec) = &reply else {
                return Err(format!(
                    "a settled card should read as settled, not {reply:?}"
                ));
            };
            let entry = opt_str(body, "entryId").unwrap_or_else(|| str_at(body, "id"));
            must!(
                spec.entry_id == entry,
                "the card should keep its id: {spec:?}"
            );
            let word = str_at(body, "formResolution");
            if FormResolution::parse(word) == FormResolution::Escalated {
                must!(
                    spec.computer_handoff == Some(ComputerHandoffStatus::ActionNeeded)
                        && spec.handoff_entry_id.as_deref() == opt_str(body, "handoffEntryId"),
                    "Open the screen should bring back the Computer card's id: {spec:?}"
                );
            } else {
                must!(
                    spec.effective_resolution() == Some(FormResolution::parse(word)),
                    "the card should settle as {word:?}: {spec:?}"
                );
            }
        }
        404 => {
            let expected = if str_at(body, "error").eq_ignore_ascii_case(FORM_ENTRY_MISSING) {
                UserFormActionReply::MissingEntry
            } else {
                UserFormActionReply::MissingRoute
            };
            must!(
                reply == expected,
                "a 404 should read as {expected:?}, not {reply:?}"
            );
        }
        403 => {
            let said = opt_str(body, "message")
                .or_else(|| opt_str(body, "error"))
                .unwrap_or_default();
            must!(
                reply == UserFormActionReply::Refused(said.to_string()),
                "a refusal should carry the server's sentence, not {reply:?}"
            );
        }
        other => return Err(format!("no reading for a {other} from a card")),
    }
    Ok(())
}

fn box_handoff(status: u16, body: &Value) -> Check {
    let reply = box_handoff_action_from_http(status, body);
    let expected = if body.get("alreadyAnswered").and_then(Value::as_bool) == Some(true) {
        BoxHandoffReply::AlreadyAnswered
    } else {
        BoxHandoffReply::Settled
    };
    must!(
        status == 200 && reply == expected,
        "a hand-back should read as {expected:?}, not {reply:?}"
    );
    Ok(())
}

// ---- the tests ----

/// The manifest and the files agree: every file has an entry naming where it came from, and
/// every entry has its file.
#[test]
fn the_manifest_names_every_fixture_and_every_fixture_is_in_it() {
    let corpus = Corpus::load();
    let manifest = &corpus.manifest;
    assert!(
        manifest.server_sha.len() == 40
            && manifest.server_sha.chars().all(|c| c.is_ascii_hexdigit()),
        "server_sha should be the opengrok-server commit, not {:?}",
        manifest.server_sha
    );
    assert!(
        ["hand-copied", "opengrok-server recorder"].contains(&manifest.recorded_by.as_str()),
        "recorded_by {:?} is neither the hand copy nor the recorder",
        manifest.recorded_by
    );
    let files: BTreeSet<&str> = corpus
        .frames
        .keys()
        .chain(corpus.bodies.keys())
        .map(String::as_str)
        .collect();
    let listed: BTreeSet<&str> = manifest
        .entries
        .iter()
        .map(|entry| entry.file.as_str())
        .collect();
    let unlisted: Vec<_> = files.difference(&listed).collect();
    let missing: Vec<_> = listed.difference(&files).collect();
    assert!(
        unlisted.is_empty() && missing.is_empty(),
        "files with no manifest entry: {unlisted:?}; entries with no file: {missing:?}"
    );
    let unsourced: Vec<_> = manifest
        .entries
        .iter()
        .filter(|entry| entry.source.trim().is_empty())
        .map(|entry| entry.file.as_str())
        .collect();
    assert!(
        unsourced.is_empty(),
        "every fixture names the server file it came from: {unsourced:?}"
    );
    assert!(
        corpus.root.join("MANIFEST.json").is_file(),
        "the manifest sits at the corpus root"
    );
    for (file, symptom, why) in KNOWN_DRIFT {
        assert!(
            files.contains(file) && !symptom.trim().is_empty() && !why.trim().is_empty(),
            "KNOWN_DRIFT names {file}, which is not in the corpus or says nothing about it"
        );
    }
}

/// #255's layout: a frame sits under its own `type`, a CUSTOM under its `name`, and a body under
/// its method and route with its status in the file name.
#[test]
fn every_fixture_sits_where_its_layout_says() {
    let corpus = Corpus::load();
    let mut problems = Vec::new();
    for (file, frame) in &corpus.frames {
        let kind = str_at(frame, "type");
        let expected = if kind == "CUSTOM" {
            format!("agui/custom/{}/", str_at(frame, "name"))
        } else {
            format!("agui/{kind}/")
        };
        if !file.starts_with(&expected) {
            problems.push(format!("{file} belongs under {expected}"));
        }
    }
    for (file, fixture) in &corpus.bodies {
        let method = str_at(fixture, "method");
        let status = fixture.get("status").and_then(Value::as_u64).unwrap_or(0);
        let mut parts = file.split('/').skip(1);
        let route = parts.next().unwrap_or("");
        let name = parts.next().unwrap_or("");
        if !route.starts_with(&format!("{method}_"))
            || !name.starts_with(&format!("{status}-"))
            || str_at(fixture, "path").is_empty()
            || fixture.get("body").is_none()
        {
            problems.push(format!(
                "{file} is not {{method, path, status, body}} filed as \
                 rest/{method}_<route>/{status}-<slug>.json"
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Every frame goes through the code that handles its type and name, and comes out the way this
/// app means to read it.
#[test]
fn every_frame_the_server_sends_is_read_as_intended() {
    let corpus = Corpus::load();
    let problems = verdicts(corpus.frames.iter(), |_, frame| check_frame(&corpus, frame));
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Every body parses with the type this app reads its route with, and the fields it reads come
/// through as the server sent them.
#[test]
fn every_body_parses_with_the_type_this_app_reads_it_with() {
    let corpus = Corpus::load();
    let problems = verdicts(corpus.bodies.iter(), |file, fixture| {
        let route = file.split('/').nth(1).unwrap_or("");
        let (_, check) = REST_ROUTES
            .iter()
            .find(|(dir, _)| *dir == route)
            .ok_or_else(|| format!("no reading for {route}: add it to REST_ROUTES"))?;
        let status = fixture
            .get("status")
            .and_then(Value::as_u64)
            .and_then(|status| u16::try_from(status).ok())
            .ok_or("no status")?;
        check(status, &fixture["body"])
    });
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Every type and name the server sends has a fixture, or the manifest lists it as unrecorded:
/// no test exercises it yet.
#[test]
fn every_type_and_name_the_server_sends_has_a_fixture() {
    let corpus = Corpus::load();
    let emits = &corpus.manifest.emits;
    let mut missing = Vec::new();
    for kind in &emits.agui_types {
        if corpus.frames_of(kind).next().is_none() && !corpus.manifest.unrecorded.contains(kind) {
            missing.push(kind.as_str());
        }
    }
    for name in &emits.custom_names {
        if corpus.customs_named(name).next().is_none() && !corpus.manifest.unrecorded.contains(name)
        {
            missing.push(name.as_str());
        }
    }
    // The words inside frames and bodies too: every approval reason, and every formResolution,
    // appears in some fixture. `dismissed` is also what an unknown word parses as, so only a
    // fixture carrying it can show it is read rather than defaulted.
    let mut carried = BTreeSet::new();
    for value in corpus.every_value() {
        collect_words(value, &mut carried);
    }
    for (slot, key) in [
        (Slot::ApprovalReason, "reason"),
        (Slot::FormResolution, "formResolution"),
    ] {
        for word in emits.words(slot) {
            let seen = carried.contains(&(key.to_string(), word.clone()));
            if !seen && !corpus.manifest.unrecorded.contains(word) {
                missing.push(word.as_str());
            }
        }
    }
    assert!(missing.is_empty(), "sent, with no fixture: {missing:?}");
}

/// Every `(key, string value)` pair anywhere in `value`.
fn collect_words(value: &Value, out: &mut BTreeSet<(String, String)>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                if let Value::String(text) = inner {
                    out.insert((key.clone(), text.clone()));
                }
                collect_words(inner, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|item| collect_words(item, out)),
        _ => {}
    }
}

/// Every word this app matches on is one the server sends, or is excused with the evidence that
/// it does not. A word in both places is an excuse that has gone stale.
#[test]
fn every_word_this_app_matches_is_sent_or_excused() {
    let corpus = Corpus::load();
    let emits = &corpus.manifest.emits;
    let ledger = ledger();
    let mut problems = Vec::new();
    for (slot, word) in &ledger {
        let sent = emits.words(*slot).iter().any(|sent| sent.as_str() == *word);
        let excused = is_excused(NOT_SENT_BY_SERVER, *slot, word);
        if sent && excused {
            problems.push(format!(
                "{} {word:?} is sent now: take it off NOT_SENT_BY_SERVER",
                slot.field()
            ));
        } else if !sent && !excused {
            problems.push(format!(
                "{} {word:?} is matched here and the server does not send it: find out why, and \
                 say so in NOT_SENT_BY_SERVER",
                slot.field()
            ));
        }
    }
    let matched: BTreeSet<&str> = ledger.iter().map(|(_, word)| *word).collect();
    for (_, word, why) in NOT_SENT_BY_SERVER {
        if !matched.contains(word) {
            problems.push(format!(
                "NOT_SENT_BY_SERVER excuses {word:?}, which nothing here matches"
            ));
        }
        if why.trim().is_empty() {
            problems.push(format!("NOT_SENT_BY_SERVER gives no evidence for {word:?}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Every word the server sends is one this app matches, or is excused with what happens to it
/// instead, so a new name from the server fails here first.
#[test]
fn every_word_the_server_sends_is_matched_or_excused() {
    let corpus = Corpus::load();
    let emits = &corpus.manifest.emits;
    let ledger = ledger();
    let mut problems = Vec::new();
    for slot in Slot::ALL {
        for word in emits.words(slot) {
            let matched = ledger
                .iter()
                .any(|(matched_slot, matched)| *matched_slot == slot && *matched == word.as_str());
            let ignored = is_excused(CLIENT_IGNORES, slot, word);
            if matched && ignored {
                problems.push(format!(
                    "{} {word:?} is matched now: take it off CLIENT_IGNORES",
                    slot.field()
                ));
            } else if !matched && !ignored {
                problems.push(format!(
                    "the server sends the {} {word:?} and nothing here reads it: read it, or \
                     say in CLIENT_IGNORES why it can be left",
                    slot.field()
                ));
            }
        }
    }
    let sent: BTreeSet<&str> = Slot::ALL
        .iter()
        .flat_map(|slot| emits.words(*slot).iter().map(String::as_str))
        .collect();
    for (_, word, why) in CLIENT_IGNORES {
        if !sent.contains(word) {
            problems.push(format!(
                "CLIENT_IGNORES lists {word:?}, which the server does not send"
            ));
        }
        if why.trim().is_empty() {
            problems.push(format!("CLIENT_IGNORES gives no reason for {word:?}"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// The part of a source file that ships: everything before its `#[cfg(test)] mod`.
fn shipped_lines(source: &str) -> Vec<&str> {
    let lines: Vec<&str> = source.lines().collect();
    let mut kept = Vec::new();
    for (at, line) in lines.iter().enumerate() {
        if *line == "#[cfg(test)]" {
            let next = lines[at + 1..]
                .iter()
                .find(|line| !line.starts_with("#["))
                .copied()
                .unwrap_or("");
            if next.starts_with("mod ") {
                break;
            }
        }
        if !line.trim_start().starts_with("//") {
            kept.push(*line);
        }
    }
    kept
}

fn rust_sources(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|error| panic!("read {}: {error}", dir.display()));
    for entry in entries {
        let path = entry.expect("a directory entry").path();
        if path.is_dir() {
            rust_sources(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// [`AGUI_TYPES`] is exactly the set of AG-UI types the shipped source matches on, found by
/// reading every source file for the protocol's type names. A new match arm, or one taken away,
/// fails here until the ledger says the same.
#[test]
fn the_ledger_types_are_the_types_the_source_matches() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_sources(&src, &mut files);
    let this_file = Path::new("opengrok").join("conformance.rs");
    let mut found = BTreeSet::new();
    for file in files.iter().filter(|file| !file.ends_with(&this_file)) {
        let source = std::fs::read_to_string(file)
            .unwrap_or_else(|error| panic!("read {}: {error}", file.display()));
        for line in shipped_lines(&source) {
            for kind in AGUI_SPEC_TYPES {
                if line.contains(&format!("\"{kind}\"")) {
                    found.insert(*kind);
                }
            }
        }
    }
    let listed: BTreeSet<&str> = AGUI_TYPES.iter().copied().collect();
    let unlisted: Vec<_> = found.difference(&listed).collect();
    let unmatched: Vec<_> = listed.difference(&found).collect();
    assert!(
        unlisted.is_empty() && unmatched.is_empty(),
        "matched in the source and missing from AGUI_TYPES: {unlisted:?}; in AGUI_TYPES and \
         matched nowhere: {unmatched:?}"
    );
}
