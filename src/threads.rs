//! Thread ids as OpenGrok files them, and the conversation each one belongs to.
//!
//! A coworker's chat is a thread named after the coworker. Its MCP door opens a
//! second thread per coworker — `mcp-{coworker_id}` — for the tool calls it
//! audits, and files the permission cards those calls raise under that thread.
//! The app has no conversation by that name and should not grow one: the card
//! is about that coworker, and it belongs in the thread the person already has
//! open for it.
//!
//! Pure on purpose: no gpui, no state, so it compiles and tests on its own
//! while the crate's test target is out of reach.

/// What the MCP door puts in front of a coworker's id when it opens a thread
/// for the calls it audits.
pub const MCP_THREAD_PREFIX: &str = "mcp-";

/// The conversation a thread's cards belong in: an MCP thread's coworker, and
/// otherwise the thread itself.
///
/// A bare `mcp-` names no coworker, so it is left whole rather than turned into
/// an empty id that would go looking for an empty conversation.
pub fn conversation_for_thread(thread_id: &str) -> &str {
    match thread_id.strip_prefix(MCP_THREAD_PREFIX) {
        Some(coworker) if !coworker.is_empty() => coworker,
        _ => thread_id,
    }
}

/// The thread is the MCP door's, not the chat's: what it holds is an audit of a
/// call the coworker made somewhere else, and none of it is a turn of the
/// conversation.
pub fn is_mcp_thread(thread_id: &str) -> bool {
    conversation_for_thread(thread_id) != thread_id
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_mcp_thread_belongs_to_the_coworker_it_is_named_after() {
        assert_eq!(conversation_for_thread("mcp-cw_1"), "cw_1");
        assert!(is_mcp_thread("mcp-cw_1"));
    }

    #[test]
    fn an_ordinary_thread_is_its_own_conversation() {
        assert_eq!(conversation_for_thread("cw_1"), "cw_1");
        assert_eq!(conversation_for_thread(""), "");
        assert!(!is_mcp_thread("cw_1"));
        assert!(!is_mcp_thread(""));
    }

    /// The prefix counts at the front and nowhere else, and it is taken off
    /// once: the door does not nest its own threads.
    #[test]
    fn only_the_leading_prefix_is_taken_off() {
        assert_eq!(conversation_for_thread("cw-mcp-1"), "cw-mcp-1");
        assert_eq!(conversation_for_thread("mcp-mcp-cw_1"), "mcp-cw_1");
        assert!(!is_mcp_thread("cw-mcp-1"));
    }

    #[test]
    fn a_bare_prefix_names_no_coworker() {
        assert_eq!(conversation_for_thread("mcp-"), "mcp-");
        assert!(!is_mcp_thread("mcp-"));
    }
}
