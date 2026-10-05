//! [`Advice`] and the typed [`AutoApply`] flag (PUA ADR 0008).

use pua_core::{Answer, Decision, OptionIndex};

use crate::option;

/// Whether a consumer may auto-apply this advice without showing a chip.
///
/// Interrupt is **never** [`AutoApply::Allowed`]: the variant is produced only for queue /
/// steer / abstain. A consumer matching on this enum cannot forget the interrupt rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum AutoApply {
    /// The consumer may apply the answer without asking (queue, or steer when the user opted in).
    Allowed,
    /// Must show a chip / ask. Always the value for interrupt.
    Never,
}

/// A [`Decision`] plus the typed auto-apply flag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Advice {
    decision: Decision,
    auto_apply: AutoApply,
}

impl Advice {
    /// Wraps a decision. Interrupt → [`AutoApply::Never`]; everything else →
    /// [`AutoApply::Allowed`].
    pub fn from_decision(decision: Decision) -> Self {
        let auto_apply = match decision.answer() {
            Answer::Choice { option, .. } if *option == option::INTERRUPT => AutoApply::Never,
            _ => AutoApply::Allowed,
        };
        Self {
            decision,
            auto_apply,
        }
    }

    /// The decision (answer + trail + data version).
    pub fn decision(&self) -> &Decision {
        &self.decision
    }

    /// Consumes into the decision.
    pub fn into_decision(self) -> Decision {
        self.decision
    }

    /// The typed auto-apply flag.
    pub const fn auto_apply(&self) -> AutoApply {
        self.auto_apply
    }

    /// The chosen option label, when the answer is a choice.
    pub fn chosen_label(&self) -> Option<&str> {
        match self.decision.answer() {
            Answer::Choice { option, .. } => self
                .decision
                .answer()
                .chosen()
                .and_then(|_| label_of(*option)),
            _ => None,
        }
    }

    /// Whether this is an interrupt that must never auto-apply.
    pub fn is_interrupt(&self) -> bool {
        matches!(
            self.decision.answer(),
            Answer::Choice { option, .. } if *option == option::INTERRUPT
        )
    }
}

fn label_of(i: OptionIndex) -> Option<&'static str> {
    match i.get() {
        0 => Some("queue"),
        1 => Some("steer"),
        2 => Some("interrupt"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use pua_core::{
        Answer, Confidence, DataVersion, Decision, OptionIndex, Profile, Ranked, Trail,
    };

    use super::*;

    fn decision(answer: Answer) -> Decision {
        Decision::new(
            answer,
            Profile::Standard,
            DataVersion::builder("t").finish(),
            Trail::new(),
        )
    }

    #[test]
    fn interrupt_is_never_auto_applied() {
        let ranked = Ranked::try_from(vec![
            (option::INTERRUPT, Confidence::new(900).unwrap()),
            (option::QUEUE, Confidence::ZERO),
            (option::STEER, Confidence::ZERO),
        ])
        .unwrap();
        let a = Advice::from_decision(decision(Answer::Choice {
            option: option::INTERRUPT,
            confidence: Confidence::new(900).unwrap(),
            ranked,
        }));
        assert_eq!(a.auto_apply(), AutoApply::Never);
        assert!(a.is_interrupt());
        assert_eq!(a.chosen_label(), Some("interrupt"));
    }

    #[test]
    fn queue_and_abstain_may_auto_apply() {
        let ranked = Ranked::try_from(vec![
            (OptionIndex::SAFE_DEFAULT, Confidence::new(800).unwrap()),
            (option::STEER, Confidence::ZERO),
            (option::INTERRUPT, Confidence::ZERO),
        ])
        .unwrap();
        let a = Advice::from_decision(decision(Answer::Choice {
            option: option::QUEUE,
            confidence: Confidence::new(800).unwrap(),
            ranked: ranked.clone(),
        }));
        assert_eq!(a.auto_apply(), AutoApply::Allowed);
        let a = Advice::from_decision(decision(Answer::Abstain {
            why: pua_core::AbstainReason::NoCandidates,
            ranked,
        }));
        assert_eq!(a.auto_apply(), AutoApply::Allowed);
        assert!(!a.is_interrupt());
    }
}
