use std::fmt;

/// HTTP failure against OpenGrok. Status is preserved so login 401 is distinguishable.
#[derive(Debug)]
pub struct OpenGrokError {
    pub status: Option<u16>,
    pub message: String,
}

impl OpenGrokError {
    pub fn message(message: impl Into<String>) -> Self {
        Self {
            status: None,
            message: message.into(),
        }
    }

    pub fn status(status: u16, message: impl Into<String>) -> Self {
        Self {
            status: Some(status),
            message: message.into(),
        }
    }

    pub fn is_unauthorized(&self) -> bool {
        self.status == Some(401)
    }
}

impl fmt::Display for OpenGrokError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.status {
            Some(code) => write!(f, "{code}: {}", self.message),
            None => write!(f, "{}", self.message),
        }
    }
}

impl std::error::Error for OpenGrokError {}
