//! Generator-sequence proptests for the autosteer invariances (PUA spec §6.1).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::float_arithmetic)]

use nativechat_autosteer::{Autosteer, Input, LiveRun};
use proptest::prelude::*;
use pua_core::{Answer, Profile};
use unicode_normalization::UnicodeNormalization as _;

fn pack() -> &'static Autosteer {
    use std::sync::OnceLock;
    static P: OnceLock<Autosteer> = OnceLock::new();
    P.get_or_init(|| Autosteer::load().unwrap())
}

fn answer_of(msg: &str) -> Answer {
    pack()
        .ask(&Input::message(msg), Profile::Standard)
        .answer()
        .clone()
}

/// Elementary moves from the §6.1 table (outside protected spans).
fn move_case(s: &str) -> String {
    s.chars()
        .enumerate()
        .map(|(i, c)| {
            if i == s.find(|c: char| c.is_alphabetic()).unwrap_or(usize::MAX) {
                if c.is_lowercase() {
                    c.to_uppercase().collect::<String>()
                } else {
                    c.to_lowercase().collect::<String>()
                }
            } else {
                c.to_string()
            }
        })
        .collect()
}

fn move_space(s: &str) -> String {
    if let Some(i) = s.find("  ") {
        let mut t = s.to_owned();
        t.insert(i, ' ');
        t
    } else if let Some(i) = s.find(' ') {
        let mut t = s.to_owned();
        t.insert(i, ' ');
        t
    } else {
        s.to_owned()
    }
}

fn move_nfd(s: &str) -> String {
    s.nfd().collect()
}

fn move_punct(s: &str) -> String {
    format!("{s}!")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn generator_sequence_preserves_answer(
        seed in prop::sample::select(vec![
            "stop the build",
            "cancel the job",
            "use rust instead",
            "also add tests",
            "stop using postgres",
            "after that deploy",
            "forget it",
            "switch to vim",
        ]),
        steps in proptest::collection::vec(0u8..4, 0..6),
    ) {
        let mut s = seed.to_owned();
        let expected = answer_of(&s);
        for step in steps {
            s = match step {
                0 => move_case(&s),
                1 => move_space(&s),
                2 => move_nfd(&s),
                _ => move_punct(&s),
            };
        }
        prop_assert_eq!(answer_of(&s), expected);
    }

    #[test]
    fn live_run_swap_is_invisible_in_phase_e(
        msg in prop::sample::select(vec!["stop", "also add x", "use y instead"]),
        a in "[a-z]{3,8}",
        b in "[a-z]{3,8}",
    ) {
        prop_assume!(a != b);
        let left = vec![
            LiveRun { id: "1".into(), label: a.clone() },
            LiveRun { id: "2".into(), label: b.clone() },
        ];
        let right = vec![
            LiveRun { id: "2".into(), label: b },
            LiveRun { id: "1".into(), label: a },
        ];
        let p = pack();
        let a = p.ask(&Input::with_runs(msg, left), Profile::Standard);
        let b = p.ask(&Input::with_runs(msg, right), Profile::Standard);
        prop_assert_eq!(a.answer(), b.answer());
    }

    #[test]
    fn determinism(msg in "\\PC{0,80}") {
        let p = pack();
        let a = p.ask(&Input::message(&msg), Profile::Standard);
        let b = p.ask(&Input::message(&msg), Profile::Standard);
        prop_assert_eq!(a, b);
    }
}
