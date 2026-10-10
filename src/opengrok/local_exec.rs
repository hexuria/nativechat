//! NativeChat as the reverse-exec daemon: enrol this Mac, hold `/local-exec/requests`,
//! run approved commands, post `/local-exec/responses`.
//!
//! The AG-UI card is the person's yes. Frames that arrive here already passed the
//! server gate, so they run without a second prompt.

use crate::private_file::write_private;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use super::client::OpenGrokClient;
use super::error::OpenGrokError;

const CREDENTIAL_FILE: &str = "local-exec-daemon.json";
const EXEC_TIMEOUT: Duration = Duration::from_secs(120);
const RECONNECT_WAIT: Duration = Duration::from_secs(2);
const CANCEL_CHECK: Duration = Duration::from_secs(1);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredDaemon {
    machine_id: String,
    token: String,
    #[serde(default)]
    label: String,
    /// The account this token was issued to (`sub` of the server's daemon token). A file from
    /// before it was kept reads as empty, which is nobody's.
    #[serde(default)]
    account: String,
}

impl StoredDaemon {
    fn credential(&self) -> MachineCredential {
        MachineCredential {
            machine_id: self.machine_id.clone(),
            token: self.token.clone(),
        }
    }
}

/// The commands this Mac is running for the server, by the `requestId` their `exec` frame
/// carried, so that a `cancel` naming one can stop it. A command takes itself out when it
/// finishes. Kept across reconnects: the server sends a cancel down whichever stream holds the
/// machine when it gives up, and that need not be the stream the command came in on.
type Running = Arc<Mutex<HashMap<String, JoinHandle<()>>>>;

pub async fn enrol_this_machine(
    client: &OpenGrokClient,
    data_dir: &Path,
) -> Result<MachineCredential, OpenGrokError> {
    let cred = ensure_daemon(client, data_dir).await?;
    Ok(cred.credential())
}

pub fn stored_machine_id(data_dir: &Path) -> Option<String> {
    load_credential(&data_dir.join(CREDENTIAL_FILE)).map(|cred| cred.machine_id)
}

/// This Mac's credential with the server as enrolment left it: its machine id, and the token the
/// local-exec stream opens with. The Mac relay opens its stream and posts its answers with the
/// same token (`super::relay`, opengrok-server #292), so enrolling is what lets this Mac answer,
/// though it never makes it the relay by itself. Its `Debug` never prints the token.
#[derive(Clone, PartialEq, Eq)]
pub struct MachineCredential {
    machine_id: String,
    token: String,
}

impl MachineCredential {
    #[cfg(test)]
    pub(crate) fn new(machine_id: &str, token: &str) -> Self {
        Self {
            machine_id: machine_id.to_string(),
            token: token.to_string(),
        }
    }

    pub fn machine_id(&self) -> &str {
        &self.machine_id
    }

    pub(crate) fn token(&self) -> &str {
        &self.token
    }
}

impl std::fmt::Debug for MachineCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MachineCredential")
            .field("machine_id", &self.machine_id)
            .field("token", &"«redacted»")
            .finish()
    }
}

/// This Mac's credential as local-exec holds it, as it changes: `None` until it has enrolled, then
/// the credential of each enrolment. The Mac relay opens its stream and answers with it
/// (`super::relay`) and follows it: when the server turns the token away, local-exec enrols this
/// Mac again, and a relay stopped for the old token starts again with the new credential.
pub type Enrolment = watch::Receiver<Option<MachineCredential>>;

/// Why local-exec stopped holding this Mac's stream by itself. Settings → Computer says it under
/// This Mac: a Mac that runs nothing for the server any more would otherwise read as if it did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocalExecStopped {
    /// Enrolling this Mac did not go through, at the start or after the server turned its token
    /// away, and why ([`enrol_failure`]).
    NotEnrolled(String),
    /// The server turned this Mac's token away again after this Mac had enrolled again.
    TurnedAwayAgain,
}

impl LocalExecStopped {
    /// What the person reads.
    pub fn sentence(&self) -> String {
        match self {
            Self::NotEnrolled(why) => format!(
                "This Mac couldn't enrol with the server, so it isn't running commands for your \
                 Bots: {why}. Sign out and in again to try again."
            ),
            Self::TurnedAwayAgain => TURNED_AWAY_AGAIN.to_string(),
        }
    }
}

/// [`LocalExecStopped::TurnedAwayAgain`] in words.
const TURNED_AWAY_AGAIN: &str = "The server turned this Mac away again after it enrolled again, \
     so it has stopped running commands for your Bots. Something else may be enrolling this Mac \
     too. Sign out and in again to take it back.";

/// Why an enrolment did not go through, in words: that the server couldn't be reached, what it
/// answered when it said nothing, or else what it said.
fn enrol_failure(error: &OpenGrokError) -> String {
    if error.unreachable().is_some() {
        "the server couldn't be reached".to_string()
    } else if error.said_nothing() {
        error.status.map_or_else(
            || error.to_string(),
            |status| format!("the server answered {status}"),
        )
    } else {
        error.message.trim().to_string()
    }
}

