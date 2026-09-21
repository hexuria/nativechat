//! The Touch ID sheet, through LocalAuthentication.
//!
//! Every use of a saved site login goes through here. The password never leaves the Mac's
//! keychain without the person's finger (or their password, when the sensor is missing or
//! locked), and the sheet says which site and which username it is for.
//!
//! This is `LAContext evaluatePolicy:localizedReason:reply:`, not a `SecAccessControl` on the
//! keychain item. The item-level gate needs the data-protection keychain, which needs a
//! `keychain-access-groups` entitlement and a stable signing identity that the ad-hoc dev
//! build does not have. The sheet works on every build, carries our own sentence, and the
//! item stays where it is. The trade-off, stated plainly: the gate is in this process, not
//! in the keychain, so it protects against a distracted person and a curious bystander, not
//! against code running as the same user.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::time::Duration;

use block::ConcreteBlock;
use cocoa::base::{id, nil};
use cocoa::foundation::NSString;
use objc::{class, msg_send, sel, sel_impl};

/// `LAPolicyDeviceOwnerAuthentication`: Touch ID, Apple Watch, or the account password.
const DEVICE_OWNER_AUTHENTICATION: i64 = 2;

/// How long the sheet may stay up before we give up on it (the person walked away).
const SHEET_TIMEOUT: Duration = Duration::from_secs(120);

/// One sheet at a time: a second ask while one is up would stack two system dialogs.
static SHEET_UP: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TouchIdOutcome {
    /// The person proved it is them.
    Verified,
    /// They cancelled the sheet, or it timed out.
    Cancelled,
    /// The Mac cannot ask (no policy available). The reason is the OS's own sentence.
    Unavailable(String),
    /// The sheet ran and the OS refused. The reason is the OS's own sentence.
    Failed(String),
}

/// Ask the person to confirm with Touch ID before a saved login for `origin` as `username`
/// is used. Blocks the calling thread until the sheet closes; call it from a background
/// thread, never from the UI thread.
pub fn confirm_use(origin: &str, username: &str) -> TouchIdOutcome {
    let reason = format!("use the saved login for {origin} as {username}");
    if SHEET_UP.swap(true, Ordering::SeqCst) {
        return TouchIdOutcome::Unavailable("another Touch ID sheet is already up".to_string());
    }
    let outcome = objc::rc::autoreleasepool(|| prompt(&reason));
    SHEET_UP.store(false, Ordering::SeqCst);
    outcome
}

fn prompt(reason: &str) -> TouchIdOutcome {
    // SAFETY: every message here is a documented LocalAuthentication selector on an
    // LAContext we own; the reply block is copied to the heap before it is handed over and
    // the channel it captures outlives the sheet through the `recv_timeout` below.
    unsafe {
        let context: id = msg_send![class!(LAContext), new];
        if context == nil {
            return TouchIdOutcome::Unavailable("LAContext could not be created".to_string());
        }
        let mut error: id = nil;
        let can: bool =
            msg_send![context, canEvaluatePolicy: DEVICE_OWNER_AUTHENTICATION error: &mut error];
        if !can {
            let why = describe(error);
            let _: () = msg_send![context, release];
            return TouchIdOutcome::Unavailable(why);
        }
        let (tx, rx) = mpsc::channel::<(bool, String)>();
        let reply = ConcreteBlock::new(move |success: bool, err: id| {
            let why = if success {
                String::new()
            } else {
                describe(err)
            };
            let _ = tx.send((success, why));
        })
        .copy();
        let ns_reason = NSString::alloc(nil).init_str(reason);
        let _: () = msg_send![context, evaluatePolicy: DEVICE_OWNER_AUTHENTICATION localizedReason: ns_reason reply: &*reply];
        // LAContext copies the reason; ours is released here.
        let _: () = msg_send![ns_reason, release];
        let outcome = match rx.recv_timeout(SHEET_TIMEOUT) {
            Ok((true, _)) => TouchIdOutcome::Verified,
            Ok((false, why)) if is_cancel(&why) => TouchIdOutcome::Cancelled,
            Ok((false, why)) => TouchIdOutcome::Failed(why),
            Err(_) => {
                let _: () = msg_send![context, invalidate];
                TouchIdOutcome::Cancelled
            }
        };
        let _: () = msg_send![context, release];
        outcome
    }
}

unsafe fn describe(error: id) -> String {
    if error == nil {
        return "no reason given".to_string();
    }
    let text: id = unsafe { msg_send![error, localizedDescription] };
    if text == nil {
        return "no reason given".to_string();
    }
    let bytes: *const std::os::raw::c_char = unsafe { msg_send![text, UTF8String] };
    if bytes.is_null() {
        return "no reason given".to_string();
    }
    unsafe { std::ffi::CStr::from_ptr(bytes) }
        .to_string_lossy()
        .into_owned()
}

/// LocalAuthentication words a cancel with "Canceled by user" / "Cancelled" / "Fallback"
/// depending on the OS release; any of them is the person choosing not to go on.
fn is_cancel(why: &str) -> bool {
    let w = why.to_ascii_lowercase();
    w.contains("cancel") || w.contains("user interaction is required")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cancelled_sheet_is_a_cancel_whatever_the_os_calls_it() {
        assert!(is_cancel("Canceled by user."));
        assert!(is_cancel("Cancelled"));
        assert!(!is_cancel("Biometry is not available on this device."));
    }

    #[test]
    fn the_sheet_names_the_site_and_the_user() {
        // The sentence is what the person reads on the sheet; keep it a sentence.
        let reason = format!("use the saved login for {} as {}", "facebook.com", "ada");
        assert_eq!(reason, "use the saved login for facebook.com as ada");
    }
}
