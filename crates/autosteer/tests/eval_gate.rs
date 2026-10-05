#![allow(clippy::expect_used, clippy::unwrap_used)]
//! CI gate: interrupt false-positive = 0 on the committed labelled set; golden journal replays.

use std::fs;
use std::path::PathBuf;

use nativechat_autosteer::{AutoApply, Autosteer, Input};
use pua_core::{Answer, Profile};
use pua_explain::{ReplayCheck, ReplayRecord};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Row {
    message: String,
    label: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct JournalInput {
    message: String,
    profile: String,
}

#[test]
fn interrupt_false_positive_is_zero_at_standard() {
    let pack = Autosteer::load().unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data/eval.jsonl");
    let data = fs::read_to_string(path).unwrap();
    let mut fp = 0u32;
    let mut n = 0u32;
    for line in data.lines().filter(|l| !l.is_empty()) {
        let row: Row = serde_json::from_str(line).unwrap();
        n += 1;
        let advice = pack.advise(&Input::message(&row.message), Profile::Standard);
        if let Answer::Choice { option, .. } = advice.decision().answer()
            && option.get() == 2
        {
            assert_eq!(advice.auto_apply(), AutoApply::Never);
            if row.label == "abstain_or_other" {
                fp += 1;
            }
        }
    }
    assert!(n >= 200, "{n}");
    assert_eq!(fp, 0, "interrupt false-positives");
}

#[test]
fn golden_journal_replays_byte_identically() {
    let pack = Autosteer::load().unwrap();
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden/journal.jsonl");
    let body = fs::read_to_string(path).unwrap();
    for (n, line) in body.lines().filter(|l| !l.is_empty()).enumerate() {
        let record: ReplayRecord<JournalInput> = ReplayRecord::from_json_line(line).unwrap();
        let check = record
            .check(|input| {
                let profile = match input.profile.as_str() {
                    "fast" => Profile::Fast,
                    "deep" => Profile::Deep,
                    _ => Profile::Standard,
                };
                pack.ask(&Input::message(&input.message), profile)
            })
            .unwrap();
        assert!(
            matches!(check, ReplayCheck::Identical),
            "line {n} drifted: {check:?}"
        );
    }
}
