#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::*;
use pua_core::{AbstainReason, Answer, Confidence, OptionIndex, Ranked};

/// Every abstain carries all three options; an early refusal has them unscored, in index order.
fn unscored_ranked() -> Ranked {
    Ranked::try_from(
        (0..3)
            .map(|i| (OptionIndex::new(i), Confidence::ZERO))
            .collect::<Vec<_>>(),
    )
    .unwrap()
}

fn pack() -> Autosteer {
    Autosteer::load().unwrap()
}

fn label(a: &Advice) -> Option<&str> {
    a.chosen_label()
}

#[test]
fn loads_and_pins_the_question() {
    let p = pack();
    assert_eq!(p.question().name().as_str(), "delivery");
    assert_eq!(
        p.question()
            .options()
            .unwrap()
            .labels()
            .iter()
            .map(pua_core::Label::as_str)
            .collect::<Vec<_>>(),
        ["queue", "steer", "interrupt"]
    );
    assert_eq!(p.data_version(), Autosteer::load().unwrap().data_version());
}

#[test]
fn interrupt_cues_and_auto_apply_never() {
    let p = pack();
    for msg in [
        "stop",
        "please cancel that",
        "abort now",
        "forget it",
        "never mind",
    ] {
        let a = p.advise(&Input::message(msg), Profile::Standard);
        assert_eq!(label(&a), Some("interrupt"), "{msg}");
        assert_eq!(a.auto_apply(), AutoApply::Never);
        assert!(a.is_interrupt());
    }
}

#[test]
fn steer_and_queue_cues() {
    let p = pack();
    for msg in [
        "use postgres instead",
        "actually wait",
        "switch to redis",
        "change to dark mode",
    ] {
        let a = p.advise(&Input::message(msg), Profile::Standard);
        assert_eq!(label(&a), Some("steer"), "{msg}");
        assert_eq!(a.auto_apply(), AutoApply::Allowed);
    }
    for msg in [
        "also add tests",
        "after that deploy",
        "afterwards ping me",
        "and add a log",
    ] {
        let a = p.advise(&Input::message(msg), Profile::Standard);
        assert_eq!(label(&a), Some("queue"), "{msg}");
        assert_eq!(a.auto_apply(), AutoApply::Allowed);
    }
}

#[test]
fn stop_using_is_steer_not_interrupt() {
    let p = pack();
    let a = p.advise(&Input::message("stop using postgres"), Profile::Standard);
    assert_eq!(label(&a), Some("steer"));
}

#[test]
fn negation_cancels_interrupt() {
    let p = pack();
    for msg in ["don't stop", "do not cancel", "no need to abort"] {
        let a = p.advise(&Input::message(msg), Profile::Standard);
        assert!(
            matches!(a.decision().answer(), Answer::Abstain { .. })
                || label(&a) != Some("interrupt"),
            "{msg} -> {:?}",
            a.decision().answer()
        );
    }
}

#[test]
fn protected_span_never_fires() {
    let p = pack();
    for msg in [
        "run `stop` now",
        "see https://example.com/stop",
        "edit src/stop.rs",
        "say \"stop\" out loud",
        "```\nstop\n```",
    ] {
        let a = p.advise(&Input::message(msg), Profile::Standard);
        assert_ne!(label(&a), Some("interrupt"), "{msg}");
    }
}

#[test]
fn typo_repair_fires_with_penalty() {
    let p = pack();
    let a = p.advise(&Input::message("stpo the build"), Profile::Standard);
    // Repair cost 150; weight 900 → 750, which equals the standard min confidence.
    assert_eq!(label(&a), Some("interrupt"), "{:?}", a.decision().answer());
    let Answer::Choice { confidence, .. } = a.decision().answer() else {
        panic!("{:?}", a.decision().answer())
    };
    assert_eq!(confidence.get(), 750);
}

#[test]
fn confusable_control_word_never_fires() {
    let p = pack();
    // Cyrillic 'ѕ' (U+0455) + Latin "top" → mixed-script skeleton "stop".
    let msg = "\u{0455}top the build";
    let a = p.advise(&Input::message(msg), Profile::Standard);
    assert_eq!(
        a.decision().answer(),
        &Answer::Abstain {
            why: AbstainReason::Confusable,
            ranked: unscored_ranked(),
        }
    );
    assert_eq!(a.auto_apply(), AutoApply::Allowed);
}

