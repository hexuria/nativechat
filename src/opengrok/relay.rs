//! This Mac as the person's relay: the server sends a turn's model calls down a stream to the Mac
//! that holds it, and the Mac asks its own opencodex and streams the answer back.
//!
//! Before the relay, the person's own plan worked only where the server ran on the same machine
//! as opencodex, because the server calls opencodex on its own loopback. The relay lifts that:
//! the Mac opens `GET /inference-relay/requests` with its local-exec machine token, the server
//! sends each model call down it as a frame, and the Mac calls opencodex at an address and with a
//! key the person gave on this Mac, then posts opencodex's answer to
//! `POST /inference-relay/responses/{requestId}` as it comes. The owner approved the rule this
//! bends: this background half forwards model calls; the window still never calls a model. Every
//! shape here is opengrok-server #292's, as built in opengrok-server PR #298, branch mac-relay
//! 5359e34, recorded at 07a951b, not yet on main (`RelayFrame` in
//! `crates/opengrok-wire/src/relay.rs`, the two routes in
//! `crates/opengrok-server/src/inference.rs`), and the conformance ledger reads every recorded
//! frame and answer with this code.
//!
//! Frames never carry a URL or a key. The address is this Mac's and must be this Mac
//! ([`OpencodexAddress`]); the key comes from the Keychain and is never logged, shown, or written
//! anywhere else ([`RelayKey`]). Each call's model is held to the same allowlist the server holds
//! it to, so a Mac asked for something its person's subscription may not answer refuses it
//! without calling opencodex.
//!
//! The Mac opens the stream only while the person has switched it on. After the server says
//! another Mac took over (`replaced`), or refuses this Mac's token, it stops for good: it does not
//! fight the other Mac for the stream, or knock on a door that has said no, until it is switched
//! off and on again.

use std::collections::HashMap;
use std::fmt;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures::{Stream, StreamExt};
use reqwest::header::ACCEPT;
use serde_json::Value;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use super::client::OpenGrokClient;
use super::error::OpenGrokError;
use super::inference::{is_loopback, is_subscription_model};
use super::local_exec::MachineCredential;

/// The most of a sentence about a failure the Mac sends the server, in characters. opencodex can
/// answer a failure with a page of HTML; the person reads one line.
const SENTENCE_CHARS: usize = 400;

/// The most of a failed answer's body read to find its sentence.
const REFUSAL_BYTES: usize = 16 * 1024;

/// The most of opencodex's model list the Mac passes on. A list is a few kilobytes; one that is
/// not is not a list.
const MODELS_BYTES: usize = 1024 * 1024;

/// The most one line of the relay stream may hold. A call carries the whole conversation, which
/// can be large, but not without end.
const LINE_BYTES: usize = 64 * 1024 * 1024;

/// What the Mac says when the server turns its token away, whether opening the stream or
/// answering a call. It stops: asking again with the same token would be refused again.
pub const TOKEN_REFUSED: &str =
    "The server turned this Mac's token away. Turn Answer with this Mac off and on to try again.";

/// What it says while the server does not answer the stream, or goes quiet on it.
pub const SERVER_QUIET: &str = "The server went quiet. Trying again…";

/// What it says while the server cannot be reached.
pub const SERVER_UNREACHED: &str = "Can't reach the server. Trying again…";

/// What it says to a server from before the relay, which answers the stream with a bare 404.
pub const SERVER_WITHOUT_RELAY: &str = "This server can't take replies from a Mac yet.";

/// A key for this Mac's opencodex, as the person typed it. It is kept in the Keychain, and in
/// memory while the relay runs, and never logged, shown, or written anywhere else: it is never
/// cloned (the type is not `Clone`; the relay shares one behind an `Arc`), and its `Debug` is a
/// mark, so no log or panic message ever carries it.
#[derive(PartialEq, Eq)]
pub struct RelayKey(String);

impl RelayKey {
    /// A key, or `None` for a field left blank, which is not a key and must not replace one.
    pub fn new(typed: &str) -> Option<Self> {
        let typed = typed.trim();
        (!typed.is_empty()).then(|| Self(typed.to_string()))
    }

    /// The key itself, for the two places that need it: the Keychain, and the `Authorization`
    /// header of a call to opencodex.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for RelayKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("RelayKey(«redacted»)")
    }
}

/// Where this Mac's opencodex listens: an `http` or `https` address on this Mac, with no path,
/// query or credentials, kept as its origin (`http://127.0.0.1:8080`). The Mac calls only what is
/// on it, as the server does with the plan on its own machine, so a key sent with a call never
/// leaves the Mac.
#[derive(Clone, PartialEq, Eq)]
pub struct OpencodexAddress(url::Url);

impl OpencodexAddress {
    /// The address as typed, checked, or why it is not one opencodex on this Mac can have.
    pub fn parse(typed: &str) -> Result<Self, &'static str> {
        const NOT_AN_ADDRESS: &str =
            "That isn't an address. Give opencodex's, like http://127.0.0.1:8080.";
        let url = url::Url::parse(typed.trim()).map_err(|_| NOT_AN_ADDRESS)?;
        if !matches!(url.scheme(), "http" | "https") || url.host().is_none() {
            return Err(NOT_AN_ADDRESS);
        }
        if !is_loopback(&url) {
            return Err("opencodex has to be on this Mac: use 127.0.0.1, [::1] or localhost.");
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err("Give the address without a name or password in it.");
        }
        if !matches!(url.path(), "" | "/") || url.query().is_some() || url.fragment().is_some() {
            return Err("Give the address with no path, like http://127.0.0.1:8080.");
        }
        let origin =
            url::Url::parse(&url.origin().ascii_serialization()).map_err(|_| NOT_AN_ADDRESS)?;
        Ok(Self(origin))
    }

    /// As the field shows it and the prefs keep it: `http://127.0.0.1:8080`.
    pub fn as_str(&self) -> &str {
        self.0.as_str().trim_end_matches('/')
    }

    /// The host and port a sentence names: `127.0.0.1:8080`.
    pub fn host_port(&self) -> String {
        let host = self.0.host_str().unwrap_or("localhost");
        match self.0.port_or_known_default() {
            Some(port) => format!("{host}:{port}"),
            None => host.to_string(),
        }
    }

    fn endpoint(&self, path: &str) -> url::Url {
        let mut url = self.0.clone();
        url.set_path(path);
        url
    }
}