/// Hold this Mac's local-exec stream until `cancel`. Each credential it holds goes to `enrolled`,
/// for the relay ([`Enrolment`]).
///
/// When the server turns this Mac's token away, which it does once something else has enrolled
/// this Mac's machine again or revoked it (a `401` before any frame, after it ended the stream the
/// token held), this enrols this Mac again as its own machine, once. A token turned away again
/// after that is something else enrolling the machine too, and enrolling once more would only
/// retire that one's token in turn, the two taking turns for good. So then it stops, as it does
/// when enrolling does not go through, and answers why. `None` when `cancel` stopped it.
pub async fn serve_local_exec(
    client: OpenGrokClient,
    data_dir: PathBuf,
    cancel: Arc<AtomicBool>,
    enrolled: watch::Sender<Option<MachineCredential>>,
) -> Option<LocalExecStopped> {
    let mut cred = match ensure_daemon(&client, &data_dir).await {
        Ok(cred) => cred,
        Err(error) => {
            eprintln!("NativeChat local-exec: could not enrol this machine: {error}");
            return (!cancel.load(Ordering::Relaxed))
                .then(|| LocalExecStopped::NotEnrolled(enrol_failure(&error)));
        }
    };
    publish(&enrolled, &cred);

    let running = Running::default();
    let mut enrolled_again = false;
    let stopped = loop {
        if cancel.load(Ordering::Relaxed) {
            break None;
        }
        match run_request_stream(&client, &cred, &cancel, &running).await {
            Err(error) if error.is_unauthorized() => {
                if cancel.load(Ordering::Relaxed) {
                    break None;
                }
                if enrolled_again {
                    eprintln!("NativeChat local-exec: turned away again after enrolling again");
                    break Some(LocalExecStopped::TurnedAwayAgain);
                }
                enrolled_again = true;
                let path = data_dir.join(CREDENTIAL_FILE);
                match enrol(&client, &path, Some(&cred.machine_id), &cred.account).await {
                    Ok(fresh) => {
                        cred = fresh;
                        publish(&enrolled, &cred);
                        // The new token is tried at once: it is the old one that was refused.
                        continue;
                    }
                    Err(error) => {
                        eprintln!("NativeChat local-exec: could not enrol again: {error}");
                        break (!cancel.load(Ordering::Relaxed))
                            .then(|| LocalExecStopped::NotEnrolled(enrol_failure(&error)));
                    }
                }
            }
            Err(error) if !cancel.load(Ordering::Relaxed) => {
                eprintln!("NativeChat local-exec: {error}");
            }
            _ => {}
        }
        if cancel.load(Ordering::Relaxed) {
            break None;
        }
        tokio::time::sleep(RECONNECT_WAIT).await;
    };
    // Signed out, or stopped: nothing this Mac started for the server goes on without it, or
    // posts a result after the person has gone or with a token the server no longer takes.
    // Aborting a command's task kills what it was running; see `ProcessGroup`.
    for (_, task) in running
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .drain()
    {
        task.abort();
    }
    stopped
}

/// Tell the relay the credential local-exec holds now, when it is not the one it held: an
/// enrolment that gave nothing new wakes nothing.
fn publish(enrolled: &watch::Sender<Option<MachineCredential>>, cred: &StoredDaemon) {
    let fresh = cred.credential();
    enrolled.send_if_modified(|held| {
        let changed = held.as_ref() != Some(&fresh);
        if changed {
            *held = Some(fresh);
        }
        changed
    });
}

/// This Mac's credential: the one kept here while it is the signed-in account's and the server
/// still lists its machine as live, and otherwise a new one, from enrolling this Mac as its own
/// machine again, or as a new machine when it has none.
///
/// The account is checked as well as the listing. Another account that signed in on this Mac
/// enrolled it too, under the same machine id, and its token is the one kept here until this
/// account enrols: kept, the relay opened its stream as that account, and this account's turns
/// found no Mac connected while its card said it relayed.
///
/// Never as another machine. The account's other machines are other Macs, and this used to enrol
/// as the first live one listed whenever its own was not: the server gave that Mac's machine a new
/// token and retired the old, and the Mac whose machine it was was turned away from then on.
async fn ensure_daemon(
    client: &OpenGrokClient,
    data_dir: &Path,
) -> Result<StoredDaemon, OpenGrokError> {
    let path = data_dir.join(CREDENTIAL_FILE);
    let stored = load_credential(&path);
    let account = client.me().await?.id;
    if let Some(stored) = stored.as_ref().filter(|stored| stored.account == account) {
        let machines = client.list_daemons().await.unwrap_or_default();
        let still_listed = machines
            .iter()
            .any(|m| m.machine_id == stored.machine_id && !m.revoked);
        if still_listed {
            return Ok(stored.clone());
        }
    }
    let own = stored.as_ref().map(|stored| stored.machine_id.as_str());
    enrol(client, &path, own, &account).await
}

