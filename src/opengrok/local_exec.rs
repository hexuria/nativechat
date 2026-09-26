//! NativeChat as the reverse-exec daemon: enrol this Mac, hold `/local-exec/requests`,
//! run approved commands, post `/local-exec/responses`.
//!
//! The AG-UI card is the person's yes. Frames that arrive here already passed the
//! server gate, so they run without a second prompt.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::sync::mpsc;
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
}

/// The commands this Mac is running for the server, by the `requestId` their `exec` frame
/// carried, so that a `cancel` naming one can stop it. A command takes itself out when it
/// finishes. Kept across reconnects: the server sends a cancel down whichever stream holds the
/// machine when it gives up, and that need not be the stream the command came in on.
type Running = Arc<Mutex<HashMap<String, JoinHandle<()>>>>;

pub async fn enrol_this_machine(
    client: &OpenGrokClient,
    data_dir: &Path,
) -> Result<String, OpenGrokError> {
    let cred = ensure_daemon(client, data_dir).await?;
    Ok(cred.machine_id)
}

pub fn stored_machine_id(data_dir: &Path) -> Option<String> {
    load_credential(&data_dir.join(CREDENTIAL_FILE)).map(|cred| cred.machine_id)
}

pub async fn serve_local_exec(client: OpenGrokClient, data_dir: PathBuf, cancel: Arc<AtomicBool>) {
    let mut cred = match ensure_daemon(&client, &data_dir).await {
        Ok(cred) => cred,
        Err(error) => {
            eprintln!("NativeChat local-exec: could not enrol this machine: {error}");
            return;
        }
    };

    let running = Running::default();
    while !cancel.load(Ordering::Relaxed) {
        if let Err(error) = run_request_stream(&client, &cred, &cancel, &running).await {
            if error.is_unauthorized() {
                match ensure_daemon(&client, &data_dir).await {
                    Ok(fresh) => {
                        cred = fresh;
                        if let Err(again) =
                            run_request_stream(&client, &cred, &cancel, &running).await
                        {
                            eprintln!("NativeChat local-exec: {again}");
                        }
                    }
                    Err(enrol) => eprintln!("NativeChat local-exec: re-enrol failed: {enrol}"),
                }
            } else if !cancel.load(Ordering::Relaxed) {
                eprintln!("NativeChat local-exec: {error}");
            }
        }
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        tokio::time::sleep(RECONNECT_WAIT).await;
    }
    // Signed out: nothing this Mac started for the server goes on without it, or posts a result
    // after the person has gone. Aborting a command's task kills what it was running; see
    // `ProcessGroup`.
    for (_, task) in running
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .drain()
    {
        task.abort();
    }
}

async fn ensure_daemon(
    client: &OpenGrokClient,
    data_dir: &Path,
) -> Result<StoredDaemon, OpenGrokError> {
    let path = data_dir.join(CREDENTIAL_FILE);
    let stored = load_credential(&path);
    let machines = client.list_daemons().await.unwrap_or_default();

    if let Some(stored) = stored.as_ref() {
        let still_listed = machines
            .iter()
            .any(|m| m.machine_id == stored.machine_id && !m.revoked);
        if still_listed {
            return Ok(stored.clone());
        }
    }

    let existing = machines.iter().find(|machine| !machine.revoked);
    let label = machine_label();
    let enrol = client
        .enrol_daemon(&label, existing.map(|machine| machine.machine_id.as_str()))
        .await?;
    // The mode is left where the server keeps it, which for a Mac nobody has set is `never`,
    // until the person turns it on in Settings. Enrolling used to write `ask` on its own, and
    // that offered this Mac's shell to every coworker without anyone having said so (#87).
    let cred = StoredDaemon {
        machine_id: enrol.machine_id,
        token: enrol.token,
        label,
    };
    save_credential(&path, &cred);
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
    let _ = write_private(path, &json);
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        std::io::Write::write_all(&mut file, bytes)?;
        file.sync_all()
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn daemon() -> StoredDaemon {
        StoredDaemon {
            machine_id: "mac_1".into(),
            token: "tok_1".into(),
            label: String::new(),
        }
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

        let machine_id = enrol_this_machine(&client, dir.path()).await.unwrap();

        assert_eq!(machine_id, "mac_new");
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
