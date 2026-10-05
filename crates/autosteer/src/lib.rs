//! `nativechat-autosteer`: delivery advice for a composer message.
//!
//! `NativeChat` owns the **what** (the `delivery` question, its three options, the lexicon and rules
//! data, the labelled eval set, the golden journal and the [`AutoApply`] rule); the PUA engine
//! (`pua_rules::RuleClassifier`) owns the **how** (normalize → lexicon → rules → decide, integer
//! scores, abstain on low confidence or margin). Moved here from `hexuria/pua` `packs/pua-steer`
//! (PUA architecture audit, Phase 2); the domain tag stays `pua-steer/1` so journals written
//! before the move still replay.
//!
//! Question: `Choice{ name: "delivery", options: [queue, steer, interrupt] }` with option 0 the
//! safe default. Pipeline: canonicalize → lexicon → rules → decide. Interrupt answers always
//! carry [`AutoApply::Never`] (PUA ADR 0008): a consumer that respects the type cannot auto-apply
//! an interrupt. Confusable control words never fire ([`pua_core::AbstainReason::Confusable`]).
//!
//! [`Input::with_runs`] carries live runs for a later target-selection step; nothing scores them
//! yet.
//!
//! ```
//! use pua_core::{Answer, OptionIndex, Profile};
//! use nativechat_autosteer::{Advice, Autosteer, Input};
//!
//! let pack = Autosteer::load().expect("embedded data is valid");
//! let advice = pack.advise(&Input::message("stop the build"), Profile::Standard);
//! assert_eq!(advice.chosen_label(), Some("interrupt"));
//! assert_eq!(advice.auto_apply(), nativechat_autosteer::AutoApply::Never);
//! assert!(matches!(advice.decision().answer(), Answer::Choice { option, .. }
//!     if *option == OptionIndex::new(2)));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
#![forbid(unsafe_code)]

mod advice;
mod data;
mod input;

pub use advice::{Advice, AutoApply};
pub use input::{Input, LiveRun};

use core::fmt;

use pua_core::{DataVersion, Decision, Profile, Question};
use pua_lexicon::LexiconSpec;
use pua_rules::{ClassifierError, ClassifierSpec, OnConfusable, RuleClassifier, RuleSetSpec};
use pua_text::NormalizeConfig;

use crate::data::{LEXICON_TOML, RULES_TOML};

/// Pack algorithm tag folded into [`DataVersion`].
pub const ALGORITHM_TAG: &str = "pua-steer/1";

/// Option indices of the delivery question.
pub mod option {
    use pua_core::OptionIndex;
    /// Queue (safe default).
    pub const QUEUE: OptionIndex = OptionIndex::SAFE_DEFAULT;
    /// Steer.
    pub const STEER: OptionIndex = OptionIndex::new(1);
    /// Interrupt.
    pub const INTERRUPT: OptionIndex = OptionIndex::new(2);
}

/// Why the pack (or its embedded data) could not be built.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum AutosteerError {
    /// Lexicon TOML failed to parse or validate.
    Lexicon(String),
    /// Rules TOML failed to parse or validate.
    Rules(String),
    /// The rule classes are not exactly `[queue, steer, interrupt]` in that order.
    ClassOrder(Vec<String>),
}

impl fmt::Display for AutosteerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lexicon(e) => write!(f, "lexicon: {e}"),
            Self::Rules(e) => write!(f, "rules: {e}"),
            Self::ClassOrder(names) => write!(
                f,
                "classes must be [queue, steer, interrupt], got {names:?}"
            ),
        }
    }
}

impl std::error::Error for AutosteerError {}

/// The autosteer pack: embedded lexicon + rules answering the delivery question.
#[derive(Debug, Clone)]
pub struct Autosteer {
    classifier: RuleClassifier,
}

impl Autosteer {
    /// Loads the embedded pack data. Always succeeds for the committed TOML; returns an error
    /// only if that data is broken (caught by CI).
    ///
    /// # Errors
    /// [`AutosteerError`] when the embedded TOML fails to parse or validate.
    pub fn load() -> Result<Self, AutosteerError> {
        Self::from_toml(LEXICON_TOML, RULES_TOML)
    }

    /// Builds a pack from lexicon and rules TOML (for tests and the offline CLI).
    ///
    /// # Errors
    /// [`AutosteerError`].
    pub fn from_toml(lexicon_toml: &str, rules_toml: &str) -> Result<Self, AutosteerError> {
        let lexicon: LexiconSpec =
            toml::from_str(lexicon_toml).map_err(|e| AutosteerError::Lexicon(e.to_string()))?;
        let rules: RuleSetSpec =
            toml::from_str(rules_toml).map_err(|e| AutosteerError::Rules(e.to_string()))?;
        let spec = ClassifierSpec {
            domain: ALGORITHM_TAG.into(),
            question: "delivery".into(),
            normalize: NormalizeConfig {
                fold: pua_text::Fold::UnicodeLower,
                punct_runs: pua_text::PunctRuns::Collapse,
            },
            lexicon,
            rules,
            on_confusable: OnConfusable::Abstain,
        };
        let classifier = RuleClassifier::new(&spec).map_err(|e| match e {
            ClassifierError::Lexicon(e) => AutosteerError::Lexicon(e.to_string()),
            ClassifierError::Rules(e) => AutosteerError::Rules(e.to_string()),
            other => AutosteerError::ClassOrder(vec![other.to_string()]),
        })?;
        // Classes must be exactly [queue, steer, interrupt] in that order: option indices
        // are hard-wired to those names.
        let labels: Vec<&str> = classifier
            .question()
            .options()
            .map(|o| o.labels().iter().map(pua_core::Label::as_str).collect())
            .unwrap_or_default();
        if labels != ["queue", "steer", "interrupt"] {
            return Err(AutosteerError::ClassOrder(
                labels.into_iter().map(str::to_owned).collect(),
            ));
        }
        Ok(Self { classifier })
    }

    /// Consumer-facing ask: a [`Decision`] plus the typed [`AutoApply`] flag.
    pub fn advise(&self, input: &Input<'_>, profile: Profile) -> Advice {
        Advice::from_decision(self.ask(input, profile))
    }
}

impl Autosteer {
    /// The question this pack answers.
    pub fn question(&self) -> &Question {
        self.classifier.question()
    }

    /// Digest of everything that can change an answer.
    pub fn data_version(&self) -> DataVersion {
        self.classifier.data_version()
    }

    /// Answers the question for `input`. Pure: same input + profile gives the same bytes.
    pub fn ask(&self, input: &Input<'_>, profile: Profile) -> Decision {
        self.classifier.decide(input.text(), profile)
    }
}

/// Re-exports used by the eval / replay examples.
pub mod prelude {
    pub use crate::{ALGORITHM_TAG, Advice, AutoApply, Autosteer, AutosteerError, Input, option};
    pub use pua_core::{Answer, Decision, OptionIndex, Profile};
}

#[cfg(test)]
mod tests;