/// Enrol this Mac with the server as `machine_id`, its own machine, which gives it a new token
/// and retires the one it held (with the streams that token opened); or, with none, as a new
/// machine the server names. The credential is kept at `path` for the next start, as `account`'s,
/// the account signed in to enrol it.
async fn enrol(
    client: &OpenGrokClient,
    path: &Path,
    machine_id: Option<&str>,
    account: &str,
) -> Result<StoredDaemon, OpenGrokError> {
    let label = machine_label();
    let enrol = client.enrol_daemon(&label, machine_id).await?;
    // The mode is left where the server keeps it, which for a Mac nobody has set is `never`,
    // until the person turns it on in Settings. Enrolling used to write `ask` on its own, and
    // that offered this Mac's shell to every coworker without anyone having said so (#87).
    let cred = StoredDaemon {
        machine_id: enrol.machine_id,
        token: enrol.token,
        label,
        account: account.to_string(),
    };
    save_credential(path, &cred);
    Ok(cred)
}

async fn run_request_stream(
    client: &OpenGrokClient,
    cred: &StoredDaemon,
    cancel: &Arc<AtomicBool>,
    running: &Running,
) -> Result<(), OpenGrokError> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let reader = {
        let client = client.clone();
        let token = cred.token.clone();
        tokio::spawn(async move {
            client
                .stream_local_exec_requests(&token, move |frame| {
                    let _ = tx.send(frame);
                })
                .await
        })
    };

    loop {
        // The flag is read at least every `CANCEL_CHECK`, not only when a frame comes: after a
        // sign-out the next frame may never come, and until it did this Mac went on holding the
        // stream and running what it had already started.
        let frame = match tokio::time::timeout(CANCEL_CHECK, rx.recv()).await {
            Ok(Some(frame)) => Some(frame),
            Ok(None) => break,
            Err(_) => None,
        };
        if cancel.load(Ordering::Relaxed) {
            reader.abort();
            return Ok(());
        }
        if let Some(frame) = frame {
            take_frame(client, cred, running, frame);
        }
    }

    match reader.await {
        Ok(result) => result,
        Err(_) => Ok(()),
    }
}

/// One frame off the request stream: `exec` starts a command, `cancel` stops the one it names.
/// Anything else, such as the `welcome` a stream opens with, is not for this loop.
///
/// A cancel is the server giving up on a command (`LocalExecBroker::cancel` in opengrok-server
/// `crates/opengrok-server/src/local_exec/broker.rs` sends `{"kind": "cancel", "requestId": …}`).
/// Aborting the task drops what it holds, and `ProcessGroup` kills the command, and everything it
/// started, there and then. The frame used to be thrown away, and a cancelled command ran on to
/// its own timeout (#87). A cancel for a command that has already finished finds nothing, which
/// is right.
fn take_frame(client: &OpenGrokClient, cred: &StoredDaemon, running: &Running, frame: Value) {
    let Some(request_id) = frame.get("requestId").and_then(Value::as_str) else {
        return;
    };
    let request_id = request_id.to_string();
    match frame.get("kind").and_then(Value::as_str) {
        Some("exec") => {
            // Held across the spawn, so a command that finishes at once cannot take itself out
            // before it has been put in, and leave a stale entry behind.
            let mut tasks = running.lock().unwrap_or_else(PoisonError::into_inner);
            // The server gives every command an id of its own, so an exec for one that is
            // already running is that command again. Running it twice is not what anybody
            // asked for, and the second would put its task where the first one's cancel
            // looks for it.
            if tasks.contains_key(&request_id) {
                return;
            }
            let client = client.clone();
            let cred = cred.clone();
            let done = Arc::clone(running);
            let id = request_id.clone();
            let task = tokio::spawn(async move {
                handle_exec(&client, &cred, &frame).await;
                done.lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .remove(&id);
            });
            tasks.insert(request_id, task);
        }
        Some("cancel") => {
            let task = running
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&request_id);
            if let Some(task) = task {
                task.abort();
            }
        }
        _ => {}
    }
}

async fn handle_exec(client: &OpenGrokClient, cred: &StoredDaemon, frame: &Value) {
    let Some(request_id) = frame.get("requestId").and_then(Value::as_str) else {
        return;
    };
    let args = frame
        .get("serverMessage")
        .and_then(|msg| msg.get("shellStreamArgs"))
        .cloned()
        .unwrap_or(Value::Null);
    let command = args
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let cwd = args
        .get("workingDirectory")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let timeout_ms = args.get("timeout").and_then(Value::as_u64).unwrap_or(0);
    let timeout = if timeout_ms == 0 {
        EXEC_TIMEOUT
    } else {
        Duration::from_millis(timeout_ms)
    };

    let outcome = run_shell(&command, &cwd, timeout).await;
    let body = json!({
        "providerId": cred.machine_id,
        "frames": [{
            "kind": "client",
            "requestId": request_id,
            "message": { "shellResult": outcome },
        }]
    });
    if let Err(error) = client.post_local_exec_responses(&cred.token, &body).await {
        eprintln!("NativeChat local-exec: could not post result: {error}");
    }
}

