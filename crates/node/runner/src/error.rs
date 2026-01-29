use std::fmt;

/// Error type for node runner operations.
#[derive(Debug)]
pub struct RunnerError(pub anyhow::Error);

impl fmt::Display for RunnerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RunnerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.0.source()
    }
}

impl From<anyhow::Error> for RunnerError {
    fn from(e: anyhow::Error) -> Self {
        Self(e)
    }
}
