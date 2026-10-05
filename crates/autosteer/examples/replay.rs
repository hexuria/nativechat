#![allow(
    clippy::expect_used,
    clippy::print_stderr,
    clippy::print_stdout,
    clippy::unwrap_used
)]
//! Offline replay: re-runs the pack on a golden JSONL journal and diffs any drift.
//!
//! ```text
//! cargo run -p nativechat-autosteer --example replay -- crates/autosteer/golden/journal.jsonl
//! ```

use std::env;
use std::fs;
use std::process::ExitCode;

use nativechat_autosteer::{Autosteer, Input};
use pua_core::Profile;
use pua_explain::{ReplayCheck, ReplayRecord, render_trail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
struct JournalInput {
    message: String,
    profile: String,
}

fn profile_of(s: &str) -> Profile {
    match s {
        "fast" => Profile::Fast,
        "deep" => Profile::Deep,
        _ => Profile::Standard,
    }
}

fn main() -> ExitCode {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| format!("{}/golden/journal.jsonl", env!("CARGO_MANIFEST_DIR")));
    let pack = Autosteer::load().expect("pack");
    let body = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    let mut ok = 0u32;
    let mut bad = 0u32;
    for (n, line) in body.lines().filter(|l| !l.is_empty()).enumerate() {
        let record: ReplayRecord<JournalInput> =
            ReplayRecord::from_json_line(line).unwrap_or_else(|e| panic!("line {n}: {e}"));
        let check = record
            .check(|input| pack.ask(&Input::message(&input.message), profile_of(&input.profile)))
            .expect("check");
        match check {
            ReplayCheck::Identical => ok += 1,
            ReplayCheck::Diverged(diff) => {
                bad += 1;
                eprintln!("drift at line {n}: {diff:?}");
                eprintln!("trail:\n{}", render_trail(record.decision().trail()));
            }
        }
    }
    println!("{path}: {ok} match, {bad} drift");
    if bad == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