impl fmt::Debug for OpencodexAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "OpencodexAddress({})", self.as_str())
    }
}

/// What the relay calls opencodex with: where it listens, and the key it wants, if it wants one.
#[derive(Clone)]
pub struct RelayTarget {
    pub address: OpencodexAddress,
    pub key: Option<Arc<RelayKey>>,
}

impl fmt::Debug for RelayTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RelayTarget")
            .field("address", &self.address)
            .field("key", &self.key.as_ref().map(|_| "«redacted»"))
            .finish()
    }
}

/// How long the relay waits. The server pings every fifteen seconds, so three missed pings is a
/// stream gone quiet, however it looks from here. The wait between attempts doubles after each
/// one that fails, up to a ceiling, and starts again from the floor once a stream is answering.
#[derive(Debug, Clone, Copy)]
pub struct RelayTimings {
    pub quiet: Duration,
    pub first_wait: Duration,
    pub longest_wait: Duration,
}

impl Default for RelayTimings {
    fn default() -> Self {
        Self {
            quiet: Duration::from_secs(45),
            first_wait: Duration::from_secs(1),
            longest_wait: Duration::from_secs(30),
        }
    }
}

/// Where the relay stands, as Settings → Reply source says it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum RelayStatus {
    /// Not running: switched off, or never started.
    #[default]
    Off,
    /// Opening the stream, or opening it again after it dropped.
    Connecting,
    /// This Mac is the relay: the server said so on the stream (`ready`).
    Answering,
    /// Another Mac opened the stream after this one, and the server gave it the relay.
    Replaced,
    /// Something is wrong, in a sentence: while it is trying again, and when it has stopped.
    Error(String),
}

/// The relay's word on itself, as the window reads it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelayReport {
    pub status: RelayStatus,
    /// Model calls this Mac is answering right now.
    pub in_flight: usize,
    /// It has stopped for good: another Mac took over, or the server turned the token away. Only
    /// switching it off and on again starts it.
    pub halted: bool,
}

/// One frame off the relay stream (`data:` holds one JSON object each). A frame of a type this
/// app has not heard of, or without what its type needs, is [`RelayFrame::Other`] and passed
/// over, never a failed stream.
#[derive(Debug, Clone, PartialEq)]
pub enum RelayFrame {
    /// Always first on a stream: this Mac is the relay now.
    Ready {
        machine_id: String,
    },
    /// Another stream took over. The server closes this one.
    Replaced,
    /// A model call: `request` is an OpenAI `chat/completions` body with `stream: true`.
    Infer {
        request_id: String,
        run_id: String,
        model: String,
        request: Value,
    },
    /// The models this Mac's opencodex lists, for the server's `/models`.
    Models {
        request_id: String,
    },
    /// The server gave up on a call.
    Cancel {
        request_id: String,
    },
    /// Every fifteen seconds, to say the stream is alive.
    Ping,
    Other,
}

impl RelayFrame {
    pub fn from_value(frame: &Value) -> Self {
        let text = |key: &str| {
            frame
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
        };
        let request_id = text("requestId");
        match (frame.get("type").and_then(Value::as_str), request_id) {
            (Some("ready"), _) => Self::Ready {
                machine_id: text("machineId").unwrap_or_default(),
            },
            (Some("replaced"), _) => Self::Replaced,
            (Some("ping"), _) => Self::Ping,
            // A call with no body still names the call, so it is answered, with why nothing was
            // asked, rather than left to the server's clock.
            (Some("infer"), Some(request_id)) => Self::Infer {
                request_id,
                run_id: text("runId").unwrap_or_default(),
                model: text("model").unwrap_or_default(),
                request: frame.get("request").cloned().unwrap_or(Value::Null),
            },
            (Some("models"), Some(request_id)) => Self::Models { request_id },
            (Some("cancel"), Some(request_id)) => Self::Cancel { request_id },
            _ => Self::Other,
        }
    }
}

/// What the Mac posts for a call (`POST /inference-relay/responses/{requestId}`).
pub(crate) enum RelayAnswer {
    /// opencodex's `text/event-stream` body, passed on as it comes rather than gathered first:
    /// a reply streams to the person as it did from the server's own machine.
    Stream(reqwest::Body),
    /// opencodex's `/v1/models` JSON.
    Models(Value),
    /// What went wrong, in a sentence, as `{"error": …}`.
    Error(String),
}

/// What the server made of an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelayAnswered {
    /// `204`: taken.
    Taken,
    /// `404`: the server has no such call, or gave up on it. As good as a cancel.
    Gone,
    /// `409`: somebody answered it already.
    AlreadyAnswered,
    /// `401`: the server no longer takes this Mac's token.
    TokenRefused,
    /// `413`: the answer ran past the server's 32 MiB (`MAX_ANSWER_BYTES` in opengrok-server's
    /// `crates/opengrok-harness/src/relay.rs`), so the server cut it off there and the run it was
    /// for ended as the Mac's failure. A failed answer, with nothing to send again.
    TooLarge,
}

/// The calls this Mac is answering, by the `requestId` their frame carried, so that a `cancel`
/// naming one can stop it. A call takes itself out when it is answered. Kept across reconnects:
/// the answer goes back by its id, whichever stream the call came in on.
type Running = Arc<Mutex<HashMap<String, JoinHandle<()>>>>;

/// The running relay. Dropping it stops it, and every call it was answering: switched off,
/// signed out, or the app quitting.
pub struct RelayHandle {
    reports: watch::Receiver<RelayReport>,
    target: watch::Sender<RelayTarget>,
    running: Running,
    serving: JoinHandle<()>,
}

impl RelayHandle {
    /// Where the relay stands now.
    pub fn report(&self) -> RelayReport {
        self.reports.borrow().clone()
    }

    /// Every change to where it stands, for the window to follow.
    pub fn reports(&self) -> watch::Receiver<RelayReport> {
        self.reports.clone()
    }

    /// Call opencodex at another address from the next call on. A call already out keeps the
    /// one it was sent to.
    pub fn readdress(&self, address: OpencodexAddress) {
        self.target.send_modify(|target| target.address = address);
    }

    /// Call opencodex with another key, or none, from the next call on.
    pub fn rekey(&self, key: Option<Arc<RelayKey>>) {
        self.target.send_modify(|target| target.key = key);
    }
}

impl Drop for RelayHandle {
    fn drop(&mut self) {
        self.serving.abort();
        for (_, call) in self
            .running
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .drain()
        {
            call.abort();
        }
    }
}

