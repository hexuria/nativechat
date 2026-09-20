//! Session broker is A.1. A.0 must not type a site password into Box Chromium.
//!
//! OpenMausBot #255 / PLAN rev2: CDP and screenshots can read `input.value`.
//! `credential.result.status = filled` means cookies/profile landed on Box
//! after this broker, not that NativeChat filled the login form in the agent-
//! observable page.
//!
//! There is no fill-into-Box function in this crate on purpose.

/// A.0: no cookie/session import onto Box. Flip only when A.1 ships.
pub const SESSION_BROKER_AVAILABLE: bool = false;

#[cfg(test)]
mod tests {
    #[test]
    fn a0_has_no_session_broker() {
        assert!(!super::SESSION_BROKER_AVAILABLE);
    }
}