async fn run_shell(command: &str, cwd: &str, timeout: Duration) -> Value {
    if command.trim().is_empty() {
        return json!({ "spawnError": { "error": "empty command" } });
    }
    let mut child = Command::new("sh");
    child
        .arg("-c")
        .arg(command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    // A group of its own, so that what the shell starts can be stopped with it; see
    // `ProcessGroup`.
    #[cfg(unix)]
    child.process_group(0);
    if !cwd.trim().is_empty() {
        child.current_dir(cwd);
    }
    let spawned = match child.spawn() {
        Ok(child) => child,
        Err(error) => return json!({ "spawnError": { "error": error.to_string() } }),
    };
    let mut group = ProcessGroup::of(&spawned);
    match tokio::time::timeout(timeout, spawned.wait_with_output()).await {
        Ok(Ok(output)) => {
            group.finished();
            let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
            let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
            let code = output.status.code().unwrap_or(1);
            let case = if output.status.success() {
                "success"
            } else {
                "failure"
            };
            json!({ case: { "exitCode": code, "stdout": stdout, "stderr": stderr } })
        }
        Ok(Err(error)) => json!({ "spawnError": { "error": error.to_string() } }),
        Err(_) => json!({ "timeout": { "timeoutMs": timeout.as_millis() as u64 } }),
    }
}

/// The process group a command runs in, killed whole when the command is given up on: when it
/// runs out of time, or when the task running it is aborted — the server cancelled it
/// (`take_frame`), or this Mac stopped serving (`serve_local_exec`) — which drops this along
/// with everything else the task held.
///
/// `kill_on_drop` kills only `sh`, and what the shell had forked ran on without it: every
/// command the server sends, since it puts a PATH preamble in front of each one, and anything
/// piped, listed or sent to the background (#87). A command that finished is left alone, and so
/// is anything it sent to the background with its output elsewhere, a dev server say, which it
/// meant to outlive it.
struct ProcessGroup(Option<i32>);

impl ProcessGroup {
    /// The group `process_group(0)` made for `child`, which it leads, so its id is the child's.
    fn of(child: &tokio::process::Child) -> Self {
        Self(child.id().and_then(|pid| i32::try_from(pid).ok()))
    }

    fn finished(&mut self) {
        self.0 = None;
    }
}

impl Drop for ProcessGroup {
    fn drop(&mut self) {
        #[cfg(unix)]
        if let Some(group) = self.0 {
            // SAFETY: `killpg` takes two integers and touches no memory of ours.
            unsafe {
                libc::killpg(group, libc::SIGKILL);
            }
        }
    }
}

fn machine_label() -> String {
    let name = std::process::Command::new("hostname")
        .output()
        .ok()
        .and_then(|out| String::from_utf8(out.stdout).ok())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "this Mac".to_string());
    format!("NativeChat on {name}")
}

fn load_credential(path: &Path) -> Option<StoredDaemon> {
    let bytes = std::fs::read(path).ok()?;
    serde_json::from_slice(&bytes).ok()
}

