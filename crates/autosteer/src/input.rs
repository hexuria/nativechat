//! Input: a chat message, plus (not yet scored) live runs.

use serde::{Deserialize, Serialize};

/// One live run the user might be addressing. Present only so the input shape is stable;
/// nothing scores live runs yet.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LiveRun {
    /// Stable id (journal key). Tie-breaks by this when scores tie.
    pub id: String,
    /// Short label (e.g. first user line).
    pub label: String,
}

/// Input to the autosteer pack.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Input<'a> {
    message: &'a str,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    live_runs: Vec<LiveRun>,
}

impl<'a> Input<'a> {
    /// A message with no live runs (Phase E).
    pub const fn message(message: &'a str) -> Self {
        Self {
            message,
            live_runs: Vec::new(),
        }
    }

    /// A message plus live runs (not scored yet; the answer ignores them).
    pub fn with_runs(message: &'a str, live_runs: Vec<LiveRun>) -> Self {
        Self { message, live_runs }
    }

    /// The user message.
    pub const fn text(&self) -> &'a str {
        self.message
    }

    /// The user message (same as [`Self::text`]).
    pub const fn as_str(&self) -> &'a str {
        self.message
    }

    /// Live runs, in input order. Order must not affect the answer (PUA spec §6.1).
    pub fn live_runs(&self) -> &[LiveRun] {
        &self.live_runs
    }
}
