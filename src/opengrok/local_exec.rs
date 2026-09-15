//! NativeChat as the reverse-exec daemon: enrol this Mac, hold `/local-exec/requests`,
//! run approved commands, post `/local-exec/responses`.
//!
//! The AG-UI card is the person's yes. Frames that arrive here already passed the
//! server gate, so they run without a second prompt.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::process::Command;
use tokio::sync::mpsc;

use super::client::OpenGrokClient;
use super::error::OpenGrokError;

const CREDENTIAL_FILE: &str = "local-exec-daemon.json";
const EXEC_TIMEOUT: Duration = Duration::from_secs(120);
const RECONNECT_WAIT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredDaemon {
    machine_id: String,
    token: String,
    #[serde(default)]
    label: String,
}

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

    while !cancel.load(Ordering::Relaxed) {
        if let Err(error) = hold_requests(&client, &cred, &cancel).await {
            if error.is_unauthorized() {
                match ensure_daemon(&client, &data_dir).await {
                    Ok(fresh) => {
                        cred = fresh;
                        if let Err(again) = hold_requests(&client, &cred, &cancel).await {
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
    let mode = client
        .local_exec_mode(&enrol.machine_id)
        .await
        .unwrap_or_default();
    if mode.is_empty() {
        let _ = client.set_local_exec_mode(&enrol.machine_id, "ask").await;
    }
    let cred = StoredDaemon {
        machine_id: enrol.machine_id,
        token: enrol.token,
        label,
    };
    save_credential(&path, &cred);
    Ok(cred)
}

async fn hold_requests(
    client: &OpenGrokClient,
    cred: &StoredDaemon,
    cancel: &Arc<AtomicBool>,
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

    while let Some(frame) = rx.recv().await {
        if cancel.load(Ordering::Relaxed) {
            reader.abort();
            return Ok(());
        }
        if frame.get("kind").and_then(Value::as_str) != Some("exec") {
            continue;
        }
        let client = client.clone();
        let cred = cred.clone();
        tokio::spawn(async move {
            handle_exec(&client, &cred, &frame).await;
        });
    }

    match reader.await {
        Ok(result) => result,
        Err(_) => Ok(()),
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
    if !cwd.trim().is_empty() {
        child.current_dir(cwd);
    }
    let spawned = match child.spawn() {
        Ok(child) => child,
        Err(error) => return json!({ "spawnError": { "error": error.to_string() } }),
    };
    match tokio::time::timeout(timeout, spawned.wait_with_output()).await {
        Ok(Ok(output)) => {
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
