//! OpenGrok HTTP client. No GPUI. Cookies are the session.

mod client;
mod error;
mod types;

pub use client::OpenGrokClient;
pub use error::OpenGrokError;
pub use types::{Account, ProfileUpdate};
