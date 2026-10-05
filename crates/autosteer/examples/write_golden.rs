#![allow(clippy::expect_used, clippy::print_stdout, clippy::unwrap_used)]
//! Writes `crates/autosteer/golden/journal.jsonl` for the committed seed messages.

use std::fs;
use std::path::PathBuf;

use nativechat_autosteer::{Autosteer, Input};
use pua_core::Profile;
use pua_explain::ReplayRecord;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct JournalInput {
    message: String,
    profile: String,
}

fn main() {
    let pack = Autosteer::load().unwrap();
    let seeds = [
        ("stop the build", "standard"),
        ("stpo now", "standard"),
        ("don't stop", "standard"),
        ("stop using postgres", "standard"),
        ("also add tests", "standard"),
        ("use rust instead", "standard"),
        ("run `stop` now", "standard"),
        ("\u{0455}top the build", "standard"),
        ("should I stop?", "standard"),
        ("hello there", "fast"),
        ("cancel the job", "deep"),
        ("STOP!!", "standard"),
    ];
    let mut out = String::new();
    for (msg, profile) in seeds {
        let input = JournalInput {
            message: msg.into(),
            profile: profile.into(),
        };
        let decision = pack.ask(
            &Input::message(msg),
            match profile {
                "fast" => Profile::Fast,
                "deep" => Profile::Deep,
                _ => Profile::Standard,
            },
        );
        let rec = ReplayRecord::new(input, decision).unwrap();
        out.push_str(&rec.to_json_line().unwrap());
        out.push('\n');
    }
    let dest = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden/journal.jsonl");
    fs::create_dir_all(dest.parent().unwrap()).ok();
    fs::write(&dest, out).unwrap();
    println!("wrote {}", dest.display());
}