fn save_credential(path: &Path, cred: &StoredDaemon) {
    let Ok(json) = serde_json::to_vec_pretty(cred) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Not kept privately means not kept: the old file stays as it was.
    if let Err(error) = write_private(path, &json) {
        eprintln!("NativeChat local exec: this Mac's credential was not saved: {error}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn daemon() -> StoredDaemon {
        StoredDaemon {
            machine_id: "mac_1".into(),
            token: "tok_1".into(),
            label: String::new(),
            account: "acct_1".into(),
        }
    }

    /// `GET /account` answers as `account` (`Account` in `super::types`).
    async fn signed_in_as(server: &MockServer, account: &str) {
        Mock::given(method("GET"))
            .and(path("/account"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "id": account, "email": "dev@nativechat.local" })),
            )
            .mount(server)
            .await;
    }

    fn exec_frame(request_id: &str, command: &str) -> Value {
        json!({
            "kind": "exec",
            "requestId": request_id,
            "serverMessage": {
                "shellStreamArgs": {
                    "command": command,
                    "workingDirectory": "",
                    "timeout": 30000
                }
            }
        })
    }

    /// Whether `pid` is still running something. A killed child the runtime has not reaped yet
    /// is a zombie, and a zombie runs nothing.
    fn is_running(pid: &str) -> bool {
        std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", pid])
            .output()
            .is_ok_and(|out| {
                out.status.success()
                    && !String::from_utf8_lossy(&out.stdout)
                        .trim_start()
                        .starts_with('Z')
            })
    }

    /// Enrolling is not consent. The server keeps a Mac nobody has set at `never`, and the first
    /// enrol used to write `ask` over that, which put this Mac's shell in front of every coworker
    /// before the person had said a word.
    #[tokio::test]
    async fn enrolling_this_mac_leaves_its_mode_to_the_person() {
        let server = MockServer::start().await;
        signed_in_as(&server, "acct_1").await;
        Mock::given(method("GET"))
            .and(path("/local-exec/daemon"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({ "machines": [] })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/local-exec/daemon"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "machineId": "mac_new", "token": "tok_new" })),
            )
            .expect(1)
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let dir = tempfile::tempdir().unwrap();

        let machine = enrol_this_machine(&client, dir.path()).await.unwrap();

        assert_eq!(machine, MachineCredential::new("mac_new", "tok_new"));
        assert_eq!(stored_machine_id(dir.path()).as_deref(), Some("mac_new"));
        let calls: Vec<String> = server
            .received_requests()
            .await
            .expect("the recorder is on")
            .iter()
            .map(|request| format!("{} {}", request.method, request.url.path()))
            .collect();
        assert!(
            calls
                .iter()
                .all(|call| !call.ends_with(" /local-exec/policy")),
            "the mode is not read or written on the way in: {calls:?}"
        );
    }

    /// One of the account's machines as `GET /local-exec/daemon` lists it (`list_daemons` in
    /// opengrok-server `crates/opengrok-server/src/local_exec.rs`).
    fn listed(machine_id: &str, label: &str, revoked: bool) -> Value {
        json!({
            "machineId": machine_id,
            "label": label,
            "enrolledAtMs": 1_790_000_000_000_u64,
            "revoked": revoked,
            "connected": !revoked,
        })
    }

    /// The machine id each `POST /local-exec/daemon` asked to enrol, in order: `None` for a body
    /// that named none, which the server answers with a new machine of its own.
    async fn enrolled_as(server: &MockServer) -> Vec<Option<String>> {
        server
            .received_requests()
            .await
            .expect("the recorder is on")
            .iter()
            .filter(|request| {
                request.method.as_str() == "POST" && request.url.path() == "/local-exec/daemon"
            })
            .map(|request| {
                let body: Value = request.body_json().expect("an enrolment's body is JSON");
                body.get("machineId")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            })
            .collect()
    }

    /// A second Mac on the account, with no machine of its own yet, enrols as a new machine. It
    /// used to enrol as the first machine the account listed, the first Mac's: the server gave
    /// that machine a new token, and the first Mac was turned away from then on.
    #[tokio::test]
    async fn a_mac_with_no_machine_of_its_own_enrols_as_a_new_one() {
        let server = MockServer::start().await;
        signed_in_as(&server, "acct_1").await;
        Mock::given(method("GET"))
            .and(path("/local-exec/daemon"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "machines": [listed("mac_first", "NativeChat on the first Mac", false)]
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/local-exec/daemon"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "machineId": "mac_second", "token": "tok_second" })),
            )
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let dir = tempfile::tempdir().unwrap();

        let machine = enrol_this_machine(&client, dir.path()).await.unwrap();

        assert_eq!(
            enrolled_as(&server).await,
            [None],
            "a new machine, named by none"
        );
        assert_eq!(machine, MachineCredential::new("mac_second", "tok_second"));
        assert_eq!(stored_machine_id(dir.path()).as_deref(), Some("mac_second"));
    }

    /// A Mac whose machine the server no longer lists as live (revoked here, while another Mac's
    /// is listed first) enrols again as its own machine, and as no other.
    #[tokio::test]
    async fn a_mac_enrols_again_as_its_own_machine_and_never_as_another() {
        let server = MockServer::start().await;
        signed_in_as(&server, "acct_1").await;
        let dir = tempfile::tempdir().unwrap();
        save_credential(&dir.path().join(CREDENTIAL_FILE), &daemon());
        Mock::given(method("GET"))
            .and(path("/local-exec/daemon"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "machines": [
                    listed("mac_other", "NativeChat on another Mac", false),
                    listed("mac_1", "NativeChat on this Mac", true),
                ]
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/local-exec/daemon"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "machineId": "mac_1", "token": "tok_2" })),
            )
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();

        let machine = enrol_this_machine(&client, dir.path()).await.unwrap();

        assert_eq!(enrolled_as(&server).await, [Some("mac_1".to_string())]);
        assert_eq!(machine, MachineCredential::new("mac_1", "tok_2"));
        assert_eq!(stored_machine_id(dir.path()).as_deref(), Some("mac_1"));
    }

    /// The token kept here is the account's that enrolled this Mac, not the account signed in
    /// now. Both accounts list this Mac's machine id, so the token used to be kept: the relay then
    /// opened its stream as the other account, and every turn on the signed-in account said this
    /// Mac was not connected while its card said it was relaying.
    #[tokio::test]
    async fn a_mac_signed_in_to_another_account_enrols_again_for_it() {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let other = StoredDaemon {
            account: "acct_other".into(),
            ..daemon()
        };
        save_credential(&dir.path().join(CREDENTIAL_FILE), &other);
        signed_in_as(&server, "acct_now").await;
        Mock::given(method("GET"))
            .and(path("/local-exec/daemon"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "machines": [listed("mac_1", "NativeChat on this Mac", false)]
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/local-exec/daemon"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "machineId": "mac_1", "token": "tok_now" })),
            )
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();

        let machine = enrol_this_machine(&client, dir.path()).await.unwrap();

        assert_eq!(enrolled_as(&server).await, [Some("mac_1".to_string())]);
        assert_eq!(machine, MachineCredential::new("mac_1", "tok_now"));
        let kept = load_credential(&dir.path().join(CREDENTIAL_FILE)).unwrap();
        assert_eq!(kept.account, "acct_now", "kept as the signed-in account's");

        // Signed in as that account again, the token it now holds is kept.
        enrol_this_machine(&client, dir.path()).await.unwrap();
        assert_eq!(enrolled_as(&server).await.len(), 1, "no second enrolment");
    }

    /// A credential kept before it said whose it was is read, and enrolled again as this Mac's own
    /// machine. Unreadable, it would enrol as a new machine, and the Mac would lose its id with
    /// the mode and relay switch kept under it.
    #[tokio::test]
    async fn a_credential_kept_before_it_named_its_account_enrols_again_as_the_same_machine() {
        let server = MockServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(CREDENTIAL_FILE);
        write_private(&file, br#"{"machine_id":"mac_1","token":"tok_1"}"#).unwrap();
        signed_in_as(&server, "acct_1").await;
        Mock::given(method("GET"))
            .and(path("/local-exec/daemon"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "machines": [listed("mac_1", "NativeChat on this Mac", false)]
            })))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/local-exec/daemon"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "machineId": "mac_1", "token": "tok_2" })),
            )
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();

        let machine = enrol_this_machine(&client, dir.path()).await.unwrap();

        assert_eq!(enrolled_as(&server).await, [Some("mac_1".to_string())]);
        assert_eq!(machine, MachineCredential::new("mac_1", "tok_2"));
        assert_eq!(load_credential(&file).unwrap().account, "acct_1");
    }

    /// A command that leaves the pid of something it started in `pid_file`, and waits on it.
    /// The shell forks it rather than becoming it, which is what every real command does: the
    /// server puts a PATH preamble in front of each one, so no command is ever the shell's last.
    fn forks_a_child(pid_file: &Path) -> String {
        format!("sleep 30 & echo $! > '{}'; wait", pid_file.display())
    }

    /// What a command wrote to `file`, once it has: the pid `forks_a_child` writes down.
    async fn written(file: &Path) -> String {
        let started = Instant::now();
        loop {
            if let Ok(text) = std::fs::read_to_string(file)
                && !text.trim().is_empty()
            {
                return text.trim().to_string();
            }
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "the command never started"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    async fn stops(pid: &str, why: &str) {
        let since = Instant::now();
        while is_running(pid) {
            assert!(since.elapsed() < Duration::from_secs(10), "{why}");
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }

    /// A cancel stops the command it names there and then, rather than leaving it to run on to
    /// its own timeout, and the cancelled command has nothing to report. Everything the command
    /// started goes with it: killing only the shell in front of it left the rest running.
    #[tokio::test]
    async fn a_cancel_frame_kills_the_command_it_names() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/local-exec/responses"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let running = Running::default();
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");

        take_frame(
            &client,
            &daemon(),
            &running,
            exec_frame("req-1", &forks_a_child(&pid_file)),
        );
        let pid = written(&pid_file).await;
        assert!(running.lock().unwrap().contains_key("req-1"));
        assert!(is_running(&pid));

        take_frame(
            &client,
            &daemon(),
            &running,
            json!({ "kind": "cancel", "requestId": "req-1" }),
        );

        assert!(running.lock().unwrap().is_empty());
        stops(&pid, "the command outlived its cancel").await;
    }

    /// A command that has finished takes itself out, so a cancel arriving after it has nothing
    /// to find, and the result it posted stands.
    #[tokio::test]
    async fn a_finished_command_leaves_nothing_behind_to_cancel() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/local-exec/responses"))
            .respond_with(ResponseTemplate::new(200))
            .expect(1)
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let running = Running::default();

        take_frame(&client, &daemon(), &running, exec_frame("req-2", "true"));
        let started = Instant::now();
        while !running.lock().unwrap().is_empty() {
            assert!(
                started.elapsed() < Duration::from_secs(10),
                "the command never finished"
            );
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        take_frame(
            &client,
            &daemon(),
            &running,
            json!({ "kind": "cancel", "requestId": "req-2" }),
        );

        assert!(running.lock().unwrap().is_empty());
    }

    /// A command that runs out of time is stopped with everything it started, the same as a
    /// cancelled one.
    #[tokio::test]
    async fn a_command_out_of_time_is_stopped_with_what_it_started() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");

        let outcome = run_shell(&forks_a_child(&pid_file), "", Duration::from_secs(1)).await;

        assert!(outcome.get("timeout").is_some(), "{outcome}");
        let pid = written(&pid_file).await;
        stops(&pid, "the command outlived its timeout").await;
    }

    /// An exec for a command that is already running does not run it again, and leaves the
    /// first where its cancel will find it.
    #[tokio::test]
    async fn an_exec_for_a_command_already_running_is_not_run_again() {
        let server = MockServer::start().await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let running = Running::default();
        let dir = tempfile::tempdir().unwrap();
        let runs = dir.path().join("runs");
        let command = format!("echo run >> '{}'; sleep 30", runs.display());

        take_frame(&client, &daemon(), &running, exec_frame("req-3", &command));
        written(&runs).await;
        let first = running.lock().unwrap()["req-3"].id();
        take_frame(&client, &daemon(), &running, exec_frame("req-3", &command));
        tokio::time::sleep(Duration::from_millis(300)).await;

        assert_eq!(std::fs::read_to_string(&runs).unwrap(), "run\n");
        assert_eq!(running.lock().unwrap().len(), 1);
        assert_eq!(running.lock().unwrap()["req-3"].id(), first);
        take_frame(
            &client,
            &daemon(),
            &running,
            json!({ "kind": "cancel", "requestId": "req-3" }),
        );
        assert!(running.lock().unwrap().is_empty());
    }

    /// Signing out stops what this Mac was running for the server, rather than leaving it to run
    /// on and post its result after the person has gone.
    #[tokio::test]
    async fn signing_out_stops_what_this_mac_was_running() {
        let server = MockServer::start().await;
        signed_in_as(&server, "acct_1").await;
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        save_credential(&dir.path().join(CREDENTIAL_FILE), &daemon());
        Mock::given(method("GET"))
            .and(path("/local-exec/daemon"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "machines": [{ "machineId": "mac_1" }] })),
            )
            .mount(&server)
            .await;
        // Each time the stream is opened it hands over the same command and ends, so serving
        // goes round its reconnect while the command runs.
        let frame = exec_frame("req-4", &forks_a_child(&pid_file));
        Mock::given(method("GET"))
            .and(path("/local-exec/requests"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_raw(format!("data: {frame}\n\n"), "text/event-stream"),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/local-exec/responses"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;
        let client = OpenGrokClient::new(&server.uri()).unwrap();
        let cancel = Arc::new(AtomicBool::new(false));
        let serving = tokio::spawn(serve_local_exec(
            client,
            dir.path().to_path_buf(),
            Arc::clone(&cancel),
            watch::channel(None).0,
        ));
        let pid = written(&pid_file).await;
        assert!(is_running(&pid));

        cancel.store(true, Ordering::Relaxed);

        tokio::time::timeout(Duration::from_secs(10), serving)
            .await
            .expect("serving stops once signed out")
            .unwrap();
        stops(&pid, "the command outlived the sign-out").await;
    }

    /// `GET /local-exec/requests` as the server answers a token it takes: the reconnect hint, the
    /// `welcome` naming the machine, and then the end of the stream, which is how it ends one whose
    /// token a re-enrolment or a revoke retired, with nothing more sent (`poll_requests` and the
    /// broker's `disconnect` in opengrok-server `crates/opengrok-server/src/local_exec.rs`).
    fn welcome_then_end(machine_id: &str) -> ResponseTemplate {
        let welcome = json!({ "kind": "welcome", "providerId": machine_id });
        ResponseTemplate::new(200).set_body_raw(
            format!("retry: 1000\n\ndata: {welcome}\n\n"),
            "text/event-stream",
        )
    }

    /// How the server answers a stream opened with a token it no longer takes, retired before
    /// the stream opened or while it did: `401` before any frame, in plain words
    /// (`poll_requests`).
    fn turned_away() -> ResponseTemplate {
        ResponseTemplate::new(401).set_body_string("enrol this machine first")
    }

    /// This Mac's machine as the server lists it while the machine is live.
    async fn lists_this_mac(server: &MockServer) {
        Mock::given(method("GET"))
            .and(path("/local-exec/daemon"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "machines": [listed("mac_1", "NativeChat on this Mac", false)]
            })))
            .mount(server)
            .await;
    }

    /// The bearer each `GET /local-exec/requests` opened the stream with, in order.
    async fn opened_with(server: &MockServer) -> Vec<String> {
        server
            .received_requests()
            .await
            .expect("the recorder is on")
            .iter()
            .filter(|request| request.url.path() == "/local-exec/requests")
            .map(|request| {
                request
                    .headers
                    .get("authorization")
                    .and_then(|bearer| bearer.to_str().ok())
                    .unwrap_or_default()
                    .to_string()
            })
            .collect()
    }

    /// Something else enrolled this Mac's machine again, so the server retired the token this Mac
    /// holds: it ended the stream that token held, and when this Mac opens it again, turns the
    /// token away before any frame. Local-exec enrols this Mac again, as its own machine, and the
    /// new credential goes to the relay, which follows it, and not only to the file: that is what
    /// starts a relay stopped for the old token again. It used to enrol again only when the server
    /// no longer listed the machine as live, which it still did here, so it knocked with the
    /// retired token every two seconds for good.
    #[tokio::test]
    async fn enrolling_again_hands_the_relay_the_new_credential() {
        let server = MockServer::start().await;
        signed_in_as(&server, "acct_1").await;
        let dir = tempfile::tempdir().unwrap();
        save_credential(&dir.path().join(CREDENTIAL_FILE), &daemon());
        lists_this_mac(&server).await;
        Mock::given(method("GET"))
            .and(path("/local-exec/requests"))
            .and(header("authorization", "Bearer tok_1"))
            .respond_with(welcome_then_end("mac_1"))
            .up_to_n_times(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/local-exec/requests"))
            .and(header("authorization", "Bearer tok_1"))
            .respond_with(turned_away())
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/local-exec/daemon"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "machineId": "mac_1", "token": "tok_2" })),
            )
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/local-exec/requests"))
            .and(header("authorization", "Bearer tok_2"))
            .respond_with(welcome_then_end("mac_1"))
            .mount(&server)
            .await;
        let (enrolled, mut enrolment) = watch::channel(None);
        let serving = tokio::spawn(serve_local_exec(
            OpenGrokClient::new(&server.uri()).unwrap(),
            dir.path().to_path_buf(),
            Arc::new(AtomicBool::new(false)),
            enrolled,
        ));

        let fresh = MachineCredential::new("mac_1", "tok_2");
        let handed = tokio::time::timeout(
            Duration::from_secs(10),
            enrolment.wait_for(|held| held.as_ref() == Some(&fresh)),
        )
        .await
        .is_ok_and(|held| held.is_ok());
        serving.abort();
        assert!(handed, "the relay is handed the new credential");
        assert_eq!(
            enrolled_as(&server).await,
            [Some("mac_1".to_string())],
            "once, as this Mac's own machine"
        );
        let kept = load_credential(&dir.path().join(CREDENTIAL_FILE)).expect("a credential");
        assert_eq!(
            (kept.machine_id.as_str(), kept.token.as_str()),
            ("mac_1", "tok_2")
        );
        assert_eq!(
            kept.account, "acct_1",
            "kept as the account it was enrolled for"
        );
    }

    /// The server turns this Mac's token away again after it has enrolled again: something else
    /// is enrolling this Mac's machine too, and enrolling once more would only retire that one's
    /// token in turn, the two taking turns for good. Local-exec stops knocking, and says why. It
    /// used to knock with a token that was turned away every two seconds, for good.
    #[tokio::test]
    async fn turned_away_again_after_enrolling_again_it_stops_and_says_why() {
        let server = MockServer::start().await;
        signed_in_as(&server, "acct_1").await;
        let dir = tempfile::tempdir().unwrap();
        save_credential(&dir.path().join(CREDENTIAL_FILE), &daemon());
        lists_this_mac(&server).await;
        Mock::given(method("GET"))
            .and(path("/local-exec/requests"))
            .respond_with(turned_away())
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/local-exec/daemon"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(json!({ "machineId": "mac_1", "token": "tok_2" })),
            )
            .mount(&server)
            .await;
        let (enrolled, enrolment) = watch::channel(None);
        let serving = tokio::spawn(serve_local_exec(
            OpenGrokClient::new(&server.uri()).unwrap(),
            dir.path().to_path_buf(),
            Arc::new(AtomicBool::new(false)),
            enrolled,
        ));

        let stopped = tokio::time::timeout(Duration::from_secs(10), serving)
            .await
            .expect("it stops knocking")
            .unwrap();

        assert_eq!(stopped, Some(LocalExecStopped::TurnedAwayAgain));
        assert_eq!(
            opened_with(&server).await,
            ["Bearer tok_1", "Bearer tok_2"],
            "once with each token, and no more"
        );
        assert_eq!(enrolled_as(&server).await, [Some("mac_1".to_string())]);
        assert_eq!(
            *enrolment.borrow(),
            Some(MachineCredential::new("mac_1", "tok_2")),
            "the relay was handed the new credential all the same"
        );
    }

    /// Enrolling again after the server turned this Mac's token away does not go through, and
    /// local-exec stops knocking, saying why in the server's words.
    #[tokio::test]
    async fn an_enrolment_that_fails_after_a_401_stops_it_and_says_why() {
        let server = MockServer::start().await;
        signed_in_as(&server, "acct_1").await;
        let dir = tempfile::tempdir().unwrap();
        save_credential(&dir.path().join(CREDENTIAL_FILE), &daemon());
        lists_this_mac(&server).await;
        Mock::given(method("GET"))
            .and(path("/local-exec/requests"))
            .respond_with(turned_away())
            .mount(&server)
            .await;
        // What the server says when it cannot keep the enrolment (`enrol_daemon`'s `failed`).
        Mock::given(method("POST"))
            .and(path("/local-exec/daemon"))
            .respond_with(ResponseTemplate::new(500).set_body_string("could not enrol the machine"))
            .mount(&server)
            .await;
        let serving = tokio::spawn(serve_local_exec(
            OpenGrokClient::new(&server.uri()).unwrap(),
            dir.path().to_path_buf(),
            Arc::new(AtomicBool::new(false)),
            watch::channel(None).0,
        ));

        let stopped = tokio::time::timeout(Duration::from_secs(10), serving)
            .await
            .expect("it stops knocking")
            .unwrap();

        assert_eq!(
            stopped,
            Some(LocalExecStopped::NotEnrolled(
                "could not enrol the machine".into()
            ))
        );
        assert_eq!(opened_with(&server).await, ["Bearer tok_1"]);
        assert_eq!(enrolled_as(&server).await, [Some("mac_1".to_string())]);
    }

    #[test]
    fn exec_frame_names_the_command() {
        let frame = json!({
            "kind": "exec",
            "requestId": "req-1",
            "serverMessage": {
                "shellStreamArgs": {
                    "command": "ls /tmp",
                    "workingDirectory": "",
                    "timeout": 5000
                }
            }
        });
        let args = frame
            .get("serverMessage")
            .and_then(|msg| msg.get("shellStreamArgs"))
            .unwrap();
        assert_eq!(args.get("command").and_then(Value::as_str), Some("ls /tmp"));
    }
}
