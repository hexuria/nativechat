//! OpenGrok #139 `TOOL_CALL_RESULT.image.visibility`.
//!
//! NativeChat persists `transcript` | `failure` | `end`. `agent` is the live
//! Computer pane (and `GET /coworkers/{id}/screen`). Missing on old frames
//! is treated as untagged: pin failure immediately and the last shot at
//! turn-end, the heuristic used before the server tagged every frame.

/// OpenGrok #139 `TOOL_CALL_RESULT.image.visibility`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageVisibility {
    Agent,
    Transcript,
    Failure,
    End,
}

impl ImageVisibility {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "agent" => Some(Self::Agent),
            "transcript" => Some(Self::Transcript),
            "failure" => Some(Self::Failure),
            "end" => Some(Self::End),
            _ => None,
        }
    }

    /// First-class chat event. Not the Computer pane.
    pub fn belongs_in_transcript(self) -> bool {
        matches!(self, Self::Transcript | Self::Failure | Self::End)
    }
}

/// Pin into chat now. Tagged `transcript`/`failure`/`end` yes; `agent` no.
/// Untagged: only a failed tool (`ok == false`), matching the pre-tag heuristic.
pub fn pin_shot_now(visibility: Option<ImageVisibility>, ok: Option<bool>) -> bool {
    match visibility {
        Some(visibility) => visibility.belongs_in_transcript(),
        None => ok == Some(false),
    }
}

/// Turn-end pin. Tagged `agent` stays off the feed; everything else may
/// already be pinned (`pin_shot_now`) and is idempotent by `call_id`.
pub fn pin_shot_at_turn_end(visibility: Option<ImageVisibility>) -> bool {
    !matches!(visibility, Some(ImageVisibility::Agent))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_locked_visibility_names() {
        assert_eq!(
            ImageVisibility::parse("agent"),
            Some(ImageVisibility::Agent)
        );
        assert_eq!(
            ImageVisibility::parse("transcript"),
            Some(ImageVisibility::Transcript)
        );
        assert_eq!(
            ImageVisibility::parse("FAILURE"),
            Some(ImageVisibility::Failure)
        );
        assert_eq!(ImageVisibility::parse("end"), Some(ImageVisibility::End));
        assert_eq!(ImageVisibility::parse("private"), None);
        assert_eq!(ImageVisibility::parse(""), None);
    }

    #[test]
    fn agent_never_belongs_in_transcript() {
        assert!(!ImageVisibility::Agent.belongs_in_transcript());
        assert!(!pin_shot_now(Some(ImageVisibility::Agent), Some(true)));
        assert!(!pin_shot_now(Some(ImageVisibility::Agent), Some(false)));
        assert!(!pin_shot_at_turn_end(Some(ImageVisibility::Agent)));
    }

    #[test]
    fn transcript_failure_end_pin_now_and_at_end() {
        for visibility in [
            ImageVisibility::Transcript,
            ImageVisibility::Failure,
            ImageVisibility::End,
        ] {
            assert!(visibility.belongs_in_transcript());
            assert!(pin_shot_now(Some(visibility), Some(true)));
            assert!(pin_shot_at_turn_end(Some(visibility)));
        }
    }

    #[test]
    fn untagged_keeps_failure_and_turn_end_heuristic() {
        assert!(pin_shot_now(None, Some(false)));
        assert!(!pin_shot_now(None, Some(true)));
        assert!(!pin_shot_now(None, None));
        assert!(pin_shot_at_turn_end(None));
    }
}
