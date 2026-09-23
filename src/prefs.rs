//! Local preferences that are the app's own, not the server's: a JSON object
//! in the data dir, one key per preference. `theme.json` came first and has
//! its own file; everything after it goes here, and a key this build does not
//! know is kept as it was so two builds can share the file.

use crate::send_policy::OnSend;
use std::path::{Path, PathBuf};

const ON_SEND: &str = "on_send";
const SHOW_TURN_TIMING: &str = "show_turn_timing";

pub fn prefs_path(data_dir: &Path) -> PathBuf {
    data_dir.join("prefs.json")
}

pub fn load_on_send(data_dir: &Path) -> OnSend {
    load_on_send_from(&prefs_path(data_dir))
}

pub fn load_on_send_from(path: &Path) -> OnSend {
    read_object(path)
        .get(ON_SEND)
        .and_then(serde_json::Value::as_str)
        .map(OnSend::parse)
        .unwrap_or_default()
}

pub fn save_on_send(data_dir: &Path, on_send: OnSend) {
    save_on_send_to(&prefs_path(data_dir), on_send);
}

pub fn save_on_send_to(path: &Path, on_send: OnSend) {
    let mut prefs = read_object(path);
    prefs.insert(ON_SEND.to_string(), on_send.as_str().into());
    write_object(path, prefs);
}

pub fn load_show_turn_timing(data_dir: &Path) -> bool {
    load_show_turn_timing_from(&prefs_path(data_dir))
}

pub fn load_show_turn_timing_from(path: &Path) -> bool {
    read_object(path)
        .get(SHOW_TURN_TIMING)
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

pub fn save_show_turn_timing(data_dir: &Path, on: bool) {
    save_show_turn_timing_to(&prefs_path(data_dir), on);
}

pub fn save_show_turn_timing_to(path: &Path, on: bool) {
    let mut prefs = read_object(path);
    prefs.insert(SHOW_TURN_TIMING.to_string(), on.into());
    write_object(path, prefs);
}

/// The file as an object, or an empty one: missing, unreadable and
/// not-an-object all read the same, and a save over any of them starts clean.
fn read_object(path: &Path) -> serde_json::Map<String, serde_json::Value> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|value| match value {
            serde_json::Value::Object(map) => Some(map),
            _ => None,
        })
        .unwrap_or_default()
}

fn write_object(path: &Path, prefs: serde_json::Map<String, serde_json::Value>) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(&serde_json::Value::Object(prefs)) {
        let _ = std::fs::write(path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn scratch(name: &str) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("nativechat-prefs-test-{name}-{stamp}"));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn on_send_round_trips_and_defaults_to_queue() {
        let dir = scratch("round-trip");
        let path = prefs_path(&dir);
        assert_eq!(load_on_send_from(&path), OnSend::Queue, "no file yet");
        save_on_send_to(&path, OnSend::Steer);
        assert_eq!(load_on_send_from(&path), OnSend::Steer);
        save_on_send_to(&path, OnSend::Queue);
        assert_eq!(load_on_send_from(&path), OnSend::Queue);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn garbage_reads_as_queue_and_is_replaced_on_save() {
        let dir = scratch("garbage");
        let path = prefs_path(&dir);
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(load_on_send_from(&path), OnSend::Queue);
        std::fs::write(&path, "[1,2]").unwrap();
        assert_eq!(load_on_send_from(&path), OnSend::Queue);
        save_on_send_to(&path, OnSend::Steer);
        assert_eq!(load_on_send_from(&path), OnSend::Steer);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn other_keys_survive_a_save() {
        let dir = scratch("other-keys");
        let path = prefs_path(&dir);
        std::fs::write(&path, r#"{"later_build":{"x":1},"on_send":"auto"}"#).unwrap();
        assert_eq!(
            load_on_send_from(&path),
            OnSend::Queue,
            "unknown word is queue"
        );
        save_on_send_to(&path, OnSend::Steer);
        let raw = std::fs::read_to_string(&path).unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(value["on_send"], "steer");
        assert_eq!(value["later_build"]["x"], 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn turn_timing_defaults_off_and_round_trips() {
        let dir = scratch("turn-timing");
        let path = prefs_path(&dir);
        assert!(!load_show_turn_timing_from(&path), "no file yet is off");
        save_show_turn_timing_to(&path, true);
        assert!(load_show_turn_timing_from(&path));
        save_on_send_to(&path, OnSend::Steer);
        assert!(
            load_show_turn_timing_from(&path),
            "saving another pref keeps the switch"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