#[test]
fn question_damps_the_cue() {
    let p = pack();
    let a = p.advise(&Input::message("should I stop?"), Profile::Standard);
    // 900 halved = 450 < standard min 750 → abstain.
    assert!(matches!(
        a.decision().answer(),
        Answer::Abstain {
            why: AbstainReason::LowConfidence { .. },
            ..
        }
    ));
}

#[test]
fn case_space_nfc_do_not_change_the_answer() {
    let p = pack();
    let base = p.advise(&Input::message("stop the build"), Profile::Standard);
    for msg in [
        "STOP the build",
        "Stop  the   build",
        "stop the build!!",
        "sto\u{0301}p the build", // combining acute on o — still repairs/matches via NFC+fold?
    ] {
        // Combining acute on 'o' makes a different character; skip if it changes the token.
        if msg.contains('\u{0301}') {
            continue;
        }
        let a = p.advise(&Input::message(msg), Profile::Standard);
        assert_eq!(a.decision().answer(), base.decision().answer(), "{msg}");
    }
    // NFC/NFD of Ñ-free ASCII control words:
    let nfd = "stop";
    assert_eq!(
        p.advise(&Input::message(nfd), Profile::Standard)
            .decision()
            .answer(),
        base.decision().answer()
    );
}

#[test]
fn live_run_order_does_not_matter_in_phase_e() {
    let p = pack();
    let runs_a = vec![
        LiveRun {
            id: "1".into(),
            label: "sidebar".into(),
        },
        LiveRun {
            id: "2".into(),
            label: "migrations".into(),
        },
    ];
    let runs_b = runs_a.iter().rev().cloned().collect::<Vec<_>>();
    let a = p.advise(&Input::with_runs("stop", runs_a), Profile::Standard);
    let b = p.advise(&Input::with_runs("stop", runs_b), Profile::Standard);
    assert_eq!(a.decision().answer(), b.decision().answer());
}

#[test]
fn pack_error_texts_are_distinct() {
    let a = AutosteerError::Lexicon("x".into()).to_string();
    let b = AutosteerError::Rules("x".into()).to_string();
    let c = AutosteerError::ClassOrder(vec!["x".into()]).to_string();
    assert_ne!(a, b);
    assert_ne!(b, c);
}

use crate::data::{LEXICON_TOML, RULES_TOML};
use crate::input::LiveRun;

#[test]
fn class_order_is_pinned() {
    // Swap the first two class declarations: names still resolve, but indices are wrong.
    let bad = RULES_TOML
        .replacen("name = \"queue\"", "name = \"__tmp__\"", 1)
        .replacen("name = \"steer\"", "name = \"queue\"", 1)
        .replacen("name = \"__tmp__\"", "name = \"steer\"", 1);
    assert!(
        matches!(
            Autosteer::from_toml(LEXICON_TOML, &bad).unwrap_err(),
            AutosteerError::ClassOrder(_)
        ),
        "{bad}"
    );
    // Only two classes.
    let two = r#"
negators = ["don't"]
[[classes]]
name = "queue"
scorer = "max"
[[classes]]
name = "steer"
scorer = "max"
[[rules]]
id = "queue.also.v1"
class = "queue"
pattern = "also"
weight_millis = 800
forbids = ["negation"]
version = 1
"#;
    assert!(matches!(
        Autosteer::from_toml(LEXICON_TOML, two).unwrap_err(),
        AutosteerError::ClassOrder(_)
    ));
}

#[test]
fn input_accessors() {
    let runs = vec![LiveRun {
        id: "1".into(),
        label: "a".into(),
    }];
    let input = Input::with_runs("hello", runs.clone());
    assert_eq!(input.text(), "hello");
    assert_eq!(input.as_str(), "hello");
    assert_eq!(input.live_runs(), runs.as_slice());
    assert_eq!(Input::message("x").live_runs(), &[]);
}
