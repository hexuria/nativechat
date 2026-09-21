//! Authenticator codes: an `otpauth://` seed and the arithmetic of RFC 6238.
//!
//! The code is minted on the Mac at the moment it is sent, never earlier, and when the
//! current 30-second step is about to end the send waits for the next one, so the digits
//! the box types are still valid after the trip.

use std::time::{SystemTime, UNIX_EPOCH};

use totp_rs::TOTP;

/// A seed with fewer seconds than this left in its step is minted on the next step instead.
pub const MIN_TTL_SECONDS: u64 = 8;

/// Parse an `otpauth://totp/...` URI. The unchecked constructor, because many real seeds are
/// shorter than the RFC's 16 bytes (GitHub's are 10) and the checked one refuses them.
pub fn parse(uri: &str) -> Result<TOTP, String> {
    TOTP::from_url_unchecked(uri).map_err(|e| format!("not an authenticator seed: {e}"))
}

/// The current code and the seconds it has left.
pub fn current(totp: &TOTP) -> Result<(String, u64), String> {
    Ok((totp.generate_current().to_string(), totp.ttl()))
}

/// The seconds to wait before minting, so the code lands with time to spare: none when the
/// step is fresh, the rest of the step when it is nearly over.
pub fn wait_before_minting(totp: &TOTP) -> u64 {
    let ttl = totp.ttl();
    if ttl < MIN_TTL_SECONDS { ttl } else { 0 }
}

/// The code for a given moment, for tests and the detail pane's countdown.
pub fn at(totp: &TOTP, unix_seconds: u64) -> String {
    totp.generate(unix_seconds).to_string()
}

pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rfc_vectors_and_a_short_seed_both_mint() {
        // RFC 6238, SHA-1, the 20-byte seed "12345678901234567890" = GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ.
        let rfc = parse("otpauth://totp/x?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&digits=8")
            .expect("seed");
        assert_eq!(at(&rfc, 59), "94287082");
        assert_eq!(at(&rfc, 1111111109), "07081804");
        assert_eq!(at(&rfc, 20000000000), "65353130");
        // A 10-byte seed, the size GitHub hands out.
        let short = parse("otpauth://totp/GitHub:ada?secret=JBSWY3DPEHPK3PXP").expect("short seed");
        assert_eq!(at(&short, 0).len(), 6);
        assert!(parse("not a uri").is_err());
    }

    #[test]
    fn a_seed_near_the_step_edge_waits() {
        let totp = parse("otpauth://totp/x?secret=JBSWY3DPEHPK3PXP").expect("seed");
        let wait = wait_before_minting(&totp);
        let ttl = totp.ttl();
        if ttl < MIN_TTL_SECONDS {
            assert_eq!(wait, ttl);
        } else {
            assert_eq!(wait, 0);
        }
    }
}
