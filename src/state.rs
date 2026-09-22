use crate::actions::TtsSource;
use crate::audio::AudioInput;
use crate::chrome::{
    ResponsiveCollapse, SIDEBAR_EXPANDED, SidebarChrome, collapse_for_width, remember_choice,
    sidebar_from_resize,
};
use crate::config::Config;
/// The routine editor's schedule and the cron line it becomes. Re-exported because every
/// caller reads it as part of a routine, and a routine is a thing on `AppState`.
pub use crate::cron_spec::{
    ScheduleDayKind, ScheduleNotCron, ScheduleSpec, ScheduleUiMode, ScheduleUnit,
};
use crate::opengrok::{
    Account, ActivityTick, AguiMessage, ApprovalSpec, BotActivity, BoxHandoffReply,
    BoxHandoffResolution, BoxShareScope, ChatPart, ComputerHandoffStatus, ConnectedComputer,
    Coworker, CoworkerComputer, CoworkerPatch, Failure, FormResolution, FormSpec, ImageVisibility,
    LocalExecMode, LocalExecResolution, ModelCatalogue, NewSchedule, NewSkill, OpenGrokClient,
    OpenGrokError, ProfileUpdate, QueuedApproval, RecipeDetail, RecipeKind, RecipeParameter,
    RecipeRunResult, RecipeShareTarget, RecipeStep, RecipeSummary, ReplyQuote, RunReplay,
    SKILL_BODY_CHARS, SKILL_BUNDLE_FILES, SKILL_BUNDLE_LIMIT, SaveLoginSpec, ScheduleKind,
    ScheduleRow, ScreenshotSpec, SkillDetail, SkillFile, SkillSource, SkillSummary, ThreadReplay,
    ThreadRun, ToolCallTracker, TurnAssembler, TurnRecipe, USER_FORM_SERVER_FILL_AVAILABLE,
    Unreachable, UserFormDismissMode, UserFormHttpSettle, UserFormValues, UserFormVerb,
    WAITING_FOR_YOU, activity_from_replay, box_handoff_resolve_entry_id, collapse_computer_roster,
    command_from_args, command_from_replay_events, deeds_from_replay, enrol_this_machine,
    env_egress_tunnel_enabled, host_egress_tunnel_available, host_egress_tunnel_flag,
    keep_local_save_offer, place_hitl_cards_in_document_order, policy_answer,
    reads_as_gateway_unreachable, save_login_from_local, serve_local_exec, stored_machine_id,
    tool_standin,
};
use crate::reachability::Reachability;
use crate::send_policy::{Busy, OnSend, SendPlan, plan_send};
use crate::services::database::{ChatMessage, DatabaseService, MessagePart, ReplyRef};
use crate::services::tts_service::TtsService;
use crate::session::Session;
use crate::site_login::{
    CardTarget, KIND_PASSWORD, PendingSave, SavedLoginUse, SiteLoginRecord, SiteLoginVault,
    card_target, login_fields, login_matches_request, login_origin, origins_match,
    registrable_origin, save_candidate,
};
use crate::threads::conversation_for_thread;
use chrono::{DateTime, Local, NaiveDateTime, Timelike};
use gpui_kit::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::time::{Duration, Instant, SystemTime};

#[derive(Clone, Debug)]
pub struct Message {
    pub id: String,
    pub sender: String,
    pub content: String,
    pub sent_at: SystemTime,
    pub is_me: bool,
    pub reply_preview: Option<String>,
    /// The message this one answers. The preview is what the bubble paints; this is what the
    /// quote sent to the coworker is built from, so a reply reaches it as more than a bubble.
    pub reply_to_id: Option<String>,
    /// The person wrote the quoted message, rather than the coworker.
    pub reply_is_me: bool,
    pub parts: Vec<ChatPart>,
    /// The run this reply came out of, for a reply that came out of one. It is how the thread
    /// knows which of the server's runs it can already account for, so reconciling against the
    /// server adds what is missing instead of saying everything a second time.
    pub run_id: Option<String>,
    /// The person hid this message. It stays in the thread, unpainted, because it is the only
    /// thing that still names its run — take it out and the thread fetches that run back from
    /// the server on the next visit, which is the person watching what they deleted return.
    pub hidden: bool,
}

impl Message {
    pub fn has_visible_body(&self) -> bool {
        !self.content.trim().is_empty()
            || self.parts.iter().any(|part| match part {
                ChatPart::Text(text) => !text.trim().is_empty(),
                ChatPart::Ui(_)
                | ChatPart::Approval(_)
                | ChatPart::Screenshot(_)
                | ChatPart::UserForm(_)
                | ChatPart::SaveLogin(_) => true,
            })
    }

    /// Words, as opposed to a card or a picture. A run that ends without any is a turn the
    /// transcript has to speak for: with the error, or with what the tools did.
    pub fn has_text_body(&self) -> bool {
        !self.content.trim().is_empty()
            || self.parts.iter().any(|part| match part {
                ChatPart::Text(text) => !text.trim().is_empty(),
                ChatPart::Ui(_)
                | ChatPart::Approval(_)
                | ChatPart::Screenshot(_)
                | ChatPart::UserForm(_)
                | ChatPart::SaveLogin(_) => false,
            })
    }

    /// Clock time on the message row, matching Grok's `12:14 PM` column.
    pub fn formatted_time(&self) -> String {
        let dt = DateTime::<Local>::from(self.sent_at);
        let (pm, hour) = dt.hour12();
        let hour = if hour == 0 { 12 } else { hour };
        format!(
            "{}:{:02} {}",
            hour,
            dt.minute(),
            if pm { "PM" } else { "AM" }
        )
    }
}

/// What is worth keeping of a message the person watched arrive: its words.
///
/// A turn that was only words keeps nothing here — `content` already holds them, and a second
/// copy would double every thread on disk. Cards are left out on purpose; see `MessagePart`.
/// Pinned screenshots (`image.visibility` = `transcript` | `failure` | `end`, plus untagged
/// failure/turn-end pins) stay on disk so a bot switch still has the picture without waiting
/// on OpenGrok. `agent` shots never become [`ChatPart::Screenshot`]. User-form cards still
/// rehydrate from `formRequest` + sibling `formResolution`. Because a card is dropped, the
/// words on either side of one are kept apart by a blank line, the same break `content` gets,
/// rather than running together into one sentence. A chart, which is cut out of the middle of
/// a sentence, leaves that sentence whole.
fn saved_parts(parts: &[ChatPart]) -> Vec<MessagePart> {
    let mut saved: Vec<MessagePart> = Vec::new();
    let mut words = String::new();
    for part in parts {
        match part {
            ChatPart::Text(text) => words.push_str(text),
            ChatPart::Screenshot(spec)
                if matches!(spec.visibility, Some(ImageVisibility::Agent)) =>
            {
                break_paragraph(&mut words);
            }
            ChatPart::Screenshot(spec) => {
                close_text_run(&mut words, &mut saved);
                saved.push(MessagePart::Screenshot {
                    call_id: spec.call_id.clone(),
                    caption: spec.caption.clone(),
                    image: spec.image.bytes.clone(),
                    width: spec.width,
                    height: spec.height,
                });
            }
            ChatPart::Approval(_) | ChatPart::UserForm(_) | ChatPart::SaveLogin(_) => {
                break_paragraph(&mut words)
            }
            ChatPart::Ui(spec) => {
                close_text_run(&mut words, &mut saved);
                saved.push(MessagePart::Ui {
                    spec: spec.to_value().to_string(),
                });
            }
        }
    }
    close_text_run(&mut words, &mut saved);
    if saved
        .iter()
        .all(|part| matches!(part, MessagePart::Text(_)))
    {
        return Vec::new();
    }
    saved
}

/// The words so far become a bubble of their own. Whitespace is not a bubble, so a run of it is
/// dropped; the edges are trimmed because a run boundary is where one bubble ends and the next
/// begins, and a blank first line there is only noise.
fn close_text_run(words: &mut String, saved: &mut Vec<MessagePart>) {
    let text = std::mem::take(words);
    let text = text.trim();
    if !text.is_empty() {
        saved.push(MessagePart::Text(text.to_string()));
    }
}

fn break_paragraph(words: &mut String) {
    if words.trim().is_empty() || words.ends_with("\n\n") {
        return;
    }
    words.truncate(words.trim_end().len());
    words.push_str("\n\n");
}

/// A saved message as the feed draws it.
///
/// A row with no pieces — every row an older build wrote, and every turn that was only words —
/// is the single bubble its words already were, so old threads read exactly as they did.
fn restored_parts(content: &str, saved: Vec<MessagePart>) -> Vec<ChatPart> {
    if saved.is_empty() {
        if content.trim().is_empty() {
            return Vec::new();
        }
        return vec![ChatPart::Text(content.to_string())];
    }
    saved
        .into_iter()
        .filter_map(|part| match part {
            MessagePart::Text(text) => Some(ChatPart::Text(text)),
            // A widget this build cannot rebuild leaves the stand-in line to speak for it.
            MessagePart::Ui { spec } => serde_json::from_str::<serde_json::Value>(&spec)
                .ok()
                .and_then(|value| crate::opengrok::UiSpec::from_value(&value))
                .map(ChatPart::Ui),
            MessagePart::Screenshot {
                call_id,
                caption,
                image,
                width,
                height,
            } => Some(ChatPart::Screenshot(crate::opengrok::ScreenshotSpec {
                call_id,
                caption,
                image: Arc::new(gpui_kit::Image::from_bytes(
                    gpui_kit::ImageFormat::Png,
                    image,
                )),
                width,
                height,
                visibility: Some(ImageVisibility::Transcript),
            })),
        })
        .collect()
}

/// Fold OpenGrok-owned cards onto a sqlite row that dropped them.
///
/// User-form (idle + settled, never secrets) lives on the server as
/// `formRequest` + sibling `formResolution`. Pinned screenshots may already
/// be in sqlite; overlay still fills gaps. Save-login is origin+username
/// only — the password stays in host `pending_save`, never on the wire.
fn overlay_server_cards(message: &mut Message, replayed: &[ChatPart]) {
    for part in replayed {
        match part {
            ChatPart::UserForm(incoming) => {
                if let Some(existing) = message.parts.iter_mut().find_map(|part| match part {
                    ChatPart::UserForm(spec)
                        if spec.same_card(incoming) || spec.shares_call_id(&incoming.call_id) =>
                    {
                        Some(spec)
                    }
                    _ => None,
                }) {
                    existing.merge(incoming.clone());
                } else {
                    message.parts.push(ChatPart::UserForm(incoming.clone()));
                }
            }
            ChatPart::Screenshot(incoming) => {
                let already = message.parts.iter().any(|part| {
                    matches!(part, ChatPart::Screenshot(spec) if spec.call_id == incoming.call_id)
                });
                if !already {
                    message.parts.push(ChatPart::Screenshot(incoming.clone()));
                }
            }
            ChatPart::SaveLogin(incoming) => {
                let already = message.parts.iter().any(|part| {
                    matches!(
                        part,
                        ChatPart::SaveLogin(spec) if spec.form_entry_id == incoming.form_entry_id
                    )
                });
                if !already {
                    message.parts.push(ChatPart::SaveLogin(incoming.clone()));
                }
            }
            _ => {}
        }
    }
    place_hitl_cards_in_document_order(&mut message.parts);
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplyTo {
    pub message_id: String,
    pub preview: String,
    pub is_me: bool,
}

/// What the feed says when a run came back with nothing at all.
pub const EMPTY_TURN_NOTE: &str = "(OpenGrok returned no assistant text.)";

/// How a run's failure is spelled in the feed.
pub const RUN_ERROR_PREFIX: &str = "OpenGrok: ";

/// What stands where a library would be when nobody is signed in. Signed out there is nothing to
/// ask for, which is not the same as a library with nothing in it — and "No skills yet" is what
/// an empty one says.
pub const SKILLS_SIGNED_OUT: &str = "Sign in to see the skills kept on the server.";

/// What the feed says for a turn the person stopped.
///
/// Addressed to the person and about what they did, because that is whose doing it was. It is
/// not spelled with [`RUN_ERROR_PREFIX`] on purpose: a stop is not a run going wrong, it is a
/// run doing exactly what it was told, and a line painted in the colour of a failure would
/// leave someone looking for what broke.
pub const STOPPED_TURN_NOTE: &str = "You stopped this turn.";

/// What the feed says when the stop never reached the server.
///
/// The app cannot stop a run by ceasing to watch it — the run drives a box, and it goes on
/// opening pages and typing into them — so when the one thing that would have stopped it did
/// not land, saying nothing would leave the person believing a stop that never happened.
pub const STOP_UNSENT_NOTE: &str =
    "OpenGrok: the stop did not reach the server, so the turn may still be running.";

/// What the feed says for a turn that never got through.
///
/// It names no machine, on purpose. The line is a historical record — this turn, at this time,
/// did not happen — and which machine was out of reach at that moment may not be the one that is
/// out of reach by the time anybody reads it. What is down *now* is the indicator's job, and the
/// indicator is live where this line is not.
///
/// Like [`STOPPED_TURN_NOTE`] it is not spelled with [`RUN_ERROR_PREFIX`]: nothing went wrong
/// with the run, because there was no run. Red would send someone looking for a fault in a turn
/// that simply did not leave.
pub const TURN_UNREACHED_NOTE: &str = "This turn did not go through.";

/// What the feed says for a turn that was not sent because the app had no session.
///
/// Its own line rather than [`TURN_UNREACHED_NOTE`], because "did not go through" reads as the
/// wire and would send somebody to look at a network that is working perfectly. And not spelled
/// with [`RUN_ERROR_PREFIX`], for the same reason as the other two: the run is not what went
/// wrong — there was no run, and no verdict about one.
///
/// It says what happened and leaves what to do about it to the banner, which is live where this
/// line is a record of one moment. Like the unreached note, it is offered again: the turn never
/// left, so sending it after signing in does nothing twice.
pub const TURN_SIGNED_OUT_NOTE: &str = "This turn was not sent: the app is signed out.";

/// The working line of a turn that has stopped to ask for permission.
///
/// A constant because it is both written and read: a poll that outlives the card checks the
/// thread's own line for it before deciding the turn is over, and a spelling that drifted
/// between the two would end the turn out from under the person's answer.
const WAITING_APPROVAL_STATUS: &str = "Waiting for approval";

/// The working line while an unresolved user-form card is on screen. Distinct from
/// [`WAITING_APPROVAL_STATUS`]: that one is a yes/no on a command, this one is credentials
/// the bot must not see.
const WAITING_FOR_YOU_STATUS: &str = WAITING_FOR_YOU;

pub fn is_waiting_on_person(status: Option<&str>) -> bool {
    matches!(
        status,
        Some(WAITING_APPROVAL_STATUS) | Some(WAITING_FOR_YOU_STATUS)
    )
}

/// Footer chrome: the server waiting label as-is, else `{name} is working`.
pub fn bot_status_line(name: &str, label: &str) -> String {
    if is_waiting_on_person(Some(label)) {
        label.to_string()
    } else if label == crate::opengrok::WAKING_COMPUTER {
        format!("Waking {name}'s computer")
    } else {
        format!("{name} is working")
    }
}

/// A turn that never reached the server, in either of the two ways that happens.
///
/// Both rows mean the same thing about the transcript — nothing ran, nothing was decided, and
/// the turn is there to be sent again — so both are offered again and neither is ever saved.
/// They are two sentences rather than one because the thing to do about them differs, and the
/// sentence is the only place a person learns that.
pub fn is_unsent_turn_note(content: &str) -> bool {
    let text = content.trim();
    text == TURN_UNREACHED_NOTE || text == TURN_SIGNED_OUT_NOTE
}

/// How much of a quoted message the coworker is shown: a reply to a long answer names it, it
/// does not replay it. The server's `reply_context` caps the same way.
const REPLY_QUOTE_CHARS: usize = 600;

/// The app talking about a turn — the empty-turn note, a run's failure, a turn the person
/// stopped — rather than anything the coworker said. These are painted as a status line, never
/// saved and never sent back: a line the app wrote is not a turn the coworker took, and the
/// model would answer to it as if it were.
///
/// The test is the content itself so that rows an older build saved are read the same way.
pub fn is_status_line(content: &str) -> bool {
    let text = content.trim();
    text == EMPTY_TURN_NOTE
        || text == STOPPED_TURN_NOTE
        || is_unsent_turn_note(text)
        || text.starts_with(RUN_ERROR_PREFIX)
}

/// The stand-in a turn that acted but said nothing leaves behind, e.g.
/// "[took a screenshot of my screen]". It is the coworker's own content — saved, and sent on
/// later turns so it remembers what it did — but it is not speech, so the feed dims it.
pub fn is_tool_standin(content: &str) -> bool {
    let text = content.trim();
    text.starts_with('[') && text.ends_with(']') && !text.contains('\n')
}

/// The bracketed line the coworker reads ahead of a reply's own words, in the sentence the
/// server's `reply_context` already writes, so a reply reads the same whichever path carried it.
fn reply_quote_line(quote: &ReplyQuote) -> String {
    let who = if quote.is_me {
        "their own earlier message"
    } else {
        "your earlier message"
    };
    format!("[Replying to {who}: \"{}\"]", quote.preview)
}

/// The quote a message carries: the words of the message it answers when that one is still in the
/// thread — all of them, not the bubble's short preview — and the preview saved with the reply
/// when it is not.
fn reply_quote(messages: &[Message], message: &Message) -> Option<ReplyQuote> {
    let message_id = message.reply_to_id.clone()?;
    let quoted = messages.iter().find(|m| m.id == message_id);
    let text = match quoted {
        Some(quoted) => quoted.content.clone(),
        None => message.reply_preview.clone()?,
    };
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    let preview = if text.chars().count() > REPLY_QUOTE_CHARS {
        let head: String = text.chars().take(REPLY_QUOTE_CHARS).collect();
        format!("{head}…")
    } else {
        text.to_string()
    };
    Some(ReplyQuote {
        message_id,
        preview,
        is_me: quoted.map_or(message.reply_is_me, |quoted| quoted.is_me),
    })
}

/// The thread as the coworker should see it.
///
/// The app's own status lines are left out, and a reply carries its quote twice: in `content`,
/// because that is all today's server reads, and in `replyTo` for a server that would rather
/// find the quoted message and word the context itself.
pub fn agui_messages(messages: &[Message]) -> Vec<AguiMessage> {
    messages
        .iter()
        .filter(|m| m.is_me || (!m.content.trim().is_empty() && !is_status_line(&m.content)))
        .map(|m| {
            let reply_to = reply_quote(messages, m);
            AguiMessage {
                id: m.id.clone(),
                role: if m.is_me { "user" } else { "assistant" }.to_string(),
                content: match &reply_to {
                    Some(quote) => format!("{}\n\n{}", reply_quote_line(quote), m.content),
                    None => m.content.clone(),
                },
                tool_call_id: None,
                reply_to,
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct EmojiPickerOpen {
    pub message_id: String,
    pub bounds: Bounds<Pixels>,
}

#[derive(Clone, Debug)]
pub struct Conversation {
    pub id: String,
    pub title: String,
    pub created_at: String,
    pub updated_at: String,
    pub messages: Vec<Message>,
    pub unread_count: usize,
}

impl Conversation {
    pub fn relative_time(&self) -> String {
        let now = SystemTime::now();

        // Parse the ISO 8601 string or fallback to now
        let created_at = NaiveDateTime::parse_from_str(&self.created_at, "%Y-%m-%d %H:%M:%S")
            .map(|dt| SystemTime::from(dt.and_utc()))
            .unwrap_or(SystemTime::now());

        let duration = now.duration_since(created_at).unwrap_or_default();
        let secs = duration.as_secs();

        if secs < 60 {
            "Just now".to_string()
        } else if secs < 3600 {
            let mins = secs / 60;
            format!("{}m ago", mins)
        } else if secs < 86400 {
            let hours = secs / 3600;
            format!("{}h ago", hours)
        } else if secs < 604800 {
            let days = secs / 86400;
            format!("{}d ago", days)
        } else if secs < 2592000 {
            let weeks = secs / 604800;
            format!("{}w ago", weeks)
        } else if secs < 31536000 {
            let months = secs / 2592000;
            format!("{}mo ago", months)
        } else {
            let years = secs / 31536000;
            format!("{}y ago", years)
        }
    }
}

/// A turn this thread is still answerable for.
///
/// It exists from the moment the turn is sent until the turn's outcome has reached SQLite. In
/// between, the thread is holding something the database does not have — the reply filling in,
/// the pictures the run took, and for a moment the person's own message — which is why a thread
/// with one of these is never refilled from the database, and why the run id has to be kept: the
/// server has the whole of the run under it, and that is what the thread is reconciled against
/// instead.
/// A message held back while its thread is busy. The bubble is already on
/// screen and on its way to disk; what waits is the turn.
#[derive(Clone, Debug)]
pub struct QueuedSend {
    /// The bubble's id — the local one until the save lands, then the row's.
    message_id: String,
    content: String,
    /// The recipe as it was when the message was typed: the composer clears it the moment
    /// the draft goes, and the turn has to carry what was on the message.
    recipe: Option<TurnRecipe>,
    /// The skill it was typed with, held for the same reason and taken off the draft at the
    /// same moment: what waits here is this message's, not the next one's.
    skill: Option<String>,
    /// The message this one answers, for the quote the coworker is sent.
    reply: Option<ReplyTo>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LiveTurn {
    /// The id the turn was sent under, so `GET /ag-ui/runs/{run_id}` can be asked what became
    /// of it. Minted before the request, because after the stream is lost there is nothing left
    /// to learn it from.
    pub run_id: String,
    /// The row the run is being painted into. Named, rather than "the last one": the last row
    /// stops being this one the moment anything else touches the list.
    pub message_id: String,
    /// The reply has been handed to the database. Set the moment the app decides what the turn
    /// came to, so a replay landing afterwards does not write the same reply down a second
    /// time; the turn itself is not let go until the write lands, because until then the
    /// database still does not have it.
    pub persisting: bool,
}

/// What one thread's turn is doing, as the app would say it out loud.
///
/// The two halves are separate because a turn between frames is still a turn: a run says
/// "Thinking", clears the label when the thought is over, and goes on running. A label with
/// nothing running is the other way round and just as real — a turn parked on a permission card
/// is waiting for a person, not working.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ThreadActivity {
    /// The working line this thread shows while it is the open one.
    label: Option<String>,
    /// A turn of this thread's is under way.
    responding: bool,
}

/// The row a run is writing into, found by the id the turn gave it when it started.
///
/// "The last one, if it isn't mine" is only true while nothing else touches the list, and a
/// reload, a deleted message or a second turn all make it false — after which the run quietly
/// fills in a bubble belonging to something else, which reads exactly like the run having
/// stopped. By id it is the same row wherever it has drifted to, and honestly nothing at all
/// when it is gone.
fn streaming_message_mut<'a>(
    conversations: &'a mut [Conversation],
    conversation_id: &str,
    message_id: &str,
) -> Option<&'a mut Message> {
    conversations
        .iter_mut()
        .find(|c| c.id == conversation_id)?
        .messages
        .iter_mut()
        .find(|m| m.id == message_id)
}

/// A thread brought up to what the database holds — unless it is in the middle of a turn, in
/// which case it is left exactly as it is and this answers false.
///
/// Nothing of a turn in flight is in SQLite: `persist_assistant_reply` writes the reply only
/// once the turn is over, and the message that asked for it is still on its way down when the
/// turn begins. So for such a thread the persisted set is a strict subset of what is on screen,
/// and swapping one for the other can only take away the bubble the person is watching fill in
/// along with every picture under it. Re-grafting the live row onto the reloaded list would keep
/// the bubble but still drop whatever else had not landed yet, and it would have to guess where
/// the row belongs; refusing the reload keeps the whole thread, and gives up nothing, because
/// the rows the database has are the rows already on screen.
/// Whose ending a run's stream is delivering, once it ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TurnEnding {
    /// This run still holds the thread: paint the ending, write it down, let the thread go.
    Ours,
    /// A later run holds the thread, or this reply is already on its way to the database.
    /// Painting now would write over an ending that is already there.
    Superseded,
    /// Nothing holds the thread. It was let go while this stream was still open — the person
    /// pressed stop, or a resync could not find the run — and nothing else is going to end the
    /// line the run lit up, so this ending clears it and nothing more.
    Orphaned,
}

fn turn_ending(live: Option<&LiveTurn>, run_id: &str) -> TurnEnding {
    match live {
        Some(turn) if turn.run_id == run_id && !turn.persisting => TurnEnding::Ours,
        Some(_) => TurnEnding::Superseded,
        None => TurnEnding::Orphaned,
    }
}

fn apply_reload(
    conversation: &mut Conversation,
    live: Option<&LiveTurn>,
    rows: Vec<ChatMessage>,
) -> bool {
    if live.is_some() {
        return false;
    }
    conversation.messages = rows.into_iter().map(restored_message).collect();
    true
}

/// Take a freshly fetched model catalogue, or keep the one already held. Answers with the note
/// the server sent, when it sent one.
///
/// `/models` never fails: a gateway it cannot reach yields an empty list and the reason, with a
/// `200` on it. Taking that answer wholesale is what emptied the Model field and left it empty —
/// a list the app already had, and could still perfectly well offer, replaced by nothing because
/// of a machine that was down for ninety seconds. So an empty catalogue that arrives with a
/// reason is read as "ask again later" rather than as the catalogue: what is held stays, wearing
/// the server's note so the field can say why it may be stale.
///
/// An empty catalogue with no note is a real answer — this key routes to nothing — and is taken.
fn apply_catalogue(held: &mut ModelCatalogue, fresh: ModelCatalogue) -> Option<String> {
    let note = fresh.note.clone();
    if fresh.models.is_empty() && !held.models.is_empty() && note.is_some() {
        held.note = note.clone();
        return note;
    }
    *held = fresh;
    note
}

/// A line of the app's own, as a row of the transcript.
///
/// A row of its own rather than words on the end of a reply. `persist_assistant_reply` refuses a
/// reply that is a status line, and a line glued onto the coworker's words would slip past that
/// refusal into the history every later turn is sent — after which the coworker reads the app's
/// account of the turn as something it said itself. Kept apart, the reply is saved as the
/// coworker's and the line is painted and forgotten, which is the decision that function
/// documents.
///
/// It came out of no run and so carries no run id: a thread reconciled against the server must
/// not take this row as evidence that it already has some run's reply.
fn status_row(line: &str) -> Message {
    Message {
        id: uuid::Uuid::now_v7().to_string(),
        sender: "AI".to_string(),
        content: line.to_string(),
        sent_at: SystemTime::now(),
        is_me: false,
        reply_preview: None,
        reply_to_id: None,
        reply_is_me: false,
        parts: Vec::new(),
        run_id: None,
        hidden: false,
    }
}

/// The transcript a stop leaves behind, and the reply it leaves to be written down.
///
/// The row the run was filling in stays if there is anything in it at all, and is handed back so
/// the caller can save it: a turn cut short is still a turn that happened, and throwing away what
/// had arrived would make the stop a worse outcome than the loop it was pressed to end. A row
/// with nothing in it goes — an empty bubble above the note reads as an answer still on its way.
fn stopped_transcript(
    messages: &mut Vec<Message>,
    message_id: &str,
) -> Option<(String, Vec<ChatPart>)> {
    let kept = messages
        .iter()
        .find(|message| message.id == message_id)
        .filter(|message| !message.content.trim().is_empty() || !message.parts.is_empty())
        .map(|message| (message.content.clone(), message.parts.clone()));
    if kept.is_none() {
        messages.retain(|message| message.id != message_id);
    }
    messages.push(status_row(STOPPED_TURN_NOTE));
    kept
}

/// A saved row as the feed paints it, with the pieces the turn was made of, so a thread reopened
/// shows the bubbles and the pictures it showed live.
fn restored_message(row: ChatMessage) -> Message {
    let sent_at = parse_sql_time(&row.created_at).unwrap_or_else(SystemTime::now);
    let content = row.content;
    let parts = restored_parts(&content, row.parts);
    Message {
        hidden: row.deleted_at.is_some(),
        id: row.id,
        sender: if row.role == "user" { "Me" } else { "AI" }.to_string(),
        content,
        sent_at,
        is_me: row.role == "user",
        reply_preview: row.reply_preview,
        reply_to_id: row.reply_to_id,
        reply_is_me: row.reply_is_me.unwrap_or(0) != 0,
        parts,
        run_id: row.run_id,
    }
}

/// Paint the streaming bubble at most ~60Hz. Non-text parts (form, screenshot,
/// approval, generative UI) flush immediately so a card is not delayed a frame.
const STREAM_PAINT_MIN: Duration = Duration::from_millis(16);

fn stream_part_sig(parts: &[ChatPart]) -> (usize, u8) {
    let flags = parts.iter().fold(0u8, |acc, part| {
        acc | match part {
            ChatPart::Text(_) => 0,
            ChatPart::Ui(_) => 1,
            ChatPart::Approval(_) => 2,
            ChatPart::Screenshot(_) => 4,
            ChatPart::UserForm(_) => 8,
            ChatPart::SaveLogin(_) => 16,
        }
    });
    (parts.len(), flags)
}

fn stream_paint_due(
    last: Option<Instant>,
    now: Instant,
    prev_sig: (usize, u8),
    sig: (usize, u8),
) -> bool {
    sig != prev_sig || last.is_none_or(|at| now.saturating_duration_since(at) >= STREAM_PAINT_MIN)
}

/// A run rebuilt from the frames the server kept, as the live stream would have painted it.
///
/// The same assembler the stream feeds, over the same frames in the same order, because a turn
/// watched live and a turn read back afterwards must come to the same bubbles and the same
/// pictures — otherwise "where was I?" and "what happened?" are two different answers and the
/// person has to decide which to believe. A run still going keeps its last frames held, exactly
/// as the live stream holds them: a tool whose arguments are still arriving is not a widget yet,
/// and forcing it out would paint half a chart and then take it away again.
fn reply_from_replay(events: &[serde_json::Value], status: &str) -> (String, Vec<ChatPart>) {
    let mut assembler = TurnAssembler::default();
    for event in events {
        assembler.push_event(event);
    }
    if status != "running" {
        assembler.finish();
    }
    assembler.snapshot()
}

/// How many of a thread's runs are asked for when reconciling it against the server.
///
/// Not politeness: a run comes back with every frame it emitted, and a computer-use run's frames
/// carry screenshots, so a whole thread's history is megabytes. Only the newest runs can disagree
/// with what is already on disk — an older run's reply was written down when it happened, and a
/// reply on disk is final — so a handful is all it takes to find what is missing.
const RECONCILE_RUNS: usize = 5;

/// A reply the server has and this thread does not.
#[derive(Debug, Clone, PartialEq)]
struct RecoveredReply {
    run_id: String,
    content: String,
    parts: Vec<ChatPart>,
    /// The run has not ended. The bubble is painted but not written down: a turn is saved when
    /// it is over, whoever happens to be watching.
    live: bool,
    /// When the run started, which is where in the thread its reply belongs.
    started_at: SystemTime,
}

/// What a thread is missing, told by comparing the runs the server kept against the runs the
/// thread can already account for.
///
/// A run this thread already has a reply for is settled and is not looked at again: that reply
/// was built from these very frames, and rebuilding it could only risk saying it differently.
/// A run it has no reply for happened while the app was elsewhere — or not running at all — and
/// comes back as a bubble.
///
/// Where the thread's knowledge begins is the newest run it can name, and nothing at or before
/// that is looked at: a run older than one we already have a reply for is a run whose reply is
/// older still. This is a position in the list rather than a comparison of timestamps on
/// purpose — the run's clock is the server's and the row's clock is this Mac's, and a second of
/// disagreement between them would decide whether a turn appears once or twice.
///
/// A thread that can name no run at all is the thread every build before this one wrote, and on
/// it a finished run and an already-saved one are indistinguishable. Guessing there would put
/// the last few turns into the transcript a second time, so only a run that has not ended is
/// taken: nothing writes a run down before it ends, so a live run cannot already be here. The
/// thread's next turn carries an id, and from then on the diff is exact.
///
/// A run that emitted nothing worth painting is skipped. A run can be started and die before it
/// says anything, and a blank bubble in the transcript is worse than the absence of one.
fn missing_replies(messages: &[Message], runs: &[ThreadRun]) -> Vec<RecoveredReply> {
    let known: HashSet<&str> = messages
        .iter()
        .filter_map(|message| message.run_id.as_deref())
        .collect();
    let anchor = runs
        .iter()
        .rposition(|run| known.contains(run.run_id.as_str()));
    let from = anchor.map_or(0, |index| index + 1);
    // Nothing the coworker said is here yet, so there is no older reply to mistake a run for.
    let trusted = anchor.is_some() || messages.iter().all(|message| message.is_me);
    runs[from.min(runs.len())..]
        .iter()
        .filter(|run| !run.run_id.trim().is_empty() && (trusted || run.is_live()))
        .filter_map(|run| {
            let (plain, parts) = reply_from_replay(&run.events, &run.status);
            let plain = replayed_ending(&run.events, &run.status, run.failure.as_deref(), &plain)
                .unwrap_or(plain);
            if plain.trim().is_empty() && parts.is_empty() {
                return None;
            }
            Some(RecoveredReply {
                run_id: run.run_id.clone(),
                content: plain,
                parts,
                live: run.is_live(),
                // A run that says when it began is placed there. One that does not — an older
                // server, or a field left out — is placed now rather than at the start of the
                // epoch, which would pin it above the message it answers for good, on screen
                // and on disk.
                started_at: (run.started_at_ms > 0)
                    .then(|| {
                        SystemTime::UNIX_EPOCH + Duration::from_millis(run.started_at_ms as u64)
                    })
                    .unwrap_or_else(SystemTime::now),
            })
        })
        .collect()
}

/// A recovered reply put back where it happened, rather than on the end.
///
/// The thread is in the order things were said, and a reply that arrives late is still a reply
/// to the message that asked for it. Placing it by when its run started puts it after that
/// message and before whatever the person said next — which is the difference between a
/// transcript and a pile.
/// The bubble a run's reply belongs to, made if there is none.
///
/// A run's reply has to land in that run's own message: the turn's, by the name it was given
/// when it started, or the one already carrying the run — a bubble grafted for it while nobody
/// was watching. A run picked up off the approvals queue after a restart has neither, and then
/// one is made for it.
///
/// What it must never do is fall back to the last thing the coworker said. That message belongs
/// to another run, and since a reply is written down under its bubble's own name, painting it
/// would overwrite that run's row on disk — the reply the person kept replaced by one they were
/// never shown.
/// The messages of these runs, put out of sight. Returns the ones that were not already.
///
/// A hidden message keeps its row and its run id: it is the only thing that tells the thread it
/// has already accounted for that turn, so taking it out would have the thread fetch it back.
fn hide_messages_of_runs(messages: &mut [Message], runs: &[String]) -> Vec<String> {
    if runs.is_empty() {
        return Vec::new();
    }
    let runs: HashSet<&str> = runs.iter().map(String::as_str).collect();
    let mut newly_hidden = Vec::new();
    for message in messages.iter_mut() {
        if message.hidden {
            continue;
        }
        if message
            .run_id
            .as_deref()
            .is_some_and(|run| runs.contains(run))
        {
            message.hidden = true;
            newly_hidden.push(message.id.clone());
        }
    }
    newly_hidden
}

/// The turns this machine has hidden that the server is still offering.
///
/// Deleting asks the server to withhold the turn, and that ask can fail. This is what the
/// failure looks like afterwards: a run still in the thread's answer that this machine has
/// already put out of sight. Asking again costs nothing — the server's answer to hiding a turn
/// twice is the same as to hiding it once — and it means a delete made on a bad connection is
/// not quietly lost for every other machine.
fn unheard_hidden_runs(messages: &[Message], offered: &[ThreadRun]) -> Vec<String> {
    if offered.is_empty() {
        return Vec::new();
    }
    let hidden: HashSet<&str> = messages
        .iter()
        .filter(|message| message.hidden)
        .filter_map(|message| message.run_id.as_deref())
        .collect();
    if hidden.is_empty() {
        return Vec::new();
    }
    offered
        .iter()
        .map(|run| run.run_id.as_str())
        .filter(|run| hidden.contains(run))
        .map(str::to_string)
        .collect()
}

fn bubble_for_run(
    messages: &mut Vec<Message>,
    target: Option<&str>,
    run_id: &str,
    started_at: Option<SystemTime>,
) -> usize {
    if let Some(at) = target.and_then(|id| messages.iter().position(|m| m.id == id)) {
        return at;
    }
    if let Some(at) = messages
        .iter()
        .position(|m| m.run_id.as_deref() == Some(run_id))
    {
        return at;
    }
    messages.push(Message {
        id: uuid::Uuid::now_v7().to_string(),
        sender: "AI".to_string(),
        content: String::new(),
        // When the run began, not when this machine noticed it: the bubble's time is what the
        // row is stamped with, and a turn recovered after a restart happened before the ones
        // either side of it.
        sent_at: started_at.unwrap_or_else(SystemTime::now),
        is_me: false,
        reply_preview: None,
        reply_to_id: None,
        reply_is_me: false,
        parts: Vec::new(),
        run_id: Some(run_id.to_string()),
        hidden: false,
    });
    messages.len() - 1
}

fn graft_reply(messages: &mut Vec<Message>, reply: &RecoveredReply) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let at = messages
        .iter()
        .position(|message| message.sent_at > reply.started_at)
        .unwrap_or(messages.len());
    messages.insert(
        at,
        Message {
            id: id.clone(),
            sender: "AI".to_string(),
            content: reply.content.clone(),
            sent_at: reply.started_at,
            is_me: false,
            reply_preview: None,
            reply_to_id: None,
            reply_is_me: false,
            parts: reply.parts.clone(),
            run_id: Some(reply.run_id.clone()),
            hidden: false,
        },
    );
    id
}

/// What the feed says for a turn that is over but said nothing.
///
/// The same three stand-ins the live path writes — why it failed, what its tools did, or the
/// note that it said nothing at all — so a turn read back off the server ends the way that turn
/// would have ended had anyone been watching it. `None` when the run spoke for itself.
fn replayed_ending(
    events: &[serde_json::Value],
    status: &str,
    failure: Option<&str>,
    plain: &str,
) -> Option<String> {
    if !plain.trim().is_empty() {
        return None;
    }
    match status {
        "failed" => Some(format!(
            "{RUN_ERROR_PREFIX}{}",
            failure.unwrap_or("the run failed")
        )),
        "finished" => Some(
            tool_standin(&deeds_from_replay(events)).unwrap_or_else(|| EMPTY_TURN_NOTE.to_string()),
        ),
        _ => None,
    }
}

/// A time as a row carries it. Rows written before times were kept to the millisecond have
/// whole seconds, and both read the same; both sort beside each other, since a whole second
/// sorts before any fraction of it.
fn parse_sql_time(value: &str) -> Option<SystemTime> {
    NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.3f")
        .or_else(|_| NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S"))
        .ok()
        .map(|dt| SystemTime::from(dt.and_utc()))
}

fn system_time_ms(at: SystemTime) -> u128 {
    at.duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

#[derive(Clone, Debug, PartialEq)]
pub enum VoiceStatus {
    Ready,
    Connecting,
    Connected,
    Disconnected,
    Error(String),
}

/// Body of a card a later message closed: user-form or approval.
pub const SUPERSEDED_NOTE: &str = "Moved on to your next message.";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SubmitChord {
    /// Enter sends; Shift+Enter inserts a newline.
    #[default]
    Enter,
    /// ⌘Enter sends; Enter inserts a newline.
    CommandEnter,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RightPane {
    #[default]
    Closed,
    Settings,
    Computer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ComputerView {
    Overview,
    Editor { id: Option<String> },
}

impl Default for ComputerView {
    fn default() -> Self {
        Self::Overview
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentRoutine {
    pub id: String,
    pub name: String,
    pub instruction: String,
    pub active: bool,
    pub triggers: Vec<RoutineTrigger>,
    pub runs: Vec<RoutineRun>,
}

/// What the app says when somebody asks a routine for a second way of firing.
///
/// One routine is one schedule, because one schedule is one prompt and one trigger. Two ways
/// of setting the same instruction off are two schedules on the server, and the honest way to
/// have both is to have two routines.
pub const ROUTINE_IS_ONE_SCHEDULE: &str =
    "A routine is one schedule on the server: make another routine for a second trigger.";

/// One schedule as the editor draws it.
fn routine_from_schedule(row: ScheduleRow) -> AgentRoutine {
    let trigger = trigger_from_schedule(&row);
    AgentRoutine {
        name: row.name.unwrap_or_default(),
        instruction: row.prompt,
        active: row.active,
        triggers: vec![trigger],
        // The server keeps no history of a schedule's runs, so there is none to show. Test run
        // still writes its own line, which is this app's note of a thing it just did.
        runs: Vec::new(),
        id: row.id,
    }
}

/// The trigger a schedule is. A cron row is read back into the picker where the line is one of
/// the shapes it can draw and left as the line where it is not; a webhook row is the URL and
/// key the server minted, which are the only ones there have ever been.
fn trigger_from_schedule(row: &ScheduleRow) -> RoutineTrigger {
    match row.kind {
        ScheduleKind::Webhook => {
            let hook = row.webhook.clone().unwrap_or_default();
            RoutineTrigger::Webhook {
                id: row.id.clone(),
                url: hook.url,
                key: hook.key,
                header: hook.header,
            }
        }
        ScheduleKind::Cron => RoutineTrigger::Schedule {
            id: row.id.clone(),
            spec: ScheduleSpec::from_cron(row.cron.as_deref().unwrap_or_default()),
        },
    }
}

/// What a trigger is before the server has made it: the only two the editor offers.
///
/// A [`RoutineTrigger`] carries an id, and for a webhook a URL and a key, none of which the
/// app is in a position to know. This is what it asks for; the row that comes back is the
/// trigger.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NewTrigger {
    Schedule(ScheduleSpec),
    Webhook,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutineTrigger {
    Schedule {
        id: String,
        spec: ScheduleSpec,
    },
    Event {
        id: String,
        kind: &'static str,
        label: String,
    },
    Webhook {
        id: String,
        url: String,
        key: String,
        header: String,
    },
}

impl RoutineTrigger {
    pub fn id(&self) -> &str {
        match self {
            Self::Schedule { id, .. } | Self::Event { id, .. } | Self::Webhook { id, .. } => id,
        }
    }

    pub fn label(&self) -> String {
        match self {
            Self::Schedule { spec, .. } => spec.label(),
            Self::Event { label, .. } => label.clone(),
            Self::Webhook { .. } => "When a webhook fires".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutineRun {
    pub at: String,
    pub ok: bool,
}

/// What the confirm dialog over the app is asking about the active bot's computer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ComputerAction {
    /// Rebuild on the newest image; files and logins stay.
    Update,
    /// Start fresh; everything on it is lost.
    Reset,
}

/// What fills the main slot beside the sidebar: the chat, or a page reached from the dock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MainPage {
    #[default]
    Chat,
    Recipes,
}

/// Which recipes the list asks the server for.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum RecipeFilter {
    #[default]
    Mine,
    Shared,
    Org,
}

impl RecipeFilter {
    pub const ALL: [Self; 3] = [Self::Mine, Self::Shared, Self::Org];

    /// The `?filter=` word.
    pub fn query(self) -> &'static str {
        match self {
            Self::Mine => "mine",
            Self::Shared => "shared",
            Self::Org => "org",
        }
    }

    pub fn from_query(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|filter| filter.query() == word)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Mine => "Mine",
            Self::Shared => "Shared with me",
            Self::Org => "Org",
        }
    }

    /// The chip's element id.
    pub fn element_id(self) -> &'static str {
        match self {
            Self::Mine => "recipes-filter-mine",
            Self::Shared => "recipes-filter-shared",
            Self::Org => "recipes-filter-org",
        }
    }
}

/// Which half of the Skills library the page is showing.
///
/// Two words rather than the server's three: `mine` is what a person has, and `org` is what
/// their colleagues have put where everybody can find it. `shared` is the same rows as `org`
/// until a skill can be sent to one person by name, so offering both would be two toggles for
/// one list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SkillScope {
    #[default]
    Yours,
    Discover,
}

impl SkillScope {
    pub const ALL: [Self; 2] = [Self::Yours, Self::Discover];

    /// The `?filter=` word.
    pub fn query(self) -> &'static str {
        match self {
            Self::Yours => "mine",
            Self::Discover => "org",
        }
    }

    /// The word a driver names this side by, which is the word on the screen rather than the
    /// one on the wire: a person reading the toggle sees "Yours", not "mine".
    pub fn word(self) -> &'static str {
        match self {
            Self::Yours => "yours",
            Self::Discover => "discover",
        }
    }

    pub fn from_word(word: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|scope| scope.word() == word)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Yours => "Yours",
            Self::Discover => "Discover",
        }
    }

    /// The toggle's element id.
    pub fn element_id(self) -> &'static str {
        match self {
            Self::Yours => "settings-skills-scope-yours",
            Self::Discover => "settings-skills-scope-discover",
        }
    }
}

/// Which surface asked for a skill to be made, which is where its refusal is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillCreateFrom {
    /// The New skill sheet, which has a slot of its own over the fields.
    Sheet,
    /// A picked file or folder. There is no sheet, so the list's slot is the only one on screen.
    Upload,
}

/// How many skills each side of the toggle has, whichever side is open.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkillCounts {
    pub yours: usize,
    pub discover: usize,
}

impl SkillCounts {
    pub fn of(self, scope: SkillScope) -> usize {
        match scope {
            SkillScope::Yours => self.yours,
            SkillScope::Discover => self.discover,
        }
    }
}

/// What a recipe's newest run came to, as far as this session has been told. A detail carries
/// a recipe's runs and the list's summaries carry none, so a row says what the app has already
/// been shown and nothing where it has not.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecipeRunNote {
    pub ok: bool,
    pub version: u32,
    /// The step the run stopped at, when it did not finish.
    pub stopped_at: Option<u64>,
    pub at_ms: i64,
}

impl RecipeRunNote {
    /// "last run ok · v3", "last run stopped at step 7 · v3".
    pub fn label(&self) -> String {
        let version = self.version;
        if self.ok {
            format!("last run ok · v{version}")
        } else {
            match self.stopped_at {
                Some(step) => format!("last run stopped at step {step} · v{version}"),
                None => format!("last run stopped · v{version}"),
            }
        }
    }
}

/// What the last Run on… came back with, decoded for the page.
#[derive(Clone)]
pub struct RecipeRunOutcome {
    pub coworker_id: String,
    pub version: u32,
    pub ok: bool,
    pub ran: Option<u64>,
    pub stopped_at: Option<u64>,
    pub error: Option<String>,
    /// The screen after the run, with its size.
    pub image: Option<(Arc<gpui_kit::Image>, u32, u32)>,
}

impl RecipeRunOutcome {
    fn from_result(coworker_id: String, result: RecipeRunResult) -> Self {
        let image = result
            .image
            .as_ref()
            .and_then(|image| crate::opengrok::ScreenshotSpec::from_frame("recipe-run", "", image))
            .map(|spec| (spec.image, spec.width, spec.height));
        Self {
            coworker_id,
            version: result.version,
            ok: result.ok,
            ran: result.ran_count(),
            stopped_at: result.stopped_at,
            error: result.error,
            image,
        }
    }

    /// The one line the page shows for the outcome.
    pub fn headline(&self) -> String {
        let steps = match self.ran {
            Some(1) => "1 step".to_string(),
            Some(count) => format!("{count} steps"),
            None => "the steps".to_string(),
        };
        if self.ok {
            return format!("Ran {steps} of v{}", self.version);
        }
        let stopped = match self.stopped_at {
            Some(step) => format!("Stopped at step {step}"),
            None => "Stopped".to_string(),
        };
        match &self.error {
            Some(error) => format!("{stopped}: {error}"),
            None => stopped,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AppSettingsTab {
    #[default]
    General,
    Profile,
    Appearance,
    Shortcuts,
    Computer,
    Updates,
    Logins,
    Skills,
}

/// Where Route traffic chrome belongs for the active bot's box.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RouteTrafficSurface {
    Hidden,
    BotPane,
    UserSettings,
}

/// One frame of in-app navigation. GPUI has no browser history; we keep this stack
/// so ⌘[ / ⌘] can walk agents, the right pane, and Settings the way macOS apps do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NavLocation {
    pub coworker_id: Option<String>,
    pub page: MainPage,
    pub right_pane: RightPane,
    pub computer_view: ComputerView,
    pub app_settings_open: bool,
    pub app_settings_tab: AppSettingsTab,
}

impl NavLocation {
    fn is_blank(&self) -> bool {
        self.coworker_id.is_none()
            && self.right_pane == RightPane::Closed
            && !self.app_settings_open
    }
}

#[derive(Clone, Debug, Default)]
pub struct NavHistory {
    back: Vec<NavLocation>,
    forward: Vec<NavLocation>,
    current: Option<NavLocation>,
    applying: bool,
}

impl NavHistory {
    fn record(&mut self, loc: NavLocation) {
        if self.applying {
            return;
        }
        match &self.current {
            Some(cur) if cur == &loc => {}
            Some(cur) if cur.is_blank() => self.current = Some(loc),
            Some(cur) => {
                self.back.push(cur.clone());
                self.forward.clear();
                self.current = Some(loc);
            }
            None => self.current = Some(loc),
        }
    }

    fn go_back(&mut self) -> Option<NavLocation> {
        let prev = self.back.pop()?;
        if let Some(cur) = self.current.take() {
            self.forward.push(cur);
        }
        self.current = Some(prev.clone());
        Some(prev)
    }

    fn go_forward(&mut self) -> Option<NavLocation> {
        let next = self.forward.pop()?;
        if let Some(cur) = self.current.take() {
            self.back.push(cur);
        }
        self.current = Some(next.clone());
        Some(next)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum AuthStatus {
    #[default]
    SignedOut,
    SigningIn,
    SignedIn,
}

pub struct AppState {
    pub conversations: Vec<Conversation>,
    /// The turns still in flight, by the thread each belongs to. A thread on this list is not
    /// refilled from the database, its run is written into by name rather than by position, and
    /// coming back to it asks the server what became of the run.
    live_turns: HashMap<String, LiveTurn>,
    /// Server run parked on HITL (user-form / approval / live handoff) after
    /// Threads already reconciled against the server this session. Once is enough: after it, the
    /// app has been watching, and every turn since has gone through the same door on its way to
    /// disk. Asking again on every visit would fetch a thread's frames — screenshots and all —
    /// to learn nothing.
    reconciled_threads: HashSet<String>,
    /// Last send/receive per coworker. Beats an unopened session's empty `messages`.
    pub last_active_at: HashMap<String, SystemTime>,
    pub active_conversation_id: Option<String>,
    pub theme_mode: String,
    pub amplitude: Arc<AtomicU32>,
    pub ai_amplitude: Arc<AtomicU32>, // New field for AI voice viz
    pub is_ai_speaking: Arc<AtomicBool>,
    pub is_voice_mode_open: bool,
    pub is_sidebar_open: bool,
    pub is_voice_muted: bool,
    pub voice_status: VoiceStatus,
    pub more_menu_open: bool,
    /// The composer's "+" picker: what it can offer, what is picked, and what each entry is.
    /// Tools named for the next message. They show as chips beside the composer's "+".
    pub picked_tools: Vec<PickedTool>,
    /// The recipe the next message runs, once one has been picked with `/`. While it is set the
    /// composer is in recipe mode: `@` offers this recipe's parameters instead of the bot's
    /// tools, because a turn that is already a recipe run has no use for a tool roster.
    pub active_recipe: Option<ActiveRecipe>,
    /// The skill the next message is sent with, once one has been picked with `/`. One to a
    /// message, because the turn names one id, and gone again the moment that message goes.
    pub active_skill: Option<ActiveSkill>,
    /// Which of the composer's lists is open, as the composer publishes it.
    ///
    /// The panel itself lives in the composer's own view and nothing outside that view can read
    /// it. This is the one bit of it the rest of the app can see, and it is what lets an agent
    /// driver — which is handed this state and nothing else — tell that typing `/` opened
    /// anything. The rows are not copied here: they are rebuilt from the same sources the panel
    /// draws them from.
    pub composer_panel: Option<crate::components::chat_input::PanelMode>,
    pub is_app_settings_open: bool,
    pub bot_finder_open: bool,
    pub command_palette_open: bool,
    nav: NavHistory,
    pub app_settings_tab: AppSettingsTab,
    pub submit_chord: SubmitChord,
    /// What a plain send does while a turn is running. Read from prefs.json at boot.
    pub on_send: OnSend,
    /// Messages held per thread until it is idle, in the order they were typed.
    queued_sends: HashMap<String, VecDeque<QueuedSend>>,
    pub audio_input: Option<AudioInput>,
    pub sidebar_collapsed: bool,
    pub sidebar_hidden: bool,
    pub sidebar_expanded_width: f32,
    pub sidebar_responsive: ResponsiveCollapse,
    pub auto_collapsed: bool,
    pub database_service: Option<DatabaseService>,
    pub config: Option<Config>,
    pub debug_markdown_disabled: bool,
    pub tts_service: Option<TtsService>,
    tts_initing: bool,
    pending_read_aloud: Option<(String, String)>,
    pub native_tts: SourceTtsState,
    pub opengrok: Option<OpenGrokClient>,
    pub account: Option<Account>,
    pub auth_status: AuthStatus,
    /// What the server refused, and why. Verdicts only: a sign-in that was turned down, a patch
    /// the server would not take, a bot that could not be hired. Nothing about the wire goes in
    /// here — that is `reachability` — because a field holding both is a field the app cannot
    /// read: it can neither retry the half that is worth retrying nor stop showing the half that
    /// has stopped being true.
    pub auth_error: Option<String>,
    /// What the app can and cannot reach, as a state that clears itself. Private because the
    /// retry loop has to be started and stopped with it, and that is what `note_failure` and
    /// `came_back` are for.
    reachability: Reachability,
    /// Whether the app still has a session, as a state that only a person clears.
    ///
    /// The third field of its kind, and it is a third because it is a third thing. `auth_error`
    /// holds verdicts, `reachability` holds the state that clears itself, and this holds the one
    /// that does neither: the wire is fine, the server answered, and nothing will work until
    /// somebody signs in. Private for the same reason as `reachability` — setting it has to also
    /// stop the app sending, and that is what `note_signed_out` and `sign_in_again` are for.
    session: Session,
    /// A retry loop is running. Only one at a time: it is asking one question of one server.
    reconnecting: bool,
    /// Which retry loop. Bumped to start one and bumped again to end it, so a loop that is
    /// mid-wait when the connection comes back finds itself out of date and stops — the shape
    /// `login_epoch` and `recipes_epoch` already use in this file.
    reconnect_epoch: u64,
    login_epoch: u64,
    pub login_email: String,
    pub login_password: String,
    pub coworkers: Vec<Coworker>,
    pub active_coworker_id: Option<String>,
    /// What each thread's turn is doing, by the thread it belongs to.
    ///
    /// Keyed the way `live_turns` is, and for the same reason. This was one label and one
    /// owner's name for the whole app, which held while only one bot could be working: the
    /// second turn to start took the field from the first, so the first bot's working line went
    /// dark while its run was still going, and — worse — its own frames were then dropped for
    /// not matching the owner the app thought was responding. A turn now survives switching away
    /// from it, so leaving one bot running while starting another is the ordinary thing to do,
    /// and a thread's line has to be the thread's own.
    thread_activity: HashMap<String, ThreadActivity>,
    pub model_catalogue: ModelCatalogue,
    pub right_pane: RightPane,
    pub computer_view: ComputerView,
    pub routines: HashMap<String, Vec<AgentRoutine>>,
    pub model_picker_open: bool,
    pub avatar_editor_open: bool,
    pub hiring: bool,
    pub pinned_coworker_ids: HashSet<String>,
    pub hidden_coworker_ids: HashSet<String>,
    pub renaming_coworker_id: Option<String>,
    pub hidden_bots_open: bool,
    pub reply_to: Option<ReplyTo>,
    pub message_reactions: HashMap<String, String>,
    pub emoji_picker: Option<EmojiPickerOpen>,
    pub form_picks: HashMap<String, HashMap<String, String>>,
    /// Checkbox / select picks on a user-form. Never passwords or other secrets —
    /// those stay in the transcript view's input state and are never written here.
    /// Keyed by [`crate::opengrok::UserFormSpec::card_key`].
    pub user_form_picks: HashMap<String, HashMap<String, String>>,
    /// Agent/E2E typed values (including secrets). In-memory only — never
    /// sqlite / AG-UI content. Continue prefers live InputState, then this.
    pub user_form_typed: HashMap<String, HashMap<String, String>>,
    /// Routes exist on this server. Diagnostic only — a missing-route 404
    /// must not freeze every stacked open user-form. Unresolved cards stay
    /// clickable until that card settles.
    pub user_form_verbs_available: bool,
    /// Local/server settlements grafted onto AG-UI replay, which does not carry
    /// `formResolution`. Keyed by gateway `entryId` or `card_key`.
    user_form_resolutions: HashMap<String, FormResolution>,
    /// First sqlite `site_logins` list has landed. Until then, do not treat an
    /// empty in-memory vec as "no logins" (startup race).
    site_logins_ready: bool,
    /// Stable resolution to restore if a POST fails (never reopen a settled card).
    /// Prior resolution to restore if an optimistic paint is rolled back.
    /// `Some(None)` means the card was idle; without that case a Dismiss that
    /// failed on the server left the card Dismissed forever.
    user_form_restore: HashMap<String, Option<FormResolution>>,
    /// Form card key → handoff card id from dismiss `handoffEntryId`.
    user_form_handoffs: HashMap<String, String>,
    /// Skip / I'm done before dismiss returned `handoffEntryId`. Local chrome
    /// already settled; POST once the sibling id lands. Never the form entryId.
    user_form_pending_resolves: HashMap<String, PendingBoxHandoff>,
    /// Form card keys whose box-handoff already resolved (I'm done / Skip).
    user_form_handoff_done: HashSet<String>,
    /// Computer sibling chrome grafted across hide→reshow / SSE replay.
    user_form_computer_handoffs: HashMap<String, ComputerHandoffStatus>,
    /// Prior Computer sibling to restore if Open the screen POST fails.
    user_form_handoff_restore: HashMap<String, Option<ComputerHandoffStatus>>,
    /// Site-login metadata for Settings → Logins. Never passwords.
    pub site_logins: Vec<SiteLoginRecord>,
    site_login_vault: Option<SiteLoginVault>,
    /// Password held only until Save / Not now. Never sqlite / ChatPart / tree.
    pending_save: HashMap<String, PendingSave>,
    /// A "Use saved login" press on a card, keyed by card key, until the form settles.
    pub saved_login_use: HashMap<String, SavedLoginUse>,
    /// Ids of the saved logins whose password is in this Mac's keychain. The others are on
    /// the server only and are fetched, after Touch ID, the first time they are used here.
    pub site_logins_on_this_mac: HashSet<String>,
    /// Which rows have a code seed on this Mac. Asked of the keychain by name only, so
    /// knowing costs nothing; the seed itself is read when a row is opened or picked.
    pub site_logins_with_code: HashSet<String>,
    /// Cards whose account list is up. A card arrives without it: the list belongs to the
    /// field, so it comes up when the person puts the cursor there, the way a browser's
    /// autofill does, and goes away again on a click anywhere else.
    pub user_form_list_open: HashSet<String>,
    /// The authenticator-code seeds this Mac holds, parsed, for the detail pane's ticker and
    /// the code card. Read off the keychain on each load.
    pub site_login_codes: HashMap<String, totp_rs::TOTP>,
    /// What the last Add / Import / sync did, for Settings → Logins.
    pub site_login_notice: Option<String>,
    pub site_login_error: Option<String>,
    /// Site icons for Settings → Logins, by origin. `None` is a remembered miss: asked once,
    /// the server had none (or would not say), not asked again this run.
    pub site_login_icons: HashMap<String, Option<Arc<Image>>>,
    /// The search field on Settings → Logins. The page draws its own field; this is the copy
    /// the list filters by, and the one the driver writes.
    pub site_login_query: String,
    /// The row the detail pane shows.
    pub site_login_selected: Option<String>,
    /// The Add-login sheet is up over the page.
    pub site_login_add_open: bool,
    pub approval_decisions: HashMap<String, ApprovalDecision>,
    pub local_exec_machine_id: Option<String>,
    local_exec_cancel: Option<Arc<AtomicBool>>,
    pub expanded_shell_output: HashSet<String>,
    pub computers: Vec<ConnectedComputer>,
    /// The active coworker's computer, as last polled. Cleared on a switch so a
    /// bot never shows the previous one's screen.
    pub coworker_computer: Option<CoworkerComputer>,
    /// `egressTunnelAvailable` on `GET /ag-ui/host-settings`: host intent AND the open
    /// coworker's box advertising the tunnel.
    pub host_egress_tunnel_available: bool,
    /// A network-policy write in flight for (bot, word): a computer record fetched meanwhile
    /// keeps the word the person chose. See `set_egress_policy_for`.
    pub egress_policy_pending: Option<(String, LocalExecMode)>,
    /// Local/host opt-in for **Route traffic through this computer**. Defaults
    /// ON; host `egressTunnelEnabled` overwrites only when the key is present.
    pub egress_tunnel_enabled: bool,
    /// This server answered 404 to `/coworkers/{id}/computer`: it has no such
    /// endpoint, so polling stops until the roster reloads.
    pub computer_endpoint_missing: bool,
    /// The active coworker's screen as last fetched, painted in the Computer
    /// pane's tile. Polled with the status, only while the box has a screen.
    pub coworker_screen: Option<std::sync::Arc<gpui_kit::Image>>,
    /// Newest tool PNG for thumbs / Open-the-screen pin. Not every chat row.
    pub last_box_shot: Option<ScreenshotSpec>,
    /// Runs while the Computer pane is open; dropped when it closes.
    computer_poll: Option<Task<()>>,
    /// One screen window per coworker: Open brings the existing one forward rather than
    /// stacking another.
    #[cfg(target_os = "macos")]
    computer_windows: std::collections::HashMap<
        String,
        WindowHandle<crate::components::computer_screen::ComputerScreen>,
    >,
    /// Update / Reset ask first: the dialog over the app, until Confirm or Cancel.
    pub computer_confirm: Option<ComputerAction>,
    /// The network-permission dialog for the open bot's own computer is on screen.
    pub network_policy_open: bool,
    /// What the last Update / Reset request said when it was refused; shown under the buttons.
    pub computer_action_error: Option<String>,
    /// The coworker whose absent computer we already asked the server to (re)provision, so a
    /// status of `absent` heals once per visit rather than on every poll.
    computer_heal_requested: Option<String>,
    /// What the main slot shows: the chat, or the Recipes page.
    pub page: MainPage,
    /// The pictures of one turn, opened full window from a tile in the transcript.
    pub lightbox: Option<crate::components::lightbox::Lightbox>,
    pub recipes: Vec<RecipeSummary>,
    pub recipes_filter: RecipeFilter,
    pub recipes_loading: bool,
    pub recipes_error: Option<String>,
    /// Bumped per list request, so a late answer for an earlier filter is dropped.
    recipes_epoch: u64,
    /// The recipe the detail view shows, once it has loaded.
    pub recipe_open: Option<RecipeDetail>,
    /// The recipe the detail view is on, from the moment it is asked for; a late answer for
    /// another one is dropped.
    pub recipe_open_id: Option<String>,
    pub recipe_loading: bool,
    /// What the detail view is doing right now ("Saving…", "Running…"), while it does it.
    pub recipe_busy: Option<String>,
    /// What the last recipe request said when it was refused.
    pub recipe_error: Option<String>,
    pub recipe_run_result: Option<RecipeRunOutcome>,
    /// Delete asks first: the dialog over the app, until Delete or Cancel.
    pub recipe_delete_confirm: bool,
    /// What each recipe's newest run came to, kept as details are read, so a row in the list
    /// can say what became of that recipe last time.
    pub recipe_last_runs: HashMap<String, RecipeRunNote>,
    /// Settings → Skills, as that page has it. A skill is prose the model reads before it
    /// works, and the library is the account's rather than one coworker's — so these are flat
    /// fields like the recipes above, not a map keyed by bot like the routines.
    pub skills: Vec<SkillSummary>,
    /// What the search field holds. The listing takes no query of its own, so this filters the
    /// rows that arrived; the state keeps the text so the driver can write it too.
    pub skills_query: String,
    pub skills_scope: SkillScope,
    /// The skills `/` offers, which are the ones this person can invoke.
    ///
    /// Kept apart from [`Self::skills`] on purpose. That list is whichever side of the Settings
    /// toggle was last looked at, and it is emptied the moment the toggle moves: a composer
    /// reading it offered a colleague's library and none of this person's own, for no reason
    /// anybody typing `/` could have seen.
    pub your_skills: Vec<SkillSummary>,
    /// Whether that listing is on its way, and what the server said if it refused. The `/` panel
    /// says which of the three it is, because a library that would not load and a library with
    /// nothing in it read the same on screen and are opposite things to do something about.
    pub your_skills_loading: bool,
    pub your_skills_error: Option<String>,
    /// How many rows each side of the toggle has. Both sides are fetched on every refresh
    /// because the toggle carries a count per side, and a count for the scope nobody has opened
    /// would otherwise be a blank where a number belongs.
    pub skills_counts: SkillCounts,
    pub skills_loading: bool,
    /// What the LIST's last request said when it was refused: a refresh, a picker, a delete.
    ///
    /// Three error slots rather than one, the way the recipes page keeps its list's and its
    /// detail's apart. They are three places on screen, and one slot for all three put a delete
    /// that the server refused inside the "New skill" sheet, where it read as a verdict on what
    /// somebody had just typed.
    pub skills_error: Option<String>,
    /// What the OPEN SKILL's fetch said when it was refused, shown on its pane.
    pub skill_error: Option<String>,
    /// What the CREATE said when it was refused — the server's own sentence, including the one
    /// naming the 8000-character cap on a body. Shown in the sheet, over the fields it is about.
    pub skill_add_error: Option<String>,
    /// Bumped per refresh, so a late answer for an earlier scope is dropped.
    skills_epoch: u64,
    /// The skill the detail pane shows, once it has loaded, and the one it is on from the
    /// moment it was asked for. A late answer for a skill the person has left is dropped.
    pub skill_open: Option<SkillDetail>,
    pub skill_open_id: Option<String>,
    /// Create a skill: the sheet over the page, until Cancel or Save.
    pub skill_add_open: bool,
    /// A create is in flight. Save is dead while it is: pressed twice, the second try is a name
    /// the server has just taken, and the sheet would show "you already have a skill called
    /// that" about the skill it had itself just made.
    pub skill_saving: bool,
    /// The server took the last create, so the sheet's fields may be emptied. The page turns
    /// this off once it has. Cancel empties them too; a click on the dimmed background does
    /// not, because a click landing wide of a sheet is not a decision to throw work away.
    pub skill_add_taken: bool,
    /// Delete asks first: the skill the dialog is about, until Delete or Cancel.
    pub skill_delete_confirm: Option<String>,
    /// The window the app's pages live in. A second window — a coworker's screen — has no
    /// page of its own: it asks this one to show the Recipes page and brings it forward.
    main_window: Option<AnyWindowHandle>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApprovalDecision {
    Pending,
    Sending,
    AllowOnce,
    Always,
    Denied,
    Never,
    /// A later message moved the thread on. The server closed the card when
    /// that message arrived; nothing was allowed or denied.
    Superseded,
    Failed(String),
}

impl ApprovalDecision {
    pub fn is_settled(&self) -> bool {
        !matches!(self, Self::Pending | Self::Sending)
    }

    /// The person (or the policy) has answered; a click still in flight counts.
    pub fn is_answered(&self) -> bool {
        !matches!(self, Self::Pending)
    }

    /// What the transcript says where the card was, once it is answered. The
    /// card itself chooses the words: a local shell and an MCP call are not
    /// answering the same question. See [`ApprovalSpec::outcome`].
    pub fn outcome_line(&self, bot: &str, spec: &ApprovalSpec) -> Option<String> {
        let resolution = match self {
            Self::AllowOnce => LocalExecResolution::AllowOnce,
            Self::Always => LocalExecResolution::Always,
            Self::Denied => LocalExecResolution::DenyOnce,
            Self::Never => LocalExecResolution::Never,
            Self::Superseded => return Some(SUPERSEDED_NOTE.to_string()),
            _ => return None,
        };
        Some(spec.outcome(bot, resolution))
    }
}

enum UserFormDispatch {
    Submit(UserFormValues),
    Dismiss(UserFormDismissMode),
    ResolveHandoff(BoxHandoffResolution),
}

#[derive(Clone, Debug)]
struct PendingBoxHandoff {
    resolution: BoxHandoffResolution,
    run_id: String,
    conversation_id: String,
    agent_id: String,
}

#[derive(Clone, Debug, Default)]
pub struct SourceTtsState {
    pub message_id: Option<String>,
    pub is_paused: bool,
    pub is_loading: bool,
}

/// A tool the person named for the next message, by typing `@` in the composer.
///
/// The kind travels with it. The chip row used to decide Tools from Apps by matching the name
/// against a hardcoded list, which only worked while the names came from one hardcoded menu;
/// a real tool's name comes from the server and matches nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PickedTool {
    /// What the server calls it: `shell`, or a plugin's qualified `plugin.server.tool`.
    pub id: String,
    /// What the chip reads.
    pub label: String,
    pub kind: PickedKind,
}

/// Which group a chip sits in. A bare name is one of the server's built-in tools; a qualified
/// one belongs to a plugin, which is what the person means by an app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickedKind {
    Tool,
    App,
}

impl PickedKind {
    /// Read the kind off the name, the same rule the server's tool listing uses.
    pub fn of(id: &str) -> Self {
        if id.contains('.') {
            Self::App
        } else {
            Self::Tool
        }
    }
}

/// The skill the next message is sent with, picked with `/` in the composer.
///
/// A skill is prose the model reads before it works, so nothing about it is asked of the person:
/// there is no declaration to fill in and no bar over the composer. What the turn carries is the
/// id; the name is kept beside it for the chip and for anything that has to say which skill is
/// on the draft, because the library it was picked out of is emptied whenever Settings → Skills
/// changes sides and a look-up by id later would come back empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveSkill {
    pub id: String,
    pub name: String,
}

/// The recipe or workflow the next message runs, picked with `/` in the composer.
///
/// The parameters are the copy the recipe carried when it was picked rather than a look-up by
/// id later: the recipe list is refetched and refiltered under the draft, and a draft that lost
/// what it needs because a list was narrowed elsewhere would be a mystery to whoever typed it.
///
/// One type for both kinds, because everything about being on the draft is the same for the
/// two: the same declaration, the same values, the same bar, the same `@`. What differs is the
/// noun, and that is [`Self::kind`], carried so that no surface has to guess.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveRecipe {
    pub id: String,
    pub name: String,
    /// A taped sequence or a decision tree, as the listing said.
    pub kind: RecipeKind,
    pub parameters: Vec<RecipeParameter>,
    /// What each parameter was filled in with, by name, as the person typed it. A parameter
    /// with no entry here is unfilled.
    pub values: HashMap<String, String>,
}

impl ActiveRecipe {
    /// A recipe put on the draft, with every declared default already standing in its field.
    /// The run would use those defaults anyway, and a field showing what will be used is worth
    /// more than an empty one that quietly means the same.
    pub fn from_summary(recipe: &RecipeSummary) -> Self {
        let values = recipe
            .parameters
            .iter()
            .filter_map(|parameter| {
                let default = parameter.default.clone()?;
                (!default.is_empty()).then(|| (parameter.name.clone(), default))
            })
            .collect();
        Self {
            id: recipe.id.clone(),
            name: if recipe.name.trim().is_empty() {
                format!("Untitled {}", recipe.kind.word())
            } else {
                recipe.name.trim().to_string()
            },
            kind: recipe.kind,
            parameters: recipe.parameters.clone(),
            values,
        }
    }

    /// A decision tree rather than a tape, which is what the bar over the composer says and the
    /// only thing the composer does differently with the two.
    pub fn is_workflow(&self) -> bool {
        self.kind == RecipeKind::Workflow
    }

    pub fn value(&self, name: &str) -> Option<&str> {
        self.values.get(name).map(String::as_str)
    }

    /// Fill a parameter in, or take its value away. Blank is not a value, so it clears too.
    pub fn set_value(&mut self, name: &str, value: Option<String>) {
        match value.map(|value| value.trim().to_string()) {
            Some(value) if !value.is_empty() => {
                self.values.insert(name.to_string(), value);
            }
            _ => {
                self.values.remove(name);
            }
        }
    }

    /// The required parameters nobody has filled in, in the order they were declared. Nothing
    /// can be sent while this is not empty.
    pub fn missing(&self) -> Vec<&str> {
        self.parameters
            .iter()
            .filter(|parameter| parameter.required && self.value(&parameter.name).is_none())
            .map(|parameter| parameter.name.as_str())
            .collect()
    }

    /// What the recipe has still to be told, required first and then optional, each with its
    /// place in the declaration so a pick can name it back.
    ///
    /// A parameter that has a value is left out on purpose. It has not gone anywhere — it is a
    /// chip in the bar above the composer, where its value is shown and can be changed — and
    /// what someone came to this list for is what is still to do, not a roll-call of the done.
    /// Required first because those are the ones stopping the message being sent; the
    /// declaration's own order is kept within each group, so the list does not reshuffle
    /// under the hand as values come in.
    pub fn unfilled(&self) -> Vec<(usize, &RecipeParameter)> {
        let mut unfilled: Vec<(usize, &RecipeParameter)> = self
            .parameters
            .iter()
            .enumerate()
            .filter(|(_, parameter)| self.value(&parameter.name).is_none())
            .collect();
        unfilled.sort_by_key(|(_, parameter)| !parameter.required);
        unfilled
    }

    /// What goes in the turn's `forwardedProps`: the recipe's id, and each filled-in value as
    /// the kind its declaration named.
    pub fn turn(&self) -> TurnRecipe {
        let mut values = serde_json::Map::new();
        for parameter in &self.parameters {
            if let Some(text) = self.value(&parameter.name) {
                values.insert(parameter.name.clone(), parameter.encode(text));
            }
        }
        TurnRecipe {
            id: self.id.clone(),
            values,
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// What a caller hears once a patch is over: nothing when the server took it, the server's
/// message when it refused.
pub type PatchDone = Box<dyn FnOnce(Option<String>, &mut App)>;

impl AppState {
    pub fn new() -> Self {
        let mut state = Self {
            conversations: Vec::new(),
            live_turns: HashMap::new(),
            reconciled_threads: HashSet::new(),
            last_active_at: HashMap::new(),
            active_conversation_id: None,
            theme_mode: "light".to_string(),
            amplitude: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            ai_amplitude: std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0)),
            is_ai_speaking: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            is_voice_mode_open: false,
            is_voice_muted: false,
            is_sidebar_open: true,
            voice_status: VoiceStatus::Ready,
            more_menu_open: false,
            picked_tools: Vec::new(),
            active_recipe: None,
            active_skill: None,
            composer_panel: None,
            is_app_settings_open: false,
            bot_finder_open: false,
            command_palette_open: false,
            nav: NavHistory::default(),
            app_settings_tab: AppSettingsTab::General,
            submit_chord: SubmitChord::Enter,
            on_send: OnSend::default(),
            queued_sends: HashMap::new(),
            audio_input: None,
            sidebar_collapsed: false,
            sidebar_hidden: false,
            sidebar_expanded_width: SIDEBAR_EXPANDED,
            sidebar_responsive: ResponsiveCollapse::default(),
            auto_collapsed: false,
            database_service: None,
            config: None,
            debug_markdown_disabled: false,
            tts_service: None,
            tts_initing: false,
            pending_read_aloud: None,
            native_tts: SourceTtsState::default(),
            opengrok: None,
            account: None,
            auth_status: AuthStatus::SignedOut,
            auth_error: None,
            reachability: Reachability::default(),
            session: Session::default(),
            reconnecting: false,
            reconnect_epoch: 0,
            login_epoch: 0,
            login_email: String::new(),
            login_password: String::new(),
            coworkers: Vec::new(),
            active_coworker_id: None,
            thread_activity: HashMap::new(),
            model_catalogue: ModelCatalogue::default(),
            right_pane: RightPane::Closed,
            computer_view: ComputerView::Overview,
            routines: HashMap::new(),
            model_picker_open: false,
            avatar_editor_open: false,
            hiring: false,
            pinned_coworker_ids: HashSet::new(),
            hidden_coworker_ids: HashSet::new(),
            renaming_coworker_id: None,
            hidden_bots_open: false,
            reply_to: None,
            message_reactions: HashMap::new(),
            emoji_picker: None,
            form_picks: HashMap::new(),
            user_form_picks: HashMap::new(),
            user_form_typed: HashMap::new(),
            user_form_verbs_available: USER_FORM_SERVER_FILL_AVAILABLE,
            user_form_resolutions: HashMap::new(),
            site_logins_ready: false,
            user_form_restore: HashMap::new(),
            user_form_handoffs: HashMap::new(),
            user_form_pending_resolves: HashMap::new(),
            user_form_handoff_done: HashSet::new(),
            user_form_computer_handoffs: HashMap::new(),
            user_form_handoff_restore: HashMap::new(),
            site_logins: Vec::new(),
            site_login_vault: None,
            pending_save: HashMap::new(),
            saved_login_use: HashMap::new(),
            site_logins_on_this_mac: HashSet::new(),
            site_logins_with_code: HashSet::new(),
            user_form_list_open: HashSet::new(),
            site_login_codes: HashMap::new(),
            site_login_notice: None,
            site_login_error: None,
            site_login_icons: HashMap::new(),
            site_login_query: String::new(),
            site_login_selected: None,
            site_login_add_open: false,
            approval_decisions: HashMap::new(),
            local_exec_machine_id: None,
            local_exec_cancel: None,
            expanded_shell_output: HashSet::new(),
            computers: Vec::new(),
            coworker_computer: None,
            host_egress_tunnel_available: false,
            egress_policy_pending: None,
            network_policy_open: false,
            egress_tunnel_enabled: true,
            coworker_screen: None,
            last_box_shot: None,
            computer_confirm: None,
            computer_action_error: None,
            computer_heal_requested: None,
            computer_endpoint_missing: false,
            computer_poll: None,
            page: MainPage::Chat,
            lightbox: None,
            recipes: Vec::new(),
            recipes_filter: RecipeFilter::Mine,
            recipes_loading: false,
            recipes_error: None,
            recipes_epoch: 0,
            recipe_open: None,
            recipe_open_id: None,
            recipe_loading: false,
            recipe_busy: None,
            recipe_error: None,
            recipe_run_result: None,
            recipe_delete_confirm: false,
            recipe_last_runs: HashMap::new(),
            skills: Vec::new(),
            skills_query: String::new(),
            skills_scope: SkillScope::Yours,
            your_skills: Vec::new(),
            your_skills_loading: false,
            your_skills_error: None,
            skills_counts: SkillCounts::default(),
            skills_loading: false,
            skills_error: None,
            skill_error: None,
            skill_add_error: None,
            skills_epoch: 0,
            skill_open: None,
            skill_open_id: None,
            skill_add_open: false,
            skill_saving: false,
            skill_add_taken: false,
            skill_delete_confirm: None,
            main_window: None,
            #[cfg(target_os = "macos")]
            computer_windows: std::collections::HashMap::new(),
        };

        state
    }

    pub fn set_config(&mut self, config: Config, cx: &mut Context<Self>) {
        match OpenGrokClient::new(&config.opengrok_base_url) {
            Ok(client) => {
                let client =
                    client.with_session_file(config.data_dir.join("opengrok-session.json"));
                self.opengrok = Some(client);
                self.restore_session(cx);
            }
            Err(error) => {
                self.auth_error = Some(error.message);
                self.opengrok = None;
            }
        }
        self.config = Some(config);
        self.ensure_site_login_vault(cx);
        cx.notify();
    }

    fn restore_session(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        if !client.load_session() {
            return;
        }
        self.login_epoch += 1;
        let epoch = self.login_epoch;
        self.auth_status = AuthStatus::SigningIn;
        self.auth_error = None;
        cx.spawn(async move |this, cx| {
            let account = match client.me().await {
                Ok(account) => Ok(account),
                Err(error) if error.is_unauthorized() => {
                    match client.refresh_after_unauthorized().await {
                        Ok(()) => client.me().await,
                        Err(_) => {
                            client.clear_session();
                            Err(error)
                        }
                    }
                }
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |state, cx| {
                if state.login_epoch != epoch {
                    return;
                }
                match account {
                    Ok(account) => {
                        state.account = Some(account);
                        state.auth_status = AuthStatus::SignedIn;
                        state.auth_error = None;
                        state.note_server_answered(cx);
                        state.start_local_exec(cx);
                        state.refresh_coworkers(cx);
                        state.refresh_computers(cx);
                        state.refresh_host_egress(cx);
                        state.sync_pending_approvals(cx);
                    }
                    Err(error) => {
                        state.account = None;
                        state.auth_status = AuthStatus::SignedOut;
                        match error.unreachable() {
                            // A saved session the app could not check is not a session that was
                            // refused, and the sign-in page should not imply it was. The
                            // indicator says the server is not answering and the loop keeps
                            // asking; signing in works as soon as it does.
                            Some(_) => state.note_failure(&error, cx),
                            // A session the server refused is a session to sign in again for,
                            // and the page says that by being the page — it is not worth a red
                            // line above the fields before anybody has typed anything.
                            None => state.note_server_answered(cx),
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn is_signed_in(&self) -> bool {
        self.auth_status == AuthStatus::SignedIn && self.account.is_some()
    }

    /// The working line under the open thread, which is that thread's own or nothing.
    ///
    /// Nothing else can put a line here: the label is looked up by the thread it belongs to, so
    /// a bot working next door has no way to reach this one's footer, and coming back to a
    /// thread whose run never stopped finds the line exactly where it was left.
    pub fn visible_bot_status(&self) -> Option<String> {
        self.thread_status(self.active_conversation_id.as_deref()?)
            .filter(|label| !label.is_empty())
            .map(str::to_string)
    }

    /// What one thread's turn is saying, whether or not that thread is the open one.
    fn thread_status(&self, conversation_id: &str) -> Option<&str> {
        self.thread_activity.get(conversation_id)?.label.as_deref()
    }

    /// The selected coworker's name, for copy that addresses it.
    pub fn active_bot_name(&self) -> String {
        self.active_coworker_id
            .as_ref()
            .and_then(|id| self.coworkers.iter().find(|c| &c.id == id))
            .map(|c| c.name.clone())
            .unwrap_or_else(|| "this agent".to_string())
    }

    pub fn is_active_bot_responding(&self) -> bool {
        self.active_conversation_id
            .as_deref()
            .is_some_and(|id| self.is_thread_responding(id))
    }

    /// A turn of this thread's is under way, wherever the person happens to be looking.
    fn is_thread_responding(&self, conversation_id: &str) -> bool {
        self.thread_activity
            .get(conversation_id)
            .is_some_and(|activity| activity.responding)
    }

    /// A turn has begun in this thread, and this is the first thing it has to say.
    ///
    /// Named by thread throughout — this one, `apply_turn_status` and `finish_responding` — so
    /// that a run only ever writes into the line of the thread it belongs to. A run with no
    /// thread to its name has nowhere to put a line and so says nothing: the only caller that
    /// can hand one over is a card picked off the approvals queue for a thread the app never saw
    /// start, and that run has no footer of its own to light up.
    fn begin_responding(&mut self, conversation_id: Option<&str>, status: &str) {
        let Some(conversation_id) = conversation_id else {
            return;
        };
        let activity = self
            .thread_activity
            .entry(conversation_id.to_string())
            .or_default();
        activity.responding = true;
        activity.label = Some(status.to_string());
    }

    /// One frame's worth of news about a thread's turn.
    ///
    /// There is no owner to check any more. A frame belongs to the thread it names, and it is
    /// written into that thread's line whatever any other thread is doing — which is the whole
    /// of the fix: the check this used to make was against a single app-wide owner, so a second
    /// bot starting a turn made every one of the first bot's frames look like somebody else's
    /// and they were thrown away.
    fn apply_turn_status(&mut self, conversation_id: Option<&str>, tick: ActivityTick) {
        let Some(conversation_id) = conversation_id else {
            return;
        };
        match tick {
            ActivityTick::Keep => {}
            ActivityTick::Clear => {
                if self.has_open_user_form(conversation_id) {
                    // Settled CUSTOM / RUN_FINISHED must not blank Waiting
                    // while another unresolved form or live handoff remains.
                    self.end_turn_waiting(Some(conversation_id), Some(WAITING_FOR_YOU_STATUS));
                } else if self.has_open_approval(conversation_id) {
                    self.end_turn_waiting(Some(conversation_id), Some(WAITING_APPROVAL_STATUS));
                } else if let Some(activity) = self.thread_activity.get_mut(conversation_id) {
                    activity.label = None;
                }
            }
            ActivityTick::Set(activity) => {
                self.thread_activity
                    .entry(conversation_id.to_string())
                    .or_default()
                    .label = Some(activity.label);
            }
        }
    }

    fn this_machine_mode(&self) -> Option<LocalExecMode> {
        self.computers
            .iter()
            .find(|computer| {
                computer.this_machine
                    || self.local_exec_machine_id.as_ref() == Some(&computer.machine_id)
            })
            .map(|computer| computer.mode)
    }

    /// What this Mac's policy answers for `spec` without a card, if anything.
    fn auto_resolve_local_exec(&self, spec: &ApprovalSpec) -> Option<LocalExecResolution> {
        policy_answer(spec, self.this_machine_mode())
    }

    pub fn approval_answered(&self, call_id: &str) -> bool {
        self.approval_decisions
            .get(call_id)
            .is_some_and(ApprovalDecision::is_answered)
    }

    /// Cards in the open conversation still waiting on the person.
    pub fn open_approvals(&self) -> Vec<ApprovalSpec> {
        self.active_conversation_id
            .as_ref()
            .and_then(|id| self.conversations.iter().find(|c| &c.id == id))
            .into_iter()
            .flat_map(|c| c.messages.iter().filter(|m| !m.hidden))
            .flat_map(|m| m.parts.iter())
            .filter_map(|part| match part {
                ChatPart::Approval(spec) if !self.approval_answered(&spec.call_id) => {
                    Some(spec.clone())
                }
                _ => None,
            })
            .collect()
    }

    /// Answer a card by id. False when no such card is open.
    pub fn answer_approval_by_id(
        &mut self,
        call_id: &str,
        resolution: LocalExecResolution,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(spec) = self
            .open_approvals()
            .into_iter()
            .find(|spec| spec.call_id == call_id)
        else {
            return false;
        };
        self.answer_approval(spec, resolution, cx);
        true
    }

    /// Idle user-form cards in the open thread (fields still on screen).
    pub fn open_user_forms(&self) -> Vec<crate::opengrok::UserFormSpec> {
        self.active_conversation_id
            .as_ref()
            .and_then(|id| self.conversations.iter().find(|c| &c.id == id))
            .into_iter()
            .flat_map(|c| c.messages.iter().filter(|m| !m.hidden))
            .flat_map(|m| m.parts.iter())
            .filter_map(|part| match part {
                ChatPart::UserForm(spec) if spec.is_unresolved() => Some(spec.clone()),
                _ => None,
            })
            .collect()
    }

    /// Escalated Open-the-screen cards waiting on Take over / I'm done / Skip.
    pub fn open_computer_handoffs(&self) -> Vec<crate::opengrok::UserFormSpec> {
        self.active_conversation_id
            .as_ref()
            .and_then(|id| self.conversations.iter().find(|c| &c.id == id))
            .into_iter()
            .flat_map(|c| c.messages.iter().filter(|m| !m.hidden))
            .flat_map(|m| m.parts.iter())
            .filter_map(|part| match part {
                ChatPart::UserForm(spec)
                    if spec.live_computer_handoff()
                        && !self.user_form_handoff_done.contains(spec.card_key()) =>
                {
                    Some(spec.clone())
                }
                _ => None,
            })
            .collect()
    }

    /// Form cards that paint in the transcript (idle or settled). Computer-only
    /// stubs are omitted so E2E does not see a fake form named Computer.
    pub fn visible_user_forms(&self) -> Vec<crate::opengrok::UserFormSpec> {
        self.active_thread_user_forms()
            .into_iter()
            .filter(|spec| spec.shows_form_chrome())
            .collect()
    }

    /// Computer siblings that paint in the transcript, including Done / Skipped.
    pub fn visible_computer_handoffs(&self) -> Vec<crate::opengrok::UserFormSpec> {
        self.active_thread_user_forms()
            .into_iter()
            .filter(|spec| spec.shows_computer_handoff())
            .collect()
    }

    fn active_thread_user_forms(&self) -> Vec<crate::opengrok::UserFormSpec> {
        self.active_conversation_id
            .as_ref()
            .and_then(|id| self.conversations.iter().find(|c| &c.id == id))
            .into_iter()
            .flat_map(|c| c.messages.iter().filter(|m| !m.hidden))
            .flat_map(|m| m.parts.iter())
            .filter_map(|part| match part {
                ChatPart::UserForm(spec) => Some(spec.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn active_computer_handoff(&self) -> Option<crate::opengrok::UserFormSpec> {
        self.open_computer_handoffs().into_iter().next_back()
    }

    /// Copy for the Computer window strip. The window must not `app.read` this
    /// during Take over's first draw (`open_window` paints before the lease ends).
    pub fn computer_window_attention(&self) -> Option<(String, String)> {
        self.active_computer_handoff()
            .map(|spec| (spec.card_key().to_string(), spec.handoff_prompt()))
    }

    /// Take over: open/focus the Computer pane and the coworker's screen.
    pub fn take_over_computer(&mut self, cx: &mut Context<Self>) {
        self.show_computer_pane(cx);
        self.open_coworker_screen(cx);
    }

    /// Agent/E2E typed a field. Secrets stay in-memory; never sqlite.
    pub fn set_user_form_typed_field(
        &mut self,
        card_key: String,
        field_id: String,
        value: String,
        cx: &mut Context<Self>,
    ) {
        self.user_form_typed
            .entry(card_key)
            .or_default()
            .insert(field_id, value);
        cx.notify();
    }

    /// Agent Continue: picks, then typed overlay. Secrets stay in-memory.
    pub fn user_form_submit_values(&self, card_key: &str) -> UserFormValues {
        let mut values = UserFormValues::default();
        if let Some(picks) = self.user_form_picks.get(card_key) {
            for (id, value) in picks {
                values.by_id.insert(id.clone(), value.clone());
            }
        }
        if let Some(typed) = self.user_form_typed.get(card_key) {
            for (id, value) in typed {
                values.by_id.insert(id.clone(), value.clone());
            }
        }
        values
    }

    pub fn submit_open_user_form(&mut self, card_key: String, cx: &mut Context<Self>) {
        // The button waits while the Touch ID sheet is up; so does this.
        if self
            .saved_login_use
            .get(&card_key)
            .is_some_and(SavedLoginUse::is_busy)
        {
            return;
        }
        let values = self.user_form_submit_values(&card_key);
        self.submit_user_form(card_key, values, cx);
    }

    /// Host env (`OG_*` / `SAND_*_EGRESS_TUNNEL_ENABLED=1`) or gateway
    /// `isEgressTunnelAvailable`. Independent of box `egress_tunnel.ready`.
    pub fn host_intends_egress_tunnel(&self) -> bool {
        self.host_egress_tunnel_available || env_egress_tunnel_enabled()
    }

    fn nonempty_box_id(id: Option<&str>) -> Option<&str> {
        id.map(str::trim).filter(|id| !id.is_empty())
    }

    fn active_box_id(&self) -> Option<&str> {
        Self::nonempty_box_id(
            self.coworker_computer
                .as_ref()
                .and_then(|computer| computer.box_id.as_deref()),
        )
        .or_else(|| {
            let active = self.active_coworker_id.as_deref()?;
            Self::nonempty_box_id(
                self.coworkers
                    .iter()
                    .find(|coworker| coworker.id == active)?
                    .box_id
                    .as_deref(),
            )
        })
    }

    fn box_shared_among_coworkers(&self) -> bool {
        let Some(box_id) = self.active_box_id() else {
            return false;
        };
        self.coworkers
            .iter()
            .filter(|coworker| Self::nonempty_box_id(coworker.box_id.as_deref()) == Some(box_id))
            .count()
            > 1
    }

    /// Nested `egress_tunnel.ready` / URL on this bot's computer JSON.
    pub fn box_egress_provisioned(&self) -> bool {
        self.coworker_computer
            .as_ref()
            .is_some_and(CoworkerComputer::box_egress_provisioned)
    }

    fn box_egress_tunnel_ready(&self) -> bool {
        self.box_egress_provisioned()
    }

    /// OpenGrok #139: host setting/env **and** this bot's box tunnel. No tunnel
    /// is invented. Review-an-action uses this AND-gate; Route traffic chrome
    /// uses [`Self::route_traffic_surface`] (provisioned + shareScope).
    pub fn egress_tunnel_available(&self) -> bool {
        self.host_intends_egress_tunnel() && self.box_egress_tunnel_ready()
    }

    /// Dedicated or group → the Computer pane header (a group's computer is the group's
    /// own, like a bot's). User or org (or unknown + shared `boxId`) → Settings → Computer
    /// (an org's is set by its admin; members see it). Unprovisioned → hide.
    pub fn route_traffic_surface(&self) -> RouteTrafficSurface {
        if !self.box_egress_provisioned() {
            return RouteTrafficSurface::Hidden;
        }
        match self
            .coworker_computer
            .as_ref()
            .and_then(|computer| computer.share_scope)
        {
            Some(BoxShareScope::Dedicated) | Some(BoxShareScope::Group) => {
                RouteTrafficSurface::BotPane
            }
            Some(BoxShareScope::User) | Some(BoxShareScope::Org) => {
                RouteTrafficSurface::UserSettings
            }
            None if self.box_shared_among_coworkers() => RouteTrafficSurface::UserSettings,
            None => RouteTrafficSurface::BotPane,
        }
    }

    pub fn show_route_traffic_on_bot_pane(&self) -> bool {
        self.route_traffic_surface() == RouteTrafficSurface::BotPane
    }

    pub fn show_route_traffic_in_user_settings(&self) -> bool {
        self.route_traffic_surface() == RouteTrafficSurface::UserSettings
    }

    /// The open bot's computer's standing answer to the tunnel's card, when the server
    /// carries one. `None` hides the control (an older server, or no computer yet).
    pub fn egress_policy(&self) -> Option<LocalExecMode> {
        self.coworker_computer
            .as_ref()
            .and_then(|computer| computer.egress_policy)
    }

    /// The open bot's computer is the organization's: its network choice is the admin's to
    /// set, and the row says so.
    pub fn computer_is_org_shared(&self) -> bool {
        self.coworker_computer
            .as_ref()
            .and_then(|computer| computer.share_scope)
            == Some(BoxShareScope::Org)
    }

    /// The choice sits where Route traffic sits, and only when there is one to show.
    pub fn show_egress_policy_on_bot_pane(&self) -> bool {
        self.show_route_traffic_on_bot_pane() && self.egress_policy().is_some()
    }

    pub fn show_egress_policy_in_user_settings(&self) -> bool {
        self.show_route_traffic_in_user_settings() && self.egress_policy().is_some()
    }

    /// Set the open bot's computer's standing answer. The bot is the one whose computer record
    /// is on screen: after a back-navigation to a frame with no bot chosen the record can
    /// outlive `active_coworker_id`, and the dropdown must still do what it says.
    pub fn set_egress_policy(&mut self, mode: LocalExecMode, cx: &mut Context<Self>) {
        let Some(coworker_id) = self.active_coworker_id.clone().or_else(|| {
            self.coworker_computer
                .as_ref()
                .map(|computer| computer.agent_id.clone())
                .filter(|id| !id.is_empty())
        }) else {
            return;
        };
        self.set_egress_policy_for(coworker_id, mode, None, cx);
    }

    /// Set the standing answer for a named bot's computer — the card's own bot, which is not
    /// always the one being looked at. The pane's copy is moved at once when it is that bot's
    /// and held against any record fetch already in flight until the write has landed; then
    /// it is read back from the server. When the write fails, `card` (the answered card, if
    /// one) is re-marked as a once-only answer, so the transcript does not claim a standing
    /// choice that was not kept.
    pub fn set_egress_policy_for(
        &mut self,
        coworker_id: String,
        mode: LocalExecMode,
        card: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.shows_computer_of(&coworker_id)
            && let Some(computer) = self.coworker_computer.as_mut()
            && computer.egress_policy.is_some()
        {
            computer.egress_policy = Some(mode);
            self.egress_policy_pending = Some((coworker_id.clone(), mode));
            cx.notify();
        }
        let Some(client) = self.opengrok.clone() else {
            self.egress_policy_pending = None;
            return;
        };
        cx.spawn(async move |this, cx| {
            let outcome = client.set_egress_policy(&coworker_id, mode).await;
            let _ = this.update(cx, |state, cx| {
                // Only this write's hold: a second word picked inside the window has its own.
                if state.egress_policy_pending.as_ref() == Some(&(coworker_id.clone(), mode)) {
                    state.egress_policy_pending = None;
                }
                if let Err(error) = &outcome {
                    eprintln!(
                        "NativeChat computer: could not set the network policy: {}",
                        error.message
                    );
                    if let Some(call_id) = card {
                        state
                            .approval_decisions
                            .insert(call_id, once_only_for(mode));
                    }
                    cx.notify();
                }
                if state.shows_computer_of(&coworker_id) {
                    state.refresh_computer_record_of(coworker_id.clone(), cx);
                }
            });
        })
        .detach();
    }

    /// Whether `coworker_id`'s computer record is the one on screen: the active bot's, or the
    /// record left behind when a back-navigation cleared the active bot but not the record.
    fn shows_computer_of(&self, coworker_id: &str) -> bool {
        self.active_coworker_id.as_deref() == Some(coworker_id)
            || (self.active_coworker_id.is_none()
                && self
                    .coworker_computer
                    .as_ref()
                    .is_some_and(|computer| computer.agent_id == coworker_id))
    }

    /// A computer record fetched while a policy write is in flight must not undo the word the
    /// person just chose; the read-back after the write is what settles it. Keyed on the bot
    /// the fetch was issued for, not on the record's own `agentId`, which a server may omit.
    fn hold_pending_egress_policy(&self, fetched_for: &str, status: &mut CoworkerComputer) {
        if let Some((coworker_id, mode)) = &self.egress_policy_pending
            && coworker_id == fetched_for
            && status.egress_policy.is_some()
        {
            status.egress_policy = Some(*mode);
        }
    }

    /// The open bot's computer record, fetched for its facts alone: no heal, no poll. The
    /// Computer pane's refresh heals an absent box; a bot merely switched to must not have a
    /// box provisioned for being looked at.
    pub fn refresh_coworker_computer_quietly(&mut self, cx: &mut Context<Self>) {
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        self.refresh_computer_record_of(coworker_id, cx);
    }

    /// The same quiet fetch for a named bot, kept only while its record is the one on screen.
    fn refresh_computer_record_of(&mut self, coworker_id: String, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        if self.computer_endpoint_missing {
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = client.coworker_computer(&coworker_id).await;
            let _ = this.update(cx, |state, cx| {
                if !state.shows_computer_of(&coworker_id) {
                    return;
                }
                match result {
                    Ok(mut status) => {
                        state.hold_pending_egress_policy(&coworker_id, &mut status);
                        if state.coworker_computer.as_ref() != Some(&status) {
                            state.coworker_computer = Some(status);
                            cx.notify();
                        }
                    }
                    // The same latch the pane's refresh sets: an older server is asked once,
                    // not on every bot switch.
                    Err(error) if error.status == Some(404) => {
                        state.computer_endpoint_missing = true;
                        state.computer_poll = None;
                    }
                    Err(_) => {}
                }
            });
        })
        .detach();
    }

    pub fn set_egress_tunnel_enabled(&mut self, enabled: bool, cx: &mut Context<Self>) {
        if enabled && !self.box_egress_provisioned() {
            return;
        }
        self.egress_tunnel_enabled = enabled;
        cx.notify();
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let coworker = self.active_coworker_id.clone();
        cx.spawn(async move |this, cx| {
            // One round trip: the record comes back whole, with `egressTunnelAvailable` for
            // this coworker already on it.
            let result = client
                .patch_host_settings(
                    coworker.as_deref(),
                    &serde_json::json!({ "egressTunnelEnabled": enabled }),
                )
                .await;
            let _ = this.update(cx, |state, cx| {
                if let Ok(settings) = result {
                    state.take_host_egress(&settings);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Read the host's settings record from the AG-UI door — the toggle's intent and whether
    /// the tunnel is live for the open coworker.
    pub fn refresh_host_egress(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let coworker = self.active_coworker_id.clone();
        cx.spawn(async move |this, cx| {
            let settings = client.host_settings(coworker.as_deref()).await.ok();
            let _ = this.update(cx, |state, cx| {
                // The answer is about the bot it was asked for; a slow answer for a bot the
                // person has since left must not paint the next bot's tunnel state.
                if state.active_coworker_id != coworker {
                    return;
                }
                if let Some(settings) = settings
                    && state.take_host_egress(&settings)
                {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The two egress facts off a host-settings record. A key the host did not send changes
    /// nothing. Returns whether anything changed.
    fn take_host_egress(&mut self, settings: &serde_json::Value) -> bool {
        let mut changed = false;
        if let Some(available) = host_egress_tunnel_available(settings)
            && self.host_egress_tunnel_available != available
        {
            self.host_egress_tunnel_available = available;
            changed = true;
        }
        if let Some(enabled) = host_egress_tunnel_flag(settings)
            && self.egress_tunnel_enabled != enabled
        {
            self.egress_tunnel_enabled = enabled;
            changed = true;
        }
        changed
    }

    pub fn approval_status_line(&self, spec: &ApprovalSpec, bot: &str) -> Option<String> {
        if let Some(line) = self
            .approval_decisions
            .get(&spec.call_id)
            .and_then(|decision| decision.outcome_line(bot, spec))
        {
            return Some(line);
        }
        self.auto_resolve_local_exec(spec)
            .map(|resolution| spec.outcome(bot, resolution))
    }

    /// This thread's turn is over, either for good or until somebody answers a card.
    ///
    /// Only this thread's. The guard that used to stand here — "is the ending's bot still the
    /// one the app thinks is responding" — was an app-wide field's only defence against one
    /// bot's ending clearing another's line, and it leaked both ways. Keyed by thread there is
    /// nothing to guard: A's ending can no more reach B's line than B's frames can reach A's.
    fn finish_responding(&mut self, conversation_id: Option<&str>, waiting_approval: bool) {
        self.end_turn_waiting(
            conversation_id,
            waiting_approval.then_some(WAITING_APPROVAL_STATUS),
        );
    }

    fn end_turn_waiting(&mut self, conversation_id: Option<&str>, waiting: Option<&str>) {
        let Some(conversation_id) = conversation_id else {
            return;
        };
        if let Some(label) = waiting {
            let activity = self
                .thread_activity
                .entry(conversation_id.to_string())
                .or_default();
            activity.responding = false;
            activity.label = Some(label.to_string());
        } else {
            self.thread_activity.remove(conversation_id);
        }
    }

    /// Server run is finished; the person holds a form. Mirror that: chrome
    /// **Waiting for you**, drop the local live-turn (Stop only while a run
    /// is actually in flight). The open form lives on OpenGrok and comes
    /// back from replay after quit/relaunch.
    fn park_waiting_for_you(&mut self, conversation_id: &str, run_id: &str) {
        self.end_turn_waiting(Some(conversation_id), Some(WAITING_FOR_YOU_STATUS));
        if !run_id.is_empty() {
            self.release_live_turn(conversation_id, run_id);
        }
    }

    /// Open user-form / approval / live handoff, or Waiting chrome: something
    /// is parked on the person. A send here is a steer.
    fn has_hitl_to_interrupt(&self, conversation_id: &str) -> bool {
        self.has_open_approval(conversation_id)
            || self.has_open_user_form(conversation_id)
            || matches!(
                self.thread_status(conversation_id),
                Some(WAITING_FOR_YOU_STATUS) | Some(WAITING_APPROVAL_STATUS)
            )
    }

    /// The thread as the composer finds it. Parked wins over running: a card
    /// painted mid-stream is already waiting on the person even though the
    /// stream that painted it is still open.
    fn busy_state(&self, conversation_id: &str) -> Busy {
        if self.has_sending_form(conversation_id) {
            // A submit is on the wire and the run resumes the moment it lands: the thread
            // is as good as running. Queue behind it rather than settle a card the server
            // is about to answer.
            Busy::Running
        } else if self.has_hitl_to_interrupt(conversation_id) {
            Busy::Parked
        } else if self.is_thread_responding(conversation_id)
            || self.is_turn_in_flight_for(conversation_id)
        {
            Busy::Running
        } else {
            Busy::Idle
        }
    }

    fn has_sending_form(&self, conversation_id: &str) -> bool {
        self.conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
            .is_some_and(|conversation| {
                conversation
                    .messages
                    .iter()
                    .flat_map(|message| message.parts.iter())
                    .any(|part| match part {
                        ChatPart::UserForm(spec) => {
                            spec.effective_resolution() == Some(FormResolution::Sending)
                        }
                        _ => false,
                    })
            })
    }

    /// A run registered against this thread that is still running, whether or
    /// not it is the open thread: the composer's button reads the open thread
    /// only, but a steer decides about the thread the message is for.
    fn is_turn_in_flight_for(&self, conversation_id: &str) -> bool {
        self.live_turns
            .get(conversation_id)
            .is_some_and(|turn| !turn.persisting)
            && self.thread_status(conversation_id) != Some(WAITING_FOR_YOU_STATUS)
    }

    /// A steer over parked cards. The server ends the parked run and closes
    /// its cards when the next message arrives (`interrupt_parked_hitl` on
    /// `POST /ag-ui`), so the app must not `/stop` it first — a stopped run
    /// is not parked, the server then finds nothing to settle, and the card
    /// keeps the coworker's screen held for ten minutes. What the app does
    /// is paint the same ending here: every open card in the thread becomes
    /// **Superseded**, so nothing keeps reading the thread as parked and the
    /// Waiting chrome can go.
    fn settle_parked_cards(&mut self, conversation_id: &str) {
        let Some(conversation) = self
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return;
        };
        let mut forms = Vec::new();
        let mut approvals = Vec::new();
        for part in conversation
            .messages
            .iter()
            .flat_map(|message| message.parts.iter())
        {
            match part {
                // Not a form mid-`Sending`: its submit is on the wire and the reply repaints
                // it; `busy_state` reads such a thread as running, not parked.
                ChatPart::UserForm(spec)
                    if spec.is_unresolved() || spec.live_computer_handoff() =>
                {
                    forms.push((spec.card_key().to_string(), spec.live_computer_handoff()));
                }
                ChatPart::Approval(spec)
                    if !self.approval_answered(&spec.call_id)
                        && self.auto_resolve_local_exec(spec).is_none() =>
                {
                    approvals.push(spec.call_id.clone());
                }
                _ => {}
            }
        }
        for (card_key, handoff) in forms {
            self.saved_login_use.remove(&card_key);
            self.paint_user_form_resolution(&card_key, FormResolution::Superseded);
            if handoff {
                self.set_computer_handoff(&card_key, ComputerHandoffStatus::Skipped);
            }
        }
        for call_id in approvals {
            self.approval_decisions
                .insert(call_id, ApprovalDecision::Superseded);
        }
        self.sync_waiting_chrome(conversation_id);
    }

    /// After Skip / Dismiss / Done (and after SSE graft): Waiting only while
    /// an unresolved form or live Computer sibling remains. Do not leave
    /// green Waiting over settled pills, and do not blank it while another
    /// stacked card is still open.
    fn sync_waiting_chrome(&mut self, conversation_id: &str) {
        if self.is_thread_responding(conversation_id) {
            return;
        }
        if self.has_open_user_form(conversation_id) {
            self.end_turn_waiting(Some(conversation_id), Some(WAITING_FOR_YOU_STATUS));
            return;
        }
        if self.has_open_approval(conversation_id) {
            self.end_turn_waiting(Some(conversation_id), Some(WAITING_APPROVAL_STATUS));
            return;
        }
        if matches!(
            self.thread_status(conversation_id),
            Some(WAITING_FOR_YOU_STATUS) | Some(WAITING_APPROVAL_STATUS)
        ) {
            self.end_turn_waiting(Some(conversation_id), None);
        }
    }

    pub fn login(&mut self, email: String, password: String, cx: &mut Context<Self>) {
        if self.auth_status == AuthStatus::SigningIn {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        self.login_epoch += 1;
        let epoch = self.login_epoch;
        self.auth_status = AuthStatus::SigningIn;
        self.auth_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = match client.login(&email, &password).await {
                Ok(()) => client.me().await,
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |state, cx| {
                if state.login_epoch != epoch {
                    return;
                }
                match result {
                    Ok(account) => {
                        state.account = Some(account);
                        state.auth_status = AuthStatus::SignedIn;
                        state.auth_error = None;
                        // The one thing that clears a session the server had stopped
                        // recognising, and the reason the banner asks for it by name.
                        state.session.signed_in();
                        // The vault syncs with the server the moment there is a person to
                        // sync for.
                        state.site_login_notice = None;
                        state.reload_site_logins(cx);
                        state.note_server_answered(cx);
                        state.start_local_exec(cx);
                        state.refresh_coworkers(cx);
                        state.refresh_computers(cx);
                        state.refresh_host_egress(cx);
                        state.sync_pending_approvals(cx);
                    }
                    Err(error) => {
                        state.account = None;
                        state.auth_status = AuthStatus::SignedOut;
                        // A password the server never saw was not a wrong password: the failure
                        // goes to the indicator, not under the fields.
                        state.note_failure(&error, cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn logout(&mut self, cx: &mut Context<Self>) {
        let client = self.opengrok.clone();
        self.account = None;
        self.auth_status = AuthStatus::SignedOut;
        self.saved_login_use.clear();
        self.pending_save.clear();
        self.auth_error = None;
        // A person who has just signed out is not somebody who needs telling they are signed
        // out. The banner is for the case where the app believed otherwise.
        self.session.signed_in();
        self.coworkers.clear();
        self.last_active_at.clear();
        self.active_coworker_id = None;
        self.thread_activity.clear();
        self.is_app_settings_open = false;
        self.bot_finder_open = false;
        self.command_palette_open = false;
        self.close_right_pane(cx);
        self.stop_local_exec();
        self.approval_decisions.clear();
        self.computers.clear();
        self.host_egress_tunnel_available = false;
        self.egress_tunnel_enabled = true;
        self.coworker_computer = None;
        cx.notify();
        if let Some(client) = client {
            cx.spawn(async move |_, _| {
                let _ = client.logout().await;
            })
            .detach();
        }
    }

    pub fn refresh_coworkers(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.list_coworkers().await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(list) => {
                        state.coworkers = list;
                        state.hidden_coworker_ids = state
                            .coworkers
                            .iter()
                            .filter(|c| c.hidden_from_sidebar)
                            .map(|c| c.id.clone())
                            .collect();
                        if state
                            .active_coworker_id
                            .as_ref()
                            .is_none_or(|id| !state.coworkers.iter().any(|c| &c.id == id))
                        {
                            if let Some(first) = state
                                .ranked_coworkers()
                                .into_iter()
                                .find(|c| !state.hidden_coworker_ids.contains(&c.id))
                            {
                                state.select_coworker(first.id, cx);
                            }
                        } else {
                            // The open bot did not change, so nothing above went and asked for
                            // its routines. A roster fetched after a sign-in or a spell out of
                            // reach is exactly when they are worth asking for again.
                            state.load_routines(cx);
                        }
                        // The roster is a server route and says nothing about the gateway, so
                        // this clears the server's state only.
                        state.note_server_answered(cx);
                    }
                    Err(error) => state.note_failure(&error, cx),
                }
                cx.notify();
            });
        })
        .detach();
        self.refresh_models(cx);
    }

    pub fn refresh_models(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.list_models().await;
            let _ = this.update(cx, |state, cx| {
                state.settle_models(result, cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// What a `/models` answer means — for the Model field, and for whether the gateway is there.
    ///
    /// This is also the reconnect loop's probe, which is why it answers whether everything is
    /// reachable now: one request settles both questions, and its success *is* the refill.
    fn settle_models(
        &mut self,
        result: Result<ModelCatalogue, OpenGrokError>,
        cx: &mut Context<Self>,
    ) -> bool {
        match result {
            Ok(catalogue) => {
                let note = apply_catalogue(&mut self.model_catalogue, catalogue);
                match note.filter(|note| reads_as_gateway_unreachable(note)) {
                    Some(note) => {
                        self.note_unreachable(Unreachable::Gateway, &note, cx);
                        false
                    }
                    // Models, from the gateway, through the server: the whole path worked.
                    None => {
                        self.note_gateway_answered(cx);
                        true
                    }
                }
            }
            Err(error) => {
                self.note_failure(&error, cx);
                error.unreachable().is_none()
            }
        }
    }

    /// Every failure the app hears about is sorted here, once, so that no call site has to guess.
    ///
    /// A refusal is a verdict about what was asked, and it goes where verdicts have always gone.
    /// A transport failure is not about what was asked at all — it is the state of the wire, it
    /// stops being true on its own, and it starts the app asking again. A session that has gone
    /// is the third thing: the wire is fine, nothing was decided about the request, and it goes
    /// to the one state a retry loop cannot help with.
    fn note_failure(&mut self, error: &OpenGrokError, cx: &mut Context<Self>) {
        // The session reads every failure for itself and takes only the one kind that is about
        // who the app is; see `Session::note` for why the other two must leave it alone.
        if self.session.note(error.failure()) {
            cx.notify();
        }
        match error.failure() {
            Failure::OutOfReach(what) => self.note_unreachable(what, &error.message, cx),
            // Nothing in `auth_error`, because this is not a verdict about what was asked, and
            // no retry loop, because there is no question left to ask — by the time a 401 gets
            // here the client has already spent its one refresh on it. The 401 does clear the
            // server's own reachability state: an answer, any answer, is proof of the wire.
            Failure::SignedOut => self.note_server_answered(cx),
            Failure::Verdict => {
                self.auth_error = Some(error.message.clone());
                self.note_server_answered(cx);
            }
        }
    }

    /// The app looked in the jar before sending and found nothing to send with.
    ///
    /// The same state a `401` puts it in, reached without the round trip that would have
    /// produced one.
    fn note_signed_out(&mut self, cx: &mut Context<Self>) {
        if self.session.note(Failure::SignedOut) {
            cx.notify();
        }
    }

    fn note_unreachable(&mut self, what: Unreachable, detail: &str, cx: &mut Context<Self>) {
        if self.reachability.fail(what, detail) {
            cx.notify();
        }
        self.start_reconnect(cx);
    }

    /// The server answered, whatever it answered. Clears the server's own state and nothing else.
    fn note_server_answered(&mut self, cx: &mut Context<Self>) {
        if self.reachability.server_answered() {
            self.came_back(cx);
        }
    }

    /// Something came through the whole path, gateway included.
    fn note_gateway_answered(&mut self, cx: &mut Context<Self>) {
        if self.reachability.all_clear() {
            self.came_back(cx);
        }
    }

    /// Everything is reachable again, having not been a moment ago.
    ///
    /// Reached only on the transition, so the refill below happens once rather than on every
    /// request that succeeds. The roster is what a spell out of reach may have emptied, and
    /// asking for it asks for the catalogue too — so the Model field fills again without anybody
    /// going and looking at it.
    fn came_back(&mut self, cx: &mut Context<Self>) {
        self.reconnecting = false;
        self.reconnect_epoch += 1;
        if self.is_signed_in() {
            self.refresh_coworkers(cx);
        }
        cx.notify();
    }

    /// Ask again, waiting longer each time, until something answers.
    ///
    /// The probe is `/models`: it is the one request that answers both questions at once — a
    /// failure at the socket is the server, a `200` carrying the gateway's reason is the gateway,
    /// and a list of models is everything working — and its success is itself the refill the
    /// Model field needs. A refusal (a `401`, a `500`) also ends the loop, because a server that
    /// refuses is a server that is being reached; what it refused is not this loop's business.
    fn start_reconnect(&mut self, cx: &mut Context<Self>) {
        if self.reconnecting {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        self.reconnecting = true;
        self.reconnect_epoch += 1;
        let epoch = self.reconnect_epoch;
        cx.spawn(async move |this, cx| {
            loop {
                let Ok(Some(wait)) = this.update(cx, |state, _| {
                    (state.reconnect_epoch == epoch).then(|| state.reachability.wait())
                }) else {
                    return;
                };
                cx.background_executor().timer(wait).await;
                let result = client.list_models().await;
                let Ok(reachable) = this.update(cx, |state, cx| {
                    if state.reconnect_epoch != epoch {
                        return true;
                    }
                    let reachable = state.settle_models(result, cx);
                    cx.notify();
                    reachable
                }) else {
                    return;
                };
                if reachable {
                    return;
                }
            }
        })
        .detach();
    }

    /// The two lines the reconnecting indicator shows, or `None` while everything answers.
    pub fn reachability_indicator(&self) -> Option<(String, String)> {
        self.reachability.indicator()
    }

    /// Which machine the app cannot reach, if it is failing to reach one.
    pub fn unreachable(&self) -> Option<Unreachable> {
        self.reachability.unreachable()
    }

    /// The two lines the signed-out banner shows, or `None` while the session holds.
    pub fn session_banner(&self) -> Option<(String, String)> {
        self.session.banner()
    }

    /// The app has been told its session is gone.
    pub fn is_session_expired(&self) -> bool {
        self.session.is_expired()
    }

    /// Whether the app holds something it could send a turn with.
    ///
    /// Asked before a turn is built rather than after it has failed, which is the whole point:
    /// the jar is right here and the answer costs nothing, where the same answer from the server
    /// costs a round trip and arrives as a line in somebody's transcript.
    fn can_send_turn(&self) -> bool {
        let holds_credential = self
            .opengrok
            .as_ref()
            .is_some_and(OpenGrokClient::has_session);
        self.session.may_send(holds_credential)
    }

    /// Take the person to the sign-in page, which is what the banner's button does.
    ///
    /// The app does not do this to them the moment the 401 lands. Their thread is on screen and
    /// still worth reading, and yanking it away to a login form is most of what a relaunch did.
    /// So the banner waits, and this runs when they say so — at which point the app is plainly
    /// signed out rather than signed in with nothing to show for it, and the sign-in page is
    /// what the shell draws for that.
    pub fn sign_in_again(&mut self, cx: &mut Context<Self>) {
        self.logout(cx);
    }

    pub fn is_right_pane_open(&self) -> bool {
        self.right_pane != RightPane::Closed
    }

    pub fn is_agent_settings_open(&self) -> bool {
        self.right_pane == RightPane::Settings
    }

    /// How often the open Computer pane asks after the coworker's box.
    const COMPUTER_POLL: Duration = Duration::from_secs(2);

    /// Every change of the right pane goes through here, so the Computer poll
    /// runs exactly while that pane is open.
    fn set_right_pane(&mut self, pane: RightPane, cx: &mut Context<Self>) {
        let computer = pane == RightPane::Computer;
        self.right_pane = pane;
        self.computer_confirm = None;
        self.computer_action_error = None;
        if computer {
            self.refresh_coworker_computer(cx);
            self.refresh_host_egress(cx);
            self.start_computer_poll(cx);
        } else {
            self.computer_poll = None;
        }
        if pane == RightPane::Settings {
            // Looking at the Model field asks again. The catalogue is fetched once at startup
            // and one failed fetch used to be the whole of it for the life of the process; the
            // reconnect loop refills it unvisited now, and this is the other half — somebody
            // who opens the pane to see why it is empty gets a fresh answer for opening it.
            self.refresh_models(cx);
            // A dedicated box's network choice lives on this sidebar and reads the computer
            // record; opening the sidebar is the moment to make sure it is this bot's and fresh.
            self.refresh_coworker_computer_quietly(cx);
        }
    }

    pub fn close_right_pane(&mut self, cx: &mut Context<Self>) {
        if self.right_pane == RightPane::Closed {
            return;
        }
        self.set_right_pane(RightPane::Closed, cx);
        self.computer_view = ComputerView::Overview;
        self.model_picker_open = false;
        self.avatar_editor_open = false;
        self.record_nav();
        cx.notify();
    }

    pub fn show_agent_settings(&mut self, cx: &mut Context<Self>) {
        self.ensure_active_coworker(cx);
        self.set_right_pane(RightPane::Settings, cx);
        self.computer_view = ComputerView::Overview;
        self.record_nav();
        cx.notify();
    }

    pub fn show_computer_pane(&mut self, cx: &mut Context<Self>) {
        self.ensure_active_coworker(cx);
        self.set_right_pane(RightPane::Computer, cx);
        self.computer_view = ComputerView::Overview;
        self.refresh_coworker_screen(cx);
        self.record_nav();
        cx.notify();
    }

    pub fn toggle_agent_settings(&mut self, cx: &mut Context<Self>) {
        if self.right_pane == RightPane::Settings {
            self.close_right_pane(cx);
            return;
        }
        self.set_right_pane(RightPane::Settings, cx);
        self.computer_view = ComputerView::Overview;
        self.record_nav();
        cx.notify();
    }

    pub fn toggle_computer_pane(&mut self, cx: &mut Context<Self>) {
        if self.right_pane == RightPane::Computer {
            self.close_right_pane(cx);
            return;
        }
        self.set_right_pane(RightPane::Computer, cx);
        self.computer_view = ComputerView::Overview;
        self.model_picker_open = false;
        self.avatar_editor_open = false;
        self.record_nav();
        cx.notify();
    }

    /// Keep the Computer pane's tile current while it is open. Idempotent; the
    /// task is dropped by `set_right_pane` when the pane closes.
    fn start_computer_poll(&mut self, cx: &mut Context<Self>) {
        if self.computer_poll.is_some() {
            return;
        }
        self.computer_poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Self::COMPUTER_POLL).await;
                let alive = this.update(cx, |state, cx| {
                    if state.right_pane == RightPane::Computer {
                        state.refresh_coworker_computer(cx);
                        state.refresh_coworker_screen(cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        }));
    }

    pub fn refresh_coworker_computer(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            self.coworker_computer = None;
            self.coworker_screen = None;
            return;
        };
        if self.computer_endpoint_missing {
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = client.coworker_computer(&coworker_id).await;
            let _ = this.update(cx, |state, cx| {
                // A late answer for a bot the person has since left is stale.
                if state.active_coworker_id.as_deref() != Some(coworker_id.as_str()) {
                    return;
                }
                match result {
                    Ok(mut status) => {
                        state.hold_pending_egress_policy(&coworker_id, &mut status);
                        // A bot with no computer gets one: ask once per visit, and let the
                        // next poll pick up the answer. A recorded error is the server saying
                        // it cannot, so that is left alone.
                        let absent = status.state == "absent" && !status.updating();
                        if state.coworker_computer.as_ref() != Some(&status) {
                            state.coworker_computer = Some(status);
                            cx.notify();
                        }
                        if absent
                            && state.computer_heal_requested.as_deref()
                                != Some(coworker_id.as_str())
                        {
                            state.computer_heal_requested = Some(coworker_id.clone());
                            state.ensure_coworker_computer(cx);
                        }
                    }
                    Err(error) if error.status == Some(404) => {
                        eprintln!(
                            "NativeChat computer: this server has no /coworkers/{{id}}/computer; not polling"
                        );
                        state.computer_endpoint_missing = true;
                        state.computer_poll = None;
                    }
                    Err(error) => {
                        // Say it once, when the status is lost, not every two seconds.
                        if state.coworker_computer.take().is_some() {
                            eprintln!("NativeChat computer: {}", error.message);
                            cx.notify();
                        }
                    }
                }
            });
        })
        .detach();
    }

    /// Fetch the screen for the tile. Only while the status says there is one — a headless or
    /// stopped box is not asked, and a 404 (no screen after all) just clears the picture.
    pub fn refresh_coworker_screen(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        let has_screen = self
            .coworker_computer
            .as_ref()
            .and_then(|status| status.vnc_url())
            .is_some();
        if !has_screen {
            if self.coworker_screen.take().is_some() {
                cx.notify();
            }
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = client.coworker_screen(&coworker_id).await;
            let _ = this.update(cx, |state, cx| {
                if state.active_coworker_id.as_deref() != Some(coworker_id.as_str()) {
                    return;
                }
                match result {
                    Ok(frame) => {
                        let next =
                            crate::opengrok::ScreenshotSpec::from_frame("screen", "", &frame)
                                .map(|spec| spec.image);
                        let unchanged = match (state.coworker_screen.as_ref(), next.as_ref()) {
                            (Some(old), Some(new)) => old.bytes == new.bytes,
                            (None, None) => true,
                            _ => false,
                        };
                        if unchanged {
                            return;
                        }
                        state.coworker_screen = next;
                        cx.notify();
                    }
                    Err(_) => {
                        if state.coworker_screen.take().is_some() {
                            cx.notify();
                        }
                    }
                }
            });
        })
        .detach();
    }

    /// Ask the server to (re)provision the active coworker's computer, and take the answer as
    /// the current status.
    pub fn ensure_coworker_computer(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.ensure_coworker_computer(&coworker_id).await;
            let _ = this.update(cx, |state, cx| {
                if state.active_coworker_id.as_deref() != Some(coworker_id.as_str()) {
                    return;
                }
                match result {
                    Ok(mut status) => {
                        state.hold_pending_egress_policy(&coworker_id, &mut status);
                        state.coworker_computer = Some(status);
                        cx.notify();
                    }
                    Err(error) => eprintln!(
                        "NativeChat computer: could not provision the computer of {coworker_id}: {}",
                        error.message
                    ),
                }
            });
        })
        .detach();
    }

    /// Ask before acting on the active bot's computer: the dialog over the app.
    pub fn open_computer_confirm(&mut self, action: ComputerAction, cx: &mut Context<Self>) {
        if self.active_coworker_id.is_none() {
            return;
        }
        self.computer_action_error = None;
        self.computer_confirm = Some(action);
        cx.notify();
    }

    pub fn close_computer_confirm(&mut self, cx: &mut Context<Self>) {
        if self.computer_confirm.take().is_some() {
            cx.notify();
        }
    }

    /// The shield badge on the Computer pane: open the dialog that picks how this bot's own
    /// computer may use the person's network. Only when there is a choice to make.
    pub fn open_network_policy(&mut self, cx: &mut Context<Self>) {
        if !self.show_egress_policy_on_bot_pane() {
            return;
        }
        self.network_policy_open = true;
        cx.notify();
    }

    pub fn close_network_policy(&mut self, cx: &mut Context<Self>) {
        if self.network_policy_open {
            self.network_policy_open = false;
            cx.notify();
        }
    }

    /// A word picked in the dialog: kept on the server, and the dialog closes.
    pub fn pick_network_policy(&mut self, mode: LocalExecMode, cx: &mut Context<Self>) {
        self.set_egress_policy(mode, cx);
        self.close_network_policy(cx);
    }

    /// The dialog's Confirm: do what it asked, then close it.
    pub fn confirm_computer_action(&mut self, cx: &mut Context<Self>) {
        let Some(action) = self.computer_confirm.take() else {
            return;
        };
        match action {
            ComputerAction::Update => self.start_computer_update(cx),
            ComputerAction::Reset => self.start_computer_reset(cx),
        }
        cx.notify();
    }

    fn start_computer_update(&mut self, cx: &mut Context<Self>) {
        self.computer_action(cx, |client, id| {
            Box::pin(async move { client.update_coworker_computer(&id).await })
        });
    }

    fn start_computer_reset(&mut self, cx: &mut Context<Self>) {
        self.computer_action(cx, |client, id| {
            Box::pin(async move { client.reset_coworker_computer(&id).await })
        });
    }

    /// Run one computer action for the active coworker and take its answer as the status; a
    /// refusal is shown under the buttons. The poll carries the phases after that.
    fn computer_action(
        &mut self,
        cx: &mut Context<Self>,
        action: impl FnOnce(
            OpenGrokClient,
            String,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<CoworkerComputer, OpenGrokError>> + Send>,
        > + 'static,
    ) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        let future = action(client, coworker_id.clone());
        cx.spawn(async move |this, cx| {
            let result = future.await;
            let _ = this.update(cx, |state, cx| {
                if state.active_coworker_id.as_deref() != Some(coworker_id.as_str()) {
                    return;
                }
                match result {
                    Ok(mut status) => {
                        state.hold_pending_egress_policy(&coworker_id, &mut status);
                        state.coworker_computer = Some(status);
                        state.coworker_screen = None;
                    }
                    Err(error) => state.computer_action_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The banner over the app while the active coworker's computer is being updated: the
    /// title and what is happening now. `None` when nothing is.
    pub fn computer_banner(&self) -> Option<(String, String)> {
        let update = self.coworker_computer.as_ref()?.update.as_ref()?;
        let name = self.active_bot_name();
        if update.in_flight() {
            Some((format!("Updating {name}'s computer"), update.detail()))
        } else {
            Some((
                format!("Could not update {name}'s computer"),
                update.detail(),
            ))
        }
    }

    /// The Recipes page in the main slot, with the list for the current filter. A docked pane
    /// steps aside: a table of steps wants the whole width left of the sidebar, and the pane
    /// is one click away in the title bar.
    pub fn open_recipes(&mut self, cx: &mut Context<Self>) {
        self.dismiss_popovers(cx);
        self.close_right_pane(cx);
        if self.page != MainPage::Recipes {
            self.page = MainPage::Recipes;
            self.record_nav();
        }
        self.refresh_recipes(cx);
        cx.notify();
    }

    /// The window that draws the pages, for a second window to hand work to.
    pub fn set_main_window(&mut self, window: AnyWindowHandle) {
        self.main_window = Some(window);
    }

    /// The Recipes page, asked for from another window (a coworker's screen). The page is
    /// drawn in the main window, so that window comes forward with it; `recipe` opens one
    /// recipe's own page rather than the list.
    pub fn show_recipes_in_main_window(&mut self, recipe: Option<String>, cx: &mut Context<Self>) {
        cx.activate(true);
        if let Some(window) = self.main_window {
            let _ = window.update(cx, |_, window, _| window.activate_window());
        }
        self.open_recipes(cx);
        if let Some(id) = recipe {
            self.open_recipe(id, cx);
        }
    }

    /// Back to the chat from a page.
    pub fn show_chat(&mut self, cx: &mut Context<Self>) {
        if self.page == MainPage::Chat {
            return;
        }
        self.page = MainPage::Chat;
        self.record_nav();
        cx.notify();
    }

    pub fn set_recipes_filter(&mut self, filter: RecipeFilter, cx: &mut Context<Self>) {
        if self.recipes_filter == filter {
            return;
        }
        self.recipes_filter = filter;
        self.refresh_recipes(cx);
        cx.notify();
    }

    /// Load the list for the current filter. A late answer for an earlier request is dropped,
    /// so switching chips quickly never shows the wrong list.
    ///
    /// ONE FETCH FOR BOTH KINDS. The listing takes `?kind=recipe|workflow` and is never asked
    /// for one: the server builds every row either way and the query only drops some of them
    /// afterwards, so asking twice would be two round trips for one answer — and two answers in
    /// flight at once, which is a `/` panel that fills in twice while somebody reads it. What a
    /// row is rides on the row, and each surface reads it: `/` shows both kinds, and the Recipes
    /// page keeps to recipes.
    pub fn refresh_recipes(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        self.recipes_epoch += 1;
        let epoch = self.recipes_epoch;
        let filter = self.recipes_filter;
        self.recipes_loading = true;
        self.recipes_error = None;
        cx.spawn(async move |this, cx| {
            let result = client.list_recipes(Some(filter.query())).await;
            let _ = this.update(cx, |state, cx| {
                if state.recipes_epoch != epoch {
                    return;
                }
                state.recipes_loading = false;
                match result {
                    Ok(recipes) => state.recipes = recipes,
                    Err(error) => state.recipes_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The detail view for one recipe.
    pub fn open_recipe(&mut self, id: String, cx: &mut Context<Self>) {
        self.recipe_open = None;
        self.recipe_open_id = Some(id);
        self.recipe_busy = None;
        self.recipe_error = None;
        self.recipe_run_result = None;
        self.recipe_delete_confirm = false;
        self.load_open_recipe(cx);
        cx.notify();
    }

    /// Fetch the open recipe, keeping whatever is shown until the answer lands.
    fn load_open_recipe(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(id) = self.recipe_open_id.clone() else {
            return;
        };
        self.recipe_loading = true;
        cx.spawn(async move |this, cx| {
            let result = client.recipe(&id).await;
            let _ = this.update(cx, |state, cx| {
                // A late answer for a recipe the person has since left is stale.
                if state.recipe_open_id.as_deref() != Some(id.as_str()) {
                    return;
                }
                state.recipe_loading = false;
                match result {
                    Ok(detail) => state.set_open_recipe(detail),
                    Err(error) => state.recipe_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Take a detail as the open recipe, keeping what its newest run came to: the list is told
    /// nothing about runs, so this is the only place the app learns it.
    fn set_open_recipe(&mut self, detail: RecipeDetail) {
        if let Some(run) = detail.runs.first() {
            self.recipe_last_runs.insert(
                detail.recipe.id.clone(),
                RecipeRunNote {
                    ok: run.ok,
                    version: run.version,
                    stopped_at: run.stopped_at,
                    at_ms: run.at_ms,
                },
            );
        }
        self.recipe_open = Some(detail);
    }

    pub fn close_recipe(&mut self, cx: &mut Context<Self>) {
        self.recipe_open = None;
        self.recipe_open_id = None;
        self.recipe_loading = false;
        self.recipe_busy = None;
        self.recipe_error = None;
        self.recipe_run_result = None;
        self.recipe_delete_confirm = false;
        cx.notify();
    }

    /// Run one request on the open recipe and take its answer as the detail; a refusal is
    /// shown on the page. The list is reloaded too, since names and versions show there.
    fn recipe_action(
        &mut self,
        busy: &str,
        cx: &mut Context<Self>,
        action: impl FnOnce(
            OpenGrokClient,
            String,
        ) -> std::pin::Pin<
            Box<dyn std::future::Future<Output = Result<RecipeDetail, OpenGrokError>> + Send>,
        > + 'static,
    ) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(id) = self.recipe_open_id.clone() else {
            return;
        };
        self.recipe_busy = Some(busy.to_string());
        self.recipe_error = None;
        cx.notify();
        let future = action(client, id.clone());
        cx.spawn(async move |this, cx| {
            let result = future.await;
            let _ = this.update(cx, |state, cx| {
                if state.recipe_open_id.as_deref() != Some(id.as_str()) {
                    return;
                }
                state.recipe_busy = None;
                match result {
                    Ok(detail) => {
                        state.set_open_recipe(detail);
                        state.refresh_recipes(cx);
                    }
                    Err(error) => state.recipe_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn rename_open_recipe(
        &mut self,
        name: String,
        description: String,
        cx: &mut Context<Self>,
    ) {
        self.recipe_action("Saving…", cx, move |client, id| {
            Box::pin(async move { client.rename_recipe(&id, &name, &description).await })
        });
    }

    /// The edited steps as the open recipe's next version.
    pub fn add_recipe_version(
        &mut self,
        steps: Vec<RecipeStep>,
        note: String,
        cx: &mut Context<Self>,
    ) {
        self.recipe_action("Saving…", cx, move |client, id| {
            Box::pin(async move { client.add_recipe_version(&id, &steps, &note).await })
        });
    }

    /// Remove one edited version of the open recipe. The server answers the delete with
    /// nothing in particular, so the detail is fetched again to see what is left; a refusal —
    /// the raw and the filtered version cannot go — lands on the page's error line.
    pub fn delete_recipe_version(&mut self, version: u32, cx: &mut Context<Self>) {
        self.recipe_action("Deleting version…", cx, move |client, id| {
            Box::pin(async move {
                client.delete_recipe_version(&id, version).await?;
                client.recipe(&id).await
            })
        });
    }

    pub fn share_open_recipe(&mut self, target: RecipeShareTarget, cx: &mut Context<Self>) {
        self.recipe_action("Sharing…", cx, move |client, id| {
            Box::pin(async move { client.share_recipe(&id, &target).await })
        });
    }

    pub fn unshare_open_recipe(&mut self, scope: String, scope_id: String, cx: &mut Context<Self>) {
        self.recipe_action("Unsharing…", cx, move |client, id| {
            Box::pin(async move { client.unshare_recipe(&id, &scope, &scope_id).await })
        });
    }

    /// Let one of the person's bots run the open recipe, or take that back.
    pub fn set_recipe_grant(&mut self, coworker_id: String, granted: bool, cx: &mut Context<Self>) {
        self.recipe_action("Updating bots…", cx, move |client, id| {
            Box::pin(async move {
                if granted {
                    client.grant_recipe(&id, &coworker_id).await
                } else {
                    client.revoke_recipe_grant(&id, &coworker_id).await
                }
            })
        });
    }

    /// Select all and Deselect all in the Bots picker: the same grant or revoke for several
    /// bots. The requests go one after another rather than together, because each answers with
    /// the whole detail and answers that raced would leave the page showing an older set.
    pub fn set_recipe_grants(
        &mut self,
        coworker_ids: Vec<String>,
        granted: bool,
        cx: &mut Context<Self>,
    ) {
        if coworker_ids.is_empty() {
            return;
        }
        self.recipe_action("Updating bots…", cx, move |client, id| {
            Box::pin(async move {
                let mut last = None;
                for coworker_id in coworker_ids {
                    let result = if granted {
                        client.grant_recipe(&id, &coworker_id).await
                    } else {
                        client.revoke_recipe_grant(&id, &coworker_id).await
                    };
                    // One refusal stops the rest: the detail it would have answered with is
                    // no longer the truth, and the reason belongs on the page.
                    last = Some(result?);
                }
                match last {
                    Some(detail) => Ok(detail),
                    None => client.recipe(&id).await,
                }
            })
        });
    }

    /// Accept or decline a recipe shared with the person, from the list or from its detail.
    pub fn answer_recipe_share(&mut self, id: String, accept: bool, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        self.recipes_error = None;
        if self.recipe_open_id.as_deref() == Some(id.as_str()) {
            self.recipe_busy = Some(
                if accept {
                    "Accepting…"
                } else {
                    "Declining…"
                }
                .to_string(),
            );
            self.recipe_error = None;
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = if accept {
                client.accept_recipe(&id).await
            } else {
                client.decline_recipe(&id).await
            };
            let _ = this.update(cx, |state, cx| {
                let open = state.recipe_open_id.as_deref() == Some(id.as_str());
                if open {
                    state.recipe_busy = None;
                }
                match result {
                    Ok(detail) => {
                        if open {
                            state.set_open_recipe(detail);
                        }
                        state.refresh_recipes(cx);
                    }
                    Err(error) if open => state.recipe_error = Some(error.message),
                    Err(error) => state.recipes_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Play the open recipe on one of the person's bots. The outcome and the screen after it
    /// show on the page, and the run joins the history.
    pub fn run_open_recipe(&mut self, coworker_id: String, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(id) = self.recipe_open_id.clone() else {
            return;
        };
        self.recipe_busy = Some("Running…".to_string());
        self.recipe_error = None;
        self.recipe_run_result = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.run_recipe(&id, &coworker_id).await;
            let _ = this.update(cx, |state, cx| {
                if state.recipe_open_id.as_deref() != Some(id.as_str()) {
                    return;
                }
                state.recipe_busy = None;
                match result {
                    Ok(result) => {
                        state.recipe_run_result =
                            Some(RecipeRunOutcome::from_result(coworker_id, result));
                        state.load_open_recipe(cx);
                    }
                    Err(error) => state.recipe_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Ask before deleting the open recipe: the dialog over the app.
    pub fn open_recipe_delete_confirm(&mut self, cx: &mut Context<Self>) {
        if self.recipe_open.is_none() {
            return;
        }
        self.recipe_delete_confirm = true;
        cx.notify();
    }

    pub fn close_recipe_delete_confirm(&mut self, cx: &mut Context<Self>) {
        if self.recipe_delete_confirm {
            self.recipe_delete_confirm = false;
            cx.notify();
        }
    }

    /// The dialog's Delete: remove the recipe, then leave its detail for the list.
    pub fn confirm_recipe_delete(&mut self, cx: &mut Context<Self>) {
        if !self.recipe_delete_confirm {
            return;
        }
        self.recipe_delete_confirm = false;
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(id) = self.recipe_open_id.clone() else {
            return;
        };
        self.recipe_busy = Some("Deleting…".to_string());
        self.recipe_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.delete_recipe(&id).await;
            let _ = this.update(cx, |state, cx| {
                if state.recipe_open_id.as_deref() != Some(id.as_str()) {
                    return;
                }
                state.recipe_busy = None;
                match result {
                    Ok(()) => {
                        state.close_recipe(cx);
                        state.refresh_recipes(cx);
                    }
                    Err(error) => state.recipe_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The name of the recipe the delete dialog asks about, while it is open.
    pub fn recipe_delete_prompt(&self) -> Option<String> {
        if !self.recipe_delete_confirm {
            return None;
        }
        self.recipe_open
            .as_ref()
            .map(|detail| detail.recipe.name.clone())
    }

    // ---- Settings → Skills ----
    //
    // A SKILL is prose the model reads before it works, kept on the server and invoked by
    // typing `/name`. A RECIPE is a taped replay of clicks. They are two different things that
    // share a slash, and nothing here calls one by the other's name.

    /// Settings → Skills, asked for from anywhere. The page is a settings tab, so this brings
    /// Settings up when it is shut rather than setting a tab nobody can see.
    pub fn open_skills(&mut self, cx: &mut Context<Self>) {
        // Opening Settings onto this tab fetches the list, and so does moving to it. Exactly one
        // of those happens here, unless neither does — which is the case this last line is for:
        // asking again for the tab already on screen, which would otherwise leave whatever was
        // there when it was last visited.
        let showing = self.is_app_settings_open && self.app_settings_tab == AppSettingsTab::Skills;
        if !self.is_app_settings_open {
            self.toggle_app_settings(cx);
        }
        self.set_app_settings_tab(AppSettingsTab::Skills, cx);
        if showing {
            self.refresh_skills(cx);
        }
        cx.notify();
    }

    pub fn set_skills_scope(&mut self, scope: SkillScope, cx: &mut Context<Self>) {
        if !self.take_skills_scope(scope) {
            return;
        }
        self.refresh_skills(cx);
        cx.notify();
    }

    /// Move the toggle, and take the rows off the page while the new side is on its way.
    ///
    /// The rows on screen are the rows of the side that was open, so they go the moment that
    /// side does. Left there they would sit under the other side's name until the fetch landed
    /// — and for as long as the app ran if it never did.
    ///
    /// `false` when the toggle was already there and nothing moved.
    fn take_skills_scope(&mut self, scope: SkillScope) -> bool {
        if self.skills_scope == scope {
            return false;
        }
        self.skills_scope = scope;
        self.skills.clear();
        true
    }

    /// Settings → Skills: the search field's text, the copy the list filters by.
    pub fn set_skills_query(&mut self, query: String, cx: &mut Context<Self>) {
        if self.skills_query != query {
            self.skills_query = query;
            cx.notify();
        }
    }

    /// Both sides of the toggle: the open one's rows, and a count for each.
    ///
    /// Two requests rather than one, because the row the server sends says nothing about whose
    /// skill it is — there is no owner on it — so which side a row belongs to is the question
    /// that was asked, not something that can be worked out afterwards.
    ///
    /// A late answer for an earlier scope is dropped, so a toggle worked quickly never shows one
    /// side's rows under the other side's name.
    pub fn refresh_skills(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            // Signed out there is no library to ask for, which is not the same as a library
            // with nothing in it — and "No skills yet" is what an empty list says.
            self.skills.clear();
            self.skills_counts = SkillCounts::default();
            self.skills_loading = false;
            self.skills_error = Some(SKILLS_SIGNED_OUT.to_string());
            cx.notify();
            return;
        };
        self.skills_epoch += 1;
        let epoch = self.skills_epoch;
        let scope = self.skills_scope;
        self.skills_loading = true;
        self.skills_error = None;
        cx.spawn(async move |this, cx| {
            let (yours, discover) = futures::future::join(
                client.list_skills(Some(SkillScope::Yours.query())),
                client.list_skills(Some(SkillScope::Discover.query())),
            )
            .await;
            let _ = this.update(cx, |state, cx| {
                if state.take_skills(epoch, scope, yours, discover) {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Take a pair of listings, unless the page has moved on. `false` when the answer was for a
    /// refresh that has been overtaken, and nothing was touched.
    ///
    /// Either half failing is the library failing: the counts and the rows come from the same
    /// pair of answers, and half a pair would put a number on the toggle that the list beneath
    /// it disagrees with.
    fn take_skills(
        &mut self,
        epoch: u64,
        scope: SkillScope,
        yours: Result<Vec<SkillSummary>, OpenGrokError>,
        discover: Result<Vec<SkillSummary>, OpenGrokError>,
    ) -> bool {
        if self.skills_epoch != epoch {
            return false;
        }
        self.skills_loading = false;
        match (yours, discover) {
            (Ok(yours), Ok(discover)) => {
                self.skills_counts = SkillCounts {
                    yours: yours.len(),
                    discover: discover.len(),
                };
                // The page asked for both sides for its counts, and one of the two is the list
                // `/` offers. Taking it here is what lets a skill written on this page be
                // invoked in the composer without anybody asking for it a second time.
                self.take_your_skills(yours.clone());
                self.skills = match scope {
                    SkillScope::Yours => yours,
                    SkillScope::Discover => discover,
                };
            }
            (Err(error), _) | (_, Err(error)) => {
                // The rows are the ones that could not be refreshed, so they go with the
                // counts: a list left on screen under a red line is a list somebody reads as
                // current.
                self.skills.clear();
                self.skills_counts = SkillCounts::default();
                self.skills_error = Some(error.message);
            }
        }
        true
    }

    /// The skills `/` offers, asked for by the composer.
    ///
    /// One request, where the Settings page makes two: the page needs a count for each side of
    /// its toggle, and nobody typing `/` is looking at the toggle. A fetch already in flight is
    /// left to land rather than joined by a second — two answers for one question can only
    /// disagree by being differently stale.
    pub fn refresh_your_skills(&mut self, cx: &mut Context<Self>) {
        if self.your_skills_loading {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            // Signed out there is no library to ask for, which is not the same as a library with
            // nothing in it.
            self.your_skills.clear();
            self.your_skills_error = Some(SKILLS_SIGNED_OUT.to_string());
            cx.notify();
            return;
        };
        self.your_skills_loading = true;
        self.your_skills_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let listed = client.list_skills(Some(SkillScope::Yours.query())).await;
            let _ = this.update(cx, |state, cx| {
                state.your_skills_loading = false;
                match listed {
                    Ok(rows) => state.take_your_skills(rows),
                    // The rows already there are left where they are, which is the opposite of
                    // what the Settings page does with its own. A row here is a thing to pick,
                    // and one that was listed a minute ago is as pickable as it was — the server
                    // is the authority on an id either way. Emptying the list because a refresh
                    // failed would take away the one thing the person opened `/` for.
                    Err(error) => state.your_skills_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Take a listing of this person's skills, from wherever it was asked for.
    fn take_your_skills(&mut self, rows: Vec<SkillSummary>) {
        self.your_skills = rows;
        self.your_skills_loading = false;
        self.your_skills_error = None;
    }

    /// The detail pane for one skill. The prose is not on the listing, so it is fetched.
    pub fn open_skill(&mut self, id: String, cx: &mut Context<Self>) {
        self.skill_open = None;
        self.skill_open_id = Some(id);
        self.skill_error = None;
        self.load_open_skill(cx);
        cx.notify();
    }

    fn load_open_skill(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(id) = self.skill_open_id.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.skill(&id).await;
            let _ = this.update(cx, |state, cx| {
                // A late answer for a skill the person has since left is stale.
                if state.skill_open_id.as_deref() != Some(id.as_str()) {
                    return;
                }
                match result {
                    Ok(detail) => state.skill_open = Some(detail),
                    // On the pane, where the person is waiting for it. In the list's slot it
                    // would be a sentence about the library, and the pane would go on saying
                    // "Loading…" at something that is never going to arrive.
                    Err(error) => state.skill_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn close_skill(&mut self, cx: &mut Context<Self>) {
        if self.skill_open_id.is_none() && self.skill_open.is_none() {
            return;
        }
        self.skill_open = None;
        self.skill_open_id = None;
        self.skill_error = None;
        cx.notify();
    }

    /// Create a skill → the sheet over the page.
    pub fn open_skill_add(&mut self, cx: &mut Context<Self>) {
        if !self.skill_add_open {
            self.skill_add_open = true;
            self.skill_add_error = None;
            cx.notify();
        }
    }

    pub fn close_skill_add(&mut self, cx: &mut Context<Self>) {
        if self.skill_add_open {
            self.skill_add_open = false;
            // The refusal was about what was in the sheet. With the sheet gone there is nothing
            // for it to be about, and left behind it reads as a verdict on the library.
            self.skill_add_error = None;
            cx.notify();
        }
    }

    /// The page has emptied the sheet's fields after a create the server took.
    pub fn skill_add_fields_cleared(&mut self) {
        self.skill_add_taken = false;
    }

    /// Write one down: the prose as typed, with the name and the description beside it.
    ///
    /// The sheet stays up until the server has taken it. What a name may be, and how long a body
    /// may be, are the server's rules and its refusals are shown as it words them — and a name
    /// it will not take is the likeliest thing to go wrong on a first try, so the words somebody
    /// wrote have to still be in the fields when that sentence arrives. The one thing refused
    /// here is a skill with no name at all, because there is nothing to send.
    pub fn create_skill(
        &mut self,
        name: String,
        description: String,
        body: String,
        cx: &mut Context<Self>,
    ) {
        if self.skill_saving {
            return;
        }
        let name = name.trim().to_string();
        if name.is_empty() {
            self.skill_add_error =
                Some("A skill needs a name — it is what you type after the slash.".into());
            cx.notify();
            return;
        }
        self.send_new_skill(
            NewSkill {
                name,
                description: description.trim().to_string(),
                body: body.trim().to_string(),
                source: SkillSource::Authored,
                files: Vec::new(),
            },
            SkillCreateFrom::Sheet,
            cx,
        );
    }

    /// Settings → Skills → Upload skill: the picker, then [`Self::upload_skill`].
    pub fn pick_skill_upload(&mut self, cx: &mut Context<Self>) {
        // A file (one `SKILL.md`) or a directory (a `SKILL.md` and what sits beside it).
        let answer = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some("Upload".into()),
        });
        cx.spawn(async move |this, cx| {
            let chosen = answer.await;
            let _ = this.update(cx, |state, cx| {
                match chosen {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            state.upload_skill(path, cx);
                        }
                    }
                    Ok(Ok(None)) | Err(_) => {}
                    Ok(Err(error)) => {
                        state.skills_error =
                            Some(format!("The file picker would not open: {error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A picked `SKILL.md`, or the folder one lives in.
    ///
    /// The name is left off on purpose: an uploaded `SKILL.md` names itself in its frontmatter,
    /// and the server reads it from there. Making somebody retype it is how a skill's file and
    /// its row come to disagree about what the thing is called.
    pub fn upload_skill(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        if self.skill_saving {
            return;
        }
        self.skills_error = None;
        self.skill_saving = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let read = cx
                .background_executor()
                .spawn(async move { read_skill_upload(&path) })
                .await;
            let _ = this.update(cx, |state, cx| {
                match read {
                    Ok((body, files)) => state.send_new_skill(
                        NewSkill {
                            name: String::new(),
                            description: String::new(),
                            body,
                            source: SkillSource::Uploaded,
                            files,
                        },
                        SkillCreateFrom::Upload,
                        cx,
                    ),
                    // What was picked is the list's business: the Upload button is on the
                    // list, and nothing was typed for this to be a verdict on.
                    Err(why) => {
                        state.skill_saving = false;
                        state.skills_error = Some(why);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The one road to `POST /skills`, taken by both the sheet and an upload: the new skill is
    /// sent, the library reloads, and the one that was just made is the one on the pane.
    ///
    /// `from` decides where a refusal is drawn, and it has to be told rather than guessed at.
    /// The sheet has a slot of its own, over the fields the sentence is about; an upload has no
    /// sheet, so a sentence put in that slot is a sentence drawn nowhere at all — which is what
    /// a click on Upload that did nothing looked like.
    fn send_new_skill(&mut self, new: NewSkill, from: SkillCreateFrom, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            self.skill_saving = false;
            self.refuse_create(from, "Sign in to keep a skill on the server.".into());
            cx.notify();
            return;
        };
        self.skill_saving = true;
        self.skill_add_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.create_skill(&new).await;
            let _ = this.update(cx, |state, cx| {
                state.skill_saving = false;
                match result {
                    Ok(detail) => {
                        // Taken: the sheet may go, and the words in ITS fields are now kept
                        // somewhere, so the page may empty them. An upload that landed says
                        // nothing about a draft somebody left in the sheet.
                        state.skill_add_open = false;
                        state.skill_add_error = None;
                        state.skill_add_taken = from == SkillCreateFrom::Sheet;
                        let id = detail.skill.id.clone();
                        state.skill_open_id = Some(id);
                        state.skill_error = None;
                        state.skill_open = Some(detail);
                        // A new skill is one of the person's own, whichever side was open.
                        state.take_skills_scope(SkillScope::Yours);
                        state.refresh_skills(cx);
                    }
                    Err(error) => state.refuse_create(from, error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A create that was refused, put where the person who asked for it is looking.
    fn refuse_create(&mut self, from: SkillCreateFrom, why: String) {
        match from {
            SkillCreateFrom::Sheet => self.skill_add_error = Some(why),
            SkillCreateFrom::Upload => self.skills_error = Some(why),
        }
    }

    /// Delete asks first, the way every recipe does: the dialog over the page, naming the skill
    /// it is about. A skill is prose somebody wrote, and it sits one row under Open in a menu
    /// that opens under the pointer.
    pub fn ask_skill_delete(&mut self, id: String, cx: &mut Context<Self>) {
        self.skill_delete_confirm = Some(id);
        self.skills_error = None;
        cx.notify();
    }

    pub fn close_skill_delete_confirm(&mut self, cx: &mut Context<Self>) {
        if self.skill_delete_confirm.take().is_some() {
            cx.notify();
        }
    }

    /// The name of the skill the delete dialog asks about, while it is open. An id with no row
    /// to put a name to is still a skill to ask about, so the id stands in for the name.
    pub fn skill_delete_prompt(&self) -> Option<String> {
        let id = self.skill_delete_confirm.as_ref()?;
        Some(
            self.skills
                .iter()
                .find(|skill| &skill.id == id)
                .map(|skill| skill.name.clone())
                .filter(|name| !name.trim().is_empty())
                .unwrap_or_else(|| id.clone()),
        )
    }

    pub fn confirm_skill_delete(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.skill_delete_confirm.take() else {
            return;
        };
        self.delete_skill(id, cx);
    }

    fn delete_skill(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            self.skills_error = Some("Sign in to change what is kept on the server.".into());
            cx.notify();
            return;
        };
        self.skills_error = None;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.delete_skill(&id).await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(()) => {
                        // The pane cannot go on showing a skill that is gone.
                        if state.skill_open_id.as_deref() == Some(id.as_str()) {
                            state.close_skill(cx);
                        }
                        state.refresh_skills(cx);
                    }
                    Err(error) => state.skills_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Teach the active bot a task: its screen, with a tape already running. The same thing the
    /// screen window's own Teach a task button does, asked for from the composer, opening the
    /// window first when there is not one yet.
    pub fn teach_task(&mut self, cx: &mut Context<Self>) {
        self.show_coworker_screen(true, cx);
    }

    pub fn open_coworker_screen(&mut self, cx: &mut Context<Self>) {
        self.show_coworker_screen(false, cx);
    }

    /// The active coworker's screen. `teach` carries the ask for a tape all the way to the
    /// window, which may be several awaits away: the box has to exist before it has a screen.
    fn show_coworker_screen(&mut self, teach: bool, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        if let Some(url) = self
            .coworker_computer
            .as_ref()
            .and_then(CoworkerComputer::vnc_url)
            .map(str::to_string)
        {
            self.open_computer_window(&coworker_id, &url, teach, cx);
            return;
        }
        cx.spawn(async move |this, cx| {
            let result = client.ensure_coworker_computer(&coworker_id).await;
            let _ = this.update(cx, |state, cx| match result {
                Ok(status) => {
                    if let Some(url) = status.vnc_url().map(str::to_string) {
                        state.open_computer_window(&coworker_id, &url, teach, cx);
                    }
                    if state.active_coworker_id.as_deref() == Some(coworker_id.as_str()) {
                        let mut status = status;
                        state.hold_pending_egress_policy(&coworker_id, &mut status);
                        state.coworker_computer = Some(status);
                        cx.notify();
                    }
                }
                Err(error) => eprintln!(
                    "NativeChat computer: could not open the computer of {coworker_id}: {}",
                    error.message
                ),
            });
        })
        .detach();
    }

    /// The screen of `coworker_id`, in its own window. The title names that
    /// coworker, not whichever one is active by the time the answer lands.
    fn open_computer_window(
        &mut self,
        coworker_id: &str,
        url: &str,
        teach: bool,
        cx: &mut Context<Self>,
    ) {
        let title = self
            .coworkers
            .iter()
            .find(|coworker| coworker.id == coworker_id)
            .map(|coworker| format!("{}'s Computer", coworker.name))
            .unwrap_or_else(|| "Computer".into());
        #[cfg(target_os = "macos")]
        {
            let attention = self.computer_window_attention();
            // Already open: bring it forward. A handle whose window was closed fails to
            // update, and that is the cue to open a fresh one.
            if let Some(existing) = self.computer_windows.get(coworker_id)
                && existing
                    .update(cx, |screen, window, cx| {
                        screen.set_handoff_attention(attention.clone(), cx);
                        window.activate_window();
                        if teach {
                            screen.start_teaching(window, cx);
                        }
                    })
                    .is_ok()
            {
                return;
            }
            let url = url.to_string();
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin: point(px(72.), px(72.)),
                    size: size(px(1100.), px(760.)),
                })),
                window_min_size: Some(size(px(640.), px(480.))),
                // The title bar is ours: transparent, with the traffic lights left where they
                // are, so the strip the window paints (name, Teach a task) IS the title bar and
                // follows the app's theme rather than the system's.
                titlebar: Some(TitlebarOptions {
                    title: Some(title.clone().into()),
                    appears_transparent: true,
                    traffic_light_position: Some(point(px(12.), px(14.))),
                }),
                ..WindowOptions::default()
            };
            let coworker = coworker_id.to_string();
            let app = cx.entity();
            let opened = cx.open_window(options, move |window, cx| {
                cx.new(|cx| {
                    crate::components::computer_screen::ComputerScreen::new(
                        &url, &coworker, &title, app, attention, window, cx,
                    )
                })
            });
            match opened {
                Ok(handle) => {
                    self.computer_windows
                        .insert(coworker_id.to_string(), handle);
                    if teach {
                        // The page is not up yet, and teaching is a flag set on it; wait the
                        // same moment the window itself waits before painting the page.
                        cx.spawn(async move |_, cx| {
                            cx.background_executor()
                                .timer(std::time::Duration::from_millis(1500))
                                .await;
                            let _ = handle.update(cx, |screen, window, cx| {
                                screen.start_teaching(window, cx);
                            });
                        })
                        .detach();
                    }
                }
                Err(error) => eprintln!("NativeChat computer: could not open a window: {error}"),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = teach;
            eprintln!("NativeChat computer: {title} is at {url}; opening it in-app is macOS-only");
        }
    }

    /// Push the live Open-the-screen strip onto any Computer window. Safe to
    /// call while this AppState is leased: the window stores a copy and does
    /// not read AppState from `render`.
    #[cfg(target_os = "macos")]
    fn push_computer_window_attention(&mut self, cx: &mut Context<Self>) {
        let attention = self.computer_window_attention();
        for handle in self.computer_windows.values() {
            let attention = attention.clone();
            let _ = handle.update(cx, |screen, _, cx| {
                screen.set_handoff_attention(attention, cx);
            });
        }
    }

    pub fn open_routine_editor(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        let id = match id {
            Some(id) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                self.routines.entry(coworker_id).or_default().insert(
                    0,
                    AgentRoutine {
                        id: id.clone(),
                        name: String::new(),
                        instruction: String::new(),
                        active: true,
                        triggers: Vec::new(),
                        runs: Vec::new(),
                    },
                );
                id
            }
        };
        self.set_right_pane(RightPane::Computer, cx);
        self.computer_view = ComputerView::Editor { id: Some(id) };
        self.record_nav();
        cx.notify();
    }

    pub fn back_to_computer(&mut self, cx: &mut Context<Self>) {
        if let (Some(coworker_id), ComputerView::Editor { id: Some(rid) }) =
            (self.active_coworker_id.clone(), self.computer_view.clone())
        {
            let empty = self
                .routines
                .get(&coworker_id)
                .and_then(|rows| rows.iter().find(|row| row.id == rid))
                .is_some_and(|row| {
                    row.name.trim().is_empty()
                        && row.instruction.trim().is_empty()
                        && row.triggers.is_empty()
                        && row.runs.is_empty()
                });
            if empty {
                self.delete_routine(&coworker_id, &rid, cx);
                return;
            }
        }
        self.computer_view = ComputerView::Overview;
        self.record_nav();
        cx.notify();
    }

    pub fn coworker_routines(&self, coworker_id: &str) -> &[AgentRoutine] {
        self.routines
            .get(coworker_id)
            .map(|rows| rows.as_slice())
            .unwrap_or(&[])
    }

    /// The editor's two fields, written back to the routine on screen.
    ///
    /// Only on screen: `/schedules` takes a name and a prompt when a schedule is made and has
    /// no route to change either afterwards. So this is what a routine reads as here until the
    /// next listing, which is the server's copy and wins.
    pub fn save_routine_fields(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
        name: String,
        instruction: String,
        cx: &mut Context<Self>,
    ) {
        if let Some(row) = self.routine_mut(coworker_id, routine_id) {
            row.name = name;
            row.instruction = instruction;
        }
        cx.notify();
    }

    pub fn routine_mut(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
    ) -> Option<&mut AgentRoutine> {
        self.routines
            .get_mut(coworker_id)?
            .iter_mut()
            .find(|row| row.id == routine_id)
    }

    /// The trigger that makes a draft a routine.
    ///
    /// This is where a routine stops being something the app made up: the trigger goes to
    /// `POST /schedules` with the name and prompt beside it, and the row that comes back is
    /// the routine — the server's id, the server's URL and key. Until this runs, a draft is a
    /// blank form and the coworker has never heard of it.
    pub fn add_routine_trigger(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
        trigger: NewTrigger,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.routine_mut(coworker_id, routine_id) else {
            return;
        };
        if !row.triggers.is_empty() {
            self.say_routine_trouble(ROUTINE_IS_ONE_SCHEDULE.to_string(), cx);
            return;
        }
        let name = row.name.clone();
        let prompt = row.instruction.clone();
        let (kind, cron) = match trigger {
            NewTrigger::Schedule(spec) => match spec.to_cron() {
                Ok(cron) => (ScheduleKind::Cron, Some(cron)),
                // A schedule that is not one cron line is not one the server can keep, and the
                // sentence says which part of it cannot be said. Nothing is created.
                Err(not_cron) => {
                    self.say_routine_trouble(not_cron.sentence().to_string(), cx);
                    return;
                }
            },
            NewTrigger::Webhook => (ScheduleKind::Webhook, None),
        };
        self.create_schedule(
            coworker_id.to_string(),
            Some(routine_id.to_string()),
            NewSchedule {
                coworker_id: coworker_id.to_string(),
                kind,
                prompt,
                name,
                cron,
            },
            cx,
        );
    }

    /// The open coworker's routines, as `/schedules` keeps them.
    pub fn load_routines(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.list_schedules(&coworker_id).await;
            let _ = this.update(cx, |state, cx| {
                // A late answer for a bot the person has since left is stale.
                if state.active_coworker_id.as_deref() != Some(coworker_id.as_str()) {
                    return;
                }
                match result {
                    Ok(rows) => {
                        // A draft has no trigger, which means it has never been to the server,
                        // which means no listing can know about it. Somebody is writing it.
                        let mut routines: Vec<AgentRoutine> = state
                            .routines
                            .get(&coworker_id)
                            .map(|rows| {
                                rows.iter()
                                    .filter(|row| row.triggers.is_empty())
                                    .cloned()
                                    .collect()
                            })
                            .unwrap_or_default();
                        routines.extend(rows.into_iter().map(routine_from_schedule));
                        state.routines.insert(coworker_id, routines);
                    }
                    Err(error) => state.computer_action_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Put a routine on the server outright: a prompt and a way of firing it, no draft in
    /// between. The e2e driver's `routine.create`, and nothing a person does.
    pub fn create_routine(
        &mut self,
        kind: ScheduleKind,
        prompt: String,
        cron: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(coworker_id) = self.active_coworker_id.clone() else {
            return;
        };
        self.create_schedule(
            coworker_id.clone(),
            None,
            NewSchedule {
                coworker_id,
                kind,
                prompt,
                name: String::new(),
                cron,
            },
            cx,
        );
    }

    fn create_schedule(
        &mut self,
        coworker_id: String,
        draft_id: Option<String>,
        new: NewSchedule,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        self.computer_action_error = None;
        cx.spawn(async move |this, cx| {
            let result = client.create_schedule(&new).await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(row) => state.settle_new_routine(&coworker_id, draft_id.as_deref(), row),
                    Err(error) => state.computer_action_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The created row takes the draft's place, id and all.
    ///
    /// The draft's id was a uuid this app minted to have something to open the editor on. The
    /// server's id is the one every call after this uses, so the editor has to follow it over.
    fn settle_new_routine(&mut self, coworker_id: &str, draft_id: Option<&str>, row: ScheduleRow) {
        let routine = routine_from_schedule(row);
        let id = routine.id.clone();
        let rows = self.routines.entry(coworker_id.to_string()).or_default();
        match draft_id.and_then(|draft| rows.iter().position(|row| row.id == draft)) {
            Some(at) => rows[at] = routine,
            None => rows.insert(0, routine),
        }
        if let ComputerView::Editor { id: Some(open) } = &self.computer_view
            && draft_id == Some(open.as_str())
        {
            self.computer_view = ComputerView::Editor { id: Some(id) };
        }
    }

    /// A new key for the routine's webhook. The old one stops working at once, so the row that
    /// comes back is the only copy of the new one and it goes straight onto the trigger.
    pub fn rotate_routine_webhook(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let coworker_id = coworker_id.to_string();
        let routine_id = routine_id.to_string();
        self.computer_action_error = None;
        cx.spawn(async move |this, cx| {
            let result = client.rotate_webhook_key(&routine_id).await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(row) => {
                        if let Some(routine) = state.routine_mut(&coworker_id, &routine_id) {
                            routine.triggers = vec![trigger_from_schedule(&row)];
                        }
                    }
                    Err(error) => state.computer_action_error = Some(error.message),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Say what went wrong with a routine where the person is looking: the same line the
    /// Computer pane puts a refused Update or Reset on.
    fn say_routine_trouble(&mut self, sentence: String, cx: &mut Context<Self>) {
        self.computer_action_error = Some(sentence);
        cx.notify();
    }

    /// Change when a routine runs, on screen.
    ///
    /// On screen only, for the same reason the name and the prompt are: `/schedules` has no
    /// route to change a schedule once it is made. What this does do is say so the moment a
    /// combination stops being one cron line — two times of day with different minutes past
    /// the hour, a week with no day picked — because that is a thing the person is building
    /// right now and can still put right.
    pub fn update_schedule_spec(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
        trigger_id: &str,
        spec: ScheduleSpec,
        cx: &mut Context<Self>,
    ) {
        let trouble = spec.to_cron().err();
        if let Some(row) = self.routine_mut(coworker_id, routine_id)
            && let Some(RoutineTrigger::Schedule { spec: current, .. }) =
                row.triggers.iter_mut().find(|t| t.id() == trigger_id)
        {
            *current = spec;
        }
        self.computer_action_error = trouble.map(|not_cron| not_cron.sentence().to_string());
        cx.notify();
    }

    pub fn record_routine_run(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
        cx: &mut Context<Self>,
    ) {
        let stamp = chrono::Local::now()
            .format("%b %d at %I:%M %p")
            .to_string()
            .replace(" 0", " ");
        if let Some(row) = self.routine_mut(coworker_id, routine_id) {
            row.runs.insert(
                0,
                RoutineRun {
                    at: stamp,
                    ok: true,
                },
            );
        }
        cx.notify();
    }

    /// Drop the routine here and on the server.
    ///
    /// It goes off the list first, because that is what the click meant and waiting on a round
    /// trip to act on it would read as the click not having landed. A refusal puts it back:
    /// the listing is asked for again, and what the server still has is what is shown.
    pub fn delete_routine(&mut self, coworker_id: &str, routine_id: &str, cx: &mut Context<Self>) {
        let on_the_server = self
            .routine_mut(coworker_id, routine_id)
            .is_some_and(|row| !row.triggers.is_empty());
        if let Some(rows) = self.routines.get_mut(coworker_id) {
            rows.retain(|row| row.id != routine_id);
        }
        self.computer_view = ComputerView::Overview;
        cx.notify();
        // A draft nobody finished is nobody's but this app's.
        if !on_the_server {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let routine_id = routine_id.to_string();
        cx.spawn(async move |this, cx| {
            let result = client.delete_schedule(&routine_id).await;
            let _ = this.update(cx, |state, cx| {
                if let Err(error) = result {
                    state.computer_action_error = Some(error.message);
                    state.load_routines(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The Active switch: pause or resume the schedule.
    ///
    /// The switch moves first and moves back if the server refuses, because a switch that sat
    /// still for a round trip would read as a click that missed.
    pub fn set_routine_active(
        &mut self,
        coworker_id: &str,
        routine_id: &str,
        active: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self.routine_mut(coworker_id, routine_id) else {
            return;
        };
        if row.active == active {
            return;
        }
        row.active = active;
        let on_the_server = !row.triggers.is_empty();
        cx.notify();
        // A draft has no schedule to pause; it is created active, whichever way this sits now.
        if !on_the_server {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let coworker_id = coworker_id.to_string();
        let routine_id = routine_id.to_string();
        cx.spawn(async move |this, cx| {
            let result = if active {
                client.resume_schedule(&routine_id).await
            } else {
                client.pause_schedule(&routine_id).await
            };
            let _ = this.update(cx, |state, cx| {
                if let Err(error) = result {
                    if let Some(row) = state.routine_mut(&coworker_id, &routine_id) {
                        row.active = !active;
                    }
                    state.computer_action_error = Some(error.message);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn dismiss_popovers(&mut self, cx: &mut Context<Self>) {
        if !self.model_picker_open && !self.avatar_editor_open && self.emoji_picker.is_none() {
            return;
        }
        self.model_picker_open = false;
        self.avatar_editor_open = false;
        self.emoji_picker = None;
        self.hidden_bots_open = false;
        cx.notify();
    }

    pub fn set_reply_to(&mut self, reply: ReplyTo, cx: &mut Context<Self>) {
        self.reply_to = Some(reply);
        cx.notify();
    }

    pub fn clear_reply_to(&mut self, cx: &mut Context<Self>) {
        if self.reply_to.take().is_some() {
            cx.notify();
        }
    }

    pub fn toggle_reaction(&mut self, message_id: String, emoji: String, cx: &mut Context<Self>) {
        match self.message_reactions.get(&message_id) {
            Some(current) if current == &emoji => {
                self.message_reactions.remove(&message_id);
            }
            _ => {
                self.message_reactions.insert(message_id, emoji);
            }
        }
        self.emoji_picker = None;
        cx.notify();
    }

    pub fn open_emoji_picker(
        &mut self,
        message_id: String,
        bounds: Bounds<Pixels>,
        cx: &mut Context<Self>,
    ) {
        self.emoji_picker = Some(EmojiPickerOpen { message_id, bounds });
        cx.notify();
    }

    /// Pick one of the composer's capabilities. The picked ones show as chips beside the field.
    /// Name a tool for the next message. Naming the same one twice is one chip, not two.
    pub fn pick_tool(&mut self, id: String, label: String, cx: &mut Context<Self>) {
        if self.picked_tools.iter().any(|picked| picked.id == id) {
            return;
        }
        let kind = PickedKind::of(&id);
        self.picked_tools.push(PickedTool { id, label, kind });
        cx.notify();
    }

    /// Take a named tool back.
    pub fn unpick_tool(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(index) = self.picked_tools.iter().position(|picked| picked.id == id) {
            self.picked_tools.remove(index);
            cx.notify();
        }
    }

    /// The names to send with the next message, in the order they were named.
    pub fn picked_tool_ids(&self) -> Vec<String> {
        self.picked_tools
            .iter()
            .map(|picked| picked.id.clone())
            .collect()
    }

    /// Put a recipe or a workflow on the next message, from the list the composer picked it out
    /// of. An id the list does not hold leaves the draft as it was, and says so, so the composer
    /// knows whether it has something to show.
    ///
    /// The two kinds go on the draft by the same route because the listing brings them back in
    /// one array: what a row IS travels on the row itself, and [`ActiveRecipe::from_summary`]
    /// carries it through to the bar.
    pub fn start_recipe(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        let Some(recipe) = self.recipes.iter().find(|recipe| recipe.id == id) else {
            return false;
        };
        self.active_recipe = Some(ActiveRecipe::from_summary(recipe));
        cx.notify();
        true
    }

    /// Say which of the composer's lists is open, or that none is. Called by the composer, and
    /// read by anything that cannot see into the composer's own view.
    pub fn set_composer_panel(
        &mut self,
        panel: Option<crate::components::chat_input::PanelMode>,
        cx: &mut Context<Self>,
    ) {
        if self.composer_panel != panel {
            self.composer_panel = panel;
            cx.notify();
        }
    }

    /// Take the recipe back off the draft.
    pub fn clear_active_recipe(&mut self, cx: &mut Context<Self>) {
        if self.active_recipe.take().is_some() {
            cx.notify();
        }
    }

    /// Put a skill on the next message, from the list the composer picked it out of. An id the
    /// list does not hold leaves the draft as it was, and says so.
    ///
    /// One skill to a message: picking a second replaces the first, because the turn names one
    /// id and a message carrying two would have to choose one of them somewhere the person
    /// cannot see.
    pub fn start_skill(&mut self, id: &str, cx: &mut Context<Self>) -> bool {
        if !self.attach_skill(id) {
            return false;
        }
        cx.notify();
        true
    }

    /// The pick itself, apart from the redraw. `false` when the library holds no such id and
    /// the draft was left as it was.
    fn attach_skill(&mut self, id: &str) -> bool {
        // The list `/` was built from, which is not the Settings page's: the page shows one side
        // of a toggle, and a pick has to be findable in the rows it was picked out of.
        let Some(skill) = self.your_skills.iter().find(|skill| skill.id == id) else {
            return false;
        };
        self.active_skill = Some(ActiveSkill {
            id: skill.id.clone(),
            // The name the `/` row showed, so the chip in the message and anything that reports
            // what is on the draft cannot come to call one skill two things.
            name: crate::components::chat_input::sources::skill_name(skill),
        });
        true
    }

    /// Take the skill back off the draft. This is what the chip leaving the message does.
    pub fn clear_active_skill(&mut self, cx: &mut Context<Self>) {
        if self.active_skill.take().is_some() {
            cx.notify();
        }
    }

    /// The id of the skill the draft is holding, taken off it as the draft goes.
    ///
    /// ONE DOOR. Several things reach [`Self::send_message_with`] and only one of them is the
    /// draft: a retry re-runs a turn that already went, a generative-UI form's answer is the
    /// card's words, and a driver's `chat.send` carries words of its own. None of those came
    /// off the composer, so none of them may take what the composer is holding — the turn would
    /// carry a skill nobody attached to it, and the chip would be left standing over a draft
    /// with nothing behind it. They pass `None`; [`Self::send_draft`] is the one caller here.
    ///
    /// ONE TURN. A skill applies to the message it was sent with and to nothing after it, which
    /// is why this takes rather than reads. A message held while the thread is busy keeps it on
    /// [`QueuedSend`], because that message WAS the draft.
    ///
    /// The id is not checked against [`Self::your_skills`] on the way past. That listing is as
    /// old as the last `/`, and a skill deleted since is not the only way for an id to be
    /// missing from it; the server knows every id and refuses the ones it does not, which is the
    /// answer the person is shown.
    fn take_draft_skill(&mut self) -> Option<String> {
        self.active_skill.take().map(|skill| skill.id)
    }

    /// Fill one of the active recipe's parameters in, or take its value away.
    pub fn set_recipe_value(&mut self, name: &str, value: Option<String>, cx: &mut Context<Self>) {
        let Some(recipe) = self.active_recipe.as_mut() else {
            return;
        };
        recipe.set_value(name, value);
        cx.notify();
    }

    pub fn close_emoji_picker(&mut self, cx: &mut Context<Self>) {
        if self.emoji_picker.take().is_some() {
            cx.notify();
        }
    }

    pub fn delete_message(&mut self, message_id: &str, cx: &mut Context<Self>) {
        if self.native_tts.message_id.as_deref() == Some(message_id) {
            if let Some(service) = &self.tts_service {
                service.stop_native();
            }
            self.native_tts = SourceTtsState::default();
        }
        // Marked, not taken out. A hidden message is the thread's own memory of a run it has
        // already accounted for; without it the next reconcile decides the server knows
        // something this thread does not and grafts the turn back on.
        let mut hidden_shots: Vec<String> = Vec::new();
        let mut hidden_run: Option<String> = None;
        if let Some(id) = &self.active_conversation_id
            && let Some(conversation) = self.conversations.iter_mut().find(|c| &c.id == id)
            && let Some(message) = conversation
                .messages
                .iter_mut()
                .find(|m| m.id == message_id)
        {
            message.hidden = true;
            hidden_run = message.run_id.clone();
            hidden_shots = message
                .parts
                .iter()
                .filter_map(|part| match part {
                    ChatPart::Screenshot(spec) => Some(spec.call_id.clone()),
                    _ => None,
                })
                .collect();
        }
        // The last picture of the computer is kept aside to stand in for a screen that has
        // not arrived yet. A picture the person hid must not be the one standing in.
        if self
            .last_box_shot
            .as_ref()
            .is_some_and(|shot| hidden_shots.contains(&shot.call_id))
        {
            self.last_box_shot = None;
        }
        self.message_reactions.remove(message_id);
        if self
            .reply_to
            .as_ref()
            .is_some_and(|r| r.message_id == message_id)
        {
            self.reply_to = None;
        }
        if self
            .emoji_picker
            .as_ref()
            .is_some_and(|p| p.message_id == message_id)
        {
            self.emoji_picker = None;
        }
        if let Some(db) = self.database_service.clone() {
            let id = message_id.to_string();
            cx.spawn(async move |_, _| {
                if let Err(error) = db.hide_message(&id).await {
                    eprintln!("Failed to hide message: {error}");
                }
            })
            .detach();
        }
        // A coworker's reply is a run the server holds, and every machine this person signs in
        // from reads the thread from there. Hiding it here only puts it out of sight here, so
        // the server is told as well. It is not what keeps the message hidden on this Mac —
        // the row already does that — so a server that cannot be reached costs nothing but the
        // other machines, and the next delete tries again.
        if let Some(run_id) = hidden_run
            && let Some(client) = self.opengrok.clone()
        {
            cx.spawn(async move |_, _| {
                if let Err(error) = client.hide_run(&run_id).await {
                    log::warn!("could not tell the server a turn was hidden: {error}");
                }
            })
            .detach();
        }
        cx.notify();
    }

    pub fn set_model_picker_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.model_picker_open == open && (!open || !self.avatar_editor_open) {
            return;
        }
        self.model_picker_open = open;
        if open {
            self.avatar_editor_open = false;
        }
        cx.notify();
    }

    pub fn set_avatar_editor_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.avatar_editor_open == open && (!open || !self.model_picker_open) {
            return;
        }
        self.avatar_editor_open = open;
        if open {
            self.model_picker_open = false;
        }
        cx.notify();
    }

    pub fn patch_active_coworker(
        &mut self,
        model: Option<String>,
        role: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.patch_active_agent(
            CoworkerPatch {
                model,
                role,
                ..Default::default()
            },
            cx,
        );
    }

    pub fn patch_active_agent(&mut self, patch: CoworkerPatch, cx: &mut Context<Self>) {
        self.patch_active_agent_then(patch, None, cx);
    }

    /// The same patch, with `done` called once the request is over, carrying the server's
    /// message when it refused. A control that was put out of the person's hands for the
    /// length of the request has no other way of hearing that the request has ended.
    pub fn patch_active_agent_then(
        &mut self,
        patch: CoworkerPatch,
        done: Option<PatchDone>,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            self.refuse_patch("OpenGrok is not configured", done, cx);
            return;
        };
        let Some(id) = self.active_coworker_id.clone() else {
            self.refuse_patch("No agent selected", done, cx);
            return;
        };
        // The roster takes the patch before the server has seen it so the pane answers the
        // click at once, and keeps what it held before it so that guess can be taken back:
        // `settle_patch` replaces it with the server's word either way.
        let before = self.coworkers.iter().find(|c| c.id == id).cloned();
        if let Some(existing) = self.coworkers.iter_mut().find(|c| c.id == id) {
            apply_patch(existing, &patch);
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.patch_coworker(&id, &patch).await;
            let error = result.as_ref().err().map(|error| error.message.clone());
            let _ = this.update(cx, |state, cx| {
                if let Some(before) = before.as_ref()
                    && let Some(existing) = state.coworkers.iter_mut().find(|c| c.id == id)
                {
                    settle_patch(existing, &patch, result.as_ref().ok(), before);
                }
                match result.as_ref() {
                    Ok(_) => {
                        state.auth_error = None;
                        state.note_server_answered(cx);
                    }
                    Err(error) => state.note_failure(error, cx),
                }
                cx.notify();
            });
            if let Some(done) = done {
                // Outside the roster's own update, so that whoever was waiting on the patch is
                // free to reach for anything the app holds, this roster included.
                cx.update(|cx| done(error, cx));
            }
        })
        .detach();
    }

    /// A patch that never left the app. The settings pane paints the reason, and whoever is
    /// waiting on the request still has to hear that it is over.
    fn refuse_patch(&mut self, reason: &str, done: Option<PatchDone>, cx: &mut Context<Self>) {
        let reason = reason.to_string();
        self.auth_error = Some(reason.clone());
        cx.notify();
        if let Some(done) = done {
            // This one is answered without the server, so the answer would land while the
            // click that asked for the patch is still being handled and the asking view is
            // still on the stack. It waits for the end of the effect cycle instead.
            cx.defer(move |cx| done(Some(reason), cx));
        }
    }

    pub fn update_opengrok_profile(
        &mut self,
        first_name: String,
        last_name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client
                .update_profile(&ProfileUpdate {
                    first_name: Some(first_name),
                    last_name: Some(last_name),
                    avatar_url: None,
                })
                .await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(account) => {
                        state.account = Some(account);
                        state.auth_error = None;
                        state.note_server_answered(cx);
                    }
                    Err(error) => state.note_failure(&error, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn change_opengrok_password(
        &mut self,
        current: String,
        new_password: String,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client.change_password(&current, &new_password).await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(()) => {
                        state.auth_error = None;
                        state.note_server_answered(cx);
                    }
                    Err(error) => state.note_failure(&error, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn set_database_service(&mut self, service: DatabaseService, cx: &mut Context<Self>) {
        self.database_service = Some(service.clone());
        self.ensure_site_login_vault(cx);
        cx.notify();

        // Load sessions when DB service is set
        self.load_sessions(cx);
    }

    pub fn load_sessions(&mut self, cx: &mut Context<Self>) {
        if let Some(db) = self.database_service.clone() {
            cx.spawn(async move |this, cx| {
                match db.get_sessions().await {
                    Ok(sessions) => {
                        this.update(cx, |state, cx| {
                            state.conversations = sessions
                                .into_iter()
                                .map(|s| Conversation {
                                    id: s.id,
                                    title: s.title,
                                    created_at: s.created_at,
                                    updated_at: s.updated_at,
                                    messages: Vec::new(),
                                    unread_count: 0,
                                })
                                .collect();

                            // If no active conversation, select the most recent one
                            if state.active_conversation_id.is_none() {
                                if let Some(first) = state.conversations.first() {
                                    let id = first.id.clone();
                                    state.select_conversation(id, cx);
                                }
                            }
                            cx.notify();
                        })
                        .ok();
                    }
                    Err(e) => eprintln!("Failed to load sessions: {}", e),
                }
            })
            .detach();
        }
    }

    pub fn ensure_active_coworker(&mut self, cx: &mut Context<Self>) {
        if self.active_coworker_id.is_some() {
            return;
        }
        if let Some(first) = self.ranked_coworkers().into_iter().next() {
            self.select_coworker(first.id, cx);
        }
    }

    pub fn open_bot_finder(&mut self, cx: &mut Context<Self>) {
        self.command_palette_open = false;
        self.bot_finder_open = true;
        self.dismiss_popovers(cx);
        cx.notify();
    }

    pub fn close_bot_finder(&mut self, cx: &mut Context<Self>) {
        if self.bot_finder_open {
            self.bot_finder_open = false;
            cx.notify();
        }
    }

    pub fn open_command_palette(&mut self, cx: &mut Context<Self>) {
        self.bot_finder_open = false;
        self.command_palette_open = true;
        self.dismiss_popovers(cx);
        cx.notify();
    }

    pub fn close_command_palette(&mut self, cx: &mut Context<Self>) {
        if self.command_palette_open {
            self.command_palette_open = false;
            cx.notify();
        }
    }

    pub fn create_agent(&mut self, cx: &mut Context<Self>) {
        self.hire_agent("New Bot", cx);
    }

    pub fn hire_agent(&mut self, name: &str, cx: &mut Context<Self>) {
        if self.hiring {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        if !self.is_signed_in() {
            self.auth_error = Some("Sign in first".to_string());
            cx.notify();
            return;
        }
        self.hiring = true;
        self.auth_error = None;
        self.bot_finder_open = false;
        self.command_palette_open = false;
        cx.notify();
        let name = name.to_string();
        cx.spawn(async move |this, cx| {
            let result = client.hire(&name, None).await;
            let _ = this.update(cx, |state, cx| {
                state.hiring = false;
                match result {
                    Ok(hired) => {
                        let id = hired.id.clone();
                        state.coworkers.insert(0, hired);
                        state.select_coworker(id, cx);
                    }
                    // "is OpenGrok running at …?" was this app's one attempt at saying the
                    // server was out of reach, in a field that never cleared. It has a home of
                    // its own now, and one that goes away when the server comes back — and so
                    // does a session that has gone, which that question misdirects even further:
                    // OpenGrok is running, it answered, it simply did not know who was asking.
                    Err(error)
                        if matches!(
                            error.failure(),
                            Failure::OutOfReach(_) | Failure::SignedOut
                        ) =>
                    {
                        state.note_failure(&error, cx)
                    }
                    Err(error) => {
                        state.auth_error = Some(format!(
                            "Could not create agent: {} (is OpenGrok running at {}?)",
                            error.message,
                            state
                                .config
                                .as_ref()
                                .map(|c| c.opengrok_base_url.as_str())
                                .unwrap_or("http://127.0.0.1:1447")
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn select_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(coworker) = self.coworkers.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        self.active_coworker_id = Some(id.clone());
        // A bot chosen is a chat: the main slot leaves whatever page it was on.
        self.page = MainPage::Chat;
        // The previous bot's screen must not show under this bot's name.
        self.coworker_computer = None;
        self.coworker_screen = None;
        self.last_box_shot = None;
        self.computer_confirm = None;
        self.computer_action_error = None;
        if !self.conversations.iter().any(|c| c.id == id) {
            self.conversations.insert(
                0,
                Conversation {
                    id: id.clone(),
                    title: coworker.name.clone(),
                    created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    updated_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                    messages: Vec::new(),
                    unread_count: 0,
                },
            );
        }
        self.select_conversation(id, cx);
        // The tunnel card's chrome and the network choice read this bot's computer record and
        // the host's answer for this bot; both used to be fetched only while the Computer pane
        // was open, so a card in a thread opened cold was painted from the last bot's facts.
        self.refresh_coworker_computer_quietly(cx);
        self.refresh_host_egress(cx);
        // Routines are the server's schedules for this bot and nobody else's; the ones on
        // screen belong to whoever was open a moment ago.
        self.load_routines(cx);
        self.record_nav();
    }

    fn nav_location(&self) -> NavLocation {
        NavLocation {
            coworker_id: self.active_coworker_id.clone(),
            page: self.page,
            right_pane: self.right_pane,
            computer_view: self.computer_view.clone(),
            app_settings_open: self.is_app_settings_open,
            app_settings_tab: self.app_settings_tab,
        }
    }

    fn record_nav(&mut self) {
        self.nav.record(self.nav_location());
    }

    pub fn nav_back(&mut self, cx: &mut Context<Self>) {
        if self.command_palette_open || self.bot_finder_open {
            self.command_palette_open = false;
            self.bot_finder_open = false;
            cx.notify();
            return;
        }
        let Some(loc) = self.nav.go_back() else {
            return;
        };
        self.apply_nav(loc, cx);
    }

    pub fn nav_forward(&mut self, cx: &mut Context<Self>) {
        if self.command_palette_open || self.bot_finder_open {
            self.command_palette_open = false;
            self.bot_finder_open = false;
            cx.notify();
            return;
        }
        let Some(loc) = self.nav.go_forward() else {
            return;
        };
        self.apply_nav(loc, cx);
    }

    fn apply_nav(&mut self, loc: NavLocation, cx: &mut Context<Self>) {
        self.nav.applying = true;
        self.bot_finder_open = false;
        self.command_palette_open = false;
        if let Some(id) = loc.coworker_id.clone() {
            if self.active_coworker_id.as_ref() != Some(&id) {
                self.select_coworker(id, cx);
            }
        } else {
            self.active_coworker_id = None;
        }
        self.set_right_pane(loc.right_pane, cx);
        self.computer_view = loc.computer_view;
        self.is_app_settings_open = loc.app_settings_open;
        self.app_settings_tab = loc.app_settings_tab;
        // Landing on Settings → Computer by navigation bypasses the tab setter, whose fetch of
        // the computer record is what keeps the Route-traffic and network rows current.
        if self.is_app_settings_open && self.app_settings_tab == AppSettingsTab::Computer {
            self.refresh_computers(cx);
            self.refresh_host_egress(cx);
            self.refresh_coworker_computer_quietly(cx);
        }
        // And landing on Settings → Skills the same way: the library is the account's, nothing
        // else fetches it, and a page that arrived here through back or forward would say "No
        // skills yet" about a library nobody had asked for.
        if self.is_app_settings_open && self.app_settings_tab == AppSettingsTab::Skills {
            self.refresh_skills(cx);
        }
        // After `select_coworker`, which lands on the chat: the page is where the person was.
        self.page = loc.page;
        if self.page == MainPage::Recipes {
            self.refresh_recipes(cx);
        }
        self.nav.applying = false;
        cx.notify();
    }

    pub fn touch_coworker_activity(&mut self, id: &str) {
        self.last_active_at
            .insert(id.to_string(), SystemTime::now());
        if let Some(conversation) = self.conversations.iter_mut().find(|c| c.id == id) {
            conversation.updated_at = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        }
    }

    /// Most recent message first; idle bots (no messages) by created date, newest first.
    pub fn ranked_coworkers(&self) -> Vec<Coworker> {
        let mut list = self.coworkers.clone();
        list.sort_by(|a, b| self.coworker_rank(b).cmp(&self.coworker_rank(a)));
        list
    }

    fn coworker_rank(&self, coworker: &Coworker) -> (u8, u128, i64) {
        match self.coworker_activity_ms(coworker) {
            Some(ms) => (1, ms, self.coworker_created_ms(coworker)),
            None => (0, 0, self.coworker_created_ms(coworker)),
        }
    }

    fn coworker_activity_ms(&self, coworker: &Coworker) -> Option<u128> {
        if let Some(at) = self.last_active_at.get(&coworker.id) {
            return Some(system_time_ms(*at));
        }
        let conversation = self.conversations.iter().find(|c| c.id == coworker.id)?;
        if let Some(message) = conversation
            .messages
            .iter()
            .rev()
            .find(|m| !m.content.trim().is_empty())
        {
            return Some(system_time_ms(message.sent_at));
        }
        let updated = parse_sql_time(&conversation.updated_at)?;
        let created = parse_sql_time(&conversation.created_at);
        if created.is_some_and(|c| updated > c) {
            Some(system_time_ms(updated))
        } else {
            None
        }
    }

    fn coworker_created_ms(&self, coworker: &Coworker) -> i64 {
        if coworker.updated_at_ms > 0 {
            return coworker.updated_at_ms;
        }
        self.conversations
            .iter()
            .find(|c| c.id == coworker.id)
            .and_then(|c| parse_sql_time(&c.created_at))
            .map(|t| system_time_ms(t) as i64)
            .unwrap_or(0)
    }

    pub fn toggle_pin_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        if !self.pinned_coworker_ids.remove(&id) {
            self.pinned_coworker_ids.insert(id);
        }
        cx.notify();
    }

    pub fn hide_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        self.hidden_coworker_ids.insert(id.clone());
        if let Some(coworker) = self.coworkers.iter_mut().find(|c| c.id == id) {
            coworker.hidden_from_sidebar = true;
        }
        self.persist_hidden(&id, true, cx);
        if self.active_coworker_id.as_ref() == Some(&id) {
            let next = self
                .ranked_coworkers()
                .into_iter()
                .map(|c| c.id)
                .find(|other| other != &id && !self.hidden_coworker_ids.contains(other));
            if let Some(next) = next {
                self.select_coworker(next, cx);
                return;
            }
            self.active_coworker_id = None;
        }
        cx.notify();
    }

    pub fn unhide_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        self.hidden_coworker_ids.remove(&id);
        if let Some(coworker) = self.coworkers.iter_mut().find(|c| c.id == id) {
            coworker.hidden_from_sidebar = false;
        }
        self.persist_hidden(&id, false, cx);
        cx.notify();
    }

    pub fn open_hidden_bots(&mut self, cx: &mut Context<Self>) {
        self.hidden_bots_open = true;
        cx.notify();
    }

    pub fn close_hidden_bots(&mut self, cx: &mut Context<Self>) {
        if self.hidden_bots_open {
            self.hidden_bots_open = false;
            cx.notify();
        }
    }

    fn persist_hidden(&mut self, id: &str, hidden: bool, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let id = id.to_string();
        let patch = CoworkerPatch {
            hidden_from_sidebar: Some(hidden),
            ..Default::default()
        };
        cx.spawn(async move |this, cx| {
            let result = client.patch_coworker(&id, &patch).await;
            let _ = this.update(cx, |state, cx| {
                if let Err(error) = result {
                    eprintln!("hide coworker: {}", error.message);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn mark_coworker_read(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(conversation) = self.conversations.iter_mut().find(|c| c.id == id) {
            conversation.unread_count = 0;
        }
        cx.notify();
    }

    pub fn begin_rename_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        self.renaming_coworker_id = Some(id);
        cx.notify();
    }

    pub fn cancel_rename_coworker(&mut self, cx: &mut Context<Self>) {
        if self.renaming_coworker_id.take().is_some() {
            cx.notify();
        }
    }

    pub fn commit_rename_coworker(&mut self, name: String, cx: &mut Context<Self>) {
        let Some(id) = self.renaming_coworker_id.take() else {
            return;
        };
        let name = name.trim().to_string();
        if name.is_empty() {
            cx.notify();
            return;
        }
        self.select_coworker(id, cx);
        self.patch_active_agent(
            CoworkerPatch {
                name: Some(name),
                ..Default::default()
            },
            cx,
        );
    }

    pub fn open_agent_profile(&mut self, id: String, cx: &mut Context<Self>) {
        self.select_coworker(id, cx);
        self.set_right_pane(RightPane::Settings, cx);
        self.computer_view = ComputerView::Overview;
        cx.notify();
    }

    pub fn duplicate_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        if self.hiring {
            return;
        }
        let Some(source) = self.coworkers.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        self.hiring = true;
        self.auth_error = None;
        cx.notify();
        let name = if source.name.trim().is_empty() {
            "New Bot".to_string()
        } else {
            format!("{} copy", source.name.trim())
        };
        let model = if source.model.is_empty() {
            None
        } else {
            Some(source.model.clone())
        };
        let patch = CoworkerPatch {
            model: model.clone(),
            role: source.role.clone(),
            title: source.title.clone(),
            avatar_shape: source.avatar_shape.clone(),
            avatar_color: source.avatar_color.clone(),
            notify_on_updates: source.notify_on_updates,
            ..Default::default()
        };
        cx.spawn(async move |this, cx| {
            let hired = client.hire(&name, model.as_deref()).await;
            let _ = this.update(cx, |state, cx| {
                state.hiring = false;
                match hired {
                    Ok(hired) => {
                        let new_id = hired.id.clone();
                        state.coworkers.insert(0, hired);
                        if !patch.is_empty() {
                            state.active_coworker_id = Some(new_id.clone());
                            state.patch_active_agent(patch, cx);
                        }
                        state.select_coworker(new_id, cx);
                    }
                    Err(error) => state.note_failure(&error, cx),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn delete_coworker(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        self.coworkers.retain(|c| c.id != id);
        self.pinned_coworker_ids.remove(&id);
        self.hidden_coworker_ids.remove(&id);
        if self.renaming_coworker_id.as_ref() == Some(&id) {
            self.renaming_coworker_id = None;
        }
        if self.active_coworker_id.as_ref() == Some(&id) {
            self.active_coworker_id = self.coworkers.first().map(|c| c.id.clone());
            if let Some(next) = self.active_coworker_id.clone() {
                self.select_coworker(next, cx);
            } else {
                self.close_right_pane(cx);
            }
        }
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = client.delete_coworker(&id).await;
            let _ = this.update(cx, |state, cx| {
                if let Err(error) = result {
                    state.note_failure(&error, cx);
                    state.refresh_coworkers(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn create_new_session(&mut self, cx: &mut Context<Self>) {
        if let Some(db) = self.database_service.clone() {
            cx.spawn(
                async move |this, cx| match db.create_session("New Chat").await {
                    Ok(id) => {
                        this.update(cx, |state, cx| {
                            state.conversations.insert(
                                0,
                                Conversation {
                                    id: id.clone(),
                                    title: "New Chat".to_string(),
                                    created_at: chrono::Local::now()
                                        .format("%Y-%m-%d %H:%M:%S")
                                        .to_string(),
                                    updated_at: chrono::Local::now()
                                        .format("%Y-%m-%d %H:%M:%S")
                                        .to_string(),
                                    messages: Vec::new(),
                                    unread_count: 0,
                                },
                            );
                            state.select_conversation(id, cx);
                        })
                        .ok();
                    }
                    Err(e) => eprintln!("Failed to create session: {}", e),
                },
            )
            .detach();
        }
    }

    pub fn rename_session(&mut self, id: String, new_title: String, cx: &mut Context<Self>) {
        if let Some(conversation) = self.conversations.iter_mut().find(|c| c.id == id) {
            conversation.title = new_title.clone();
            cx.notify();

            if let Some(db) = self.database_service.clone() {
                cx.spawn(async move |_this, _cx| {
                    if let Err(e) = db.update_session_title(&id, &new_title).await {
                        eprintln!("Failed to rename session: {}", e);
                    }
                })
                .detach();
            }
        }
    }

    pub fn delete_session(&mut self, id: String, cx: &mut Context<Self>) {
        if let Some(index) = self.conversations.iter().position(|c| c.id == id) {
            self.conversations.remove(index);

            // If we deleted the active conversation, select another one
            if self.active_conversation_id.as_ref() == Some(&id) {
                self.active_conversation_id = self.conversations.first().map(|c| c.id.clone());
                if let Some(new_id) = self.active_conversation_id.clone() {
                    self.load_session_messages(new_id, cx);
                }
            }

            cx.notify();

            if let Some(db) = self.database_service.clone() {
                cx.spawn(async move |_this, _cx| {
                    if let Err(e) = db.delete_session(&id).await {
                        eprintln!("Failed to delete session: {}", e);
                    }
                })
                .detach();
            }
        }
    }

    /// Refill a thread from the database.
    ///
    /// Whether the rows are taken is decided when they arrive rather than when they are asked
    /// for, because a turn can begin while the query is still in the air, and the thread it
    /// begins in is one this must not touch. `apply_reload` is where that is judged.
    pub fn load_session_messages(&mut self, session_id: String, cx: &mut Context<Self>) {
        if let Some(db) = self.database_service.clone() {
            let session_id_clone = session_id.clone();
            cx.spawn(
                async move |this, cx| match db.get_messages(&session_id_clone).await {
                    Ok(db_messages) => {
                        this.update(cx, |state, cx| {
                            let live = state.live_turns.get(&session_id_clone).cloned();
                            if let Some(conversation) = state
                                .conversations
                                .iter_mut()
                                .find(|c| c.id == session_id_clone)
                            {
                                apply_reload(conversation, live.as_ref(), db_messages);
                                state.sync_pending_approvals(cx);
                                cx.notify();
                            }
                            // Only once the cache has been painted, so the server's answer is
                            // corrected onto a thread rather than racing the rows it corrects.
                            state.reconcile_thread(&session_id_clone, cx);
                        })
                        .ok();
                    }
                    Err(e) => {
                        eprintln!("Failed to load messages: {}", e);
                        let _ = this.update(cx, |state, cx| {
                            state.sync_pending_approvals(cx);
                            state.reconcile_thread(&session_id_clone, cx);
                        });
                    }
                },
            )
            .detach();
        } else {
            self.sync_pending_approvals(cx);
            self.reconcile_thread(&session_id, cx);
        }
    }

    /// Bring a thread up to what the server says was said in it.
    ///
    /// The app has been treating its own SQLite as the record of a conversation, which it never
    /// was: the turn happens on the server, and the app is one of the things that may or may not
    /// have been watching. So the database is a cache — it paints the thread at once and works
    /// with no network — and this is where the record corrects it. A turn that ran while the
    /// person was in another thread, or while the app was not running at all, is found here and
    /// nowhere else.
    ///
    /// Once per thread per session, and only for the thread being read. After that the app has
    /// been watching, and every turn since has gone to disk through the same door.
    /// Bring a thread up to what the server says was said in it.
    ///
    /// SQLite is a cache of words and pinned feed shots. User-form cards live
    /// on OpenGrok (`formRequest` + `formResolution`) and are folded on every
    /// visit (bot switch / thread load), not only the first reconcile.
    fn reconcile_thread(&mut self, conversation_id: &str, cx: &mut Context<Self>) {
        if self.active_conversation_id.as_deref() != Some(conversation_id) {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let conversation_id = conversation_id.to_string();
        cx.spawn(async move |this, cx| {
            let thread = client.replay_thread(&conversation_id, RECONCILE_RUNS).await;
            let _ = this.update(cx, |state, cx| {
                match thread {
                    Ok(thread) => {
                        state.reconciled_threads.insert(conversation_id.clone());
                        state.hide_withheld_runs(&conversation_id, &thread, cx);
                        state.apply_thread_replay(&conversation_id, &thread, cx);
                        state.overlay_replay_cards(&conversation_id, &thread.runs);
                    }
                    Err(_) => {
                        state.reconciled_threads.remove(&conversation_id);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Put out of sight the turns this account hid somewhere else, and tell the server about
    /// any it has not heard of.
    ///
    /// Each machine keeps its own copy of a thread, so a turn deleted on one would go on being
    /// painted by the others from their caches. The server names what it withheld; this is the
    /// other half of that sentence. The row stays, as it does for a delete made here, so the
    /// thread still knows it has accounted for that run.
    ///
    /// It heals the other direction too. A delete asks the server to withhold the turn, and
    /// that ask can fail — a flaky connection, an app closed a moment later. The server is
    /// still offering a turn this machine has hidden, which is exactly how that failure looks
    /// from here, so it is asked again. Nothing has to be remembered between runs of the app
    /// for that: the rows say what was hidden, and the answer says what the server knows.
    fn hide_withheld_runs(
        &mut self,
        conversation_id: &str,
        thread: &ThreadReplay,
        cx: &mut Context<Self>,
    ) {
        let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        else {
            return;
        };
        let unheard = unheard_hidden_runs(&conversation.messages, &thread.runs);
        let newly_hidden =
            hide_messages_of_runs(&mut conversation.messages, &thread.hidden_run_ids);
        if !unheard.is_empty()
            && let Some(client) = self.opengrok.clone()
        {
            cx.spawn(async move |_, _| {
                for run_id in unheard {
                    if let Err(error) = client.hide_run(&run_id).await {
                        log::warn!("could not tell the server a turn was hidden: {error}");
                    }
                }
            })
            .detach();
        }
        if let Some(db) = self.database_service.clone()
            && !newly_hidden.is_empty()
        {
            // The marks go to disk too, so reopening this thread does not paint them again
            // before the next reconcile has run.
            cx.spawn(async move |_, _| {
                for id in newly_hidden {
                    if let Err(error) = db.hide_message(&id).await {
                        eprintln!("Failed to hide a message the server withheld: {error}");
                    }
                }
            })
            .detach();
        }
    }

    /// The runs the server has that this thread has not, put back into it.
    ///
    /// A run that ended goes onto the thread and into the database, through the same
    /// `persist_assistant_reply` a turn watched to the end goes through. Pinned
    /// feed shots (`transcript`/`failure`/`end`) go to sqlite; `overlay_replay_cards`
    /// still folds user-form cards and any pins the cache dropped. A run still going is a turn that
    /// outlived whatever stopped watching it — a bot switch, or the app closing — and is
    /// re-attached to: the bubble comes back, the status line comes back, and the rest of
    /// the turn arrives in it.
    fn apply_thread_replay(
        &mut self,
        conversation_id: &str,
        thread: &ThreadReplay,
        cx: &mut Context<Self>,
    ) {
        let Some(conversation) = self.conversations.iter().find(|c| c.id == conversation_id) else {
            return;
        };
        // A turn named as withheld is not grafted, whatever else the answer carries. The
        // server leaves hidden runs out of the list, and this thread is not painted from a
        // promise: a run named in both places was hidden on some machine, and putting it back
        // is the one thing this must never do.
        let offered: Vec<ThreadRun> = if thread.hidden_run_ids.is_empty() {
            thread.runs.clone()
        } else {
            let hidden: HashSet<&str> = thread.hidden_run_ids.iter().map(String::as_str).collect();
            thread
                .runs
                .iter()
                .filter(|run| !hidden.contains(run.run_id.as_str()))
                .cloned()
                .collect()
        };
        let missing = missing_replies(&conversation.messages, &offered);
        if missing.is_empty() {
            return;
        }
        for reply in missing {
            let Some(conversation) = self
                .conversations
                .iter_mut()
                .find(|c| c.id == conversation_id)
            else {
                return;
            };
            let grafted_id = graft_reply(&mut conversation.messages, &reply);
            let message_id = grafted_id.clone();
            if reply.live {
                // Whatever stopped watching this run, the run did not stop. Registering it makes
                // the thread hold still for it again, and following it brings the rest of the
                // turn in.
                self.live_turns.insert(
                    conversation_id.to_string(),
                    LiveTurn {
                        run_id: reply.run_id.clone(),
                        message_id,
                        persisting: false,
                    },
                );
                self.begin_responding(Some(conversation_id), "Working");
                self.follow_run(reply.run_id.clone(), Some(conversation_id.to_string()), cx);
            } else {
                self.persist_assistant_reply(
                    conversation_id,
                    &grafted_id,
                    reply.content,
                    &reply.parts,
                    Some(&reply.run_id),
                    cx,
                );
            }
        }
        cx.notify();
    }

    /// Idle + settled user-form cards (never secrets) from
    /// `formRequest` + sibling `formResolution`, local save prompts, and
    /// pinned screenshots from OpenGrok onto rows sqlite already has.
    fn overlay_replay_cards(&mut self, conversation_id: &str, runs: &[ThreadRun]) {
        let mut grafted: Vec<(String, Vec<ChatPart>)> = Vec::new();
        for run in runs.iter().filter(|run| !run.run_id.trim().is_empty()) {
            let (_, parts) = reply_from_replay(&run.events, &run.status);
            let parts = self.graft_user_forms(parts);
            grafted.push((run.run_id.clone(), parts));
        }
        let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return;
        };
        for (run_id, parts) in &grafted {
            if let Some(message) = conversation
                .messages
                .iter_mut()
                .find(|message| message.run_id.as_deref() == Some(run_id.as_str()))
            {
                overlay_server_cards(message, parts);
            }
        }
        for (_, parts) in &grafted {
            for part in parts {
                if let ChatPart::UserForm(spec) = part {
                    self.remember_user_form_resolution(spec);
                }
            }
        }
        if let Some(shot) = grafted
            .iter()
            .rev()
            .flat_map(|(_, parts)| parts.iter())
            .rev()
            .find_map(|part| match part {
                ChatPart::Screenshot(spec) => Some(spec.clone()),
                _ => None,
            })
        {
            self.last_box_shot = Some(shot);
        }
    }

    pub fn select_conversation(&mut self, conversation_id: String, cx: &mut Context<Self>) {
        self.active_conversation_id = Some(conversation_id.clone());
        self.load_session_messages(conversation_id.clone(), cx);
        // Coming back to a thread whose turn never stopped. What the app is still holding is a
        // guess about a run it stopped watching; the server holds the run itself, so that is
        // what the thread is rebuilt from — including the case where the turn ended out of
        // sight, which is the only chance there is to write that ending down.
        self.resync_live_turn(&conversation_id, cx);
        self.sync_pending_approvals(cx);
        // A message held while this thread was out of sight goes now if the thread is idle.
        self.drain_queued_send(&conversation_id, cx);
        cx.notify();
    }

    /// Re-attach a thread to the run the server is keeping for it.
    ///
    /// `GET /ag-ui/runs/{run_id}` answers with every frame the run emitted, in order, whether or
    /// not anybody was listening when it did — so this is the whole of the user's instinct made
    /// real: they should not have to stay on a thread for its turn to survive, because the turn
    /// was never the app's to keep in the first place.
    fn resync_live_turn(&mut self, conversation_id: &str, cx: &mut Context<Self>) {
        let Some(turn) = self.live_turns.get(conversation_id).cloned() else {
            return;
        };
        if turn.run_id.trim().is_empty() {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let conversation_id = conversation_id.to_string();
        cx.spawn(async move |this, cx| {
            let replay = client.replay_run(&turn.run_id).await;
            let _ = this.update(cx, |state, cx| {
                match replay {
                    Ok(replay) => state.apply_replayed_turn(&conversation_id, &turn, &replay, cx),
                    Err(_) => {
                        // The server has no such run *yet*. A run is recorded once the turn is
                        // past whatever it waits on first — a sleeping box takes up to 90 s to
                        // wake — and the stream that sent it stays open the whole time, so the
                        // turn is kept: that stream ends it, with the reply or with the reason
                        // there is none. Letting go here was the bug (21 Sep 2026): the reply
                        // then arrived to a thread that had stopped listening, was thrown away
                        // as somebody else's, and the line said "working" until the app was
                        // restarted. A turn that truly never started ends the same way, from
                        // its own stream's error, so nothing here has to guess.
                        eprintln!(
                            "NativeChat: the server has no run {} yet; the turn stays open",
                            turn.run_id
                        );
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A thread repainted from what the server says its run came to.
    ///
    /// A run still going is only painted: the stream, if it is still attached, is a frame or two
    /// ahead and will say so shortly. A run that is over is also written down — through the same
    /// `persist_assistant_reply` a turn watched to the end goes through, so the pieces reach
    /// `chat_message_parts` and the screenshots are still there the next time the thread is
    /// opened. That door is used once: if the live path already decided what to save, this one
    /// only paints, because a turn saved twice is a thread that says everything twice.
    fn apply_replayed_turn(
        &mut self,
        conversation_id: &str,
        turn: &LiveTurn,
        replay: &RunReplay,
        cx: &mut Context<Self>,
    ) {
        let (plain, parts) = reply_from_replay(&replay.events, &replay.status);
        let parts = self.graft_user_forms(parts);
        let plain = replayed_ending(
            &replay.events,
            &replay.status,
            replay.failure.as_deref(),
            &plain,
        )
        .unwrap_or(plain);
        if let Some(message) =
            streaming_message_mut(&mut self.conversations, conversation_id, &turn.message_id)
        {
            message.content = plain.clone();
            message.parts = parts.clone();
        }
        if let Some(shot) = parts.iter().rev().find_map(|part| match part {
            ChatPart::Screenshot(spec) => Some(spec.clone()),
            _ => None,
        }) {
            self.last_box_shot = Some(shot);
        }
        // A card whose command already ran is not a question any more, so the thread stops
        // asking it.
        for part in &parts {
            if let ChatPart::Approval(spec) = part
                && spec.output.is_some()
            {
                self.approval_decisions
                    .insert(spec.call_id.clone(), ApprovalDecision::AllowOnce);
            }
        }
        match replay.status.as_str() {
            "running" => {
                let activity = activity_from_replay(&replay.events).unwrap_or(BotActivity {
                    label: "Working".into(),
                });
                self.begin_responding(Some(conversation_id), &activity.label);
            }
            "awaiting-approval" => {
                self.finish_responding(Some(conversation_id), true);
                self.fill_open_approval_commands(cx);
            }
            "finished" => {
                if self.has_open_user_form(conversation_id) {
                    self.park_waiting_for_you(conversation_id, &turn.run_id);
                } else {
                    self.finish_responding(Some(conversation_id), false);
                }
                // Only when nobody has written this turn down yet. The live stream may have come
                // back and settled it while the replay was in the air, and a turn saved twice is
                // a thread that says everything twice.
                if !self.has_open_user_form(conversation_id)
                    && !self.has_open_approval(conversation_id)
                    && self.turn_is_unsettled(conversation_id, &turn.run_id)
                {
                    self.persist_assistant_reply(
                        conversation_id,
                        &turn.message_id,
                        plain,
                        &parts,
                        Some(&turn.run_id),
                        cx,
                    );
                }
            }
            "failed" => {
                self.finish_responding(Some(conversation_id), false);
                self.release_live_turn(conversation_id, &turn.run_id);
            }
            _ => {}
        }
        self.collect_handoff_ids_and_flush(conversation_id, cx);
    }

    /// This thread is no longer answerable for a turn.
    ///
    /// The other way out is `persist_assistant_reply`, which lets go once the reply reaches the
    /// database. This one is for the turns that end with nothing to write down — a run that
    /// failed leaves a line the app wrote about it, and a line the app wrote is never saved — so
    /// waiting for a write that will never come would strand the thread on its own reply
    /// forever.
    ///
    /// Named by run, because the turn being let go has to be the turn that ended: a thread whose
    /// next turn has already begun must not be let go by the last one finishing late.
    fn release_live_turn(&mut self, conversation_id: &str, run_id: &str) {
        if self
            .live_turns
            .get(conversation_id)
            .is_some_and(|turn| turn.run_id == run_id)
        {
            self.live_turns.remove(conversation_id);
        }
    }

    /// This thread's turn is still `run_id`'s, and nothing has decided yet what to write down
    /// for it.
    ///
    /// Four things can arrive at the end of one turn — the stream it went out on, the replay a
    /// thread re-attaches to, the poll that follows a resumed run, and the person pressing stop
    /// — and whichever gets there first settles it. The others have to be able to tell that they
    /// are late, because an ending painted twice is a turn saved twice, and an ending painted
    /// over a stop is the stop undone.
    fn turn_is_unsettled(&self, conversation_id: &str, run_id: &str) -> bool {
        self.live_turns
            .get(conversation_id)
            .is_some_and(|turn| !turn.persisting && turn.run_id == run_id)
    }

    /// The open thread has a turn in flight: the coworker is still doing something.
    ///
    /// Read off `live_turns`, which is the record of a turn being in flight and is kept per
    /// thread. That is what makes it worth trusting: it is still true of this thread after
    /// looking at another bot and coming back, and it is not disturbed by what some other bot is
    /// doing meanwhile. The working line beside this button is now kept the same way — it was
    /// once one label for the whole app, so the second bot to start a turn took it from the
    /// first and the two indicators disagreed about the same fact.
    ///
    /// A turn whose outcome is decided and on its way to disk is not in flight. It stays
    /// registered until the write lands, because the thread is held out of reload for that long,
    /// but nothing is running any more and the composer must not go on offering to stop it.
    pub fn is_turn_in_flight(&self) -> bool {
        self.turn_to_stop().is_some()
    }

    fn conversation_title(&self, id: &str) -> String {
        self.conversations
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.title.clone())
            .unwrap_or_else(|| id.to_string())
    }

    /// Keep the coworker's reply so the thread survives a relaunch.
    ///
    /// Only what the coworker actually said: a status line is the app's own words about the turn,
    /// and saving it would put a line nobody spoke into the history every later turn is sent.
    ///
    /// The pieces go with it. A recipe run is several bubbles with pictures of the box's screen
    /// between them, and a reply flattened to its text would come back as one long paragraph.
    ///
    /// The run id goes with it. A reply on disk that cannot say which run it came out of is a
    /// reply a later reconcile cannot recognise, and the thread would grow a second copy of it
    /// every time the app was opened.
    ///
    /// This is also where a turn stops being in flight, because that is the same event: the
    /// thread is held out of reload exactly while the database does not yet have the turn. A
    /// reply this refuses — the app's own status line, or nothing at all — is never going to
    /// reach the database, so the thread is let go at once; a reply on its way down is let go
    /// when the write lands, so that switching away in the moment between deciding and writing
    /// cannot lose it either.
    #[allow(clippy::too_many_arguments)]
    fn persist_assistant_reply(
        &mut self,
        conversation_id: &str,
        message_id: &str,
        content: String,
        parts: &[ChatPart],
        run_id: Option<&str>,
        cx: &mut Context<Self>,
    ) {
        let settles = run_id.is_some_and(|run_id| {
            self.live_turns
                .get(conversation_id)
                .is_some_and(|turn| turn.run_id == run_id)
        });
        if content.trim().is_empty() || is_status_line(&content) {
            if settles {
                self.live_turns.remove(conversation_id);
            }
            return;
        }
        let Some(db) = self.database_service.clone() else {
            if settles {
                self.live_turns.remove(conversation_id);
            }
            return;
        };
        if settles && let Some(turn) = self.live_turns.get_mut(conversation_id) {
            turn.persisting = true;
        }
        let title = self.conversation_title(conversation_id);
        // A reply the person hid while it was still being typed out has no row yet: the mark
        // rides along with the write rather than waiting for a row that is not there.
        let bubble = self
            .conversations
            .iter()
            .find(|c| c.id == conversation_id)
            .and_then(|c| c.messages.iter().find(|m| m.id == message_id));
        let hidden = bubble.is_some_and(|m| m.hidden);
        // The turn's own time, so a reply recovered from the server keeps the place it had
        // rather than landing at the bottom of the thread on the next load.
        let sent_at = bubble.map_or_else(SystemTime::now, |m| m.sent_at);
        let conversation_id = conversation_id.to_string();
        let message_id = message_id.to_string();
        let run_id = run_id.map(str::to_string);
        let parts = saved_parts(parts);
        cx.spawn(async move |this, cx| {
            let saved = match db.ensure_session(&conversation_id, &title).await {
                // The row is the bubble's, by its own name. Two paths settling one run write
                // the same row twice rather than two rows that say the same thing.
                Ok(()) => {
                    db.save_message(
                        &message_id,
                        &conversation_id,
                        "assistant",
                        &content,
                        None,
                        None,
                        None,
                        &parts,
                        run_id.as_deref(),
                        hidden,
                        sent_at,
                    )
                    .await
                }
                Err(error) => Err(error),
            };
            if let Err(error) = saved {
                eprintln!("Failed to save assistant message: {error}");
            }
            if settles {
                let _ = this.update(cx, |state, _| {
                    if let Some(run_id) = run_id.as_deref() {
                        state.release_live_turn(&conversation_id, run_id);
                    }
                });
            }
        })
        .detach();
    }

    fn send_opengrok_turn(
        &mut self,
        conversation_id: String,
        content: String,
        cx: &mut Context<Self>,
    ) {
        let recipe = self.active_recipe.as_ref().map(ActiveRecipe::turn);
        // No skill. The only caller is "Try again", which re-runs a turn the thread already
        // has rather than building one out of the draft: the words are the ones that were
        // sent, and what the composer is holding now belongs to the message still being
        // written. Taking it here sent somebody's skill on a turn they never attached it to
        // and left the chip standing over a draft with nothing behind it.
        self.send_opengrok_turn_with(conversation_id, content, recipe, None, None, cx);
    }

    /// `recipe` and `skill` are what the message was typed with. `stop_first` is a run this turn
    /// replaces: it is stopped on the wire before the turn is posted.
    fn send_opengrok_turn_with(
        &mut self,
        conversation_id: String,
        content: String,
        recipe: Option<TurnRecipe>,
        skill: Option<String>,
        stop_first: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            self.auth_error = Some("OpenGrok is not configured".to_string());
            cx.notify();
            return;
        };
        // The other door into this is "Try again" on a turn that did not go out, which reaches
        // here without passing `send_message`'s guard — and it is exactly the button somebody
        // presses while the session is gone. A turn is a turn: it does not leave while the app
        // knows it has nothing to sign it with.
        if !self.can_send_turn() {
            self.note_signed_out(cx);
            return;
        }
        let coworker_id = self.active_coworker_id.clone();
        let history: Vec<AguiMessage> = self
            .conversations
            .iter()
            .find(|c| c.id == conversation_id)
            .map(|c| agui_messages(&c.messages))
            .unwrap_or_default();

        // Both ids are minted here, before anything is sent. The run id because the server files
        // every frame under it and this is the app's only handle on the run once the stream is
        // gone; the message id because the run has to be able to find the bubble it is filling
        // in by name, whatever else happens to the thread meanwhile.
        let run_id = uuid::Uuid::now_v7().to_string();
        let reply_id = uuid::Uuid::now_v7().to_string();
        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        {
            conversation.messages.push(Message {
                id: reply_id.clone(),
                sender: "AI".to_string(),
                content: String::new(),
                sent_at: SystemTime::now(),
                is_me: false,
                reply_preview: None,
                reply_to_id: None,
                reply_is_me: false,
                parts: Vec::new(),
                run_id: Some(run_id.clone()),
                hidden: false,
            });
        }
        self.live_turns.insert(
            conversation_id.clone(),
            LiveTurn {
                run_id: run_id.clone(),
                message_id: reply_id.clone(),
                persisting: false,
            },
        );
        if let Some(id) = self.active_coworker_id.clone() {
            self.touch_coworker_activity(&id);
        }
        self.begin_responding(Some(&conversation_id), "Thinking");
        cx.notify();

        cx.spawn(async move |this, cx| {
            let coworker = match coworker_id.clone() {
                Some(id) => Ok(id),
                None => match client.hire("NativeChat", None).await {
                    Ok(hired) => {
                        let id = hired.id.clone();
                        let _ = this.update(cx, |state, _| {
                            state.active_coworker_id = Some(id.clone());
                            state.coworkers = vec![hired];
                        });
                        Ok(id)
                    }
                    Err(error) => Err(error),
                },
            };
            let (result, waiting_approval, waiting_user_form, deeds) = match coworker {
                Ok(id) => {
                    // A forced send over a running turn: that turn is stopped here, in order,
                    // before this one is posted. 404 means it had already ended.
                    if let Some(stopped) = stop_first.as_deref()
                        && let Err(error) = client.stop_run(stopped).await
                        && error.status != Some(404)
                    {
                        eprintln!(
                            "NativeChat: the stop before a forced send did not land: {error}"
                        );
                    }
                    let mut tracker = ToolCallTracker::default();
                    let mut assembler = TurnAssembler::default();
                    let mut last_stream_paint: Option<Instant> = None;
                    let mut last_stream_sig = (0usize, 0u8);
                    let result = client
                        .run_turn(
                            &id,
                            &conversation_id,
                            &run_id,
                            &history,
                            recipe.as_ref(),
                            skill.as_deref(),
                            |event| {
                                match tracker.tick(event) {
                                    ActivityTick::Keep => {}
                                    tick => {
                                        let _ = this.update(cx, |state, cx| {
                                            // A frame from a run the person has stopped or sent
                                            // past must not write over the turn that replaced it.
                                            if !state.turn_is_unsettled(&conversation_id, &run_id) {
                                                return;
                                            }
                                            // The frame is this thread's news, and it is written
                                            // into this thread's line whoever else is working.
                                            let before = state
                                                .thread_status(&conversation_id)
                                                .map(str::to_string);
                                            state.apply_turn_status(Some(&conversation_id), tick);
                                            if state.thread_status(&conversation_id)
                                                != before.as_deref()
                                            {
                                                cx.notify();
                                            }
                                        });
                                    }
                                }
                                assembler.push_event(&event);
                                let (plain, parts) = assembler.snapshot();
                                let box_shot = assembler.latest_screenshot().cloned();
                                let sig = stream_part_sig(&parts);
                                let now = Instant::now();
                                let paint =
                                    stream_paint_due(last_stream_paint, now, last_stream_sig, sig);
                                let _ = this.update(cx, |state, cx| {
                                    // A run the person stopped or sent past has no row any more;
                                    // its late frames must not graft into the turn that replaced it.
                                    if !state.turn_is_unsettled(&conversation_id, &run_id) {
                                        return;
                                    }
                                    // Into the thread the run belongs to, and into the row the run
                                    // was given — not the thread that happens to be open, and not
                                    // whichever row happens to be last in it.
                                    if let Some(shot) = box_shot {
                                        state.last_box_shot = Some(shot);
                                    }
                                    let grafted = state.graft_user_forms(parts.clone());
                                    if let Some(message) = streaming_message_mut(
                                        &mut state.conversations,
                                        &conversation_id,
                                        &reply_id,
                                    ) {
                                        message.content = plain.clone();
                                        message.parts = grafted;
                                        // Tokens update the row every frame; notify at ~60Hz
                                        // or when a card/picture lands, not on every SSE event.
                                        if paint {
                                            cx.notify();
                                        }
                                    }
                                    state.collect_handoff_ids_and_flush(&conversation_id, cx);
                                    if assembler.waiting_approval() {
                                        let open = parts.iter().rev().find_map(|part| match part {
                                            ChatPart::Approval(spec) => Some(spec.clone()),
                                            _ => None,
                                        });
                                        if let Some(spec) = open {
                                            if let Some(resolution) =
                                                state.auto_resolve_local_exec(&spec)
                                            {
                                                state.answer_approval(spec, resolution, cx);
                                            }
                                        }
                                    }
                                });
                                if paint {
                                    last_stream_paint = Some(now);
                                    last_stream_sig = sig;
                                }
                            },
                        )
                        .await;
                    assembler.finish();
                    let waiting_approval = assembler.waiting_approval();
                    let waiting_user_form = assembler.waiting_user_form();
                    // What the tools did, in case the turn ends without a word about it.
                    let deeds = tracker.deeds();
                    let (plain, parts) = assembler.snapshot();
                    let box_shot = assembler.latest_screenshot().cloned();
                    let _ = this.update(cx, |state, cx| {
                        if let Some(shot) = box_shot {
                            state.last_box_shot = Some(shot);
                        }
                        let grafted = state.graft_user_forms(parts);
                        if let Some(message) = streaming_message_mut(
                            &mut state.conversations,
                            &conversation_id,
                            &reply_id,
                        ) {
                            message.content = plain;
                            message.parts = grafted;
                        }
                        state.collect_handoff_ids_and_flush(&conversation_id, cx);
                        cx.notify();
                    });
                    let waiting_form = this
                        .update(cx, |state, _| state.has_open_user_form(&conversation_id))
                        .unwrap_or(waiting_user_form);
                    (result, waiting_approval, waiting_form, deeds)
                }
                Err(error) => (Err(error), false, false, Vec::new()),
            };
            let _ = this.update(cx, |state, cx| {
                // Something else may already have decided what this turn came to: the person
                // stopped it, a replay settled it while the stream was still open, or the
                // thread has moved on to a later turn. Painting an ending now would write over
                // the one that is there — and in the stopped case it would report the person's
                // own stop as a run that failed, then save the turn a second time.
                match turn_ending(state.live_turns.get(&conversation_id), &run_id) {
                    TurnEnding::Ours => {}
                    TurnEnding::Superseded => {
                        if let Err(error) = &result {
                            eprintln!("NativeChat: the turn failed: {}", error.message);
                        }
                        return;
                    }
                    TurnEnding::Orphaned => {
                        // Nobody holds the thread, so nobody else will put out the status line
                        // this run lit. Left alone it read "working" until the app was
                        // restarted, and every message sent meanwhile queued behind it.
                        if let Err(error) = &result {
                            eprintln!("NativeChat: the turn failed: {}", error.message);
                        }
                        let waiting = is_waiting_on_person(state.thread_status(&conversation_id));
                        if !waiting {
                            state.finish_responding(Some(&conversation_id), false);
                        }
                        cx.notify();
                        return;
                    }
                }
                if let Some(message) =
                    streaming_message_mut(&mut state.conversations, &conversation_id, &reply_id)
                {
                    // A turn that ends without words is spoken for by the app: why it
                    // failed, what its tools did, or the note that it said nothing at all.
                    // A picture is not an answer, so a failed run says so even when the
                    // turn left a screenshot behind.
                    if !message.has_text_body() {
                        match &result {
                            Ok(text) if !text.is_empty() => message.content = text.clone(),
                            // Parked on a permission card or a user-form: the turn is not over yet.
                            Ok(_) if waiting_approval || waiting_user_form => {}
                            Ok(_) => {
                                message.content = tool_standin(&deeds)
                                    .unwrap_or_else(|| EMPTY_TURN_NOTE.to_string())
                            }
                            // A turn nothing answered is not a turn that went wrong. The red
                            // line with the server's sentence in it is a verdict, and it stayed
                            // in the transcript long after the wire came back, still naming a
                            // URL that was working again. This row says only that the turn did
                            // not happen, and offers it again; what is broken *now* is the
                            // indicator's business, and the indicator can go away.
                            Err(error) if error.unreachable().is_some() => {
                                message.content = TURN_UNREACHED_NOTE.to_string()
                            }
                            // The server answered and did not know who was asking. Not a verdict
                            // about the turn — it never got as far as being one — so the row
                            // says the turn was not sent, and the banner says what to do. The
                            // red line this replaces was the bug: the server's sentence about
                            // whose spend it was, read as if the model had refused.
                            Err(error) if error.is_signed_out() => {
                                message.content = TURN_SIGNED_OUT_NOTE.to_string()
                            }
                            Err(error) => {
                                message.content = format!("{RUN_ERROR_PREFIX}{}", error.message)
                            }
                        }
                    }
                }
                if !waiting_approval && !waiting_user_form && result.is_ok() {
                    // The run is final; a run parked on a card is saved when it finishes.
                    // A status line is painted, never saved: `persist_assistant_reply` refuses
                    // it, so it cannot become history the model is shown next turn. Settling the
                    // reply is also what lets the thread go: from here it reads from the
                    // database again, because from here the database has the turn.
                    let reply = state
                        .conversations
                        .iter()
                        .find(|c| c.id == conversation_id)
                        .and_then(|c| c.messages.iter().find(|m| m.id == reply_id))
                        .map(|m| (m.content.clone(), m.parts.clone()));
                    if let Some((content, parts)) = reply {
                        state.persist_assistant_reply(
                            &conversation_id,
                            &reply_id,
                            content,
                            &parts,
                            Some(&run_id),
                            cx,
                        );
                    }
                }
                if result.is_err() {
                    // A run that failed leaves the app's own words in the feed and those are
                    // never written down, so there is nothing to wait for: the thread is let go
                    // here instead.
                    state.release_live_turn(&conversation_id, &run_id);
                }
                if waiting_user_form {
                    state.park_waiting_for_you(&conversation_id, &run_id);
                } else if waiting_approval {
                    let open = state
                        .conversations
                        .iter()
                        .find(|c| c.id == conversation_id)
                        .and_then(|c| c.messages.iter().rev().find(|m| !m.is_me))
                        .and_then(|m| {
                            m.parts.iter().rev().find_map(|part| match part {
                                ChatPart::Approval(spec) => Some(spec.clone()),
                                _ => None,
                            })
                        });
                    let auto = open
                        .as_ref()
                        .and_then(|spec| state.auto_resolve_local_exec(spec))
                        .zip(open);
                    if let Some((resolution, spec)) = auto {
                        state.answer_approval(spec, resolution, cx);
                        state.finish_responding(Some(&conversation_id), false);
                    } else {
                        state.finish_responding(Some(&conversation_id), true);
                        state.fill_open_approval_commands(cx);
                        state.sync_pending_approvals(cx);
                    }
                } else {
                    state.finish_responding(Some(&conversation_id), false);
                }
                // A failed turn does not send the next held message into the same failure:
                // the queue keeps them, still marked, until something goes through.
                if result.is_ok() {
                    state.drain_queued_send(&conversation_id, cx);
                }
                // A failed run has already ended the last assistant row, with the server's
                // sentence or with the note that the turn never left; `auth_error` is the
                // sign-in / settings error and the settings pane paints it, so a run's failure
                // must not land there too.
                match &result {
                    // Parked on a card counts: the model asked for the tool, so the answer came
                    // through the gateway like any other.
                    Ok(_) => state.note_gateway_answered(cx),
                    Err(error) => {
                        // Reachability and the session are the two exceptions, and for the same
                        // reason: neither is a verdict about the run. A turn is the one request
                        // that goes the whole way, so it is the freshest word the app has about
                        // both — whether it arrived, and whether the server knew whose it was.
                        if matches!(error.failure(), Failure::OutOfReach(_) | Failure::SignedOut) {
                            state.note_failure(error, cx);
                        }
                        eprintln!("NativeChat: the turn failed: {}", error.message);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The row of the open thread's last turn that did not go through, if its last turn is one.
    ///
    /// Only the last: an older one has messages after it, and re-running the thread from here
    /// would answer the newest message rather than the one that went unanswered.
    pub fn retryable_turn(&self) -> Option<String> {
        if self.is_turn_in_flight() {
            return None;
        }
        // Not while the app has been told its session is gone. Pressing it then sends nothing —
        // the guard on the way out sees to that — so it would be a button that does nothing,
        // under a banner already saying why. It comes back when somebody signs in.
        if self.session.is_expired() {
            return None;
        }
        let conversation = self
            .conversations
            .iter()
            .find(|c| Some(&c.id) == self.active_conversation_id.as_ref())?;
        let last = conversation.messages.iter().rev().find(|m| !m.hidden)?;
        (!last.is_me && is_unsent_turn_note(&last.content)).then(|| last.id.clone())
    }

    /// Send the turn that did not go through, again.
    ///
    /// It is offered rather than done for the person, and that is deliberate. In the plain case
    /// — the app never reached OpenGrok — the request did not leave and nothing ran, so sending
    /// it again is free. But a turn can also fail at the gateway *after* it has been going for a
    /// while, with tools already run against a real computer, and re-sending that one silently
    /// would do those things a second time to somebody who never asked. So the app says the turn
    /// did not go through and waits to be told.
    ///
    /// The person's message is still in the thread and is not sent again; the failed row goes,
    /// and the turn is run from the thread as it stands, which is where `send_opengrok_turn`
    /// reads its history from anyway.
    pub fn retry_turn(&mut self, cx: &mut Context<Self>) {
        let Some(message_id) = self.retryable_turn() else {
            return;
        };
        let Some(conversation_id) = self.active_conversation_id.clone() else {
            return;
        };
        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        {
            conversation.messages.retain(|m| m.id != message_id);
        }
        self.send_opengrok_turn(conversation_id, String::new(), cx);
    }

    /// Stop the turn the open thread has in flight.
    ///
    /// The server is told, and that is the whole of it: a run drives a box — it opens pages and
    /// types into them — so the app closing its own stream would stop nothing but the watching.
    /// The route is idempotent, so a turn that ended between the person deciding and the press
    /// landing is a success and there is nothing to check about the run first.
    ///
    /// The ending is painted without waiting for the answer, because a button that takes a round
    /// trip to respond gets pressed again. The answer is still read: a stop that did not land
    /// says so in the feed rather than letting the app claim a quiet it has no evidence for. A
    /// `404` is not that case — the run is unknown or is not ours, and either way nothing of
    /// ours is running under it.
    ///
    /// Nothing happens when no turn is in flight. The button is not there to be pressed then,
    /// but a keystroke or a driver can still ask, and "there is nothing to stop" is an answer
    /// rather than a fault.
    pub fn stop_turn(&mut self, cx: &mut Context<Self>) {
        let Some(conversation_id) = self.active_conversation_id.clone() else {
            return;
        };
        self.stop_live_turn(cx);
        // Stop with messages held is "send them now": the thread is idle the moment the stop
        // is painted, and the first held message goes.
        self.drain_queued_send(&conversation_id, cx);
    }

    /// The stop itself, without the queue: a forced send (⌘⇧↩) stops this way and then posts
    /// its own message ahead of anything held.
    fn stop_live_turn(&mut self, cx: &mut Context<Self>) {
        let Some((conversation_id, turn)) = self.turn_to_stop() else {
            return;
        };
        if let Some(client) = self.opengrok.clone() {
            let run_id = turn.run_id.clone();
            let thread = conversation_id.clone();
            cx.spawn(async move |this, cx| {
                if let Err(error) = client.stop_run(&run_id).await
                    && error.status != Some(404)
                {
                    eprintln!("NativeChat: the stop did not reach the server: {error}");
                    let _ = this.update(cx, |state, cx| {
                        state.say_status_line(&thread, STOP_UNSENT_NOTE);
                        cx.notify();
                    });
                }
            })
            .detach();
        }
        self.paint_stopped_turn(&conversation_id, &turn, cx);
    }

    /// A forced send's half of the stop: end the turn here and hand back the run id, which
    /// the send then stops on the wire *before* posting — so the server never has the two
    /// turns at once. The button's stop (`stop_live_turn`) has no turn to order it against.
    fn take_running_turn_for_steer(&mut self, cx: &mut Context<Self>) -> Option<String> {
        let (conversation_id, turn) = self.turn_to_stop()?;
        self.paint_stopped_turn(&conversation_id, &turn, cx);
        Some(turn.run_id)
    }

    fn paint_stopped_turn(
        &mut self,
        conversation_id: &str,
        turn: &LiveTurn,
        cx: &mut Context<Self>,
    ) {
        if let Some((content, parts)) = self.end_stopped_turn(conversation_id, turn) {
            self.persist_assistant_reply(
                conversation_id,
                &turn.message_id,
                content,
                &parts,
                Some(&turn.run_id),
                cx,
            );
        }
        cx.notify();
    }

    /// The turn a stop would be about, if there is one.
    ///
    /// A thread with nothing in flight has nothing to stop, and that is an answer rather than a
    /// fault: the button is not a stop button then, but a keystroke or a driver can still ask.
    /// Neither is a turn whose outcome is already decided and on its way to disk — it stays
    /// registered until the write lands, but nothing is running under it.
    ///
    /// A finished run that left a user-form card is not in flight on the
    /// server. Do not keep Stop over **Waiting for you**.
    fn turn_to_stop(&self) -> Option<(String, LiveTurn)> {
        let conversation_id = self.active_conversation_id.clone()?;
        let turn = self.live_turns.get(&conversation_id)?;
        if turn.persisting {
            return None;
        }
        if self.thread_status(&conversation_id) == Some(WAITING_FOR_YOU_STATUS) {
            return None;
        }
        Some((conversation_id, turn.clone()))
    }

    /// The turn is over here, whatever the server makes of the stop. Answers with the reply to
    /// write down, when the run left one worth keeping.
    ///
    /// What arrived before the stop stays. The words are the coworker's and the pictures are of
    /// things that really happened to the box, so a turn cut short is still a turn that
    /// happened, and it goes to disk through `persist_assistant_reply` like any other ending —
    /// which is also what lets the thread go. A row with nothing in it is not a turn that
    /// happened, and an empty bubble above the note would read as an answer still on its way.
    fn end_stopped_turn(
        &mut self,
        conversation_id: &str,
        turn: &LiveTurn,
    ) -> Option<(String, Vec<ChatPart>)> {
        self.finish_responding(Some(conversation_id), false);
        let kept = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
            .and_then(|conversation| {
                stopped_transcript(&mut conversation.messages, &turn.message_id)
            });
        match kept {
            // The outcome is decided the moment the stop lands, even though the thread stays
            // registered until the reply reaches the database. Saying so here is what puts the
            // send arrow back at once rather than after a disk write, and what tells the stream
            // — should it come back with an ending of its own — that it is too late.
            Some(_) => {
                if let Some(live) = self
                    .live_turns
                    .get_mut(conversation_id)
                    .filter(|live| live.run_id == turn.run_id)
                {
                    live.persisting = true;
                }
            }
            // Nothing is going to the database, so nothing will let the thread go later.
            None => self.release_live_turn(conversation_id, &turn.run_id),
        }
        kept
    }

    /// Put a line of the app's own into a thread.
    fn say_status_line(&mut self, conversation_id: &str, line: &str) {
        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        {
            conversation.messages.push(status_row(line));
        }
    }

    pub fn answer_approval(
        &mut self,
        spec: ApprovalSpec,
        resolution: LocalExecResolution,
        cx: &mut Context<Self>,
    ) {
        if self.approval_answered(&spec.call_id) {
            return;
        }
        self.approval_decisions
            .insert(spec.call_id.clone(), ApprovalDecision::Sending);
        self.drop_other_pending_approvals(&spec.call_id);
        cx.notify();
        let Some(client) = self.opengrok.clone() else {
            self.approval_decisions.insert(
                spec.call_id.clone(),
                ApprovalDecision::Failed("OpenGrok is not configured".into()),
            );
            return;
        };
        let machine_id = self
            .local_exec_machine_id
            .clone()
            .or_else(|| {
                self.computers
                    .iter()
                    .find(|computer| computer.this_machine)
                    .map(|computer| computer.machine_id.clone())
            })
            .or_else(|| {
                self.config
                    .as_ref()
                    .and_then(|config| stored_machine_id(&config.data_dir))
            })
            .or_else(|| {
                self.computers
                    .first()
                    .map(|computer| computer.machine_id.clone())
            });
        // The thread the run belongs to, not the thread that happens to be open: a card can be
        // answered from the notification while the person is reading somewhere else, and the
        // resumed run must go on filling in its own bubble. Failing that, the thread the card
        // was filed under — which for the MCP door is not a conversation at all but the coworker
        // whose calls it audits, and that is the thread the card was grafted onto. The open
        // thread is the last resort, for a card that came with no thread on it.
        let conversation_id = self
            .live_turns
            .iter()
            .find(|(_, turn)| turn.run_id == spec.run_id)
            .map(|(id, _)| id.clone())
            .or_else(|| spec.conversation_id().map(str::to_string))
            .or_else(|| self.active_conversation_id.clone());
        let (approved, decision, mode) = match resolution {
            LocalExecResolution::Always => (true, ApprovalDecision::Always, Some("bypass")),
            LocalExecResolution::AllowOnce => (true, ApprovalDecision::AllowOnce, None),
            LocalExecResolution::Never => (false, ApprovalDecision::Never, Some("never")),
            LocalExecResolution::DenyOnce => (false, ApprovalDecision::Denied, None),
        };
        // Only the local-shell tool can move this Mac's policy.
        let mode = mode.filter(|_| spec.runs_on_this_mac());
        // Always and Never on the tunnel's card are the computer's standing choice, kept on
        // the server for the card's own bot — which is not always the one being looked at.
        if spec.is_egress_tunnel() {
            let standing = match resolution {
                LocalExecResolution::Always => Some(LocalExecMode::Always),
                LocalExecResolution::Never => Some(LocalExecMode::Never),
                _ => None,
            };
            if let (Some(standing), Some(coworker_id)) = (standing, conversation_id.clone()) {
                self.set_egress_policy_for(coworker_id, standing, Some(spec.call_id.clone()), cx);
            }
        }
        // What the thread says while the answered run finishes. "Running commands" is the
        // local shell being let loose; an MCP card is one call being let through, so it says
        // which. A no reads the same either way — it is the same no.
        let working = match (approved, spec.is_mcp()) {
            (false, _) => "Telling them no".to_string(),
            (true, true) => format!("Answering {}", spec.tool),
            (true, false) => "Running commands".to_string(),
        };
        if let (Some(machine_id), Some(stored)) = (machine_id.as_ref(), mode) {
            if let Some(computer) = self
                .computers
                .iter_mut()
                .find(|computer| &computer.machine_id == machine_id)
            {
                computer.mode = LocalExecMode::from_stored(stored);
            }
            cx.notify();
        }
        let run_id_empty = spec.run_id.trim().is_empty();
        cx.spawn(async move |this, cx| {
            if let (Some(machine_id), Some(mode)) = (machine_id.as_deref(), mode) {
                let _ = client.set_local_exec_mode(machine_id, mode).await;
                let _ = this.update(cx, |state, cx| {
                    state.refresh_computers(cx);
                });
            }
            if run_id_empty {
                let _ = this.update(cx, |state, cx| {
                    state.drop_dead_approval(&spec.call_id);
                    let current = state.approval_decisions.get(&spec.call_id).cloned();
                    if let Some(settled) = settled_decision(current.as_ref(), decision) {
                        state
                            .approval_decisions
                            .insert(spec.call_id.clone(), settled);
                    }
                    cx.notify();
                });
                return;
            }
            let mut run_id = spec.run_id.clone();
            if let Ok(queue) = client.list_approvals().await
                && let Some(item) = queue.iter().find(|item| item.call_id == spec.call_id)
            {
                run_id = item.run_id.clone();
            }
            let result = client.answer_run(&run_id, &spec.call_id, approved).await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(_) => {
                        // A standing choice the policy write already failed to keep must not be
                        // written back over the once-only line that failure left; see
                        // `settled_decision`.
                        let current = state.approval_decisions.get(&spec.call_id).cloned();
                        if let Some(settled) = settled_decision(current.as_ref(), decision) {
                            state
                                .approval_decisions
                                .insert(spec.call_id.clone(), settled);
                        }
                        // The thread the resumed run belongs to, which is not necessarily the
                        // one being read: a card answered from the notification leaves the
                        // person somewhere else entirely, and the working line belongs where the
                        // work is.
                        // A NO IS FOLLOWED TOO, and for the same reason a yes is: the run
                        // carries on either way. Answering puts it back to running whichever way
                        // the person answered, because the refusal still has to reach the
                        // coworker — it is told what it may not use, and it answers that rather
                        // than being cut off mid-tool with a call nobody ever resolved.
                        //
                        // This branch used to end the turn here instead, and the composer paid
                        // for it: the working line went, so nothing looked busy, while the turn
                        // stayed registered and the send button stayed a stop button over a turn
                        // the app was no longer watching. Following it is what lets the ordinary
                        // ending arrive and settle the thread.
                        state.begin_responding(conversation_id.as_deref(), &working);
                        if spec.is_mcp() {
                            state.follow_mcp_answer(run_id.clone(), conversation_id, cx);
                        } else {
                            state.follow_run(run_id.clone(), conversation_id, cx);
                        }
                    }
                    Err(error) => {
                        if error.message.contains("no such run") {
                            state.drop_dead_approval(&spec.call_id);
                        } else {
                            state.approval_decisions.insert(
                                spec.call_id.clone(),
                                ApprovalDecision::Failed(error.message),
                            );
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Watch an answered MCP run to its end, for the working line and nothing else.
    ///
    /// The door answers such a run by finishing it: there is no remainder to stream and no
    /// reply text ever comes. What its journal does hold is the audit of a call the coworker
    /// made somewhere else, and that is not a turn of this conversation — [`Self::follow_run`]
    /// would paint it over the bubble this thread is holding, which is whatever the coworker
    /// last said. So the run is followed only far enough to know when to put the line down.
    fn follow_mcp_answer(
        &mut self,
        run_id: String,
        conversation_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            self.finish_responding(conversation_id.as_deref(), false);
            return;
        };
        cx.spawn(async move |this, cx| {
            for _ in 0..40 {
                match client.replay_run(&run_id).await {
                    Ok(replay) if replay.status == "running" => {}
                    _ => break,
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            let _ = this.update(cx, |state, cx| {
                state.finish_responding(conversation_id.as_deref(), false);
                cx.notify();
            });
        })
        .detach();
    }

    /// Watch a run to its end through the replay route, painting the thread as it goes.
    ///
    /// Two runs need this and they need the same thing: one the person has just allowed to
    /// continue, whose remainder never comes down the original stream, and one that outlived the
    /// app that started it and has no stream to come down at all. Both are runs the app is not
    /// listening to and has to ask about instead.
    fn follow_run(
        &mut self,
        run_id: String,
        conversation_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        // Whether the thread is holding a row for this run. When it is, the run being let go
        // means this poll has nothing of its own left to paint into and must stop — the row it
        // would fall back to, "the last thing the coworker said", is by then somebody else's.
        // When it never was, there is no such row to lose: that is a run picked up off the
        // approvals queue after a restart, which is followed on behalf of a thread that was
        // never watching it.
        let registered = conversation_id
            .as_deref()
            .is_some_and(|id| self.turn_is_unsettled(id, &run_id));
        cx.spawn(async move |this, cx| {
            let mut last_len = 0usize;
            let mut last_status = String::new();
            for _ in 0..400 {
                if registered {
                    let settled = this
                        .update(cx, |state, _| {
                            conversation_id
                                .as_deref()
                                .is_some_and(|id| !state.turn_is_unsettled(id, &run_id))
                        })
                        .unwrap_or(true);
                    // Whoever settled the turn has already ended the responding state and said
                    // what the turn came to, so there is nothing to do on the way out either.
                    if settled {
                        return;
                    }
                }
                match client.replay_run(&run_id).await {
                    Ok(replay) => {
                        // The status counts as news of its own: the frame that ends a run is
                        // often one the last poll already saw, and a run that stopped holding
                        // its text back has words to show for it even when nothing new arrived.
                        if replay.events.len() != last_len || replay.status != last_status {
                            last_len = replay.events.len();
                            last_status = replay.status.clone();
                            let (plain, parts) = reply_from_replay(&replay.events, &replay.status);
                            let status = replay.status.clone();
                            // A resumed turn that only ran tools says what it did, so the
                            // thread keeps a memory of the run the person allowed.
                            let plain = replayed_ending(
                                &replay.events,
                                &status,
                                replay.failure.as_deref(),
                                &plain,
                            )
                            .unwrap_or(plain);
                            // The journal says what the run is doing now; "Working" only
                            // when no frame has said.
                            let activity =
                                activity_from_replay(&replay.events).unwrap_or(BotActivity {
                                    label: "Working".into(),
                                });
                            // When the run began, for a bubble that has to be made for it.
                            let run_started_at = (replay.started_at_ms > 0).then(|| {
                                SystemTime::UNIX_EPOCH
                                    + Duration::from_millis(replay.started_at_ms as u64)
                            });
                            let _ = this.update(cx, |state, cx| {
                                let parts = state.graft_user_forms(parts.clone());
                                // The bubble this run has been filling in all along, by the name
                                // it was given when the turn started — the resumed half of a turn
                                // belongs to the same row as the half before the card.
                                let target = conversation_id.as_ref().and_then(|id| {
                                    state.live_turns.get(id).map(|turn| turn.message_id.clone())
                                });
                                // The bubble this actually painted into, so the row it is
                                // written down as is that bubble and not a new one.
                                let mut painted: Option<String> = None;
                                if let Some(conversation_id) = conversation_id.as_ref()
                                    && let Some(conversation) = state
                                        .conversations
                                        .iter_mut()
                                        .find(|c| &c.id == conversation_id)
                                {
                                    let at = bubble_for_run(
                                        &mut conversation.messages,
                                        target.as_deref(),
                                        &run_id,
                                        run_started_at,
                                    );
                                    let last = &mut conversation.messages[at];
                                    painted = Some(last.id.clone());
                                    last.content = plain.clone();
                                    last.parts = parts.clone();
                                    if let Some(shot) =
                                        parts.iter().rev().find_map(|part| match part {
                                            ChatPart::Screenshot(spec) => Some(spec.clone()),
                                            _ => None,
                                        })
                                    {
                                        state.last_box_shot = Some(shot);
                                    }
                                    for part in &parts {
                                        if let ChatPart::Approval(spec) = part
                                            && spec.output.is_some()
                                        {
                                            state.approval_decisions.insert(
                                                spec.call_id.clone(),
                                                ApprovalDecision::AllowOnce,
                                            );
                                        }
                                    }
                                }
                                match status.as_str() {
                                    "awaiting-approval" => {
                                        state.finish_responding(conversation_id.as_deref(), true);
                                    }
                                    "running" => {
                                        state.apply_turn_status(
                                            conversation_id.as_deref(),
                                            ActivityTick::Set(activity.clone()),
                                        );
                                    }
                                    "finished" => {
                                        if let Some(id) = conversation_id.as_deref() {
                                            if state.has_open_user_form(id) {
                                                state.park_waiting_for_you(id, &run_id);
                                            } else {
                                                state.finish_responding(
                                                    conversation_id.as_deref(),
                                                    false,
                                                );
                                                // Only when there is a bubble to write down.
                                                // Without one there is nothing on screen this
                                                // row would belong to, and minting an id here
                                                // would overwrite some other run's row.
                                                if !state.has_open_approval(id)
                                                    && let Some(painted) = painted.as_deref()
                                                {
                                                    state.persist_assistant_reply(
                                                        id,
                                                        painted,
                                                        plain.clone(),
                                                        &parts,
                                                        Some(&run_id),
                                                        cx,
                                                    );
                                                }
                                            }
                                        }
                                    }
                                    "failed" => {
                                        state.finish_responding(conversation_id.as_deref(), false);
                                        if let Some(id) = conversation_id.as_ref() {
                                            state.release_live_turn(id, &run_id);
                                        }
                                    }
                                    _ => {}
                                }
                                if let Some(id) = conversation_id.as_deref() {
                                    state.collect_handoff_ids_and_flush(id, cx);
                                }
                                cx.notify();
                            });
                        }
                        match replay.status.as_str() {
                            "finished" | "failed" => break,
                            "awaiting-approval" => {
                                let _ = this.update(cx, |state, cx| {
                                    state.finish_responding(conversation_id.as_deref(), true);
                                    cx.notify();
                                });
                                break;
                            }
                            _ => {}
                        }
                    }
                    Err(_) => break,
                }
                tokio::time::sleep(Duration::from_millis(300)).await;
            }
            let _ = this.update(cx, |state, cx| {
                // This thread's own line, not the app's: a poll that has run out of patience
                // must not decide that some other bot's turn is over, and must not end this one
                // while it is stopped at a card waiting for a person.
                let waiting = is_waiting_on_person(
                    conversation_id
                        .as_deref()
                        .and_then(|id| state.thread_status(id)),
                );
                if !waiting {
                    state.finish_responding(conversation_id.as_deref(), false);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn fill_approval_command(&mut self, call_id: &str, command: String) {
        if command.trim().is_empty() {
            return;
        }
        for conversation in &mut self.conversations {
            for message in &mut conversation.messages {
                for part in &mut message.parts {
                    if let ChatPart::Approval(spec) = part
                        && spec.call_id == call_id
                        && spec.command.is_empty()
                    {
                        spec.command = command.clone();
                    }
                }
            }
        }
    }

    fn fill_open_approval_commands(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let mut jobs = Vec::new();
        for conversation in &self.conversations {
            for message in &conversation.messages {
                for part in &message.parts {
                    if let ChatPart::Approval(spec) = part
                        && spec.command.is_empty()
                        && !spec.run_id.is_empty()
                    {
                        jobs.push((spec.run_id.clone(), spec.call_id.clone()));
                    }
                }
            }
        }
        if jobs.is_empty() {
            return;
        }
        cx.spawn(async move |this, cx| {
            for (run_id, call_id) in jobs {
                let mut command = String::new();
                if let Ok(replay) = client.replay_run(&run_id).await {
                    command = command_from_replay_events(&replay.events, &call_id);
                    if command.is_empty()
                        && let Some(pending) = replay.pending
                    {
                        let pending_id = pending
                            .get("call_id")
                            .or_else(|| pending.get("callId"))
                            .and_then(serde_json::Value::as_str);
                        if pending_id.is_none_or(|id| id == call_id) {
                            command = command_from_args(
                                pending.get("arguments").unwrap_or(&serde_json::Value::Null),
                            );
                        }
                    }
                }
                if command.is_empty()
                    && let Ok(queue) = client.list_approvals().await
                    && let Some(item) = queue.iter().find(|item| item.call_id == call_id)
                {
                    command = command_from_args(&item.arguments);
                }
                if command.is_empty() {
                    continue;
                }
                let _ = this.update(cx, |state, cx| {
                    state.fill_approval_command(&call_id, command);
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn sync_pending_approvals(&mut self, cx: &mut Context<Self>) {
        // A card is only ever grafted onto the open thread, so it is the open thread's own turn
        // that would paint one of its own — a bot working next door is no reason to leave this
        // thread without the card it is waiting on.
        if self.is_active_bot_responding() {
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let thread_id = self.active_conversation_id.clone();
        cx.spawn(async move |this, cx| {
            let Ok(queue) = client.list_approvals().await else {
                return;
            };
            let _ = this.update(cx, |state, cx| {
                // The policy answers what it covers; the rest wait for a card.
                let mut needs_card = Vec::new();
                for item in queue {
                    if state.approval_answered(&item.call_id) {
                        continue;
                    }
                    let spec = spec_from_queued(&item);
                    match state.auto_resolve_local_exec(&spec) {
                        Some(resolution) => state.answer_approval(spec, resolution, cx),
                        None => needs_card.push(item),
                    }
                }
                let Some(thread_id) = thread_id.as_deref() else {
                    return;
                };
                // A turn may have started in this thread while the queue was in the air, and
                // that turn will paint its own card.
                if state.is_thread_responding(thread_id) {
                    return;
                }
                // By conversation: the queue files an MCP door's card under `mcp-{coworker}`,
                // and this thread is where that coworker is read.
                let Some(item) = QueuedApproval::latest_for_conversation(&needs_card, thread_id)
                else {
                    return;
                };
                if item.run_id.trim().is_empty() {
                    return;
                }
                state.attach_queued_approval(item.clone());
                state.finish_responding(Some(thread_id), true);
                cx.notify();
            });
        })
        .detach();
    }

    fn drop_dead_approval(&mut self, call_id: &str) {
        self.approval_decisions.remove(call_id);
        for conversation in &mut self.conversations {
            for message in &mut conversation.messages {
                message.parts.retain(
                    |part| !matches!(part, ChatPart::Approval(spec) if spec.call_id == call_id),
                );
            }
        }
    }

    fn drop_other_pending_approvals(&mut self, keep_call_id: &str) {
        let decisions = &self.approval_decisions;
        for conversation in &mut self.conversations {
            for message in &mut conversation.messages {
                message.parts.retain(|part| match part {
                    ChatPart::Approval(spec) => {
                        spec.call_id == keep_call_id
                            || decisions
                                .get(&spec.call_id)
                                .is_some_and(ApprovalDecision::is_answered)
                    }
                    _ => true,
                });
            }
        }
    }

    fn attach_queued_approval(&mut self, item: QueuedApproval) {
        let spec = spec_from_queued(&item);
        self.approval_decisions
            .entry(spec.call_id.clone())
            .or_insert(ApprovalDecision::Pending);
        let conversation_id = conversation_for_thread(&item.thread_id);
        let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        else {
            return;
        };
        if let Some(last) = conversation.messages.iter_mut().rev().find(|m| !m.is_me) {
            if let Some(ChatPart::Approval(existing)) = last.parts.iter_mut().find(|part| {
                matches!(part, ChatPart::Approval(existing) if existing.call_id == spec.call_id
                    || existing.run_id == spec.run_id)
            }) {
                if existing.command.is_empty() && !spec.command.is_empty() {
                    existing.command = spec.command;
                }
                if existing.run_id.is_empty() {
                    existing.run_id = spec.run_id;
                }
                return;
            }
            if last
                .parts
                .iter()
                .any(|part| matches!(part, ChatPart::Approval(_)))
            {
                return;
            }
            last.parts.push(ChatPart::Approval(spec));
            return;
        }
        conversation.messages.push(Message {
            id: uuid::Uuid::now_v7().to_string(),
            sender: "AI".to_string(),
            content: String::new(),
            sent_at: SystemTime::now(),
            is_me: false,
            reply_preview: None,
            reply_to_id: None,
            reply_is_me: false,
            run_id: Some(spec.run_id.clone()),
            parts: vec![ChatPart::Approval(spec)],
            hidden: false,
        });
    }

    fn start_local_exec(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let Some(config) = self.config.clone() else {
            return;
        };
        self.stop_local_exec();
        let cancel = Arc::new(AtomicBool::new(false));
        self.local_exec_cancel = Some(cancel.clone());
        cx.spawn(async move |this, cx| {
            match enrol_this_machine(&client, &config.data_dir).await {
                Ok(machine_id) => {
                    let _ = this.update(cx, |state, cx| {
                        state.local_exec_machine_id = Some(machine_id);
                        state.refresh_computers(cx);
                        cx.notify();
                    });
                }
                Err(error) => {
                    eprintln!("NativeChat local-exec: {error}");
                }
            }
            serve_local_exec(client, config.data_dir, cancel).await;
        })
        .detach();
    }

    fn stop_local_exec(&mut self) {
        if let Some(cancel) = &self.local_exec_cancel {
            cancel.store(true, Ordering::Relaxed);
        }
        self.local_exec_cancel = None;
        self.local_exec_machine_id = None;
    }

    pub fn toggle_shell_output(&mut self, call_id: String, cx: &mut Context<Self>) {
        if !self.expanded_shell_output.remove(&call_id) {
            self.expanded_shell_output.insert(call_id);
        }
        cx.notify();
    }

    pub fn pick_form_option(
        &mut self,
        message_id: String,
        field_id: String,
        value: String,
        cx: &mut Context<Self>,
    ) {
        self.form_picks
            .entry(message_id)
            .or_default()
            .insert(field_id, value);
        cx.notify();
    }

    /// Non-secret user-form control (checkbox / select). Do not call this with a
    /// password or otp; those never belong on `AppState`. `card_key` is
    /// [`crate::opengrok::UserFormSpec::card_key`], not `callId` as a fill id.
    pub fn pick_user_form_option(
        &mut self,
        card_key: String,
        field_id: String,
        value: String,
        cx: &mut Context<Self>,
    ) {
        let masked = self.conversations.iter().any(|conversation| {
            conversation.messages.iter().any(|message| {
                message.parts.iter().any(|part| match part {
                    ChatPart::UserForm(spec) if spec.card_key() == card_key => spec
                        .fields
                        .iter()
                        .any(|field| field.id == field_id && field.masked()),
                    _ => false,
                })
            })
        });
        if masked {
            return;
        }
        self.user_form_picks
            .entry(card_key)
            .or_default()
            .insert(field_id, value);
        cx.notify();
    }

    fn graft_user_forms(&self, mut parts: Vec<ChatPart>) -> Vec<ChatPart> {
        for part in &mut parts {
            if let ChatPart::UserForm(spec) = part {
                let key = spec.card_key().to_string();
                let local_res = self
                    .user_form_resolutions
                    .get(&spec.entry_id)
                    .or_else(|| self.user_form_resolutions.get(&key))
                    .copied()
                    .or_else(|| {
                        (!spec.call_id.is_empty())
                            .then(|| self.user_form_resolutions.get(&spec.call_id).copied())
                            .flatten()
                    });
                if spec.effective_resolution() != Some(FormResolution::Sending)
                    && let Some(res) = local_res
                {
                    spec.resolution = Some(res);
                }
                let local_handoff = self
                    .user_form_computer_handoffs
                    .get(&spec.entry_id)
                    .or_else(|| self.user_form_computer_handoffs.get(&key))
                    .copied();
                spec.computer_handoff =
                    ComputerHandoffStatus::fold(spec.computer_handoff, local_handoff);
                if spec.handoff_entry_id.is_none() {
                    if let Some(id) = self
                        .user_form_handoffs
                        .get(&spec.entry_id)
                        .or_else(|| self.user_form_handoffs.get(&key))
                    {
                        spec.handoff_entry_id = Some(id.clone());
                    }
                }
            }
        }
        parts.retain(|part| match part {
            ChatPart::SaveLogin(spec) => keep_local_save_offer(
                self.pending_save.contains_key(&spec.form_entry_id),
                self.already_saved_login(&spec.origin, &spec.username),
            ),
            _ => true,
        });
        self.inject_local_save_logins(&mut parts);
        place_hitl_cards_in_document_order(&mut parts);
        parts
    }

    /// After Continue, `follow_run` / SSE overwrite `message.parts` from the
    /// assembler. Re-attach the save prompt from local form values so we do
    /// not wait for `credential.offer_save` (and never for a password).
    fn inject_local_save_logins(&self, parts: &mut Vec<ChatPart>) {
        for pending in self.pending_save.values() {
            let already_has_card = parts.iter().any(|part| {
                matches!(
                    part,
                    ChatPart::SaveLogin(spec) if spec.form_entry_id == pending.form_entry_id
                )
            });
            let form_submitted = parts.iter().any(|part| match part {
                ChatPart::UserForm(spec) => {
                    let same = spec.entry_id == pending.form_entry_id
                        || spec.card_key() == pending.form_entry_id;
                    same && spec.effective_resolution() == Some(FormResolution::Submitted)
                }
                _ => false,
            });
            if let Some(spec) = save_login_from_local(
                &pending.form_entry_id,
                &pending.origin,
                &pending.username,
                self.already_saved_login(&pending.origin, &pending.username),
                form_submitted,
                already_has_card,
            ) {
                parts.push(ChatPart::SaveLogin(spec));
            }
        }
    }

    fn user_form_mut(&mut self, card_key: &str) -> Option<&mut crate::opengrok::UserFormSpec> {
        for conversation in &mut self.conversations {
            for message in &mut conversation.messages {
                for part in &mut message.parts {
                    if let ChatPart::UserForm(spec) = part
                        && spec.card_key() == card_key
                    {
                        return Some(spec);
                    }
                }
            }
        }
        None
    }

    fn user_form_context(&self, card_key: &str) -> Option<(String, String, String, String)> {
        for conversation in &self.conversations {
            for message in &conversation.messages {
                for part in &message.parts {
                    if let ChatPart::UserForm(spec) = part
                        && spec.card_key() == card_key
                    {
                        let run_id = if spec.run_id.is_empty() {
                            self.live_turns
                                .get(&conversation.id)
                                .map(|turn| turn.run_id.clone())
                                .unwrap_or_default()
                        } else {
                            spec.run_id.clone()
                        };
                        return Some((
                            spec.entry_id.clone(),
                            run_id,
                            conversation.id.clone(),
                            conversation.id.clone(),
                        ));
                    }
                }
            }
        }
        None
    }

    pub fn user_form_handoff_id(&self, card_key: &str) -> Option<String> {
        self.user_form_handoffs.get(card_key).cloned()
    }

    /// POST id for I'm done / Skip. Sibling `handoffEntryId` only — never the
    /// form gateway `entryId` (that hits `is_live_handoff`).
    fn box_handoff_post_id(&self, card_key: &str, form_entry_id: &str) -> Option<String> {
        let stored = self
            .user_form_handoffs
            .get(card_key)
            .or_else(|| {
                (!form_entry_id.is_empty())
                    .then(|| self.user_form_handoffs.get(form_entry_id))
                    .flatten()
            })
            .map(String::as_str);
        let spec_id = self.conversations.iter().find_map(|conversation| {
            conversation.messages.iter().find_map(|message| {
                message.parts.iter().find_map(|part| match part {
                    ChatPart::UserForm(spec)
                        if spec.card_key() == card_key || spec.entry_id == form_entry_id =>
                    {
                        spec.handoff_entry_id.as_deref()
                    }
                    _ => None,
                })
            })
        });
        box_handoff_resolve_entry_id(stored.or(spec_id), form_entry_id)
    }

    fn queue_pending_box_handoff(
        &mut self,
        card_key: &str,
        form_entry_id: &str,
        pending: PendingBoxHandoff,
    ) {
        if !form_entry_id.is_empty() && form_entry_id != card_key {
            self.user_form_pending_resolves
                .insert(form_entry_id.to_string(), pending.clone());
        }
        self.user_form_pending_resolves
            .insert(card_key.to_string(), pending);
    }

    fn take_pending_box_handoff(
        &mut self,
        card_key: &str,
        form_entry_id: &str,
    ) -> Option<PendingBoxHandoff> {
        self.user_form_pending_resolves
            .remove(card_key)
            .or_else(|| {
                (!form_entry_id.is_empty())
                    .then(|| self.user_form_pending_resolves.remove(form_entry_id))
                    .flatten()
            })
    }

    fn flush_pending_box_handoff(
        &mut self,
        card_key: &str,
        form_entry_id: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(handoff_id) = self.box_handoff_post_id(card_key, form_entry_id) else {
            return;
        };
        let Some(pending) = self.take_pending_box_handoff(card_key, form_entry_id) else {
            return;
        };
        self.post_box_handoff_resolve(
            card_key.to_string(),
            handoff_id,
            pending.resolution,
            pending.run_id,
            pending.conversation_id,
            pending.agent_id,
            cx,
        );
    }

    fn collect_handoff_ids_and_flush(&mut self, conversation_id: &str, cx: &mut Context<Self>) {
        let pending_keys: Vec<String> = self.user_form_pending_resolves.keys().cloned().collect();
        let mut discovered: Vec<(String, String, String)> = Vec::new();
        let mut flush: Vec<(String, String)> = Vec::new();
        if let Some(conversation) = self
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        {
            for message in &conversation.messages {
                for part in &message.parts {
                    let ChatPart::UserForm(spec) = part else {
                        continue;
                    };
                    let key = spec.card_key().to_string();
                    if let Some(id) = spec
                        .handoff_entry_id
                        .as_deref()
                        .filter(|id| !id.is_empty() && *id != spec.entry_id)
                    {
                        discovered.push((key.clone(), spec.entry_id.clone(), id.to_string()));
                    }
                    if pending_keys.iter().any(|pending| {
                        pending == &key || (!spec.entry_id.is_empty() && pending == &spec.entry_id)
                    }) {
                        flush.push((key, spec.entry_id.clone()));
                    }
                }
            }
        }
        for (key, entry_id, id) in discovered {
            self.user_form_handoffs.insert(key, id.clone());
            if !entry_id.is_empty() {
                self.user_form_handoffs.insert(entry_id, id);
            }
        }
        for (card_key, entry_id) in flush {
            self.flush_pending_box_handoff(&card_key, &entry_id, cx);
        }
        self.settle_chrome_and_drain(conversation_id, cx);
    }

    fn post_box_handoff_resolve(
        &mut self,
        card_key: String,
        handoff_entry_id: String,
        resolution: BoxHandoffResolution,
        run_id: String,
        conversation_id: String,
        agent_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let result = client
                .resolve_box_handoff(&handoff_entry_id, &agent_id, resolution)
                .await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(
                        BoxHandoffReply::Settled
                        | BoxHandoffReply::AlreadyAnswered
                        | BoxHandoffReply::Empty,
                    ) => {
                        state.user_form_handoff_done.insert(card_key.clone());
                        #[cfg(target_os = "macos")]
                        state.push_computer_window_attention(cx);
                        if !run_id.is_empty() {
                            state.begin_responding(Some(&conversation_id), "Working");
                            state.follow_run(run_id, Some(conversation_id.clone()), cx);
                        }
                    }
                    Ok(BoxHandoffReply::MissingRoute) => {
                        // Wrong id or a resolve 404 is not "routes missing".
                        // Do not freeze every stacked open user-form.
                    }
                    Ok(BoxHandoffReply::MissingEntryId) => {
                        state.user_form_handoff_done.remove(&card_key);
                        if let Some(spec) = state.user_form_mut(&card_key) {
                            spec.resolution = None;
                        }
                        state.user_form_resolutions.remove(&card_key);
                        state.set_computer_handoff(&card_key, ComputerHandoffStatus::ActionNeeded);
                        #[cfg(target_os = "macos")]
                        state.push_computer_window_attention(cx);
                    }
                    Err(error) => {
                        if error.is_signed_out() {
                            state.note_signed_out(cx);
                        }
                    }
                }
                state.settle_chrome_and_drain(&conversation_id, cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn user_form_handoff_resolved(&self, card_key: &str) -> bool {
        self.user_form_handoff_done.contains(card_key)
    }

    fn remember_user_form_resolution(&mut self, spec: &crate::opengrok::UserFormSpec) {
        if let Some(resolution) = spec.effective_resolution() {
            if resolution == FormResolution::Sending {
                return;
            }
            if spec.has_gateway_entry_id() {
                self.user_form_resolutions
                    .insert(spec.entry_id.clone(), resolution);
            }
            self.user_form_resolutions
                .insert(spec.card_key().to_string(), resolution);
            if !spec.call_id.is_empty() {
                self.user_form_resolutions
                    .insert(spec.call_id.clone(), resolution);
            }
            self.user_form_restore.remove(spec.card_key());
        }
        if let Some(id) = spec.handoff_entry_id.as_deref().filter(|id| !id.is_empty()) {
            self.user_form_handoffs
                .insert(spec.card_key().to_string(), id.to_string());
            if spec.has_gateway_entry_id() {
                self.user_form_handoffs
                    .insert(spec.entry_id.clone(), id.to_string());
            }
        }
        if let Some(status) = spec.computer_handoff {
            self.remember_computer_handoff(spec.card_key(), spec, status);
        }
    }

    fn remember_computer_handoff(
        &mut self,
        card_key: &str,
        spec: &crate::opengrok::UserFormSpec,
        status: ComputerHandoffStatus,
    ) {
        let folded = ComputerHandoffStatus::fold(
            self.user_form_computer_handoffs.get(card_key).copied(),
            Some(status),
        )
        .unwrap_or(status);
        self.user_form_computer_handoffs
            .insert(card_key.to_string(), folded);
        if spec.has_gateway_entry_id() {
            self.user_form_computer_handoffs
                .insert(spec.entry_id.clone(), folded);
        }
    }

    fn set_computer_handoff(&mut self, card_key: &str, status: ComputerHandoffStatus) {
        let call_id = self
            .user_form_mut(card_key)
            .map(|spec| spec.call_id.clone())
            .unwrap_or_default();
        let painted = if let Some(spec) = self.user_form_mut(card_key) {
            spec.computer_handoff = Some(status);
            Some((
                spec.card_key().to_string(),
                spec.entry_id.clone(),
                spec.has_gateway_entry_id(),
            ))
        } else {
            None
        };
        if let Some((key, entry_id, has_entry)) = painted {
            self.user_form_computer_handoffs.insert(key, status);
            if has_entry {
                self.user_form_computer_handoffs.insert(entry_id, status);
            }
        } else {
            self.user_form_computer_handoffs
                .insert(card_key.to_string(), status);
        }
        if !call_id.is_empty() {
            for conversation in &mut self.conversations {
                for message in &mut conversation.messages {
                    for part in &mut message.parts {
                        if let ChatPart::UserForm(spec) = part
                            && spec.shares_call_id(&call_id)
                        {
                            spec.computer_handoff = Some(status);
                        }
                    }
                }
            }
        }
    }

    fn restore_computer_handoff(&mut self, card_key: &str) {
        let Some(prior) = self.user_form_handoff_restore.remove(card_key) else {
            return;
        };
        if let Some(spec) = self.user_form_mut(card_key) {
            spec.computer_handoff = prior;
        }
        match prior {
            Some(status) => {
                self.user_form_computer_handoffs
                    .insert(card_key.to_string(), status);
            }
            None => {
                self.user_form_computer_handoffs.remove(card_key);
            }
        }
    }

    fn paint_user_form_resolution(&mut self, card_key: &str, resolution: FormResolution) {
        let (prior, call_id) = match self.user_form_mut(card_key) {
            Some(spec) => {
                // Outer `None`: mid-Sending, leave the stored prior alone.
                // `Some(None)`: the card was idle, and that is stored too. The
                // old `.flatten()` folded both into `None`, so idle was never
                // recorded and could never be restored.
                let prior = (spec.resolution != Some(FormResolution::Sending))
                    .then(|| spec.effective_resolution());
                (prior, spec.call_id.clone())
            }
            None => (None, String::new()),
        };
        if let Some(prior) = prior {
            self.user_form_restore.insert(card_key.to_string(), prior);
        }
        if let Some(spec) = self.user_form_mut(card_key) {
            spec.resolution = Some(resolution);
        }
        self.paint_user_form_call_peers(&call_id, resolution);
        if resolution != FormResolution::Sending {
            self.user_form_resolutions
                .insert(card_key.to_string(), resolution);
            // A settled card holds nothing: a picked password does not outlive the card
            // it was picked for.
            if resolution != FormResolution::FillFailed {
                self.saved_login_use.remove(card_key);
            }
        }
    }

    /// Log in was pressed with a picked password on this card.
    fn saved_login_in_flight(&self, card_key: &str) -> bool {
        matches!(
            self.saved_login_use.get(card_key),
            Some(SavedLoginUse::Filling { .. })
        )
    }

    /// A fill that did not end in Submitted forgets the picked password: the card is back
    /// to idle (or Not filled), and a new pick asks Touch ID again.
    fn drop_saved_login_fill(&mut self, card_key: &str) {
        if self.saved_login_in_flight(card_key) {
            self.saved_login_use.remove(card_key);
        }
    }

    /// Collapse every `ChatPart::UserForm` that shares this AG-UI `callId`
    /// (`user-form-call-{callId}-*`). Clone `call_id` before this walk — do
    /// not hold `user_form_mut` across the iteration (Mac E0499).
    fn paint_user_form_call_peers(&mut self, call_id: &str, resolution: FormResolution) {
        if call_id.is_empty() {
            return;
        }
        for conversation in &mut self.conversations {
            for message in &mut conversation.messages {
                for part in &mut message.parts {
                    if let ChatPart::UserForm(spec) = part
                        && spec.shares_call_id(call_id)
                    {
                        spec.resolution = Some(resolution);
                    }
                }
            }
        }
        if resolution != FormResolution::Sending {
            self.user_form_resolutions
                .insert(call_id.to_string(), resolution);
        }
    }

    /// Open-the-screen milestone: pin the last box PNG into chat once, not every step.
    /// Treat it as `transcript` (explicit observe), even if the live frame was `agent`.
    fn pin_open_screen_shot(&mut self, conversation_id: &str) {
        let Some(mut shot) = self.last_box_shot.clone() else {
            return;
        };
        shot.visibility = Some(ImageVisibility::Transcript);
        let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return;
        };
        let Some(last) = conversation.messages.iter_mut().rev().find(|m| !m.is_me) else {
            return;
        };
        if last
            .parts
            .iter()
            .any(|part| matches!(part, ChatPart::Screenshot(spec) if spec.call_id == shot.call_id))
        {
            return;
        }
        last.parts.push(ChatPart::Screenshot(shot));
        place_hitl_cards_in_document_order(&mut last.parts);
    }

    fn restore_user_form(&mut self, card_key: &str) {
        let local_call_settle = self
            .user_form_mut(card_key)
            .is_some_and(|spec| spec.entry_id.is_empty() && !spec.call_id.is_empty());
        if local_call_settle {
            // call-* cards settle locally; HTTP never had a gateway target.
            self.user_form_restore.remove(card_key);
            return;
        }
        if let Some(prior) = self.user_form_restore.remove(card_key) {
            if let Some(spec) = self.user_form_mut(card_key) {
                spec.resolution = prior;
            }
            match prior {
                Some(prior) => {
                    self.user_form_resolutions
                        .insert(card_key.to_string(), prior);
                }
                None => {
                    self.user_form_resolutions.remove(card_key);
                }
            }
        }
        self.restore_computer_handoff(card_key);
    }

    /// Continue: POST `/ag-ui/user-form/submit`. Secrets stay in `values` for
    /// this request only — never `send_message`, AG-UI `content`, or sqlite.
    pub fn submit_user_form(
        &mut self,
        card_key: String,
        mut values: UserFormValues,
        cx: &mut Context<Self>,
    ) {
        // A card with a sheet up, a code waiting for its next step, or a fill already on
        // its way, does not take a second press — wherever the press came from.
        if self
            .saved_login_use
            .get(&card_key)
            .is_some_and(SavedLoginUse::is_busy)
        {
            return;
        }
        // A held code near the end of its step is minted on the next step instead. The
        // card is busy meanwhile, so Continue cannot be pressed twice, and the send happens
        // only if that same pick is still the one on that same open card.
        if let Some(wait) = self.held_code_wait(&card_key) {
            let held = self
                .saved_login_use
                .get(&card_key)
                .and_then(SavedLoginUse::ready)
                .map(|(username, secret)| (username.to_string(), secret.to_string()));
            let login_id = self
                .saved_login_use
                .get(&card_key)
                .and_then(SavedLoginUse::held_id)
                .unwrap_or_default()
                .to_string();
            let Some((username, password)) = held else {
                return;
            };
            self.saved_login_use.insert(
                card_key.clone(),
                SavedLoginUse::Waiting {
                    login_id: login_id.clone(),
                    username,
                    password,
                },
            );
            cx.notify();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(wait).await;
                let _ = this.update(cx, |state, cx| {
                    let still_ours = matches!(
                        state.saved_login_use.get(&card_key),
                        Some(SavedLoginUse::Waiting { login_id: held, .. }) if held == &login_id
                    ) && state
                        .user_form_mut(&card_key)
                        .is_some_and(|spec| spec.effective_resolution().is_none());
                    if !still_ours {
                        return;
                    }
                    if !state.fold_held_saved_login(&card_key, &mut values) {
                        cx.notify();
                        return;
                    }
                    state.dispatch_user_form(card_key, UserFormDispatch::Submit(values), cx);
                });
            })
            .detach();
            return;
        }
        if !self.fold_held_saved_login(&card_key, &mut values) {
            cx.notify();
            return;
        }
        self.dispatch_user_form(card_key, UserFormDispatch::Submit(values), cx);
    }

    /// Open the screen (`escalated`) or Dismiss (`dismissed`).
    pub fn dismiss_user_form(
        &mut self,
        card_key: String,
        mode: UserFormDismissMode,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_user_form(card_key, UserFormDispatch::Dismiss(mode), cx);
    }

    /// Hand back / decline. POSTs dismiss `handoffEntryId` / sibling Computer
    /// card id. Never the form gateway `entryId`. Queues until that id lands.
    pub fn resolve_user_form_handoff(
        &mut self,
        card_key: String,
        resolution: BoxHandoffResolution,
        cx: &mut Context<Self>,
    ) {
        self.dispatch_user_form(card_key, UserFormDispatch::ResolveHandoff(resolution), cx);
    }

    fn ensure_site_login_vault(&mut self, cx: &mut Context<Self>) {
        if self.site_login_vault.is_some() {
            return;
        }
        let Some(db) = self.database_service.as_ref() else {
            return;
        };
        let Some(config) = self.config.as_ref() else {
            return;
        };
        self.site_login_vault = Some(SiteLoginVault::open(db.pool(), &config.data_dir));
        self.reload_site_logins(cx);
    }

    fn reload_site_logins(&mut self, cx: &mut Context<Self>) {
        let Some(vault) = self.site_login_vault.clone() else {
            return;
        };
        let client = self.opengrok.clone();
        cx.spawn(async move |this, cx| {
            let list = match sync_site_logins(&vault, client.as_ref()).await {
                Ok(()) => vault.list().await,
                Err(sync_error) => {
                    // The server was not reachable, or has no vault: this Mac's rows still
                    // stand, and the next reload tries again.
                    let _ = this.update(cx, |state, _| {
                        state.site_login_notice = Some(format!(
                            "Saved logins are from this Mac only for now: {sync_error}"
                        ));
                    });
                    vault.list().await
                }
            };
            // Which rows have their password on this Mac is a keychain question per row;
            // it is answered off the UI thread.
            let list = match list {
                Ok(rows) => {
                    let vault = vault.clone();
                    Ok(cx
                        .background_executor()
                        .spawn(async move {
                            // Both of these ask the keychain whether an item is there,
                            // by name; neither opens one. Reading a secret is what makes
                            // the keychain ask the person to unlock it, and nothing on
                            // this path needs the secret itself.
                            let here: HashSet<String> = rows
                                .iter()
                                .filter(|row| vault.secret_present(&row.id))
                                .map(|row| row.id.clone())
                                .collect();
                            let with_code: HashSet<String> = rows
                                .iter()
                                .filter(|row| vault.code_present(&row.id))
                                .map(|row| row.id.clone())
                                .collect();
                            (rows, here, with_code)
                        })
                        .await)
                }
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |state, cx| {
                match list {
                    Ok((rows, here, with_code)) => {
                        state.site_logins_on_this_mac = here;
                        // The parsed seeds already in hand are kept; a row whose seed has
                        // not been read yet is read when the person opens it.
                        state
                            .site_login_codes
                            .retain(|id, _| with_code.contains(id));
                        state.site_logins_with_code = with_code;
                        state.site_logins = rows;
                        state.site_login_error = None;
                        // Only a successful read makes the vault readable. A vault
                        // that failed to open must not read as empty: left false, the
                        // card offers nothing and the person types the login instead.
                        state.site_logins_ready = true;
                        // A row that went away takes its detail pane with it.
                        if let Some(selected) = &state.site_login_selected
                            && !state.site_logins.iter().any(|row| &row.id == selected)
                        {
                            state.site_login_selected = None;
                        }
                        state.fetch_site_login_icons(cx);
                    }
                    Err(err) => state.site_login_error = Some(err.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// The rows a card can take: the vault has loaded, the card has a target (a login, a
    /// code, a passkey), and the row is of that kind for the card's site.
    pub fn saved_logins_for_form(
        &self,
        spec: &crate::opengrok::UserFormSpec,
    ) -> Vec<SiteLoginRecord> {
        if !self.site_logins_ready {
            return Vec::new();
        }
        let Some(target) = card_target(spec) else {
            return Vec::new();
        };
        let Some(origin) = login_origin(spec) else {
            return Vec::new();
        };
        self.site_logins
            .iter()
            .filter(|row| login_matches_request(&row.origin, &row.username, &origin, None))
            .filter(|row| match &target {
                CardTarget::Login(_) => row.kind == crate::site_login::KIND_PASSWORD,
                // A code can sit beside a password or on a row of its own; either way the
                // seed has to be on this Mac (or the row on the server) to be offered.
                // Only a row that really carries a code: its seed is on this Mac, or the
                // row is a code row. A password row whose seed is on the server and has
                // never been used here is not offered — nothing here knows it has one —
                // and one reveal puts it in `site_login_codes` for every card after that.
                CardTarget::Code { .. } => {
                    self.site_logins_with_code.contains(&row.id)
                        || self.site_login_codes.contains_key(&row.id)
                        || row.kind == crate::site_login::KIND_CODE
                }
                CardTarget::Passkey { register: false } => {
                    row.kind == crate::site_login::KIND_PASSKEY
                }
                CardTarget::Passkey { register: true } => false,
            })
            .cloned()
            .collect()
    }

    /// A pick from the card's list: Touch ID first, then what the row holds is fetched for
    /// the card — the password from this Mac's keychain (or once from the server), the code
    /// seed the same way, nothing at all for a passkey (its key stays on the server, in the
    /// box's browser) — and held for the press of the button. The name goes into its field;
    /// a secret is never painted and never put in an input, and the Bot never sees it.
    pub fn pick_saved_login(&mut self, card_key: String, login_id: String, cx: &mut Context<Self>) {
        let Some(spec) = self.user_form_mut(&card_key).map(|spec| spec.clone()) else {
            return;
        };
        if spec.effective_resolution().is_some() || !spec.has_gateway_entry_id() {
            return;
        }
        let (Some(target), Some(origin)) = (card_target(&spec), login_origin(&spec)) else {
            return;
        };
        let Some(row) = self
            .site_logins
            .iter()
            .find(|row| row.id == login_id)
            .cloned()
        else {
            return;
        };
        let Some(vault) = self.site_login_vault.clone() else {
            return;
        };
        if self
            .saved_login_use
            .get(&card_key)
            .is_some_and(SavedLoginUse::is_busy)
        {
            return;
        }
        let username = row.username.clone();
        self.saved_login_use.insert(
            card_key.clone(),
            SavedLoginUse::Confirming {
                username: username.clone(),
            },
        );
        if let CardTarget::Login(fields) = &target {
            // The name shows in its field at once (the driver reads it from here; the view
            // sets its input too).
            self.user_form_typed
                .entry(card_key.clone())
                .or_default()
                .insert(fields.username_id.clone(), username.clone());
        }
        cx.notify();
        let client = self.opengrok.clone();
        let wants = match &target {
            CardTarget::Login(_) => HeldSecret::Password,
            CardTarget::Code { .. } => HeldSecret::CodeSeed,
            CardTarget::Passkey { .. } => HeldSecret::None,
        };
        cx.spawn(async move |this, cx| {
            let unlocked = cx
                .background_executor()
                .spawn({
                    let origin = origin.clone();
                    let username = username.clone();
                    let row_id = row.id.clone();
                    let vault = vault.clone();
                    async move {
                        match crate::site_login::touch_id::confirm_use(&origin, &username) {
                            crate::site_login::touch_id::TouchIdOutcome::Verified => Ok(match wants {
                                HeldSecret::Password => vault.secret_for_fill(&row_id),
                                HeldSecret::CodeSeed => vault.code_for(&row_id),
                                HeldSecret::None => Ok(Some(String::new())),
                            }),
                            other => Err(other),
                        }
                    }
                })
                .await;
            // Touch ID passed but this Mac has no copy: the server's vault has it. Fetch it
            // once and keep it in the keychain for next time.
            let unlocked = match unlocked {
                Ok(Ok(None)) => match client.as_ref() {
                    Some(client) => match client.reveal_site_login_secrets(&row.id).await {
                        Ok(secrets) => {
                            if let Some(seed) = &secrets.otpauth {
                                if let Err(error) = vault.set_code(&row.id, seed) {
                                    log::warn!("could not cache a revealed code seed: {error}");
                                }
                                // The row can fill a code card from now on, without waiting
                                // for the next reload to notice.
                                if let Ok(totp) = crate::site_login::totp::parse(seed) {
                                    let row_id = row.id.clone();
                                    let _ = this.update(cx, |state, _| {
                                        state.site_logins_with_code.insert(row_id.clone());
                                        state.site_logins_with_code.insert(row_id.clone());
                                        state.site_login_codes.insert(row_id, totp);
                                    });
                                }
                            }
                            if let Some(password) = &secrets.password {
                                match vault.cache_secret(&row.id, password) {
                                    Ok(()) => {
                                        let _ = this.update(cx, |state, _| {
                                            state.site_logins_on_this_mac.insert(row.id.clone());
                                        });
                                    }
                                    Err(error) => {
                                        log::warn!("could not cache a revealed site login: {error}");
                                    }
                                }
                            }
                            Ok(Ok(match wants {
                                HeldSecret::Password => secrets.password,
                                HeldSecret::CodeSeed => secrets.otpauth,
                                HeldSecret::None => Some(String::new()),
                            }))
                        }
                        Err(error) => Ok(Err(crate::site_login::StoreError::Secret(format!(
                            "the server would not reveal it: {error}"
                        )))),
                    },
                    None => Ok(Ok(None)),
                },
                other => other,
            };
            let _ = this.update(cx, |state, cx| {
                use crate::site_login::touch_id::TouchIdOutcome;
                // The card may have settled or been dismissed while the sheet was up; a
                // secret for a card that is gone is not kept.
                let still_waiting = matches!(
                    state.saved_login_use.get(&card_key),
                    Some(SavedLoginUse::Confirming { .. })
                ) && state
                    .user_form_mut(&card_key)
                    .is_some_and(|spec| spec.effective_resolution().is_none());
                if !still_waiting {
                    state.saved_login_use.remove(&card_key);
                    cx.notify();
                    return;
                }
                let next = match unlocked {
                    Ok(Ok(Some(secret))) => {
                        let mut unreadable = false;
                        if wants == HeldSecret::CodeSeed {
                            match crate::site_login::totp::parse(&secret) {
                                Ok(totp) => {
                                    state.site_logins_with_code.insert(row.id.clone());
                                    state.site_logins_with_code.insert(row.id.clone());
                                    state.site_login_codes.insert(row.id.clone(), totp);
                                }
                                // Better to say so now than to send a card with an empty
                                // field and let the person wonder what the box typed.
                                Err(_) => unreadable = true,
                            }
                        }
                        if unreadable {
                            Some(SavedLoginUse::Unavailable {
                                message: format!(
                                    "The code seed for {username} is not readable. Import it \
                                     again, or type the code."
                                ),
                            })
                        } else {
                            Some(SavedLoginUse::Ready {
                                login_id: row.id.clone(),
                                username: username.clone(),
                                password: secret,
                            })
                        }
                    }
                    Ok(Ok(None)) => Some(SavedLoginUse::Unavailable {
                        message: match wants {
                            HeldSecret::CodeSeed => format!(
                                "The code seed for {username} is not in this Mac's keychain. Import it again."
                            ),
                            _ => format!(
                                "The password for {username} is not in this Mac's keychain. Type it once and save it again."
                            ),
                        },
                    }),
                    Ok(Err(error)) => Some(SavedLoginUse::Unavailable {
                        message: format!("The keychain would not give up the secret: {error}"),
                    }),
                    Err(TouchIdOutcome::Cancelled) => Some(SavedLoginUse::Cancelled {
                        username: username.clone(),
                    }),
                    Err(TouchIdOutcome::Unavailable(why)) | Err(TouchIdOutcome::Failed(why)) => {
                        Some(SavedLoginUse::Unavailable {
                            message: format!("Touch ID did not confirm it: {why}"),
                        })
                    }
                    Err(TouchIdOutcome::Verified) => unreachable!("verified is Ok"),
                };
                if let Some(next) = next {
                    state.saved_login_use.insert(card_key.clone(), next);
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A passkey card in register mode: the person confirms with Touch ID that this site may
    /// make a passkey for them; the site does the making, in the bot's browser.
    pub fn confirm_passkey_register(&mut self, card_key: String, cx: &mut Context<Self>) {
        let Some(spec) = self.user_form_mut(&card_key).map(|spec| spec.clone()) else {
            return;
        };
        if spec.effective_resolution().is_some() || !spec.has_gateway_entry_id() {
            return;
        }
        if card_target(&spec) != Some(CardTarget::Passkey { register: true }) {
            return;
        }
        let Some(origin) = login_origin(&spec) else {
            return;
        };
        if self
            .saved_login_use
            .get(&card_key)
            .is_some_and(SavedLoginUse::is_busy)
        {
            return;
        }
        // No name here: the site makes the passkey for whichever account is signed in
        // there, which is not this app's account and may not be a name at all.
        let username = origin.clone();
        self.saved_login_use.insert(
            card_key.clone(),
            SavedLoginUse::ConfirmingRegister {
                site: origin.clone(),
            },
        );
        cx.notify();
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn({
                    let origin = origin.clone();
                    async move { crate::site_login::touch_id::confirm_register(&origin) }
                })
                .await;
            let _ = this.update(cx, |state, cx| {
                use crate::site_login::touch_id::TouchIdOutcome;
                let still_waiting = matches!(
                    state.saved_login_use.get(&card_key),
                    Some(SavedLoginUse::ConfirmingRegister { .. })
                );
                if !still_waiting {
                    return;
                }
                let next = match outcome {
                    TouchIdOutcome::Verified => SavedLoginUse::Ready {
                        login_id: String::new(),
                        username: username.clone(),
                        password: String::new(),
                    },
                    TouchIdOutcome::Cancelled => SavedLoginUse::Cancelled {
                        username: username.clone(),
                    },
                    TouchIdOutcome::Unavailable(why) | TouchIdOutcome::Failed(why) => {
                        SavedLoginUse::Unavailable {
                            message: format!("Touch ID did not confirm it: {why}"),
                        }
                    }
                };
                state.saved_login_use.insert(card_key.clone(), next);
                cx.notify();
            });
        })
        .detach();
    }

    /// "Change" on the locked password row: the held password is dropped; the person types.
    pub fn clear_saved_login_pick(&mut self, card_key: String, cx: &mut Context<Self>) {
        self.saved_login_use.remove(&card_key);
        cx.notify();
    }

    /// A submit with a held secret: for a login the name and the password ride along; for a
    /// code the digits are minted now, from the held seed; for a passkey nothing is added,
    /// the row's id is enough. The submit is marked as a saved login for the server's
    /// own-computer rule.
    /// `false` when the card must not be sent: a held code seed this Mac cannot read is
    /// said so on the card instead of sending an empty field to the box.
    fn fold_held_saved_login(&mut self, card_key: &str, values: &mut UserFormValues) -> bool {
        let Some((login_id, username, secret)) =
            self.saved_login_use
                .get(card_key)
                .and_then(|use_| match use_ {
                    SavedLoginUse::Ready {
                        login_id,
                        username,
                        password,
                    }
                    | SavedLoginUse::Waiting {
                        login_id,
                        username,
                        password,
                    } => Some((login_id.clone(), username.clone(), password.clone())),
                    _ => None,
                })
        else {
            // Nothing is held: an ordinary typed submit.
            return true;
        };
        let Some(target) = self
            .user_form_mut(card_key)
            .and_then(|spec| card_target(spec))
        else {
            return true;
        };
        match target {
            CardTarget::Login(fields) => {
                // Both fields are locked on the card while a pick is held, so the picked name
                // is the name that goes with the picked password.
                values.by_id.insert(fields.username_id, username.clone());
                values.by_id.insert(fields.password_id, secret);
            }
            CardTarget::Code { code_id } => {
                let Ok(totp) = crate::site_login::totp::parse(&secret) else {
                    self.saved_login_use.insert(
                        card_key.to_string(),
                        SavedLoginUse::Unavailable {
                            message: format!(
                                "The code seed for {username} is not readable. Import it again, \
                                 or type the code."
                            ),
                        },
                    );
                    return false;
                };
                values
                    .by_id
                    .insert(code_id, totp.generate_current().to_string());
            }
            CardTarget::Passkey { .. } => {}
        }
        self.saved_login_use.insert(
            card_key.to_string(),
            SavedLoginUse::Filling { username, login_id },
        );
        true
    }

    /// How long a submit should wait before minting a held code, so the digits are still
    /// valid after the trip to the box: nothing, or the rest of a step that is nearly over.
    fn held_code_wait(&self, card_key: &str) -> Option<std::time::Duration> {
        let seed = self
            .saved_login_use
            .get(card_key)
            .and_then(SavedLoginUse::ready)
            .map(|(_, secret)| secret.to_string())?;
        let target = self.user_form_mut_ref(card_key).and_then(card_target)?;
        if !matches!(target, CardTarget::Code { .. }) {
            return None;
        }
        let totp = crate::site_login::totp::parse(&seed).ok()?;
        let wait = crate::site_login::totp::wait_before_minting(&totp);
        (wait > 0).then(|| std::time::Duration::from_secs(wait))
    }

    fn user_form_mut_ref(&self, card_key: &str) -> Option<&crate::opengrok::UserFormSpec> {
        self.conversations.iter().find_map(|conversation| {
            conversation.messages.iter().find_map(|message| {
                message.parts.iter().find_map(|part| match part {
                    ChatPart::UserForm(spec) if spec.card_key() == card_key => Some(spec),
                    _ => None,
                })
            })
        })
    }

    /// A typed submit clears an old Touch ID note; a saved-login submit keeps "Filling".
    fn user_form_saved_login_note_only_when_idle(&mut self, card_key: &str) {
        if !self
            .saved_login_use
            .get(card_key)
            .is_some_and(SavedLoginUse::is_busy)
        {
            self.saved_login_use.remove(card_key);
        }
    }

    fn already_saved_login(&self, origin: &str, username: &str) -> bool {
        self.site_logins
            .iter()
            .any(|row| origins_match(&row.origin, origin) && row.username == username)
    }

    fn stash_save_candidate(&mut self, card_key: &str, values: &UserFormValues) {
        let pending = {
            let Some(spec) = self.user_form_mut(card_key) else {
                return;
            };
            save_candidate(spec, values)
        };
        let Some(pending) = pending else {
            return;
        };
        self.pending_save
            .insert(pending.form_entry_id.clone(), pending);
    }

    fn offer_save_login(&mut self, card_key: &str) {
        let entry_id = self
            .user_form_mut(card_key)
            .map(|spec| {
                if spec.has_gateway_entry_id() {
                    spec.entry_id.clone()
                } else {
                    spec.card_key().to_string()
                }
            })
            .unwrap_or_else(|| card_key.to_string());
        let Some(pending) = self.pending_save.get(&entry_id) else {
            return;
        };
        if self.already_saved_login(&pending.origin, &pending.username) {
            self.pending_save.remove(&entry_id);
            return;
        }
        let spec = SaveLoginSpec {
            form_entry_id: pending.form_entry_id.clone(),
            origin: pending.origin.clone(),
            username: pending.username.clone(),
        };
        self.push_save_login_part(spec);
    }

    fn push_save_login_part(&mut self, spec: SaveLoginSpec) {
        let entry = spec.form_entry_id.clone();
        for conversation in &mut self.conversations {
            for message in conversation.messages.iter_mut().rev() {
                let has_form = message.parts.iter().any(|part| match part {
                    ChatPart::UserForm(form) => form.card_key() == entry || form.entry_id == entry,
                    _ => false,
                });
                if !has_form {
                    continue;
                }
                if let Some(existing) = message.parts.iter_mut().find_map(|part| match part {
                    ChatPart::SaveLogin(existing)
                        if existing.form_entry_id == spec.form_entry_id =>
                    {
                        Some(existing)
                    }
                    _ => None,
                }) {
                    *existing = spec;
                    return;
                }
                message.parts.push(ChatPart::SaveLogin(spec));
                return;
            }
        }
    }

    fn remove_save_login_part(&mut self, form_entry_id: &str) {
        for conversation in &mut self.conversations {
            for message in &mut conversation.messages {
                message.parts.retain(|part| match part {
                    ChatPart::SaveLogin(spec) => spec.form_entry_id != form_entry_id,
                    _ => true,
                });
            }
        }
    }

    /// Save the offered login: Keychain + sqlite metadata. Never a ChatPart password.
    pub fn save_offered_login(&mut self, form_entry_id: String, cx: &mut Context<Self>) {
        let Some(pending) = self.pending_save.remove(&form_entry_id) else {
            return;
        };
        self.remove_save_login_part(&form_entry_id);
        let Some(vault) = self.site_login_vault.clone() else {
            self.site_login_error = Some("Login vault is not ready.".into());
            cx.notify();
            return;
        };
        let client = self.opengrok.clone();
        cx.spawn(async move |this, cx| {
            let result = save_site_login_everywhere(
                &vault,
                client.as_ref(),
                &pending.origin,
                &pending.username,
                "",
                "",
                KIND_PASSWORD,
                &pending.password,
                None,
            )
            .await;
            drop(pending);
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok((_, notice)) => {
                        state.site_login_error = None;
                        state.site_login_notice = notice;
                        state.reload_site_logins(cx);
                    }
                    Err(err) => state.site_login_error = Some(err),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    /// Settings → Logins → Add. The origin is reduced to the site the vault keys by. True
    /// when the values were taken (the sheet closes and its fields clear); false with the
    /// reason on the page. The new row is the selected one once the list has it.
    pub fn add_site_login(
        &mut self,
        origin: String,
        username: String,
        password: String,
        label: String,
        notes: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(origin) = registrable_origin(origin.trim()) else {
            self.site_login_error = Some("Enter the site, like facebook.com.".into());
            cx.notify();
            return false;
        };
        let username = username.trim().to_string();
        if username.is_empty() || password.is_empty() {
            self.site_login_error = Some("A username and a password are both needed.".into());
            cx.notify();
            return false;
        }
        let Some(vault) = self.site_login_vault.clone() else {
            self.site_login_error = Some("Login vault is not ready.".into());
            cx.notify();
            return false;
        };
        let label = label.trim().to_string();
        let notes = notes.trim_end().to_string();
        self.site_login_add_open = false;
        let client = self.opengrok.clone();
        cx.spawn(async move |this, cx| {
            let result = save_site_login_everywhere(
                &vault,
                client.as_ref(),
                &origin,
                &username,
                &label,
                &notes,
                KIND_PASSWORD,
                &password,
                None,
            )
            .await;
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok((id, notice)) => {
                        state.site_login_error = None;
                        state.site_login_notice =
                            notice.or_else(|| Some(format!("Saved {username} on {origin}.")));
                        state.site_login_selected = Some(id);
                        state.reload_site_logins(cx);
                    }
                    Err(err) => state.site_login_error = Some(err),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
        true
    }

    /// Settings → Logins → Import: a passwords export (CSV) from the Passwords app or a
    /// browser. Each row is saved like an Add; the file is read once and not kept.
    pub fn import_site_logins(&mut self, path: std::path::PathBuf, cx: &mut Context<Self>) {
        let Some(vault) = self.site_login_vault.clone() else {
            self.site_login_error = Some("Login vault is not ready.".into());
            cx.notify();
            return;
        };
        let client = self.opengrok.clone();
        cx.spawn(async move |this, cx| {
            // A file is one of the exports; a directory is a `pass` store, read through the
            // `pass` command with the person's own key.
            let read = cx
                .background_executor()
                .spawn({
                    let path = path.clone();
                    async move {
                        if path.is_dir() {
                            crate::site_login::importers::pass::read_store(&path)
                        } else {
                            crate::site_login::importers::read_file(&path).map(|r| (r, None))
                        }
                    }
                })
                .await;
            let (report, first_error) = match read {
                Ok(read) => read,
                Err(error) => {
                    let _ = this.update(cx, |state, cx| {
                        state.site_login_error = Some(format!("Import failed: {error}"));
                        cx.notify();
                    });
                    return;
                }
            };
            let mut saved = 0;
            let mut failed = Vec::new();
            for item in &report.items {
                match save_site_login_everywhere(
                    &vault,
                    client.as_ref(),
                    &item.origin,
                    &item.username,
                    &item.label,
                    &item.notes,
                    item.kind,
                    item.password.as_deref().unwrap_or(""),
                    item.otpauth.as_deref(),
                )
                .await
                {
                    Ok(_) => saved += 1,
                    Err(error) => {
                        failed.push(format!("{} on {}: {error}", item.username, item.origin))
                    }
                }
            }
            let _ = this.update(cx, |state, cx| {
                let mut notice = format!(
                    "Imported {saved} login{} from {}.",
                    if saved == 1 { "" } else { "s" },
                    report.source
                );
                if report.skipped > 0 {
                    notice.push_str(&match &report.stopped_early {
                        // A store that stops decrypting stops the read: the rest were not
                        // looked at, and saying they had nothing in them would be untrue.
                        Some(entry) => format!(
                            " Stopped at {entry}: {} row{} were not read.",
                            report.skipped,
                            if report.skipped == 1 { "" } else { "s" }
                        ),
                        None => format!(
                            " Skipped {} row{} with no site, name or secret.",
                            report.skipped,
                            if report.skipped == 1 { "" } else { "s" }
                        ),
                    });
                }
                state.site_login_notice = Some(notice);
                let mut errors = failed;
                if let Some(first) = first_error {
                    errors.insert(0, first);
                }
                state.site_login_error = (!errors.is_empty()).then(|| errors.join("; "));
                state.reload_site_logins(cx);
                cx.notify();
            });
        })
        .detach();
    }

    /// Settings → Logins → Import…: the file picker, then [`Self::import_site_logins`].
    pub fn pick_site_logins_import(&mut self, cx: &mut Context<Self>) {
        // A file (an export) or a directory (a `pass` store).
        let answer = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: true,
            directories: true,
            multiple: false,
            prompt: Some("Import".into()),
        });
        cx.spawn(async move |this, cx| {
            let chosen = answer.await;
            let _ = this.update(cx, |state, cx| {
                match chosen {
                    Ok(Ok(Some(paths))) => {
                        if let Some(path) = paths.into_iter().next() {
                            state.import_site_logins(path, cx);
                        }
                    }
                    Ok(Ok(None)) | Err(_) => {}
                    Ok(Err(error)) => {
                        state.site_login_error =
                            Some(format!("The file picker would not open: {error}"));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn skip_save_login(&mut self, form_entry_id: String, cx: &mut Context<Self>) {
        self.pending_save.remove(&form_entry_id);
        self.remove_save_login_part(&form_entry_id);
        cx.notify();
    }

    pub fn delete_site_login(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(vault) = self.site_login_vault.clone() else {
            return;
        };
        let client = self.opengrok.clone();
        cx.spawn(async move |this, cx| {
            // The server first: a row deleted here but not there would come back on the
            // next sync.
            let remote = match client.as_ref() {
                Some(client) => client
                    .delete_site_login(&id)
                    .await
                    .map_err(|error| format!("the server kept it: {error}")),
                None => Ok(()),
            };
            let result = match remote {
                Ok(()) => vault.delete(&id).await.map_err(|error| error.to_string()),
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(()) => {
                        state.site_logins.retain(|row| row.id != id);
                        if state.site_login_selected.as_deref() == Some(id.as_str()) {
                            state.site_login_selected = None;
                        }
                        state.site_login_error = None;
                        state.reload_site_logins(cx);
                    }
                    Err(err) => state.site_login_error = Some(err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// A click anywhere else puts the card's account list away. Nothing is chosen and
    /// nothing is held; the field is free to type in.
    pub fn close_saved_login_list(&mut self, card_key: String, cx: &mut Context<Self>) {
        if self.user_form_list_open.remove(&card_key) {
            cx.notify();
        }
    }

    /// The cursor is in the field the accounts belong to, so the list comes up. Only that
    /// field: the password field is not where an account is chosen, and a card that has
    /// just arrived shows nothing until the person asks.
    pub fn open_saved_login_list(
        &mut self,
        card_key: String,
        field_id: &str,
        cx: &mut Context<Self>,
    ) {
        if self.user_form_list_open.contains(&card_key) {
            return;
        }
        let belongs = self
            .user_form_mut_ref(&card_key)
            .and_then(card_target)
            .is_some_and(|target| match target {
                CardTarget::Login(fields) => fields.username_id == field_id,
                CardTarget::Code { code_id } => code_id == field_id,
                CardTarget::Passkey { .. } => false,
            });
        if belongs {
            self.user_form_list_open.insert(card_key);
            cx.notify();
        }
    }

    /// Settings → Logins: the search field's text, the copy the list filters by.
    pub fn set_site_login_query(&mut self, query: String, cx: &mut Context<Self>) {
        if self.site_login_query != query {
            self.site_login_query = query;
            cx.notify();
        }
    }

    /// Settings → Logins: the row the detail pane shows. An id the list does not have
    /// clears it.
    pub fn select_site_login(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        let id = id.filter(|id| self.site_logins.iter().any(|row| &row.id == id));
        if self.site_login_selected != id {
            self.site_login_selected = id.clone();
            // The pane shows a live code for the row that is open, so this is where its
            // seed is read — one row, when the person asked to see it, and kept after.
            if let Some(id) = id.filter(|id| {
                self.site_logins_with_code.contains(id) && !self.site_login_codes.contains_key(id)
            }) && let Some(vault) = self.site_login_vault.clone()
            {
                cx.spawn(async move |this, cx| {
                    let read = cx
                        .background_executor()
                        .spawn({
                            let id = id.clone();
                            async move {
                                let seed = vault.code_for(&id).ok().flatten()?;
                                crate::site_login::totp::parse(&seed).ok()
                            }
                        })
                        .await;
                    let _ = this.update(cx, |state, cx| {
                        match read {
                            Some(totp) => {
                                state.site_login_codes.insert(id, totp);
                            }
                            // The keychain would not give it up (a cancelled sheet, a
                            // seed that will not parse): the pane shows no code, and the
                            // list stops saying it has one.
                            None => {
                                state.site_logins_with_code.remove(&id);
                            }
                        }
                        cx.notify();
                    });
                })
                .detach();
            }
            cx.notify();
        }
    }

    /// Settings → Logins → Add: the sheet comes up over the page, with a clean slate.
    pub fn open_site_login_add(&mut self, cx: &mut Context<Self>) {
        if !self.site_login_add_open {
            self.site_login_add_open = true;
            self.site_login_error = None;
            cx.notify();
        }
    }

    pub fn close_site_login_add(&mut self, cx: &mut Context<Self>) {
        if self.site_login_add_open {
            self.site_login_add_open = false;
            cx.notify();
        }
    }

    /// The notes on the detail pane, as typed: this Mac's row now, the server's next. Notes
    /// that did not reach the server stay here with a notice saying so.
    pub fn update_site_login_notes(&mut self, id: String, notes: String, cx: &mut Context<Self>) {
        let Some(row) = self.site_logins.iter_mut().find(|row| row.id == id) else {
            return;
        };
        if row.notes == notes {
            return;
        }
        row.notes = notes.clone();
        let Some(vault) = self.site_login_vault.clone() else {
            self.site_login_error = Some("Login vault is not ready.".into());
            cx.notify();
            return;
        };
        let client = self.opengrok.clone();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let result = match vault.update_notes(&id, &notes).await {
                Err(error) => Err(error.to_string()),
                Ok(()) => match client.as_ref() {
                    None => Ok(Some(
                        "Notes saved on this Mac. Sign in to keep them on the server too."
                            .to_string(),
                    )),
                    Some(client) => {
                        let update = crate::opengrok::SiteLoginUpdate {
                            notes: Some(notes.clone()),
                            ..Default::default()
                        };
                        match client.update_site_login(&id, &update).await {
                            Ok(_) => Ok(None),
                            Err(error) => Ok(Some(format!(
                                "Notes saved on this Mac. The server did not take them yet: {error}"
                            ))),
                        }
                    }
                },
            };
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(notice) => {
                        state.site_login_error = None;
                        state.site_login_notice = notice;
                        state.reload_site_logins(cx);
                    }
                    Err(err) => state.site_login_error = Some(err),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// One request per site for its icon, remembered either way: a site with none is not
    /// asked about again this run. Signed out, nothing is asked; the next reload after
    /// signing in asks.
    fn fetch_site_login_icons(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        let missing: Vec<String> = {
            let mut seen = HashSet::new();
            self.site_logins
                .iter()
                .map(|row| row.origin.clone())
                .filter(|origin| {
                    !self.site_login_icons.contains_key(origin) && seen.insert(origin.clone())
                })
                .collect()
        };
        for origin in missing {
            self.site_login_icons.insert(origin.clone(), None);
            let client = client.clone();
            cx.spawn(async move |this, cx| {
                let bytes = match client.site_login_icon(&origin).await {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        log::debug!("no icon for {origin}: {error}");
                        None
                    }
                };
                let Some(image) = bytes.and_then(site_login_icon_image) else {
                    return;
                };
                let _ = this.update(cx, |state, cx| {
                    state.site_login_icons.insert(origin, Some(image));
                    cx.notify();
                });
            })
            .detach();
        }
    }

    fn dispatch_user_form(
        &mut self,
        card_key: String,
        action: UserFormDispatch,
        cx: &mut Context<Self>,
    ) {
        // Do not early-return on `user_form_verbs_available`. That global lock
        // left every stacked open card gray after one MissingRoute 404.
        let Some((entry_id, run_id, conversation_id, agent_id)) = self.user_form_context(&card_key)
        else {
            return;
        };
        let handoff_entry_id = match &action {
            UserFormDispatch::ResolveHandoff(_) => self.box_handoff_post_id(&card_key, &entry_id),
            _ => None,
        };
        if matches!(action, UserFormDispatch::Submit(_)) && entry_id.is_empty() {
            // #140: never POST callId as submit. Dismiss / Open the screen
            // still paint locally on call-* cards.
            return;
        }
        match &action {
            UserFormDispatch::Submit(values) => {
                // A login that came from the vault is not offered to the vault again, and
                // its password is not kept on the side for a save that will not happen.
                if !self.saved_login_in_flight(&card_key) {
                    self.stash_save_candidate(&card_key, values);
                }
                self.paint_user_form_resolution(&card_key, FormResolution::Sending);
                self.user_form_saved_login_note_only_when_idle(&card_key);
                if self
                    .user_form_mut(&card_key)
                    .is_some_and(|spec| spec.live_computer_handoff())
                {
                    self.set_computer_handoff(&card_key, ComputerHandoffStatus::Done);
                    self.user_form_handoff_done.insert(card_key.clone());
                }
            }
            UserFormDispatch::Dismiss(mode) => match mode {
                UserFormDismissMode::Dismissed => {
                    self.paint_user_form_resolution(&card_key, FormResolution::Dismissed);
                    if self
                        .user_form_mut(&card_key)
                        .is_some_and(|spec| spec.shows_computer_handoff())
                    {
                        self.set_computer_handoff(&card_key, ComputerHandoffStatus::Skipped);
                        self.user_form_handoff_done.insert(card_key.clone());
                    }
                }
                UserFormDismissMode::Escalated => {
                    // The person is taking the screen: a held secret (and a code waiting
                    // for its next step) is not theirs to send any more.
                    self.saved_login_use.remove(&card_key);
                    let prior = self
                        .user_form_mut(&card_key)
                        .and_then(|spec| spec.computer_handoff);
                    self.user_form_handoff_restore
                        .insert(card_key.clone(), prior);
                    self.set_computer_handoff(&card_key, ComputerHandoffStatus::ActionNeeded);
                    self.pin_open_screen_shot(&conversation_id);
                    self.show_computer_pane(cx);
                    #[cfg(target_os = "macos")]
                    self.push_computer_window_attention(cx);
                }
            },
            UserFormDispatch::ResolveHandoff(resolution) => {
                self.paint_user_form_resolution(
                    &card_key,
                    crate::opengrok::UserFormSpec::settle_form_from_box(*resolution),
                );
                let computer = match resolution {
                    BoxHandoffResolution::Declined => ComputerHandoffStatus::Skipped,
                    BoxHandoffResolution::HandedBack | BoxHandoffResolution::TimedOut => {
                        ComputerHandoffStatus::Done
                    }
                };
                self.set_computer_handoff(&card_key, computer);
                self.user_form_handoff_done.insert(card_key.clone());
                #[cfg(target_os = "macos")]
                self.push_computer_window_attention(cx);
            }
        }
        // Chrome only: the POST below resumes the parked run on the server, so the thread is
        // not idle yet. The queue is looked at once the server has answered.
        self.sync_waiting_chrome(&conversation_id);
        cx.notify();
        if let UserFormDispatch::ResolveHandoff(resolution) = action {
            if let Some(handoff_entry_id) = handoff_entry_id {
                self.post_box_handoff_resolve(
                    card_key,
                    handoff_entry_id,
                    resolution,
                    run_id,
                    conversation_id,
                    agent_id,
                    cx,
                );
            } else {
                // Sibling id not back yet. Keep Skip live (do not POST form id).
                self.queue_pending_box_handoff(
                    &card_key,
                    &entry_id,
                    PendingBoxHandoff {
                        resolution,
                        run_id,
                        conversation_id,
                        agent_id,
                    },
                );
            }
            return;
        }
        if entry_id.is_empty() {
            // Local Dismiss / Open the screen on call-* (#140). Never POST
            // callId as entryId, never restore the optimistic settle. Nothing resumes on
            // the server, so a held message may go now.
            self.drain_queued_send(&conversation_id, cx);
            return;
        }
        let Some(client) = self.opengrok.clone() else {
            self.restore_user_form(&card_key);
            self.drop_saved_login_fill(&card_key);
            self.settle_chrome_and_drain(&conversation_id, cx);
            return;
        };
        let saved_login = self.saved_login_in_flight(&card_key);
        let saved_login_id = self
            .saved_login_use
            .get(&card_key)
            .and_then(SavedLoginUse::filling_id)
            .map(str::to_string);
        cx.spawn(async move |this, cx| {
            let verb = match &action {
                UserFormDispatch::Submit(_) => UserFormVerb::Submit,
                UserFormDispatch::Dismiss(_) => UserFormVerb::Dismiss,
                UserFormDispatch::ResolveHandoff(_) => unreachable!("resolved above"),
            };
            let dismissed = matches!(
                action,
                UserFormDispatch::Dismiss(UserFormDismissMode::Dismissed)
            );
            let result = match action {
                UserFormDispatch::Submit(values) => {
                    client
                        .submit_user_form(
                            &entry_id,
                            &agent_id,
                            &values,
                            saved_login,
                            saved_login_id.as_deref(),
                        )
                        .await
                }
                UserFormDispatch::Dismiss(mode) => {
                    client.dismiss_user_form(&entry_id, &agent_id, mode).await
                }
                UserFormDispatch::ResolveHandoff(_) => unreachable!("resolved above"),
            };
            let _ = this.update(cx, |state, cx| {
                match result {
                    Ok(reply) => {
                        match crate::opengrok::settle_user_form_http(verb, &reply) {
                            UserFormHttpSettle::Merge(incoming) => {
                                let resolution = incoming.effective_resolution();
                                let incoming_call = incoming.call_id.clone();
                                let origin_call = state
                                    .user_form_mut(&card_key)
                                    .map(|spec| spec.call_id.clone())
                                    .unwrap_or_default();
                                state.remember_user_form_resolution(&incoming);
                                if let Some(spec) = state.user_form_mut(&card_key) {
                                    spec.merge(incoming);
                                }
                                if let Some(res) = resolution {
                                    let call_id = if !incoming_call.is_empty() {
                                        incoming_call
                                    } else {
                                        origin_call
                                    };
                                    state.paint_user_form_call_peers(&call_id, res);
                                }
                                if resolution != Some(FormResolution::FillFailed) {
                                    state.user_form_picks.remove(&card_key);
                                    state.user_form_typed.remove(&card_key);
                                }
                                if resolution == Some(FormResolution::Submitted) {
                                    state.offer_save_login(&card_key);
                                }
                                if resolution.is_some() {
                                    state.saved_login_use.remove(&card_key);
                                }
                                let had_pending =
                                    state.user_form_pending_resolves.contains_key(&card_key)
                                        || state.user_form_pending_resolves.contains_key(&entry_id);
                                state.flush_pending_box_handoff(&card_key, &entry_id, cx);
                                let live_handoff = state
                                    .user_form_mut(&card_key)
                                    .is_some_and(|spec| spec.live_computer_handoff());
                                let follow = if had_pending {
                                    // Skip already queued a resolve; that POST resumes.
                                    false
                                } else if live_handoff
                                    || resolution == Some(FormResolution::Escalated)
                                {
                                    state.end_turn_waiting(
                                        Some(&conversation_id),
                                        Some(WAITING_FOR_YOU_STATUS),
                                    );
                                    false
                                } else if resolution == Some(FormResolution::FillFailed) {
                                    false
                                } else {
                                    true
                                };
                                if follow && !run_id.is_empty() {
                                    state.begin_responding(Some(&conversation_id), "Working");
                                    state.follow_run(run_id, Some(conversation_id.clone()), cx);
                                }
                            }
                            UserFormHttpSettle::Paint(resolution) => {
                                // After Sending: Not filled on 200-null / 404.
                                // Submitted only from a body formResolution (Merge).
                                state.drop_saved_login_fill(&card_key);
                                state.paint_user_form_resolution(&card_key, resolution);
                                state.user_form_restore.remove(&card_key);
                                if resolution != FormResolution::FillFailed {
                                    state.user_form_picks.remove(&card_key);
                                    state.user_form_typed.remove(&card_key);
                                }
                                if resolution == FormResolution::Submitted {
                                    state.offer_save_login(&card_key);
                                }
                                if resolution == FormResolution::Submitted && !run_id.is_empty() {
                                    state.begin_responding(Some(&conversation_id), "Working");
                                    state.follow_run(run_id, Some(conversation_id.clone()), cx);
                                }
                            }
                            UserFormHttpSettle::Keep => {
                                if dismissed && !run_id.is_empty() {
                                    state.begin_responding(Some(&conversation_id), "Working");
                                    state.follow_run(run_id, Some(conversation_id.clone()), cx);
                                }
                            }
                            UserFormHttpSettle::Restore => {
                                state.restore_user_form(&card_key);
                                state.drop_saved_login_fill(&card_key);
                            }
                            UserFormHttpSettle::Refused(message) => {
                                // Nothing was typed; the fields come back, with the reason.
                                state.restore_user_form(&card_key);
                                state
                                    .saved_login_use
                                    .insert(card_key.clone(), SavedLoginUse::Refused { message });
                            }
                        }
                    }
                    Err(error) => {
                        match verb {
                            UserFormVerb::Submit => {
                                state.drop_saved_login_fill(&card_key);
                                state.paint_user_form_resolution(
                                    &card_key,
                                    FormResolution::FillFailed,
                                );
                                state.user_form_restore.remove(&card_key);
                            }
                            UserFormVerb::Dismiss => state.restore_user_form(&card_key),
                        }
                        if error.is_signed_out() {
                            state.note_signed_out(cx);
                        }
                    }
                }
                state.settle_chrome_and_drain(&conversation_id, cx);
                cx.notify();
            });
        })
        .detach();
    }

    pub fn submit_form(&mut self, message_id: String, spec: FormSpec, cx: &mut Context<Self>) {
        // Generative UI only. User-form secrets must never take this path:
        // it concatenates values into `send_message` / AG-UI `content`.
        let picks = self
            .form_picks
            .get(&message_id)
            .cloned()
            .unwrap_or_default();
        let mut lines = Vec::new();
        if let Some(title) = &spec.title {
            lines.push(title.clone());
        }
        for field in &spec.fields {
            if let Some(value) = picks.get(&field.id) {
                lines.push(format!("{}: {value}", field.label));
            }
        }
        let body = lines.join("\n");
        if body.trim().is_empty() {
            return;
        }
        self.send_message(body, cx);
    }

    /// Words from somewhere other than the composer: a form's answer, a driver's `chat.send`.
    /// They go as themselves, with nothing off the draft on them — see [`Self::take_draft_skill`].
    pub fn send_message(&mut self, content: String, cx: &mut Context<Self>) {
        self.send_message_with(content, false, None, cx);
    }

    /// The composer's draft going out, with what the composer had on it.
    ///
    /// The one door that takes the skill off the draft, because it is the one whose words came
    /// off the draft. It is taken before the guards below rather than after, which is what the
    /// composer does with the chip: a send the app refuses empties the field and the chip with
    /// it, so a skill left attached would be one nothing on screen still mentions.
    pub fn send_draft(&mut self, content: String, force_steer: bool, cx: &mut Context<Self>) {
        let skill = self.take_draft_skill();
        self.send_message_with(content, force_steer, skill, cx);
    }

    /// `force_steer` is ⌘⇧↩: send now even if a turn is running. What that
    /// means for each state of the thread is [`plan_send`]. `skill` is what the draft was
    /// holding, which only [`Self::send_draft`] has any business supplying.
    pub fn send_message_with(
        &mut self,
        content: String,
        force_steer: bool,
        skill: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if !self.is_signed_in() {
            self.auth_error = Some("Sign in first".to_string());
            cx.notify();
            return;
        }
        // Signed in by the app's own reckoning and holding nothing to prove it. That gap is the
        // bug: the roster was on screen, the composer took the message, and the turn went out
        // bare. Nothing is put in the thread and nothing is sent — the banner is already saying
        // why, and a turn sent from here would only fetch the same answer back from the server.
        if !self.can_send_turn() {
            self.note_signed_out(cx);
            return;
        }
        if self.active_coworker_id.is_none() {
            self.auth_error = Some("Create a bot first".to_string());
            cx.notify();
            return;
        }
        let conversation_id = match &self.active_conversation_id {
            Some(id) => id.clone(),
            None => return,
        };
        // Decided before the bubble goes in: a forced send over a running turn stops that turn
        // first, so its "You stopped this turn." lands under the turn it is about, not under
        // the message that stopped it.
        let busy = self.busy_state(&conversation_id);
        let plan = plan_send(busy, self.on_send, force_steer);
        let stop_first = if plan == SendPlan::Steer && busy == Busy::Running {
            self.take_running_turn_for_steer(cx)
        } else {
            None
        };
        // The recipe as it stands now, not when the turn reaches the wire: the composer clears
        // it the moment the draft is sent, and a held message must still carry it.
        let recipe = self.active_recipe.as_ref().map(ActiveRecipe::turn);

        let local_id = uuid::Uuid::now_v7().to_string();
        // The bubble and its row are stamped with one moment, so the thread reads back in the
        // order it was said in.
        let said_at = SystemTime::now();
        // The whole reply, not just its preview: the bubble paints the preview, and the quote
        // the coworker is sent is built from the message this one points at.
        let reply = self.reply_to.take();
        let reply_for_queue = reply.clone();
        // Add user message to UI immediately
        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        {
            let message = Message {
                id: local_id.clone(),
                sender: "Me".to_string(),
                content: content.clone(),
                sent_at: said_at,
                is_me: true,
                reply_preview: reply.as_ref().map(|r| r.preview.clone()),
                reply_to_id: reply.as_ref().map(|r| r.message_id.clone()),
                reply_is_me: reply.as_ref().is_some_and(|r| r.is_me),
                parts: Vec::new(),
                run_id: None,
                hidden: false,
            };
            conversation.messages.push(message);
        }
        if let Some(id) = self.active_coworker_id.clone() {
            self.touch_coworker_activity(&id);
        }
        cx.notify();

        // Save to DB
        if let Some(db) = self.database_service.clone() {
            let content_clone = content.clone();
            let conversation_id_clone = conversation_id.clone();
            let title = self.conversation_title(&conversation_id);
            let local_id = local_id.clone();
            let reply = reply.map(|reply| ReplyRef {
                message_id: reply.message_id,
                preview: reply.preview,
                is_me: reply.is_me,
            });
            cx.spawn(async move |this, cx| {
                // A coworker thread has no session row until it first speaks.
                if let Err(e) = db.ensure_session(&conversation_id_clone, &title).await {
                    eprintln!("Failed to save user message: {}", e);
                    return;
                }
                // The row is filed under the bubble's own id, so nothing has to be swapped
                // afterwards: what is on screen and what is on disk answer to the same name
                // from the first moment, and a delete in the meantime finds its row.
                if let Err(e) = db
                    .save_message(
                        &local_id,
                        &conversation_id_clone,
                        "user",
                        &content_clone,
                        None,
                        None,
                        reply,
                        &[],
                        // The person's own message came out of no run. The server's record of a
                        // thread is its runs, and a run is only the coworker's half of a turn.
                        None,
                        false,
                        said_at,
                    )
                    .await
                {
                    eprintln!("Failed to save user message: {}", e);
                }
            })
            .detach();
        }

        match plan {
            SendPlan::Post => {}
            // Held until the thread is idle; `drain_queued_send` posts it then. The bubble is
            // on screen and on its way to disk already, so nothing is lost if the app quits
            // first — the row reads as a message that got no answer, which is what it is.
            SendPlan::Queue => {
                self.queued_sends
                    .entry(conversation_id)
                    .or_default()
                    .push_back(QueuedSend {
                        message_id: local_id,
                        content,
                        recipe,
                        skill,
                        reply: reply_for_queue,
                    });
                cx.notify();
                return;
            }
            // Parked: the server ends the parked run on this message and closes its cards;
            // the app paints the same ending (Superseded) and never `/stop`s first — see
            // `settle_parked_cards`. Running: the turn was stopped above, the way the button
            // does it, and this message goes now. Anything already held stays held — the
            // message the person forced ahead goes first, and the rest follow when it ends.
            SendPlan::Steer => {
                if busy == Busy::Parked {
                    self.settle_parked_cards(&conversation_id);
                }
            }
        }
        self.send_opengrok_turn_with(conversation_id, content, recipe, skill, stop_first, cx);
    }

    /// Post the next held message, if the thread has one and is idle — and is the open
    /// thread, since a turn is sent for the open coworker. Called wherever a thread can go
    /// idle: the ending of a live, replayed or followed run, a card settling with no run
    /// after it, and coming back to the thread. Safe to call anywhere else too: it is a no-op
    /// while anything is running or parked.
    fn drain_queued_send(&mut self, conversation_id: &str, cx: &mut Context<Self>) {
        // Nothing held is the common case, and this runs on every frame: answer it before
        // walking the thread for its busy state.
        if !self
            .queued_sends
            .get(conversation_id)
            .is_some_and(|queue| !queue.is_empty())
        {
            return;
        }
        if self.active_conversation_id.as_deref() != Some(conversation_id)
            || self.busy_state(conversation_id) != Busy::Idle
        {
            return;
        }
        // A turn that cannot leave stays held rather than popped and lost: the signed-out
        // banner already says why, and the pill keeps saying the message is waiting.
        if self.opengrok.is_none() || !self.can_send_turn() {
            return;
        }
        let Some(next) = self
            .queued_sends
            .get_mut(conversation_id)
            .and_then(VecDeque::pop_front)
        else {
            return;
        };
        if self
            .queued_sends
            .get(conversation_id)
            .is_some_and(VecDeque::is_empty)
        {
            self.queued_sends.remove(conversation_id);
        }
        // The turn is built from the thread as it is in memory. A reload may have replaced the
        // thread from disk meanwhile: the bubble is there under its saved id, or — if the save
        // never landed — not at all, in which case it is put back so the coworker is sent what
        // the person typed.
        if let Some(conversation) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
            && !conversation
                .messages
                .iter()
                .any(|m| m.id == next.message_id)
        {
            conversation.messages.push(Message {
                id: next.message_id.clone(),
                sender: "Me".to_string(),
                content: next.content.clone(),
                sent_at: SystemTime::now(),
                is_me: true,
                reply_preview: next.reply.as_ref().map(|r| r.preview.clone()),
                reply_to_id: next.reply.as_ref().map(|r| r.message_id.clone()),
                reply_is_me: next.reply.as_ref().is_some_and(|r| r.is_me),
                parts: Vec::new(),
                run_id: None,
                hidden: false,
            });
        }
        self.send_opengrok_turn_with(
            conversation_id.to_string(),
            next.content,
            next.recipe,
            next.skill,
            None,
            cx,
        );
    }

    /// Waiting chrome, then the queue: the two things a thread going idle has to settle.
    fn settle_chrome_and_drain(&mut self, conversation_id: &str, cx: &mut Context<Self>) {
        self.sync_waiting_chrome(conversation_id);
        self.drain_queued_send(conversation_id, cx);
    }

    fn has_open_approval(&self, conversation_id: &str) -> bool {
        let Some(conversation) = self
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return false;
        };
        conversation.messages.iter().rev().any(|message| {
            !message.is_me
                && !message.hidden
                && message.parts.iter().any(|part| match part {
                    ChatPart::Approval(spec) => {
                        !spec.run_id.is_empty()
                            && !self.approval_answered(&spec.call_id)
                            && self.auto_resolve_local_exec(spec).is_none()
                    }
                    _ => false,
                })
        })
    }

    fn has_open_user_form(&self, conversation_id: &str) -> bool {
        let Some(conversation) = self
            .conversations
            .iter()
            .find(|conversation| conversation.id == conversation_id)
        else {
            return false;
        };
        conversation.messages.iter().rev().any(|message| {
            !message.is_me
                && !message.hidden
                && message.parts.iter().any(|part| match part {
                    ChatPart::UserForm(spec) => {
                        spec.is_unresolved()
                            || spec.effective_resolution() == Some(FormResolution::Sending)
                            || spec.live_computer_handoff()
                    }
                    _ => false,
                })
        })
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_hidden = !self.sidebar_hidden;
        cx.notify();
    }

    pub fn toggle_mini_sidebar(&mut self, cx: &mut Context<Self>) {
        if self.sidebar_hidden {
            self.sidebar_hidden = false;
            self.sidebar_collapsed = false;
        } else {
            self.sidebar_collapsed = !self.sidebar_collapsed;
        }
        self.auto_collapsed = false;
        self.sidebar_responsive = remember_choice(self.sidebar_responsive, self.sidebar_collapsed);
        cx.notify();
    }

    pub fn resize_sidebar(&mut self, width: f32, cx: &mut Context<Self>) {
        let next = sidebar_from_resize(
            SidebarChrome {
                hidden: self.sidebar_hidden,
                collapsed: self.sidebar_collapsed,
                expanded_width: self.sidebar_expanded_width,
            },
            width,
        );
        self.sidebar_hidden = next.hidden;
        self.sidebar_collapsed = next.collapsed;
        self.sidebar_expanded_width = next.expanded_width;
        self.auto_collapsed = false;
        if !next.hidden {
            self.sidebar_responsive = remember_choice(self.sidebar_responsive, next.collapsed);
        }
        cx.notify();
    }

    pub fn set_sidebar_collapsed(&mut self, collapsed: bool, auto: bool, cx: &mut Context<Self>) {
        self.sidebar_collapsed = collapsed;
        self.auto_collapsed = auto;
        self.sidebar_responsive = remember_choice(self.sidebar_responsive, collapsed);
        cx.notify();
    }

    pub fn apply_responsive_sidebar(&mut self, width: f32, cx: &mut Context<Self>) {
        let result = collapse_for_width(self.sidebar_responsive, width, self.sidebar_collapsed);
        self.sidebar_responsive = result.next;
        if let Some(apply) = result.apply {
            if self.sidebar_collapsed != apply {
                self.sidebar_collapsed = apply;
                self.auto_collapsed = true;
                cx.notify();
            }
        }
    }

    pub fn toggle_debug_markdown(&mut self, cx: &mut Context<Self>) {
        self.debug_markdown_disabled = !self.debug_markdown_disabled;
        println!(
            "[DEBUG] Markdown rendering: {}",
            if self.debug_markdown_disabled {
                "DISABLED (plain text)"
            } else {
                "ENABLED"
            }
        );
        cx.notify();
    }

    pub fn toggle_theme(&mut self, cx: &mut Context<Self>) {
        use gpui_kit::component::{ActiveTheme, Theme};

        let visually_dark = cx.has_global::<Theme>() && cx.theme().is_dark();
        let next = crate::theme::next_toggle_mode(&self.theme_mode, visually_dark);
        self.set_theme_mode(next, cx);
    }

    pub fn restore_saved_theme(&mut self, cx: &mut Context<Self>) {
        self.theme_mode = crate::theme::load_saved_mode();
        crate::theme::apply_mode(&self.theme_mode, cx);
        cx.notify();
    }

    pub fn restore_saved_on_send(&mut self) {
        self.on_send = crate::prefs::load_on_send(&Config::data_dir());
    }

    pub fn set_on_send(&mut self, on_send: OnSend, cx: &mut Context<Self>) {
        if self.on_send != on_send {
            self.on_send = on_send;
            #[cfg(not(test))]
            crate::prefs::save_on_send(&Config::data_dir(), on_send);
            cx.notify();
        }
    }

    /// The bubble is on screen but its turn has not gone yet.
    pub fn is_send_queued(&self, message_id: &str) -> bool {
        self.queued_sends
            .values()
            .flatten()
            .any(|queued| queued.message_id == message_id)
    }

    /// How many messages the open thread is holding back.
    pub fn queued_send_count(&self) -> usize {
        self.active_conversation_id
            .as_deref()
            .and_then(|id| self.queued_sends.get(id))
            .map_or(0, VecDeque::len)
    }

    pub fn set_theme_mode(&mut self, mode: &str, cx: &mut Context<Self>) {
        self.theme_mode = crate::theme::normalize_mode(mode).to_string();
        #[cfg(not(test))]
        crate::theme::save_mode(&self.theme_mode);
        crate::theme::apply_mode(&self.theme_mode, cx);
        cx.notify();
    }

    pub fn toggle_app_settings(&mut self, cx: &mut Context<Self>) {
        self.is_app_settings_open = !self.is_app_settings_open;
        if self.is_app_settings_open {
            self.dismiss_popovers(cx);
            self.refresh_host_egress(cx);
            if self.app_settings_tab == AppSettingsTab::Computer {
                self.refresh_computers(cx);
                self.refresh_coworker_computer_quietly(cx);
            }
            if self.app_settings_tab == AppSettingsTab::Skills {
                self.refresh_skills(cx);
            }
        }
        self.record_nav();
        cx.notify();
    }

    pub fn set_app_settings_tab(&mut self, tab: AppSettingsTab, cx: &mut Context<Self>) {
        if self.app_settings_tab != tab {
            self.app_settings_tab = tab;
            self.record_nav();
            if tab == AppSettingsTab::Computer {
                self.refresh_computers(cx);
                self.refresh_host_egress(cx);
                // Route traffic and the network choice for a shared box live on this tab and
                // read the open bot's computer record, which nothing else on this page fetches.
                self.refresh_coworker_computer_quietly(cx);
            }
            // The library is the account's and nothing else fetches it, so arriving on the tab
            // is when it is asked for.
            if tab == AppSettingsTab::Skills {
                self.refresh_skills(cx);
            }
            cx.notify();
        }
    }

    pub fn refresh_computers(&mut self, cx: &mut Context<Self>) {
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        // A roster reload is the moment a server upgrade would show; ask again.
        self.computer_endpoint_missing = false;
        let this_id = self.local_exec_machine_id.clone();
        cx.spawn(async move |this, cx| {
            let Ok(mut computers) = client.list_computers().await else {
                return;
            };
            for computer in &mut computers {
                computer.this_machine = this_id.as_ref() == Some(&computer.machine_id);
                if computer.this_machine {
                    computer.online = true;
                }
            }
            computers = collapse_computer_roster(computers);
            computers.sort_by_key(|computer| !computer.this_machine);
            let _ = this.update(cx, |state, cx| {
                state.computers = computers;
                cx.notify();
            });
        })
        .detach();
    }

    pub fn set_computer_exec_mode(
        &mut self,
        machine_id: String,
        mode: LocalExecMode,
        cx: &mut Context<Self>,
    ) {
        if let Some(computer) = self
            .computers
            .iter_mut()
            .find(|computer| computer.machine_id == machine_id)
        {
            computer.mode = mode;
        }
        cx.notify();
        let Some(client) = self.opengrok.clone() else {
            return;
        };
        cx.spawn(async move |this, cx| {
            let stored = mode.as_stored();
            let ok = client
                .set_local_exec_mode(&machine_id, stored)
                .await
                .is_ok();
            let confirmed = if ok {
                client
                    .local_exec_mode(&machine_id)
                    .await
                    .ok()
                    .map(|mode| LocalExecMode::from_stored(&mode))
            } else {
                None
            };
            let _ = this.update(cx, |state, cx| {
                if confirmed != Some(mode) {
                    state.refresh_computers(cx);
                    return;
                }
                if mode == LocalExecMode::Always {
                    state.sync_pending_approvals(cx);
                }
            });
        })
        .detach();
    }

    pub fn set_submit_chord(&mut self, chord: SubmitChord, cx: &mut Context<Self>) {
        if self.submit_chord != chord {
            self.submit_chord = chord;
            cx.notify();
        }
    }

    pub fn set_voice_mode(&mut self, open: bool, cx: &mut Context<Self>) {
        self.is_voice_mode_open = open;
        if !open {
            self.stop_voice_mode(cx);
        }
        cx.notify();
    }

    pub fn toggle_voice_mute(&mut self, cx: &mut Context<Self>) {
        self.is_voice_muted = !self.is_voice_muted;
        cx.notify();
    }

    pub fn open_app_settings(&mut self, tab: AppSettingsTab, cx: &mut Context<Self>) {
        self.app_settings_tab = tab;
        if !self.is_app_settings_open {
            self.is_app_settings_open = true;
            self.dismiss_popovers(cx);
        }
        self.refresh_host_egress(cx);
        if tab == AppSettingsTab::Computer {
            self.refresh_computers(cx);
        }
        self.record_nav();
        cx.notify();
    }

    pub fn toggle_account_settings(&mut self, cx: &mut Context<Self>) {
        self.open_app_settings(AppSettingsTab::Profile, cx);
    }

    pub fn start_voice_mode(&mut self, cx: &mut Context<Self>) {
        self.is_voice_mode_open = true;
        self.voice_status = VoiceStatus::Connecting;
        cx.notify();

        match AudioInput::new(self.amplitude.clone()) {
            Ok(input) => {
                self.audio_input = Some(input);
                self.voice_status = VoiceStatus::Connected;
            }
            Err(e) => {
                eprintln!("Failed to start local audio input: {}", e);
                self.voice_status = VoiceStatus::Error("Mic Error".to_string());
            }
        }
        cx.notify();
    }

    pub fn stop_voice_mode(&mut self, cx: &mut Context<Self>) {
        self.is_voice_mode_open = false;
        self.voice_status = VoiceStatus::Disconnected;
        self.audio_input = None;
        cx.notify();
    }

    pub fn warm_tts(&mut self, cx: &mut Context<Self>) {
        self.ensure_tts_service(cx);
    }

    fn ensure_tts_service(&mut self, cx: &mut Context<Self>) {
        if self.tts_service.is_some() || self.tts_initing {
            return;
        }
        self.tts_initing = true;
        cx.spawn(async move |this, cx| {
            let service = cx
                .background_executor()
                .spawn(async move {
                    let service = TtsService::new();
                    service.warm_native();
                    service
                })
                .await;
            let _ = this.update(cx, |state, cx| {
                state.tts_initing = false;
                state.tts_service = Some(service);
                if let Some((text, message_id)) = state.pending_read_aloud.take() {
                    state.read_aloud(text, message_id, TtsSource::Native, cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn read_aloud(
        &mut self,
        text: String,
        message_id: String,
        source: TtsSource,
        cx: &mut Context<Self>,
    ) {
        let _ = source;
        if self.tts_service.is_none() {
            self.pending_read_aloud = Some((text.clone(), message_id.clone()));
            self.native_tts.message_id = Some(message_id);
            self.native_tts.is_loading = true;
            self.native_tts.is_paused = false;
            self.ensure_tts_service(cx);
            cx.notify();
            return;
        }

        if let Some(service) = &self.tts_service {
            self.native_tts.message_id = Some(message_id.clone());
            self.native_tts.is_paused = false;
            self.native_tts.is_loading = false;
            cx.notify();

            let service = service.clone();
            let message_id = message_id.clone();
            let text = text.clone();

            if service.start_speaking_native(&text, &message_id) {
                cx.spawn(async move |this, cx| {
                    service.wait_until_finished_native().await;
                    if let Some(this) = this.upgrade() {
                        let _ = this.update(cx, |state, cx| {
                            if state.native_tts.message_id.as_ref() == Some(&message_id) {
                                state.native_tts.message_id = None;
                                cx.notify();
                            }
                        });
                    }
                })
                .detach();
            }
        }
    }

    pub fn stop_read_aloud(&mut self, cx: &mut Context<Self>) {
        if let Some(service) = &self.tts_service {
            service.stop_native();
            self.native_tts = SourceTtsState::default();
            cx.notify();
        }
    }

    pub fn pause_read_aloud(&mut self, cx: &mut Context<Self>) {
        if let Some(service) = &self.tts_service {
            if self.native_tts.message_id.is_some() && !self.native_tts.is_paused {
                service.pause_native();
                self.native_tts.is_paused = service
                    .native_paused
                    .load(std::sync::atomic::Ordering::SeqCst);
            }
            cx.notify();
        }
    }

    pub fn resume_read_aloud(&mut self, cx: &mut Context<Self>) {
        if let Some(service) = &self.tts_service {
            if self.native_tts.message_id.is_some() && self.native_tts.is_paused {
                service.resume_native();
                self.native_tts.is_paused = false;
            }
            cx.notify();
        }
    }

    pub fn active_highlight_range(&self) -> Option<std::ops::Range<usize>> {
        self.tts_service
            .as_ref()
            .and_then(|s| s.get_active_word_range())
    }

    /// The one entry the menu and F8 use: reads a message; on the message being read, pauses;
    /// on a paused one, resumes. A message still loading its voice is left alone, so a double
    /// click cannot pause a synthesizer that has not started (which would wedge the next start).
    pub fn toggle_read_aloud(
        &mut self,
        message_id: String,
        text: String,
        mode: TtsSource,
        cx: &mut Context<Self>,
    ) {
        let _ = mode;
        if self.native_tts.message_id.as_ref() == Some(&message_id) {
            if self.native_tts.is_loading {
                return;
            }
            if self.native_tts.is_paused {
                self.resume_read_aloud(cx);
            } else {
                self.pause_read_aloud(cx);
            }
            return;
        }
        self.read_aloud(text, message_id, TtsSource::Native, cx);
    }

    /// Whether this message is the one being read, and whether it is paused.
    pub fn read_aloud_state(&self, message_id: &str) -> (bool, bool) {
        let reading = self.native_tts.message_id.as_deref() == Some(message_id);
        (reading, reading && self.native_tts.is_paused)
    }
}

// ---------------------------------------------------------------------------
// Lightbox. Everything the picture overlay does to the app's state lives here;
// the overlay itself is components/lightbox.rs.
// ---------------------------------------------------------------------------

use crate::components::lightbox::Lightbox;

impl AppState {
    /// Show a turn's pictures full window, starting at the one that was clicked.
    pub fn open_lightbox(
        &mut self,
        shots: Vec<crate::opengrok::ScreenshotSpec>,
        index: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(last) = shots.len().checked_sub(1) else {
            return;
        };
        self.lightbox = Some(Lightbox {
            shots,
            index: index.min(last),
        });
        cx.notify();
    }

    pub fn close_lightbox(&mut self, cx: &mut Context<Self>) {
        if self.lightbox.take().is_some() {
            cx.notify();
        }
    }

    /// The next (+1) or previous (-1) picture of the set. A set is a ring: the arrows never
    /// dead-end, they come round again.
    pub fn step_lightbox(&mut self, delta: i32, cx: &mut Context<Self>) {
        let Some(open) = self.lightbox.as_mut() else {
            return;
        };
        let total = open.shots.len() as i32;
        if total < 2 {
            return;
        }
        open.index = (open.index as i32 + delta).rem_euclid(total) as usize;
        cx.notify();
    }

    /// The picture the filmstrip was clicked on.
    pub fn show_lightbox_image(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(open) = self.lightbox.as_mut() else {
            return;
        };
        if index >= open.shots.len() || open.index == index {
            return;
        }
        open.index = index;
        cx.notify();
    }

    /// Write the picture being shown into the person's Downloads folder. The bytes are the
    /// ones the run already decoded, so nothing is re-encoded and nothing is asked of the
    /// person: a file dialog over a picture they are looking at helps no one.
    pub fn download_lightbox_image(&self) -> Result<String, String> {
        let open = self
            .lightbox
            .as_ref()
            .ok_or_else(|| "No picture is open.".to_string())?;
        let shot = open
            .current()
            .ok_or_else(|| "No picture is open.".to_string())?;
        let dir = directories::UserDirs::new()
            .and_then(|dirs| dirs.download_dir().map(std::path::Path::to_path_buf))
            .ok_or_else(|| "This Mac has no Downloads folder.".to_string())?;
        let stamp = Local::now().format("%Y%m%d-%H%M%S");
        let path = dir.join(format!("nativechat-{stamp}-{}.png", open.index + 1));
        std::fs::write(&path, &shot.image.bytes).map_err(|error| error.to_string())?;
        Ok(path.display().to_string())
    }
}

/// The queue's own words where it has them.
///
/// A card off the queue used to be described by the app: every one of them was
/// a host shell waiting on consent, so saying so cost nothing. The MCP door
/// raises cards for reasons of its own — a policy, an auto-review — and says
/// why in the item, so the card now reads back what the server sent and only
/// falls through to the old wording for a server that sends neither.
/// The once-only line a standing answer falls back to when its policy write failed: a Never
/// that was not kept still denied this call; an Always still allowed it.
fn once_only_for(mode: LocalExecMode) -> ApprovalDecision {
    match mode {
        LocalExecMode::Never => ApprovalDecision::Denied,
        LocalExecMode::Always | LocalExecMode::Ask => ApprovalDecision::AllowOnce,
    }
}

/// What a card's decision becomes once the run has taken the answer. A standing choice
/// (Always / Never) whose policy write has already failed was re-marked as once-only by that
/// failure, and the answer landing later must not promote it back: `None` keeps what is
/// there. Everything else is written as decided.
fn settled_decision(
    current: Option<&ApprovalDecision>,
    decided: ApprovalDecision,
) -> Option<ApprovalDecision> {
    let standing = matches!(decided, ApprovalDecision::Always | ApprovalDecision::Never);
    let downgraded = matches!(
        current,
        Some(ApprovalDecision::AllowOnce | ApprovalDecision::Denied)
    );
    if standing && downgraded {
        None
    } else {
        Some(decided)
    }
}

fn spec_from_queued(item: &QueuedApproval) -> ApprovalSpec {
    ApprovalSpec {
        run_id: item.run_id.clone(),
        thread_id: some_unless_blank(&item.thread_id),
        call_id: item.call_id.clone(),
        tool: item.tool.clone(),
        command: command_from_args(&item.arguments),
        why: item
            .why
            .as_deref()
            .and_then(some_unless_blank)
            .unwrap_or_else(|| "your machine's owner must approve this command".to_string()),
        reason: item
            .reason
            .as_deref()
            .and_then(some_unless_blank)
            .unwrap_or_else(|| "exec-consent".to_string()),
        output: None,
        ok: None,
    }
}

/// One of a coworker's optional fields as the roster keeps it. The app asks for such a field
/// to be cleared by patching it with nothing in it.
fn some_unless_blank(value: &str) -> Option<String> {
    (!value.trim().is_empty()).then(|| value.to_string())
}

/// The roster's copy of a coworker brought up to what a patch asks for, before the server has
/// been asked at all.
fn apply_patch(coworker: &mut Coworker, patch: &CoworkerPatch) {
    if let Some(name) = patch.name.as_ref() {
        coworker.name = name.clone();
    }
    if let Some(model) = patch.model.as_ref() {
        coworker.model = model.clone();
    }
    if let Some(role) = patch.role.as_deref() {
        coworker.role = some_unless_blank(role);
    }
    if let Some(title) = patch.title.as_deref() {
        coworker.title = some_unless_blank(title);
    }
    if let Some(shape) = patch.avatar_shape.as_deref() {
        coworker.avatar_shape = some_unless_blank(shape);
    }
    if let Some(color) = patch.avatar_color.as_deref() {
        coworker.avatar_color = some_unless_blank(color);
    }
    if let Some(notify) = patch.notify_on_updates {
        coworker.notify_on_updates = Some(notify);
    }
}

/// The roster's copy of a coworker once the server has answered, for the fields the patch
/// asked about and no others.
///
/// What was written optimistically is not evidence of anything: the server may have refused
/// the patch, or taken the request and stored none of it, which is what this one still does
/// with several of these fields. So a field the server echoed back is the server's, and a
/// field it said nothing about goes back to what the roster held before the request. The value
/// the app guessed is kept nowhere. `echo` is `None` when the request failed, which leaves
/// every patched field as it was.
///
/// A patch that asks for a field to be cleared is the one place where a silent answer is
/// taken for agreement: there is no value to be wrong about, and a server that leaves empty
/// fields out of its answer would otherwise make clearing an avatar impossible. A refused
/// request is not a silent answer but no answer at all, and takes the clearing back with
/// everything else.
fn settle_patch(
    coworker: &mut Coworker,
    patch: &CoworkerPatch,
    echo: Option<&Coworker>,
    before: &Coworker,
) {
    let answered = echo.is_some();
    if patch.name.is_some() {
        coworker.name = settled_text(echo.map(|c| c.name.as_str()), &before.name);
    }
    if patch.model.is_some() {
        coworker.model = settled_text(echo.map(|c| c.model.as_str()), &before.model);
    }
    if let Some(role) = patch.role.as_deref() {
        coworker.role = settled_option(
            echo.and_then(|c| c.role.clone()),
            answered && some_unless_blank(role).is_none(),
            before.role.clone(),
        );
    }
    if let Some(title) = patch.title.as_deref() {
        coworker.title = settled_option(
            echo.and_then(|c| c.title.clone()),
            answered && some_unless_blank(title).is_none(),
            before.title.clone(),
        );
    }
    if let Some(shape) = patch.avatar_shape.as_deref() {
        coworker.avatar_shape = settled_option(
            echo.and_then(|c| c.avatar_shape.clone()),
            answered && some_unless_blank(shape).is_none(),
            before.avatar_shape.clone(),
        );
    }
    if let Some(color) = patch.avatar_color.as_deref() {
        coworker.avatar_color = settled_option(
            echo.and_then(|c| c.avatar_color.clone()),
            answered && some_unless_blank(color).is_none(),
            before.avatar_color.clone(),
        );
    }
    if patch.notify_on_updates.is_some() {
        coworker.notify_on_updates = echo
            .and_then(|c| c.notify_on_updates)
            .or(before.notify_on_updates);
    }
}

/// A field a coworker always has, after the server has answered. An answer that left the field
/// out says nothing about it, and nothing is not a name or a model.
fn settled_text(echo: Option<&str>, before: &str) -> String {
    echo.filter(|text| !text.is_empty())
        .unwrap_or(before)
        .to_string()
}

/// A field a coworker may not have, after the server has answered.
fn settled_option(echo: Option<String>, cleared: bool, before: Option<String>) -> Option<String> {
    match echo {
        Some(value) => Some(value),
        None if cleared => None,
        None => before,
    }
}

/// What a pick fetches after Touch ID, by the card's target.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HeldSecret {
    Password,
    CodeSeed,
    None,
}

/// Save on the server first, then in this Mac's keychain under the server's id. Without a
/// reachable server (or one with no vault) the row is saved here only and the notice says
/// so; the next sync files it on the server. The row's id comes back with the notice, so
/// the page can show what was just saved.
#[allow(clippy::too_many_arguments)]
async fn save_site_login_everywhere(
    vault: &SiteLoginVault,
    client: Option<&crate::opengrok::OpenGrokClient>,
    origin: &str,
    username: &str,
    label: &str,
    notes: &str,
    kind: &str,
    password: &str,
    otpauth: Option<&str>,
) -> Result<(String, Option<String>), String> {
    let save = crate::opengrok::SiteLoginSave {
        origin,
        username,
        label,
        notes,
        kind,
        password: (!password.is_empty()).then_some(password),
        otpauth,
    };
    let keep_code = |row_id: &str| -> Result<(), String> {
        if let Some(seed) = otpauth {
            vault.set_code(row_id, seed).map_err(|e| e.to_string())?;
        }
        Ok(())
    };
    if let Some(client) = client {
        match client.save_site_login_full(&save).await {
            Ok(remote) => {
                // The server's words and stamp, not this Mac's: the row it wrote is the one
                // every Mac syncs from, and a later local stamp would refuse its corrections.
                let row = vault
                    .save_with_id(
                        &remote.id,
                        origin,
                        username,
                        &remote.label,
                        &remote.notes,
                        &remote.kind,
                        password,
                        (remote.updated_at_ms > 0).then_some(remote.updated_at_ms),
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                keep_code(&row.id)?;
                return Ok((row.id, None));
            }
            Err(error) => {
                let row = vault
                    .save(origin, username, label, notes, kind, password)
                    .await
                    .map_err(|error| error.to_string())?;
                keep_code(&row.id)?;
                return Ok((
                    row.id,
                    Some(format!(
                        "Saved on this Mac. The server did not take it yet: {error}"
                    )),
                ));
            }
        }
    }
    let row = vault
        .save(origin, username, label, notes, kind, password)
        .await
        .map_err(|error| error.to_string())?;
    keep_code(&row.id)?;
    Ok((
        row.id,
        Some("Saved on this Mac. Sign in to keep it on the server too.".to_string()),
    ))
}

/// Bring this Mac and the server to the same list. Rows the server has and this Mac does
/// not are remembered here without their password; rows this Mac saved before the server
/// knew them are filed there and take the server's id; a row both have takes the server's
/// title, notes, kind and last use when the server's copy is the newer one.
async fn sync_site_logins(
    vault: &SiteLoginVault,
    client: Option<&crate::opengrok::OpenGrokClient>,
) -> Result<(), String> {
    let Some(client) = client else {
        return Ok(());
    };
    let remote = client
        .list_site_logins()
        .await
        .map_err(|error| error.to_string())?;
    let local = vault.list().await.map_err(|error| error.to_string())?;
    for row in &remote {
        let known = local.iter().find(|mine| {
            mine.id == row.id || (mine.origin == row.origin && mine.username == row.username)
        });
        match known {
            None => vault
                .remember_remote(
                    &row.id,
                    &row.origin,
                    &row.username,
                    &row.label,
                    &row.notes,
                    &row.kind,
                    row.last_used_at_ms,
                )
                .await
                .map_err(|error| error.to_string())?,
            Some(mine) => {
                let took = vault
                    .update_from_server(
                        mine,
                        &row.label,
                        &row.notes,
                        &row.kind,
                        row.last_used_at_ms,
                        row.updated_at_ms,
                    )
                    .await
                    .map_err(|error| error.to_string())?;
                // This Mac's copy is the newer one and says something else: the words go
                // up. A notes edit the server refused at the time is filed on the next
                // sync instead of being lost.
                if !took
                    && (mine.label.trim() != row.label.trim()
                        || mine.notes.trim() != row.notes.trim())
                {
                    let update = crate::opengrok::SiteLoginUpdate {
                        label: Some(mine.label.trim().to_string()),
                        notes: Some(mine.notes.trim().to_string()),
                    };
                    match client.update_site_login(&row.id, &update).await {
                        // The server's reply is the last word, stamp included, so the two
                        // are level and the next sync sends nothing.
                        Ok(filed) => {
                            let _ = vault
                                .update_from_server(
                                    mine,
                                    &filed.label,
                                    &filed.notes,
                                    &filed.kind,
                                    filed.last_used_at_ms,
                                    filed.updated_at_ms,
                                )
                                .await;
                        }
                        Err(error) => {
                            log::warn!("could not send a login's words to the server: {error}")
                        }
                    }
                }
            }
        }
    }
    for mine in &local {
        if remote.iter().any(|row| row.id == mine.id) {
            continue;
        }
        // The same login under another id: the server's id wins, and the keychain item
        // moves with it, so a later delete reaches both.
        if let Some(twin) = remote
            .iter()
            .find(|row| row.origin == mine.origin && row.username == mine.username)
        {
            vault
                .adopt_id(&mine.id, &twin.id)
                .await
                .map_err(|error| error.to_string())?;
            continue;
        }
        // A row the server can be given: its password, its code seed, or both. A row whose
        // secret is on another Mac only has nothing to file. A keychain that would not
        // answer is not the same as a row with no secret: the row waits for the next sync
        // rather than being filed without what it holds.
        let (Ok(password), Ok(otpauth)) =
            (vault.secret_for_fill(&mine.id), vault.code_for(&mine.id))
        else {
            log::warn!("the keychain would not answer for a login; leaving it for the next sync");
            continue;
        };
        if password.is_none() && otpauth.is_none() {
            continue;
        }
        let filed = client
            .save_site_login_full(&crate::opengrok::SiteLoginSave {
                origin: &mine.origin,
                username: &mine.username,
                label: &mine.label,
                notes: &mine.notes,
                kind: &mine.kind,
                password: password.as_deref(),
                otpauth: otpauth.as_deref(),
            })
            .await
            .map_err(|error| error.to_string())?;
        vault
            .adopt_id(&mine.id, &filed.id)
            .await
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// A site's icon bytes as a picture. A site serves whatever it likes, so the format is
/// read off the first bytes; bytes that are none of the known pictures are no icon.
fn site_login_icon_image(bytes: Vec<u8>) -> Option<Arc<Image>> {
    let format = image_format_of(&bytes)?;
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

fn image_format_of(bytes: &[u8]) -> Option<ImageFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(ImageFormat::Png)
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some(ImageFormat::Jpeg)
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some(ImageFormat::Gif)
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some(ImageFormat::Webp)
    } else if bytes.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        Some(ImageFormat::Ico)
    } else if bytes.starts_with(b"BM") {
        Some(ImageFormat::Bmp)
    } else {
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
        let head = head.trim_start();
        (head.starts_with("<svg") || (head.starts_with("<?xml") && head.contains("<svg")))
            .then_some(ImageFormat::Svg)
    }
}

/// The one file a skill's instructions live in. Everything else in the folder is a file the
/// instructions refer to.
const SKILL_FILE: &str = "SKILL.md";

/// What an upload sends, from what was picked: a `SKILL.md` on its own, or the folder one lives
/// in with the files beside it.
fn read_skill_upload(path: &std::path::Path) -> Result<(String, Vec<SkillFile>), String> {
    if path.is_dir() {
        return skill_bundle(read_skill_folder(path)?);
    }
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_else(|| path.display().to_string());
    // What it weighs before what it holds. A 500 MB file picked by mistake is half a gigabyte
    // allocated to find out it was a mistake, and the sentence that came back was about a
    // request body rather than about the file somebody chose.
    let about =
        std::fs::metadata(path).map_err(|error| format!("{name} could not be read: {error}"))?;
    // A pipe or a device says it is zero bytes long and then reads forever. The walk over a
    // folder has always asked this of every entry; a picked path is the same question.
    if !about.is_file() {
        return Err(format!(
            "{name} is not a file, so it cannot be a skill's instructions."
        ));
    }
    if let Some(why) = instructions_too_heavy(&name, about.len()) {
        return Err(why);
    }
    let bytes =
        std::fs::read(path).map_err(|error| format!("{name} could not be read: {error}"))?;
    String::from_utf8(bytes)
        .map(|body| (body, Vec::new()))
        .map_err(|_| format!("{name} is not text, so it cannot be a skill's instructions."))
}

/// The words for a picked file that cannot be a skill's instructions, or `None` when it could.
///
/// The cap it is measured against is the bundle's, which is about this Mac's memory; the cap it
/// NAMES is the instructions' own, which is the server's and is what the person is up against.
/// A file this far over it is not a `SKILL.md` that needs trimming, it is the wrong file — and
/// a refusal that named a bundle would send somebody looking for files they never picked.
fn instructions_too_heavy(name: &str, bytes: u64) -> Option<String> {
    (bytes > SKILL_BUNDLE_LIMIT as u64).then(|| {
        format!(
            "{name} is {:.1} MB. A skill's instructions are at most {SKILL_BODY_CHARS} \
             characters — whatever that file is, it is not a SKILL.md.",
            bytes as f64 / (1024. * 1024.)
        )
    })
}

/// The words for one bundled file that is too heavy to go with a skill, or `None` when it is
/// not. Asked of what the filesystem says it weighs, before it is opened.
fn too_heavy(name: &str, bytes: u64) -> Option<String> {
    (bytes > SKILL_BUNDLE_LIMIT as u64).then(|| {
        format!(
            "{name} is {:.1} MB. A skill is instructions and the small files they refer to — at \
             most {} KB of them.",
            bytes as f64 / (1024. * 1024.),
            SKILL_BUNDLE_LIMIT / 1024
        )
    })
}

/// The words for a bundle over the size cap.
///
/// Said in two places — the walk over a picked folder stops on it, and the bundle built from
/// that walk refuses on it — so a folder that was stopped part-way through is refused in the
/// same words as one that was read to the end. Both count the same thing: the files BESIDE the
/// instructions, which is what the server caps.
fn bundle_too_big() -> String {
    format!(
        "A skill's files come to at most {} KB together. Leave out what the instructions do not \
         refer to.",
        SKILL_BUNDLE_LIMIT / 1024
    )
}

/// The words for a bundle with too many files in it, said in the same two places and for the
/// same reason.
///
/// It names the cap and not what arrived: the walk stops counting the moment it is past, so a
/// number from there would mean "at least this many" while reading as a fact.
fn bundle_too_many() -> String {
    format!(
        "A skill carries at most {SKILL_BUNDLE_FILES} files beside its {SKILL_FILE}. Leave out \
         what the instructions do not refer to."
    )
}

/// Every file under a picked folder, as (path relative to it, bytes).
///
/// Nothing is read until the filesystem has been asked what it weighs, and the walk stops the
/// moment the bundle is past either cap. A folder somebody picked by mistake — a checkout, a
/// downloads directory, a mail store — is refused on what it says it is, rather than read into
/// memory and refused afterwards.
///
/// Dot entries are left where they are: a `.git` is not part of a skill, and a mis-picked folder
/// is mostly dot entries. So are symlinks, and everything that is neither a file nor a folder:
/// what a link points at is not inside the bundle, and copying it onto a computer would be
/// copying something the person never picked.
///
/// A subfolder that will not open is stepped over rather than taken as the end of the upload,
/// the way an entry that will not stat is. The folder the person actually picked is different:
/// if THAT cannot be read there is nothing to upload, and it says so.
fn read_skill_folder(dir: &std::path::Path) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut found: Vec<(String, Vec<u8>)> = Vec::new();
    let mut beside = 0usize;
    let mut total = 0usize;
    let mut stack = vec![(dir.to_path_buf(), String::new())];
    while let Some((at, prefix)) = stack.pop() {
        let entries = match std::fs::read_dir(&at) {
            Ok(entries) => entries,
            Err(error) if prefix.is_empty() => {
                return Err(format!("{} could not be read: {error}", at.display()));
            }
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let path = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                stack.push((entry.path(), path));
                // A folder of folders is a folder either way: the file cap on its own lets a
                // tree of empty directories push paths for as long as there are any, without
                // one file ever being found to count.
                if stack.len() > SKILL_BUNDLE_FILES {
                    return Err(bundle_too_many());
                }
                continue;
            }
            if !kind.is_file() {
                continue;
            }
            let Ok(size) = entry.metadata().map(|meta| meta.len()) else {
                continue;
            };
            if let Some(why) = too_heavy(&path, size) {
                return Err(why);
            }
            // The instructions are capped by the server on their own, in characters, and are
            // not part of what a bundle may weigh — so they are weighed here only as one file,
            // above, which is what keeps a six-gigabyte SKILL.md from being read.
            if !path.eq_ignore_ascii_case(SKILL_FILE) {
                beside += 1;
                if beside > SKILL_BUNDLE_FILES {
                    return Err(bundle_too_many());
                }
                total = total.saturating_add(size as usize);
                if total > SKILL_BUNDLE_LIMIT {
                    return Err(bundle_too_big());
                }
            }
            let bytes = std::fs::read(entry.path())
                .map_err(|error| format!("{path} could not be read: {error}"))?;
            found.push((path, bytes));
        }
    }
    Ok(found)
}

/// A picked folder as a skill: its `SKILL.md` is the instructions, and everything else is a file
/// that goes with them, base64 as the server takes it.
///
/// The caps are the server's and are checked here too, because a bundle refused after it has
/// been uploaded is a refusal that cost the person the upload. They are counted the way the walk
/// counts them — the files BESIDE the instructions — and refused in the same words, so a folder
/// stopped part-way through and one read to the end say the same thing.
fn skill_bundle(found: Vec<(String, Vec<u8>)>) -> Result<(String, Vec<SkillFile>), String> {
    use base64::Engine as _;
    let mut body: Option<String> = None;
    let mut files = Vec::new();
    let mut total = 0usize;
    for (path, bytes) in found {
        if path.eq_ignore_ascii_case(SKILL_FILE) {
            body = Some(String::from_utf8(bytes).map_err(|_| {
                format!("{SKILL_FILE} is not text, so it cannot be a skill's instructions.")
            })?);
            continue;
        }
        total += bytes.len();
        files.push(SkillFile {
            path,
            bytes: base64::engine::general_purpose::STANDARD.encode(&bytes),
        });
    }
    let Some(body) = body else {
        return Err(format!(
            "That folder has no {SKILL_FILE} in it, which is where a skill's instructions live."
        ));
    };
    if files.len() > SKILL_BUNDLE_FILES {
        return Err(bundle_too_many());
    }
    if total > SKILL_BUNDLE_LIMIT {
        return Err(bundle_too_big());
    }
    // A stack walk comes back in whatever order the filesystem hands the entries over, and the
    // same folder uploaded twice should send the same bundle.
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((body, files))
}

#[cfg(test)]
mod tests {
    /// Whether `chmod 000` stops this process at all.
    ///
    /// It stops everybody but root, so on a root CI container the two refusals below have
    /// nothing to say — and a guard that silently stops guarding is worse than one that says it
    /// did not run. Asked by trying it rather than by reading a uid, because what the tests turn
    /// on is whether the open fails, not who is running them.
    fn permissions_bind(at: &std::path::Path) -> bool {
        use std::os::unix::fs::PermissionsExt as _;
        let probe = at.join("probe");
        std::fs::write(&probe, b"x").unwrap();
        std::fs::set_permissions(&probe, std::fs::Permissions::from_mode(0o000)).unwrap();
        let stopped = std::fs::read(&probe).is_err();
        std::fs::remove_file(&probe).unwrap();
        stopped
    }

    /// Where a refused create is drawn is decided by which surface asked for it, and it has to
    /// be told rather than guessed at: the sheet has a slot over the fields the sentence is
    /// about, and an upload has no sheet at all. A sentence put in a slot that is not on screen
    /// is a click that did nothing.
    #[test]
    fn a_refused_create_is_drawn_where_it_was_asked_for() {
        let mut sheet = super::AppState::new();
        sheet.refuse_create(super::SkillCreateFrom::Sheet, "that name is taken".into());
        assert_eq!(sheet.skill_add_error.as_deref(), Some("that name is taken"));
        assert!(sheet.skills_error.is_none());

        let mut upload = super::AppState::new();
        upload.refuse_create(super::SkillCreateFrom::Upload, "that name is taken".into());
        assert_eq!(upload.skills_error.as_deref(), Some("that name is taken"));
        assert!(
            upload.skill_add_error.is_none(),
            "there is no sheet on screen to draw it in"
        );
    }

    /// The walk over a real folder: what it takes, what it steps over, and what it refuses
    /// before opening anything.
    ///
    /// The oversized file is unreadable on purpose. If the size check ever moves back below the
    /// open, this test stops seeing the sentence about megabytes and starts seeing "could not be
    /// read" — which is the whole difference between refusing a 6 GB file and allocating it.
    #[test]
    fn a_folder_is_walked_without_opening_what_it_must_not_send() {
        use std::io::Write as _;
        let dir = tempfile::tempdir().expect("a folder to walk");
        let at = dir.path();
        let write = |path: &str, bytes: &[u8]| {
            let full = at.join(path);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            let mut file = std::fs::File::create(&full).unwrap();
            file.write_all(bytes).unwrap();
            full
        };
        write("SKILL.md", b"Ask for the receipt first.");
        write("reference/rates.csv", b"ok, hi\n");
        write(".git/config", b"[core]\n");
        std::os::unix::fs::symlink("/etc/hosts", at.join("hosts")).unwrap();

        let found = super::read_skill_folder(at).expect("a folder with instructions in it");
        let mut paths: Vec<&str> = found.iter().map(|(path, _)| path.as_str()).collect();
        paths.sort_unstable();
        assert_eq!(
            paths,
            vec!["SKILL.md", "reference/rates.csv"],
            "a dot directory is not part of a skill, and neither is what a link points at"
        );

        // A file too heavy to be part of a skill, which cannot be opened at all.
        let bound = permissions_bind(at);
        let big = write("big.bin", b"");
        std::fs::File::options()
            .write(true)
            .open(&big)
            .unwrap()
            .set_len(super::SKILL_BUNDLE_LIMIT as u64 + 1)
            .unwrap();
        if bound {
            std::fs::set_permissions(&big, std::os::unix::fs::PermissionsExt::from_mode(0o000))
                .unwrap();
        }
        let why = super::read_skill_folder(at).expect_err("over the cap");
        assert!(why.contains("MB"), "{why}");
        if bound {
            assert!(
                !why.contains("could not be read"),
                "the size was asked for before the file was opened: {why}"
            );
        }
        std::fs::remove_file(&big).unwrap();

        // And one file too many, counted as they are found rather than at the end.
        for index in 0..=super::SKILL_BUNDLE_FILES {
            write(&format!("note-{index}.md"), b"x");
        }
        let why = super::read_skill_folder(at).expect_err("too many files");
        assert!(
            why.contains(&super::SKILL_BUNDLE_FILES.to_string()),
            "{why}"
        );
    }

    /// A folder that will not open has nothing in it to upload, and a subfolder that will not
    /// open is one corner of a folder that does.
    #[test]
    fn a_folder_that_will_not_open_says_so_and_a_subfolder_is_stepped_over() {
        let dir = tempfile::tempdir().expect("a folder to walk");
        let at = dir.path();
        std::fs::write(at.join("SKILL.md"), b"Ask first.").unwrap();
        // Nothing is shut to root, so there is no shut corner to step over and nothing here to
        // check. The refusal below is about a folder that is not there, which binds everybody.
        if permissions_bind(at) {
            let shut = at.join("shut");
            std::fs::create_dir(&shut).unwrap();
            std::fs::write(shut.join("inside.md"), b"x").unwrap();
            std::fs::set_permissions(&shut, std::os::unix::fs::PermissionsExt::from_mode(0o000))
                .unwrap();
            let found = super::read_skill_folder(at).expect("one shut corner is not the end of it");
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].0, "SKILL.md");
            std::fs::set_permissions(&shut, std::os::unix::fs::PermissionsExt::from_mode(0o700))
                .unwrap();
        }

        let why = super::read_skill_folder(&at.join("nowhere")).expect_err("no such folder");
        assert!(why.contains("could not be read"), "{why}");
    }

    /// A picked file is the instructions themselves: read when it is text and small enough to
    /// be a skill's, and refused on what the filesystem says it weighs before that.
    #[test]
    fn a_picked_file_is_the_instructions_when_it_can_be_one() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().expect("a folder to write in");
        let at = dir.path();

        let md = at.join("SKILL.md");
        std::fs::write(&md, b"Ask for the receipt first.").unwrap();
        let (body, files) = super::read_skill_upload(&md).expect("a file of instructions");
        assert_eq!(body, "Ask for the receipt first.");
        assert!(
            files.is_empty(),
            "one file is one file, with nothing beside it"
        );

        let binary = at.join("notes.bin");
        std::fs::write(&binary, [0xFF, 0xFE, 0x00]).unwrap();
        let why = super::read_skill_upload(&binary).expect_err("not text");
        assert!(why.contains("is not text"), "{why}");

        // A device is not a file: it says it is zero bytes long and then reads forever.
        let why = super::read_skill_upload(std::path::Path::new("/dev/null"))
            .expect_err("a device is not a SKILL.md");
        assert!(why.contains("is not a file"), "{why}");

        let bound = permissions_bind(at);
        let big = at.join("big.bin");
        std::fs::File::create(&big)
            .unwrap()
            .set_len(super::SKILL_BUNDLE_LIMIT as u64 + 1)
            .unwrap();
        if bound {
            std::fs::set_permissions(&big, std::fs::Permissions::from_mode(0o000)).unwrap();
        }
        let why = super::read_skill_upload(&big).expect_err("over the cap");
        assert!(why.contains("MB"), "{why}");
        assert!(
            why.contains(&crate::opengrok::SKILL_BODY_CHARS.to_string()),
            "the file being refused IS the instructions, so the cap it names is theirs: {why}"
        );
        if bound {
            assert!(
                !why.contains("could not be read"),
                "half a gigabyte is not allocated to find out it was a mistake: {why}"
            );
        }
    }

    /// The instructions are the `SKILL.md`; everything else in the folder is a file they refer
    /// to, sent base64 and in a settled order so the same folder twice is the same bundle.
    #[test]
    fn a_picked_folder_is_its_instructions_and_the_files_beside_them() {
        let (body, files) = super::skill_bundle(vec![
            ("reference/rates.csv".to_string(), b"ok, hi\n".to_vec()),
            (
                "SKILL.md".to_string(),
                b"Ask for the receipt first.".to_vec(),
            ),
            ("checklist.md".to_string(), b"one\ntwo\n".to_vec()),
        ])
        .expect("a folder with instructions in it");
        assert_eq!(body, "Ask for the receipt first.");
        let paths: Vec<&str> = files.iter().map(|file| file.path.as_str()).collect();
        assert_eq!(paths, vec!["checklist.md", "reference/rates.csv"]);
        assert_eq!(
            files[1].bytes, "b2ssIGhpCg==",
            "a file travels base64, the way an artifact does"
        );
    }

    /// A folder of anything at all is not a skill, and saying which file is missing is the only
    /// way somebody knows what to add.
    #[test]
    fn a_folder_with_no_instructions_says_which_file_is_missing() {
        let why = super::skill_bundle(vec![("notes.txt".to_string(), b"hello".to_vec())])
            .expect_err("no SKILL.md");
        assert!(why.contains("SKILL.md"), "{why}");
    }

    /// Both caps are the server's, and both are checked before the upload rather than after it:
    /// a refusal that arrives after the bytes have gone up cost the person the upload.
    #[test]
    fn a_bundle_over_a_cap_is_refused_with_the_number_in_it() {
        let mut many = vec![("SKILL.md".to_string(), b"body".to_vec())];
        for index in 0..=super::SKILL_BUNDLE_FILES {
            many.push((format!("file-{index}.md"), b"x".to_vec()));
        }
        let why = super::skill_bundle(many).expect_err("too many files");
        assert!(
            why.contains(&super::SKILL_BUNDLE_FILES.to_string()),
            "{why}"
        );

        let heavy = vec![
            ("SKILL.md".to_string(), b"body".to_vec()),
            (
                "big.bin".to_string(),
                vec![0u8; super::SKILL_BUNDLE_LIMIT + 1],
            ),
        ];
        let why = super::skill_bundle(heavy).expect_err("too many bytes");
        assert!(why.contains("KB"), "{why}");
    }

    /// The rows on the page and the numbers on the toggle come out of one pair of answers, so
    /// either half failing takes both: a count that the list under it disagrees with is worse
    /// than no count at all. A pair for a refresh that has since been overtaken is dropped
    /// whole, which is what keeps a slow answer for one side of the toggle off the other.
    #[test]
    fn a_library_lands_whole_or_not_at_all() {
        let rows = |ids: &[&str]| -> Vec<crate::opengrok::SkillSummary> {
            ids.iter()
                .map(|id| {
                    serde_json::from_value(serde_json::json!({ "id": id, "name": id })).unwrap()
                })
                .collect()
        };
        let mut state = super::AppState::new();
        state.skills_epoch = 4;

        assert!(
            !state.take_skills(
                3,
                super::SkillScope::Yours,
                Ok(rows(&["skl_1"])),
                Ok(Vec::new())
            ),
            "an answer for a refresh that has been overtaken is not this page's answer"
        );
        assert!(state.skills.is_empty());

        assert!(state.take_skills(
            4,
            super::SkillScope::Discover,
            Ok(rows(&["skl_1"])),
            Ok(rows(&["skl_2", "skl_3"])),
        ));
        assert_eq!(
            state.skills.len(),
            2,
            "the side that is open is the side shown"
        );
        assert_eq!(state.skills[0].id, "skl_2");
        assert_eq!(
            state.skills_counts,
            super::SkillCounts {
                yours: 1,
                discover: 2
            }
        );
        assert!(!state.skills_loading);

        state.skills_epoch = 5;
        assert!(state.take_skills(
            5,
            super::SkillScope::Discover,
            Ok(rows(&["skl_1"])),
            Err(crate::opengrok::OpenGrokError::message(
                "the gateway is down"
            )),
        ));
        assert!(
            state.skills.is_empty(),
            "rows left under a red line are rows somebody reads as current"
        );
        assert_eq!(state.skills_counts, super::SkillCounts::default());
        assert_eq!(state.skills_error.as_deref(), Some("the gateway is down"));
    }

    /// The rows on screen belong to the side that was open, so they go the moment it does —
    /// otherwise they sit under the other side's name until the fetch lands, and for as long as
    /// the app runs if it never does.
    #[test]
    fn moving_the_toggle_takes_the_rows_with_it() {
        let mut state = super::AppState::new();
        state.skills =
            serde_json::from_value(serde_json::json!([{ "id": "skl_1", "name": "a" }])).unwrap();
        assert!(state.take_skills_scope(super::SkillScope::Discover));
        assert_eq!(state.skills_scope, super::SkillScope::Discover);
        assert!(state.skills.is_empty());
        assert!(
            !state.take_skills_scope(super::SkillScope::Discover),
            "the toggle was already there and nothing moved"
        );
    }

    /// The two words on the toggle are the words a driver uses, and they are not the words on
    /// the wire: "Yours" asks the server for `mine`.
    #[test]
    fn the_skills_toggle_keeps_the_screens_words_and_the_wires_apart() {
        use super::SkillScope;
        assert_eq!(SkillScope::from_word("yours"), Some(SkillScope::Yours));
        assert_eq!(
            SkillScope::from_word("discover"),
            Some(SkillScope::Discover)
        );
        assert_eq!(SkillScope::from_word("mine"), None);
        assert_eq!(SkillScope::Yours.query(), "mine");
        assert_eq!(SkillScope::Discover.query(), "org");
        assert_eq!(SkillScope::Discover.label(), "Discover");
        let counts = super::SkillCounts {
            yours: 3,
            discover: 11,
        };
        assert_eq!(counts.of(SkillScope::Yours), 3);
        assert_eq!(counts.of(SkillScope::Discover), 11);
    }

    /// A site's icon is whatever the site serves; the first bytes say which picture it is,
    /// and a page that is not a picture is no icon at all.
    #[test]
    fn an_icon_is_read_by_its_first_bytes() {
        use super::image_format_of;
        use gpui_kit::ImageFormat;
        assert_eq!(
            image_format_of(b"\x89PNG\r\n\x1a\n rest"),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            image_format_of(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(image_format_of(b"GIF89a...."), Some(ImageFormat::Gif));
        assert_eq!(
            image_format_of(b"RIFF\x00\x00\x00\x00WEBPVP8 "),
            Some(ImageFormat::Webp)
        );
        assert_eq!(
            image_format_of(&[0x00, 0x00, 0x01, 0x00, 0x01, 0x00]),
            Some(ImageFormat::Ico)
        );
        assert_eq!(image_format_of(b"BM\x00\x00"), Some(ImageFormat::Bmp));
        assert_eq!(
            image_format_of(b"  <?xml version=\"1.0\"?>\n<svg xmlns=\"x\"/>"),
            Some(ImageFormat::Svg)
        );
        assert_eq!(image_format_of(b"<svg/>"), Some(ImageFormat::Svg));
        assert_eq!(image_format_of(b"<html><body>404</body></html>"), None);
        assert_eq!(image_format_of(b""), None);
    }

    /// A cron row is the schedule picker again, so the editor opens a routine off the wire
    /// showing the words it was chosen by rather than a line to decipher.
    #[test]
    fn a_cron_row_is_a_routine_with_the_picker_back_on_it() {
        let routine = super::routine_from_schedule(
            serde_json::from_value(serde_json::json!({
                "id": "sch_1", "coworkerId": "cw_1", "kind": "cron", "cron": "0 9 * * 1,2,3,4,5",
                "prompt": "Read the inbox", "name": "Morning post", "active": true
            }))
            .unwrap(),
        );
        assert_eq!(routine.id, "sch_1");
        assert_eq!(routine.name, "Morning post");
        assert_eq!(routine.instruction, "Read the inbox");
        assert!(routine.active);
        let [super::RoutineTrigger::Schedule { id, spec }] = &routine.triggers[..] else {
            panic!("a cron row is a schedule trigger: {:?}", routine.triggers);
        };
        assert_eq!(id, "sch_1", "the trigger is the schedule, so it is its id");
        assert_eq!(spec.label(), "Weekdays at 9:00 AM");
    }

    /// The URL and key on a webhook routine are the server's and no others: the app used to
    /// mint both, and what it minted was never anywhere but the app.
    #[test]
    fn a_webhook_row_carries_the_servers_url_and_key() {
        let routine = super::routine_from_schedule(
            serde_json::from_value(serde_json::json!({
                "id": "sch_2", "coworkerId": "cw_1", "kind": "webhook", "cron": null,
                "prompt": "Deal with it", "active": false,
                "webhook": {
                    "url": "https://og.example/hooks/sch_2",
                    "key": "og_live_abc",
                    "header": "Authorization: Bearer og_live_abc"
                }
            }))
            .unwrap(),
        );
        assert!(!routine.active, "a paused schedule is a paused routine");
        let [
            super::RoutineTrigger::Webhook {
                url, key, header, ..
            },
        ] = &routine.triggers[..]
        else {
            panic!("a webhook row is a webhook trigger: {:?}", routine.triggers);
        };
        assert_eq!(url, "https://og.example/hooks/sch_2");
        assert_eq!(key, "og_live_abc");
        assert_eq!(header, "Authorization: Bearer og_live_abc");
    }

    /// A line the pickers cannot draw is still a routine: it lists, it pauses, it deletes, and
    /// the line is shown as the server wrote it.
    #[test]
    fn a_line_with_no_picker_for_it_is_still_a_routine() {
        let routine = super::routine_from_schedule(
            serde_json::from_value(serde_json::json!({
                "id": "sch_3", "coworkerId": "cw_1", "kind": "cron", "cron": "@every 90m",
                "prompt": "Sweep", "active": true
            }))
            .unwrap(),
        );
        assert_eq!(routine.triggers[0].label(), "@every 90m");
    }

    #[test]
    fn a_picked_tool_knows_whether_it_is_a_tool_or_an_app() {
        // The chip row used to read the kind off a hardcoded list of menu names. A real tool's
        // name comes from the server, so the rule is the one the server itself uses: a
        // qualified name belongs to a plugin, which is what a person means by an app.
        assert_eq!(PickedKind::of("shell"), PickedKind::Tool);
        assert_eq!(PickedKind::of("run_recipe"), PickedKind::Tool);
        assert_eq!(PickedKind::of("gmail.api.send"), PickedKind::App);
    }
    /// The values a turn carries are typed as the declaration said they would be, because that
    /// is what the server validates them against — a number sent as the word "5" is a number
    /// the server has every right to refuse.
    #[test]
    /// Picking a workflow from `/` puts it on the draft the way a recipe goes, and the draft
    /// keeps the one thing that differs: what it is called. Everything else — the declaration,
    /// the defaults standing in their fields, what the turn carries — is the same machinery,
    /// which is why there is one type for both and not two that drift.
    #[test]
    fn a_workflow_goes_on_the_draft_under_its_own_noun() {
        let declaration = serde_json::json!([
            { "name": "term", "required": true, "kind": "text" },
            { "name": "tries", "required": false, "kind": "number", "default": 3 }
        ]);
        let tape: RecipeSummary = serde_json::from_value(serde_json::json!({
            "id": "rcp_1", "name": "search", "kind": "recipe", "parameters": declaration
        }))
        .unwrap();
        let tree: RecipeSummary = serde_json::from_value(serde_json::json!({
            "id": "rcp_2", "name": "search", "kind": "workflow", "parameters": declaration
        }))
        .unwrap();

        let on_draft = ActiveRecipe::from_summary(&tree);
        assert!(on_draft.is_workflow());
        assert_eq!(on_draft.kind.label(), "Workflow");
        assert_eq!(
            on_draft.parameters,
            ActiveRecipe::from_summary(&tape).parameters,
            "a tree declares what it needs told exactly as a tape does"
        );
        assert_eq!(
            on_draft.value("tries"),
            Some("3"),
            "a declared default stands in its field on a tree too"
        );
        assert_eq!(on_draft.missing(), vec!["term"]);
        assert_eq!(on_draft.turn().id, "rcp_2");

        // A listing from a server that has never heard of workflows says nothing about kind,
        // and every row it sends is a tape.
        let old: RecipeSummary =
            serde_json::from_value(serde_json::json!({ "id": "rcp_3", "name": "search" })).unwrap();
        assert!(!ActiveRecipe::from_summary(&old).is_workflow());
    }

    #[test]
    fn a_recipe_on_the_draft_sends_each_value_as_the_kind_it_was_declared() {
        let recipe: RecipeSummary = serde_json::from_value(serde_json::json!({
            "id": "rcp_1",
            "name": "youtube",
            "parameters": [
                { "name": "search_term", "required": true, "kind": "text" },
                { "name": "count", "required": false, "kind": "number", "default": 5 },
                { "name": "shorts", "required": false, "kind": "boolean" },
                { "name": "lang", "required": false, "kind": "text", "values": ["en", "es"] }
            ]
        }))
        .unwrap();
        let mut active = ActiveRecipe::from_summary(&recipe);
        assert_eq!(
            active.value("count"),
            Some("5"),
            "a declared default is what the run would use, so the field shows it standing there"
        );
        assert_eq!(active.missing(), vec!["search_term"]);

        active.set_value("search_term", Some("mundo".to_string()));
        active.set_value("shorts", Some("yes".to_string()));
        active.set_value("lang", Some("ES".to_string()));
        assert!(active.missing().is_empty());

        let turn = active.turn();
        assert_eq!(turn.id, "rcp_1");
        assert_eq!(
            serde_json::Value::Object(turn.values),
            serde_json::json!({
                "search_term": "mundo",
                "count": 5,
                "shorts": true,
                "lang": "es"
            })
        );

        // Blank is not a value: it leaves the parameter unfilled, and out of the turn.
        active.set_value("search_term", Some("   ".to_string()));
        assert_eq!(active.missing(), vec!["search_term"]);
        assert!(!active.turn().values.contains_key("search_term"));
    }

    /// ONE TURN. A skill is put on the draft by a pick and comes off as the turn is built, so
    /// the message after it carries nothing unless somebody picked again.
    ///
    /// What the first assertion pins is narrow and worth saying out loud: a draft that was never
    /// picked from has nothing on it. It does NOT pin that the words are unread — nothing in
    /// this file reads them, and a test can only show that nothing here puts a skill on a draft
    /// except [`AppState::attach_skill`], which is reached from the panel's pick and from
    /// nowhere else in the app.
    #[test]
    fn a_picked_skill_goes_with_one_turn_and_a_typed_one_goes_with_none() {
        let mut state = AppState::new();
        state.your_skills = serde_json::from_value(serde_json::json!([
            { "id": "skl_1", "name": "expense-report", "description": "File a receipt" },
            { "id": "skl_2", "name": "  ", "description": "Nobody named it" }
        ]))
        .unwrap();
        assert_eq!(
            state.take_draft_skill(),
            None,
            "nothing was picked, so there is nothing for the turn to carry"
        );

        assert!(state.attach_skill("skl_1"));
        assert_eq!(
            state.active_skill.as_ref().map(|skill| skill.name.as_str()),
            Some("expense-report")
        );
        assert_eq!(state.take_draft_skill().as_deref(), Some("skl_1"));
        assert_eq!(
            state.take_draft_skill(),
            None,
            "a skill applies to the message it was sent with and to nothing after it"
        );

        // One to a message: the second pick is the one that goes.
        assert!(state.attach_skill("skl_1"));
        assert!(state.attach_skill("skl_2"));
        assert_eq!(
            state.active_skill.as_ref().map(|skill| skill.id.as_str()),
            Some("skl_2")
        );
        assert_eq!(
            state.active_skill.as_ref().map(|skill| skill.name.as_str()),
            Some("Untitled skill"),
            "the draft calls a nameless skill what the `/` row called it"
        );

        // An id the library does not hold is not a pick, and leaves what was there alone.
        assert!(!state.attach_skill("skl_9"));
        assert_eq!(
            state.take_draft_skill().as_deref(),
            Some("skl_2"),
            "the skill on the draft survived a pick that named nothing"
        );
    }

    /// `/` offers this person's own skills, whatever the Settings page is showing.
    ///
    /// The page's list is one side of a toggle and is emptied the moment the toggle moves. A
    /// composer reading it offered a colleague's library and none of this person's own, to
    /// somebody who had merely looked at the other tab once and closed it.
    #[test]
    fn the_slash_list_is_your_own_skills_and_not_whichever_tab_was_left_open() {
        let yours: Vec<SkillSummary> = serde_json::from_value(serde_json::json!([
            { "id": "skl_1", "name": "expense-report", "description": "File a receipt" }
        ]))
        .unwrap();
        let theirs: Vec<SkillSummary> = serde_json::from_value(serde_json::json!([
            { "id": "skl_9", "name": "their-skill", "description": "Ada's" }
        ]))
        .unwrap();

        let mut state = AppState::new();
        state.skills_scope = SkillScope::Discover;
        assert!(state.take_skills(
            state.skills_epoch,
            SkillScope::Discover,
            Ok(yours.clone()),
            Ok(theirs.clone())
        ));
        assert_eq!(
            state.skills, theirs,
            "the page shows the side of the toggle that was asked for"
        );
        assert_eq!(
            state.your_skills, yours,
            "and `/` has the other half of the same answer, so a skill written on that page \
             can be used without asking for it again"
        );
        assert!(
            state.attach_skill("skl_1"),
            "the row `/` offered is the row a pick can find"
        );
        assert!(
            !state.attach_skill("skl_9"),
            "the Discover side is not what `/` listed, so nothing there is pickable from it"
        );

        // A listing that failed takes the page's rows down with it, because a list under a red
        // line reads as current. What `/` already has is left alone: it is still true, and a
        // panel emptied by somebody else's failed refresh would offer nothing at all.
        assert!(state.take_skills(
            state.skills_epoch,
            SkillScope::Discover,
            Ok(yours.clone()),
            Err(OpenGrokError::message(
                "the server would not say".to_string()
            ))
        ));
        assert!(state.skills.is_empty());
        assert_eq!(state.your_skills, yours);
    }

    /// The declaration the owner hit this on, with an optional parameter declared ahead of a
    /// required one so the ordering is a claim about the list and not about the JSON.
    fn youtube() -> RecipeSummary {
        serde_json::from_value(serde_json::json!({
            "id": "rcp_1",
            "name": "youtube",
            "parameters": [
                { "name": "count", "required": false, "kind": "number", "default": 5 },
                { "name": "search_term", "required": true, "kind": "text" },
                { "name": "channel", "required": true, "kind": "text" },
                { "name": "lang", "required": false, "kind": "text" }
            ]
        }))
        .unwrap()
    }

    #[test]
    fn what_is_left_to_tell_a_recipe_puts_the_required_first_and_leaves_the_told_out() {
        let mut active = ActiveRecipe::from_summary(&youtube());
        let names = |recipe: &ActiveRecipe| -> Vec<String> {
            recipe
                .unfilled()
                .into_iter()
                .map(|(_, parameter)| parameter.name.clone())
                .collect()
        };
        assert_eq!(
            names(&active),
            vec!["search_term", "channel", "lang"],
            "what stops the message being sent is asked for first, and count came with a \
             default standing in its field, so there is nothing left to ask about it"
        );
        assert_eq!(
            active
                .unfilled()
                .iter()
                .map(|(index, _)| *index)
                .collect::<Vec<_>>(),
            vec![1, 2, 3],
            "each one keeps its place in the declaration, which is how a pick names it back"
        );

        active.set_value("search_term", Some("mundo".to_string()));
        assert_eq!(
            names(&active),
            vec!["channel", "lang"],
            "a parameter that has been told something is done with, and drops out"
        );

        active.set_value("channel", Some("anything".to_string()));
        active.set_value("lang", Some("es".to_string()));
        assert!(
            active.unfilled().is_empty(),
            "nothing left to tell it, which is what the panel shows its empty state for"
        );
    }

    /// Dropping the recipe is `Option::take`, and the values live inside the recipe: nothing
    /// outside it remembers them, so picking the same one again starts from its declaration
    /// rather than resuming a run somebody abandoned.
    #[test]
    fn a_recipe_picked_again_starts_empty_rather_than_where_the_last_one_stopped() {
        let declared = youtube();
        let mut on_draft = Some(ActiveRecipe::from_summary(&declared));
        if let Some(active) = on_draft.as_mut() {
            active.set_value("search_term", Some("mundo".to_string()));
            active.set_value("count", Some("20".to_string()));
        }
        on_draft.take();

        let again = ActiveRecipe::from_summary(&declared);
        assert_eq!(
            again.value("search_term"),
            None,
            "what was typed for the last run has no business in this one"
        );
        assert_eq!(
            again.value("count"),
            Some("5"),
            "the declared default stands again, in place of the 20 that went with the recipe"
        );
    }

    // Item by item rather than a glob: `use super::*` would drag in gpui_kit's own `test`.
    use super::{
        ActiveRecipe, ActivityTick, AppState, BotActivity, ChatMessage, ChatPart, Conversation,
        DatabaseService, EMPTY_TURN_NOTE, LiveTurn, Message, ModelCatalogue, PickedKind,
        REPLY_QUOTE_CHARS, RUN_ERROR_PREFIX, RecipeSummary, RecoveredReply, RouteTrafficSurface,
        STOP_UNSENT_NOTE, STOPPED_TURN_NOTE, SkillScope, SkillSummary, TURN_SIGNED_OUT_NOTE,
        TURN_UNREACHED_NOTE, ThreadRun, TurnAssembler, TurnEnding, WAITING_APPROVAL_STATUS,
        WAITING_FOR_YOU_STATUS, agui_messages, apply_catalogue, apply_reload, bot_status_line,
        bubble_for_run, graft_reply, hide_messages_of_runs, is_status_line, is_tool_standin,
        is_unsent_turn_note, missing_replies, overlay_server_cards, parse_sql_time,
        reads_as_gateway_unreachable, replayed_ending, reply_from_replay, restored_message,
        restored_parts, saved_parts, spec_from_queued, stream_paint_due, stream_part_sig,
        streaming_message_mut, turn_ending, unheard_hidden_runs,
    };
    // `CredentialRequestResolution` went with the broker this branch deleted; everything
    // else main's tests reach for is still here.
    use crate::opengrok::{
        Failure, FormField, FormResolution, FormSpec, LocalExecMode, ModelEntry, OpenGrokClient,
        OpenGrokError, QueuedApproval, USER_MACHINE_SHELL, UiSpec,
    };
    use crate::state::{
        ApprovalDecision, Busy, MessagePart, PendingBoxHandoff, PendingSave, once_only_for,
        settled_decision,
    };
    use std::str::FromStr;
    use std::sync::Arc;
    use std::time::{Duration, Instant, SystemTime};

    fn message(id: &str, is_me: bool, content: &str) -> Message {
        Message {
            id: id.to_string(),
            sender: if is_me { "Me" } else { "AI" }.to_string(),
            content: content.to_string(),
            sent_at: SystemTime::UNIX_EPOCH,
            is_me,
            reply_preview: None,
            reply_to_id: None,
            reply_is_me: false,
            parts: Vec::new(),
            run_id: None,
            hidden: false,
        }
    }

    fn replying_to(mut message: Message, quoted: &Message) -> Message {
        message.reply_to_id = Some(quoted.id.clone());
        message.reply_preview = Some(quoted.content.clone());
        message.reply_is_me = quoted.is_me;
        message
    }

    /// The whole point: "what am I replying to?" must arrive with the quote, because the server
    /// answers from the array the app sends and nothing else.
    #[test]
    fn a_reply_reaches_the_coworker_as_a_quote_ahead_of_its_own_words() {
        let bot = message("m1", false, "The build is green.");
        let reply = replying_to(message("m2", true, "what am I replying to?"), &bot);
        let sent = agui_messages(&[bot, reply]);
        assert_eq!(sent.len(), 2);
        assert_eq!(
            sent[1].content,
            "[Replying to your earlier message: \"The build is green.\"]\n\nwhat am I replying to?"
        );
        let quote = sent[1].reply_to.as_ref().expect("the field is filled too");
        assert_eq!(quote.message_id, "m1");
        assert_eq!(quote.preview, "The build is green.");
        assert!(!quote.is_me);
    }

    #[test]
    fn stream_paint_coalesces_text_and_flushes_on_a_card() {
        let t0 = Instant::now();
        let text = vec![ChatPart::Text("hi".into())];
        let sig = stream_part_sig(&text);
        assert!(
            stream_paint_due(None, t0, (0, 0), sig),
            "the first token paints"
        );
        assert!(
            !stream_paint_due(Some(t0), t0, sig, sig),
            "another token in the same 16ms does not notify"
        );
        assert!(
            stream_paint_due(Some(t0), t0 + Duration::from_millis(16), sig, sig),
            "text flushes at ~60Hz"
        );
        let card = crate::opengrok::UserFormSpec::parse(
            &serde_json::json!({
                "entryId": "e1",
                "formRequest": {
                    "title": "Login",
                    "fields": [{"id": "p", "label": "Password", "type": "password", "required": true}]
                }
            }),
            None,
        )
        .unwrap();
        let with_form = vec![ChatPart::Text("hi".into()), ChatPart::UserForm(card)];
        let form_sig = stream_part_sig(&with_form);
        assert_ne!(sig, form_sig);
        assert!(
            stream_paint_due(Some(t0), t0, sig, form_sig),
            "a user-form card paints immediately"
        );
    }

    #[test]
    fn replying_to_your_own_message_is_told_apart_from_replying_to_the_bot() {
        let mine = message("m1", true, "remind me at five");
        let reply = replying_to(message("m2", true, "make that six"), &mine);
        let sent = agui_messages(&[mine, reply]);
        assert_eq!(
            sent[1].content,
            "[Replying to their own earlier message: \"remind me at five\"]\n\nmake that six"
        );
        assert!(sent[1].reply_to.as_ref().expect("a quote").is_me);
    }

    /// A reply to a long answer names it, it does not replay it.
    #[test]
    fn a_long_quote_is_clipped() {
        let bot = message("m1", false, &"x".repeat(REPLY_QUOTE_CHARS + 50));
        let reply = replying_to(message("m2", true, "go on"), &bot);
        let sent = agui_messages(&[bot, reply]);
        let quote = sent[1].reply_to.as_ref().expect("a quote");
        assert_eq!(quote.preview.chars().count(), REPLY_QUOTE_CHARS + 1);
        assert!(quote.preview.ends_with('…'));
    }

    /// A message can be deleted after it was answered; the preview saved with the reply is what
    /// is left of it.
    #[test]
    fn a_quote_whose_message_is_gone_falls_back_to_the_saved_preview() {
        let mut reply = message("m2", true, "why?");
        reply.reply_to_id = Some("deleted".to_string());
        reply.reply_preview = Some("The build is green.".to_string());
        let sent = agui_messages(&[reply]);
        assert_eq!(
            sent[0].content,
            "[Replying to your earlier message: \"The build is green.\"]\n\nwhy?"
        );
    }

    /// The app's own words about a turn are not a turn: sending them back would have the model
    /// answering a line nobody said.
    #[test]
    fn the_apps_status_lines_are_never_sent_back_as_the_coworkers_words() {
        let thread = vec![
            message("m1", true, "hi"),
            message("m2", false, EMPTY_TURN_NOTE),
            message("m3", true, "still there?"),
            message("m4", false, "OpenGrok: the model returned no text"),
            message("m5", false, "[took a screenshot of my screen]"),
        ];
        let sent = agui_messages(&thread);
        let kept: Vec<&str> = sent.iter().map(|m| m.content.as_str()).collect();
        assert_eq!(
            kept,
            vec!["hi", "still there?", "[took a screenshot of my screen]"]
        );
    }

    /// A user-form card's typed values — especially a password — must never become the
    /// next turn's AG-UI `content`. The card lives in `parts`; `content` is the prose.
    #[test]
    fn user_form_values_never_enter_agui_content() {
        let spec = crate::opengrok::UserFormSpec::parse(
            &serde_json::json!({
                "entryId": "entry-pw",
                "formRequest": {
                    "title": "Google password",
                    "fields": [{
                        "id": "password",
                        "label": "Password",
                        "type": "password",
                        "required": true,
                        "value": "s3cret-pass"
                    }]
                }
            }),
            None,
        )
        .expect("a card");
        assert!(spec.fields[0].prefill.is_none());
        let mut bot = message("m1", false, "I'll sign you in.");
        bot.parts = vec![ChatPart::UserForm(spec)];
        let sent = agui_messages(&[bot]);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].content, "I'll sign you in.");
        assert!(
            !sent[0].content.contains("s3cret-pass"),
            "password leaked into AguiMessage.content: {}",
            sent[0].content
        );
        let dump = format!("{sent:?}");
        assert!(
            !dump.contains("s3cret-pass"),
            "password in Debug of messages: {dump}"
        );
        let mut values = crate::opengrok::UserFormValues::default();
        values.by_id.insert("password".into(), "s3cret-pass".into());
        let body = crate::opengrok::submit_request_body("e_form", "cw_1", &values, false);
        assert_eq!(body["values"]["password"], "s3cret-pass");
        assert_eq!(body["entryId"], "e_form");
        assert_ne!(body["entryId"], "mock-form-1");
        assert!(
            !sent[0].content.contains("s3cret-pass"),
            "submit values must not leak into AguiMessage.content"
        );
        let official = crate::opengrok::UserFormSpec::from_custom_event(&serde_json::json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "callId": "mock-form-1",
            "reason": "user-form",
            "entryId": "e_form",
            "arguments": {
                "title": "Google password",
                "fields": [{
                    "id": "password",
                    "label": "Password",
                    "type": "password",
                    "required": true
                }]
            }
        }))
        .expect("d12fffc card");
        assert_eq!(official.entry_id, "e_form");
        assert_eq!(official.call_id, "mock-form-1");
        let rest = crate::opengrok::submit_request_body(&official.entry_id, "cw_1", &values, false);
        assert_eq!(rest["entryId"], official.entry_id);
        assert_ne!(rest["entryId"], official.call_id);
        assert_eq!(
            crate::opengrok::user_form_action_from_http(404, &serde_json::Value::Null),
            crate::opengrok::UserFormActionReply::MissingRoute
        );
    }

    #[test]
    fn paint_settle_collapses_the_same_call_id_twin() {
        let call_twin = crate::opengrok::UserFormSpec::from_tool_args(
            &serde_json::json!({
                "formRequest": {
                    "title": "Google account",
                    "fields": [
                        {"id": "email", "label": "Email", "type": "email", "required": true},
                        {"id": "password", "label": "Password", "type": "password", "required": true}
                    ]
                }
            }),
            "call-9",
        )
        .unwrap();
        let entry = crate::opengrok::UserFormSpec::from_custom_event(&serde_json::json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "callId": "call-9",
            "entryId": "e_form",
            "reason": "user-form",
            "arguments": {
                "title": "Google account",
                "fields": [
                    {"id": "email", "label": "Email", "type": "email", "required": true},
                    {"id": "password", "label": "Password", "type": "password", "required": true}
                ]
            }
        }))
        .unwrap();
        let otp = crate::opengrok::UserFormSpec::from_custom_event(&serde_json::json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "callId": "call-otp",
            "entryId": "e_otp",
            "reason": "user-form",
            "arguments": {
                "title": "Enter the code",
                "fields": [{"id": "otp", "label": "Code", "type": "otp", "required": true}]
            }
        }))
        .unwrap();
        let mut bot = message("m1", false, "");
        bot.parts = vec![
            ChatPart::UserForm(call_twin),
            ChatPart::UserForm(entry),
            ChatPart::UserForm(otp),
        ];
        let mut state = AppState::new();
        state.conversations.push(thread("cw_1", vec![bot]));
        state.active_conversation_id = Some("cw_1".into());

        state.paint_user_form_resolution("e_form", FormResolution::Submitted);

        let open = state.open_user_forms();
        assert!(
            open.iter().all(|spec| spec.call_id != "call-9"),
            "call twin must not stay unresolved: {open:?}"
        );
        assert_eq!(open.len(), 1);
        assert_eq!(open[0].entry_id, "e_otp");

        let fresh = crate::opengrok::UserFormSpec::from_tool_args(
            &serde_json::json!({
                "formRequest": {
                    "title": "Google account",
                    "fields": [
                        {"id": "email", "label": "Email", "type": "email", "required": true}
                    ]
                }
            }),
            "call-9",
        )
        .unwrap();
        let grafted = state.graft_user_forms(vec![ChatPart::UserForm(fresh)]);
        match grafted.as_slice() {
            [ChatPart::UserForm(spec)] => {
                assert_eq!(spec.effective_resolution(), Some(FormResolution::Submitted));
                assert!(!spec.is_unresolved());
            }
            other => panic!("expected grafted settle, got {other:?}"),
        }
    }

    #[test]
    fn saved_parts_drop_user_form_cards_and_keep_no_password() {
        let spec = crate::opengrok::UserFormSpec::parse(
            &serde_json::json!({
                "entryId": "entry-pw",
                "formRequest": {
                    "title": "Google password",
                    "fields": [{
                        "id": "password",
                        "label": "Password",
                        "type": "password",
                        "value": "s3cret-pass"
                    }]
                }
            }),
            None,
        )
        .expect("a card");
        let live = vec![
            ChatPart::Text("I'll sign you in.".into()),
            ChatPart::UserForm(spec),
            ChatPart::Text("The page is ready.".into()),
        ];
        let saved = saved_parts(&live);
        let blob = format!("{saved:?}");
        assert!(!blob.contains("s3cret-pass"));
        assert!(
            saved
                .iter()
                .all(|part| matches!(part, crate::services::database::MessagePart::Text(_))),
            "a card is not a saved part: {saved:?}"
        );
    }

    #[test]
    fn saved_parts_drop_save_login() {
        let live = vec![ChatPart::SaveLogin(crate::opengrok::SaveLoginSpec {
            form_entry_id: "e_form".into(),
            origin: "google.com".into(),
            username: "ada@example.com".into(),
        })];
        assert!(saved_parts(&live).is_empty());
    }

    #[test]
    fn graft_injects_save_login_from_local_form_not_from_server() {
        let mut submitted = crate::opengrok::UserFormSpec::parse(
            &serde_json::json!({
                "entryId": "e_form",
                "formRequest": {
                    "title": "Google account",
                    "fields": [
                        {"id": "email", "label": "Email", "type": "email", "required": true},
                        {"id": "password", "label": "Password", "type": "password", "required": true}
                    ]
                }
            }),
            None,
        )
        .unwrap();
        submitted.resolution = Some(FormResolution::Submitted);
        let mut state = AppState::new();
        state.pending_save.insert(
            "e_form".into(),
            PendingSave {
                form_entry_id: "e_form".into(),
                origin: "google.com".into(),
                username: "ada@example.com".into(),
                password: "s3cret-pass".into(),
            },
        );
        let grafted = state.graft_user_forms(vec![ChatPart::UserForm(submitted)]);
        match grafted.as_slice() {
            [ChatPart::UserForm(_), ChatPart::SaveLogin(spec)] => {
                assert_eq!(spec.origin, "google.com");
                assert_eq!(spec.username, "ada@example.com");
                assert_eq!(spec.form_entry_id, "e_form");
            }
            other => panic!("expected submitted form + local save prompt, got {other:?}"),
        }
        let dump = format!("{grafted:?}");
        assert!(!dump.contains("s3cret-pass"));
        assert!(saved_parts(&grafted).is_empty());
    }

    #[test]
    fn graft_drops_server_offer_save_without_local_secret() {
        let state = AppState::new();
        let grafted =
            state.graft_user_forms(vec![ChatPart::SaveLogin(crate::opengrok::SaveLoginSpec {
                form_entry_id: "e_form".into(),
                origin: "google.com".into(),
                username: "ada@example.com".into(),
            })]);
        assert!(
            grafted.is_empty(),
            "never wait for the server to echo a password: {grafted:?}"
        );
    }

    #[test]
    fn overlay_puts_local_save_login_back_on_a_sqlite_row() {
        let mut bot = message("m1", false, "I'll sign you in.");
        bot.parts = vec![ChatPart::Text("I'll sign you in.".into())];
        overlay_server_cards(
            &mut bot,
            &[ChatPart::SaveLogin(crate::opengrok::SaveLoginSpec {
                form_entry_id: "e_form".into(),
                origin: "google.com".into(),
                username: "ada@example.com".into(),
            })],
        );
        match bot.parts.as_slice() {
            [ChatPart::Text(_), ChatPart::SaveLogin(spec)] => {
                assert_eq!(spec.origin, "google.com");
                assert_eq!(spec.username, "ada@example.com");
            }
            other => panic!("expected save prompt overlaid, got {other:?}"),
        }
        assert!(!format!("{:?}", bot.parts).contains("password"));
    }

    #[test]
    fn overlay_settles_user_form_from_send_message_envelope() {
        let settled = crate::opengrok::UserFormSpec::from_custom_event(&serde_json::json!({
            "type": "CUSTOM",
            "name": "user-form",
            "value": {
                "kind": "send-message",
                "id": "e_form",
                "message": {
                    "type": "user-form",
                    "formRequest": {
                        "title": "Sign in",
                        "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
                    }
                },
                "formResolution": "submitted"
            }
        }))
        .unwrap();
        let mut bot = message("m1", false, "I'll sign you in.");
        bot.parts = vec![ChatPart::Text("I'll sign you in.".into())];
        overlay_server_cards(&mut bot, &[ChatPart::UserForm(settled)]);
        match bot.parts.as_slice() {
            [ChatPart::Text(_), ChatPart::UserForm(spec)] => {
                assert_eq!(spec.entry_id, "e_form");
                assert_eq!(spec.effective_resolution(), Some(FormResolution::Submitted));
                assert!(spec.fields.iter().all(|f| f.prefill.is_none()));
            }
            other => panic!("expected settled send-message card, got {other:?}"),
        }
    }

    /// The app's own schema on a database that lives for the length of the test. One connection:
    /// a second connection to `:memory:` would open a second, empty database.
    async fn test_db() -> DatabaseService {
        let options = sqlx::sqlite::SqliteConnectOptions::from_str("sqlite::memory:")
            .expect("an in-memory database")
            .create_if_missing(true)
            .foreign_keys(true);
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .expect("a pool");
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("the app's schema");
        DatabaseService::new(pool)
    }

    /// Bytes stand in for a PNG: nothing in this path decodes one, and distinct bytes prove the
    /// right picture came back under the right caption.
    fn screenshot(call_id: &str, caption: &str, bytes: &[u8], size: (u32, u32)) -> ChatPart {
        ChatPart::Screenshot(crate::opengrok::ScreenshotSpec {
            call_id: call_id.to_string(),
            caption: caption.to_string(),
            image: Arc::new(gpui_kit::Image::from_bytes(
                gpui_kit::ImageFormat::Png,
                bytes.to_vec(),
            )),
            width: size.0,
            height: size.1,
            visibility: None,
        })
    }

    /// Order, kind and contents in one line per part. `ChatPart`'s own `PartialEq` takes one
    /// screenshot per call id on trust and never looks at the bytes, so the bytes are spelled
    /// out here instead.
    fn shape(parts: &[ChatPart]) -> Vec<String> {
        parts
            .iter()
            .map(|part| match part {
                ChatPart::Text(text) => format!("text {text}"),
                ChatPart::Screenshot(spec) => format!(
                    "shot {} {}x{} {:?} {}",
                    spec.call_id, spec.width, spec.height, spec.image.bytes, spec.caption
                ),
                ChatPart::Ui(_) => "ui".to_string(),
                ChatPart::Approval(_) => "approval".to_string(),
                ChatPart::UserForm(spec) => format!(
                    "user-form {} {} {}",
                    spec.entry_id,
                    spec.effective_resolution()
                        .map(|r| r.as_str())
                        .unwrap_or("idle"),
                    spec.computer_handoff
                        .map(|s| s.as_str())
                        .unwrap_or("no-computer")
                ),
                ChatPart::SaveLogin(spec) => {
                    format!("save-login {} {}", spec.origin, spec.username)
                }
            })
            .collect()
    }

    /// The bug the person reported: a recipe run is several bubbles with pictures of the box's
    /// screen between them, and switching to another bot and back brought it all back as one
    /// paragraph with every picture gone.
    #[tokio::test]
    async fn a_turn_of_words_and_pictures_comes_back_the_way_it_was_seen() {
        let db = test_db().await;
        db.ensure_session("s1", "Recipes").await.expect("a session");
        let live = vec![
            ChatPart::Text("I'll open YouTube on my box using the taught recipe.".to_string()),
            screenshot(
                "c1",
                "ran recipe \"youtube\" (v2): 6 steps; screenshot of the screen afterwards attached",
                b"png-one",
                (1280, 800),
            ),
            ChatPart::Text("YouTube is open on my box (not your Mac).".to_string()),
            screenshot(
                "c2",
                "clicking at 175,705; screenshot of the 1280x800 screen attached",
                b"png-two",
                (640, 480),
            ),
        ];
        let content = "I'll open YouTube on my box using the taught recipe.\n\nYouTube is open on my box (not your Mac).";

        // Pinned feed shots persist. `agent` computer-step PNGs never become ChatPart.
        assert!(
            saved_parts(&live).iter().any(
                |part| matches!(part, MessagePart::Screenshot { call_id, .. } if call_id == "c1")
            ),
            "pinned shots belong in sqlite: {:?}",
            saved_parts(&live)
        );
        db.save_message(
            "m1",
            "s1",
            "assistant",
            content,
            None,
            None,
            None,
            &saved_parts(&live),
            Some("run_1"),
            false,
            SystemTime::UNIX_EPOCH,
        )
        .await
        .expect("the turn is saved");

        let rows = db.get_messages("s1").await.expect("the thread reopens");
        assert_eq!(rows.len(), 1);
        let restored = restored_parts(&rows[0].content, rows[0].parts.clone());
        assert_eq!(
            shape(&restored),
            shape(&live),
            "sqlite keeps the words and the pinned pictures"
        );
        assert_eq!(
            rows[0].run_id.as_deref(),
            Some("run_1"),
            "the row says which run it came out of, which is how reconciling against the server \
             knows it already has this turn"
        );
    }

    #[test]
    fn overlay_puts_submitted_user_form_back_on_a_sqlite_row() {
        let submitted = crate::opengrok::UserFormSpec::from_custom_event(&serde_json::json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "callId": "call-9",
            "entryId": "e_form",
            "reason": "user-form",
            "arguments": {
                "title": "Google account",
                "formResolution": "submitted",
                "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
            }
        }))
        .unwrap();
        let mut submitted = submitted;
        submitted.resolution = Some(FormResolution::Submitted);
        let mut bot = message("m1", false, "I'll sign you in.");
        bot.run_id = Some("run_1".into());
        bot.parts = vec![ChatPart::Text("I'll sign you in.".into())];
        overlay_server_cards(
            &mut bot,
            &[
                ChatPart::Text("I'll sign you in.".into()),
                ChatPart::UserForm(submitted.clone()),
            ],
        );
        match bot.parts.as_slice() {
            [ChatPart::Text(_), ChatPart::UserForm(spec)] => {
                assert_eq!(spec.entry_id, "e_form");
                assert_eq!(spec.effective_resolution(), Some(FormResolution::Submitted));
                assert!(spec.fields.iter().all(|f| f.prefill.is_none()));
            }
            other => panic!("expected text + Submitted card, got {other:?}"),
        }
    }

    #[test]
    fn overlay_mounts_the_open_form_above_screenshots_not_at_the_bottom() {
        let open = crate::opengrok::UserFormSpec::parse(
            &serde_json::json!({
                "entryId": "e_form",
                "formRequest": {
                    "title": "Form Label",
                    "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
                }
            }),
            None,
        )
        .unwrap();
        let mut bot = message("m1", false, "I'll raise a single in-chat form…");
        bot.parts = vec![
            ChatPart::Text("I'll raise a single in-chat form…".into()),
            screenshot("obs-1", "observe", b"png-1", (1280, 800)),
        ];
        overlay_server_cards(
            &mut bot,
            &[
                ChatPart::Text("I'll raise a single in-chat form…".into()),
                ChatPart::UserForm(open.clone()),
                screenshot("obs-1", "observe", b"png-1", (1280, 800)),
            ],
        );
        match bot.parts.as_slice() {
            [
                ChatPart::Text(_),
                ChatPart::UserForm(spec),
                ChatPart::Screenshot(_),
            ] => {
                assert_eq!(spec.title, "Form Label");
                assert!(spec.is_unresolved());
            }
            other => panic!("open form must not sit under the screenshot block: {other:?}"),
        }
        let mut dismissed = open;
        dismissed.resolution = Some(FormResolution::Dismissed);
        overlay_server_cards(&mut bot, &[ChatPart::UserForm(dismissed)]);
        match bot.parts.as_slice() {
            [
                ChatPart::Text(_),
                ChatPart::UserForm(spec),
                ChatPart::Screenshot(_),
            ] => {
                assert_eq!(spec.effective_resolution(), Some(FormResolution::Dismissed));
            }
            other => panic!("Dismissed must stay in the same slot, got {other:?}"),
        }
    }

    #[test]
    fn open_handoff_does_not_post_form_entry_id_when_sibling_id_is_missing() {
        let mut spec = crate::opengrok::UserFormSpec::parse(
            &serde_json::json!({
                "entryId": "e_form",
                "formRequest": {
                    "title": "Form Label",
                    "instruction": "Sign in on the computer.",
                    "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
                }
            }),
            None,
        )
        .unwrap();
        spec.computer_handoff = Some(crate::opengrok::ComputerHandoffStatus::ActionNeeded);
        assert!(spec.shows_form_chrome());
        assert!(spec.shows_computer_handoff());
        assert!(spec.handoff_entry_id.is_none());
        let mut bot = message("m1", false, "Open the screen");
        bot.parts = vec![ChatPart::UserForm(spec.clone())];
        let mut state = AppState::new();
        state.conversations.push(thread("cw_1", vec![bot]));
        assert_eq!(
            state.box_handoff_post_id("e_form", "e_form"),
            None,
            "Skip / I'm done must wait for handoffEntryId — never POST the form id"
        );
        spec.handoff_entry_id = Some("e_hand".into());
        state.conversations[0].messages[0].parts = vec![ChatPart::UserForm(spec)];
        assert_eq!(
            state.box_handoff_post_id("e_form", "e_form").as_deref(),
            Some("e_hand")
        );
        state
            .user_form_handoffs
            .insert("e_form".into(), "e_form".into());
        assert_eq!(
            state.box_handoff_post_id("e_form", "e_form"),
            None,
            "a stored form id is not a sibling"
        );
    }

    #[test]
    fn computer_window_attention_is_a_copy_of_the_open_handoff() {
        let mut spec = crate::opengrok::UserFormSpec::parse(
            &serde_json::json!({
                "entryId": "e_form",
                "formRequest": {
                    "title": "Form Label",
                    "instruction": "Sign in on the computer.",
                    "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
                }
            }),
            None,
        )
        .unwrap();
        spec.computer_handoff = Some(crate::opengrok::ComputerHandoffStatus::ActionNeeded);
        let mut bot = message("m1", false, "Open the screen");
        bot.parts = vec![ChatPart::UserForm(spec)];
        let mut state = AppState::new();
        state.conversations.push(thread("cw_1", vec![bot]));
        state.active_conversation_id = Some("cw_1".into());
        assert_eq!(
            state.computer_window_attention(),
            Some(("e_form".into(), "Sign in on the computer.".into())),
            "Take over copies this into ComputerScreen so first draw never reads AppState"
        );
        state.user_form_handoff_done.insert("e_form".into());
        if let ChatPart::UserForm(spec) = &mut state.conversations[0].messages[0].parts[0] {
            spec.computer_handoff = Some(crate::opengrok::ComputerHandoffStatus::Done);
        }
        assert_eq!(
            state.computer_window_attention(),
            None,
            "Done / Skipped drops the strip"
        );
        assert!(
            state.visible_computer_handoffs().iter().any(|spec| {
                spec.computer_handoff == Some(crate::opengrok::ComputerHandoffStatus::Done)
            }),
            "Computer card remains as Done history"
        );
    }

    #[test]
    fn form_computer_lifecycle_keeps_both_cards_and_document_order() {
        let idle = crate::opengrok::UserFormSpec::parse(
            &serde_json::json!({
                "entryId": "e_form",
                "formRequest": {
                    "title": "Form Label",
                    "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
                }
            }),
            None,
        )
        .unwrap();
        let mut bot = message("m1", false, "I'll raise a single in-chat form…");
        bot.parts = vec![
            ChatPart::Text("I'll raise a single in-chat form…".into()),
            ChatPart::UserForm(idle.clone()),
            screenshot("obs-1", "observe", b"png-1", (1280, 800)),
        ];
        let mut state = AppState::new();
        state.conversations.push(thread("cw_1", vec![bot]));
        state.active_conversation_id = Some("cw_1".into());
        let before = shape(&state.conversations[0].messages[0].parts);

        state.set_computer_handoff(
            "e_form",
            crate::opengrok::ComputerHandoffStatus::ActionNeeded,
        );
        {
            let spec = match &state.conversations[0].messages[0].parts[1] {
                ChatPart::UserForm(spec) => spec,
                other => panic!("form must stay in slot 1, got {other:?}"),
            };
            assert!(spec.is_unresolved());
            assert!(spec.shows_form_chrome());
            assert_eq!(
                spec.computer_handoff,
                Some(crate::opengrok::ComputerHandoffStatus::ActionNeeded)
            );
            assert_ne!(spec.pill(), Some("On the computer"));
        }
        let after_open = shape(&state.conversations[0].messages[0].parts);
        assert_eq!(
            before
                .iter()
                .map(|s| s.split_whitespace().next())
                .collect::<Vec<_>>(),
            after_open
                .iter()
                .map(|s| s.split_whitespace().next())
                .collect::<Vec<_>>(),
            "Open the screen must not reorder parts: {before:?} vs {after_open:?}"
        );

        state.paint_user_form_resolution("e_form", FormResolution::Dismissed);
        state.set_computer_handoff("e_form", crate::opengrok::ComputerHandoffStatus::Done);
        {
            let spec = match &state.conversations[0].messages[0].parts[1] {
                ChatPart::UserForm(spec) => spec,
                other => panic!("I'm done must not drop the form, got {other:?}"),
            };
            assert_eq!(spec.effective_resolution(), Some(FormResolution::Dismissed));
            assert_eq!(spec.pill(), Some("Dismissed"));
            assert_eq!(
                spec.computer_handoff,
                Some(crate::opengrok::ComputerHandoffStatus::Done)
            );
        }

        state.paint_user_form_resolution("e_form", FormResolution::Skipped);
        state.set_computer_handoff("e_form", crate::opengrok::ComputerHandoffStatus::Skipped);
        {
            let spec = match &state.conversations[0].messages[0].parts[1] {
                ChatPart::UserForm(spec) => spec,
                other => panic!("Skip must not drop the form, got {other:?}"),
            };
            assert_eq!(spec.effective_resolution(), Some(FormResolution::Skipped));
            assert_eq!(
                spec.computer_handoff,
                Some(crate::opengrok::ComputerHandoffStatus::Skipped)
            );
        }

        let remounted = crate::opengrok::UserFormSpec::parse(
            &serde_json::json!({
                "entryId": "e_form",
                "formResolution": "escalated",
                "formRequest": {
                    "title": "Form Label",
                    "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
                }
            }),
            None,
        )
        .unwrap();
        let grafted = state.graft_user_forms(vec![
            ChatPart::Text("I'll raise a single in-chat form…".into()),
            ChatPart::UserForm(remounted),
            screenshot("obs-1", "observe", b"png-1", (1280, 800)),
        ]);
        match grafted.as_slice() {
            [
                ChatPart::Text(_),
                ChatPart::UserForm(spec),
                ChatPart::Screenshot(_),
            ] => {
                assert_eq!(spec.effective_resolution(), Some(FormResolution::Skipped));
                assert_eq!(
                    spec.computer_handoff,
                    Some(crate::opengrok::ComputerHandoffStatus::Skipped)
                );
                assert_ne!(spec.pill(), Some("On the computer"));
            }
            other => panic!("hide→reshow must keep form+computer in order, got {other:?}"),
        }
    }

    /// A row written before pieces were kept has none of them — which is also every row the
    /// build in the person's hands is writing right now. It must still open, as the one bubble
    /// its words always were.
    /// A run's reply lands in that run's own bubble, or in one made for it.
    ///
    /// It used to fall back to the last thing the coworker said, which was harmless only while
    /// a reply was written down under a fresh name every time. Now that a reply is written down
    /// under its bubble's own name, that fallback would put this run's words on another run's
    /// row and take that row's pictures with it.
    #[test]
    fn a_run_writes_into_its_own_bubble_or_one_made_for_it() {
        let mut messages = vec![
            at(message("m_ask", true, "do it"), 500),
            from_run("m_old", "an older reply", "run_old", 1_000),
        ];

        // The turn named its bubble when it started.
        let named = from_run("m_live", "", "run_1", 2_000);
        messages.push(named);
        assert_eq!(
            bubble_for_run(&mut messages, Some("m_live"), "run_1", None),
            2,
            "the bubble the turn named"
        );

        // No name, but the thread already carries a bubble for the run.
        assert_eq!(
            bubble_for_run(&mut messages, None, "run_1", None),
            2,
            "found by the run it carries"
        );

        // Neither: a bubble is made, and the older reply is left alone.
        let before = messages.len();
        let made = bubble_for_run(
            &mut messages,
            None,
            "run_new",
            Some(SystemTime::UNIX_EPOCH + Duration::from_millis(1_500)),
        );
        assert_eq!(made, before, "appended rather than borrowed");
        assert_eq!(messages.len(), before + 1);
        assert_eq!(messages[made].run_id.as_deref(), Some("run_new"));
        assert_eq!(
            messages[made].sent_at,
            SystemTime::UNIX_EPOCH + Duration::from_millis(1_500),
            "stamped with when the run began, not when it was noticed"
        );
        assert_eq!(
            messages[1].content, "an older reply",
            "the last thing said before is untouched"
        );
    }

    /// A turn hidden on another machine goes out of sight here, and is never fetched back.
    #[test]
    fn a_turn_hidden_somewhere_else_is_put_out_of_sight_here() {
        let mut messages = vec![
            at(message("m_ask", true, "do it"), 500),
            from_run("m_1", "done", "run_1", 1_000),
            from_run("m_2", "done again", "run_2", 2_000),
        ];
        let hidden = vec!["run_1".to_string()];

        let newly = hide_messages_of_runs(&mut messages, &hidden);
        assert_eq!(
            newly,
            vec!["m_1".to_string()],
            "only the turn that was hidden"
        );
        assert!(messages[1].hidden);
        assert!(!messages[2].hidden, "the other turn is untouched");

        // Asking again changes nothing, so a thread opened every day writes nothing every day.
        assert!(hide_messages_of_runs(&mut messages, &hidden).is_empty());

        // And a hidden message still names its run, so the thread never fetches it back.
        let runs = vec![
            thread_run("run_1", "finished", 1_000, &turn_frames()),
            thread_run("run_2", "finished", 2_000, &turn_frames()),
        ];
        assert!(missing_replies(&messages, &runs).is_empty());
    }

    /// A delete the server never heard is asked again, rather than lost.
    ///
    /// Telling the server can fail — a bad connection, an app closed a moment later — and the
    /// message is already hidden here, so there is no second delete to try. This is what the
    /// failure looks like from the next visit: the server still offering a turn this machine
    /// has put out of sight.
    #[test]
    fn a_turn_hidden_here_that_the_server_still_offers_is_asked_for_again() {
        let mut messages = vec![
            at(message("m_ask", true, "do it"), 500),
            from_run("m_1", "done", "run_1", 1_000),
            from_run("m_2", "done again", "run_2", 2_000),
        ];
        let runs = vec![
            thread_run("run_1", "finished", 1_000, &turn_frames()),
            thread_run("run_2", "finished", 2_000, &turn_frames()),
        ];
        assert!(
            unheard_hidden_runs(&messages, &runs).is_empty(),
            "nothing is hidden yet, so there is nothing to tell"
        );

        messages[1].hidden = true;
        assert_eq!(
            unheard_hidden_runs(&messages, &runs),
            vec!["run_1".to_string()],
            "the server is still offering a turn this machine hid"
        );

        // Once the server withholds it, there is nothing left to say.
        let withheld = vec![thread_run("run_2", "finished", 2_000, &turn_frames())];
        assert!(unheard_hidden_runs(&messages, &withheld).is_empty());
    }

    /// A run that does not say when it began is placed now, not at the start of time.
    ///
    /// The reply is placed by its run's start, and an older server — or a field left out —
    /// reads as zero. Taken at face value that is 1970, which would pin the reply above the
    /// message it answers, on screen and then on disk, for good.
    #[test]
    fn a_run_that_never_said_when_it_began_is_not_pinned_to_the_start_of_time() {
        let runs = vec![thread_run("run_1", "finished", 0, &turn_frames())];
        let messages = vec![at(message("m_ask", true, "do it"), 1_000)];
        let recovered = missing_replies(&messages, &runs);
        assert_eq!(recovered.len(), 1);
        assert!(
            recovered[0].started_at > SystemTime::UNIX_EPOCH + Duration::from_secs(1),
            "not the epoch"
        );

        let mut thread = messages.clone();
        graft_reply(&mut thread, &recovered[0]);
        assert_eq!(
            ids(&thread)[0],
            "m_ask",
            "the reply lands after the message it answers, not above it"
        );
    }

    /// A reply recovered from the server keeps the place it had when the thread is reopened.
    ///
    /// The recovered reply is put back where it happened in memory, but it used to be written
    /// down stamped with the moment it was recovered — so the next load, which reads the thread
    /// in stamped order, dropped it to the bottom, below messages it came before.
    #[tokio::test]
    async fn a_recovered_reply_is_still_where_it_happened_after_the_thread_is_reopened() {
        let db = test_db().await;
        db.ensure_session("s1", "Ada").await.expect("a session");
        let ask = SystemTime::UNIX_EPOCH + Duration::from_millis(1_000);
        let reply = SystemTime::UNIX_EPOCH + Duration::from_millis(2_000);
        let next = SystemTime::UNIX_EPOCH + Duration::from_millis(3_000);

        // Written down out of order: the person's two messages first, then the reply that
        // belongs between them, recovered from the server afterwards.
        for (id, role, content, at) in [
            ("m_ask", "user", "do it", ask),
            ("m_next", "user", "and then?", next),
            ("m_reply", "assistant", "done", reply),
        ] {
            db.save_message(
                id,
                "s1",
                role,
                content,
                None,
                None,
                None,
                &[],
                None,
                false,
                at,
            )
            .await
            .expect("saved");
        }

        let rows = db.get_messages("s1").await.expect("the thread reopens");
        assert_eq!(
            ids_of(&rows),
            vec!["m_ask", "m_reply", "m_next"],
            "the thread reads back in the order it was said in"
        );
        assert_eq!(
            restored_message(rows[1].clone()).sent_at,
            reply,
            "and the reply keeps its own time"
        );
    }

    /// A second write of a message leaves the time it was said alone.
    ///
    /// Two paths can settle one run, and the later one must not move the message down the
    /// thread: it is the same message, said when it was said.
    #[tokio::test]
    async fn a_second_write_does_not_move_when_a_message_was_said() {
        let db = test_db().await;
        db.ensure_session("s1", "Ada").await.expect("a session");
        let said = SystemTime::UNIX_EPOCH + Duration::from_millis(1_000);
        let noticed = SystemTime::UNIX_EPOCH + Duration::from_millis(9_000);
        for at in [said, noticed] {
            db.save_message(
                "bubble_1",
                "s1",
                "assistant",
                "done",
                None,
                None,
                None,
                &[],
                Some("run_1"),
                false,
                at,
            )
            .await
            .expect("saved");
        }
        let rows = db.get_messages("s1").await.expect("the thread reopens");
        assert_eq!(rows.len(), 1);
        assert_eq!(
            restored_message(rows[0].clone()).sent_at,
            said,
            "the first word on when it was said is the last"
        );
    }

    /// Two messages inside one second keep their order.
    #[tokio::test]
    async fn two_messages_in_the_same_second_still_read_in_order() {
        let db = test_db().await;
        db.ensure_session("s1", "Ada").await.expect("a session");
        let ask = SystemTime::UNIX_EPOCH + Duration::from_millis(4_100);
        let reply = SystemTime::UNIX_EPOCH + Duration::from_millis(4_350);
        for (id, role, at) in [("m_reply", "assistant", reply), ("m_ask", "user", ask)] {
            db.save_message(id, "s1", role, "x", None, None, None, &[], None, false, at)
                .await
                .expect("saved");
        }
        let rows = db.get_messages("s1").await.expect("the thread reopens");
        assert_eq!(
            ids_of(&rows),
            vec!["m_ask", "m_reply"],
            "a quarter of a second apart is enough to tell them apart"
        );
    }

    /// Times written before they were kept to the millisecond still read.
    #[test]
    fn a_whole_second_stamp_still_reads_beside_a_finer_one() {
        let second = parse_sql_time("2026-09-21 22:30:01").expect("the old shape reads");
        let finer = parse_sql_time("2026-09-21 22:30:01.250").expect("the new shape reads");
        assert!(finer > second, "and sorts after it inside the same second");
        assert!(parse_sql_time("not a time").is_none());
    }

    /// A reply hidden before it was ever written down stays hidden.
    ///
    /// A reply has no row until its turn settles, so hiding one that is still being typed out
    /// had nothing to mark: the write that came afterwards put it on disk unmarked, and the
    /// next time the thread was opened it was back.
    #[tokio::test]
    async fn a_reply_hidden_before_it_was_written_down_is_written_down_hidden() {
        let db = test_db().await;
        db.ensure_session("s1", "Ada").await.expect("a session");

        // The person hides the bubble while the turn is still going: no row to mark yet.
        db.hide_message("bubble_1").await.expect("nothing to hide");
        assert!(
            db.get_messages("s1").await.expect("empty").is_empty(),
            "there is no row yet"
        );

        // The turn settles and the reply is written down, carrying the mark.
        db.save_message(
            "bubble_1",
            "s1",
            "assistant",
            "The build is green.",
            None,
            None,
            None,
            &[],
            Some("run_1"),
            true,
            SystemTime::UNIX_EPOCH,
        )
        .await
        .expect("the reply is saved");
        let rows = db.get_messages("s1").await.expect("the thread reopens");
        assert_eq!(rows.len(), 1);
        assert!(rows[0].deleted_at.is_some(), "it stays hidden");

        // A later write of the same reply does not bring it back.
        db.save_message(
            "bubble_1",
            "s1",
            "assistant",
            "The build is green, again.",
            None,
            None,
            None,
            &[],
            Some("run_1"),
            false,
            SystemTime::UNIX_EPOCH,
        )
        .await
        .expect("the second write");
        let rows = db.get_messages("s1").await.expect("the thread reopens");
        assert!(
            rows[0].deleted_at.is_some(),
            "a write never clears what the person hid"
        );
    }

    /// What the person hid stays in the thread, unpainted, so the thread keeps its memory.
    ///
    /// The row is the only thing that names the run it came out of. Take it away and the next
    /// reconcile decides the server knows a turn this thread does not, and grafts it back on —
    /// which is the person watching what they deleted return.
    #[test]
    fn a_hidden_reply_still_names_its_run_so_the_thread_does_not_fetch_it_back() {
        let runs = vec![
            thread_run("run_1", "finished", 1_000, &turn_frames()),
            thread_run("run_2", "finished", 2_000, &turn_frames()),
        ];
        let mut messages = vec![
            at(message("m_ask", true, "do it"), 500),
            from_run("m_1", "done", "run_1", 1_000),
            from_run("m_2", "done again", "run_2", 2_000),
        ];
        assert!(
            missing_replies(&messages, &runs).is_empty(),
            "a thread that can name both runs asks for nothing"
        );

        // The person hides both replies. The rows stay, so the thread still names both runs.
        for message in messages.iter_mut().filter(|m| !m.is_me) {
            message.hidden = true;
        }
        assert!(
            missing_replies(&messages, &runs).is_empty(),
            "hidden is still known: {:?}",
            missing_replies(&messages, &runs)
                .iter()
                .map(|reply| reply.run_id.clone())
                .collect::<Vec<_>>()
        );

        // Taking the rows away is what used to bring the turns back.
        messages.retain(|m| m.is_me);
        assert_eq!(
            missing_replies(&messages, &runs)
                .iter()
                .map(|reply| reply.run_id.clone())
                .collect::<Vec<_>>(),
            vec!["run_1", "run_2"],
            "a thread that can name nothing takes the lot back"
        );
    }

    /// A bubble and its row answer to one name, so the delete finds it.
    ///
    /// The row used to be filed under an id the database minted, which nothing on screen had:
    /// deleting a reply ran a statement that matched nothing, and the next time the thread was
    /// opened the message came straight back off disk.
    #[tokio::test]
    async fn a_reply_is_deleted_by_the_name_its_bubble_already_has() {
        let db = test_db().await;
        db.ensure_session("s1", "Ada").await.expect("a session");
        db.save_message(
            "bubble_1",
            "s1",
            "assistant",
            "The build is green.",
            None,
            None,
            None,
            &[],
            Some("run_1"),
            false,
            SystemTime::UNIX_EPOCH,
        )
        .await
        .expect("the reply is saved");

        let rows = db.get_messages("s1").await.expect("the thread reopens");
        assert_eq!(ids_of(&rows), vec!["bubble_1"], "the row is the bubble's");

        db.hide_message("bubble_1").await.expect("hidden");
        let rows = db.get_messages("s1").await.expect("the thread reopens");
        assert_eq!(rows.len(), 1, "the row stays, so the run stays named");
        assert!(
            rows[0].deleted_at.is_some(),
            "and it comes back marked, so nothing paints it"
        );
    }

    /// Two paths can settle one run — the live stream and the watcher that outlives it. Both
    /// write the same bubble, so the thread says it once.
    #[tokio::test]
    async fn a_turn_written_down_twice_is_still_one_row() {
        let db = test_db().await;
        db.ensure_session("s1", "Ada").await.expect("a session");
        let once = vec![MessagePart::Screenshot {
            call_id: "c1".to_string(),
            image: vec![1, 2, 3],
            width: 2,
            height: 1,
            caption: String::new(),
        }];
        db.save_message(
            "bubble_1",
            "s1",
            "assistant",
            "Working",
            None,
            None,
            None,
            &once,
            Some("run_1"),
            false,
            SystemTime::UNIX_EPOCH,
        )
        .await
        .expect("the first write");
        db.save_message(
            "bubble_1",
            "s1",
            "assistant",
            "The build is green.",
            None,
            None,
            None,
            &[],
            Some("run_1"),
            false,
            SystemTime::UNIX_EPOCH,
        )
        .await
        .expect("the second write");

        let rows = db.get_messages("s1").await.expect("the thread reopens");
        assert_eq!(rows.len(), 1, "one row, not two: {:?}", ids_of(&rows));
        assert_eq!(
            rows[0].content, "The build is green.",
            "the later ending is the one that stands"
        );
        assert!(
            rows[0].parts.is_empty(),
            "the first write's pieces went with it: {:?}",
            rows[0].parts
        );
    }

    fn ids_of(rows: &[crate::services::database::ChatMessage]) -> Vec<String> {
        rows.iter().map(|row| row.id.clone()).collect()
    }

    #[tokio::test]
    async fn a_row_with_only_words_still_loads_as_a_single_bubble() {
        let db = test_db().await;
        db.ensure_session("s1", "Old").await.expect("a session");
        db.save_message(
            "m1",
            "s1",
            "assistant",
            "The build is green.",
            None,
            None,
            None,
            &[],
            None,
            false,
            SystemTime::UNIX_EPOCH,
        )
        .await
        .expect("the message is saved");

        let rows = db.get_messages("s1").await.expect("the thread reopens");
        assert!(rows[0].parts.is_empty(), "no pieces were written");
        assert_eq!(
            shape(&restored_parts(&rows[0].content, rows[0].parts.clone())),
            vec!["text The build is green.".to_string()]
        );
    }

    /// Words alone are already in `content`. Writing them a second time would double every
    /// thread on disk for nothing.
    #[test]
    fn a_turn_that_was_only_words_saves_no_pieces() {
        let live = vec![
            ChatPart::Text("The build ".to_string()),
            ChatPart::Text("is green.".to_string()),
        ];
        assert!(saved_parts(&live).is_empty());
    }

    /// All of these are the app talking, whether painted now or read back from an older build's
    /// rows.
    #[test]
    fn a_status_line_is_told_from_something_the_coworker_said() {
        assert!(is_status_line(EMPTY_TURN_NOTE));
        assert!(is_status_line(STOPPED_TURN_NOTE));
        assert!(is_status_line(STOP_UNSENT_NOTE));
        assert!(is_status_line("OpenGrok: the run failed"));
        assert!(!is_status_line("OpenGrok is a server."));
        assert!(!is_status_line("[took a screenshot of my screen]"));
        assert!(is_tool_standin("[took a screenshot of my screen]"));
        assert!(!is_tool_standin("The build is green."));
    }

    // The patch reconciliation, apart from the app: what the roster holds for a coworker once
    // the server has answered.
    use super::{Coworker, CoworkerPatch, apply_patch, settle_patch};

    fn bob() -> Coworker {
        Coworker {
            id: "cw_1".to_string(),
            name: "Bob".to_string(),
            model: "xai/grok-4.6@sub".to_string(),
            role: Some("Research".to_string()),
            title: Some("Analyst".to_string()),
            avatar_shape: Some("circle".to_string()),
            avatar_color: Some("blue".to_string()),
            notify_on_updates: Some(false),
            updated_at_ms: 17,
            hidden_from_sidebar: false,
            box_id: None,
        }
    }

    /// The roster took the new name before the server was asked. The server answered without a
    /// word about the name — which is what a route that quietly drops the field does — so the
    /// new name was never stored, and the roster must not go on showing it.
    #[test]
    fn a_rename_the_server_never_echoed_does_not_stay_on_the_roster() {
        let before = bob();
        let patch = CoworkerPatch {
            name: Some("Roberta".to_string()),
            title: Some("Analyst".to_string()),
            role: Some("Research".to_string()),
            ..Default::default()
        };
        let mut roster = before.clone();
        apply_patch(&mut roster, &patch);
        assert_eq!(roster.name, "Roberta", "the click is answered at once");

        let echo = Coworker {
            name: String::new(),
            ..before.clone()
        };
        settle_patch(&mut roster, &patch, Some(&echo), &before);
        assert_eq!(
            roster.name, "Bob",
            "an answer that says nothing about the name leaves the stored name standing"
        );
    }

    /// The same, for a server that answers with the name it kept rather than with no name at
    /// all: the echo is the truth even when it is the old value.
    #[test]
    fn the_servers_echo_wins_over_what_the_app_sent() {
        let before = bob();
        let patch = CoworkerPatch {
            name: Some("Roberta".to_string()),
            ..Default::default()
        };
        let mut roster = before.clone();
        apply_patch(&mut roster, &patch);
        settle_patch(&mut roster, &patch, Some(&before), &before);
        assert_eq!(roster.name, "Bob");
    }

    /// Nothing was stored, so nothing the patch asked for may be left behind — including the
    /// fields the app had already written into the roster to look quick.
    #[test]
    fn a_refused_patch_leaves_the_roster_as_it_was() {
        let before = bob();
        let patch = CoworkerPatch {
            name: Some("Roberta".to_string()),
            title: Some(String::new()),
            role: Some("Marketing".to_string()),
            notify_on_updates: Some(true),
            ..Default::default()
        };
        let mut roster = before.clone();
        apply_patch(&mut roster, &patch);
        assert_eq!(roster.title, None, "the click is answered at once");

        settle_patch(&mut roster, &patch, None, &before);
        assert_eq!(roster, before);
    }

    /// A field the patch never mentioned is none of the reconciliation's business, which is
    /// what keeps a server that answers with half a coworker from blanking the other half.
    #[test]
    fn a_field_outside_the_patch_is_left_alone() {
        let before = bob();
        let patch = CoworkerPatch {
            model: Some("xai/grok-4.7@sub".to_string()),
            ..Default::default()
        };
        let mut roster = before.clone();
        apply_patch(&mut roster, &patch);
        let echo = Coworker {
            id: "cw_1".to_string(),
            name: String::new(),
            model: "xai/grok-4.7@sub".to_string(),
            role: None,
            title: None,
            avatar_shape: None,
            avatar_color: None,
            notify_on_updates: None,
            updated_at_ms: 0,
            hidden_from_sidebar: false,
            box_id: None,
        };
        settle_patch(&mut roster, &patch, Some(&echo), &before);
        assert_eq!(roster.model, "xai/grok-4.7@sub");
        assert_eq!(roster.name, "Bob", "a partial answer blanks nothing else");
        assert_eq!(roster.title.as_deref(), Some("Analyst"));
        assert_eq!(roster.updated_at_ms, 17);
    }

    /// Clearing is the one thing an answer with nothing in it confirms: the avatar the person
    /// reset stays reset rather than coming back on the next frame.
    #[test]
    fn a_cleared_field_stays_cleared_when_the_server_echoes_nothing() {
        let before = bob();
        let patch = CoworkerPatch {
            avatar_shape: Some(String::new()),
            avatar_color: Some(String::new()),
            ..Default::default()
        };
        let mut roster = before.clone();
        apply_patch(&mut roster, &patch);
        let echo = Coworker {
            avatar_shape: None,
            avatar_color: None,
            ..before.clone()
        };
        settle_patch(&mut roster, &patch, Some(&echo), &before);
        assert_eq!(roster.avatar_shape, None);
        assert_eq!(roster.avatar_color, None);
    }

    // ---- A turn survives looking away -------------------------------------------------------

    fn at(mut message: Message, at_ms: u64) -> Message {
        message.sent_at = SystemTime::UNIX_EPOCH + Duration::from_millis(at_ms);
        message
    }

    /// A reply already on disk, which is to say a run this thread can account for.
    fn from_run(id: &str, content: &str, run_id: &str, at_ms: u64) -> Message {
        let mut message = at(message(id, false, content), at_ms);
        message.run_id = Some(run_id.to_string());
        message
    }

    fn thread(id: &str, messages: Vec<Message>) -> Conversation {
        Conversation {
            id: id.to_string(),
            title: id.to_string(),
            created_at: String::new(),
            updated_at: String::new(),
            messages,
            unread_count: 0,
        }
    }

    fn ids(messages: &[Message]) -> Vec<&str> {
        messages.iter().map(|message| message.id.as_str()).collect()
    }

    /// A row as the local database hands it back.
    fn row(id: &str, role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            id: id.to_string(),
            session_id: "cw_1".to_string(),
            role: role.to_string(),
            content: content.to_string(),
            created_at: "2026-09-17 10:00:00".to_string(),
            model: None,
            provider: None,
            reply_to_id: None,
            reply_preview: None,
            reply_is_me: None,
            run_id: None,
            deleted_at: None,
            parts: Vec::new(),
        }
    }

    fn in_flight() -> LiveTurn {
        LiveTurn {
            run_id: "run_1".to_string(),
            message_id: "m_live".to_string(),
            persisting: false,
        }
    }

    /// One turn's frames as the server kept them: some words, a tool that took a picture of the
    /// box's screen, more words. The picture arrives base64 inside the frame, because that is
    /// how a frame carries one, and decoding it is part of what the two paths have to agree on.
    fn turn_frames() -> Vec<serde_json::Value> {
        use base64::Engine as _;
        let png = base64::engine::general_purpose::STANDARD.encode(b"png-one");
        vec![
            serde_json::json!({ "type": "RUN_STARTED", "threadId": "cw_1", "runId": "run_1" }),
            serde_json::json!({
                "type": "TEXT_MESSAGE_CONTENT", "messageId": "m1", "delta": "Opening YouTube "
            }),
            serde_json::json!({
                "type": "TEXT_MESSAGE_CONTENT", "messageId": "m1", "delta": "on my box."
            }),
            serde_json::json!({
                "type": "TOOL_CALL_START", "toolCallId": "c1", "toolCallName": "computer"
            }),
            serde_json::json!({ "type": "TOOL_CALL_END", "toolCallId": "c1" }),
            serde_json::json!({
                "type": "TOOL_CALL_RESULT",
                "toolCallId": "c1",
                "content": "screenshot of the 1280x800 screen attached",
                "image": { "mime": "image/png", "base64": png, "width": 1280, "height": 800 }
            }),
            serde_json::json!({
                "type": "TEXT_MESSAGE_CONTENT", "messageId": "m2", "delta": "It is open."
            }),
            serde_json::json!({ "type": "RUN_FINISHED", "runId": "run_1" }),
        ]
    }

    fn thread_run(
        run_id: &str,
        status: &str,
        started_at_ms: i64,
        events: &[serde_json::Value],
    ) -> ThreadRun {
        ThreadRun {
            run_id: run_id.to_string(),
            status: status.to_string(),
            started_at_ms,
            updated_at_ms: started_at_ms,
            failure: None,
            events: events.to_vec(),
        }
    }

    /// The turn as the stream painted it: every frame pushed as it arrived, the feed repainted
    /// from a snapshot each time, and the closing the end of the stream gives it.
    fn watched_live(events: &[serde_json::Value]) -> (String, Vec<ChatPart>) {
        let mut assembler = TurnAssembler::default();
        for event in events {
            assembler.push_event(event);
            let _ = assembler.snapshot();
        }
        assembler.finish();
        assembler.snapshot()
    }

    /// The bug the person reported, in the place it happens: switching bots mid-turn reloaded
    /// the thread from a database that does not yet have the reply, and the bubble and every
    /// picture under it went with it.
    #[test]
    fn a_thread_with_a_turn_in_flight_is_not_reloaded_out_from_under_it() {
        let mut conversation = thread(
            "cw_1",
            vec![
                at(message("m_ask", true, "open youtube"), 1_000),
                at(message("m_live", false, "Opening YouTube "), 2_000),
            ],
        );
        // What SQLite has: the person's message and nothing else. The reply is not written down
        // until the turn ends, so the persisted set is a subset of what is on screen.
        let rows = vec![row("db_ask", "user", "open youtube")];

        assert!(
            !apply_reload(&mut conversation, Some(&in_flight()), rows.clone()),
            "a thread in the middle of a turn refuses the rows"
        );
        assert_eq!(ids(&conversation.messages), vec!["m_ask", "m_live"]);
        assert_eq!(
            conversation.messages[1].content, "Opening YouTube ",
            "the bubble being filled in is still the one being filled in"
        );

        assert!(
            apply_reload(&mut conversation, None, rows),
            "with no turn in flight the rows are taken, which is what every other reload is"
        );
        assert_eq!(ids(&conversation.messages), vec!["db_ask"]);
    }

    #[test]
    fn a_turn_ending_belongs_to_the_run_that_still_holds_the_thread() {
        assert_eq!(turn_ending(Some(&in_flight()), "run_1"), TurnEnding::Ours);
    }

    #[test]
    fn a_turn_ending_stands_down_when_another_run_holds_the_thread_or_the_reply_is_being_saved() {
        let later = LiveTurn {
            run_id: "run_2".to_string(),
            ..in_flight()
        };
        assert_eq!(turn_ending(Some(&later), "run_1"), TurnEnding::Superseded);
        let saving = LiveTurn {
            persisting: true,
            ..in_flight()
        };
        assert_eq!(turn_ending(Some(&saving), "run_1"), TurnEnding::Superseded);
    }

    /// The thread was let go while this run's stream was still open — the person pressed stop,
    /// or (until 21 Sep 2026) a resync read the server's 404 for a run it had not recorded yet
    /// as "never started". Nobody else will end the line the run started, so its ending still
    /// has to clear it, or the thread says "working" until the app is restarted.
    #[test]
    fn a_turn_let_go_while_its_stream_was_open_still_ends_its_own_line() {
        assert_eq!(turn_ending(None, "run_1"), TurnEnding::Orphaned);
    }

    fn a_form() -> UiSpec {
        UiSpec::Form(FormSpec {
            title: Some("QA collapse remasure".into()),
            prompt: Some("Enter a label.".into()),
            fields: vec![FormField {
                id: "label".into(),
                label: "Label".into(),
                options: vec![],
            }],
            submit: "Submit".into(),
        })
    }

    /// A turn that answered with a form and no words was saved as its stand-in line alone —
    /// `saved_parts` dropped every `Ui` part — so the thread showed "[used form]" the moment it
    /// was read back from the database: on the next visit, and forever after a restart.
    #[test]
    fn a_wordless_form_turn_is_saved_with_its_form_and_comes_back_as_one() {
        let saved = saved_parts(&[ChatPart::Ui(a_form())]);
        assert_eq!(saved.len(), 1, "the form is written down: {saved:?}");
        assert!(matches!(saved[0], MessagePart::Ui { .. }), "{saved:?}");
        assert_eq!(
            restored_parts("[showed you a form]", saved),
            vec![ChatPart::Ui(a_form())]
        );
    }

    /// The frames of the run behind the Vamos "hi" of 21 Sep 2026: a `form` tool call, its
    /// arguments in pieces, no words. Read back off the server it has to be a form again, and
    /// the line it leaves for the model has to say so.
    #[test]
    fn a_form_only_run_replays_as_a_form_with_a_line_that_says_so() {
        let events = vec![
            serde_json::json!({"type":"RUN_STARTED","runId":"run_f","threadId":"cw_1"}),
            serde_json::json!({"type":"TOOL_CALL_START","toolCallId":"call_1","toolCallName":"form"}),
            serde_json::json!({"type":"TOOL_CALL_ARGS","toolCallId":"call_1","delta":"{\"title\":\"QA collapse remasure\",\"prompt\":\"Enter a label.\","}),
            serde_json::json!({"type":"TOOL_CALL_ARGS","toolCallId":"call_1","delta":"\"submit\":\"Submit\",\"fields\":[{\"id\":\"label\",\"label\":\"Label\",\"options\":[]}] }"}),
            serde_json::json!({"type":"TOOL_CALL_END","toolCallId":"call_1"}),
            serde_json::json!({"type":"TOOL_CALL_RESULT","toolCallId":"call_1","content":"rendered"}),
            serde_json::json!({"type":"RUN_FINISHED","runId":"run_f"}),
        ];
        let (plain, parts) = reply_from_replay(&events, "finished");
        assert_eq!(parts, vec![ChatPart::Ui(a_form())]);
        assert_eq!(
            replayed_ending(&events, "finished", None, &plain).as_deref(),
            Some("[showed you a form]")
        );
    }

    /// "The last one, if it isn't mine" was the whole of the old rule, and everything that
    /// touches the list breaks it — after which the run fills in a bubble belonging to something
    /// else, which reads exactly like the run having stopped.
    #[test]
    fn a_run_finds_its_bubble_after_the_thread_moved_underneath_it() {
        let mut conversations = vec![
            thread("cw_other", vec![message("m_live", false, "another bot")]),
            thread(
                "cw_1",
                vec![
                    at(message("m_ask", true, "open youtube"), 1_000),
                    at(message("m_live", false, ""), 2_000),
                ],
            ),
        ];
        // A permission card, a reload, a recovered older reply: each of these makes some other
        // row the last one.
        conversations[1]
            .messages
            .push(at(message("m_card", false, "may I?"), 3_000));
        conversations[1]
            .messages
            .insert(0, at(message("m_older", false, "yesterday"), 100));

        let found = streaming_message_mut(&mut conversations, "cw_1", "m_live")
            .expect("the bubble the turn was given");
        found.content = "Opening YouTube on my box.".to_string();
        assert_eq!(
            conversations[1].messages[2].content, "Opening YouTube on my box.",
            "the words went where the run's own bubble had drifted to"
        );
        assert_eq!(
            conversations[1].messages[3].content, "may I?",
            "and nowhere near the row that happens to be last"
        );

        assert_eq!(
            conversations[0].messages[0].content, "another bot",
            "and not into the thread the person switched to, which holds a row under the very \
             same id — the thread is looked up first, and only then the row"
        );

        conversations[1].messages.retain(|m| m.id != "m_live");
        assert!(
            streaming_message_mut(&mut conversations, "cw_1", "m_live").is_none(),
            "a bubble that is genuinely gone is written nowhere at all"
        );
    }

    /// The two paths must not drift. A turn watched live and the same turn read back off the
    /// server have to come to the same bubbles and the same pictures, or "where was I?" and
    /// "what happened?" are two different answers and the person has to pick one to believe.
    #[test]
    fn a_turn_read_back_off_the_server_comes_to_what_the_same_turn_watched_live_came_to() {
        let events = turn_frames();
        let (live_plain, live_parts) = watched_live(&events);
        let (replayed_plain, replayed_parts) = reply_from_replay(&events, "finished");

        assert_eq!(replayed_plain, live_plain);
        assert_eq!(shape(&replayed_parts), shape(&live_parts));
        assert!(
            shape(&live_parts)
                .iter()
                .any(|part| part.starts_with("shot c1 1280x800")),
            "the picture is in the turn, or this proves nothing about pictures"
        );
        assert_eq!(saved_parts(&replayed_parts), saved_parts(&live_parts));
        assert!(
            saved_parts(&replayed_parts)
                .iter()
                .any(|part| matches!(part, crate::services::database::MessagePart::Screenshot { call_id, .. } if call_id == "c1")),
            "the turn-end pin is kept: {:?}",
            saved_parts(&replayed_parts)
        );
    }

    /// Reconciling is a diff, not a rebuild: a run the thread already has a reply for was built
    /// from these very frames, and saying it again would put the same turn in twice.
    #[test]
    fn reconciling_takes_only_the_runs_the_thread_cannot_account_for() {
        let events = turn_frames();
        let runs = vec![
            thread_run("run_1", "finished", 1_000, &events),
            thread_run("run_2", "finished", 5_000, &events),
            thread_run("run_3", "running", 9_000, &events[..3]),
        ];
        let messages = vec![
            at(message("m_ask", true, "open youtube"), 900),
            from_run("m_run1", "Opening YouTube on my box.", "run_1", 1_100),
            at(message("m_ask2", true, "again"), 4_900),
        ];

        let missing = missing_replies(&messages, &runs);
        assert_eq!(
            missing
                .iter()
                .map(|reply| reply.run_id.as_str())
                .collect::<Vec<_>>(),
            vec!["run_2", "run_3"],
            "oldest first, and run_1 is already written down"
        );
        assert!(
            !missing[0].live,
            "a run that ended is one to show and write down"
        );
        assert!(
            missing[1].live,
            "a run still going is one to re-attach to, which is how a restart mid-turn recovers"
        );
        assert!(
            shape(&missing[0].parts)
                .iter()
                .any(|part| part.starts_with("shot c1 ")),
            "a recovered turn brings its pictures back with it"
        );

        // Nothing missing is the ordinary case, and it must cost the thread nothing.
        let settled = vec![
            from_run("m_run1", "…", "run_1", 1_100),
            from_run("m_run2", "…", "run_2", 5_100),
            from_run("m_run3", "…", "run_3", 9_100),
        ];
        assert!(missing_replies(&settled, &runs).is_empty());
    }

    /// Every thread the build in the person's hands wrote has replies that cannot say which run
    /// they came out of. On one of those a finished run and an already-saved one look the same,
    /// and guessing would put the last few turns into the transcript twice.
    #[test]
    fn a_thread_that_can_name_no_run_takes_only_the_one_still_going() {
        let events = turn_frames();
        let runs = vec![
            thread_run("run_1", "finished", 1_000, &events),
            thread_run("run_2", "finished", 5_000, &events),
            thread_run("run_3", "running", 9_000, &events[..3]),
        ];
        // What an older build left behind: replies with no run id on them at all.
        let legacy = vec![
            at(message("m_ask", true, "open youtube"), 900),
            at(
                message("m_said", false, "Opening YouTube on my box."),
                1_100,
            ),
            at(message("m_ask2", true, "again"), 4_900),
            at(message("m_said2", false, "Open again."), 5_100),
        ];
        let missing = missing_replies(&legacy, &runs);
        assert_eq!(
            missing
                .iter()
                .map(|reply| reply.run_id.as_str())
                .collect::<Vec<_>>(),
            vec!["run_3"],
            "the finished runs are left alone; only the one that cannot already be here is taken"
        );

        // One reply that names its run is all it takes: from there the diff is exact again, and
        // the runs before it are behind the thread's knowledge rather than missing from it.
        let mut named = legacy.clone();
        named[3].run_id = Some("run_2".to_string());
        assert_eq!(
            missing_replies(&named, &runs)
                .iter()
                .map(|reply| reply.run_id.as_str())
                .collect::<Vec<_>>(),
            vec!["run_3"],
            "run_1 is older than a run the thread can name, so its reply is older still"
        );
    }

    /// A run can be started and die before it says anything. A blank bubble in the transcript is
    /// worse than the absence of one.
    #[test]
    fn a_run_that_said_nothing_at_all_is_not_grafted_as_an_empty_bubble() {
        let silent = vec![serde_json::json!({
            "type": "RUN_STARTED", "threadId": "cw_1", "runId": "run_9"
        })];
        let runs = vec![thread_run("run_9", "running", 1_000, &silent)];
        assert!(missing_replies(&[], &runs).is_empty());
    }

    /// The thread is in the order things were said, and a reply that arrives late is still a
    /// reply to the message that asked for it.
    #[test]
    fn a_recovered_reply_goes_back_where_it_happened_rather_than_on_the_end() {
        let mut messages = vec![
            at(message("m_ask", true, "open youtube"), 1_000),
            at(message("m_ask2", true, "and close it"), 5_000),
        ];
        let reply = RecoveredReply {
            run_id: "run_1".to_string(),
            content: "Opening.".to_string(),
            parts: vec![ChatPart::Text("Opening.".to_string())],
            live: false,
            started_at: SystemTime::UNIX_EPOCH + Duration::from_millis(2_000),
        };

        let id = graft_reply(&mut messages, &reply);
        assert_eq!(
            ids(&messages),
            vec!["m_ask", id.as_str(), "m_ask2"],
            "after the message that asked for it, before whatever was said next"
        );
        assert_eq!(
            messages[1].run_id.as_deref(),
            Some("run_1"),
            "and it says which run it came out of, so the next reconcile leaves it alone"
        );
    }

    // ---- Stopping a turn --------------------------------------------------------------------

    /// A thread with a turn in flight in it, the way `send_opengrok_turn` leaves one: the row the
    /// run is filling in, the run registered against the thread, and the bot marked as working.
    fn mid_turn(reply: Message) -> AppState {
        let mut state = AppState::new();
        state.conversations.push(thread(
            "cw_1",
            vec![at(message("m_ask", true, "open youtube"), 10), reply],
        ));
        state.active_conversation_id = Some("cw_1".to_string());
        state.active_coworker_id = Some("cw_1".to_string());
        state.live_turns.insert("cw_1".to_string(), in_flight());
        state.begin_responding(Some("cw_1"), "Working");
        state
    }

    /// The composer's button is a stop button for exactly as long as there is something to stop,
    /// which is the whole of what it promises.
    #[test]
    fn the_button_is_a_stop_button_while_the_turn_runs_and_a_send_arrow_at_every_ending() {
        let mut state = mid_turn(at(message("m_live", false, ""), 20));
        assert!(
            state.is_turn_in_flight(),
            "the turn was sent and is registered against the thread"
        );

        // An ordinary ending: `persist_assistant_reply` marks the turn the moment it takes the
        // reply, and the thread stays registered only until that write lands.
        state.live_turns.get_mut("cw_1").unwrap().persisting = true;
        assert!(
            !state.is_turn_in_flight(),
            "the outcome is decided, so there is nothing left running to stop"
        );

        // A run that failed leaves the app's own words behind, which are never written down, so
        // the thread is let go on the spot instead.
        state.live_turns.insert("cw_1".to_string(), in_flight());
        state.release_live_turn("cw_1", "run_1");
        assert!(!state.is_turn_in_flight());

        // And a turn running in the thread next door is not this thread's to stop.
        state.live_turns.insert("cw_2".to_string(), in_flight());
        assert!(
            !state.is_turn_in_flight(),
            "the live turn is kept per thread, which is what makes it worth asking"
        );
        assert!(
            state.turn_to_stop().is_none(),
            "so a stop pressed here finds nothing, rather than stopping somebody else's turn"
        );
    }

    /// Clearing the working line is NOT an ending, and the composer is where the difference shows.
    ///
    /// THIS IS THE PAIR A PERSON REPORTED AS A BUG on 18 Sep 2026: they denied a command, no
    /// coworker was thinking any more — and the send button was still a stop button. The denial
    /// path had reached for `finish_responding` as though it ended the turn. It does not: it
    /// takes away the line that says somebody is working, and leaves the turn registered against
    /// the thread, which is the fact the button is drawn from.
    ///
    /// Both real endings do more than this — `persist_assistant_reply` marks the turn while the
    /// reply goes to disk, `release_live_turn` lets go of a turn with nothing to write down — so
    /// this test is here to say that the third thing, on its own, is neither.
    #[test]
    fn clearing_the_working_line_does_not_let_go_of_the_turn() {
        let mut state = mid_turn(at(message("m_live", false, ""), 20));
        assert_eq!(state.thread_status("cw_1"), Some("Working"));

        state.finish_responding(Some("cw_1"), false);
        assert_eq!(
            state.thread_status("cw_1"),
            None,
            "nothing looks busy in the thread any more"
        );
        assert!(
            state.is_turn_in_flight(),
            "but the turn is still this thread's, so the composer still offers to stop it — \
             which is exactly the pair that reads as a bug: no coworker working, and a stop \
             button over it"
        );

        // Waiting on a card is the one time the two are meant to differ, and it says so rather
        // than falling silent: the turn is genuinely still in flight, stopped on a person.
        state.finish_responding(Some("cw_1"), true);
        assert_eq!(state.thread_status("cw_1"), Some(WAITING_APPROVAL_STATUS));
        assert!(state.is_turn_in_flight());

        // And an ending is what lets the button go back to a send arrow.
        state.release_live_turn("cw_1", "run_1");
        assert!(!state.is_turn_in_flight());
    }

    #[test]
    fn finished_user_form_wait_drops_stop_and_keeps_waiting_chrome() {
        let spec = crate::opengrok::UserFormSpec::from_custom_event(&serde_json::json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "callId": "call-9",
            "entryId": "e_form",
            "reason": "user-form",
            "arguments": {
                "title": "Google account",
                "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
            }
        }))
        .unwrap();
        let mut bot = message("m_live", false, "");
        bot.parts = vec![ChatPart::UserForm(spec)];
        let mut state = mid_turn(at(bot, 20));
        assert!(state.is_turn_in_flight());

        state.park_waiting_for_you("cw_1", "run_1");

        assert_eq!(
            state.visible_bot_status().as_deref(),
            Some(WAITING_FOR_YOU_STATUS)
        );
        assert_eq!(
            bot_status_line("Grok", WAITING_FOR_YOU_STATUS),
            "Waiting for you"
        );
        assert_eq!(bot_status_line("Grok", "Working"), "Grok is working");
        assert_eq!(
            bot_status_line("Vamos", crate::opengrok::WAKING_COMPUTER),
            "Waking Vamos's computer",
            "the wait for the box is named, not folded into working"
        );
        assert!(
            !state.is_turn_in_flight(),
            "server run is finished; composer is Send, not Stop"
        );
        assert!(state.turn_to_stop().is_none());
        assert!(
            state.live_turns.get("cw_1").is_none(),
            "do not keep a local live-turn past the run"
        );
        assert_eq!(state.open_user_forms().len(), 1);
    }

    fn parked_form() -> crate::opengrok::UserFormSpec {
        crate::opengrok::UserFormSpec::from_custom_event(&serde_json::json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "runId": "run_1",
            "callId": "call-9",
            "entryId": "e_form",
            "reason": "user-form",
            "arguments": {
                "title": "Google account",
                "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
            }
        }))
        .unwrap()
    }

    /// A card painted mid-stream is already parked, and a send over it is a
    /// steer that settles the card here: the server closes it on the same
    /// message, and the app must not `/stop` the run first.
    #[test]
    fn a_form_painted_mid_stream_is_parked_and_a_steer_settles_it() {
        let mut bot = message("m_live", false, "");
        bot.parts = vec![ChatPart::UserForm(parked_form())];
        let mut state = mid_turn(at(bot, 20));
        assert!(state.has_open_user_form("cw_1"));
        assert_eq!(state.busy_state("cw_1"), Busy::Parked);

        state.settle_parked_cards("cw_1");
        assert!(!state.has_open_user_form("cw_1"));
        assert!(!state.has_hitl_to_interrupt("cw_1"));
        let spec = state
            .conversations
            .iter()
            .flat_map(|c| c.messages.iter())
            .flat_map(|m| m.parts.iter())
            .find_map(|part| match part {
                ChatPart::UserForm(spec) => Some(spec.clone()),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            spec.effective_resolution(),
            Some(FormResolution::Superseded)
        );
        assert_eq!(
            state.busy_state("cw_1"),
            Busy::Running,
            "the stream that painted the card is still open; it is not parked any more"
        );
    }

    /// An open approval and a live Computer handoff are parked too, and a
    /// steer settles both: the approval reads Superseded (no allow, no deny
    /// line), the handoff is skipped, and neither keeps the thread parked.
    #[test]
    fn an_open_approval_or_live_handoff_is_parked_and_a_steer_settles_it() {
        let approval = crate::opengrok::ApprovalSpec {
            run_id: "run_1".into(),
            thread_id: None,
            call_id: "c1".into(),
            tool: "Shell".into(),
            command: "ls".into(),
            why: String::new(),
            reason: "exec-consent".into(),
            output: None,
            ok: None,
        };
        let mut bot = message("m_live", false, "");
        bot.parts = vec![ChatPart::Approval(approval)];
        let mut state = mid_turn(at(bot, 20));
        state.finish_responding(Some("cw_1"), true);
        assert!(state.has_open_approval("cw_1"));
        assert_eq!(state.busy_state("cw_1"), Busy::Parked);
        state.settle_parked_cards("cw_1");
        assert!(!state.has_open_approval("cw_1"));
        assert_eq!(
            state.approval_decisions.get("c1"),
            Some(&ApprovalDecision::Superseded)
        );
        assert_eq!(
            state.thread_status("cw_1"),
            None,
            "Waiting for approval is gone"
        );

        let spec = crate::opengrok::UserFormSpec::from_custom_event(&serde_json::json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "callId": "call-9",
            "entryId": "e_form",
            "reason": "user-form",
            "arguments": {
                "title": "Website login",
                "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
            }
        }))
        .unwrap();
        let mut form_bot = message("m_live", false, "");
        form_bot.parts = vec![ChatPart::UserForm(spec)];
        let mut handoff = mid_turn(at(form_bot, 20));
        handoff.park_waiting_for_you("cw_1", "run_1");
        handoff.paint_user_form_resolution("e_form", FormResolution::Dismissed);
        handoff.set_computer_handoff(
            "e_form",
            crate::opengrok::ComputerHandoffStatus::ActionNeeded,
        );
        assert!(
            handoff.has_open_user_form("cw_1"),
            "live Computer handoff is HITL"
        );
        assert_eq!(handoff.busy_state("cw_1"), Busy::Parked);
        handoff.settle_parked_cards("cw_1");
        assert!(!handoff.has_open_user_form("cw_1"));
        assert_eq!(
            handoff.user_form_computer_handoffs.get("e_form"),
            Some(&crate::opengrok::ComputerHandoffStatus::Skipped)
        );
        assert_eq!(handoff.busy_state("cw_1"), Busy::Idle);
    }

    /// A working turn with no card is running, not parked: a plain send is
    /// held (queue) rather than sent over it, and nothing settles.
    #[test]
    fn a_working_turn_without_hitl_is_running_not_parked() {
        let state = mid_turn(at(message("m_live", false, ""), 20));
        assert!(state.is_active_bot_responding());
        assert!(state.is_turn_in_flight());
        assert_eq!(state.busy_state("cw_1"), Busy::Running);
        assert_eq!(
            crate::send_policy::plan_send(Busy::Running, crate::send_policy::OnSend::Queue, false),
            crate::send_policy::SendPlan::Queue
        );
    }

    /// Cold "hi": no open HITL, no Waiting, no parked run. The thread is idle.
    #[test]
    fn cold_composer_send_is_idle() {
        let mut state = AppState::new();
        state
            .conversations
            .push(thread("cw_1", vec![at(message("m_hi", true, "hi"), 10)]));
        state.active_conversation_id = Some("cw_1".to_string());
        state.active_coworker_id = Some("cw_1".to_string());
        assert!(!state.has_open_user_form("cw_1"));
        assert!(!state.has_open_approval("cw_1"));
        assert!(!state.has_hitl_to_interrupt("cw_1"));
        assert_eq!(state.busy_state("cw_1"), Busy::Idle);
    }

    /// The run finished and left the card (**Waiting for you**). A send is a
    /// steer: the card settles as Superseded, the Waiting chrome goes, the
    /// parked-run entry goes, and the thread reads idle — nothing to `/stop`.
    #[test]
    fn a_parked_steer_settles_every_card_and_clears_waiting() {
        let mut bot = message("m_live", false, "");
        bot.parts = vec![ChatPart::UserForm(parked_form())];
        let mut state = mid_turn(at(bot, 20));
        state.park_waiting_for_you("cw_1", "run_1");
        assert_eq!(state.thread_status("cw_1"), Some(WAITING_FOR_YOU_STATUS));
        assert_eq!(state.busy_state("cw_1"), Busy::Parked);

        state.settle_parked_cards("cw_1");
        assert!(!state.has_hitl_to_interrupt("cw_1"));
        assert_eq!(state.thread_status("cw_1"), None);
        assert_eq!(state.busy_state("cw_1"), Busy::Idle);
    }

    #[test]
    fn skip_clears_waiting_when_no_open_form_or_handoff_remains() {
        let spec = crate::opengrok::UserFormSpec::from_custom_event(&serde_json::json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "callId": "call-9",
            "entryId": "e_form",
            "reason": "user-form",
            "arguments": {
                "title": "Website login",
                "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
            }
        }))
        .unwrap();
        let mut bot = message("m_live", false, "");
        bot.parts = vec![ChatPart::UserForm(spec)];
        let mut state = mid_turn(at(bot, 20));
        state.park_waiting_for_you("cw_1", "run_1");
        assert_eq!(state.thread_status("cw_1"), Some(WAITING_FOR_YOU_STATUS));

        state.paint_user_form_resolution("e_form", FormResolution::Skipped);
        state.set_computer_handoff("e_form", crate::opengrok::ComputerHandoffStatus::Skipped);
        state.user_form_handoff_done.insert("e_form".into());
        state.sync_waiting_chrome("cw_1");

        assert_eq!(
            state.thread_status("cw_1"),
            None,
            "Waiting for you must drop once Skip settles the last open card"
        );
        assert!(state.open_user_forms().is_empty());
        assert!(state.open_computer_handoffs().is_empty());
    }

    #[test]
    fn stacked_open_forms_stay_enabled_and_waiting_until_each_settles() {
        fn login(entry: &str, call: &str) -> crate::opengrok::UserFormSpec {
            crate::opengrok::UserFormSpec::from_custom_event(&serde_json::json!({
                "type": "CUSTOM",
                "name": "run-awaiting-approval",
                "callId": call,
                "entryId": entry,
                "reason": "user-form",
                "arguments": {
                    "title": "Website login",
                    "fields": [
                        {"id": "email", "label": "Email", "type": "email", "required": true},
                        {"id": "password", "label": "Password", "type": "password", "required": true}
                    ]
                }
            }))
            .unwrap()
        }
        let a = login("e_a", "call-a");
        let b = login("e_b", "call-b");
        let c = login("e_c", "call-c");
        assert!(a.can_post(true));
        assert!(
            b.can_post(false),
            "global verbs-off must not gray this card"
        );
        assert!(c.can_post(false));
        let mut bot = message("m_live", false, "");
        bot.parts = vec![
            ChatPart::UserForm(a),
            ChatPart::UserForm(b),
            ChatPart::UserForm(c),
        ];
        let mut state = mid_turn(at(bot, 20));
        state.user_form_verbs_available = false;
        state.park_waiting_for_you("cw_1", "run_1");
        assert_eq!(state.open_user_forms().len(), 3);
        for spec in state.open_user_forms() {
            assert!(
                spec.can_post(state.user_form_verbs_available),
                "{} must stay Continue-able",
                spec.entry_id
            );
            assert!(
                spec.can_dismiss(),
                "{} must stay Dismiss-able",
                spec.entry_id
            );
        }

        state.paint_user_form_resolution("e_a", FormResolution::Dismissed);
        state.sync_waiting_chrome("cw_1");
        assert_eq!(
            state.thread_status("cw_1"),
            Some(WAITING_FOR_YOU_STATUS),
            "Dismiss one of three must not blank Waiting"
        );
        let open = state.open_user_forms();
        assert_eq!(open.len(), 2);
        assert!(open.iter().all(|spec| spec.can_post(false)));
        assert!(open.iter().all(|spec| spec.entry_id != "e_a"));

        state.paint_user_form_resolution("e_b", FormResolution::Dismissed);
        state.paint_user_form_resolution("e_c", FormResolution::Dismissed);
        state.sync_waiting_chrome("cw_1");
        assert_eq!(
            state.thread_status("cw_1"),
            None,
            "Waiting clears only after the last open form settles"
        );
        assert!(state.open_user_forms().is_empty());
    }

    fn call_keyed_login(call: &str) -> crate::opengrok::UserFormSpec {
        crate::opengrok::UserFormSpec::from_custom_event(&serde_json::json!({
            "type": "CUSTOM",
            "name": "run-awaiting-approval",
            "callId": call,
            "reason": "user-form",
            "arguments": {
                "title": "Website login",
                "fields": [
                    {"id": "email", "label": "Email", "type": "email", "required": true}
                ]
            }
        }))
        .unwrap()
    }

    /// `user-form-dismiss-call-*` must settle the card locally. HTTP has no
    /// gateway `entryId`; restore must not snap the form back to live.
    #[test]
    fn call_keyed_dismiss_settles_locally_and_survives_restore() {
        let spec = call_keyed_login("call-9");
        assert_eq!(spec.card_key(), "call-9");
        assert!(spec.entry_id.is_empty());
        assert!(spec.can_dismiss());
        assert!(!spec.can_post(true), "Continue stays gated without entryId");
        let mut bot = message("m_live", false, "");
        bot.parts = vec![ChatPart::UserForm(spec)];
        let mut state = mid_turn(at(bot, 20));
        state.park_waiting_for_you("cw_1", "run_1");
        assert_eq!(state.open_user_forms().len(), 1);
        assert_eq!(state.thread_status("cw_1"), Some(WAITING_FOR_YOU_STATUS));

        state.paint_user_form_resolution("call-9", FormResolution::Dismissed);
        state.sync_waiting_chrome("cw_1");
        assert!(
            state.open_user_forms().is_empty(),
            "call-* Dismiss must settle that card"
        );
        assert_eq!(state.thread_status("cw_1"), None);

        state.restore_user_form("call-9");
        assert!(
            state.open_user_forms().is_empty(),
            "no-gateway restore must not revive call-*"
        );
        match &state.conversations[0].messages[1].parts[0] {
            ChatPart::UserForm(spec) => {
                assert_eq!(spec.effective_resolution(), Some(FormResolution::Dismissed));
            }
            other => panic!("expected user-form, got {other:?}"),
        }
    }

    /// Open the screen on a call-keyed card mints `computer-handoff-call-*`
    /// even without a gateway `entryId`.
    #[test]
    fn call_keyed_open_screen_mints_computer_handoff() {
        let spec = call_keyed_login("call-9");
        let mut bot = message("m_live", false, "");
        bot.parts = vec![ChatPart::UserForm(spec)];
        let mut state = mid_turn(at(bot, 20));
        state.set_computer_handoff(
            "call-9",
            crate::opengrok::ComputerHandoffStatus::ActionNeeded,
        );
        let handoffs = state.visible_computer_handoffs();
        assert_eq!(handoffs.len(), 1);
        assert_eq!(handoffs[0].card_key(), "call-9");
        assert_eq!(
            crate::opengrok::computer_handoff_card_id(handoffs[0].card_key()),
            "computer-handoff-call-9"
        );
        assert!(handoffs[0].live_computer_handoff());
        state
            .user_form_handoff_restore
            .insert("call-9".into(), None);
        state.restore_user_form("call-9");
        assert!(
            state
                .visible_computer_handoffs()
                .iter()
                .any(|spec| spec.card_key() == "call-9" && spec.live_computer_handoff()),
            "call-* Open the screen must keep computer-handoff-call-*"
        );
    }

    /// Stacked call-* Dismiss one; others stay enabled. After those settle,
    /// Skip both e_* siblings with nothing else pending clears Waiting.
    #[test]
    fn call_keyed_dismiss_one_then_skip_siblings_clears_waiting() {
        fn gateway_login(entry: &str, call: &str) -> crate::opengrok::UserFormSpec {
            crate::opengrok::UserFormSpec::from_custom_event(&serde_json::json!({
                "type": "CUSTOM",
                "name": "run-awaiting-approval",
                "callId": call,
                "entryId": entry,
                "reason": "user-form",
                "arguments": {
                    "title": "Website login",
                    "fields": [
                        {"id": "email", "label": "Email", "type": "email", "required": true}
                    ]
                }
            }))
            .unwrap()
        }
        let e_a = gateway_login("e_a", "call-a");
        let e_b = gateway_login("e_b", "call-b");
        let extra_x = call_keyed_login("call-x");
        let extra_y = call_keyed_login("call-y");
        assert!(extra_x.can_dismiss() && extra_y.can_dismiss());
        assert!(!extra_x.can_post(true) && !extra_y.can_post(true));
        let mut bot = message("m_live", false, "");
        bot.parts = vec![
            ChatPart::UserForm(e_a),
            ChatPart::UserForm(e_b),
            ChatPart::UserForm(extra_x),
            ChatPart::UserForm(extra_y),
        ];
        let mut state = mid_turn(at(bot, 20));
        state.park_waiting_for_you("cw_1", "run_1");
        assert_eq!(state.open_user_forms().len(), 4);

        state.paint_user_form_resolution("call-x", FormResolution::Dismissed);
        state.sync_waiting_chrome("cw_1");
        let open = state.open_user_forms();
        assert_eq!(open.len(), 3, "dismiss one call-* leaves the rest live");
        assert!(open.iter().all(|spec| spec.card_key() != "call-x"));
        assert!(open.iter().any(|spec| spec.card_key() == "call-y"));
        assert!(open.iter().any(|spec| spec.entry_id == "e_a"));
        assert!(open.iter().any(|spec| spec.entry_id == "e_b"));
        assert!(open.iter().all(|spec| spec.can_dismiss()));
        assert_eq!(state.thread_status("cw_1"), Some(WAITING_FOR_YOU_STATUS));

        state.paint_user_form_resolution("call-y", FormResolution::Dismissed);
        state.sync_waiting_chrome("cw_1");
        assert_eq!(state.open_user_forms().len(), 2);
        assert_eq!(state.thread_status("cw_1"), Some(WAITING_FOR_YOU_STATUS));

        for key in ["e_a", "e_b"] {
            state.paint_user_form_resolution(key, FormResolution::Skipped);
            state.set_computer_handoff(key, crate::opengrok::ComputerHandoffStatus::Skipped);
            state.user_form_handoff_done.insert(key.into());
        }
        state.sync_waiting_chrome("cw_1");
        assert!(state.open_user_forms().is_empty());
        assert!(state.open_computer_handoffs().is_empty());
        assert_eq!(
            state.thread_status("cw_1"),
            None,
            "Skip both e_* siblings with no other pending forms must clear Waiting"
        );
    }

    #[test]
    fn skip_without_sibling_id_queues_instead_of_posting_form_id() {
        let mut spec = crate::opengrok::UserFormSpec::parse(
            &serde_json::json!({
                "entryId": "e_form",
                "formRequest": {
                    "title": "Website login",
                    "fields": [{"id": "email", "label": "Email", "type": "email", "required": true}]
                }
            }),
            None,
        )
        .unwrap();
        spec.computer_handoff = Some(crate::opengrok::ComputerHandoffStatus::ActionNeeded);
        let mut bot = message("m1", false, "");
        bot.parts = vec![ChatPart::UserForm(spec)];
        let mut state = AppState::new();
        state.conversations.push(thread("cw_1", vec![bot]));
        state.active_conversation_id = Some("cw_1".into());
        state.park_waiting_for_you("cw_1", "run_1");

        state.paint_user_form_resolution("e_form", FormResolution::Skipped);
        state.set_computer_handoff("e_form", crate::opengrok::ComputerHandoffStatus::Skipped);
        state.user_form_handoff_done.insert("e_form".into());
        state.queue_pending_box_handoff(
            "e_form",
            "e_form",
            PendingBoxHandoff {
                resolution: crate::opengrok::BoxHandoffResolution::Declined,
                run_id: "run_1".into(),
                conversation_id: "cw_1".into(),
                agent_id: "cw_1".into(),
            },
        );
        state.sync_waiting_chrome("cw_1");
        assert_eq!(state.box_handoff_post_id("e_form", "e_form"), None);
        assert!(state.user_form_pending_resolves.contains_key("e_form"));
        assert_eq!(state.thread_status("cw_1"), None);

        if let ChatPart::UserForm(spec) = &mut state.conversations[0].messages[0].parts[0] {
            spec.handoff_entry_id = Some("e_hand".into());
        }
        assert_eq!(
            state.box_handoff_post_id("e_form", "e_form").as_deref(),
            Some("e_hand")
        );
        let pending = state.take_pending_box_handoff("e_form", "e_form").unwrap();
        assert_eq!(
            pending.resolution,
            crate::opengrok::BoxHandoffResolution::Declined
        );
        assert_ne!(pending.resolution.as_str(), "e_form");
    }

    #[test]
    fn egress_tunnel_is_host_and_box_ready() {
        let mut state = AppState::new();
        assert!(state.egress_tunnel_enabled, "Route traffic defaults ON");
        state.host_egress_tunnel_available = true;
        assert!(
            !state.egress_tunnel_available(),
            "host without a ready box is not a tunnel"
        );
        assert_eq!(
            state.route_traffic_surface(),
            RouteTrafficSurface::Hidden,
            "unprovisioned hides Route traffic"
        );
        state.coworker_computer = Some(
            serde_json::from_value(serde_json::json!({
                "agentId": "cw_1",
                "state": "running",
                "egress_tunnel": { "ready": true }
            }))
            .unwrap(),
        );
        assert!(state.egress_tunnel_available());
        state.coworker_computer = Some(
            serde_json::from_value(serde_json::json!({
                "agentId": "cw_1",
                "state": "running",
                "egress_tunnel": { "ready": false }
            }))
            .unwrap(),
        );
        assert!(!state.egress_tunnel_available());
        assert_eq!(state.route_traffic_surface(), RouteTrafficSurface::Hidden);
        state.host_egress_tunnel_available = false;
        state.coworker_computer = Some(
            serde_json::from_value(serde_json::json!({
                "agentId": "cw_1",
                "state": "running",
                "egress_tunnel": { "ready": true }
            }))
            .unwrap(),
        );
        assert!(
            !state.egress_tunnel_available(),
            "a ready box without host/env is not a Review-an-action tunnel"
        );
        assert_eq!(
            state.route_traffic_surface(),
            RouteTrafficSurface::BotPane,
            "provisioned unique box still shows dedicated chrome without host intent"
        );
        state.coworker_computer = Some(
            serde_json::from_value(serde_json::json!({
                "agentId": "cw_1",
                "state": "running",
                "egress_tunnel": { "url": "ws://127.0.0.1:8790" }
            }))
            .unwrap(),
        );
        state.host_egress_tunnel_available = true;
        assert!(
            state.egress_tunnel_available(),
            "box tunnel URL is ready for Route traffic"
        );
        state.coworker_computer = None;
        state.host_egress_tunnel_available = true;
        assert!(!state.egress_tunnel_available());
        assert_eq!(
            state.route_traffic_surface(),
            RouteTrafficSurface::Hidden,
            "host intent without a provisioned box does not paint Route traffic"
        );
        state.host_egress_tunnel_available = false;
        state.egress_tunnel_enabled = true;
        assert_eq!(state.route_traffic_surface(), RouteTrafficSurface::Hidden);
        state.coworker_computer = Some(
            serde_json::from_value(serde_json::json!({
                "agentId": "cw_1",
                "state": "running",
                "isEgressTunnelAvailable": true
            }))
            .unwrap(),
        );
        assert_eq!(
            state.route_traffic_surface(),
            RouteTrafficSurface::Hidden,
            "host isEgressTunnelAvailable is not box provisioned"
        );
    }

    /// A failed policy write re-marks the card as once-only; the run's answer landing
    /// afterwards must not promote it back to the standing wording.
    #[test]
    fn a_failed_policy_write_keeps_the_once_only_line() {
        // The failure wrote Denied; the late answer says Never: keep Denied.
        assert_eq!(
            settled_decision(Some(&ApprovalDecision::Denied), ApprovalDecision::Never),
            None
        );
        assert_eq!(
            settled_decision(Some(&ApprovalDecision::AllowOnce), ApprovalDecision::Always),
            None
        );
        // The ordinary order: the answer lands on a Sending card and is written as decided.
        assert_eq!(
            settled_decision(Some(&ApprovalDecision::Sending), ApprovalDecision::Never),
            Some(ApprovalDecision::Never)
        );
        assert_eq!(
            settled_decision(None, ApprovalDecision::Always),
            Some(ApprovalDecision::Always)
        );
        // A once-only answer is never blocked by an earlier once-only line.
        assert_eq!(
            settled_decision(Some(&ApprovalDecision::Denied), ApprovalDecision::Denied),
            Some(ApprovalDecision::Denied)
        );
        // And the fallback itself: a Never not kept still denied this call.
        assert_eq!(
            once_only_for(LocalExecMode::Never),
            ApprovalDecision::Denied
        );
        assert_eq!(
            once_only_for(LocalExecMode::Always),
            ApprovalDecision::AllowOnce
        );
    }

    /// The network choice follows Route traffic's surface, and only exists when the server
    /// sent one.
    #[test]
    fn egress_policy_sits_where_route_traffic_sits_and_only_when_sent() {
        fn computer(extra: serde_json::Value) -> crate::opengrok::CoworkerComputer {
            let mut body = serde_json::json!({
                "agentId": "cw_1",
                "state": "running",
                "boxId": "box_1",
                "egress_tunnel": { "ready": true }
            });
            if let (serde_json::Value::Object(map), Some(obj)) = (extra, body.as_object_mut()) {
                obj.extend(map);
            }
            serde_json::from_value(body).unwrap()
        }
        let mut state = AppState::new();
        state.active_coworker_id = Some("cw_1".into());
        state.coworker_computer = Some(computer(serde_json::json!({ "shareScope": "dedicated" })));
        assert!(state.show_route_traffic_on_bot_pane());
        assert_eq!(
            state.egress_policy(),
            None,
            "an older server sends no policy"
        );
        assert!(!state.show_egress_policy_on_bot_pane());

        state.coworker_computer = Some(computer(serde_json::json!({
            "shareScope": "dedicated",
            "egressPolicy": "ask"
        })));
        assert_eq!(state.egress_policy(), Some(LocalExecMode::Ask));
        assert!(state.show_egress_policy_on_bot_pane());
        assert!(!state.show_egress_policy_in_user_settings());

        state.coworker_computer = Some(computer(serde_json::json!({
            "shareScope": "user",
            "egressPolicy": "never"
        })));
        assert_eq!(state.egress_policy(), Some(LocalExecMode::Never));
        assert!(!state.show_egress_policy_on_bot_pane());
        assert!(state.show_egress_policy_in_user_settings());

        state.coworker_computer = Some(computer(serde_json::json!({
            "shareScope": "org",
            "egressPolicy": "bypass"
        })));
        assert!(!state.show_egress_policy_on_bot_pane());
        assert!(state.show_egress_policy_in_user_settings());
        state.coworker_computer = Some(computer(serde_json::json!({
            "shareScope": "group",
            "egressPolicy": "ask"
        })));
        assert!(state.show_egress_policy_on_bot_pane());
        assert!(!state.show_egress_policy_in_user_settings());
    }

    #[test]
    fn route_traffic_surface_follows_share_scope() {
        fn computer(extra: serde_json::Value) -> crate::opengrok::CoworkerComputer {
            let mut body = serde_json::json!({
                "agentId": "cw_1",
                "state": "running",
                "boxId": "box_1",
                "egress_tunnel": { "ready": true }
            });
            if let serde_json::Value::Object(map) = extra {
                if let Some(obj) = body.as_object_mut() {
                    obj.extend(map);
                }
            }
            serde_json::from_value(body).unwrap()
        }
        fn coworker(id: &str, box_id: &str) -> Coworker {
            serde_json::from_value(serde_json::json!({
                "id": id,
                "name": id,
                "boxId": box_id
            }))
            .unwrap()
        }

        let mut state = AppState::new();
        state.active_coworker_id = Some("cw_1".into());
        state.coworkers = vec![coworker("cw_1", "box_1")];
        state.coworker_computer = Some(computer(serde_json::json!({
            "shareScope": "dedicated"
        })));
        assert!(state.show_route_traffic_on_bot_pane());
        assert!(!state.show_route_traffic_in_user_settings());

        state.coworker_computer = Some(computer(serde_json::json!({
            "shareScope": "user"
        })));
        assert!(!state.show_route_traffic_on_bot_pane());
        assert!(state.show_route_traffic_in_user_settings());

        state.coworker_computer = Some(computer(serde_json::json!({
            "shareScope": "group",
            "groupId": "grp_1"
        })));
        assert_eq!(
            state.route_traffic_surface(),
            RouteTrafficSurface::BotPane,
            "a group's computer is the group's own: its pane"
        );

        state.coworker_computer = Some(computer(serde_json::json!({
            "shareScope": "org"
        })));
        assert_eq!(
            state.route_traffic_surface(),
            RouteTrafficSurface::UserSettings,
            "an org's computer is shared: Settings → Computer"
        );
        assert!(state.computer_is_org_shared());

        state.coworker_computer = Some(computer(serde_json::json!({})));
        state.coworkers = vec![coworker("cw_1", "box_1"), coworker("cw_2", "box_1")];
        assert!(
            state.show_route_traffic_in_user_settings(),
            "missing shareScope + shared boxId is user-level Settings, not both panes"
        );
        assert!(!state.show_route_traffic_on_bot_pane());

        state.coworkers = vec![coworker("cw_1", "box_1"), coworker("cw_2", "box_2")];
        assert!(
            state.show_route_traffic_on_bot_pane(),
            "missing shareScope + unique box is dedicated pane chrome"
        );
    }

    /// Nothing is in flight, so a stop is a question with the answer "there is nothing to stop".
    /// The button is not a stop button then, but a keystroke or a driver can still ask.
    #[test]
    fn a_stop_with_no_turn_in_flight_finds_nothing_rather_than_failing() {
        let mut state = AppState::new();
        assert!(state.turn_to_stop().is_none(), "no thread is even open yet");

        state.conversations.push(thread("cw_1", Vec::new()));
        state.active_conversation_id = Some("cw_1".to_string());
        assert!(
            state.turn_to_stop().is_none(),
            "an open thread that has never been asked anything"
        );

        let mut settled = in_flight();
        settled.persisting = true;
        state.live_turns.insert("cw_1".to_string(), settled);
        assert!(
            state.turn_to_stop().is_none(),
            "a turn already on its way to disk is registered, but nothing is running under it"
        );
    }

    /// The loop the person filmed, stopped: the bubble the run had been filling in keeps what it
    /// had said, and the app says what happened to the rest of it.
    #[test]
    fn a_stop_keeps_what_the_turn_had_said_and_says_that_it_was_stopped() {
        let mut live = at(message("m_live", false, "Opening YouTube on my box."), 20);
        live.run_id = Some("run_1".to_string());
        live.parts = vec![ChatPart::Text("Opening YouTube on my box.".to_string())];
        let mut state = mid_turn(live);

        let kept = state.end_stopped_turn("cw_1", &in_flight());
        assert_eq!(
            kept.as_ref().map(|(content, _)| content.as_str()),
            Some("Opening YouTube on my box."),
            "a turn cut short is still a turn that happened, and what it said goes to disk"
        );

        let messages = &state.conversations[0].messages;
        assert_eq!(
            ids(messages)[..2],
            ["m_ask", "m_live"],
            "the message that asked and the reply that had begun are both still there"
        );
        let note = messages.last().unwrap();
        assert_eq!(note.content, STOPPED_TURN_NOTE);
        assert!(
            is_status_line(&note.content),
            "which is how the feed knows to paint it as a line rather than as speech, and how \
             `persist_assistant_reply` knows never to write it down"
        );
        assert!(
            note.run_id.is_none(),
            "it came out of no run, so a reconcile cannot mistake it for the run's own reply"
        );
        assert!(
            agui_messages(messages)
                .iter()
                .all(|sent| sent.content != STOPPED_TURN_NOTE),
            "and the coworker is never told the app's account of the turn as if it were its own"
        );

        assert!(
            !state.is_turn_in_flight(),
            "the button is a send arrow again at once, not after the reply reaches the database"
        );
        assert!(
            state.visible_bot_status().is_none(),
            "and the working line is gone"
        );
        assert!(!state.is_active_bot_responding());
    }

    /// A stop pressed before the coworker managed to say anything. There is nothing to write
    /// down, so the thread is let go here — and an empty bubble left standing over the note
    /// would read as an answer still on its way.
    #[test]
    fn a_stop_before_the_turn_said_anything_leaves_no_empty_bubble_behind() {
        let mut state = mid_turn(at(message("m_live", false, ""), 20));

        let kept = state.end_stopped_turn("cw_1", &in_flight());
        assert!(kept.is_none());

        let messages = &state.conversations[0].messages;
        assert_eq!(messages.len(), 2);
        assert_eq!(ids(messages)[0], "m_ask");
        assert_eq!(messages[1].content, STOPPED_TURN_NOTE);
        assert!(
            !state.live_turns.contains_key("cw_1"),
            "nothing is going to the database, so nothing would let the thread go later"
        );
        assert!(!state.is_turn_in_flight());
    }

    /// A turn that only took pictures said nothing, but it did something, and the pictures are of
    /// things that really happened to the box.
    #[test]
    fn a_stop_keeps_a_turn_that_had_only_taken_pictures() {
        let mut live = at(message("m_live", false, ""), 20);
        live.parts = vec![screenshot(
            "c1",
            "the box's screen",
            b"png-one",
            (1280, 800),
        )];
        let mut state = mid_turn(live);

        let kept = state.end_stopped_turn("cw_1", &in_flight());
        assert_eq!(
            kept.map(|(_, parts)| parts.len()),
            Some(1),
            "the row is kept, pictures and all"
        );
        assert!(
            state.conversations[0]
                .messages
                .iter()
                .any(|message| message.id == "m_live")
        );
    }

    // ---- A working line per thread ----------------------------------------------------------

    /// Two threads, each with a turn in it, the way `send_opengrok_turn` leaves them: the run
    /// registered against the thread and the thread's own working line lit.
    fn two_bots_working() -> AppState {
        let mut state = AppState::new();
        state.conversations.push(thread("cw_1", Vec::new()));
        state.conversations.push(thread("cw_2", Vec::new()));
        state.active_conversation_id = Some("cw_1".to_string());
        state.active_coworker_id = Some("cw_1".to_string());
        state.live_turns.insert("cw_1".to_string(), in_flight());
        state.begin_responding(Some("cw_1"), "Thinking");
        // The second bot is started while the first is still going, which is the whole of the
        // case: a turn now survives being switched away from, so leaving one running and asking
        // another for something is the ordinary thing to do.
        state.live_turns.insert("cw_2".to_string(), in_flight());
        state.begin_responding(Some("cw_2"), "Running commands");
        state
    }

    /// Reading another thread, as switching bots does.
    fn open_thread(state: &mut AppState, id: &str) {
        state.active_conversation_id = Some(id.to_string());
        state.active_coworker_id = Some(id.to_string());
    }

    /// The label a person reads is the label of the thread they are reading. Starting a second
    /// bot used to take the one field there was, so the first bot's line went dark while its run
    /// was still going.
    #[test]
    fn two_turns_in_flight_each_keep_their_own_working_line() {
        let mut state = two_bots_working();
        assert_eq!(
            state.visible_bot_status().as_deref(),
            Some("Thinking"),
            "the open thread's line is still the open thread's own"
        );
        assert!(state.is_active_bot_responding());

        open_thread(&mut state, "cw_2");
        assert_eq!(
            state.visible_bot_status().as_deref(),
            Some("Running commands"),
            "and the other thread's line is the other thread's own"
        );
        assert!(state.is_active_bot_responding());
    }

    /// Coming back to a bot that never stopped working. Its line is where it was left, and the
    /// bot next door is undisturbed by the visit.
    ///
    /// This is the sequence from the report: start a turn on one bot, start another on a second,
    /// then go back. The line used to be gone on the way back, and putting it back — which is
    /// what re-attaching to the run does — used to take it from the second bot.
    #[test]
    fn returning_to_a_running_thread_shows_that_threads_status_and_not_the_other_ones() {
        let mut state = two_bots_working();
        open_thread(&mut state, "cw_2");
        open_thread(&mut state, "cw_1");
        assert_eq!(
            state.visible_bot_status().as_deref(),
            Some("Thinking"),
            "the line is there the moment the thread is open again, not a round trip later"
        );
        assert!(
            state.is_turn_in_flight(),
            "and it agrees with the button beside it, which has always read the live turn"
        );

        // What re-attaching to the run does once the server says it is still going.
        state.begin_responding(Some("cw_1"), "Working");
        open_thread(&mut state, "cw_2");
        assert_eq!(
            state.visible_bot_status().as_deref(),
            Some("Running commands"),
            "and the bot that was left running was never touched by the visit"
        );
    }

    /// A frame belongs to the thread it came out of. It used to be checked against a single
    /// app-wide owner, so every frame of the first bot's turn was thrown away from the moment a
    /// second bot started one — the turn went on, and the app stopped hearing about it.
    #[test]
    fn a_frame_lands_on_its_own_thread_while_another_bot_is_working() {
        let mut state = two_bots_working();
        state.apply_turn_status(
            Some("cw_1"),
            ActivityTick::Set(BotActivity {
                label: "Opening youtube.com".into(),
            }),
        );
        assert_eq!(
            state.visible_bot_status().as_deref(),
            Some("Opening youtube.com"),
            "the frame was applied rather than discarded"
        );

        open_thread(&mut state, "cw_2");
        assert_eq!(
            state.visible_bot_status().as_deref(),
            Some("Running commands"),
            "and it was not painted into the thread that happened to start a turn last"
        );
    }

    /// One turn ending says nothing about the other. The clear used to be app-wide, so whichever
    /// bot finished first put the light out on the one still working.
    #[test]
    fn ending_one_threads_turn_leaves_the_other_threads_line_alone() {
        let mut state = two_bots_working();
        state.finish_responding(Some("cw_1"), false);
        assert_eq!(
            state.visible_bot_status(),
            None,
            "the thread whose turn ended has no working line"
        );
        assert!(!state.is_active_bot_responding());

        open_thread(&mut state, "cw_2");
        assert_eq!(
            state.visible_bot_status().as_deref(),
            Some("Running commands"),
            "and the bot still working still says so"
        );
        assert!(state.is_active_bot_responding());
    }

    /// A turn that has stopped to ask for permission is waiting on a person rather than working,
    /// and that is true of one thread at a time like everything else here.
    #[test]
    fn a_thread_waiting_for_an_answer_says_so_without_ending_the_other_turn() {
        let mut state = two_bots_working();
        state.finish_responding(Some("cw_1"), true);
        assert_eq!(
            state.visible_bot_status().as_deref(),
            Some(WAITING_APPROVAL_STATUS)
        );
        assert!(
            !state.is_active_bot_responding(),
            "nothing is running under it: the card is the person's to answer"
        );

        open_thread(&mut state, "cw_2");
        assert_eq!(
            state.visible_bot_status().as_deref(),
            Some("Running commands")
        );
        assert!(state.is_active_bot_responding());
    }

    // ---- A card the MCP door raised ---------------------------------------------------------

    fn queued(thread_id: &str, call_id: &str, tool: &str) -> QueuedApproval {
        QueuedApproval {
            run_id: "run_mcp".into(),
            thread_id: thread_id.into(),
            call_id: call_id.into(),
            tool: tool.into(),
            arguments: serde_json::json!({ "path": "/etc/hosts" }),
            reason: Some("policy-approval".into()),
            why: Some("Reading a file outside the workspace.".into()),
        }
    }

    /// The door files its cards under a thread of its own, `mcp-{coworker}`. There is no
    /// conversation by that name and there should not be one: the card goes to the coworker.
    #[test]
    fn a_card_filed_under_an_mcp_thread_is_grafted_onto_its_coworkers_thread() {
        let mut state = AppState::new();
        state.conversations.push(thread(
            "cw_1",
            vec![at(message("m_said", false, "Done."), 10)],
        ));

        state.attach_queued_approval(queued("mcp-cw_1", "call_9", "read_file"));

        let parts = &state.conversations[0].messages[0].parts;
        assert!(
            matches!(parts.as_slice(), [ChatPart::Approval(spec)] if spec.call_id == "call_9"),
            "the coworker's thread is holding the card, not a thread named after the door"
        );
        assert_eq!(state.conversations.len(), 1, "no `mcp-cw_1` conversation");
    }

    /// The card knows where it came from, so answering it puts the working line on the thread
    /// it was grafted onto rather than on whatever happens to be open.
    #[test]
    fn a_queued_card_carries_the_thread_it_was_filed_under() {
        let spec = spec_from_queued(&queued("mcp-cw_1", "call_9", "read_file"));
        assert_eq!(spec.thread_id.as_deref(), Some("mcp-cw_1"));
        assert_eq!(spec.conversation_id(), Some("cw_1"));
        assert!(spec.is_mcp());

        let chat = spec_from_queued(&queued("cw_1", "call_1", USER_MACHINE_SHELL));
        assert_eq!(chat.conversation_id(), Some("cw_1"));
        assert!(!chat.is_mcp());
    }

    /// The queue says why it stopped, and the card says it back. Only a server
    /// that sends neither gets the sentence the app used to make up.
    #[test]
    fn a_queued_card_reads_back_the_reason_the_server_sent() {
        let told = spec_from_queued(&queued("mcp-cw_1", "call_9", "read_file"));
        assert_eq!(told.reason, "policy-approval");
        assert_eq!(told.why, "Reading a file outside the workspace.");

        let mut silent = queued("cw_1", "call_1", USER_MACHINE_SHELL);
        silent.reason = None;
        silent.why = Some("   ".into());
        let spec = spec_from_queued(&silent);
        assert_eq!(spec.reason, "exec-consent");
        assert_eq!(spec.why, "your machine's owner must approve this command");
    }

    /// The card the door raised is answered from the coworker's thread, and the line it leaves
    /// behind is about the call, not about this Mac.
    #[test]
    fn an_answered_mcp_card_says_which_call_was_let_through() {
        let mut state = AppState::new();
        state.conversations.push(thread(
            "cw_1",
            vec![at(message("m_said", false, "Done."), 10)],
        ));
        state.attach_queued_approval(queued("mcp-cw_1", "call_9", "read_file"));
        let spec = match &state.conversations[0].messages[0].parts[0] {
            ChatPart::Approval(spec) => spec.clone(),
            other => panic!("the card is the part on the row: {other:?}"),
        };

        state
            .approval_decisions
            .insert("call_9".into(), ApprovalDecision::AllowOnce);
        assert_eq!(
            state.approval_status_line(&spec, "Hexuria").as_deref(),
            Some("Hexuria may run read_file once.")
        );

        state
            .approval_decisions
            .insert("call_9".into(), ApprovalDecision::Denied);
        assert_eq!(
            state.approval_status_line(&spec, "Hexuria").as_deref(),
            Some("Hexuria was told no.")
        );
    }

    // ---- Reaching the gateway, and the list that depends on it ------------------------------

    fn catalogue(ids: &[&str], note: Option<&str>) -> ModelCatalogue {
        ModelCatalogue {
            models: ids
                .iter()
                .map(|id| ModelEntry {
                    id: (*id).to_string(),
                })
                .collect(),
            note: note.map(str::to_string),
        }
    }

    /// The whole of the reported bug, on the data: the gateway goes, the Model field keeps what
    /// it had and says why it may be stale, and the answer that arrives once the gateway is back
    /// refills it.
    #[test]
    fn a_gateway_outage_does_not_empty_the_model_list_and_coming_back_refills_it() {
        let mut held = ModelCatalogue::default();

        let note = apply_catalogue(&mut held, catalogue(&["xai/grok-4.6", "oag/auto"], None));
        assert_eq!(note, None);
        assert_eq!(held.models.len(), 2);

        // `/models` never fails: a gateway it cannot reach answers 200, empty, with the reason.
        let down = catalogue(
            &[],
            Some(
                "the gateway could not be reached: error sending request for url \
                 (http://127.0.0.1:29080/v1/models)",
            ),
        );
        let note = apply_catalogue(&mut held, down);
        assert_eq!(
            held.models.len(),
            2,
            "the list the app already has is still a list it can offer"
        );
        assert!(reads_as_gateway_unreachable(
            note.as_deref().expect("the server said why")
        ));
        assert!(
            held.note.is_some(),
            "and the field can say why it may be stale"
        );

        let note = apply_catalogue(
            &mut held,
            catalogue(&["xai/grok-4.6", "oag/auto", "new"], None),
        );
        assert_eq!(note, None, "nothing left to explain");
        assert_eq!(held.models.len(), 3, "the newer list is taken");
        assert_eq!(held.note, None);
    }

    /// An empty answer with nothing to explain it is a real answer — this key routes nowhere —
    /// and holding a stale list against it would be the app inventing models.
    #[test]
    fn an_empty_answer_with_no_reason_is_taken_as_the_answer() {
        let mut held = catalogue(&["xai/grok-4.6"], None);
        let note = apply_catalogue(&mut held, catalogue(&[], None));
        assert_eq!(note, None);
        assert!(held.models.is_empty());
    }

    /// A gateway that answered and had nothing to offer is not a gateway out of reach: the note
    /// belongs on the field, but nothing is worth retrying.
    #[test]
    fn a_gateway_that_answers_with_no_models_is_not_a_gateway_to_wait_for() {
        let mut held = ModelCatalogue::default();
        let note = apply_catalogue(
            &mut held,
            catalogue(
                &[],
                Some(
                    "the gateway advertises no models on this key's route — a pin can still be \
                     typed by hand",
                ),
            ),
        );
        assert!(!reads_as_gateway_unreachable(
            note.as_deref().expect("a note")
        ));
        assert!(held.note.is_some(), "the field still says what happened");
    }

    /// The app's own account of a turn that never left is one of its status lines: painted, not
    /// saved, and never handed to the model as something the coworker said.
    #[test]
    fn a_turn_that_never_left_is_a_status_line_and_not_the_coworkers_words() {
        assert!(is_status_line(TURN_UNREACHED_NOTE));
        let sent = agui_messages(&[
            message("m1", true, "open youtube"),
            message("m2", false, TURN_UNREACHED_NOTE),
        ]);
        assert_eq!(
            sent.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(),
            vec!["open youtube"]
        );
    }

    /// Only the thread's last turn is offered again. An older one has messages after it, and
    /// re-running the thread from there would answer the newest message instead.
    #[test]
    fn the_turn_offered_again_is_the_last_one_and_only_while_nothing_is_running() {
        let mut state = AppState::new();
        state.conversations.push(thread(
            "cw_1",
            vec![
                at(message("m_ask", true, "open youtube"), 10),
                at(message("m_failed", false, TURN_UNREACHED_NOTE), 20),
            ],
        ));
        state.active_conversation_id = Some("cw_1".to_string());
        assert_eq!(state.retryable_turn().as_deref(), Some("m_failed"));

        state.live_turns.insert("cw_1".to_string(), in_flight());
        assert_eq!(
            state.retryable_turn(),
            None,
            "a thread already running a turn has nothing to send again"
        );
        state.live_turns.remove("cw_1");

        state.conversations[0]
            .messages
            .push(at(message("m_next", true, "never mind"), 30));
        assert_eq!(
            state.retryable_turn(),
            None,
            "the thread has moved on, and the turn to run is not the old one"
        );
    }

    /// A run the server refused is a verdict, and it keeps the red line with the server's own
    /// sentence in it. Only a turn that never left gets the quiet line that can be answered.
    #[test]
    fn a_refused_run_still_says_why_where_a_turn_that_never_left_does_not() {
        let refused = format!("{RUN_ERROR_PREFIX}the model gateway refused: 402 spend cap");
        assert!(is_status_line(&refused));
        assert_ne!(refused, TURN_UNREACHED_NOTE);
        assert!(
            !TURN_UNREACHED_NOTE.starts_with(RUN_ERROR_PREFIX),
            "which is what keeps it out of the colour of a failure"
        );
    }

    /// The three endings a turn can have that are not the coworker speaking read as three
    /// different things, and the signed-out one is the newest of them.
    ///
    /// It was a red line with the server's sentence in it — "this turn does not say whose spend
    /// it is" — which read as a verdict about the turn and sent somebody looking at spend
    /// limits. It is now its own quiet line, distinct from the wire's, because the thing to do
    /// about it is different and the sentence is where a person learns that.
    #[test]
    fn a_turn_that_was_not_sent_says_so_without_naming_a_machine_or_a_verdict() {
        assert!(is_status_line(TURN_SIGNED_OUT_NOTE));
        assert!(
            !TURN_SIGNED_OUT_NOTE.starts_with(RUN_ERROR_PREFIX),
            "nothing about the run went wrong, so it is not painted as a failure"
        );
        assert_ne!(
            TURN_SIGNED_OUT_NOTE, TURN_UNREACHED_NOTE,
            "one is fixed by waiting and one is fixed by signing in"
        );
        assert!(
            !TURN_SIGNED_OUT_NOTE.contains("go through"),
            "which would send somebody to look at a network that is working"
        );

        // And, like every other line the app writes itself, it is never handed back to the model
        // as something the coworker said.
        let sent = agui_messages(&[
            message("m1", true, "open youtube"),
            message("m2", false, TURN_SIGNED_OUT_NOTE),
        ]);
        assert_eq!(
            sent.iter().map(|m| m.content.as_str()).collect::<Vec<_>>(),
            vec!["open youtube"]
        );
    }

    /// Both turns that never left are offered again, and nothing else is.
    ///
    /// A turn that was not sent ran nothing and decided nothing, so sending it after signing in
    /// does nothing twice — which is the whole reason it can be offered at all.
    #[test]
    fn a_turn_the_app_never_sent_is_offered_again_once_there_is_a_session() {
        assert!(is_unsent_turn_note(TURN_SIGNED_OUT_NOTE));
        assert!(is_unsent_turn_note(TURN_UNREACHED_NOTE));
        assert!(
            !is_unsent_turn_note(&format!("{RUN_ERROR_PREFIX}the model gateway refused")),
            "a run that happened and was refused is not a run to repeat"
        );
        assert!(!is_unsent_turn_note(STOPPED_TURN_NOTE));

        let mut state = AppState::new();
        state.conversations.push(thread(
            "cw_1",
            vec![
                at(message("m_ask", true, "open youtube"), 10),
                at(message("m_unsent", false, TURN_SIGNED_OUT_NOTE), 20),
            ],
        ));
        state.active_conversation_id = Some("cw_1".to_string());
        assert_eq!(state.retryable_turn().as_deref(), Some("m_unsent"));

        // Not while the app is still signed out, though: it would send nothing, and a button
        // that does nothing under a banner explaining why is worse than no button.
        state.session.note(Failure::SignedOut);
        assert_eq!(state.retryable_turn(), None);
        state.session.signed_in();
        assert_eq!(state.retryable_turn().as_deref(), Some("m_unsent"));
    }

    /// With no session, a turn is not attempted.
    ///
    /// Two ways of knowing and the app takes either: nothing in the jar to sign the turn with,
    /// or a `401` already heard back. The first is the one that was missing — the roster was on
    /// screen, the composer took the message, and the turn went out with no credential on it.
    #[test]
    fn a_turn_is_not_attempted_while_the_app_has_no_session() {
        let mut state = AppState::new();
        state.opengrok =
            Some(OpenGrokClient::new("http://127.0.0.1:1/").expect("a URL that parses"));
        assert!(
            !state.can_send_turn(),
            "an empty jar is an answer the app already has, without spending a round trip on it"
        );
        assert_eq!(
            state.session_banner(),
            None,
            "though it has not been told yet, so there is nothing on screen about it"
        );

        // Being told is what puts it on screen, and it stays there: no retry loop can help.
        state.session.note(Failure::SignedOut);
        assert!(state.is_session_expired());
        assert!(state.session_banner().is_some());
        assert!(!state.can_send_turn());

        // And a machine out of reach must not do any of that. Signing in cannot fix a router.
        let mut wire = AppState::new();
        wire.session
            .note(crate::opengrok::OpenGrokError::from_server(Some(502), "Bad Gateway").failure());
        assert!(!wire.is_session_expired());
        assert_eq!(wire.session_banner(), None);
    }

    /// Signing in clears it, and so does signing out on purpose.
    #[test]
    fn signing_in_clears_the_signed_out_state() {
        let mut state = AppState::new();
        state.session.note(Failure::SignedOut);
        assert!(state.is_session_expired());

        state.session.signed_in();
        assert!(!state.is_session_expired());
        assert_eq!(state.session_banner(), None);
        assert!(
            state.session.may_send(true),
            "and turns can be sent again the moment there is something to send them with"
        );
    }
}