impl fmt::Debug for RelayHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RelayHandle")
            .field("report", &*self.reports.borrow())
            .field("target", &*self.target.borrow())
            .finish()
    }
}

/// Start answering for the person: open the stream with this Mac's machine token and serve it
/// until the handle is dropped, the server hands the relay to another Mac, or it refuses the
/// token. On the tokio runtime, which the app enters at startup.
pub fn start_relay(
    client: OpenGrokClient,
    machine: MachineCredential,
    target: RelayTarget,
    timings: RelayTimings,
) -> RelayHandle {
    let (report, reports) = watch::channel(RelayReport {
        status: RelayStatus::Connecting,
        ..RelayReport::default()
    });
    let (target_tx, target_rx) = watch::channel(target);
    let (halt, halts) = mpsc::unbounded_channel();
    let running = Running::default();
    let relay = Arc::new(Relay {
        client,
        machine,
        target: target_rx,
        opencodex: opencodex_client(),
        report,
        running: Arc::clone(&running),
        halt,
    });
    let serving = tokio::spawn(serve(relay, halts, timings));
    RelayHandle {
        reports,
        target: target_tx,
        running,
        serving,
    }
}

/// An HTTP client for this Mac's opencodex alone. It follows no redirect, so a key sent to
/// opencodex is never sent on somewhere else, and it goes through no proxy, so a call to this Mac
/// stays on it. A connection is given ten seconds; a reply as long as it takes, since the server
/// keeps its own clock on the call and cancels it when that runs out.
fn opencodex_client() -> reqwest::Client {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(10))
        .build()
        .unwrap_or_else(|_| reqwest::Client::new())
}

struct Relay {
    client: OpenGrokClient,
    machine: MachineCredential,
    target: watch::Receiver<RelayTarget>,
    opencodex: reqwest::Client,
    report: watch::Sender<RelayReport>,
    running: Running,
    /// An answer the server refused for the token tells the stream to stop.
    halt: mpsc::UnboundedSender<()>,
}

/// How one stream ended.
enum Ended {
    /// It was answering and then dropped, or went quiet: open another after the shortest wait.
    Answered,
    /// It never got as far as answering, and why.
    Failed(String),
    /// Another Mac took over.
    Replaced,
    /// The server turned the token away.
    TokenRefused,
}

async fn serve(relay: Arc<Relay>, mut halts: mpsc::UnboundedReceiver<()>, timings: RelayTimings) {
    let mut wait = timings.first_wait;
    loop {
        match relay.stream_once(&mut halts, timings.quiet).await {
            Ended::Replaced => {
                relay.set_status(RelayStatus::Replaced, true);
                return;
            }
            Ended::TokenRefused => {
                relay.set_status(RelayStatus::Error(TOKEN_REFUSED.into()), true);
                return;
            }
            Ended::Answered => {
                wait = timings.first_wait;
                relay.set_status(RelayStatus::Connecting, false);
            }
            Ended::Failed(why) => relay.set_status(RelayStatus::Error(why), false),
        }
        tokio::select! {
            _ = tokio::time::sleep(wait) => {}
            Some(()) = halts.recv() => {
                relay.set_status(RelayStatus::Error(TOKEN_REFUSED.into()), true);
                return;
            }
        }
        wait = (wait * 2).min(timings.longest_wait);
    }
}

impl Relay {
    fn set_status(&self, status: RelayStatus, halted: bool) {
        self.report.send_if_modified(|report| {
            let changed = report.status != status || report.halted != halted;
            report.status = status;
            report.halted = halted;
            changed
        });
    }

    fn count_in_flight(&self, calls: usize) {
        self.report.send_if_modified(|report| {
            let changed = report.in_flight != calls;
            report.in_flight = calls;
            changed
        });
    }

    async fn stream_once(
        self: &Arc<Self>,
        halts: &mut mpsc::UnboundedReceiver<()>,
        quiet: Duration,
    ) -> Ended {
        let opened = tokio::time::timeout(
            quiet,
            self.client.open_inference_relay(self.machine.token()),
        )
        .await;
        let response = match opened {
            Err(_) => return Ended::Failed(SERVER_QUIET.into()),
            Ok(Err(error)) if token_turned_away(&error) => return Ended::TokenRefused,
            Ok(Err(error)) => return Ended::Failed(stream_refusal(&error)),
            Ok(Ok(response)) => response,
        };
        let mut frames = SseFrames::new(response.bytes_stream());
        let mut answering = false;
        loop {
            let next = tokio::select! {
                Some(()) = halts.recv() => return Ended::TokenRefused,
                next = tokio::time::timeout(quiet, frames.next_frame()) => next,
            };
            let frame = match next {
                Ok(Some(Ok(frame))) => RelayFrame::from_value(&frame),
                // Quiet past three pings, closed, or broken: gone either way.
                Err(_) | Ok(None) | Ok(Some(Err(()))) => {
                    return if answering {
                        Ended::Answered
                    } else {
                        Ended::Failed(SERVER_QUIET.into())
                    };
                }
            };
            match frame {
                RelayFrame::Ready { .. } => {
                    answering = true;
                    self.set_status(RelayStatus::Answering, false);
                }
                RelayFrame::Replaced => return Ended::Replaced,
                frame => self.take_frame(frame),
            }
        }
    }

    /// A call off the stream: answer it in a task of its own, so calls run side by side; or stop
    /// the one a `cancel` names. Anything else is not a call.
    fn take_frame(self: &Arc<Self>, frame: RelayFrame) {
        match frame {
            RelayFrame::Infer {
                request_id,
                model,
                request,
                ..
            } => self.answer_in_turn(request_id, move |relay| async move {
                relay.infer(model, request).await
            }),
            RelayFrame::Models { request_id } => {
                self.answer_in_turn(request_id, |relay| async move { relay.list_models().await });
            }
            RelayFrame::Cancel { request_id } => self.cancel(&request_id),
            RelayFrame::Ready { .. }
            | RelayFrame::Replaced
            | RelayFrame::Ping
            | RelayFrame::Other => {}
        }
    }

    fn answer_in_turn<F, Fut>(self: &Arc<Self>, request_id: String, ask: F)
    where
        F: FnOnce(Arc<Relay>) -> Fut + Send + 'static,
        Fut: std::future::Future<Output = RelayAnswer> + Send + 'static,
    {
        // Held across the spawn, so a call answered at once cannot take itself out before it
        // has been put in and leave a stale entry behind.
        let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
        // The server gives every call an id of its own, so one already running is that call
        // again. Answering it twice is not what anybody asked for.
        if running.contains_key(&request_id) {
            return;
        }
        let relay = Arc::clone(self);
        let id = request_id.clone();
        let call = tokio::spawn(async move {
            let answer = ask(Arc::clone(&relay)).await;
            relay.answer(&id, answer).await;
            let left = {
                let mut running = relay.running.lock().unwrap_or_else(PoisonError::into_inner);
                running.remove(&id);
                running.len()
            };
            relay.count_in_flight(left);
        });
        running.insert(request_id, call);
        self.count_in_flight(running.len());
    }

    /// The server gave up on a call: stop it there and then. Aborting its task drops the call to
    /// opencodex, which stops generating, and the upload to the server, which asked for this.
    /// A cancel for a call already answered finds nothing, which is right.
    fn cancel(&self, request_id: &str) {
        let left = {
            let mut running = self.running.lock().unwrap_or_else(PoisonError::into_inner);
            if let Some(call) = running.remove(request_id) {
                call.abort();
            }
            running.len()
        };
        self.count_in_flight(left);
    }

    /// One model call: opencodex's stream, or why there is none.
    async fn infer(&self, model: String, mut request: Value) -> RelayAnswer {
        let target = self.target.borrow().clone();
        let Some(body) = request.as_object_mut() else {
            return RelayAnswer::Error("The server's call carried no request to send.".into());
        };
        let named = body
            .get("model")
            .and_then(Value::as_str)
            .map(str::to_string);
        // Defence in depth: the server holds the model to the allowlist, and so does the Mac,
        // every model the call names, before opencodex hears of it.
        for asked in [Some(model.as_str()), named.as_deref()]
            .into_iter()
            .flatten()
            .filter(|asked| !asked.trim().is_empty())
        {
            if !is_subscription_model(asked) {
                return RelayAnswer::Error(bounded(&format!(
                    "{asked} isn't a model this Mac answers with your subscription: only \
                     OpenAI's (gpt-*, o1, o3, o4, codex) and xAI's (grok-*) may use it."
                )));
            }
        }
        if named.is_none() {
            if model.trim().is_empty() {
                return RelayAnswer::Error("The server's call named no model.".into());
            }
            body.insert("model".into(), Value::String(model));
        }
        let mut call = self
            .opencodex
            .post(target.address.endpoint("/v1/chat/completions"))
            .header(ACCEPT, "text/event-stream")
            .json(&request);
        if let Some(key) = &target.key {
            call = call.bearer_auth(key.expose());
        }
        match call.send().await {
            Err(error) => RelayAnswer::Error(unreachable_sentence(&target, &error)),
            Ok(response) if response.status().is_success() => {
                RelayAnswer::Stream(reqwest::Body::wrap_stream(response.bytes_stream()))
            }
            Ok(response) => RelayAnswer::Error(refusal_sentence(&target, response).await),
        }
    }

    /// opencodex's model list, passed on as the JSON it is.
    async fn list_models(&self) -> RelayAnswer {
        let target = self.target.borrow().clone();
        let mut call = self.opencodex.get(target.address.endpoint("/v1/models"));
        if let Some(key) = &target.key {
            call = call.bearer_auth(key.expose());
        }
        let response = match call.send().await {
            Err(error) => return RelayAnswer::Error(unreachable_sentence(&target, &error)),
            Ok(response) => response,
        };
        if !response.status().is_success() {
            return RelayAnswer::Error(refusal_sentence(&target, response).await);
        }
        let listed = read_capped(response, MODELS_BYTES)
            .await
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        match listed {
            Some(models) => RelayAnswer::Models(models),
            None => RelayAnswer::Error(format!(
                "opencodex at {} listed its models in a shape this Mac can't read.",
                target.address.host_port()
            )),
        }
    }

    async fn answer(&self, request_id: &str, answer: RelayAnswer) {
        match self
            .client
            .answer_inference_relay(self.machine.token(), request_id, answer)
            .await
        {
            // A call the server gave up on, or never had, is as good as cancelled: nothing to
            // say. One answered already was somebody's answer, and that one stands.
            Ok(RelayAnswered::Taken | RelayAnswered::Gone | RelayAnswered::AlreadyAnswered) => {}
            Ok(RelayAnswered::TokenRefused) => {
                let _ = self.halt.send(());
            }
            // The run already says so, in the server's words; the Mac has nothing to add, and
            // the same answer would be cut off again.
            Ok(RelayAnswered::TooLarge) => {
                eprintln!("NativeChat relay: the server cut off an answer past its 32 MiB");
            }
            Err(error) => {
                eprintln!("NativeChat relay: an answer did not reach the server: {error}");
            }
        }
    }
}

/// The server refused the stream because it no longer takes this Mac's token: a `401`, which the
/// relay stops for rather than knocking again with a token that would be turned away again.
pub(super) fn token_turned_away(error: &OpenGrokError) -> bool {
    error.status == Some(401)
}

/// Why the stream did not open, in a sentence, while the relay tries again.
pub(super) fn stream_refusal(error: &OpenGrokError) -> String {
    if error.unreachable().is_some() {
        return SERVER_UNREACHED.to_string();
    }
    // A bare 404 is a server from before the relay; its own 404 is something else.
    if error.is_not_found() && !error.written_by_opengrok() {
        return SERVER_WITHOUT_RELAY.to_string();
    }
    let said = error.message.trim();
    if said.is_empty() {
        format!("The server refused the relay ({}). Trying again…", error)
    } else {
        bounded(&format!(
            "The server refused the relay: {said} Trying again…"
        ))
    }
}

/// Why opencodex could not be asked at all, in a sentence.
fn unreachable_sentence(target: &RelayTarget, error: &reqwest::Error) -> String {
    let at = target.address.host_port();
    let said = if error.is_connect() {
        format!("opencodex isn't running at {at}.")
    } else if error.is_timeout() {
        format!("opencodex at {at} didn't answer in time.")
    } else {
        format!("Couldn't reach opencodex at {at}.")
    };
    scrubbed(target, bounded(&said))
}

/// What opencodex said when it refused a call, in a sentence: the message its body carries, in
/// OpenAI's shape or its own, or the first line of whatever it sent.
async fn refusal_sentence(target: &RelayTarget, response: reqwest::Response) -> String {
    let status = response.status().as_u16();
    let body = read_capped(response, REFUSAL_BYTES)
        .await
        .unwrap_or_default();
    let text = String::from_utf8_lossy(&body);
    let said = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|body| {
            [
                body.pointer("/error/message"),
                body.get("error"),
                body.get("message"),
                body.get("detail"),
            ]
            .into_iter()
            .flatten()
            .find_map(Value::as_str)
            .map(str::to_string)
        })
        .or_else(|| {
            text.lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_default();
    let said = said.trim();
    let sentence = if said.is_empty() {
        format!("opencodex answered {status}.")
    } else {
        format!("opencodex answered {status}: {said}")
    };
    scrubbed(target, bounded(&sentence))
}

/// A sentence with the key taken out, should opencodex ever say it back: it goes to the server,
/// and the key goes nowhere but opencodex.
fn scrubbed(target: &RelayTarget, sentence: String) -> String {
    match &target.key {
        Some(key) if !key.expose().is_empty() => sentence.replace(key.expose(), "«redacted»"),
        _ => sentence,
    }
}

/// At most [`SENTENCE_CHARS`] of a sentence, cut on a character and marked as cut.
fn bounded(sentence: &str) -> String {
    if sentence.chars().count() <= SENTENCE_CHARS {
        return sentence.to_string();
    }
    let mut cut: String = sentence.chars().take(SENTENCE_CHARS - 1).collect();
    cut.push('…');
    cut
}

/// A response's body, or `None` once it passes `cap` bytes or breaks.
async fn read_capped(response: reqwest::Response, cap: usize) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    let mut chunks = response.bytes_stream();
    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.ok()?;
        if body.len() + chunk.len() > cap {
            return None;
        }
        body.extend_from_slice(&chunk);
    }
    Some(body)
}

/// The relay stream's frames, one per `data:` line, read off the bytes as they come. Lines are
/// split on the bytes rather than on text decoded chunk by chunk, so a character cut in two by
/// the network arrives whole in the call it belongs to.
struct SseFrames<S> {
    bytes: S,
    buffer: Vec<u8>,
}

impl<S, B> SseFrames<S>
where
    S: Stream<Item = reqwest::Result<B>> + Unpin,
    B: AsRef<[u8]>,
{
    fn new(bytes: S) -> Self {
        Self {
            bytes,
            buffer: Vec::new(),
        }
    }

    /// The next frame; `Some(Err(()))` for a stream that broke, and `None` for one that ended.
    /// A `data:` line that is not JSON, a comment, or any other field is passed over.
    async fn next_frame(&mut self) -> Option<Result<Value, ()>> {
        loop {
            if let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
                let line: Vec<u8> = self.buffer.drain(..=end).collect();
                if let Some(frame) = data_frame(&line) {
                    return Some(Ok(frame));
                }
                continue;
            }
            match self.bytes.next().await? {
                Ok(chunk) => {
                    self.buffer.extend_from_slice(chunk.as_ref());
                    if self.buffer.len() > LINE_BYTES {
                        return Some(Err(()));
                    }
                }
                Err(_) => return Some(Err(())),
            }
        }
    }
}

/// The JSON a `data:` line holds, if it is one that holds JSON.
pub(super) fn data_frame(line: &[u8]) -> Option<Value> {
    let line = line.strip_suffix(b"\n").unwrap_or(line);
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    let data = line.strip_prefix(b"data:")?;
    let data = std::str::from_utf8(data).ok()?.trim();
    serde_json::from_str(data).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::Instant;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TOKEN: &str = "tok_machine";
    const KEY: &str = "opencodex-test-key-4f2a";

    fn machine() -> MachineCredential {
        MachineCredential::new("mac_1", TOKEN)
    }

    fn target(address: &str, key: Option<&str>) -> RelayTarget {
        RelayTarget {
            address: OpencodexAddress::parse(address).expect("an address on this Mac"),
            key: key.and_then(RelayKey::new).map(Arc::new),
        }
    }

    fn quick() -> RelayTimings {
        RelayTimings {
            quiet: Duration::from_secs(5),
            first_wait: Duration::from_millis(50),
            longest_wait: Duration::from_millis(200),
        }
    }

    fn sse(frames: &[Value]) -> String {
        frames
            .iter()
            .map(|frame| format!("data: {frame}\n\n"))
            .collect()
    }

    fn infer(request_id: &str, model: &str) -> Value {
        json!({
            "type": "infer", "requestId": request_id, "runId": "run_1", "model": model,
            "request": {
                "model": model, "stream": true,
                "messages": [{"role": "user", "content": "Say hello to Zoë — 你好"}]
            }
        })
    }

    /// The relay, not started, for a test to hand frames to one at a time.
    fn relay(server: &MockServer, target: RelayTarget) -> Arc<Relay> {
        let (report, _) = watch::channel(RelayReport::default());
        let (_, target) = watch::channel(target);
        let (halt, _) = mpsc::unbounded_channel();
        Arc::new(Relay {
            client: OpenGrokClient::new(&server.uri()).unwrap(),
            machine: machine(),
            target,
            opencodex: opencodex_client(),
            report,
            running: Running::default(),
            halt,
        })
    }

    async fn until(what: &str, mut done: impl AsyncFnMut() -> bool) {
        let started = Instant::now();
        while !done().await {
            assert!(started.elapsed() < Duration::from_secs(10), "{what}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn answers_to(server: &MockServer, request_id: &str) -> Vec<wiremock::Request> {
        let at = format!("/inference-relay/responses/{request_id}");
        server
            .received_requests()
            .await
            .expect("the recorder is on")
            .into_iter()
            .filter(|request| request.method.as_str() == "POST" && request.url.path() == at)
            .collect()
    }

    /// Every frame of the contract reads as itself; a type this app has not heard of, one without
    /// the id its type needs, and a line that is not an object at all are passed over, never a
    /// broken stream.
    #[test]
    fn the_frames_read_as_the_contract_writes_them() {
        assert_eq!(
            RelayFrame::from_value(&json!({"type": "ready", "machineId": "mac_1"})),
            RelayFrame::Ready {
                machine_id: "mac_1".into()
            }
        );
        assert_eq!(
            RelayFrame::from_value(&json!({"type": "replaced"})),
            RelayFrame::Replaced
        );
        assert_eq!(
            RelayFrame::from_value(&infer("req_1", "gpt-5-codex")),
            RelayFrame::Infer {
                request_id: "req_1".into(),
                run_id: "run_1".into(),
                model: "gpt-5-codex".into(),
                request: infer("req_1", "gpt-5-codex")["request"].clone(),
            }
        );
        assert_eq!(
            RelayFrame::from_value(&json!({"type": "models", "requestId": "req_2"})),
            RelayFrame::Models {
                request_id: "req_2".into()
            }
        );
        assert_eq!(
            RelayFrame::from_value(&json!({"type": "cancel", "requestId": "req_1"})),
            RelayFrame::Cancel {
                request_id: "req_1".into()
            }
        );
        assert_eq!(
            RelayFrame::from_value(&json!({"type": "ping"})),
            RelayFrame::Ping
        );
        for other in [
            json!({"type": "stretch", "requestId": "req_3"}),
            json!({"type": "infer", "model": "gpt-5-codex"}),
            json!({"type": "cancel"}),
            json!({"type": "models", "requestId": "  "}),
            json!({"requestId": "req_4"}),
            json!("ready"),
            json!(null),
        ] {
            assert_eq!(RelayFrame::from_value(&other), RelayFrame::Other, "{other}");
        }
        // A call without a body is still a call, answered with why nothing was asked.
        assert!(matches!(
            RelayFrame::from_value(&json!({"type": "infer", "requestId": "req_5"})),
            RelayFrame::Infer { request, .. } if request.is_null()
        ));
    }

    /// Only an address on this Mac is one: loopback by number or by name, with no path, query or
    /// credentials, kept as its origin. Anything else is refused with why.
    #[test]
    fn opencodex_is_called_only_on_this_mac() {
        for (typed, kept, named) in [
            (
                "http://127.0.0.1:8080",
                "http://127.0.0.1:8080",
                "127.0.0.1:8080",
            ),
            (
                "  http://localhost:9090/  ",
                "http://localhost:9090",
                "localhost:9090",
            ),
            ("https://[::1]:8443", "https://[::1]:8443", "[::1]:8443"),
            ("http://127.0.0.1", "http://127.0.0.1", "127.0.0.1:80"),
        ] {
            let address = OpencodexAddress::parse(typed).expect(typed);
            assert_eq!(
                (address.as_str(), address.host_port().as_str()),
                (kept, named)
            );
        }
        for elsewhere in [
            "http://192.168.1.5:8080",
            "https://opencodex.example.com",
            "http://0.0.0.0:8080",
            "http://127.0.0.1:8080/v1",
            "http://127.0.0.1:8080?x=1",
            "http://me:pw@127.0.0.1:8080",
            "ftp://127.0.0.1",
            "127.0.0.1:8080",
            "",
        ] {
            assert!(OpencodexAddress::parse(elsewhere).is_err(), "{elsewhere:?}");
        }
    }

    /// The key is never in any `Debug`: the key's, the target's, the handle's, or a sentence
    /// built from opencodex's answer, which has it taken out should opencodex say it back.
    #[tokio::test]
    async fn the_key_never_shows_in_any_debug_output() {
        let key = RelayKey::new(KEY).unwrap();
        assert!(!format!("{key:?}").contains(KEY));
        let target = target("http://127.0.0.1:8080", Some(KEY));
        let printed = format!("{target:?}");
        assert!(
            !printed.contains(KEY) && printed.contains("redacted"),
            "{printed}"
        );
        let server = MockServer::start().await;
        let handle = start_relay(
            OpenGrokClient::new(&server.uri()).unwrap(),
            machine(),
            target.clone(),
            quick(),
        );
        let printed = format!("{handle:?}");
        assert!(!printed.contains(KEY), "{printed}");
        assert!(
            !format!("{:?}", machine()).contains(TOKEN),
            "nor the machine token"
        );
        assert_eq!(
            scrubbed(&target, format!("opencodex answered 401: bad key {KEY}")),
            "opencodex answered 401: bad key «redacted»"
        );
        drop(handle);
    }

    /// A call is asked of opencodex with the key, as the server sent it, and opencodex's answer
    /// reaches the server byte for byte, characters outside ASCII included, streamed, marked as
    /// the event stream it is, with the machine token.
    #[tokio::test]
    async fn a_call_is_piped_to_the_server_as_opencodex_answered_it() {
        let server = MockServer::start().await;
        let opencodex = MockServer::start().await;
        let answered = "data: {\"choices\":[{\"delta\":{\"content\":\"Hello, Zoë — 你好\"}}]}\n\n\
                        data: [DONE]\n\n";
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .and(header("authorization", format!("Bearer {KEY}").as_str()))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(answered.as_bytes().to_vec(), "text/event-stream"),
            )
            .expect(1)
            .mount(&opencodex)
            .await;
        Mock::given(method("POST"))
            .and(path("/inference-relay/responses/req_1"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let relay = relay(&server, target(&opencodex.uri(), Some(KEY)));

        let frame = infer("req_1", "gpt-5-codex");
        relay.take_frame(RelayFrame::from_value(&frame));
        until("the answer reached the server", async || {
            !answers_to(&server, "req_1").await.is_empty()
        })
        .await;

        let posted = &answers_to(&server, "req_1").await[0];
        assert_eq!(
            posted.body,
            answered.as_bytes(),
            "the bytes are opencodex's"
        );
        let header = |name: &str| {
            posted
                .headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .unwrap_or_default()
                .to_string()
        };
        assert_eq!(header("content-type"), "text/event-stream");
        assert_eq!(header("authorization"), format!("Bearer {TOKEN}"));
        let asked = &opencodex.received_requests().await.unwrap()[0];
        assert_eq!(
            serde_json::from_slice::<Value>(&asked.body).unwrap(),
            frame["request"],
            "opencodex is asked what the server sent"
        );
        until("the call took itself out", async || {
            relay.running.lock().unwrap().is_empty()
        })
        .await;
        assert_eq!(relay.report.borrow().in_flight, 0);
    }

    /// A cancel stops the call it names there and then: opencodex's connection is dropped, and
    /// nothing is posted for it.
    #[tokio::test]
    async fn a_cancel_stops_the_call_it_names() {
        let server = MockServer::start().await;
        let opencodex = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw("data: [DONE]\n\n", "text/event-stream")
                    .set_delay(Duration::from_secs(3)),
            )
            .mount(&opencodex)
            .await;
        Mock::given(method("POST"))
            .and(path("/inference-relay/responses/req_1"))
            .respond_with(ResponseTemplate::new(204))
            .expect(0)
            .mount(&server)
            .await;
        let relay = relay(&server, target(&opencodex.uri(), None));

        relay.take_frame(RelayFrame::from_value(&infer("req_1", "gpt-5-codex")));
        until("opencodex was asked", async || {
            !opencodex.received_requests().await.unwrap().is_empty()
        })
        .await;
        assert_eq!(relay.report.borrow().in_flight, 1);
        relay.take_frame(RelayFrame::from_value(
            &json!({"type": "cancel", "requestId": "req_1"}),
        ));
        assert!(relay.running.lock().unwrap().is_empty());
        assert_eq!(relay.report.borrow().in_flight, 0);
        // Long enough for opencodex's delayed answer, had the call gone on to post it.
        tokio::time::sleep(Duration::from_secs(4)).await;
        assert!(answers_to(&server, "req_1").await.is_empty());
    }

    /// A model the allowlist refuses is refused on the Mac too, with why, and opencodex never
    /// hears of it: the frame's model or the one in the body, whichever names it.
    #[tokio::test]
    async fn a_model_the_allowlist_refuses_is_refused_without_calling_opencodex() {
        let server = MockServer::start().await;
        let opencodex = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&opencodex)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let relay = relay(&server, target(&opencodex.uri(), Some(KEY)));

        relay.take_frame(RelayFrame::from_value(&infer("req_1", "claude-opus-4")));
        let mut smuggled = infer("req_2", "gpt-5-codex");
        smuggled["request"]["model"] = json!("gemini-2.5-pro");
        relay.take_frame(RelayFrame::from_value(&smuggled));
        for id in ["req_1", "req_2"] {
            until("the refusal reached the server", async || {
                !answers_to(&server, id).await.is_empty()
            })
            .await;
            let posted = &answers_to(&server, id).await[0];
            let said: Value = serde_json::from_slice(&posted.body).unwrap();
            let sentence = said["error"].as_str().expect("a sentence under error");
            assert!(
                sentence.contains("isn't a model this Mac answers"),
                "{sentence}"
            );
            assert_eq!(
                posted.headers.get("content-type").unwrap(),
                "application/json"
            );
        }
    }

    /// opencodex not running is said in a plain sentence naming where it was looked for, and
    /// opencodex refusing a call passes on what it said, bounded.
    #[tokio::test]
    async fn opencodex_down_or_refusing_is_said_in_a_sentence() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        // A port nothing listens on: bound, read, and let go.
        let port = std::net::TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port();
        let down = relay(
            &server,
            target(&format!("http://127.0.0.1:{port}"), Some(KEY)),
        );
        down.take_frame(RelayFrame::from_value(&infer("req_1", "gpt-5-codex")));
        until("the sentence reached the server", async || {
            !answers_to(&server, "req_1").await.is_empty()
        })
        .await;
        let said: Value =
            serde_json::from_slice(&answers_to(&server, "req_1").await[0].body).unwrap();
        assert_eq!(
            said,
            json!({"error": format!("opencodex isn't running at 127.0.0.1:{port}.")})
        );

        let opencodex = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/v1/chat/completions"))
            .respond_with(ResponseTemplate::new(429).set_body_json(json!({
                "error": {"message": format!("{} usage limit reached", "x".repeat(900))}
            })))
            .mount(&opencodex)
            .await;
        let refusing = relay(&server, target(&opencodex.uri(), Some(KEY)));
        refusing.take_frame(RelayFrame::from_value(&infer("req_2", "gpt-5-codex")));
        until("the refusal reached the server", async || {
            !answers_to(&server, "req_2").await.is_empty()
        })
        .await;
        let said: Value =
            serde_json::from_slice(&answers_to(&server, "req_2").await[0].body).unwrap();
        let sentence = said["error"].as_str().unwrap();
        assert!(
            sentence.starts_with("opencodex answered 429: xxx"),
            "{sentence}"
        );
        assert_eq!(sentence.chars().count(), SENTENCE_CHARS);
    }

    /// The models call passes opencodex's list on as JSON.
    #[tokio::test]
    async fn the_models_call_passes_opencodexs_list_on() {
        let server = MockServer::start().await;
        let opencodex = MockServer::start().await;
        let listed = json!({"object": "list", "data": [{"id": "gpt-5-codex"}, {"id": "grok-4"}]});
        Mock::given(method("GET"))
            .and(path("/v1/models"))
            .and(header("authorization", format!("Bearer {KEY}").as_str()))
            .respond_with(ResponseTemplate::new(200).set_body_json(listed.clone()))
            .mount(&opencodex)
            .await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;
        let relay = relay(&server, target(&opencodex.uri(), Some(KEY)));
        relay.take_frame(RelayFrame::from_value(
            &json!({"type": "models", "requestId": "req_m"}),
        ));
        until("the list reached the server", async || {
            !answers_to(&server, "req_m").await.is_empty()
        })
        .await;
        let posted = &answers_to(&server, "req_m").await[0];
        assert_eq!(
            serde_json::from_slice::<Value>(&posted.body).unwrap(),
            listed
        );
        assert_eq!(
            posted.headers.get("content-type").unwrap(),
            "application/json"
        );
    }

    /// The stream as the server sends it: `ready` makes this Mac the relay, and a call is
    /// answered once. An answer the server gave up on (`404`), one somebody answered already
    /// (`409`), and one past the server's 32 MiB (`413`, cut off there: a failed answer) are
    /// quiet: the answer is not sent again, and the relay goes on answering, stops for nothing and
    /// says nothing is wrong.
    #[tokio::test]
    async fn an_answer_gone_answered_already_or_too_large_is_quiet() {
        for status in [404, 409, 413] {
            let server = MockServer::start().await;
            let opencodex = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/v1/chat/completions"))
                .respond_with(
                    ResponseTemplate::new(200)
                        .set_body_raw("data: [DONE]\n\n", "text/event-stream"),
                )
                .mount(&opencodex)
                .await;
            // The first stream brings the call; every one after it only says this Mac is the
            // relay, as a server with nothing to ask would.
            Mock::given(method("GET"))
                .and(path("/inference-relay/requests"))
                .and(header("authorization", format!("Bearer {TOKEN}").as_str()))
                .respond_with(ResponseTemplate::new(200).set_body_raw(
                    sse(&[
                        json!({"type": "ready", "machineId": "mac_1"}),
                        infer("req_1", "gpt-5-codex"),
                        json!({"type": "ping"}),
                    ]),
                    "text/event-stream",
                ))
                .up_to_n_times(1)
                .mount(&server)
                .await;
            Mock::given(method("GET"))
                .and(path("/inference-relay/requests"))
                .respond_with(ResponseTemplate::new(200).set_body_raw(
                    sse(&[json!({"type": "ready", "machineId": "mac_1"})]),
                    "text/event-stream",
                ))
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path("/inference-relay/responses/req_1"))
                .respond_with(
                    ResponseTemplate::new(status).set_body_json(json!({"error": "refused"})),
                )
                .expect(1)
                .mount(&server)
                .await;
            let handle = start_relay(
                OpenGrokClient::new(&server.uri()).unwrap(),
                machine(),
                target(&opencodex.uri(), None),
                quick(),
            );
            until("the answer went", async || {
                !answers_to(&server, "req_1").await.is_empty()
            })
            .await;
            until("the call took itself out", async || {
                handle.report().in_flight == 0
            })
            .await;
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert_eq!(
                answers_to(&server, "req_1").await.len(),
                1,
                "{status}: the answer is not sent again"
            );
            let report = handle.report();
            assert!(
                !report.halted
                    && matches!(
                        report.status,
                        RelayStatus::Answering | RelayStatus::Connecting
                    ),
                "{status}: an answer refused so stops nothing and is no error: {report:?}"
            );
        }
    }

    /// After `replaced` the Mac stops and does not open the stream again: another Mac is
    /// answering, and fighting it for the stream would take turns from it.
    #[tokio::test]
    async fn replaced_stops_the_reconnecting() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/inference-relay/requests"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                sse(&[
                    json!({"type": "ready", "machineId": "mac_1"}),
                    json!({"type": "replaced"}),
                ]),
                "text/event-stream",
            ))
            .expect(1)
            .mount(&server)
            .await;
        let handle = start_relay(
            OpenGrokClient::new(&server.uri()).unwrap(),
            machine(),
            target("http://127.0.0.1:8080", None),
            quick(),
        );
        let mut reports = handle.reports();
        let replaced = tokio::time::timeout(
            Duration::from_secs(10),
            reports.wait_for(|report| report.status == RelayStatus::Replaced),
        )
        .await
        .expect("the server's word is heard")
        .expect("the relay reports")
        .clone();
        assert!(replaced.halted);
        // Several times the longest wait between attempts: none is made.
        tokio::time::sleep(Duration::from_secs(1)).await;
        let opened = server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .filter(|request| request.url.path() == "/inference-relay/requests")
            .count();
        assert_eq!(opened, 1, "no stream is opened after replaced");
        assert_eq!(handle.report().status, RelayStatus::Replaced);
    }

    /// A stream that drops is opened again after a wait, and one that fails to open is tried
    /// again with a longer one; a server that turns the token away stops it for good, and says so.
    #[tokio::test]
    async fn a_dropped_stream_is_opened_again_and_a_refused_token_stops_it() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/inference-relay/requests"))
            .respond_with(ResponseTemplate::new(200).set_body_raw(
                sse(&[json!({"type": "ready", "machineId": "mac_1"})]),
                "text/event-stream",
            ))
            .up_to_n_times(2)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/inference-relay/requests"))
            .respond_with(
                ResponseTemplate::new(401).set_body_json(json!({"error": "unknown machine"})),
            )
            .mount(&server)
            .await;
        let handle = start_relay(
            OpenGrokClient::new(&server.uri()).unwrap(),
            machine(),
            target("http://127.0.0.1:8080", None),
            quick(),
        );
        let mut reports = handle.reports();
        let stopped = tokio::time::timeout(
            Duration::from_secs(10),
            reports.wait_for(|report| report.halted),
        )
        .await
        .expect("it stops")
        .expect("the relay reports")
        .clone();
        assert_eq!(stopped.status, RelayStatus::Error(TOKEN_REFUSED.into()));
        let opened = server.received_requests().await.unwrap().len();
        assert_eq!(opened, 3, "twice answering, then refused");
        tokio::time::sleep(Duration::from_millis(500)).await;
        assert_eq!(
            server.received_requests().await.unwrap().len(),
            3,
            "and not again"
        );
    }

    /// A stream that goes quiet past three pings is given up and opened again, however open it
    /// looks: a server that holds a connection and sends nothing is a relay nobody can reach.
    #[tokio::test]
    async fn a_stream_gone_quiet_is_opened_again() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let opened = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = Arc::clone(&opened);
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                tokio::spawn(async move {
                    let mut asked = [0u8; 4096];
                    let _ = socket.read(&mut asked).await;
                    let ready = format!("data: {}\n\n", json!({"type": "ready"}));
                    let head = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\n\
                         transfer-encoding: chunked\r\n\r\n{:x}\r\n{ready}\r\n",
                        ready.len()
                    );
                    let _ = socket.write_all(head.as_bytes()).await;
                    // Then nothing, holding the connection.
                    tokio::time::sleep(Duration::from_secs(30)).await;
                });
            }
        });
        let timings = RelayTimings {
            quiet: Duration::from_millis(300),
            ..quick()
        };
        let handle = start_relay(
            OpenGrokClient::new(&format!("http://{address}")).unwrap(),
            machine(),
            target("http://127.0.0.1:8080", None),
            timings,
        );
        until("the quiet stream was opened again", async || {
            opened.load(std::sync::atomic::Ordering::SeqCst) >= 2
        })
        .await;
        assert!(!handle.report().halted);
    }

    /// The stream's lines are split on the bytes: a frame cut across chunks, a character cut in
    /// two, CRLF endings, comments and other fields all read as the server meant them.
    #[tokio::test]
    async fn the_streams_frames_are_read_whole_across_chunks() {
        let frame = json!({"type": "infer", "requestId": "r", "request": {"content": "Zoë 你好"}});
        let text = format!(": hello\r\nevent: relay\r\ndata: {frame}\r\n\r\ndata: not json\n\n");
        let bytes = text.as_bytes();
        // Cut inside the multibyte characters, one byte at a time.
        let chunks: Vec<reqwest::Result<Vec<u8>>> =
            bytes.chunks(1).map(|chunk| Ok(chunk.to_vec())).collect();
        let mut frames = SseFrames::new(futures::stream::iter(chunks));
        assert_eq!(frames.next_frame().await, Some(Ok(frame)));
        assert_eq!(
            frames.next_frame().await,
            None,
            "the line that is not JSON is passed over"
        );
    }
}
